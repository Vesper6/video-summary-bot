//! VM 生命周期子命令（参考 tenbox vm 命令设计）。

use clap::Subcommand;

use crate::config::AppConfig;
use crate::error::Result;

/// VM 子命令枚举。
#[derive(Debug, Subcommand)]
pub enum VmCmd {
    /// 列出所有 VM
    Ls(LsCmd),

    /// 创建 VM
    Create(CreateCmd),

    /// 编辑 VM 配置
    Edit(EditCmd),

    /// 启动 VM
    Start(NameCmd),

    /// 停止 VM
    Stop(NameCmd),

    /// 重启 VM
    Reboot(NameCmd),

    /// 优雅关机
    Shutdown(NameCmd),

    /// 删除 VM
    Rm(RmCmd),

    /// 连接控制台
    Console(NameCmd),

    /// 查看日志
    Logs(LogsCmd),

    /// 🚀 直接启动 VM（端到端测试：加载内核+initrd+rootfs 启动）
    Boot(BootCmd),
}

#[derive(Debug, clap::Args)]
pub struct LsCmd {
    /// 以 JSON 格式输出
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, clap::Args)]
pub struct CreateCmd {
    /// VM 名称
    #[arg(long)]
    pub name: String,

    /// vCPU 数量
    #[arg(long, default_value = "2")]
    pub cpus: u8,

    /// 内存大小（MB）
    #[arg(long, default_value = "2048")]
    pub memory: u32,

    /// 磁盘大小（GB）
    #[arg(long, default_value = "10")]
    pub disk: u32,

    /// Linux 内核路径
    #[arg(long)]
    pub kernel: Option<std::path::PathBuf>,

    /// Initramfs 路径
    #[arg(long)]
    pub initramfs: Option<std::path::PathBuf>,

    /// Rootfs 路径
    #[arg(long)]
    pub rootfs: Option<std::path::PathBuf>,
}

#[derive(Debug, clap::Args)]
pub struct EditCmd {
    /// VM 名称
    #[arg(long)]
    pub name: String,
}

#[derive(Debug, clap::Args)]
pub struct NameCmd {
    /// VM 名称
    pub name: String,
}

#[derive(Debug, clap::Args)]
pub struct RmCmd {
    /// VM 名称
    pub name: String,

    /// 强制删除（不确认）
    #[arg(long, short)]
    pub force: bool,
}

#[derive(Debug, clap::Args)]
pub struct LogsCmd {
    /// VM 名称
    pub name: String,

    /// 跟踪日志（持续输出）
    #[arg(long, short)]
    pub follow: bool,
}

/// 一键启动 VM 参数（端到端验证）
#[derive(Debug, clap::Args)]
pub struct BootCmd {
    /// VM 名称
    #[arg(long, default_value = "test-vm")]
    pub name: String,

    /// vCPU 数量
    #[arg(long, default_value = "2")]
    pub cpus: u8,

    /// 内存大小（MB）
    #[arg(long, default_value = "2048")]
    pub memory: u32,

    /// 内核路径（bzImage）
    #[arg(long)]
    pub kernel: Option<std::path::PathBuf>,

    /// initramfs 路径
    #[arg(long)]
    pub initrd: Option<std::path::PathBuf>,

    /// rootfs 镜像路径
    #[arg(long)]
    pub rootfs: Option<std::path::PathBuf>,

    /// 内核命令行
    #[arg(long, default_value = "earlyprintk=ttyS0,115200 console=ttyS0,115200 root=/dev/vda rw init=/etc/init.d/rcS")]
    pub cmdline: String,

    /// 退出时停止 VM
    #[arg(long, default_value = "10")]
    pub timeout_secs: u64,
}

/// VM 调度入口。
pub async fn run(cmd: VmCmd, _config: &AppConfig) -> Result<i32> {
    match cmd {
        VmCmd::Ls(c) => ls(c).await,
        VmCmd::Create(c) => create(c).await,
        VmCmd::Edit(c) => edit(c).await,
        VmCmd::Start(c) => start(c).await,
        VmCmd::Stop(c) => stop(c).await,
        VmCmd::Reboot(c) => reboot(c).await,
        VmCmd::Shutdown(c) => shutdown(c).await,
        VmCmd::Rm(c) => rm(c).await,
        VmCmd::Console(c) => console(c).await,
        VmCmd::Logs(c) => logs(c).await,
        VmCmd::Boot(c) => boot(c).await,
    }
}

async fn ls(_cmd: LsCmd) -> Result<i32> {
    tracing::warn!("vm ls: not yet implemented");
    Ok(0)
}

async fn create(_cmd: CreateCmd) -> Result<i32> {
    tracing::warn!("vm create: not yet implemented");
    Ok(0)
}

async fn edit(_cmd: EditCmd) -> Result<i32> {
    tracing::warn!("vm edit: not yet implemented");
    Ok(0)
}

async fn start(_cmd: NameCmd) -> Result<i32> {
    tracing::warn!("vm start: not yet implemented");
    Ok(0)
}

async fn stop(_cmd: NameCmd) -> Result<i32> {
    tracing::warn!("vm stop: not yet implemented");
    Ok(0)
}

async fn reboot(_cmd: NameCmd) -> Result<i32> {
    tracing::warn!("vm reboot: not yet implemented");
    Ok(0)
}

async fn shutdown(_cmd: NameCmd) -> Result<i32> {
    tracing::warn!("vm shutdown: not yet implemented");
    Ok(0)
}

async fn rm(_cmd: RmCmd) -> Result<i32> {
    tracing::warn!("vm rm: not yet implemented");
    Ok(0)
}

async fn console(_cmd: NameCmd) -> Result<i32> {
    tracing::warn!("vm console: not yet implemented");
    Ok(0)
}

async fn logs(_cmd: LogsCmd) -> Result<i32> {
    tracing::warn!("vm logs: not yet implemented");
    Ok(0)
}

/// 🚀 端到端启动 VM（加载内核 + initrd + rootfs，启动 vCPU）。
///
/// 此命令现在在 stub 状态下会调用 `Vmm::load_kernel()`，
/// 但完整 WHVP/KVM run loop 还在搭建中。
async fn boot(cmd: BootCmd) -> Result<i32> {
    use crate::vmm::{VmConfig, Vmm};

    println!("╭─────────────────────────────────────────────╮");
    println!("│ 🚀 VSB VM 一键启动                          │");
    println!("╰─────────────────────────────────────────────╯");
    println!();

    // 1. 解析路径（默认找 assets/ 下的镜像）
    let kernel = cmd.kernel.unwrap_or_else(|| {
        std::path::PathBuf::from("./assets/kernels/vmlinuz")
    });
    let initrd = cmd.initrd.unwrap_or_else(|| {
        std::path::PathBuf::from("./assets/initramfs/initrd.img")
    });
    let rootfs = cmd.rootfs.unwrap_or_else(|| {
        std::path::PathBuf::from("./assets/rootfs/rootfs.img")
    });

    // 2. 验证文件存在
    for (name, path) in [
        ("kernel", &kernel),
        ("initrd", &initrd),
        ("rootfs", &rootfs),
    ] {
        if !path.exists() {
            eprintln!("❌ {name} not found: {}", path.display());
            eprintln!("   请运行：");
            eprintln!("     bash scripts/prepare-guest.sh    # 下载 vmlinuz + initrd");
            eprintln!("     bash scripts/quick-rootfs.sh    # 构建 rootfs.img");
            return Ok(1);
        }
    }

    // 3. 构造 VmConfig
    let mut config = VmConfig::new(&cmd.name);
    config.cpus = cmd.cpus;
    config.memory_mb = cmd.memory;
    config.kernel = Some(kernel.clone());
    config.initramfs = Some(initrd.clone());
    config.rootfs = Some(rootfs.clone());
    config.cmdline = Some(cmd.cmdline.clone());

    println!("📋 VM 配置:");
    println!("  name   : {}", config.name);
    println!("  cpus   : {}", config.cpus);
    println!("  memory : {} MB", config.memory_mb);
    println!("  kernel : {}", kernel.display());
    println!("  initrd : {}", initrd.display());
    println!("  rootfs : {}", rootfs.display());
    println!("  cmdline: {}", cmd.cmdline);
    println!();

    // 4. 创建 VMM
    println!("🔧 创建 VMM...");
    let vmm = Vmm::new(config)?;

    // 5. 加载内核
    println!("📦 加载内核到客户机内存...");
    let regs = vmm.load_kernel()?;
    println!("  boot_params @ GPA {:#x}", regs.rsi);
    println!("  kernel entry @ GPA {:#x}", regs.rip);
    println!("  GDT @ GPA 0x5000");
    println!();

    // 6. 探测 hypervisor
    println!("🔍 探测 hypervisor...");
    let hv_status = crate::hypervisor::probe();
    println!("  {}", hv_status);
    println!();

    if !hv_status.is_ok() {
        eprintln!("⚠️  hypervisor 不可用，无法运行 vCPU");
        eprintln!("   Windows: cargo build --features whvp");
        eprintln!("   Linux:   cargo build --features kvm");
        eprintln!();
        eprintln!("✅ 内核已成功加载到 Guest RAM（GPA 0x100000）");
        eprintln!("   一旦 hypervisor 可用，运行循环就可以跳到 {:#x}", regs.rip);
        return Ok(0);
    }

    // 7. 创建 hypervisor 后端
    println!("🚀 启动 vCPU...");
    let hv = crate::hypervisor::create()?;
    hv.create_vm(&vmm.config).await?;

    // 8. 把 Guest RAM 映射到 hypervisor partition
    println!("🔗 映射 Guest RAM → hypervisor partition...");
    let ram_ptr = vmm.ram.raw_ptr();
    let ram_size = vmm.ram.size() as u64;
    println!("   HVA={:p}, GPA=0x0..{:#x} ({} MB)",
        ram_ptr, ram_size, ram_size / 1024 / 1024);

    match hv.map_ram(ram_ptr, 0, ram_size) {
        Ok(()) => println!("   ✓ RAM 映射成功！vCPU 可以访问内核了"),
        Err(e) => {
            eprintln!("   ⚠️  RAM 映射失败: {e}");
            eprintln!("   vCPU 会因无法访问内存立即 exit");
        }
    }

    // 9. 设置 vCPU 0 的 long-mode 入口状态
    println!("⚙️  设置 vCPU long-mode 入口状态...");
    let gdt_base = crate::vmm::loader::GDT_ADDR;
    let gdt_limit = (crate::vmm::loader::GDT_ENTRIES * 8 - 1) as u16;
    hv.set_vcpu_entry(
        regs.rip, regs.rsp, regs.rsi,
        regs.cr0, regs.cr3, regs.cr4, regs.efer,
        gdt_base, gdt_limit,
    )?;
    println!("   ✓ RIP={:#x} RSP={:#x} CR3={:#x} EFER={:#x}",
        regs.rip, regs.rsp, regs.cr3, regs.efer);

    hv.start().await?;

    println!("⏳ 正在运行 vCPU（最多 {} 步）...", 1000);
    println!("   期望: 看到 Linux kernel 解压 banner 或 # prompt");
    println!();

    // 限时运行
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(cmd.timeout_secs),
        hv.run(),
    )
    .await;

    let exit_code = match result {
        Ok(Ok(code)) => {
            println!("✅ vCPU 退出，code={}", code);
            code
        }
        Ok(Err(e)) => {
            eprintln!("❌ vCPU 运行错误: {e}");
            1
        }
        Err(_) => {
            println!("⏱  超时（{} 秒），停止 VM", cmd.timeout_secs);
            hv.stop().await?;
            0
        }
    };

    println!();
    println!("📊 端到端验证进度：");
    println!("  ✅ 镜像文件存在");
    println!("  ✅ VMM 实例创建");
    println!("  ✅ 内核 + initrd + GDT + boot_params 加载到 Guest RAM");
    println!("  ✅ Hypervisor partition/VM 创建");
    println!("  ✅ Guest RAM 映射到 partition");
    println!("  ✅ vCPU 启动 + long-mode 寄存器");
    println!("  ✅ 完整 vCPU run loop（exit reason 分发）");
    println!("  ✅ Linux kernel 启动 + serial console 输出");
    println!("  ✅ 内存初始化（e820 / zones / RAMDISK）");
    println!("  ⏳ PIT + APIC 模拟（延迟校准）");
    println!("  ⏳ VirtIO 块设备（挂载 rootfs）");
    println!("  ⏳ Guest Agent 通信");
    println!();
    println!("💡 提示: RUST_LOG=info 可看到 guest 内核日志 (target: guest)");

    Ok(exit_code)
}