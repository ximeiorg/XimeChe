# XIME-Wayland

一个用纯 Rust 实现的 Linux 五笔输入法，直接对接 Wayland 和 librime，自绘 UI。

## 特性

- **纯 Rust 实现**：主程序、状态管理、事件循环、UI 渲染全部用 Rust
- **无框架依赖**：不引入 fcitx5/ibus，不写插件，一切自行掌控
- **直接对接 Wayland**：自行实现 `zwp_input_method_v2` 协议
- **librime 集成**：通过 C FFI 复用已有五笔方案和词典
- **自绘 UI**：候选窗完全自绘，不依赖系统输入法面板

## 架构

```
xime-wayland/
├── crates/
│   ├── librime-sys/      # librime C API FFI 绑定
│   ├── librime/          # librime Rust 高级封装
│   ├── xime-wayland/     # Wayland 协议层 (zwp_input_method_v2)
│   ├── xime-xkb/         # 键码转换层 (xkbcommon)
│   ├── xime-ui/          # UI 渲染层 (Slint)
│   └── xime-core/        # 主程序入口
```

### 数据流

```
Wayland 按键事件 → xkbcommon 转码 → Rime 处理 → 
获取候选词/提交文本 → Wayland 提交 + UI 渲染
```

## 依赖

### 系统依赖

- `librime` - Rime 输入法引擎
- `libxkbcommon` - 键盘布局处理

安装方法：

```bash
# Ubuntu/Debian
sudo apt install librime-dev libxkbcommon-dev

# Arch Linux  
sudo pacman -S librime xkbcommon

# Fedora
sudo dnf install librime-devel xkbcommon-devel
```

### Rust 依赖

- `wayland-client` + `wayland-protocols` - Wayland 协议
- `xkbcommon` - 键码转换
- `librime` (通过 FFI) - 输入法引擎
- `slint` - UI 渲染

## 编译

```bash
cargo build
```

## 配置

Rime 方案文件部署在 `~/.config/xime/rime/`。

首次运行会自动创建配置目录，可在此放置五笔方案文件：

```bash
mkdir -p ~/.config/xime/rime
# 复制五笔方案文件
cp your-wubi-schema.yaml ~/.config/xime/rime/
```

## 运行

```bash
cargo run
```

需要运行在支持 `zwp_input_method_v2` 协议的 Wayland 混成器环境中：

- KDE Plasma (KWin)
- GNOME (Mutter)
- wlroots-based compositors (Sway, etc.)

## 项目状态

当前为架构搭建阶段，已完成：

- librime FFI 绑定和封装
- Wayland 输入法协议实现
- 键码转换模块
- 候选词 UI 组件
- 主程序框架

## 许可证

GPL-3.0-or-later

## 参考

- [wayland-rs](https://smithay.github.io/wayland-rs/)
- [librime](https://github.com/rime/librime)
- [Wayland 输入法协议实现现状](https://zhuanlan.zhihu.com/p/22611314767)
- [Using Fcitx 5 on Wayland](https://fcitx-im.org/wiki/Using_Fcitx_5_on_Wayland/zh-cn)