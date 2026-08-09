# XIME-Wayland 开发进度

## 当前状态
**xime-setup 已迁移到 Iced（libximecore 侧），本项目适配薄壳二进制；ximed 已删除；rime-wubi 更新到 2.1.2。**

## 本次变更（2026-08-09）
1. **libximecore 同步到 Iced 版本**（`xime-setup-lib` 用 `iced` 重写，自带 `[[bin]]`）
   - 本地 libximecore pull 到 origin/main
   - 修复 libximecore `Cargo.toml`：`windows` 依赖改为 `[target.'cfg(windows)']`，解决 Linux 上 windows-future 0.3.2 编译失败
2. **xime-setup 薄壳适配**（crates/xime-setup）
   - 移除 gpui/gpui_platform 依赖，改用 `iced` + `xime_setup_lib::run()`
   - main.rs 保留单例锁和 DBus 通知，新增 `set_notify_select_schema` 回调
3. **daemon 新增 SelectSchema DBus 方法**（`org.xime.Xime.Controller`）
   - `DaemonCommand::SelectSchema(String, oneshot::Sender<bool>)`
   - `RimeEngine::select_schema()` 调用 librime `session.select_schema()`
4. **删除 ximed**（剪切板同步 HTTP 服务）
   - xime-daemon 移除 `ximed` 依赖和 server 启动代码（端口 8370）
5. **rime-wubi 子模块更新到 2.1.2**
   - 2.1.2 删除了 `xime.custom.yaml`、`xime.yaml`、`rime.lua`
   - install.sh/dev-install.sh 移除对 `xime.custom.yaml` 的安装
   - 新增 handwriting.schema.yaml、t9_pinyin.schema.yaml（随通配符自动安装）
6. **Rime 数据目录参数化，仅使用 rime-wubi 方案**（libximecore dd5d54e）
   - xime-config 新增 `set_rime_paths(RimePaths)` 接口，shared/user 目录由宿主应用注入，移除 `/usr/share/rime-data` 硬编码
   - daemon main.rs / setup 薄壳 main.rs 解析 rime-wubi 目录（dev: `~/.local/share/xime/rime-data`，系统: `/usr/share/xime/rime-data`）并注入
   - install.sh/dev-install.sh 把 rime-wubi 完整安装（含 default.yaml/symbols.yaml + lua/）到 shared 目录
   - 验证：部署与 setup 方案列表仅含 rime-wubi 9 个方案，无系统内置 stroke/luna_pinyin

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
12. **WebDAV 配置同步 (xime-setup)**
    - 新增"同步"设置页，位于侧边栏"快捷键"和"关于"之间
    - 提供 WebDAV 配置表单：服务器地址、用户名、密码
    - 配置保存到 `~/.config/xime/webdav.yaml`（权限 600）
    - "上传到服务器"按钮：将 `~/.config/xime/rime/` 打包为 tar.gz 上传
    - "从服务器下载"按钮：下载 tar.gz 并解压到 rime 目录
    - 下载前自动备份旧配置，解压失败自动恢复
    - 使用 `tar` 命令打包/解压，`reqwest::blocking` 进行 HTTP 请求
    - 文本输入通过 `zenity` 对话框实现

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
1. 测试 xime-setup（Iced 版）单例锁 + DBus 通知 + SelectSchema 切换方案
2. 测试主题颜色实时更新功能（重新安装后验证）
3. 测试 Ctrl 键字根显示功能
4. 考虑为 xime-daemon/xime-launcher 添加集成测试
5. 修复 xime-predict 中 test_predict_basic 的分数范围断言
6. 添加 xime-setup 组件测试（state.rs, theme.rs）

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