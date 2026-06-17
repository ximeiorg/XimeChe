#!/bin/bash
# Install script for xime-wayland

set -e

PREFIX="${PREFIX:-/usr}"
BINDIR="${PREFIX}/bin"
DATADIR="${PREFIX}/share"

# Get project root
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "Installing xime-wayland to ${PREFIX}"

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

# Install default config file
install -Dm644 "${PROJECT_ROOT}/resources/xime.yaml" "${DATADIR}/xime/xime.yaml"

# Install rime-wubi schemas to shared directory
if [ -d "${PROJECT_ROOT}/rime-wubi" ]; then
    install -d "${DATADIR}/xime/rime-data"
    for file in "${PROJECT_ROOT}/rime-wubi"/*.schema.yaml "${PROJECT_ROOT}/rime-wubi"/*.dict.yaml "${PROJECT_ROOT}/rime-wubi"/*.lua; do
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
    install -m644 "${PROJECT_ROOT}/rime-wubi"/default.custom.yaml "${DATADIR}/xime/rime-data/" 2>/dev/null || true
    install -m644 "${PROJECT_ROOT}/rime-wubi"/xime.custom.yaml "${DATADIR}/xime/rime-data/" 2>/dev/null || true
    echo "Installed rime-wubi schemas to ${DATADIR}/xime/rime-data"
fi

echo "Installation complete!"
echo ""
echo "To use with KDE Plasma:"
echo "1. Configure in System Settings > Virtual Keyboard"
echo "   OR run: kwriteconfig5 --file kwinrc --group Wayland --key VirtualKeyboard xime-launcher.desktop"
echo "2. Restart KDE Plasma (logout/login)"
echo ""