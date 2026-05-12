# XIME-Wayland 开发进度

## 当前状态
**Wayland 连接成功！输入法核心流程已打通，待验证按键处理。**

## 今日完成
1. **修复 DBus fd 传递 Bug** - daemon 中 `zbus::Fd` 在方法返回后会关闭 fd
   - 问题：`unsafe { OwnedFd::from_raw_fd(fd.as_raw_fd()) }` 导致 "Bad file descriptor"
   - 修复：使用 `nix::unistd::dup()` 复制 fd 后再创建 `OwnedFd`
   - 文件：`crates/xime-daemon/src/main.rs`
2. **Wayland 连接成功** - `zwp_input_method_v1` 协议可用
   - 日志：`DEBUG: zwp_input_method_v1 available`
   - daemon 和 launcher 都在运行
   - 等待测试按键输入

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