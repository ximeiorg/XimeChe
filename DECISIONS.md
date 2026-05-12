# XIME-Wayland 重要决策

## 2025-05-12: 架构分离决策

**问题**：单进程架构无法满足 KDE VirtualKeyboard 要求
- KWin 通过 `WAYLAND_SOCKET` fd 启动输入法
- `zwp_input_method` 协议不暴露在普通 `wayland-0` socket 上
- 需要按需启动机制

**决策**：采用 daemon + launcher 分离架构
- launcher：接收 `WAYLAND_SOCKET`，通过 DBus 传递 fd
- daemon：DBus 服务，按需激活，处理 Wayland 事件

**理由**：
1. fcitx5 使用相同架构（fcitx5-wayland-launcher）
2. DBus 按需激活符合 KDE VirtualKeyboard 规范
3. 分离后 launcher 轻量，daemon 可持续运行

## 2025-05-12: DBus Service 配置

**问题**：launcher 调用 `org.xime.Xime.OpenWaylandSocket` 时找不到服务

**决策**：创建 DBus service 文件
- 位置：`resources/dbus/org.xime.Xime.service.in`
- 安装：`~/.local/share/dbus-1/services/` 或 `/usr/share/dbus-1/services/`
- DBus 会按需激活 daemon

**理由**：DBus 激活机制要求 service 文件定义服务名称和可执行路径

## 2025-05-12: zwp_input_method_v1 vs v2

**决策**：优先支持 v1 协议（KWin）

**理由**：
1. KWin 5.27 使用 `zwp_input_method_v1`
2. `WAYLAND_SOCKET` fd 是单次使用，必须先尝试 v1
3. v2 用于 Sway/Hyprland，作为备选

## 2025-05-12: 候选窗口定位

**决策**：使用 `zwp_input_panel_surface_v1.set_overlay_panel()`

**理由**：
1. compositor 自动定位在光标附近
2. 不需要手动计算坐标
3. fcitx5 使用相同方法