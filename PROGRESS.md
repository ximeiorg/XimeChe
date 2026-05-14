# XIME-Wayland 开发进度

## 当前状态
**Ctrl 键显示字根功能已实现，使用 set_toplevel 让窗口显示在屏幕底部。**

## 已完成功能
1. **候选栏绘制**
   - 圆角边框（8px 圆角，2px 边框）- tiny-skia 实现
   - 第一个候选词紫色背景高亮（0x8F73E2，圆角）
   - 候选词横向单行显示
   - 抗锯齿文本渲染（cosmic-text）
   - 阴影效果（偏移阴影）
   - 候选栏宽度动态计算（根据候选词内容）

2. **按键处理**
   - Rime 按键处理正确
   - 非输入按键（退格等）正确转发给应用
   - Shift 键切换中英模式（Shift_L inline_ascii，Shift_R commit_text）
   
3. **Wayland 集成**
   - zwp_input_method_v1 协议正确实现
   - 候选窗口刷新（damage_buffer）
   - 键盘 grab 和 keymap 加载
    
4. **修复的问题**
    - wayland-client panic（event_created_child 宏）
    - xkbcommon panic（升级到 0.9.0）
    - 按键转发问题
    - 候选词刷新问题
    - 输入编码延迟问题（preedit/commit 请求没有立即 flush）
    - Shift 键卡死问题（移除嵌套 sync_roundtrip、hide_candidate_window 后 flush、减少 sleep 间隔）
    - 颜色显示问题（tiny-skia 使用 RGBA 格式，需要转换成 BGRA/ARGB）

5. **渲染重构**
   - 移除 slint UI 依赖
   - 使用 cosmic-text 进行文本渲染
   - 使用 tiny-skia 进行背景绘制（圆角、边框）

6. **系统托盘**
   - 实现 StatusNotifierItem 协议（xime-tray crate）
   - 左键点击切换中英文模式（使用 Rime set_option API）
   - 右键菜单：切换中英文、重新部署、退出
   - 图标显示 "ZH"（紫色背景）或 "EN"（灰色背景）文字
   - tooltip 显示当前模式（中文输入/英文输入）
   - 模式随 Rime ascii_mode 状态同步变化
   - 托盘图标仅在输入法激活时显示（通过 NewStatus 信号控制 Active/Passive）

7. **Rime 配置**
   - 用户配置目录：`~/.config/xime/rime`
   - 支持右键菜单"重新部署"加载用户配置

8. **Ctrl 键显示字根功能**
   - 当候选框可见时，按下 Ctrl 显示最后输入键的字根
   - 例如：输入 "a" 后，按 Ctrl 显示 "a: 工匚戈艹廿龷七弋戈"
   - 使用 `set_toplevel(output, center_bottom)` 让窗口显示在屏幕底部
   - 松开 Ctrl 自动隐藏

## 待解决问题
1. **阴影效果优化** - 当前使用简单偏移阴影，可考虑添加模糊效果

## 待完成功能
1. **完整测试流程** - 验证 Ctrl 字根显示功能

## 技术栈
- `wayland-client` + `wayland-protocols` - Wayland IM 协议
- `xkbcommon 0.9` - 键码转换
- `librime` - 输入法引擎
- `cosmic-text` - 文本渲染
- `tiny-skia` - 背景/形状绘制
- `zbus` - DBus 通信（系统托盘）

## 下一步
1. 测试 Ctrl 字根显示功能
2. 验证窗口显示位置是否正确