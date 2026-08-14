# XimeChe（曦码·澈输入法）重要决策

## 2026-08-15: 应用元数据参数化（AppMetadata）

**问题**：libximecore 是跨平台共享库，内部硬编码 "Xime"/"xime" 名称（配置路径、librime distribution 标识），无法被不同宿主平台复用。

**决策**：
1. xime-config 新增 `AppMetadata` 结构（metadata.rs）：`display_name` / `config_dir_name` / `config_file_base` / `distribution_name` / `distribution_code_name` / `app_name` / `version`
2. `set_app_metadata()` 全局注入，未注入时默认 "Xime" 保持兼容
3. 参数化范围**仅限核心**：配置路径（`~/.config/xime`、`/usr/share/xime`）、librime distribution/app 标识、`xime.yaml` 配置文件名、`Xime::SchemaConfigManager` generator_id
4. xime-setup UI 文案（"Xime 设置"、"关于 Xime" 等）**不改**，避免组件签名大规模改动
5. librime `deploy_all` 新增 `deploy_all_with_config(config_name)`，不破坏原签名
6. XimeChe 注入：`config_dir_name` 沿用 `xime`（兼容既有安装），`distribution_name = XimeChe`，显示名"曦码·澈输入法"

**理由**：
1. 目录沿用 xime 保证老用户配置/词库无缝迁移
2. 只参数化功能相关的名称，UI 文案属于显示层，各平台可自行维护
3. librime 是底层 crate，不能依赖 xime-config，故用独立函数参数而非全局状态

## 2026-08-15: 多合成器适配（v2 协议 + 后端抽象）

**问题**：当前只适配 KWin 的 `zwp_input_method_v1`，GNOME 45+ 只暴露 v2。

**决策**：
1. 完整实现 `zwp_input_method_v2` 后端（参考 fcitx5 waylandimserverv2）
2. 新增 `ImBackend` trait 抽象 v1/v2，daemon 用 `Box<dyn ImBackend>` 操作
3. 连接策略：
   - launcher（KWin）：fd 传入后探测 global，优先 v1，无则 v2
   - 直接连接（GNOME）：daemon 启动时连 `$WAYLAND_DISPLAY`，检测 v2
4. 按键转发：v2 无 forward_key 请求，用 `zwp_virtual_keyboard_v1.key()`（fcitx5 同款）
5. 提交语义：v2 请求 double-buffered，必须 `commit(serial)`（serial=done 计数）
6. 候选窗：v2 用 `zwp_input_popup_surface_v2`（合成器锚定光标），v1 保持 overlay_panel

**理由**：
1. GNOME（mutter 45+）、KWin 6、wlroots（Sway/Hyprland）都暴露 v2，一份后端覆盖大多数桌面
2. fcitx5 验证过的方案，避免踩 v2 语义坑（grab 事件、vk 转发、unavailable 重建）
3. GNOME 无 VirtualKeyboard/launcher 机制，必须支持 daemon 直接连接普通 socket

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

## 2025-05-15: 配置系统架构

**问题**：主题颜色修改后不生效，需要重启 daemon

**决策**：创建独立的 `xime-config` crate + DBus `ReloadStyle` 方法

**架构**：
1. `xime-config` crate：共享配置模块，所有 crate 依赖
2. 配置合并：系统默认 (`/usr/share/xime/xime.yaml`) + 用户覆盖 (`~/.config/xime/xime.yaml`)
3. 内置默认：编译时嵌入，确保系统配置缺失时有 fallback
4. DBus IPC：`ReloadStyle` 方法通知 daemon 重新加载配置

**理由**：
1. 共享模块避免代码重复
2. 配置合并允许系统级和用户级配置共存
3. DBus IPC 实现无需重启的实时更新

## 2025-05-15: font_size 类型修复

**问题**：用户配置 `font_size: 14.0` 解析失败

**原因**：`StyleConfig.font_size: i32` 不接受浮点数

**决策**：`font_size` 改为 `f32` 类型

**理由**：
1. YAML 中 `14.0` 是浮点数，`serde_yaml` 严格类型检查
2. `f32` 兼容整数和浮点数输入
3. 后续可支持小数字号（如 14.5）