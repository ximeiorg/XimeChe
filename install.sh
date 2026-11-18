#!/bin/bash
# Install script for xime-wayland

set -e

PREFIX="${PREFIX:-/usr}"
BINDIR="${PREFIX}/bin"
DATADIR="${PREFIX}/share"
SYSCONFDIR="${SYSCONFDIR:-/etc}"

# Get project root
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "Installing xime-wayland to ${PREFIX}"

# 编译产物目录同样不能假设在 <repo>/target：~/.cargo/config.toml 可能设了
# 全局 build.target-dir（如 /home/kkch/.cargo/target），问 cargo 要真实路径。
TARGET_DIR="$(cargo metadata --format-version 1 --no-deps \
    --manifest-path "${PROJECT_ROOT}/Cargo.toml" 2>/dev/null \
    | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"
[ -n "${TARGET_DIR}" ] || TARGET_DIR="${PROJECT_ROOT}/target"

# Install binaries
install -Dm755 "${TARGET_DIR}/release/xime-daemon" "${BINDIR}/xime-daemon"
install -Dm755 "${TARGET_DIR}/release/xime-launcher" "${BINDIR}/xime-launcher"

# Generate and install DBus service file
install -Dm644 "${PROJECT_ROOT}/resources/dbus/org.xime.Xime.service.in" \
    "${DATADIR}/dbus-1/services/org.xime.Xime.service"
sed -i "s|@BINDIR@|${BINDIR}|g" "${DATADIR}/dbus-1/services/org.xime.Xime.service"

# Generate and install desktop files
install -Dm644 "${PROJECT_ROOT}/resources/applications/xime-launcher.desktop.in" \
    "${DATADIR}/applications/xime-launcher.desktop"
sed -i "s|@BINDIR@|${BINDIR}|g" "${DATADIR}/applications/xime-launcher.desktop"

install -Dm644 "${PROJECT_ROOT}/resources/applications/xime-setup.desktop.in" \
    "${DATADIR}/applications/xime-setup.desktop"
sed -i "s|@BINDIR@|${BINDIR}|g" "${DATADIR}/applications/xime-setup.desktop"

# XDG autostart（系统级）：COSMIC / GNOME / sway 没有 KWin 的虚拟键盘拉起机制，
# 只能靠会话读 autostart 目录启动 daemon。条目带 NotShowIn=KDE，Plasma 下不生效，
# 因此无需按当前桌面条件安装（也避免 sudo 下 XDG_CURRENT_DESKTOP 被清掉时判错）。
install -Dm644 "${PROJECT_ROOT}/resources/applications/xime-daemon.desktop.in" \
    "${SYSCONFDIR}/xdg/autostart/xime-daemon.desktop"
sed -i "s|@BINDIR@|${BINDIR}|g" "${SYSCONFDIR}/xdg/autostart/xime-daemon.desktop"

# Update desktop database
update-desktop-database "${DATADIR}/applications" 2>/dev/null || true

# Install application icons
ICONDIR="${DATADIR}/icons/hicolor"
if [ -d "${PROJECT_ROOT}/resources/icons" ]; then
    for size in 48x48 64x64 128x128 256x256 512x512; do
        if [ -f "${PROJECT_ROOT}/resources/icons/${size}/apps/xime.png" ]; then
            install -Dm644 "${PROJECT_ROOT}/resources/icons/${size}/apps/xime.png" \
                "${ICONDIR}/${size}/apps/xime.png"
        fi
    done
    echo "Installed application icons to ${ICONDIR}"
    gtk-update-icon-cache -f -t "${ICONDIR}" 2>/dev/null || true
fi

# Install default config file
install -Dm644 "${PROJECT_ROOT}/resources/xime.yaml" "${DATADIR}/xime/xime.yaml"

# Install rime-wubi schemas to shared directory
if [ -d "${PROJECT_ROOT}/rime-wubi" ]; then
    install -d "${DATADIR}/xime/rime-data"
    for file in "${PROJECT_ROOT}/rime-wubi"/*.yaml; do
        [ -f "$file" ] && install -m644 "$file" "${DATADIR}/xime/rime-data/"
    done
    # Copy lua/ subdirectory (uuid, date_translator, etc.)
    if [ -d "${PROJECT_ROOT}/rime-wubi/lua" ]; then
        install -d "${DATADIR}/xime/rime-data/lua"
        for lua_file in "${PROJECT_ROOT}/rime-wubi/lua"/*.lua; do
            [ -f "$lua_file" ] && install -m644 "$lua_file" "${DATADIR}/xime/rime-data/lua/"
        done
        echo "Installed lua scripts to ${DATADIR}/xime/rime-data/lua"
    fi
    echo "Installed rime-wubi schemas to ${DATADIR}/xime/rime-data"
fi

echo "Installation complete!"
echo ""
echo "To use with KDE Plasma:"
echo "1. Configure in System Settings > Virtual Keyboard"
echo "   OR run: kwriteconfig6 --file kwinrc --group Wayland --key VirtualKeyboard xime-launcher.desktop"
echo "   （Plasma 5 用 kwriteconfig5，参数相同）"
echo "2. Restart KDE Plasma (logout/login)"
echo ""
echo "To use with other Wayland desktops (COSMIC / GNOME / sway):"
echo "   Nothing to configure — xime-daemon autostarts via ${SYSCONFDIR}/xdg/autostart."
echo "   Log out and back in. On COSMIC 1.9+, disable the on-screen keyboard first:"
echo "   cosmic-osk holds the only zwp_input_method_v2 slot, which starves any IME."
echo ""
echo "Voice input (optional):"
echo "   Needs the separate daemon 'xime-speechd' (Qwen3-ASR, from libximecore)."
if command -v xime-speechd >/dev/null 2>&1; then
    echo "   Found: $(command -v xime-speechd)"
else
    echo "   Not installed — voice input will report a readable error until you add it."
fi
echo "   Models live in ~/.config/xime/models/<model-id>/ (see Qwen3-ASR docs)."
echo "   Hold Ctrl+Alt+V to dictate; release to commit."
echo ""