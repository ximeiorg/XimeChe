#!/bin/bash
# Development install script for local testing

set -e

BINDIR="${HOME}/.local/bin"
DATADIR="${HOME}/.local/share"

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "Installing xime-wayland for development to ${HOME}/.local"

# Build release
cargo build --release -p xime-daemon -p xime-launcher

# Install binaries
install -Dm755 "${PROJECT_ROOT}/target/release/xime-daemon" "${BINDIR}/xime-daemon"
install -Dm755 "${PROJECT_ROOT}/target/release/xime-launcher" "${BINDIR}/xime-launcher"

# Generate and install DBus service file
install -Dm644 "${PROJECT_ROOT}/resources/dbus/org.xime.Xime.service.in" \
    "${DATADIR}/dbus-1/services/org.xime.Xime.service"
sed -i "s|@BINDIR@|${BINDIR}|g" "${DATADIR}/dbus-1/services/org.xime.Xime.service"

# Generate and install desktop files
install -Dm644 "${PROJECT_ROOT}/resources/applications/xime-launcher.desktop.in" \
    "${DATADIR}/applications/xime-launcher.desktop"
sed -i "s|@BINDIR@|${BINDIR}|g" "${DATADIR}/applications/xime-launcher.desktop"

# Update desktop database
update-desktop-database "${DATADIR}/applications" 2>/dev/null || true

# Configure KWin
kwriteconfig5 --file kwinrc --group Wayland --key VirtualKeyboard xime-launcher.desktop
kwriteconfig5 --file kwinrc --group Wayland --key VirtualKeyboardEnabled true

echo ""
echo "Development installation complete!"
echo "KWin configured to use xime-launcher.desktop"
echo ""
echo "To test:"
echo "1. Restart KDE Plasma (logout/login) OR restart KWin"
echo "2. Open Kate and type to trigger VirtualKeyboard"
echo ""
echo "To manually test daemon:"
echo "  ${BINDIR}/xime-daemon"
echo "  qdbus org.xime.Xime"