# Fedora / RHEL / openSUSE 打包配方。
#
# CI 会用 tag 覆盖 Version（rpmbuild 前 sed 改写），仓库里保持与
# Cargo.toml 的 [workspace.package] version 一致即可。
#
# Source0 指向 GitHub Release 里的源码包（含 rime-wubi 子模块内容）。
# GitHub 自动生成的 /archive/ 源码包不含子模块，不能用。
#
# 构建:
#   sudo dnf install rpm-build cargo rust git gcc gcc-c++ make \
#        pkgconf-pkg-config librime-devel libxkbcommon-devel
#   spectool -g xime.spec && rpmbuild -ba xime.spec

%global srcname XimeChe

Name:           xime
Version:        0.2.0
Release:        1%{?dist}
Summary:        曦码 — 基于 Rime 引擎的 Wayland 中文输入法

License:        GPL-3.0-or-later
URL:            https://github.com/ximeiorg/XimeChe
Source0:        %{url}/releases/download/%{version}/%{name}-%{version}.tar.gz

# 运行期依赖：链接到的系统库。怀旧版 SUSE 上 librime 拆包名不同。
%if 0%{?suse_version}
Requires:       librime1
%else
Requires:       librime
%endif
Requires:       libxkbcommon
Requires:       hicolor-icon-theme

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  git
BuildRequires:  gcc
BuildRequires:  gcc-c++
BuildRequires:  make
BuildRequires:  pkgconf-pkg-config
BuildRequires:  librime-devel
BuildRequires:  libxkbcommon-devel
# libquickjs-ng-sys 用 bindgen，需要 libclang（Fedora 的 clang-libs 提供 .so）
BuildRequires:  clang
# libdbus-sys 走 pkg-config
BuildRequires:  dbus-devel

# 语音运行库（sherpa-onnx / onnxruntime）随包自带到 /usr/share/xime/lib，
# daemon 用 rpath 定位，不需要系统仓库提供。但 rpm 的自动依赖扫描仍会从
# ELF 里读出 libonnxruntime.so(VERS_x.y.z)(64bit) 之类的符号版本依赖，而
# Fedora/openSUSE 仓库没有 onnxruntime，装包会报 "nothing provides"——
# 过滤掉这批由自带库产生的自动依赖（不碰 librime 等真正的系统依赖）。
%global __requires_exclude ^lib(onnxruntime|sherpa-onnx).*\\.so.*$

Requires(post):   desktop-file-utils
# %post 里调 gtk-update-icon-cache；没有这个依赖时 scriptlet 会刷
# "command not found"（已被 || : 吞掉，但会在安装日志里报警）。
Requires(post):   gtk-update-icon-cache
Requires(postun): desktop-file-utils

%description
曦码（Xime）是用纯 Rust 实现的 Linux 中文输入法，直接对接 Wayland 的
input-method 协议并复用 Rime 引擎，候选窗自绘。

%prep
%autosetup -n %{srcname}-%{version}

%build
# 统一用 GNU ld/bfd 链接：Arch 的工具链默认塞 -fuse-ld=lld，lld 链接
# sherpa-onnx-sys 的 build script（zip 的 zstd + ring 静态库）会报
# ZSTD_*/ring_core_* undefined symbol；这里显式指定，各发行版行为一致。
# 不加 --locked：libximecore 的 git 依赖在本地开发时被 .cargo/config.toml
# patch 成路径依赖，Cargo.lock 里没有对应的 git 条目，构建时需要联网解析。
RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-fuse-ld=bfd" cargo build --release

%install
./packaging/stage.sh %{buildroot} %{_prefix}

%post
update-desktop-database -q %{_datadir}/applications || :
gtk-update-icon-cache -q -t -f %{_datadir}/icons/hicolor || :

%postun
update-desktop-database -q %{_datadir}/applications || :
gtk-update-icon-cache -q -t -f %{_datadir}/icons/hicolor || :

%files
%license LICENSE
%doc README.md
%{_bindir}/xime-daemon
%{_bindir}/xime-launcher
%{_bindir}/xime-setup
%{_datadir}/xime/
%{_datadir}/dbus-1/services/org.xime.Xime.service
%{_datadir}/applications/xime-launcher.desktop
%{_datadir}/applications/xime-setup.desktop
%config(noreplace) %{_sysconfdir}/xdg/autostart/xime-daemon.desktop
%{_datadir}/icons/hicolor/*/apps/xime.png

%changelog
* Fri Oct 23 2026 Kor1 <kingzcheung@gmail.com> - 0.2.0-1
- Initial RPM packaging
