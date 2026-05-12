# XIME-Wayland 开发进度

## 当前状态
**修复 DBus fd 传递问题，等待重新登录测试完整流程。**

## 发现问题
1. **DBus 服务未自动激活** - xime-daemon 没有随 launcher 调用自动启动
2. **fd 传递错误** - launcher 中报错 `Bad file descriptor (os error 9)`
   - 原因：WAYLAND_SOCKET fd 在当前测试环境无效（非 KWin 启动）
   - 修复：launcher 中也使用 `dup()` 复制 fd，并添加服务预激活

## 今日完成
1. **修复 launcher fd 处理** - 添加 `nix::unistd::dup()` 复制 fd
   - 文件：`crates/xime-launcher/src/main.rs`
   - 添加 DBus 服务预激活 (`StartServiceByName`)
2. **构建测试通过** - daemon 可手动启动并注册 DBus 服务

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