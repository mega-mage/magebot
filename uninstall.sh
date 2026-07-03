#!/bin/bash
set -e

echo "====== Magebot 一键卸载脚本 (Linux) ======"

# 1. 停止运行中的守护进程
if command -v magebot &> /dev/null; then
    echo "正在尝试安全停止 Magebot 后台服务..."
    magebot stop || true
    sleep 1
fi

# 再次通过进程名确认，防止残留
if pgrep -x "magebot" > /dev/null; then
    echo "正在强制终止残留的 Magebot 进程..."
    sudo pkill -9 -x "magebot" || true
fi

# 2. 删除系统全局路径下的二进制文件
if [ -f /usr/local/bin/magebot ]; then
    echo "正在移除全局可执行文件 /usr/local/bin/magebot..."
    sudo rm -f /usr/local/bin/magebot
    echo "✅ 全局可执行文件已移除。"
else
    echo "ℹ️  未在 /usr/local/bin/magebot 发现安装文件。"
fi

# 3. 询问是否清理本地数据文件夹
echo ""
read -p "是否删除所有用户数据（包括登录会话、config.toml配置、本地日志与下载缓存）？[y/N]: " confirm
if [[ "$confirm" =~ ^[Yy]$ ]]; then
    if [ -d "$HOME/.magebot" ]; then
        echo "正在删除数据文件夹 $HOME/.magebot ..."
        rm -rf "$HOME/.magebot"
        echo "✅ 用户数据文件夹已删除。"
    else
        echo "ℹ️  未发现 $HOME/.magebot 文件夹。"
    fi
else
    echo "ℹ️  已保留用户配置与会话数据 ($HOME/.magebot)。"
fi

echo "========================================="
echo "🎉 Magebot 卸载完成！"
echo "========================================="
