#!/bin/bash
set -e

echo "====== Magebot 一键部署脚本 (Linux) ======"

# 1. 检测并安装系统基本依赖 (Debian/Ubuntu/CentOS 系列)
if [ -f /etc/debian_version ]; then
    echo "检测到 Debian/Ubuntu 系列系统，正在安装系统依赖项 (ffmpeg, curl, build-essential, openssl, sqlite3, nodejs)..."
    sudo apt-get update -y
    sudo apt-get install -y ffmpeg curl build-essential pkg-config libssl-dev sqlite3 libsqlite3-dev nodejs
elif [ -f /etc/redhat-release ]; then
    echo "检测到 RHEL/CentOS 系列系统，正在安装系统依赖项 (ffmpeg, curl, openssl, sqlite, nodejs)..."
    # CentOS 需要 EPEL 源来获取 ffmpeg
    sudo dnf install -y epel-release || sudo yum install -y epel-release
    sudo dnf install -y ffmpeg curl make gcc openssl-devel sqlite-devel nodejs || sudo yum install -y ffmpeg curl make gcc openssl-devel sqlite-devel nodejs
else
    echo "⚠️ 未检测到兼容的包管理器，请确保手动安装了 ffmpeg、curl、openssl-dev、sqlite3 以及 nodejs 依赖。"
fi

# 2. 检查或安装 Rust/Cargo 编译环境
if ! command -v cargo &> /dev/null; then
    echo "未检测到 Rust 编译环境，正在在线安装 Rust 工具链..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
else
    echo "✅ 检测到 Rust 环境已存在。"
fi

# 3. 创建工作与运行依赖目录
echo "正在创建工作文件夹..."
mkdir -p dependency
mkdir -p ~/.magebot/downloads

# 4. 检测系统平台架构并下载对应的 yt-dlp 核心
OS_NAME=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH_NAME=$(uname -m)

echo "检测到操作系统: $OS_NAME, 硬件架构: $ARCH_NAME"

YTDLP_URL="https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp"

if [ "$OS_NAME" = "linux" ]; then
    if [ "$ARCH_NAME" = "x86_64" ]; then
        YTDLP_URL="https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp"
    elif [ "$ARCH_NAME" = "aarch64" ] || [ "$ARCH_NAME" = "arm64" ]; then
        YTDLP_URL="https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux_aarch64"
    elif [[ "$ARCH_NAME" =~ armv7 ]]; then
        YTDLP_URL="https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux_armv7l"
    else
        echo "⚠️ 尚未适配的 Linux 架构 ($ARCH_NAME)，默认下载标准 x86_64 版本..."
    fi
elif [ "$OS_NAME" = "darwin" ]; then
    YTDLP_URL="https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos"
else
    echo "⚠️ 未知的操作系统平台 ($OS_NAME)，将默认下载标准 Linux 版本..."
fi

echo "正在从 $YTDLP_URL 下载适合当前架构的 yt-dlp 核心..."
curl -L "$YTDLP_URL" -o dependency/yt-dlp
chmod +x dependency/yt-dlp
echo "✅ yt-dlp 准备完毕。"

# 5. 编译编译项目 (Release 模式)
echo "正在编译 Magebot (Release 模式)..."
cargo build --release

# 6. 将编译产物安装到系统全局路径以便直接调用
echo "正在安装 magebot 二进制文件到 /usr/local/bin..."
sudo cp target/release/magebot /usr/local/bin/magebot
sudo chmod +x /usr/local/bin/magebot

echo "========================================="
echo "🎉 Magebot 一键部署完成！"
echo "请参考以下指令开启同步服务："
echo "  1. 设定配置项: magebot set api_id <id> / api_hash <hash> / phone_number <+86...>"
   echo "     (自动生成并配置 ~/.magebot/config.toml)"
echo "  2. 登录 Telegram 授权: magebot login"
echo "  3. 在后台启动守护进程: magebot start"
echo "  4. 打开实时 TUI 监控看板: magebot monitor"
echo "========================================="
