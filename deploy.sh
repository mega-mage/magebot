#!/bin/bash
set -e

ACTION="${1:-install}"

# ── 1. 增量更新与升级分支 (magebot update) ─────────────────
if [ "$ACTION" = "update" ] || [ "$ACTION" = "--update" ] || [ "$ACTION" = "-u" ]; then
    echo "====== Magebot 快捷升级与代码更新 ======"

    # 1.1 尝试停止现有守护进程
    if command -v magebot &> /dev/null; then
        echo "正在停止后台运行中的 Magebot 守护进程..."
        magebot stop || true
    fi

    # 1.2 拉取 Git 最新代码
    if [ -d .git ]; then
        echo "正在从 Git 仓库拉取最新源码..."
        git pull || echo "⚠️ git pull 提示异常或本地有未提交修改，继续使用当前源码编译..."
    fi

    # 1.3 更新 yt-dlp 核心
    mkdir -p dependency
    OS_NAME=$(uname -s | tr '[:upper:]' '[:lower:]')
    ARCH_NAME=$(uname -m)
    YTDLP_URL="https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp"

    if [ "$OS_NAME" = "linux" ]; then
        if [ "$ARCH_NAME" = "aarch64" ] || [ "$ARCH_NAME" = "arm64" ]; then
            YTDLP_URL="https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux_aarch64"
        elif [[ "$ARCH_NAME" =~ armv7 ]]; then
            YTDLP_URL="https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux_armv7l"
        fi
    elif [ "$OS_NAME" = "darwin" ]; then
        YTDLP_URL="https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos"
    fi

    echo "正在更新 yt-dlp 核心引擎..."
    curl -L "$YTDLP_URL" -o dependency/yt-dlp
    chmod +x dependency/yt-dlp

    # 1.4 重新编译 Release 产物
    echo "正在重新编译 Magebot (Release 模式)..."
    cargo build --release

    # 1.5 替换全局二进制文件
    echo "正在更新系统全局二进制文件 /usr/local/bin/magebot..."
    sudo cp target/release/magebot /usr/local/bin/magebot
    sudo chmod +x /usr/local/bin/magebot

    echo "========================================="
    echo "🎉 Magebot 升级完成 (v0.1.1)！"
    echo "请使用以下命令重启守护进程与打开看板："
    echo "  1. 启动服务: magebot start"
    echo "  2. 打开 TUI 控制台: magebot monitor"
    echo "========================================="
    exit 0
fi

# ── 2. 全量部署安装分支 (Default Install) ──────────────────
echo "====== Magebot 一键部署脚本 (Linux) ======"

# 2.1 检测并安装系统基本依赖 (Debian/Ubuntu/CentOS 系列)
if [ -f /etc/debian_version ]; then
    echo "检测到 Debian/Ubuntu 系列系统，正在安装系统依赖项 (ffmpeg, curl, build-essential, openssl, sqlite3, nodejs)..."
    sudo apt-get update -y
    sudo apt-get install -y ffmpeg curl build-essential pkg-config libssl-dev sqlite3 libsqlite3-dev nodejs
elif [ -f /etc/redhat-release ]; then
    echo "检测到 RHEL/CentOS 系列系统，正在安装系统依赖项 (ffmpeg, curl, openssl, sqlite, nodejs)..."
    sudo dnf install -y epel-release || sudo yum install -y epel-release
    sudo dnf install -y ffmpeg curl make gcc openssl-devel sqlite-devel nodejs || sudo yum install -y ffmpeg curl make gcc openssl-devel sqlite-devel nodejs
else
    echo "⚠️ 未检测到兼容的包管理器，请确保手动安装了 ffmpeg、curl、openssl-dev、sqlite3 以及 nodejs 依赖。"
fi

# 2.2 检查或安装 Rust/Cargo 编译环境
if ! command -v cargo &> /dev/null; then
    echo "未检测到 Rust 编译环境，正在在线安装 Rust 工具链..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
else
    echo "✅ 检测到 Rust 环境已存在。"
fi

# 2.3 创建工作目录
echo "正在创建工作文件夹..."
mkdir -p ~/.magebot/downloads
mkdir -p dependency

# 2.4 检测系统平台架构并下载对应的 yt-dlp 核心
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

# 2.5 将 yt-dlp 注册到系统全局路径 (/usr/local/bin/yt-dlp)
if [ -f /usr/local/bin/yt-dlp ]; then
    echo "ℹ️  系统全局路径中已存在 yt-dlp (/usr/local/bin/yt-dlp)。"
    read -p "是否覆盖已有的全局 yt-dlp 文件？[y/N]: " overwrite_ytdlp
    if [[ "$overwrite_ytdlp" =~ ^[Yy]$ ]]; then
        echo "正在覆盖安装 yt-dlp 到 /usr/local/bin..."
        sudo cp dependency/yt-dlp /usr/local/bin/yt-dlp
        sudo chmod +x /usr/local/bin/yt-dlp
        echo "✅ 全局 yt-dlp 更新完成。"
    else
        echo "ℹ️  跳过覆盖，保持原有的全局 yt-dlp 文件不变。"
    fi
else
    echo "正在安装 yt-dlp 到系统全局路径 /usr/local/bin/yt-dlp..."
    sudo cp dependency/yt-dlp /usr/local/bin/yt-dlp
    sudo chmod +x /usr/local/bin/yt-dlp
    echo "✅ 全局 yt-dlp 安装完成。"
fi

# 2.6 编译项目 (Release 模式)
echo "正在编译 Magebot (Release 模式)..."
cargo build --release

# 2.7 将编译产物安装到系统全局路径
echo "正在安装 magebot 二进制文件到 /usr/local/bin..."
sudo cp target/release/magebot /usr/local/bin/magebot
sudo chmod +x /usr/local/bin/magebot

echo "========================================="
echo "🎉 Magebot 一键部署完成 (v0.1.1)！"
echo "请参考以下指令开启同步服务："
echo "  1. 登录 Telegram 授权: magebot login"
> echo "  2. 在后台启动守护进程: magebot start"
echo "  3. 打开纯鼠标 TUI 监控看板: magebot monitor"
echo "  4. 后续快速升级代码: ./deploy.sh update"
echo "========================================="
