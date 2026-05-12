## 项目概述
一个用纯 Rust 实现一个 Linux 五笔输入法，直接对接 Wayland 和 librime，自绘 UI。

## 核心原则
- **无框架依赖**：不引入 fcitx5/ibus，不写插件，一切自行掌控。
- **纯 Rust 实现**：主程序、状态管理、事件循环、UI 渲染全部用 Rust。
- **直接调用 librime**：通过 `extern "C"` FFI 复用已有五笔方案和词典。
- **直面 Wayland**：自行实现 `zwp_input_method_v2` 协议，仅支持 Wayland。
- **自绘 UI**：候选窗完全自绘，不依赖系统输入法面板。

## 架构概要

程序采用 **daemon + launcher** 分离架构，符合 KDE VirtualKeyboard 规范：

- **xime-daemon**：持续运行的守护进程
  - DBus 服务：`org.xime.Xime.Controller`
  - Rime 引擎：拼音/五笔转换
  - Wayland 事件循环：处理按键
  - 候选窗口：自绘 SHM buffer

- **xime-launcher**：轻量启动器
  - 接收 KWin 传递的 `WAYLAND_SOCKET` fd
  - 通过 DBus 将 fd 传递给 daemon
  - 保持运行（KWin 要求）

数据流:
```
KWin → WAYLAND_SOCKET fd → launcher → DBus → daemon →
Wayland 连接 → 按键事件 → Rime → 候选词 → 提交文本
```

## 模块要点
- **Wayland 协议层**：使用 `wayland-client` 实现 `zwp_input_method_v1`（KWin 支持），通过 `connect_from_fd()` 从 WAYLAND_SOCKET 连接。
- **键码转换**：通过 `xkbcommon` 将 Wayland 原始键码转换为 keysym。
- **Rime 集成**：直接调用 librime C API，配置目录 `~/.config/xime/rime/`。
- **UI 层**：SHM buffer 自绘候选窗口，`zwp_input_panel_surface_v1.set_overlay_panel()` 定位。
- **DBus 服务**：`org.xime.Xime.Controller` 提供 `OpenWaylandSocket(fd, display)` 方法。

## 安装与测试
```bash
# 开发安装（安装到 ~/.local）
./dev-install.sh

# 系统安装（安装到 /usr）
sudo ./install.sh

# 测试流程
# 1. 重新登录 KDE Plasma
# 2. 打开 Kate，输入拼音
# 3. KWin 应启动 launcher → daemon
```

## 开发路线
1. ✅ 搭 Wayland 键盘通道：接收 `activate`/`deactivate`/`key` 事件
2. ✅ 集成 librime：初始化引擎，模拟按键获取候选词
3. ✅ 实现架构分离：daemon + launcher + DBus 服务
4. ✅ 候选窗口基础：SHM buffer 绘制，overlay_panel 定位
5. 🔄 测试完整流程：KWin → launcher → daemon → 按键处理
6. 待完成：文本渲染、光标跟随、样式打磨

## 关键依赖
- `wayland-client`、`wayland-protocols`、`wayland-backend`
- `xkbcommon`
- `librime`
- `zbus` (DBus)
- `nix` (mmap, fd handling)

## 注意事项
- 仅支持 Wayland，完全不兼容 X11。
- 需自行处理焦点跟踪和组词状态，代码量约 300-500 行 Rust。
- librime 方案文件部署在 `~/.config/xime/rime/`。


## 工作规则
- 每次只做一个功能点
- 当前功能点端到端验证通过后，才能开始下一个
- 不要在实现功能 A 时"顺便"重构功能 B
- 当觉得有必要时，就添加单元测试


## 每次会话开始时（上班打卡）
1. 读 PROGRESS.md 了解当前状态
2. 读 DECISIONS.md 了解重要决策
3. 跑 `./cargo build --quiet` 确认仓库处于一致状态
4. 从 PROGRESS.md 的"下一步"部分继续工作

## 每次会话结束前（下班打卡）
1. 更新 PROGRESS.md
2. 跑 `./cargo build --quiet` 确认一致状态
3. 提交所有已完成的工作
