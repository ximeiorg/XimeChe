#!/bin/bash
# 统一安装布局：把已构建的产物铺到 <DESTDIR><PREFIX> 下。
#
# 用法:
#   packaging/stage.sh <DESTDIR> [PREFIX]
#
# 例子:
#   packaging/stage.sh /tmp/stage /usr     # 系统布局（rpm / Arch / 便携 tarball）
#
# 说明:
#   - .deb 仍由 cargo-deb 生成（见 crates/xime-pack/Cargo.toml 的 assets），
#     其余格式都走这个脚本。两份文件清单必须保持一致，改安装布局时请同时修改。
#   - 语音运行库（sherpa-onnx / onnxruntime）在构建时由 sherpa-onnx-sys 拷到
#     <target>/release（target 目录见下方 TARGET_DIR）；daemon 的 rpath 是
#     $ORIGIN/../share/xime/lib，所以它们必须落在 $PREFIX/share/xime/lib。
#     构建时没启用语音可设 XIME_SKIP_SPEECH=1。
set -euo pipefail

DESTDIR="${1:?用法: stage.sh <DESTDIR> [PREFIX]}"
PREFIX="${2:-/usr}"
BINDIR="$PREFIX/bin"
DATADIR="$PREFIX/share"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# 产物目录：~/.cargo/config.toml 可能设了全局 build.target-dir，产物未必在
# <repo>/target 下。问 cargo 要真实路径，问不到才退回默认。
TARGET_DIR="$(cargo metadata --format-version 1 --no-deps \
    --manifest-path "$ROOT/Cargo.toml" 2>/dev/null \
    | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"
[ -n "$TARGET_DIR" ] || TARGET_DIR="$ROOT/target"

die() {
    echo "error: $*" >&2
    exit 1
}

shopt -s nullglob

# ── 可执行文件 ────────────────────────────────────────────────────────────
for bin in xime-daemon xime-launcher xime-setup; do
    [ -x "$TARGET_DIR/release/$bin" ] || die "缺少 $TARGET_DIR/release/$bin（先运行 cargo build --release）"
    install -Dm755 "$TARGET_DIR/release/$bin" "$DESTDIR$BINDIR/$bin"
done

# ── 默认配置 + Rime 方案数据 ──────────────────────────────────────────────
install -Dm644 resources/xime.yaml "$DESTDIR$DATADIR/xime/xime.yaml"

schemas=(rime-wubi/*.yaml)
[ ${#schemas[@]} -gt 0 ] || die "rime-wubi/ 为空：子模块未初始化（git submodule update --init --recursive）"
install -d "$DESTDIR$DATADIR/xime/rime-data"
install -m644 "${schemas[@]}" "$DESTDIR$DATADIR/xime/rime-data/"

luas=(rime-wubi/lua/*.lua)
if [ ${#luas[@]} -gt 0 ]; then
    install -d "$DESTDIR$DATADIR/xime/rime-data/lua"
    install -m644 "${luas[@]}" "$DESTDIR$DATADIR/xime/rime-data/lua/"
fi

# ── 图标 ──────────────────────────────────────────────────────────────────
for size in 48x48 64x64 128x128 256x256 512x512; do
    icon="resources/icons/$size/apps/xime.png"
    [ -f "$icon" ] || die "缺少 $icon"
    install -Dm644 "$icon" "$DESTDIR$DATADIR/icons/hicolor/$size/apps/xime.png"
done

# ── DBus 服务 + 桌面入口（模板里的 @BINDIR@ 换成实际 bin 目录）────────────
render() { # <resources 下的相对路径，不含 .in> <相对于 PREFIX 的目标目录>
    local out="$DESTDIR$PREFIX/$2/$(basename "$1")"
    install -d "$(dirname "$out")"
    sed "s|@BINDIR@|$BINDIR|g" "resources/$1.in" >"$out"
    chmod 644 "$out"
}

render dbus/org.xime.Xime.service share/dbus-1/services
render applications/xime-launcher.desktop share/applications
render applications/xime-setup.desktop share/applications

# ── 语音运行库 ────────────────────────────────────────────────────────────
if [ "${XIME_SKIP_SPEECH:-0}" = "1" ]; then
    echo "warning: XIME_SKIP_SPEECH=1，跳过语音运行库（语音输入将不可用）" >&2
else
    speech_libs=("$TARGET_DIR"/release/libsherpa-onnx*.so* "$TARGET_DIR"/release/libonnxruntime*.so*)
    [ ${#speech_libs[@]} -gt 0 ] ||
        die "找不到语音运行库（$TARGET_DIR/release/libsherpa-onnx*.so*）；不需要语音请设 XIME_SKIP_SPEECH=1"
    install -d "$DESTDIR$DATADIR/xime/lib"
    install -m755 "${speech_libs[@]}" "$DESTDIR$DATADIR/xime/lib/"
fi

echo "staged $(find "$DESTDIR" -type f | wc -l) files into $DESTDIR ($PREFIX)"
