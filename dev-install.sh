#!/bin/bash
# Development install script for local testing

set -e

BINDIR="${HOME}/.local/bin"
DATADIR="${HOME}/.local/share"

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIBXIMECORE="${PROJECT_ROOT}/../libximecore"

echo "Installing xime-wayland for development to ${HOME}/.local"

# Ensure .cargo/config.toml exists with local libximecore patch
CARGO_CONF="${PROJECT_ROOT}/.cargo/config.toml"
if [ ! -f "${CARGO_CONF}" ]; then
    mkdir -p "${PROJECT_ROOT}/.cargo"
    cat > "${CARGO_CONF}" << 'EOF'
# Local development override — use local libximecore checkout.
# Remove this file to use git dependencies (CI, release builds).
[patch."https://github.com/ximeiorg/libximecore"]
librime = { path = "../libximecore/crates/librime" }
xime-config = { path = "../libximecore/crates/xime-config" }
xime-setup-lib = { path = "../libximecore/crates/xime-setup" }
EOF
    echo "Created ${CARGO_CONF} with local libximecore patch"
fi

# Stop existing xime processes to refresh
echo "Stopping existing xime processes..."
pkill -9 xime-daemon 2>/dev/null || true
pkill -9 xime-launcher 2>/dev/null || true
sleep 1

# Initialize git submodules (rime-wubi for data, libximecore for librime)
if [ -f "${PROJECT_ROOT}/.gitmodules" ]; then
    git submodule update --init --recursive
fi
if [ -f "${LIBXIMECORE}/.gitmodules" ]; then
    git -C "${LIBXIMECORE}" submodule update --init --recursive
fi

cargo clippy --all-targets --all-features -- -D warnings
# Build release (librime build is handled by librime-sys2/build.rs)
cargo build --release -p xime-daemon -p xime-launcher -p xime-setup

# Install binaries
install -Dm755 "${PROJECT_ROOT}/target/release/xime-daemon" "${BINDIR}/xime-daemon"
install -Dm755 "${PROJECT_ROOT}/target/release/xime-launcher" "${BINDIR}/xime-launcher"
install -Dm755 "${PROJECT_ROOT}/target/release/xime-setup" "${BINDIR}/xime-setup"

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

# Install default config file (if not exists)
CONFIGDIR="${HOME}/.config/xime"
RIMEDIR="${CONFIGDIR}/rime"

if [ ! -f "${CONFIGDIR}/xime.custom.yaml" ]; then
    install -Dm644 "${PROJECT_ROOT}/resources/xime.yaml" "${CONFIGDIR}/xime.custom.yaml"
    echo "Installed default config to ${CONFIGDIR}/xime.custom.yaml"
fi

# Install rime-wubi schemas to shared data directory (only rime-wubi schemas,
# not system librime-data builtin schemas)
SHAREDRIMEDIR="${DATADIR}/xime/rime-data"
if [ -d "${PROJECT_ROOT}/rime-wubi" ]; then
    mkdir -p "${SHAREDRIMEDIR}"
    cp -r "${PROJECT_ROOT}/rime-wubi"/*.yaml "${SHAREDRIMEDIR}/" 2>/dev/null || true
    # Copy lua/ subdirectory (uuid, date_translator, etc.)
    if [ -d "${PROJECT_ROOT}/rime-wubi/lua" ]; then
        cp -r "${PROJECT_ROOT}/rime-wubi/lua" "${SHAREDRIMEDIR}/"
        echo "Installed lua scripts to ${SHAREDRIMEDIR}/lua"
    fi
    echo "Installed rime-wubi schemas to ${SHAREDRIMEDIR}"
fi

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
echo "If librime was built from the submodule, you may need:"
echo "  export LD_LIBRARY_PATH=\"${LIBXIMECORE}/librime/dist/lib:\${LD_LIBRARY_PATH:-}\""
echo "Or install librime system-wide: sudo make install -C ${LIBXIMECORE}/librime/build"
