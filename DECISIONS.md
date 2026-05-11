# XIME-Wayland 架构决策

## 决策 1: 多 crate 架构
**日期**: 2026-05-11

**背景**: 需要决定如何组织代码结构。

**决策**: 采用多 crate 架构，按功能模块划分。

**理由**:
- 清晰的模块边界
- 便于独立测试
- 支持渐进开发
- 可复用性强

**模块划分**:
- `librime-sys`: FFI 绑定
- `librime`: Rime 引擎封装
- `xime-wayland`: Wayland 协议
- `xime-xkb`: 键码转换
- `xime-ui`: UI 渲染
- `xime-core`: 主程序

## 决策 2: 使用 Slint 作为 UI 框架
**日期**: 2026-05-11

**背景**: 需要自绘候选词窗口。

**决策**: 使用 Slint 框架。

**理由**:
- 纯 Rust 实现
- 支持无焦点窗口
- 声明式 UI
- 性能良好

## 决策 3: 仅支持 Wayland
**日期**: 2026-05-11

**背景**: 是否同时支持 X11 和 Wayland。

**决策**: 仅支持 Wayland。

**理由**:
- 简化实现
- X11 已是遗留技术
- 目标用户使用现代桌面环境

## 决策 4: 使用 zwp_input_method_v2 协议
**日期**: 2026-05-11

**背景**: Wayland 有多个输入法协议版本。

**决策**: 使用 zwp_input_method_v2 (unstable)。

**理由**:
- v2 是当前主流
- 得到主流混成器支持 (KWin, Mutter, wlroots)
- 提供 activate/deactivate/keypress 事件
- 支持 preedit/commit

## 决策 5: 配置目录
**日期**: 2026-05-11

**背景**: Rime 方案文件存放位置。

**决策**: 使用 `~/.config/xime/rime/`。

**理由**:
- 符合 XDG 规范
- 与其他输入法隔离
- 方便用户管理

## 决策 6: 键码转换使用 xkbcommon
**日期**: 2026-05-11

**背景**: Wayland 提供的是原始键码，需要转换为 keysym。

**决策**: 使用 xkbcommon crate。

**理由**:
- 标准做法
- wayland-client 推荐使用
- Rime 接受 XKB keysym

## 决策 7: 主循环使用同步模型
**日期**: 2026-05-11

**背景**: 选择事件循环模型。

**决策**: 使用同步阻塞模型，后续可考虑 async。

**理由**:
- 初始实现简单
- Rime API 是同步的
- Wayland 事件循环本就是同步的
- 后续可按需优化