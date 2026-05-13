# XIME-Wayland

一个用纯 Rust 实现的 Linux 五笔输入法，直接对接 Wayland 和 librime，自绘 UI。

## 特性

- **纯 Rust 实现**：主程序、状态管理、事件循环、UI 渲染全部用 Rust
- **无框架依赖**：不引入 fcitx5/ibus，不写插件，一切自行掌控
- **直接对接 Wayland**：自行实现 `zwp_input_method_v1` 协议（KWin 支持）
- **librime 集成**：通过 C FFI 复用已有五笔方案和词典
- **自绘 UI**：候选窗完全自绘，不依赖系统输入法面板
- **系统托盘**：支持 StatusNotifierItem 协议，显示输入状态
- **设置界面**：独立的 GPUI 设置程序，支持方案切换、外观配置

## 架构

采用 **daemon + launcher** 分离架构，符合 KDE VirtualKeyboard 规范：

```
xime-wayland/
├── crates/
│   ├── librime-sys/      # librime C API FFI 绑定
│   ├── librime/          # librime Rust 高级封装
│   ├── xime-wayland/     # Wayland 协议层 (zwp_input_method_v1)
│   ├── xime-xkb/         # 键码转换层 (xkbcommon)
│   ├── xime-ui/          # 候选窗渲染 (tiny-skia + cosmic-text)
│   ├── xime-tray/        # 系统托盘 (StatusNotifierItem + DBusMenu)
│   ├── xime-daemon/      # 守护进程（DBus 服务 + Rime + Wayland）
│   ├── xime-launcher/    # KWin 入口（传递 WAYLAND_SOCKET fd）
│   └── xime-setup/       # 设置界面 (GPUI)
├── resources/
│   ├── dbus/             # DBus 服务文件
│   ├── applications/     # 桌面入口文件
│   └── icons/            # 图标
├── package/              # Debian 打包配置
├── dev-install.sh        # 开发安装脚本
└── build-deb.sh          # Debian 包构建脚本
```

### 数据流

```
KWin → WAYLAND_SOCKET fd → launcher → DBus → daemon →
Wayland 连接 → 按键事件 → Rime → 候选词 → 提交文本
```

## 依赖

### 系统依赖

- `librime` (>= 1.8) - Rime 输入法引擎
- `libxkbcommon` (>= 1.0) - 键盘布局处理
- `rime-data-wubi` (推荐) - 五笔方案数据

安装方法：

```bash
# Ubuntu/Debian
sudo apt install librime-dev libxkbcommon-dev rime-data-wubi

# Arch Linux  
sudo pacman -S librime xkbcommon rime-data-wubi

# Fedora
sudo dnf install librime-devel xkbcommon-devel
```

## 安装

### 从 Debian 包安装

```bash
# 下载 deb 包后安装
sudo dpkg -i xime_0.1.0_amd64.deb
```

### 开发安装

```bash
./dev-install.sh
```

安装后需要重新登录 KDE Plasma 或重启 KWin。

## 配置

Rime 方案文件部署在 `~/.config/xime/rime/`。

首次运行会自动创建配置目录和默认配置：

```
~/.config/xime/rime/
├── default.custom.yaml   # 方案列表配置
├── xime.yaml             # xime 配置
├── wubi86_jidian.schema.yaml  # 五笔方案
└── build/                # 编译后的方案
```

### 设置界面

托盘菜单点击"设置..."打开设置界面，或直接运行：

```bash
xime-setup
```

设置界面功能：
- 输入方案选择和详细配置
- 外观设置（字体大小、候选词数量等）
- 词库管理
- 关于信息

## 构建

### 开发构建

```bash
cargo build --release
```

### 构建 Debian 包

使用 cargo-deb：

```bash
# 安装 cargo-deb
cargo install cargo-deb

# 构建
cargo build --release
cd package && cargo deb --no-build
```

或使用脚本：

```bash
./build-deb.sh
```

生成的 deb 包位于 `target/debian/xime_0.1.0_amd64.deb`。

## 运行

需要运行在支持 `zwp_input_method_v1` 协议的 Wayland 混成器环境中：

- **KDE Plasma (KWin)** - 完整支持，通过 VirtualKeyboard 机制
- GNOME (Mutter) - 需使用 `zwp_input_method_v2`（当前版本不支持）
- wlroots-based compositors - 需适配

### 测试流程

1. 安装后重新登录 KDE Plasma
2. 打开 Kate 或其他文本编辑器
3. 点击文本区域，KWin 会自动启动 xime
4. 系统托盘显示输入法图标
5. 输入五笔编码，候选窗显示候选词

## FAQ

### Q: 为什么 VSCode 无法输入中文？

A: VSCode 需要以 Wayland 模式启动。官方网站下载的 VSCode 可以正常使用，但 Snap Store 版本不支持。

### Q: 如何切换中英文？

A: 点击托盘图标，或使用右键菜单"切换中英文"。默认快捷键为 Shift。

### Q: 如何添加新的输入方案？

A: 将方案文件复制到 `~/.config/xime/rime/`，然后在设置界面选择，或手动编辑 `default.custom.yaml`。

## 项目状态

已完成：
- librime FFI 绑定和封装
- Wayland 输入法协议实现 (zwp_input_method_v1)
- 键码转换模块
- 候选词 UI 渲染
- 系统托盘支持
- DBus 服务 (org.xime.Xime.Controller)
- daemon + launcher 分离架构
- 设置界面 (GPUI)
- Debian 打包

待完成：
- 光标跟随
- 更多方案支持
- 快捷键自定义

## 许可证

GPL-3.0-or-later

## 参考

- [wayland-rs](https://smithay.github.io/wayland-rs/)
- [librime](https://github.com/rime/librime)
- [Wayland 输入法协议实现现状](https://zhuanlan.zhihu.com/p/22611314767)
- [Using Fcitx 5 on Wayland](https://fcitx-im.org/wiki/Using_Fcitx_5_on_Wayland/zh-cn)
- [GPUI](https://github.com/zed-industries/zed)