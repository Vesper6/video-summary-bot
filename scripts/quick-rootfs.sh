#!/bin/bash
# quick-rootfs via losetup (WSL2-friendly)
set -e
IMG=/tmp/vsb/rootfs.img
MNT=/tmp/vsb/mnt
rm -rf "$MNT"
mkdir -p "$MNT"
rm -f "$IMG"

echo "[+] dd..."
dd if=/dev/zero of="$IMG" bs=1M count=128 status=none

echo "[+] mkfs.ext4..."
mkfs.ext4 -q -F -L vsb-rootfs "$IMG"

echo "[+] losetup..."
LOOP=$(losetup --find --show "$IMG")
echo "  loop device: $LOOP"

echo "[+] mount (using loop device directly)..."
mount "$LOOP" "$MNT"

echo "[+] populate directories..."
mkdir -p "$MNT"/{bin,sbin,etc,proc,sys,dev,tmp,root,home,usr/bin,usr/sbin,var,lib,run,mnt,opt,etc/init.d}

echo "[+] install busybox..."
cp /usr/bin/busybox "$MNT/bin/"
chmod +x "$MNT/bin/busybox"
for a in sh mount umount echo cat ls cp mv rm mkdir ln ps ip ifconfig route ping sleep dmesg; do
    ln -sf /bin/busybox "$MNT/bin/$a" 2>/dev/null || true
done

echo "[+] write config files..."
echo "root:x:0:0:root:/root:/bin/sh" | tee "$MNT/etc/passwd" >/dev/null
echo "root:x:0:" | tee "$MNT/etc/group" >/dev/null
echo "vsb-guest" | tee "$MNT/etc/hostname" >/dev/null

printf "127.0.0.1 localhost vsb-guest\n::1 localhost\n" | tee "$MNT/etc/hosts" >/dev/null

printf "/dev/vda / ext4 defaults,ro 0 1\nproc /proc proc defaults 0 0\nsysfs /sys sysfs defaults 0 0\ndevtmpfs /dev devtmpfs defaults 0 0\ntmpfs /tmp tmpfs defaults 0 0\n" | tee "$MNT/etc/fstab" >/dev/null

printf "::sysinit:/etc/init.d/rcS\n::respawn:/sbin/getty -L 115200 hvc0 vt100\n::ctrlaltdel:/sbin/reboot\n::shutdown:/etc/init.d/rcK\n" | tee "$MNT/etc/inittab" >/dev/null

tee "$MNT/etc/init.d/rcS" >/dev/null << 'INITEOF'
#!/bin/sh
mount -t proc proc /proc 2>/dev/null
mount -t sysfs sysfs /sys 2>/dev/null
mount -t devtmpfs devtmpfs /dev 2>/dev/null
echo "=========================="
echo "  VSB Guest Linux v0.1"
echo "=========================="
hostname
echo ""
echo "VSB-AGENT running..."
while true; do
    echo "[$(date +%T)] vsb-agent: ready"
    sleep 10
done
INITEOF
chmod +x "$MNT/etc/init.d/rcS"

echo "[+] sync + cleanup..."
sync
umount "$LOOP"
losetup -d "$LOOP"
ls -la "$IMG"
echo ""
echo "Image ready: $IMG"
echo "Now copy to: /mnt/d/Zycrest/Agents/Video-Summary-Bot/assets/rootfs/rootfs.img"