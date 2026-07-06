#!/usr/bin/env bash
# =============================================================
# build-rootfs.sh
# 构建 Guest rootfs（Alpine minimal + Agent 预装）
#
# 参考 tenbox 做法：
# - 使用 Alpine Linux miniroot（比 Debian debootstrap 快很多）
# - 预装 Python3 / yt-dlp / curl
# - 预装 vsb-agent（宿主与 Guest 通信的守护进程）
# - 打包为 qcow2（zstd 压缩，参考 tenbox）
#
# 用法：
#   bash scripts/build-rootfs.sh
#
# 产物：
#   assets/rootfs/rootfs.qcow2      Guest 根文件系统（qcow2）
# =============================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
ASSETS="$REPO_ROOT/assets"
ROOTFS_DIR="$ASSETS/rootfs"
TMP_DIR="$(mktemp -d)"

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
# 依赖检查
# =============================================================
for cmd in wget tar qemu-img; do
    command -v "$cmd" &>/dev/null || die "Missing: $cmd"
done

# qemu-nbd 用于挂载 qcow2（可选，有则用）
HAS_NBD=false
command -v qemu-nbd &>/dev/null && HAS_NBD=true

# =============================================================
# 1. 下载 Alpine Linux miniroot
# =============================================================
ALPINE_VERSION="${ALPINE_VERSION:-3.20.3}"
ALPINE_ARCH="x86_64"
ALPINE_MIRROR="${ALPINE_MIRROR:-https://dl-cdn.alpinelinux.org/alpine}"
ALPINE_URL="$ALPINE_MIRROR/v${ALPINE_VERSION%.*}/releases/$ALPINE_ARCH"
MINIROOTFS="alpine-minirootfs-${ALPINE_VERSION}-${ALPINE_ARCH}.tar.gz"

log "Downloading Alpine $ALPINE_VERSION miniroot..."
wget -q --show-progress -O "$TMP_DIR/$MINIROOTFS" \
    "$ALPINE_URL/$MINIROOTFS"

# =============================================================
# 2. 解压到工作目录
# =============================================================
ROOTFS_WORK="$TMP_DIR/rootfs"
mkdir -p "$ROOTFS_WORK"
tar xzf "$TMP_DIR/$MINIROOTFS" -C "$ROOTFS_WORK"
log "Alpine extracted to $ROOTFS_WORK"

# =============================================================
# 3. chroot 内安装软件
#    （WSL2 支持 chroot x86_64，无需 qemu-user-static）
# =============================================================
log "Installing packages in chroot..."

# 复制 DNS 配置
cp /etc/resolv.conf "$ROOTFS_WORK/etc/resolv.conf" 2>/dev/null || true

# 挂载必要的伪文件系统
mount --bind /proc "$ROOTFS_WORK/proc" 2>/dev/null || true
mount --bind /sys  "$ROOTFS_WORK/sys"  2>/dev/null || true
mount --bind /dev  "$ROOTFS_WORK/dev"  2>/dev/null || true

umount_all() {
    umount "$ROOTFS_WORK/proc" 2>/dev/null || true
    umount "$ROOTFS_WORK/sys"  2>/dev/null || true
    umount "$ROOTFS_WORK/dev"  2>/dev/null || true
}
trap "umount_all; rm -rf $TMP_DIR" EXIT

chroot "$ROOTFS_WORK" /bin/sh << 'CHROOT_EOF'
set -e

# Alpine 包管理器
apk update
apk add --no-cache \
    openrc \
    busybox-initscripts \
    python3 \
    py3-pip \
    curl \
    wget \
    ca-certificates \
    bash \
    jq \
    openssh-client \
    procps \
    util-linux \
    e2fsprogs

# yt-dlp
pip3 install --break-system-packages yt-dlp 2>/dev/null || \
    pip3 install yt-dlp

# 时区
apk add --no-cache tzdata
cp /usr/share/zoneinfo/Asia/Shanghai /etc/localtime
echo "Asia/Shanghai" > /etc/timezone

# 网络配置（VirtIO NIC = eth0）
cat > /etc/network/interfaces << 'NET_EOF'
auto lo
iface lo inet loopback

auto eth0
iface eth0 inet dhcp
NET_EOF

# 开机自启网络
rc-update add networking default 2>/dev/null || true

# 默认 hostname
echo "vsb-agent" > /etc/hostname

# 清理
apk cache clean 2>/dev/null || true
rm -rf /tmp/* /var/cache/apk/*
CHROOT_EOF

umount_all
trap "rm -rf $TMP_DIR" EXIT

log "Packages installed"

# =============================================================
# 4. 写入 vsb-agent 守护进程
#    参考 tenbox guest_agent：监听 VirtIO Serial，执行宿主命令
# =============================================================
log "Installing vsb-agent..."

mkdir -p "$ROOTFS_WORK/usr/local/bin"
mkdir -p "$ROOTFS_WORK/etc/init.d"

# vsb-agent Python 脚本（轻量级，兼容 QEMU Guest Agent 协议）
cat > "$ROOTFS_WORK/usr/local/bin/vsb-agent" << 'AGENT_EOF'
#!/usr/bin/env python3
"""
vsb-agent: VirtIO Serial 守护进程，实现精简版 QEMU Guest Agent 协议
宿主通过 VirtIO Serial（/dev/hvc0 或 /dev/vport0p1）发送 JSON 命令

协议（参考 tenbox guest_agent 和 QEMU GA）：
- 宿主先发 0xFF 字节重置缓冲区
- 宿主发 {"execute": "guest-sync-delimited", "arguments": {"id": <N>}}
- 我们回复 0xFF + {"return": <N>}
- 后续命令：{"execute": "...", "arguments": {...}}
- 我们回复：{"return": ...} 或 {"error": ...}
"""

import os, sys, json, subprocess, threading, time, glob

SERIAL_PATHS = ["/dev/hvc0", "/dev/vport0p1", "/dev/virtio-ports/vsb.agent"]
LOG_FILE = "/var/log/vsb-agent.log"

def find_serial():
    for p in SERIAL_PATHS:
        if os.path.exists(p):
            return p
    return None

def log(msg):
    ts = time.strftime("%Y-%m-%d %H:%M:%S")
    line = f"[{ts}] {msg}\n"
    sys.stderr.write(line)
    try:
        with open(LOG_FILE, "a") as f:
            f.write(line)
    except Exception:
        pass

def handle_command(cmd):
    execute = cmd.get("execute", "")
    args = cmd.get("arguments", {})

    if execute == "guest-sync-delimited":
        return {"return": args.get("id", 0)}

    elif execute == "guest-ping":
        return {"return": {}}

    elif execute == "guest-info":
        return {"return": {"version": "vsb-agent-0.1", "supported_commands": []}}

    elif execute == "guest-exec":
        # 执行命令，返回 pid
        path = args.get("path", "")
        argv = args.get("arg", [])
        env  = args.get("env", [])
        capture = args.get("capture-output", False)
        try:
            full_cmd = [path] + argv
            env_dict = os.environ.copy()
            for e in env:
                k, _, v = e.partition("=")
                env_dict[k] = v
            proc = subprocess.Popen(
                full_cmd,
                stdout=subprocess.PIPE if capture else None,
                stderr=subprocess.PIPE if capture else None,
                env=env_dict,
            )
            return {"return": {"pid": proc.pid}}
        except Exception as e:
            return {"error": {"class": "GenericError", "desc": str(e)}}

    elif execute == "guest-exec-status":
        pid = args.get("pid", -1)
        try:
            os.kill(pid, 0)
            return {"return": {"exited": False}}
        except ProcessLookupError:
            return {"return": {"exited": True, "exitcode": 0}}
        except Exception as e:
            return {"error": {"class": "GenericError", "desc": str(e)}}

    elif execute == "guest-shutdown":
        mode = args.get("mode", "powerdown")
        log(f"Shutdown requested: {mode}")
        threading.Timer(1.0, lambda: os.system("poweroff")).start()
        return {"return": {}}

    elif execute == "guest-file-open":
        path = args.get("path", "")
        mode = args.get("mode", "r")
        try:
            fd = open(path, mode + "b")
            return {"return": id(fd)}
        except Exception as e:
            return {"error": {"class": "GenericError", "desc": str(e)}}

    elif execute == "vsb-run-summary":
        # 自定义命令：运行 yt-dlp + 总结
        url   = args.get("url", "")
        level = args.get("level", "standard")
        lang  = args.get("language", "zh-CN")
        log(f"vsb-run-summary: url={url} level={level}")
        try:
            result = subprocess.run(
                ["yt-dlp", "--write-auto-sub", "--sub-lang", "zh-Hans,en",
                 "--sub-format", "vtt", "--skip-download", "-o", "/tmp/sub.%(ext)s",
                 "--no-update", "--quiet", url],
                capture_output=True, text=True, timeout=120
            )
            subtitle = ""
            for vtt in glob.glob("/tmp/sub*.vtt"):
                with open(vtt) as f:
                    subtitle = f.read()
                os.unlink(vtt)
                break
            return {"return": {"subtitle": subtitle, "url": url}}
        except Exception as e:
            return {"error": {"class": "GenericError", "desc": str(e)}}

    return {"error": {"class": "CommandNotFound", "desc": f"unknown: {execute}"}}


def main():
    log("vsb-agent starting...")

    serial = None
    for _ in range(30):
        serial = find_serial()
        if serial:
            break
        time.sleep(1)

    if not serial:
        log("ERROR: no VirtIO serial device found")
        sys.exit(1)

    log(f"Using serial: {serial}")

    buf = b""
    with open(serial, "r+b", buffering=0) as port:
        log("Listening for commands...")
        while True:
            try:
                chunk = port.read(4096)
                if not chunk:
                    time.sleep(0.01)
                    continue

                # 0xFF = 重置缓冲区（tenbox 握手协议）
                if b'\xff' in chunk:
                    idx = chunk.rfind(b'\xff')
                    buf = chunk[idx+1:]
                else:
                    buf += chunk

                # 尝试解析完整 JSON 行
                while b'\n' in buf:
                    line, buf = buf.split(b'\n', 1)
                    line = line.strip()
                    if not line:
                        continue
                    try:
                        cmd = json.loads(line.decode('utf-8', errors='replace'))
                        log(f"CMD: {cmd.get('execute', '?')}")
                        resp = handle_command(cmd)
                        data = json.dumps(resp).encode() + b'\n'
                        port.write(data)
                        port.flush()
                    except json.JSONDecodeError:
                        pass

            except Exception as e:
                log(f"Error: {e}")
                time.sleep(1)


if __name__ == "__main__":
    main()
AGENT_EOF
chmod +x "$ROOTFS_WORK/usr/local/bin/vsb-agent"

# OpenRC 服务
cat > "$ROOTFS_WORK/etc/init.d/vsb-agent" << 'SVC_EOF'
#!/sbin/openrc-run
name="vsb-agent"
description="VSB Guest Agent"
command="/usr/local/bin/vsb-agent"
command_background=true
pidfile="/run/vsb-agent.pid"
output_log="/var/log/vsb-agent.log"
error_log="/var/log/vsb-agent.log"

depend() {
    after networking
}
SVC_EOF
chmod +x "$ROOTFS_WORK/etc/init.d/vsb-agent"

# 开机自启
chroot "$ROOTFS_WORK" rc-update add vsb-agent default 2>/dev/null || true

log "vsb-agent installed"

# =============================================================
# 5. 配置 LLM 代理（宿主 10.0.2.3:80）
# =============================================================
cat > "$ROOTFS_WORK/etc/profile.d/vsb.sh" << 'ENV_EOF'
# vsb: 通过宿主代理访问 LLM API
export ANTHROPIC_BASE_URL="http://10.0.2.3:8080"
export ANTHROPIC_API_KEY="vsb-proxied"
ENV_EOF

log "LLM proxy env configured (→ 10.0.2.3:8080)"

# =============================================================
# 6. 打包为 qcow2
# =============================================================
QCOW2="$ROOTFS_DIR/rootfs.qcow2"
RAW_IMG="$TMP_DIR/rootfs.raw"
ROOTFS_SIZE="${ROOTFS_SIZE:-4G}"

log "Creating $ROOTFS_SIZE raw image..."
dd if=/dev/zero of="$RAW_IMG" bs=1M count=0 seek=4096 2>/dev/null
mkfs.ext4 -q -F -L vsb-rootfs "$RAW_IMG"

# 挂载并复制
MOUNT_POINT="$TMP_DIR/mnt"
mkdir -p "$MOUNT_POINT"
mount -o loop "$RAW_IMG" "$MOUNT_POINT"

log "Copying rootfs to image..."
cp -a "$ROOTFS_WORK/." "$MOUNT_POINT/"
sync
umount "$MOUNT_POINT"

log "Converting to qcow2..."
mkdir -p "$ROOTFS_DIR"
qemu-img convert -f raw -O qcow2 -c "$RAW_IMG" "$QCOW2"

QCOW2_SIZE=$(du -sh "$QCOW2" | cut -f1)
log "Saved: $QCOW2 ($QCOW2_SIZE)"

# =============================================================
# 7. 输出摘要
# =============================================================
echo ""
echo "======================================================"
log "rootfs build complete!"
echo ""
echo "  rootfs : $QCOW2"
echo "  size   : $QCOW2_SIZE"
echo ""
echo "Contents:"
echo "  - Alpine Linux $ALPINE_VERSION"
echo "  - Python3 + yt-dlp"
echo "  - vsb-agent (VirtIO Serial daemon)"
echo "  - LLM proxy → 10.0.2.3:8080"
echo "======================================================"
echo ""
echo "Next step: cargo build --features whvp && vsb vm start"
