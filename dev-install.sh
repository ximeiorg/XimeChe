#!/bin/bash
# Development install script for local testing

set -e

BINDIR="${HOME}/.local/bin"
DATADIR="${HOME}/.local/share"

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIBXIMECORE="${PROJECT_ROOT}/../libximecore"

# 编译产物目录：~/.cargo/config.toml 里可能设了全局 build.target-dir
# （本机就是 /home/kkch/.cargo/target），产物不在 <repo>/target 下。
# 一律问 cargo 要真实路径（cargo metadata 会带上配置解析结果），
# 问不到才退回默认的 <manifest 同级>/target。
cargo_target_dir() { # <manifest-path>
    local dir
    dir="$(cargo metadata --format-version 1 --no-deps --manifest-path "$1" 2>/dev/null \
        | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"
    [ -n "$dir" ] || dir="$(dirname "$1")/target"
    printf '%s\n' "$dir"
}

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

TARGET_DIR="$(cargo_target_dir "${PROJECT_ROOT}/Cargo.toml")"
LIBXIMECORE_TARGET_DIR="$(cargo_target_dir "${LIBXIMECORE}/Cargo.toml")"

# Build & install xime-sync-server (clipboard sync server, from libximecore)
if [ -f "${LIBXIMECORE}/Cargo.toml" ]; then
    cargo build --release --manifest-path "${LIBXIMECORE}/Cargo.toml" -p xime-sync-server
    install -Dm755 "${LIBXIMECORE_TARGET_DIR}/release/xime-sync-server" "${BINDIR}/xime-sync-server"
fi

# Install binaries
install -Dm755 "${TARGET_DIR}/release/xime-daemon" "${BINDIR}/xime-daemon"
install -Dm755 "${TARGET_DIR}/release/xime-launcher" "${BINDIR}/xime-launcher"
install -Dm755 "${TARGET_DIR}/release/xime-setup" "${BINDIR}/xime-setup"

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

if [ ! -f "${CONFIGDIR}/xime.custom.yaml" ]; then
    install -Dm644 "${PROJECT_ROOT}/resources/xime.yaml" "${CONFIGDIR}/xime.custom.yaml"
    echo "Installed default config to ${CONFIGDIR}/xime.custom.yaml"
fi

# Install rime-wubi schemas to read-only shared data dir (dev install).
# 用户数据目录 ~/.config/xime/rime 不落默认文件；librime 在用户目录无同名文件时自动回退到此处。
RIMEDATADIR="${HOME}/.local/share/xime/rime-data"
if [ -d "${PROJECT_ROOT}/rime-wubi" ]; then
    install -d "${RIMEDATADIR}"
    for file in "${PROJECT_ROOT}/rime-wubi"/*.yaml; do
        [ -f "$file" ] && install -m644 "$file" "${RIMEDATADIR}/"
    done
    # Copy lua/ subdirectory (uuid, date_translator, etc.)
    if [ -d "${PROJECT_ROOT}/rime-wubi/lua" ]; then
        install -d "${RIMEDATADIR}/lua"
        for lua_file in "${PROJECT_ROOT}/rime-wubi/lua"/*.lua; do
            [ -f "$lua_file" ] && install -m644 "$lua_file" "${RIMEDATADIR}/lua/"
        done
        echo "Installed lua scripts to ${RIMEDATADIR}/lua"
    fi
    echo "Installed rime-wubi schemas to ${RIMEDATADIR}"
fi

# Configure KWin（Plasma 6 只提供 kwriteconfig6，5.x 才是 kwriteconfig5；
# 都没有时不要让 set -e 把整个安装判成失败，只提示手动设置）
KWRITECONFIG=""
for cmd in kwriteconfig6 kwriteconfig5; do
    if command -v "$cmd" >/dev/null 2>&1; then
        KWRITECONFIG="$cmd"
        break
    fi
done
if [ -n "${KWRITECONFIG}" ]; then
    "${KWRITECONFIG}" --file kwinrc --group Wayland --key VirtualKeyboard xime-launcher.desktop
    "${KWRITECONFIG}" --file kwinrc --group Wayland --key VirtualKeyboardEnabled true
    echo "KWin configured to use xime-launcher.desktop"
else
    echo "warning: 未找到 kwriteconfig6/kwriteconfig5，请手动把虚拟键盘设为 xime-launcher.desktop" >&2
fi

echo ""
echo "Development installation complete!"
echo ""
echo "To test:"
echo "1. Restart KDE Plasma (logout/login) OR restart KWin"
echo "2. Open Kate and type to trigger VirtualKeyboard"
echo ""
# 语音运行库（sherpa-onnx / onnxruntime）：xime-daemon 的 rpath 指向
# $ORIGIN/../share/xime/lib，这里从构建缓存拷入。
SPEECH_LIB_DIR="${DATADIR}/xime/lib"
SPEECH_SRC=$(dirname "$(find "${TARGET_DIR}/sherpa-onnx-prebuilt" -name "libsherpa-onnx-c-api.so" 2>/dev/null | head -1)" 2>/dev/null)
if [ -n "${SPEECH_SRC}" ] && [ -d "${SPEECH_SRC}" ]; then
    install -d "${SPEECH_LIB_DIR}"
    for so in "${SPEECH_SRC}"/*.so; do
        [ -f "$so" ] && install -m755 "$so" "${SPEECH_LIB_DIR}/"
    done
    echo "Installed speech runtime libs to ${SPEECH_LIB_DIR}"
fi

echo "librime is built from the submodule (${LIBXIMECORE}/librime/dist/lib);"
echo "binaries embed an rpath to it, no LD_LIBRARY_PATH needed."
