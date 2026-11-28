#!/usr/bin/env bash
# 本地打 .deb：CPU / CUDA 两个变体，步骤与 .github/workflows/release.yml 等价。
#
#   ./packaging/build-deb.sh              # 两个都打
#   ./packaging/build-deb.sh cpu          # 只打 CPU 版
#   ./packaging/build-deb.sh cuda         # 只打 CUDA 版
#
# 产物落在 cargo 的 target 目录下的 debian/（见脚本结尾的路径输出）。
# 编 CUDA 不需要 GPU，但需要 CUDA toolkit；没有 nvidia-smi 时必须自己给
# CUDA_COMPUTE_CAP（例如 CUDA_COMPUTE_CAP=80），否则 candle 的构建脚本会
# 探测失败、直接报错中断。
set -euo pipefail

REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$REPO"

VARIANT=${1:-both}
case "$VARIANT" in
    cpu|cuda|both) ;;
    *) echo "用法: $0 [cpu|cuda|both]" >&2; exit 2 ;;
esac

die() { echo "错误: $*" >&2; exit 1; }
say() { printf '\n\033[1m==> %s\033[0m\n' "$*"; }

command -v cargo >/dev/null || die "找不到 cargo"
cargo deb --version >/dev/null 2>&1 || die "找不到 cargo-deb，先 cargo install cargo-deb"

# target 目录可能被 ~/.cargo/config.toml 的 build.target-dir 挪到别处，
# 不能假定是本仓库下的 target/。
TARGET_DIR=$(cargo metadata --format-version 1 --no-deps 2>/dev/null \
    | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
[ -n "$TARGET_DIR" ] || die "取不到 cargo 的 target_directory"

LIBCORE=$(cd ../libximecore 2>/dev/null && pwd) || \
    die "找不到 ../libximecore（.cargo/config.toml 里 patch 的本地路径），先 clone 到同级目录"
[ -f "$LIBCORE/crates/xime-speechd/Cargo.toml" ] || \
    die "$LIBCORE 里没有 crates/xime-speechd"

# ── CUDA 环境 ────────────────────────────────────────────────────────────
# 只编译，不需要 GPU；但必须有 nvcc，且要告诉 candle 编给哪个架构。
nvcc_version() { sed -n 's/.*release \([0-9]*\.[0-9]*\).*/\1/p' "$1" 2>/dev/null | head -1; }

# cudarc 0.19.10 的 build.rs 只认这份白名单（最高 13.4）；更新的 toolkit 直接
# panic。装的是 13.5+ 时用「不高于它的最高白名单版本」生成绑定（运行期 ABI
# 向后兼容，已在 CUDA 13.5 + RTX 4060 Ti 上验证）。
CUDARC_CUDA_SUPPORTED="13.4 13.3 13.2 13.1 13.0 12.9 12.8 12.6 12.5 12.4 12.3 12.2 12.1 12.0 11.8 11.7 11.6 11.5 11.4"

cuda_supported() {
    case " ${CUDARC_CUDA_SUPPORTED} " in *" $1 "*) return 0 ;; *) return 1 ;; esac
}

# 不高于 $1 的最高白名单版本（供 CUDARC_CUDA_VERSION 用）；空 = 没有。
cuda_compat_version() {
    local cand
    for cand in ${CUDARC_CUDA_SUPPORTED}; do
        if [ "$(printf '%s\n%s\n' "$1" "$cand" | sort -V | head -1)" = "$cand" ]; then
            echo "$cand"
            return 0
        fi
    done
    return 1
}

# 机器上常同时装了几个 CUDA（/usr/local/cuda 往往指向最新的 13.5；它不在白名单
# 里，但可以按 13.4 的绑定编）。因此把候选都列出来：先挑白名单内的最高版本；
# 一个都没有时，再挑能降级绑定的最高版本，而不是见到第一个就用。
detect_cuda() {
    local candidates=() dir ver best="" best_ver=""
    if [ -n "${CUDA_HOME:-}" ] && [ -x "$CUDA_HOME/bin/nvcc" ]; then
        echo "$CUDA_HOME"      # 显式指定优先，越界由后面的检查报错
        return
    fi
    for dir in /usr/local/cuda /usr/local/cuda-*; do
        [ -x "$dir/bin/nvcc" ] && candidates+=("$dir")
    done
    if command -v nvcc >/dev/null; then
        candidates+=("$(dirname "$(dirname "$(command -v nvcc)")")")
    fi
    for dir in "${candidates[@]}"; do
        ver=$(nvcc_version "$dir/bin/nvcc")
        cuda_supported "$ver" || continue
        if [ -z "$best_ver" ] || [ "$(printf '%s\n%s\n' "$ver" "$best_ver" | sort -V | tail -1)" = "$ver" ]; then
            best="$dir"
            best_ver="$ver"
        fi
    done
    if [ -z "$best" ]; then
        # 没有白名单内的版本：退而求其次，挑一个能用更旧绑定的最高版本
        # （例如 13.5 → 13.4 绑定、链接 13.5 运行库）。
        for dir in "${candidates[@]}"; do
            ver=$(nvcc_version "$dir/bin/nvcc")
            [ -n "$ver" ] || continue
            cuda_compat_version "$ver" >/dev/null || continue
            if [ -z "$best_ver" ] || [ "$(printf '%s\n%s\n' "$ver" "$best_ver" | sort -V | tail -1)" = "$ver" ]; then
                best="$dir"
                best_ver="$ver"
            fi
        done
    fi
    if [ -z "$best" ]; then
        for dir in "${candidates[@]}"; do
            ver=$(nvcc_version "$dir/bin/nvcc")
            [ -n "$ver" ] && echo "  检测到 CUDA $ver（$dir）" >&2
        done
        return 1
    fi
    echo "$best"
}

cuda_env() {
    local cuda_home nvcc_ver gcc_major
    cuda_home=$(detect_cuda) || die "找不到可用的 CUDA toolkit（cudarc 0.19.10 白名单：${CUDARC_CUDA_SUPPORTED}）。装一个，或用 CUDA_HOME 指定"
    export CUDA_HOME="$cuda_home"
    export PATH="$cuda_home/bin:$PATH"

    nvcc_ver=$(nvcc_version "$cuda_home/bin/nvcc")
    say "CUDA $nvcc_ver（$cuda_home）"
    if ! cuda_supported "$nvcc_ver"; then
        compat=$(cuda_compat_version "$nvcc_ver") || \
            die "CUDA $nvcc_ver 比 cudarc 0.19.10 白名单还旧，用不了。
      白名单：${CUDARC_CUDA_SUPPORTED}"
        export CUDARC_CUDA_VERSION="$(printf '%s0' "${compat//./0}")"
        say "CUDA $nvcc_ver 不在 cudarc 白名单，按 $compat 的绑定编译（CUDARC_CUDA_VERSION=$CUDARC_CUDA_VERSION）"
    fi

    # 架构：优先环境变量，其次真实显卡，都没有就只能让用户自己给。
    if [ -z "${CUDA_COMPUTE_CAP:-}" ]; then
        if command -v nvidia-smi >/dev/null; then
            cue=$(nvidia-smi --query-gpu=compute_cap --format=csv,noheader 2>/dev/null \
                | head -1 | tr -d ' .')
            [ -n "$cue" ] && export CUDA_COMPUTE_CAP="$cue"
        fi
    fi
    [ -n "${CUDA_COMPUTE_CAP:-}" ] || \
        die "没有 GPU 也没有 CUDA_COMPUTE_CAP：显式给一个（如 CUDA_COMPUTE_CAP=80）"
    say "目标架构 sm_${CUDA_COMPUTE_CAP}（cubin 在同一 major 内向上兼容）"

    # CUDA 12.x 只支持到 gcc 13.2，而 Ubuntu 24.04 自带 13.3：不换会把
    # 「unsupported GNU version」当错误抛出来。
    if [ -z "${NVCC_CCBIN:-}" ]; then
        gcc_major=$(gcc -dumpversion 2>/dev/null | cut -d. -f1)
        if [ "$(printf '%s\n%s\n' "$nvcc_ver" 13.0 | sort -V | head -1)" = "$nvcc_ver" ] \
            && [ "${gcc_major:-0}" -gt 13 ]; then
            if command -v g++-12 >/dev/null; then
                export NVCC_CCBIN=/usr/bin/g++-12
                say "nvcc $nvcc_ver 不认 gcc $gcc_major，改用 g++-12"
            else
                die "nvcc $nvcc_ver 需要 gcc ≤13，装 g++-12 或设 NVCC_CCBIN"
            fi
        fi
    fi
}

# 编译并安装语音守护进程（两个变体共用一个文件名，放哪个进包就出哪个变体）
build_speechd() {
    local backend=$1 speech_target
    if [ "$backend" = cuda ]; then
        cuda_env
        cargo build --release --manifest-path "$LIBCORE/Cargo.toml" \
            -p xime-speechd --features cuda
    else
        cargo build --release --manifest-path "$LIBCORE/Cargo.toml" -p xime-speechd
    fi
    # 全局 ~/.cargo/config.toml 可能设了 build.target-dir，本仓库和 libximecore
    # 就共用一个 target 了——所以每棵树都要各自问 cargo，不能写死 <repo>/target。
    speech_target=$(cargo metadata --format-version 1 --no-deps \
        --manifest-path "$LIBCORE/Cargo.toml" 2>/dev/null \
        | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
    [ -x "$speech_target/release/xime-speechd" ] || \
        die "没找到 $speech_target/release/xime-speechd"
    src=$(readlink -f "$speech_target/release/xime-speechd")
    dst="$TARGET_DIR/release/xime-speechd"
    # 本机两棵树共用 target 时 src 和 dst 是同一个文件，install 会拒绝（"为同一
    # 文件"），此时已经就位，跳过即可。
    if [ "$src" != "$(readlink -f "$dst" 2>/dev/null || true)" ]; then
        install -Dm755 "$src" "$dst"
    fi
}

inspect() {
    local deb=$1 backend=$2
    local bin
    bin=$(mktemp -d)
    dpkg-deb -x "$deb" "$bin" 2>/dev/null || die "解不开 $deb"
    if ldd "$bin/usr/bin/xime-speechd" 2>/dev/null | grep -q libcudart; then
        [ "$backend" = cuda ] || die "$deb 链接了 CUDA，但打的是 CPU 变体"
        # ldd 只能证明「声明了 libcudart」，cubin 才能证明 kernel 编进去了
        local cuobjdump cubins
        cuobjdump=$(command -v cuobjdump || echo "${CUDA_HOME:-/usr/local/cuda}/bin/cuobjdump")
        cubins=$("$cuobjdump" -lelf "$bin/usr/bin/xime-speechd" \
            | grep -c "\.sm_${CUDA_COMPUTE_CAP}\.cubin" || true)
        [ "$cubins" -gt 0 ] || die "$deb 里没有 sm_${CUDA_COMPUTE_CAP} cubin"
        # 运行时 soname 跟着工具链大版本走：CUDA 12 编出来要 libcudart.so.12，
        # CUDA 13 编出来要 .so.13。release 包固定用 12，本机用 13 编出来的包
        # 不能发给只装了 CUDA 12 的用户。
        local runtime
        runtime=$(ldd "$bin/usr/bin/xime-speechd" 2>/dev/null \
            | sed -n 's/.*libcudart\.so\.\([0-9]*\).*/\1/p' | head -1)
        echo "  语音后端 cuda，内嵌 sm_${CUDA_COMPUTE_CAP} cubin ${cubins} 个"
        echo "  运行时要求：CUDA ${runtime}.x（release 包是 CUDA 12.x）"
        if [ "${runtime:-}" != 12 ]; then
            # 提示里别写死某个小版本：本机装的是 12.6 还是 12.8 都可能，
            # release 只认 12.x 这条线。
            local suggest
            suggest=$(ls -d /usr/local/cuda-12.* 2>/dev/null | sort -V | tail -1)
            echo "  提示：本包基于 CUDA ${runtime}.x，与 release 的 CUDA 12.x 包不通用；" \
                 "要与 release 对齐就 CUDA_HOME=${suggest:-/usr/local/cuda-12.6} $0 cuda"
        fi
    elif [ "$backend" = cuda ]; then
        die "$deb 没链接 libcudart，CUDA 变体没生效（speechd 是不是被 CPU 版覆盖了？）"
    else
        echo "  语音后端 cpu"
    fi
    rm -rf "$bin"
}

say "构建主程序（xime-daemon / launcher / setup）"
cargo build --release

DEBS=()
# 两个变体都写在 target/debian 下同一个位置，打第二个前必须清空；先把成果
# 挪到独立目录，否则 CPU 包会被下一次 rm -rf 删掉。
OUT="$TARGET_DIR/deb-out"
mkdir -p "$OUT"

if [ "$VARIANT" = cpu ] || [ "$VARIANT" = both ]; then
    say "打 CPU 版 deb"
    build_speechd cpu
    rm -rf "$TARGET_DIR/debian"
    cargo deb -p xime --no-build
    deb="$OUT/$(basename "$(ls "$TARGET_DIR"/debian/xime_*_amd64.deb)")"
    cp "$(ls "$TARGET_DIR"/debian/xime_*_amd64.deb)" "$deb"
    inspect "$deb" cpu
    DEBS+=("$deb")
fi

if [ "$VARIANT" = cuda ] || [ "$VARIANT" = both ]; then
    say "打 CUDA 版 deb"
    build_speechd cuda
    rm -rf "$TARGET_DIR/debian"
    cargo deb -p xime --no-build --variant cuda
    deb="$OUT/$(basename "$(ls "$TARGET_DIR"/debian/xime-cuda_*_amd64.deb)")"
    cp "$(ls "$TARGET_DIR"/debian/xime-cuda_*_amd64.deb)" "$deb"
    inspect "$deb" cuda
    DEBS+=("$deb")
fi

say "完成"
for deb in "${DEBS[@]}"; do
    printf '  %-52s %s\n' "$deb" "$(du -h "$deb" | cut -f1)"
done
echo
echo "安装：sudo apt install ${DEBS[*]}（两个包互斥，装第二个会替换第一个）"
