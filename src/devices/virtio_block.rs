//! virtio-blk 块设备模型（VirtIO 1.2 §5.2）。
//!
//! Guest 内核 `root=/dev/vda` 的存储后端：把 rootfs 镜像文件暴露为块设备。
//!
//! 请求格式（split virtqueue，每个请求是一条描述符链）：
//!
//! ```text
//! ┌──────────────────┐ device-readable
//! │ virtio_blk_req   │  type(u32) reserved(u32) sector(u64)
//! ├──────────────────┤ IN 时 device-writable / OUT 时 device-readable
//! │ data buffers …   │  512 字节扇区的整数倍
//! ├──────────────────┤ device-writable
//! │ status (1 byte)  │  0=OK 1=IOERR 2=UNSUPP
//! └──────────────────┘
//! ```

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;

use crate::devices::virtio_mmio::{
    VirtioDevice, VirtioDeviceId, VirtqDesc, Virtqueue, VIRTQ_DESC_F_NEXT, VIRTQ_DESC_F_WRITE,
};
use crate::error::{Error, Result};
use crate::vmm::memory::GuestRam;

/// 扇区大小（字节）。
pub const SECTOR_SIZE: u64 = 512;

/// VirtIO 1.0 必需 feature（VIRTIO_F_VERSION_1）。
const VIRTIO_F_VERSION_1: u64 = 1 << 32;

// 请求类型（virtio_blk_req.type）
/// 读请求
const VIRTIO_BLK_T_IN: u32 = 0;
/// 写请求
const VIRTIO_BLK_T_OUT: u32 = 1;
/// flush 请求
const VIRTIO_BLK_T_FLUSH: u32 = 4;
/// 读取设备 ID 字符串
const VIRTIO_BLK_T_GET_ID: u32 = 8;

// 状态字节
const VIRTIO_BLK_S_OK: u8 = 0;
const VIRTIO_BLK_S_IOERR: u8 = 1;
const VIRTIO_BLK_S_UNSUPP: u8 = 2;

/// 描述符链最大长度（防御 guest 恶意构造的环）
const MAX_CHAIN_LEN: usize = 128;

/// virtio-blk 设备。
pub struct VirtioBlock {
    /// 后端镜像路径
    pub image_path: PathBuf,
    /// 容量（扇区数，每扇区 512 字节）
    pub capacity_sectors: u64,
    /// 只读设备
    read_only: bool,
    /// 后端镜像文件
    file: Mutex<File>,
    /// Guest RAM（virtqueue 与数据缓冲所在）
    ram: Arc<GuestRam>,
    /// 每个队列已处理到的 avail index
    last_avail: Mutex<Vec<u16>>,
}

impl VirtioBlock {
    /// 打开镜像文件创建块设备，容量按文件大小自动计算。
    pub fn open(image_path: PathBuf, ram: Arc<GuestRam>, read_only: bool) -> Result<Arc<Self>> {
        let file = OpenOptions::new()
            .read(true)
            .write(!read_only)
            .open(&image_path)
            .map_err(|e| Error::Vmm(format!("open disk image {}: {e}", image_path.display())))?;
        let len = file
            .metadata()
            .map_err(|e| Error::Vmm(format!("stat disk image: {e}")))?
            .len();

        Ok(Arc::new(Self {
            image_path,
            capacity_sectors: len / SECTOR_SIZE,
            read_only,
            file: Mutex::new(file),
            ram,
            last_avail: Mutex::new(vec![0; 4]),
        }))
    }

    // ── Guest 内存小端读写辅助 ──────────────────

    fn read_u16(&self, gpa: u64) -> Result<u16> {
        let mut b = [0u8; 2];
        self.ram.read(gpa, &mut b)?;
        Ok(u16::from_le_bytes(b))
    }

    fn write_u16(&self, gpa: u64, v: u16) -> Result<()> {
        self.ram.write(gpa, &v.to_le_bytes())
    }

    fn write_u32(&self, gpa: u64, v: u32) -> Result<()> {
        self.ram.write(gpa, &v.to_le_bytes())
    }

    fn read_desc(&self, desc_table: u64, idx: u16) -> Result<VirtqDesc> {
        let mut b = [0u8; 16];
        self.ram.read(desc_table + idx as u64 * 16, &mut b)?;
        Ok(VirtqDesc {
            addr: u64::from_le_bytes(b[0..8].try_into().unwrap()),
            len: u32::from_le_bytes(b[8..12].try_into().unwrap()),
            flags: u16::from_le_bytes(b[12..14].try_into().unwrap()),
            next: u16::from_le_bytes(b[14..16].try_into().unwrap()),
        })
    }

    /// 处理一条描述符链，返回写入 guest 的字节数（用于 used.len）。
    fn process_chain(&self, q: &Virtqueue, head: u16) -> Result<u32> {
        // 1. 收集整条链
        let mut chain = Vec::new();
        let mut idx = head;
        loop {
            let desc = self.read_desc(q.desc_addr, idx % q.size.max(1))?;
            chain.push(desc);
            if chain.len() > MAX_CHAIN_LEN {
                return Err(Error::Vmm("virtio-blk: descriptor chain too long".into()));
            }
            if desc.flags & VIRTQ_DESC_F_NEXT == 0 {
                break;
            }
            idx = desc.next;
        }
        if chain.len() < 2 {
            return Err(Error::Vmm("virtio-blk: chain too short".into()));
        }

        // 2. 解析请求头（第一个描述符，device-readable，16 字节）
        let hdr_desc = &chain[0];
        let mut hdr = [0u8; 16];
        if hdr_desc.len < 16 {
            return Err(Error::Vmm("virtio-blk: header descriptor too small".into()));
        }
        self.ram.read(hdr_desc.addr, &mut hdr)?;
        let req_type = u32::from_le_bytes(hdr[0..4].try_into().unwrap());
        let sector = u64::from_le_bytes(hdr[8..16].try_into().unwrap());

        // 3. 最后一个描述符是状态字节（device-writable）
        let status_desc = chain[chain.len() - 1];
        if status_desc.flags & VIRTQ_DESC_F_WRITE == 0 || status_desc.len < 1 {
            return Err(Error::Vmm("virtio-blk: bad status descriptor".into()));
        }
        let data_descs = &chain[1..chain.len() - 1];

        // 4. 执行请求
        let (status, data_written) = match req_type {
            VIRTIO_BLK_T_IN => match self.do_read(sector, data_descs) {
                Ok(n) => (VIRTIO_BLK_S_OK, n),
                Err(e) => {
                    tracing::warn!("virtio-blk read error @sector {sector}: {e}");
                    (VIRTIO_BLK_S_IOERR, 0)
                }
            },
            VIRTIO_BLK_T_OUT => {
                if self.read_only {
                    (VIRTIO_BLK_S_IOERR, 0)
                } else {
                    match self.do_write(sector, data_descs) {
                        Ok(()) => (VIRTIO_BLK_S_OK, 0),
                        Err(e) => {
                            tracing::warn!("virtio-blk write error @sector {sector}: {e}");
                            (VIRTIO_BLK_S_IOERR, 0)
                        }
                    }
                }
            }
            VIRTIO_BLK_T_FLUSH => match self.file.lock().sync_all() {
                Ok(()) => (VIRTIO_BLK_S_OK, 0),
                Err(_) => (VIRTIO_BLK_S_IOERR, 0),
            },
            VIRTIO_BLK_T_GET_ID => {
                let id = b"vsb-blk\0";
                let mut n = 0u32;
                if let Some(d) = data_descs.first() {
                    let len = (d.len as usize).min(id.len());
                    self.ram.write(d.addr, &id[..len])?;
                    n = len as u32;
                }
                (VIRTIO_BLK_S_OK, n)
            }
            other => {
                tracing::debug!("virtio-blk unsupported request type {other}");
                (VIRTIO_BLK_S_UNSUPP, 0)
            }
        };

        // 5. 写状态字节
        self.ram.write(status_desc.addr, &[status])?;

        // used.len = 设备写入的总字节数（数据 + 状态字节）
        Ok(data_written + 1)
    }

    /// 读镜像 → guest 缓冲。
    fn do_read(&self, sector: u64, descs: &[VirtqDesc]) -> Result<u32> {
        let mut file = self.file.lock();
        file.seek(SeekFrom::Start(sector * SECTOR_SIZE))
            .map_err(|e| Error::Vmm(format!("seek: {e}")))?;
        let mut total = 0u32;
        for d in descs {
            if d.flags & VIRTQ_DESC_F_WRITE == 0 {
                return Err(Error::Vmm("virtio-blk: IN data buffer not writable".into()));
            }
            let mut buf = vec![0u8; d.len as usize];
            file.read_exact(&mut buf)
                .map_err(|e| Error::Vmm(format!("read image: {e}")))?;
            self.ram.write(d.addr, &buf)?;
            total += d.len;
        }
        Ok(total)
    }

    /// guest 缓冲 → 写镜像。
    fn do_write(&self, sector: u64, descs: &[VirtqDesc]) -> Result<()> {
        let mut file = self.file.lock();
        file.seek(SeekFrom::Start(sector * SECTOR_SIZE))
            .map_err(|e| Error::Vmm(format!("seek: {e}")))?;
        for d in descs {
            let mut buf = vec![0u8; d.len as usize];
            self.ram.read(d.addr, &mut buf)?;
            file.write_all(&buf)
                .map_err(|e| Error::Vmm(format!("write image: {e}")))?;
        }
        Ok(())
    }
}

impl VirtioDevice for VirtioBlock {
    fn device_id(&self) -> VirtioDeviceId {
        VirtioDeviceId::Block
    }

    fn device_features(&self) -> u64 {
        // VIRTIO_BLK_F_RO = bit 5
        let ro = if self.read_only { 1 << 5 } else { 0 };
        VIRTIO_F_VERSION_1 | ro
    }

    fn notify(&self, queue_idx: u32, queues: &[Virtqueue]) -> Result<()> {
        let q = queues
            .get(queue_idx as usize)
            .ok_or_else(|| Error::Vmm(format!("virtio-blk: bad queue {queue_idx}")))?;
        if !q.ready || q.size == 0 {
            return Ok(());
        }

        // split virtqueue 布局：
        //   avail: flags(u16) idx(u16) ring[size](u16)
        //   used:  flags(u16) idx(u16) ring[size]{id(u32) len(u32)}
        let avail_idx = self.read_u16(q.avail_addr + 2)?;
        let mut last = {
            let guard = self.last_avail.lock();
            *guard
                .get(queue_idx as usize)
                .ok_or_else(|| Error::Vmm("virtio-blk: queue idx out of range".into()))?
        };

        let mut used_idx = self.read_u16(q.used_addr + 2)?;
        while last != avail_idx {
            let slot = (last % q.size) as u64;
            let head = self.read_u16(q.avail_addr + 4 + slot * 2)?;

            let written = match self.process_chain(q, head) {
                Ok(n) => n,
                Err(e) => {
                    tracing::error!("virtio-blk request failed: {e}");
                    0
                }
            };

            // 写 used ring entry
            let used_slot = (used_idx % q.size) as u64;
            let entry_gpa = q.used_addr + 4 + used_slot * 8;
            self.write_u32(entry_gpa, head as u32)?;
            self.write_u32(entry_gpa + 4, written)?;

            used_idx = used_idx.wrapping_add(1);
            last = last.wrapping_add(1);
        }

        // 更新 used.idx（先写 entry 再发布 idx；x86 下 TSO 保证顺序，加栅栏防御）
        std::sync::atomic::fence(std::sync::atomic::Ordering::SeqCst);
        self.write_u16(q.used_addr + 2, used_idx)?;

        if let Some(slot) = self.last_avail.lock().get_mut(queue_idx as usize) {
            *slot = last;
        }
        Ok(())
    }

    fn read_config(&self, offset: u64, size: u8) -> u32 {
        // 配置空间偏移 0..8：capacity（u64 LE，单位扇区）
        let cap = self.capacity_sectors.to_le_bytes();
        let mut out = [0u8; 4];
        for (i, slot) in out.iter_mut().enumerate().take(size.min(4) as usize) {
            let src = offset as usize + i;
            if src < cap.len() {
                *slot = cap[src];
            }
        }
        u32::from_le_bytes(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RAM_SIZE: usize = 1024 * 1024;
    const DESC_TABLE: u64 = 0x1000;
    const AVAIL_RING: u64 = 0x2000;
    const USED_RING: u64 = 0x3000;
    const QUEUE_SIZE: u16 = 8;

    struct TestRig {
        ram: Arc<GuestRam>,
        dev: Arc<VirtioBlock>,
        _tmp: tempfile::NamedTempFile,
        queue: Virtqueue,
    }

    fn setup(image_content: &[u8]) -> TestRig {
        let ram = GuestRam::allocate(0, RAM_SIZE).unwrap();
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(image_content).unwrap();
        tmp.flush().unwrap();
        let dev = VirtioBlock::open(tmp.path().to_path_buf(), Arc::clone(&ram), false).unwrap();
        let queue = Virtqueue {
            size: QUEUE_SIZE,
            desc_addr: DESC_TABLE,
            avail_addr: AVAIL_RING,
            used_addr: USED_RING,
            ready: true,
            last_avail_idx: 0,
        };
        TestRig {
            ram,
            dev,
            _tmp: tmp,
            queue,
        }
    }

    fn write_desc(ram: &GuestRam, idx: u16, addr: u64, len: u32, flags: u16, next: u16) {
        let mut b = Vec::with_capacity(16);
        b.extend_from_slice(&addr.to_le_bytes());
        b.extend_from_slice(&len.to_le_bytes());
        b.extend_from_slice(&flags.to_le_bytes());
        b.extend_from_slice(&next.to_le_bytes());
        ram.write(DESC_TABLE + idx as u64 * 16, &b).unwrap();
    }

    /// 提交一个请求（描述符 0=头, 1=数据, 2=状态），返回 avail idx。
    fn submit_request(
        rig: &TestRig,
        req_type: u32,
        sector: u64,
        data_gpa: u64,
        data_len: u32,
        avail_pos: u16,
    ) {
        // 请求头 @ 0x5000 + avail_pos*0x100
        let hdr_gpa = 0x5000 + avail_pos as u64 * 0x100;
        let mut hdr = Vec::with_capacity(16);
        hdr.extend_from_slice(&req_type.to_le_bytes());
        hdr.extend_from_slice(&0u32.to_le_bytes());
        hdr.extend_from_slice(&sector.to_le_bytes());
        rig.ram.write(hdr_gpa, &hdr).unwrap();

        let status_gpa = hdr_gpa + 0x80;
        rig.ram.write(status_gpa, &[0xEE]).unwrap(); // 先填非法值以验证被覆盖

        let base = avail_pos.wrapping_mul(3) % QUEUE_SIZE;
        let d0 = base;
        let d1 = (base + 1) % QUEUE_SIZE;
        let d2 = (base + 2) % QUEUE_SIZE;
        let data_write_flag = if req_type == VIRTIO_BLK_T_IN {
            VIRTQ_DESC_F_WRITE
        } else {
            0
        };
        write_desc(&rig.ram, d0, hdr_gpa, 16, VIRTQ_DESC_F_NEXT, d1);
        write_desc(
            &rig.ram,
            d1,
            data_gpa,
            data_len,
            VIRTQ_DESC_F_NEXT | data_write_flag,
            d2,
        );
        write_desc(&rig.ram, d2, status_gpa, 1, VIRTQ_DESC_F_WRITE, 0);

        // avail ring：ring[avail_pos % size] = 链头，然后发布 idx = avail_pos+1
        rig.ram
            .write(
                AVAIL_RING + 4 + (avail_pos % QUEUE_SIZE) as u64 * 2,
                &d0.to_le_bytes(),
            )
            .unwrap();
        rig.ram
            .write(AVAIL_RING + 2, &(avail_pos.wrapping_add(1)).to_le_bytes())
            .unwrap();
    }

    fn read_status(rig: &TestRig, avail_pos: u16) -> u8 {
        let status_gpa = 0x5000 + avail_pos as u64 * 0x100 + 0x80;
        let mut b = [0u8; 1];
        rig.ram.read(status_gpa, &mut b).unwrap();
        b[0]
    }

    fn used_idx(rig: &TestRig) -> u16 {
        let mut b = [0u8; 2];
        rig.ram.read(USED_RING + 2, &mut b).unwrap();
        u16::from_le_bytes(b)
    }

    #[test]
    fn capacity_from_image_size() {
        let rig = setup(&vec![0u8; 4096]);
        assert_eq!(rig.dev.capacity_sectors, 8);
        // 配置空间读 capacity（4 字节小端）
        assert_eq!(rig.dev.read_config(0, 4), 8);
        assert_eq!(rig.dev.read_config(4, 4), 0);
    }

    #[test]
    fn read_request_returns_image_data() {
        // 镜像：扇区 0 全 0xAA，扇区 1 全 0xBB
        let mut img = vec![0xAAu8; 512];
        img.extend_from_slice(&[0xBBu8; 512]);
        let rig = setup(&img);

        let data_gpa = 0x10000;
        submit_request(&rig, VIRTIO_BLK_T_IN, 1, data_gpa, 512, 0);
        rig.dev.notify(0, std::slice::from_ref(&rig.queue)).unwrap();

        assert_eq!(read_status(&rig, 0), VIRTIO_BLK_S_OK);
        assert_eq!(used_idx(&rig), 1);
        let mut got = vec![0u8; 512];
        rig.ram.read(data_gpa, &mut got).unwrap();
        assert!(got.iter().all(|&b| b == 0xBB), "sector 1 should be 0xBB");

        // used.len = 512 数据 + 1 状态
        let mut lenb = [0u8; 4];
        rig.ram.read(USED_RING + 4 + 4, &mut lenb).unwrap();
        assert_eq!(u32::from_le_bytes(lenb), 513);
    }

    #[test]
    fn write_request_persists_to_image() {
        let rig = setup(&vec![0u8; 2048]);

        let data_gpa = 0x20000;
        rig.ram.write(data_gpa, &[0x5Au8; 512]).unwrap();
        submit_request(&rig, VIRTIO_BLK_T_OUT, 2, data_gpa, 512, 0);
        rig.dev.notify(0, std::slice::from_ref(&rig.queue)).unwrap();

        assert_eq!(read_status(&rig, 0), VIRTIO_BLK_S_OK);

        // 直接读镜像文件验证
        let mut content = std::fs::read(&rig.dev.image_path).unwrap();
        assert_eq!(content.len(), 2048);
        assert!(content.drain(1024..1536).all(|b| b == 0x5A));
    }

    #[test]
    fn out_of_range_read_reports_ioerr() {
        let rig = setup(&vec![0u8; 1024]); // 2 个扇区
        submit_request(&rig, VIRTIO_BLK_T_IN, 100, 0x30000, 512, 0);
        rig.dev.notify(0, std::slice::from_ref(&rig.queue)).unwrap();
        assert_eq!(read_status(&rig, 0), VIRTIO_BLK_S_IOERR);
        // 出错也要发布 used entry，否则 guest 永远等待
        assert_eq!(used_idx(&rig), 1);
    }

    #[test]
    fn unsupported_request_reports_unsupp() {
        let rig = setup(&vec![0u8; 1024]);
        submit_request(&rig, 0xDEAD, 0, 0x30000, 512, 0);
        rig.dev.notify(0, std::slice::from_ref(&rig.queue)).unwrap();
        assert_eq!(read_status(&rig, 0), VIRTIO_BLK_S_UNSUPP);
    }

    #[test]
    fn multiple_requests_processed_in_order() {
        let mut img = vec![0x11u8; 512];
        img.extend_from_slice(&[0x22u8; 512]);
        let rig = setup(&img);

        submit_request(&rig, VIRTIO_BLK_T_IN, 0, 0x40000, 512, 0);
        submit_request(&rig, VIRTIO_BLK_T_IN, 1, 0x41000, 512, 1);
        rig.dev.notify(0, std::slice::from_ref(&rig.queue)).unwrap();

        assert_eq!(used_idx(&rig), 2);
        let mut a = [0u8; 1];
        let mut b = [0u8; 1];
        rig.ram.read(0x40000, &mut a).unwrap();
        rig.ram.read(0x41000, &mut b).unwrap();
        assert_eq!(a[0], 0x11);
        assert_eq!(b[0], 0x22);

        // 再次 notify 无新请求：used idx 不变（last_avail 正确推进）
        rig.dev.notify(0, std::slice::from_ref(&rig.queue)).unwrap();
        assert_eq!(used_idx(&rig), 2);
    }

    #[test]
    fn read_only_device_rejects_writes() {
        let ram = GuestRam::allocate(0, RAM_SIZE).unwrap();
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        tmp.write_all(&vec![0u8; 1024]).unwrap();
        tmp.flush().unwrap();
        let dev = VirtioBlock::open(tmp.path().to_path_buf(), Arc::clone(&ram), true).unwrap();
        assert!(dev.device_features() & (1 << 5) != 0, "RO feature bit");

        let rig = TestRig {
            ram,
            dev,
            _tmp: tmp,
            queue: Virtqueue {
                size: QUEUE_SIZE,
                desc_addr: DESC_TABLE,
                avail_addr: AVAIL_RING,
                used_addr: USED_RING,
                ready: true,
                last_avail_idx: 0,
            },
        };
        rig.ram.write(0x20000, &[1u8; 512]).unwrap();
        submit_request(&rig, VIRTIO_BLK_T_OUT, 0, 0x20000, 512, 0);
        rig.dev.notify(0, std::slice::from_ref(&rig.queue)).unwrap();
        assert_eq!(read_status(&rig, 0), VIRTIO_BLK_S_IOERR);
    }
}
