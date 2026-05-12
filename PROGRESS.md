# XIME-Wayland 开发进度

## 当前状态
**修复了 zwp_input_method_v1 panic，等待重新登录测试。**

## 发现并修复的关键问题
1. **wayland-client panic** - daemon 在收到 activate 事件时崩溃
   - 错误：`Missing event_created_child specialization for event opcode 0 of zwp_input_method_v1`
   - 原因：activate 事件创建 `ZwpInputMethodContextV1` 子对象，但 Dispatch 没有正确处理
   - 修复：使用 `event_created_child!` 宏声明子对象类型

## 下一步测试
1. 重新登录 KDE Plasma
2. 打开 Kate，点击输入框
3. 检查是否有候选窗口和按键响应

## 待完成功能
1. **系统托盘图标** - 用户需求，需要实现 StatusNotifierItem 协议
2. **候选窗口渲染** - 当前只有彩色块，需要文本渲染
3. **完整按键处理** - 验证 Rime 输入流程

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