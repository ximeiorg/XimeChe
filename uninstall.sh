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
rm -f ~/.local/share/dbus-1/services/org.xime.IM.service 2>/dev/null || true

# 4. 移除 Desktop 文件
echo "移除 Desktop 文件..."
rm -f ~/.local/share/applications/xime-launcher.desktop

# 5. 清理日志文件
echo "清理日志文件..."
rm -rf ~/.local/share/sddm/xime*.log 2>/dev/null || true
find ~/.config/xime/rime -name "*.log" -type f -delete 2>/dev/null || true
# 清理 Rime 构建缓存日志
rm -f ~/.config/xime/rime/build/*.log 2>/dev/null || true

# 6. 重置 KWin 虚拟键盘配置（可选）
read -p "是否重置 KWin 虚拟键盘配置? [y/N] " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo "重置 KWin 配置..."
    kwriteconfig5 --file kwinrc --group Wayland --key VirtualKeyboard ""
    kwriteconfig5 --file kwinrc --group Wayland --key InputMethod ""
    kwriteconfig5 --file kwinrc --group Wayland --key VirtualKeyboardEnabled "" 2>/dev/null || true
    echo "KWin 配置已重置，重新登录后生效"
fi

# 7. 清理配置文件（可选）
read -p "是否删除配置文件和词典 (~/.config/xime)? [y/N] " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo "删除配置文件..."
    rm -rf ~/.config/xime
    echo "配置文件已删除"
fi

echo "=== 卸载完成 ==="
echo ""
echo "重新安装步骤:"
echo "  ./dev-install.sh"
