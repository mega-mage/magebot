#!/bin/bash
set -e

echo "====== Magebot 一键部署脚本 (Linux) ======"

# 1. 检测并安装系统基本依赖 (Debian/Ubuntu/CentOS 系列)
if [ -f /etc/debian_version ]; then
    echo "检测到 Debian/Ubuntu 系列系统，正在安装系统依赖项 (ffmpeg, curl, build-essential, openssl, sqlite3)..."
    sudo apt-get update -y
    sudo apt-get install -y ffmpeg curl build-essential pkg-config libssl-dev sqlite3 libsqlite3-dev
elif [ -f /etc/redhat-release ]; then
    echo "检测到 RHEL/CentOS 系列系统，正在安装系统依赖项 (ffmpeg, curl, openssl, sqlite)..."
    # CentOS 需要 EPEL 源来获取 ffmpeg
    sudo dnf install -y epel-release || sudo yum install -y epel-release
    sudo dnf install -y ffmpeg curl make gcc openssl-devel sqlite-devel || sudo yum install -y ffmpeg curl make gcc openssl-devel sqlite-devel
else
    echo "⚠️ 未检测到兼容的包管理器，请确保手动安装了 ffmpeg、curl、openssl-dev 以及 sqlite3 依赖库。"
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

# 4. 下载最新版 Linux-x86_64 版 yt-dlp
echo "正在下载最新版 Linux yt-dlp 核心..."
curl -L https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp -o dependency/yt-dlp
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
