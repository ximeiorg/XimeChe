# XIME-Wayland 开发进度

## 当前状态
**托盘图标显示 ZH/EN 文字，点击切换中英文（发送 Shift_L 按下+释放）。**

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
   - 左键点击切换中英文模式（发送 Shift_L 按下+释放）
   - 图标显示 "ZH"（紫色背景）或 "EN"（灰色背景）文字
   - tooltip 显示当前模式（中文输入/英文输入）
   - 模式随 Rime ascii_mode 状态同步变化
   - 托盘图标仅在输入法激活时显示（通过 NewStatus 信号控制 Active/Passive）

## 待解决问题
1. **阴影效果优化** - 当前使用简单偏移阴影，可考虑添加模糊效果

## 待完成功能
1. **完整测试流程** - 验证输入、提交、候选选择、托盘切换

## 技术栈
- `wayland-client` + `wayland-protocols` - Wayland IM 协议
- `xkbcommon 0.9` - 键码转换
- `librime` - 输入法引擎
- `cosmic-text` - 文本渲染
- `tiny-skia` - 背景/形状绘制
- `zbus` - DBus 通信（系统托盘）

## 历史完成
1. **架构重构** - daemon + launcher 分离
2. **项目结构** - 资源文件纳入项目管理
   - `resources/dbus/org.xime.Xime.service.in` - DBus 服务模板
   - `resources/applications/xime-launcher.desktop.in` - Desktop 模板
3. **安装脚本**
   - `install.sh` - 系统安装（PREFIX=/usr）
   - `dev-install.sh` - 开发安装（~/.local）

## 项目结构
```
xime-wayland/
├── crates/
│   ├── xime-daemon/      # DBus 服务 + Rime + Wayland 循环
│   ├── xime-launcher/    # 接收 WAYLAND_SOCKET → DBus
│   ├── xime-wayland/     # Wayland 协议层
│   ├── xime-xkb/         # 键码转换
│   ├── librime/          # Rime 封装
│   └── librime-sys/      # Rime FFI
├── resources/
│   ├── dbus/             # DBus service 文件模板
│   └── applications/     # Desktop 文件模板
├── install.sh            # 系统安装脚本
├── dev-install.sh        # 开发安装脚本
└── PROGRESS.md
```

## 开发测试流程
```bash
# 1. 构建 + 安装到 ~/.local
./dev-install.sh

# 2. 重新登录 KDE Plasma

# 3. 打开 Kate 测试
```

## 安装位置
- Binary: `~/.local/bin/xime-daemon`, `~/.local/bin/xime-launcher`
- DBus: `~/.local/share/dbus-1/services/org.xime.Xime.service`
- Desktop: `~/.local/share/applications/xime-launcher.desktop`
- Config: `~/.config/kwinrc` (VirtualKeyboard=xime-launcher.desktop)

## 测试步骤
```bash
# 快速卸载重装测试
./uninstall.sh  # 选择重置 KWin 配置
./dev-install.sh
# 重新登录 KDE Plasma，打开 Kate 测试
```

## 下一步
1. 测试按键输入是否能正确处理
2. 验证候选窗口显示
3. 验证文本提交