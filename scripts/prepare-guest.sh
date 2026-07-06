#!/usr/bin/env bash
# =============================================================
# prepare-guest.sh
# 下载 Debian vmlinuz + 构建最小 initramfs
#
# 参考 tenbox 做法：
# - 内核直接从 Debian Trixie 仓库下载（不自己编译）
# - initramfs 用 BusyBox static + VirtIO 内核模块
#
# 用法：
#   bash scripts/prepare-guest.sh
#
# 产物：
#   assets/kernels/vmlinuz          Linux 内核（bzImage）
#   assets/initramfs/initrd.img     初始 ramdisk
# =============================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
ASSETS="$REPO_ROOT/assets"
KERNEL_DIR="$ASSETS/kernels"
INITRAMFS_DIR="$ASSETS/initramfs"
TMP_DIR="$(mktemp -d)"

# 颜色
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

log()  { echo -e "${GREEN}[+]${NC} $*"; }
warn() { echo -e "${YELLOW}[!]${NC} $*"; }
die()  { echo -e "${RED}[✗]${NC} $*" >&2; exit 1; }

cleanup() { rm -rf "$TMP_DIR"; }
trap cleanup EXIT

# =============================================================
# 1. 下载 Debian Trixie vmlinuz + initrd
# =============================================================
DEBIAN_MIRROR="${DEBIAN_MIRROR:-https://deb.debian.org/debian}"
ARCH="amd64"
SUITE="trixie"

log "Fetching Debian $SUITE kernel package list..."
PKG_URL="$DEBIAN_MIRROR/dists/$SUITE/main/binary-$ARCH/Packages.gz"
curl -fsSL "$PKG_URL" | gunzip > "$TMP_DIR/Packages"

# linux-image-amd64 是元包，它 Depends 于真正的内核包
# 先找元包的 Depends，再找真实内核包
META_DEPENDS=$(grep -A 30 '^Package: linux-image-amd64$' "$TMP_DIR/Packages" \
  | grep '^Depends:' | head -1 | grep -oP 'linux-image-[0-9][^ ,)]+' | head -1)

if [ -n "$META_DEPENDS" ]; then
    log "Real kernel package: $META_DEPENDS"
    REAL_PKG="$META_DEPENDS"
else
    # 直接搜索 linux-image-*-amd64（含版本号的真实包）
    REAL_PKG=$(grep '^Package: linux-image-[0-9]' "$TMP_DIR/Packages" \
      | grep 'amd64' | head -1 | awk '{print $2}')
    log "Found kernel package: $REAL_PKG"
fi

[ -z "$REAL_PKG" ] && die "Could not find real kernel package"

DEB_PATH=$(grep -A 30 "^Package: ${REAL_PKG}$" "$TMP_DIR/Packages" \
  | grep '^Filename:' | head -1 | awk '{print $2}')

[ -z "$DEB_PATH" ] && die "Could not find .deb path for $REAL_PKG"

DEB_URL="$DEBIAN_MIRROR/$DEB_PATH"
DEB_FILE="$TMP_DIR/linux-image.deb"

log "Downloading kernel: $DEB_URL"
curl -fsSL --progress-bar "$DEB_URL" -o "$DEB_FILE"

# 解压 .deb → 取出 vmlinuz
log "Extracting vmlinuz from .deb..."
cd "$TMP_DIR"
ar x "$DEB_FILE"

# data.tar 可能是 .xz / .zst / .gz，解压全部内容
if [ -f data.tar.xz ]; then
    tar xJf data.tar.xz -C "$TMP_DIR" 2>/dev/null || true
elif [ -f data.tar.zst ]; then
    tar --zstd -xf data.tar.zst -C "$TMP_DIR" 2>/dev/null || true
elif [ -f data.tar.gz ]; then
    tar xzf data.tar.gz -C "$TMP_DIR" 2>/dev/null || true
else
    # 尝试所有可能的 data.tar 变体
    for f in data.tar.*; do
        [ -f "$f" ] && tar xf "$f" -C "$TMP_DIR" 2>/dev/null && break || true
    done
fi

VMLINUZ=$(find "$TMP_DIR" -name 'vmlinuz-*' -not -path '*/proc/*' | head -1)
[ -z "$VMLINUZ" ] && die "vmlinuz not found in .deb"

KERNEL_VER=$(basename "$VMLINUZ" | sed 's/vmlinuz-//')
log "Kernel version: $KERNEL_VER"

cp "$VMLINUZ" "$KERNEL_DIR/vmlinuz"
log "Saved: $KERNEL_DIR/vmlinuz"

# =============================================================
# 2. 提取 VirtIO 内核模块
# =============================================================
log "Looking for kernel modules package..."
MODULES_PATH=$(grep -A 20 "^Package: linux-image-${KERNEL_VER}$" "$TMP_DIR/Packages" \
  | grep '^Filename:' | head -1 | awk '{print $2}')

MODULES_DIR="$TMP_DIR/modules"
mkdir -p "$MODULES_DIR"

if [ -n "$MODULES_PATH" ]; then
    log "Downloading kernel modules: $MODULES_PATH"
    MODULES_DEB="$TMP_DIR/linux-modules.deb"
    curl -fsSL --progress-bar "$DEBIAN_MIRROR/$MODULES_PATH" -o "$MODULES_DEB"

    cd "$TMP_DIR"
    ar x "$MODULES_DEB" modules.deb 2>/dev/null || true
    ar x "$MODULES_DEB"

    if [ -f data.tar.xz ]; then
        tar xJf data.tar.xz -C "$MODULES_DIR" \
            "./lib/modules/$KERNEL_VER/kernel/drivers/virtio" 2>/dev/null || true
        tar xJf data.tar.xz -C "$MODULES_DIR" \
            "./lib/modules/$KERNEL_VER/modules.dep" 2>/dev/null || true
    fi
else
    warn "Modules package not found, initramfs will use built-in virtio"
fi

# =============================================================
# 3. 构建 initramfs（BusyBox + VirtIO 模块 + init 脚本）
# =============================================================
log "Building initramfs..."
INITRAMFS_WORK="$TMP_DIR/initramfs"
mkdir -p "$INITRAMFS_WORK"/{bin,sbin,etc,proc,sys,dev,tmp,mnt,run,lib/modules}

# BusyBox
if command -v busybox-static &>/dev/null; then
    BUSYBOX=$(which busybox-static)
elif command -v busybox &>/dev/null; then
    BUSYBOX=$(which busybox)
else
    die "busybox not found. Install with: apt install busybox-static"
fi

cp "$BUSYBOX" "$INITRAMFS_WORK/bin/busybox"
chmod +x "$INITRAMFS_WORK/bin/busybox"

# 安装 busybox applets
for applet in sh ls mount echo cat sleep insmod modprobe mdev mkdir ln; do
    ln -sf /bin/busybox "$INITRAMFS_WORK/bin/$applet"
done
ln -sf /bin/busybox "$INITRAMFS_WORK/sbin/init"

# 复制 VirtIO 内核模块（如果有）
VIRTIO_MODS=$(find "$MODULES_DIR" -name "virtio*.ko*" 2>/dev/null || true)
if [ -n "$VIRTIO_MODS" ]; then
    log "Embedding VirtIO modules..."
    mkdir -p "$INITRAMFS_WORK/lib/modules/$KERNEL_VER"
    find "$MODULES_DIR" -name "*.ko*" | while read -r ko; do
        rel="${ko#$MODULES_DIR/lib/modules/$KERNEL_VER/}"
        dst="$INITRAMFS_WORK/lib/modules/$KERNEL_VER/$rel"
        mkdir -p "$(dirname "$dst")"
        cp "$ko" "$dst"
    done
    [ -f "$MODULES_DIR/lib/modules/$KERNEL_VER/modules.dep" ] && \
        cp "$MODULES_DIR/lib/modules/$KERNEL_VER/modules.dep" \
           "$INITRAMFS_WORK/lib/modules/$KERNEL_VER/"
fi

# init 脚本
cat > "$INITRAMFS_WORK/init" << 'INIT_EOF'
#!/bin/sh
# vsb initramfs init

mount -t proc proc /proc
mount -t sysfs sysfs /sys
mount -t devtmpfs devtmpfs /dev 2>/dev/null || \
    mdev -s

# 加载 VirtIO 驱动
for m in virtio virtio_ring virtio_pci virtio_mmio \
          virtio_blk virtio_net virtio_console virtiofs; do
    modprobe $m 2>/dev/null || true
done

# 等待 VirtIO 块设备就绪
DISK=""
for i in $(seq 1 20); do
    [ -b /dev/vda ] && DISK=/dev/vda && break
    sleep 0.5
done

if [ -n "$DISK" ]; then
    mount -o ro "$DISK" /mnt 2>/dev/null && \
        exec switch_root /mnt /sbin/init ||
        exec switch_root /mnt /bin/init ||
        exec switch_root /mnt /etc/init.d/rcS
fi

echo "[vsb] No rootfs found, dropping to shell"
exec /bin/sh
INIT_EOF
chmod +x "$INITRAMFS_WORK/init"

# 打包 initramfs
log "Packing initramfs.img..."
(cd "$INITRAMFS_WORK" && find . | cpio --quiet -H newc -o) \
    | gzip -9 > "$INITRAMFS_DIR/initrd.img"

INITRD_SIZE=$(du -sh "$INITRAMFS_DIR/initrd.img" | cut -f1)
log "Saved: $INITRAMFS_DIR/initrd.img ($INITRD_SIZE)"

# =============================================================
# 4. 输出摘要
# =============================================================
echo ""
echo "======================================================"
log "Guest preparation complete!"
echo ""
echo "  Kernel : $KERNEL_DIR/vmlinuz"
echo "  Initrd : $INITRAMFS_DIR/initrd.img"
echo "  Version: $KERNEL_VER"
echo "======================================================"
echo ""
echo "Next step: bash scripts/build-rootfs.sh"
