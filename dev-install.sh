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
xime-plugin = { path = "../libximecore/crates/xime-plugin" }
xime-speech-service = { path = "../libximecore/crates/xime-speech-service" }
EOF
    echo "Created ${CARGO_CONF} with local libximecore patch"
fi

# Stop existing xime processes to refresh
echo "Stopping existing xime processes..."
pkill -9 xime-daemon 2>/dev/null || true
pkill -9 xime-launcher 2>/dev/null || true
# 语音服务也一起收掉：旧实例不认识「总开关关闭就退出」的新行为，留着会继续
# 占内存；新实例由 daemon / 设置页在需要时按需拉起。
pkill -9 xime-speechd 2>/dev/null || true
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

# Build & install xime-speechd（语音识别独立进程，from libximecore）
#
# 它持有 Qwen3-ASR 权重与麦克风，daemon 只通过 socket 说话。candle 首次编译
# 要十几分钟，所以装过就跳过；改了语音引擎相关代码时置
# XIME_SPEECH_REBUILD=1 强制重建。
#
# 后端决定流式体验：
#   GPU（cuda）：8 秒窗口一轮 ~300ms，逐字上屏；
#   CPU        ：2 秒窗口一轮 ~1.2s，退化成句级（每两秒吐一段）。
# 有可用的 NVIDIA 显卡 + cudarc 认得的 toolkit 时自动编 cuda 版；
# XIME_SPEECH_CUDA=0 强制 CPU，=1 强制 GPU（找不到工具链直接失败）。
#
# cudarc 0.19.10 的 build.rs 只认一份白名单（最高 13.4），更新的 toolkit 直接
# panic。所以：
#   1) 机器上有多份 CUDA 时，优先挑白名单内的最高版本；
#   2) 只有比 13.4 新的（例如 13.5）时，用「不高于它的最高白名单版本」生成
#      绑定（CUDARC_CUDA_VERSION），链接的仍是系统里更新的运行库——ABI 向后
#      兼容，已在 CUDA 13.5 + RTX 4060 Ti 上验证输出与 CPU 逐字相同。
CUDARC_CUDA_SUPPORTED="13.4 13.3 13.2 13.1 13.0 12.9 12.8 12.6 12.5 12.4 12.3 12.2 12.1 12.0 11.8 11.7 11.6 11.5 11.4"

# nvcc 版本号（major.minor）。
cuda_nvcc_version() {
    "$1" --version 2>/dev/null | sed -n 's/.*release \([0-9]*\.[0-9]*\).*/\1/p' | head -1
}

# 这个版本在 cudarc 白名单里？
cuda_supported() {
    case " ${CUDARC_CUDA_SUPPORTED} " in *" $1 "*) return 0 ;; *) return 1 ;; esac
}

# 不高于 $1 的最高白名单版本；空 = 没有（比所有白名单版本都旧）。
cuda_compat_version() {
    for cand in ${CUDARC_CUDA_SUPPORTED}; do
        if [ "$(printf '%s\n%s\n' "$1" "$cand" | sort -V | head -1)" = "$cand" ]; then
            echo "$cand"
            return 0
        fi
    done
    return 1
}

speech_features=""
speech_backend="cpu"
if [ "${XIME_SPEECH_CUDA:-auto}" != "0" ] && command -v nvidia-smi >/dev/null 2>&1; then
    if nvidia-smi -L >/dev/null 2>&1; then
        cuda_dirs="$(ls -d /usr/local/cuda-* 2>/dev/null | sort -Vr) /usr/local/cuda"
        if command -v nvcc >/dev/null 2>&1; then
            cuda_dirs="${cuda_dirs} $(dirname "$(dirname "$(command -v nvcc)")")"
        fi
        # 第一遍：白名单内的版本优先（多版本共存时取最高）。
        for cuda_dir in ${cuda_dirs}; do
            [ -x "${cuda_dir}/bin/nvcc" ] || continue
            cuda_ver="$(cuda_nvcc_version "${cuda_dir}/bin/nvcc")"
            [ -n "${cuda_ver}" ] || continue
            if cuda_supported "${cuda_ver}"; then
                export CUDA_HOME="${cuda_dir}"
                export PATH="${cuda_dir}/bin:${PATH}"
                speech_features="--features cuda"
                speech_backend="cuda ${cuda_ver} (${cuda_dir})"
                break
            fi
        done
        # 第二遍：只有更新的 toolkit 时，按最高的白名单版本生成绑定。
        if [ -z "${speech_features}" ]; then
            for cuda_dir in ${cuda_dirs}; do
                [ -x "${cuda_dir}/bin/nvcc" ] || continue
                cuda_ver="$(cuda_nvcc_version "${cuda_dir}/bin/nvcc")"
                [ -n "${cuda_ver}" ] || continue
                if cuda_bindings="$(cuda_compat_version "${cuda_ver}")"; then
                    export CUDA_HOME="${cuda_dir}"
                    export PATH="${cuda_dir}/bin:${PATH}"
                    # build.rs 认 "13040" 这种格式（major 0 minor 0）。
                    export CUDARC_CUDA_VERSION="$(printf '%s0' "${cuda_bindings//./0}")"
                    speech_features="--features cuda"
                    speech_backend="cuda ${cuda_ver} (${cuda_dir}，按 ${cuda_bindings} 绑定)"
                    echo "提示：CUDA ${cuda_ver} 比 cudarc 0.19.10 白名单新，按 ${cuda_bindings} 的绑定编译（CUDARC_CUDA_VERSION=${CUDARC_CUDA_VERSION}）" >&2
                    break
                fi
            done
        fi
        if [ -z "${speech_features}" ] && [ "${XIME_SPEECH_CUDA:-auto}" = "1" ]; then
            echo "错误：XIME_SPEECH_CUDA=1 但没有可用的 CUDA toolkit（cudarc 白名单：${CUDARC_CUDA_SUPPORTED}）" >&2
            exit 1
        fi
    fi
fi
if [ -f "${LIBXIMECORE}/Cargo.toml" ]; then
    if [ -x "${BINDIR}/xime-speechd" ] && [ "${XIME_SPEECH_REBUILD:-0}" != "1" ]; then
        echo "xime-speechd 已安装，跳过构建（XIME_SPEECH_REBUILD=1 强制重建）"
    else
        echo "构建 xime-speechd（candle，后端 ${speech_backend}，首次较慢）..."
        cargo build --release --manifest-path "${LIBXIMECORE}/Cargo.toml" -p xime-speechd ${speech_features}
        install -Dm755 "${LIBXIMECORE_TARGET_DIR}/release/xime-speechd" "${BINDIR}/xime-speechd"
    fi
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

# 桌面集成：KDE 走 KWin 的虚拟键盘机制（写 kwinrc），其他 Wayland 桌面
# （COSMIC / GNOME / sway）没有这套机制，只能靠 XDG autostart 把 daemon 拉起来。
#
# autostart 条目带 NotShowIn=KDE，Plasma 下不会生效；这里无条件装上，是为了
# 让同一份用户目录在切换桌面后依然可用（装的时候不一定是将来要用的桌面）。
AUTOSTARTDIR="${HOME}/.config/autostart"
install -Dm644 "${PROJECT_ROOT}/resources/applications/xime-daemon.desktop.in" \
    "${AUTOSTARTDIR}/xime-daemon.desktop"
sed -i "s|@BINDIR@|${BINDIR}|g" "${AUTOSTARTDIR}/xime-daemon.desktop"
echo "Installed XDG autostart entry to ${AUTOSTARTDIR}/xime-daemon.desktop"

# Configure KWin（Plasma 6 只提供 kwriteconfig6，5.x 才是 kwriteconfig5；
# 都没有时不要让 set -e 把整个安装判成失败，只提示手动设置）
case "${XDG_CURRENT_DESKTOP:-}" in
*KDE* | *Plasma* | "")
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
    ;;
*)
    echo "Non-KDE desktop (${XDG_CURRENT_DESKTOP}): daemon will autostart via XDG,"
    echo "log out and back in to let the session start it."
    # COSMIC 1.9+ 的屏幕键盘会先绑定唯一的 zwp_input_method_manager_v2 槽位，
    # 后到的输入法只会拿到 unavailable（daemon 日志里是一条 warn，按退避重试）。
    if command -v cosmic-osk >/dev/null 2>&1; then
        echo "warning: cosmic-osk 会抢占唯一的输入法槽位（COSMIC 1.9+）：" >&2
        echo "         先在 COSMIC 面板里关掉屏幕键盘；若无效，放一个" >&2
        echo "         ~/.local/bin/cosmic-osk stub（exec sleep infinity）顶掉它。" >&2
    fi
    ;;
esac

echo ""
echo "Development installation complete!"
echo ""
echo "To test:"
case "${XDG_CURRENT_DESKTOP:-}" in
*KDE* | *Plasma* | "")
    echo "1. Restart KDE Plasma (logout/login) OR restart KWin"
    echo "2. Open Kate and type to trigger VirtualKeyboard"
    ;;
*)
    echo "1. Log out and back in — the session starts xime-daemon via XDG autostart"
    echo "2. Focus any text field and press Ctrl+Space to switch Chinese/English"
    ;;
esac
echo ""
echo "librime is built from the submodule (${LIBXIMECORE}/librime/dist/lib);"
echo "binaries embed an rpath to it, no LD_LIBRARY_PATH needed."
