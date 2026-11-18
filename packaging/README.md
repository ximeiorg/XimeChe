# 打包

所有格式都由 GitHub Actions 在打 tag 时自动构建（`.github/workflows/release.yml`），
产物直接挂到 Release 页面。

## 产物矩阵

| 文件 | 包管理器 | 主要发行版 | 构建环境 |
| --- | --- | --- | --- |
| `xime_<ver>_amd64.deb` | apt / dpkg | Debian、Ubuntu、Mint、Deepin、UOS | ubuntu-24.04（`cargo-deb`，CPU 语音） |
| `xime-cuda_<ver>_amd64.deb` | apt / dpkg | 同上，且有 NVIDIA 显卡 | ubuntu-24.04 + CUDA 12.4（`cargo-deb --variant cuda`） |
| `xime-<ver>-1.fc<N>.x86_64.rpm` | dnf / zypper | Fedora、RHEL 系、openSUSE | Fedora 容器（`rpmbuild`） |
| `xime-<ver>-1-x86_64.pkg.tar.zst` | pacman | Arch、Manjaro、EndeavourOS | Arch 容器（`makepkg`） |
| `xime-<ver>-x86_64-linux.tar.gz` | 手动 | 任意发行版 | ubuntu-24.04（`stage.sh`，CPU 语音） |
| `xime-<ver>.tar.gz` | 源码 | 打包者 | 含 rime-wubi 子模块的完整源码树 |

两个 deb 的**文件清单完全一样**，只差 `xime-speechd`（语音识别守护进程）编的是哪个
后端，所以它们互相冲突——`apt` 会提示用哪个替换哪个，装一个就行：

- **纯 CPU 版**：任何 x86_64 机器都能跑，识别延迟高一些（句级上屏）。
- **CUDA 版**：需要 NVIDIA 驱动 + CUDA 12 运行时，识别延迟低一个量级（逐字上屏）。

rpm 和 arch 都在各自的发行版容器里**从源码编译**，不是把 Ubuntu 的二进制换个壳，
所以 glibc / 依赖关系是对的（语音守护进程目前还没进这两种包）。

## 安装

```bash
# Debian / Ubuntu：先看有没有 NVIDIA 显卡
nvidia-smi && sudo apt install ./xime-cuda_*_amd64.deb
# 没有独显（或不想装 CUDA）就用 CPU 版
sudo apt install ./xime_*_amd64.deb

# Fedora / RHEL / openSUSE
sudo dnf install ./xime-*.rpm          # openSUSE 用 sudo zypper install ./xime-*.rpm

# Arch / Manjaro
sudo pacman -U ./xime-*.pkg.tar.zst

# 通用（解开就是 /usr 下的布局）
sudo tar -C / -xzf xime-*-x86_64-linux.tar.gz
```

CUDA 版**不硬依赖** CUDA 库（官方仓库里没有 `libcudart12`，硬依赖会装不上），
所以需要你自己保证这三个库在：`libcudart.so.12`、`libcublas.so.12`、
`libcurand.so.10`。最省事的办法是装发行版的 `nvidia-cuda-toolkit`，或者从
NVIDIA 的 CUDA 仓库装 `cuda-cudart-12-4` + `libcublas12`。缺库时语音用不了
（输入法本体照常工作），状态栏/通知会给出错误提示。

装完在 KDE 里启用（KWin 只在启动时装配虚拟键盘，改完需要重新登录）：

```bash
kwriteconfig6 --file kwinrc --group Wayland --key VirtualKeyboard xime-launcher.desktop
qdbus6 org.kde.KWin /KWin reconfigure   # 没生效就注销重登
```

设置界面：`xime-setup`。首次运行时 daemon 会把 `/usr/share/xime/rime-data`
部署到 `~/.config/xime/rime/`。

## 从源码构建

三种格式共用同一个安装布局脚本 `packaging/stage.sh`：

```bash
cargo build --release
./packaging/stage.sh /tmp/xime-stage /usr   # 铺出完整安装树
```

语音守护进程 `xime-speechd` 在 libximecore 里，要单独构建一次（`dev-install.sh`
会自动做，也可以手动）：

```bash
git clone --depth 1 https://github.com/ximeiorg/libximecore.git ../libximecore

# CPU 版
cargo build --release --manifest-path ../libximecore/Cargo.toml -p xime-speechd
# CUDA 版（需要 CUDA toolkit，cudarc 白名单内 12.x–13.4）。编译**不需要 GPU**，
# 但无 GPU 时必须显式给 CUDA_COMPUTE_CAP：cudaforge 探测不到 nvidia-smi 会
# 直接报错中断构建（不是回落到某个默认值）。
CUDA_COMPUTE_CAP=89 cargo build --release \
  --manifest-path ../libximecore/Cargo.toml -p xime-speechd --features cuda

# 两种后端产物同名，放哪个进包就出哪个变体
cp ../libximecore/target/release/xime-speechd target/release/
```

各格式的配方：

```bash
# deb（先按上面把 xime-speechd 放好）
cargo install cargo-deb
cargo deb -p xime                 # → xime_<ver>_amd64.deb（CPU）
cargo deb -p xime --variant cuda  # → xime-cuda_<ver>_amd64.deb（CUDA）

# rpm（Fedora 上）
dnf install rpm-build cargo rust git gcc gcc-c++ make pkgconf-pkg-config \
            librime-devel libxkbcommon-devel wayland-devel dbus-devel \
            fontconfig-devel freetype-devel clang clang-devel
rpmbuild -bb packaging/rpm/xime.spec --define "_topdir $PWD/rpmbuild"

# Arch
cd packaging/arch && makepkg -si
```

`.deb` 的文件清单在 `crates/xime-pack/Cargo.toml` 的 `[package.metadata.deb]`，
CUDA 变体只在同一个文件的 `[...deb.variants.cuda]` 里覆盖元数据（包名自动变成
`xime-cuda`），文件清单继承主机段。其余格式走 `packaging/stage.sh`。
**改安装布局时两处都要改**。

## 版本同步

CI 用 tag 作为版本号，并校验它和 `Cargo.toml` 的 `[workspace.package] version`
一致（不一致直接失败，避免打出版本号错乱的包）。所以发版前需要一起改：

1. `Cargo.toml` 的 `version`
2. `packaging/arch/PKGBUILD` 的 `pkgver`（CI 会覆盖，但仓库里要保持可读可构建）
3. `packaging/rpm/xime.spec` 的 `Version`（同上）

升级 `rime-wubi` 子模块后，还要同步 `PKGBUILD` 里的 `_rime_wubi_commit`。

## 依赖说明

运行期：`librime`、`libxkbcommon`、`libwayland-client`、`dbus`、`libbz2`、
`libasound2`（语音守护进程录音用 ALSA）。Rime 走 pkg-config（有 `rime.pc` 时）
或直接链系统 `librime`；仓库里不带 librime 源码，也不需要 cmake 构建它。

语音识别由独立进程 `xime-speechd` 提供（candle + Qwen3-ASR）。**模型不随包分发**，
用户在设置页的「语音」里下载到 `~/.config/xime/models`。守护进程本身约 50 MB：

- CPU 版：任何 x86_64 机器可跑，识别是「句级上屏」（每两秒一段）。
- CUDA 版：额外链接 `libcudart.so.12`、`libcublas.so.12`、`libcurand.so.10`，
  缺库时语音起不来（输入法本体照常工作）。识别是「逐字上屏」。
- 源码构建需要 `libasound2-dev`；CUDA 版另需 CUDA toolkit（cudarc 0.19 的
  白名单是 12.x–13.4，13.5+ 会在 build script 直接 panic）。

## Arch Linux 专项说明

`packaging/arch/PKGBUILD` 可以直接作为 AUR 包使用（`xime` 这个名字在 AUR 还没被占用）：

```bash
cd packaging/arch
makepkg -si                          # 本地构建安装
makepkg --printsrcinfo > .SRCINFO    # 推 AUR 前生成元数据
```

- 依赖：`librime`、`libxkbcommon`、`wayland`（运行时 dlopen）、`dbus`、`bzip2`、
  `gcc-libs`、`hicolor-icon-theme`、`desktop-file-utils`。
- `rime-wubi` 是 git 子模块，makepkg 不会自动初始化，`prepare()` 会单独拉取并
  填回源码树；升级子模块后记得同步 PKGBUILD 里的 `_rime_wubi_commit`。
- 链接器用 GNU ld/bfd（`build()` 里显式 `-fuse-ld=bfd`）：Arch 工具链默认的 lld
  在链接 `sherpa-onnx-sys` build script（zip 的 zstd + ring 静态库）时会漏符号。
- CI 的 Arch job 不只是打个包：它会把生成的 `.pkg.tar.zst` 装进容器，用 `ldd`
  确认 daemon 能按 rpath 找到 `/usr/share/xime/lib` 下的语音库，并检查方案数据、
  DBus 服务、可执行文件都在位（`namcap` 的输出只作为提示）。
- Release 里另外挂一个 `xime-<ver>-aur.tar.gz`，里面是版本号已经对好的
  `PKGBUILD`、`makepkg --printsrcinfo` 生成的 `.SRCINFO` 和 `xime.install`。

### 第一次发布到 AUR

```bash
# 1. 在 aur.archlinux.org 建账号并上传 SSH 公钥，然后克隆空仓库
git clone ssh://aur@aur.archlinux.org/xime.git && cd xime

# 2. 解开 release 里的 AUR 提交物
tar -xzf ~/Downloads/xime-<ver>-aur.tar.gz

# 3. 提交
git add PKGBUILD .SRCINFO xime.install
git commit -m "Initial commit: xime <ver>"
git push
```

之后每次发版只要用新的 `xime-<ver>-aur.tar.gz` 覆盖这三个文件再 push
（改了 `PKGBUILD` 就必须重新生成 `.SRCINFO`，否则 AUR 拒绝）。

## 没有提供的格式

- **Flatpak / Snap**：输入法守护进程必须作为 KWin 的虚拟键盘客户端工作——
  由 KWin 通过 `WAYLAND_SOCKET` 传 fd 进来，再用 `zwp_input_method_v2` 收发按键，
  并通过 DBus(`org.xime.Xime.Controller`) 在两个进程间传递这个 fd。沙箱化的应用
  既拿不到这个 fd，也无法被 KWin 选为虚拟键盘，这不是权限问题而是架构冲突。
- **Alpine (apk)**：语音运行库 sherpa-onnx / onnxruntime 上游只提供 glibc 预编译包，
  musl 下链不上。等语音做成可选特性后才好支持。
- **Nix**：git 依赖 + 子模块需要在 `fetchgit`/`cargoHash` 里固定 hash，
  当前没有 Nix 环境生成，欢迎 PR。

## 已知限制

`.deb` 在 `ubuntu-24.04`（glibc 2.39）上编译，因此要求
Ubuntu 24.04+ / Debian 13+ / 同等 glibc 的发行版。更老的发行版需要自己编译。

CUDA 变体是在**没有 GPU 的 runner** 上编的，`CUDA_COMPUTE_CAP` 钉在 80：
cubin 在同一 major 内向上兼容，所以 30/40 系（sm_86/89）能直接跑；Hopper（sm_90）
及更新的架构需要用 `CUDA_COMPUTE_CAP=90`（或对应值）自行重编。

构建时 `libximecore` 按 `main` 分支的 HEAD 解析（CI 里显式 clone 后 patch 成本地
路径，和本地开发一致），不使用 `Cargo.lock` 里可能过期的 revision。这意味着同一个
tag 在不同时间构建，依赖代码可能不同；要严格复现得把 `Cargo.lock` 换成带 git
revision 的版本再构建。
