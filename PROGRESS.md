# XIME-Wayland 开发进度

## 当前状态
**xime-server HTTP API 服务已实现，支持局域网剪切板同步。**

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

10. **单元测试体系建立**
    - xime-xkb: 34 个测试（KeyBinding 解析、modifier 匹配、keysym 转换）
    - librime: 21 个测试（KeyEvent from_char、Traits builder pattern）
    - xime-config: 17 个测试（配置解析、颜色方案、合并逻辑）
    - xime-ui: 23 个测试（CandidateList 分页、导航、选择逻辑）
    - xime-tray: 8 个测试（状态切换、颜色设置）
    - 总计: 103 个测试，覆盖核心纯函数逻辑

11. **局域网剪切板同步服务 (xime-server)**
    - 基于 Axum 的 HTTP REST API 服务
    - 配对功能：6 位配对码 + HMAC-SHA256 Token 认证
    - 剪切板读写：hash 去重防循环
    - 配对持久化：`~/.config/xime/pairs.json`
    - 端口：16888（硬编码，待配置化）
    - API Endpoints：
      - `POST /pair/request` - 发起配对请求
      - `GET /pair/status?code=xxx` - 查询配对状态
      - `POST /pair/confirm` - 确认配对
      - `GET /pair/list` - 列出已配对设备
      - `POST /pair/remove/{device_id}` - 移除设备
      - `GET /clipboard/read` - 读取剪切板（需 Token）
      - `POST /clipboard/write` - 写入剪切板（需 Token）
      - `GET /health` - 健康检查

## 待解决问题
1. **阴影效果优化** - 当前使用简单偏移阴影，可考虑添加模糊效果

## 待完成功能
1. 验证候选窗口使用正确的主题颜色
2. 验证托盘图标使用正确的主题颜色
3. 验证 Ctrl 键显示字根功能

## 技术栈
- `wayland-client` + `wayland-protocols` - Wayland IM 协议
- `xkbcommon 0.9` - 键码转换
- `librime` - 输入法引擎
- `cosmic-text` - 文本渲染
- `tiny-skia` - 背景/形状绘制
- `zbus` - DBus 通信（系统托盘）
- `serde_yaml` - 配置文件解析
- `axum` + `tower-http` - HTTP REST API 服务
- `hmac` + `sha2` - Token 签名认证

## 测试覆盖
- **xime-xkb**: 45 tests - KeyBinding 解析、keysym 转换、XKB Error 类型
- **librime**: 51 tests - KeyEvent、Traits builder、Error 类型、Status/Context/Commit 结构化测试
- **xime-config**: 46 tests - 配置解析、合并逻辑、schema 解析、config 合并、rime 配置提取
- **xime-ui**: 37 tests - CandidateList 状态机、渲染器、root_display 绘制、blend 边界
- **xime-tray**: 28 tests - 状态切换、颜色设置、文字图标渲染、MenuAction、rounded_rect
- **xime-wayland**: 17 tests - IM 状态机、错误类型、KeyEvent 数据
- **xime-daemon**: 6 tests - DaemonCommand 枚举、get_config_dir
- **xime-predict**: 17 tests (1 个预先存在失败：test_predict_basic 分数断言)
- **xime-setup/launcher/pack**: 0 tests (UI 密集型，需集成测试)
- **总计**: ~247 tests

## 下一步
1. 测试主题颜色实时更新功能（重新安装后验证）
2. 测试 Ctrl 键字根显示功能
3. 考虑为 xime-daemon/xime-launcher 添加集成测试
4. 修复 xime-predict 中 test_predict_basic 的分数范围断言
5. 添加 xime-setup 组件测试（state.rs, theme.rs）

## 剪切板同步待完成功能
1. **托盘集成**：在 `xime-tray` 添加配对确认菜单
   - 收到配对请求时弹出菜单："xxx 设备请求配对，码 123456，允许？"
   - 添加"已配对设备"菜单项，支持查看/移除
2. **剪切板读写**：接入 `zbus` 读取/写入桌面剪切板
   - 使用 `org.freedesktop.portal.Desktop` clipboard portal
   - 或使用 `arboard` crate 直接读写
3. **mDNS 发现**：添加 `mdns-sd` crate
   - PC 发布 `_xime._tcp` 服务
   - 手机扫描局域网发现 PC
4. **端口配置**：从 `xime.yaml` 读取端口
   - 配置项：`server.port: 16888`
   - 配置项：`server.enabled: true/false`
5. **词库同步接口预留**：为未来扩展预留 `/dict/*` 路由