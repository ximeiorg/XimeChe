#!/bin/bash
# XIME Wayland 卸载脚本

set -e

echo "=== XIME Wayland 卸载 ==="

# 1. 停止所有 xime 进程
echo "停止 xime 进程..."
pkill -9 xime-daemon 2>/dev/null || true
pkill -9 xime-launcher 2>/dev/null || true
sleep 1

# 2. 移除二进制文件
echo "移除二进制文件..."
rm -f ~/.local/bin/xime-daemon
rm -f ~/.local/bin/xime-launcher
rm -f ~/.local/bin/xime-core 2>/dev/null || true

# 3. 移除 DBus 服务文件
echo "移除 DBus 服务..."
rm -f ~/.local/share/dbus-1/services/org.xime.Xime.service

# 4. 移除 Desktop 文件
echo "移除 Desktop 文件..."
rm -f ~/.local/share/applications/xime-launcher.desktop

# 5. 重置 KWin 虚拟键盘配置（可选）
read -p "是否重置 KWin 虚拟键盘配置? [y/N] " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo "重置 KWin 配置..."
    kwriteconfig5 --file kwinrc --group Wayland --key VirtualKeyboard ""
    kwriteconfig5 --file kwinrc --group Wayland --key InputMethod ""
    echo "KWin 配置已重置，重新登录后生效"
fi

echo "=== 卸载完成 ==="
echo ""
echo "剩余文件（如需完全清理请手动删除）:"
echo "  ~/.config/xime/ - 配置文件和词典"
echo ""
echo "重新安装步骤:"
echo "  ./dev-install.sh"
