## 项目概述
一个用纯 Rust 实现一个 Linux 五笔输入法，直接对接 Wayland 和 librime，自绘 UI。

## 核心原则
- **无框架依赖**：不引入 fcitx5/ibus，不写插件，一切自行掌控。
- **纯 Rust 实现**：主程序、状态管理、事件循环、UI 渲染全部用 Rust。
- **直接调用 librime**：通过 `extern "C"` FFI 复用已有五笔方案和词典。
- **直面 Wayland**：自行实现 `zwp_input_method_v2` 协议，仅支持 Wayland。
- **自绘 UI**：候选窗完全自绘，不依赖系统输入法面板。

## 架构概要

程序由四个核心模块组成，由 Rust 主循环统一调度：

- **Wayland 协议层**：通过 `zwp_input_method_v2` 从混成器接收按键事件、焦点事件，并提交文本。
- **键码转换层**：通过 `xkbcommon` 将原始键码转换为通用键值。
- **Rime 引擎层**：通过 C FFI 调用 librime，进行五笔转换、候选词获取和词典管理。
- **UI 渲染层**：自绘候选词窗口，负责光标跟随、选词和高亮。

数据流方向:
`Wayland 按键事件 → xkbcommon 转码 → Rime 处理 → 获取候选词/提交文本 → Wayland 提交 + UI 渲染`

## 模块要点
- **Wayland 协议层**：使用 `wayland-client` + `wayland-protocols` 实现 `zwp_input_method_v2`，负责接收按键事件、管理焦点、提交文本和预编辑字符串。
- **键码转换**：通过 `xkbcommon` crate 将 Wayland 原始键码转换为 keysym 和 Rime 所需格式。
- **Rime 集成**：直接调用 librime 的 C API（`rime_get_api` → `setup` → `initialize` → `create_session` → `process_key` → `get_context`），配置目录指向五笔方案。
- **状态管理**：轻量 `struct` 维护当前激活状态、预编辑文本、候选词列表、提交文本。
- **UI 层**：用 `slint` 绘制无焦点弹出候选窗，实现光标跟随和选词交互。
- **主循环**：Rust `async` 或简单轮询，将 Wayland 事件分发给各模块处理。

## 开发路线
1. 搭 Wayland 键盘通道：接收 `activate`/`deactivate`/`key` 事件，打印键码。
2. 集成 librime：初始化引擎，模拟按键获取候选词，打印到终端。
3. 串联数据流：按键 → 转码 → Rime 处理 → `commit_string` 上屏（无 UI）。
4. 实现候选窗 UI：显示候选词，支持选词和翻页。
5. 光标跟随、样式打磨、打包发布。

## 关键依赖
- `wayland-client`、`wayland-protocols`(https://smithay.github.io/wayland-rs/)
- `xkbcommon` ([Rust crate](https://docs.rs/xkbcommon/0.9.0/xkbcommon/))
- `librime`
- UI：`slint`

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
