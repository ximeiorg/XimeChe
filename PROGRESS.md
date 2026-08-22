# XimeChe（曦码·澈输入法）开发进度

## 当前状态
**菜单面板升级为路由容器：表情/符号网格直接铺在面板区（不占候选栏），剪切板/快捷发送置灰**（2026-08-22）

## 本次变更（2026-08-22）
1. **修复内容网格宽度不足**：单元格固定 36px 导致颜文字换行、排列错位
   - 按最宽项估算单元格宽（`content_text_width`：ASCII 10px/CJK 17px/零宽组合符 0，保守偏大 +16 内边距）
   - 列数在 660px 上限内自适应（4..=10 列）；面板宽度随内容变化（颜文字页 7 列 ≈645px，纯符号 10 列 414px）
   - 单元格文本 `Wrapping::None` 禁止换行兜底；渲染/命中测试共用同一纯函数保证一致
2. **面板路由化**（PanelView 状态机）
   - 面板区不再是纯菜单，而是可路由内容容器：`Closed / Menu(入口网格) / Content(表情或符号网格)`
   - 点「表情」「符号」→ 面板区直接铺开 10 列 × 3 行网格（不再走候选栏候选位）
   - 内容打开时点菜单按钮 → 路由回菜单视图；Esc → 关闭面板恢复候选栏
   - 点网格项直接上屏且面板保持打开（可连续选择）；数字键/Tab/方向键/回车均可选；`;` 上屏分号
   - 输入字符实时过滤（表情走插件搜索；符号走分类/关键词/字符匹配）；↑↓ 翻页（每页 30）
2. **xime-ui 内容网格**（menu.rs / iced_view.rs）
   - `PanelView`、`GridItem`、`content_capacity/content_panel_height/content_panel_width/content_item_hit` + 命中测试
   - `content_grid` 渲染：固定 10×3 网格，空位留白，高亮项着色；draw_panel 改收 `&PanelView`
3. **xime-wayland 面板视图**：v1/v2 状态 `menu_open: bool` → `panel_view: PanelView`；
   `show_content_panel()` 新接口；show_candidate_window 按视图决定增高（菜单 88px / 内容 120px）与最小宽度（内容 414px）
4. **daemon 路由状态**：`PanelState::ContentOpen`；SearchPanel 移除 page_size（每页 = 网格容量）；
   `show_content` 渲染（复用候选缓存，无缓存时空候选栏）；`redraw_menu_candidates` 无缓存时隐藏窗口
5. **测试**：content_item_hit/几何 3 个（xime-ui）、面板提交/翻页/符号模式 3 个（xime-daemon）

## 本次变更（2026-08-22）
1. **修复失焦后残留候选栏阻塞输入**（wayland.rs deactivate 分支）
   - 原 bug：切换窗口/输入框时只置标志 `candidate_window_visible = false`，从未调用 `hide_candidate_window()`
   - 残留候选栏 surface 继续显示并接收指针事件，吞掉新输入框的点击 → text-input 不重新 enable → 输入法"再也无法切换出来"
   - 修复：deactivate 时立即隐藏候选栏/菜单面板/Ctrl 字根窗口、关闭 emoji 面板、清空按键消费记录
   - 底层另有 KWin/Chromium text-input-v3 失焦失效 bug（KWin 493098，fcitx5 同样受影响，Chromium 139 才修复），本修复保证点击输入框即可恢复
2. **菜单入口接通**（此前点击只关菜单、无任何动作）
   - 「表情」：复用 `;` 触发的搜索面板（插件数据源）
   - 「符号」：新增 `crates/xime-daemon/src/symbols.rs` 内置符号表（17 类约 2000 符号，数据来自 Xime 安卓版 SymbolData.kt），支持分类/关键词/字符搜索
   - 「剪切板」「快捷发送」：未实现，`is_available() == false` 置灰显示、点击保持菜单打开
3. **SearchPanel 双模式改造**（wayland.rs）
   - `PanelMode::{Emoji, Symbols}`：表情模式第 1 位分号保留位、翻页每页 page_size-1；符号模式无保留位、每页 page_size
   - 符号模式刷新全量加载（分页浏览），表情模式维持 3 页上限
   - 菜单点击不再依赖按键循环，直接 `refresh_search_panel` 打开面板
4. **xime-ui 菜单置灰渲染**（iced_view.rs menu_cell）：不可用入口灰色 chip + 灰色文字
5. **单元测试**：symbols::search 5 个（空查询/分类 id/中文关键词/字符/top_k）、符号模式提交与翻页 1 个

## 本次变更（2026-08-15）
1. **菜单按钮改用用户提供的 SVG 图标**（crates/xime-ui/resources/menu.svg）
   - resvg/usvg 渲染（OnceLock 缓存），active 时着紫色、默认灰色
2. **菜单面板改为候选栏增高模式**（修复 KWin 下第二个 overlay panel 不显示）
   - 面板不再用独立 surface，而是候选栏 buffer 增高：上面面板、下面候选栏
   - 点击面板入口按 y 坐标命中（menu_item_hit 语义改为从面板顶部 0 起算）
   - daemon 缓存最近候选（candidate_cache），菜单开/关后立即重绘
3. **Wayland 指针事件接入**（crates/xime-wayland）
   - `PointerEvent` + v1/v2 绑定 `wl_pointer`，`pop_pointer_events()`
4. **菜单按钮 + 菜单面板**（crates/xime-ui/src/menu.rs）
   - 候选栏最右侧固定菜单按钮（36px）
   - 菜单面板（200x176）：表情/符号/剪切板/快捷发送 4 个入口
5. **daemon 面板状态机**：点击按钮开/关面板、点击入口进入功能页、按键自动关闭

## 本次变更（2026-08-15）
1. **插件热重载机制**
   - 设置程序插件变更（安装/卸载/启停）→ DBus `ReloadPlugins` → daemon `PluginHost::reload()`
   - 兜底：`;` 触发 emoji 面板前自动 reload，daemon 早于插件安装启动时也能用
   - libximecore 新增 `set_notify_reload_plugins` 回调，state.rs 插件任务完成后触发
2. **插件下载即安装**（libximecore xime-setup）
   - 插件下载改为下载到临时文件后立即 `install_from_zip`，删除 .xipk，不再需要手动"安装"
   - 修复 `download_file` 未创建父目录导致 "no such file or dir"
2. **daemon 插件宿主**（crates/xime-daemon/src/plugin_host.rs）
   - 新增 `PluginHost`：启动时扫描 `~/.config/xime/plugins/`，加载 enabled 插件（PluginRuntime + mlua 沙箱）
   - `query_emojis` / `emoji_plugin_count` 契约查询
   - 端到端测试：临时目录安装 .xipk → 加载 → 查询/搜索
3. **emoji 候选窗**（wayland.rs）
   - 中文态按 `;` 进入 emoji 面板（需已安装 emoji 类插件）
   - 字符实时搜索、BackSpace 删词、数字键选择上屏、Return/Space 高亮上屏、Escape 退出
   - `emoji_select_index` 纯函数 + 测试
4. **设置侧边栏新增"插件管理"页**（libximecore xime-setup）
   - 已安装插件列表：类型图标 + 名称 + 版本 + 启用开关 + 卸载（二次确认）

## 本次变更（2026-08-15）
1. **项目改名 XimeChe（曦码·澈输入法）**
   - git remote → `git@github.com:ximeiorg/XimeChe.git`
   - README/DECISIONS/PROGRESS 标题、Cargo.toml repository、desktop 文件中文名
2. **libximecore 应用元数据参数化**（crates/xime-config/src/metadata.rs）
   - 新增 `AppMetadata` + `set_app_metadata()`（默认 Xime 兼容）
   - 参数化配置路径（`~/.config/xime`、`/usr/share/xime`、`~/.local/share/xime`）、librime distribution/app 标识、`xime.yaml` 文件名、`Xime::SchemaConfigManager` generator_id
   - librime 新增 `deploy_all_with_config()`，保持 `deploy_all()` 兼容
   - xime-setup UI 文案未改（避免组件签名大规模改动）
3. **XimeChe 注入 metadata**（daemon + xime-setup 薄壳 main.rs）
   - 目录沿用 `xime`（兼容既有安装），distribution_name = XimeChe，显示名"曦码·澈输入法"

## 本次变更（2026-08-15）
1. **完整实现 zwp_input_method_v2 后端**（crates/xime-wayland）
   - 协议文件更新为上游版本：`input-method-unstable-v2.xml`（含 `zwp_input_popup_surface_v2`），新增 `virtual-keyboard-unstable-v1.xml`
   - 键盘 grab：`zwp_input_method_keyboard_grab_v2` 处理 keymap/key/modifiers/repeat_info
   - 按键转发：v2 无 forward_key，改用 `zwp_virtual_keyboard_v1.key()`（fcitx5 同款方案）
   - 提交语义：v2 请求是 double-buffered，commit_string/set_preedit_string 后需 `commit(serial)`，serial 为 done 事件计数
   - 候选窗：`zwp_input_popup_surface_v2`（合成器自动锚定文本光标）
   - `unavailable` 事件处理（GNOME 锁屏）：销毁并重建 input method 对象
2. **daemon 后端抽象**（crates/xime-daemon/src/wayland.rs）
   - 新增 `ImBackend` trait，v1/v2 统一接口，daemon 通过 `Box<dyn ImBackend>` 操作
   - `connect_im_from_fd(fd)`：launcher 模式，优先 v1（KWin 5.x），无则 v2（KWin 6/wlroots/GNOME）
   - `connect_im_to_env()`：新增直接连接模式，daemon 启动时连接 `$WAYLAND_DISPLAY`（GNOME 无 launcher 机制），KWin 下失败后等待 fd
3. **KeyEvent 提升到 lib.rs** 共享（v1/v2 同构）
4. **清理调试输出**：删除 im_v1.rs/renderer.rs 全部 13 处 `eprintln!`


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
2. **v2 实机验证** - 代码按 fcitx5 语义实现，需在 GNOME 45+ / KWin 6 / Sway 实机测试
3. **GNOME 自动启动** - daemon 直接连接模式已实现，但缺少 GNOME 下的自启动机制（autostart/systemd user unit）
4. **v2 按键重复** - grab 的 repeat_info 事件未驱动重复按键（与 v1 现状一致）

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
- **xime-ui**: 22 tests - CandidateList 状态机、渲染器、root_display 绘制、blend 边界、菜单/内容网格命中
- **xime-tray**: 28 tests - 状态切换、颜色设置、文字图标渲染、MenuAction、rounded_rect
- **xime-wayland**: 17 tests - IM 状态机、错误类型、KeyEvent 数据
- **xime-daemon**: 29 tests - DaemonCommand 枚举、get_config_dir、插件宿主、symbols 搜索、面板提交/翻页
- **xime-predict**: 17 tests (1 个预先存在失败：test_predict_basic 分数断言)
- **xime-setup/launcher/pack**: 0 tests (UI 密集型，需集成测试)
- **总计**: ~247 tests

## 下一步
1. **GNOME 实机验证 v2 后端**（输入、候选窗定位、Shift 切换、锁屏恢复）
2. GNOME 自启动机制（XDG autostart 或 systemd user unit，直接启动 xime-daemon）
3. 测试 xime-setup（Iced 版）单例锁 + DBus 通知 + SelectSchema 切换方案
4. 测试主题颜色实时更新功能（重新安装后验证）
5. 测试 Ctrl 键字根显示功能
6. 考虑为 xime-daemon/xime-launcher 添加集成测试
7. 修复 xime-predict 中 test_predict_basic 的分数范围断言
8. 添加 xime-setup 组件测试（state.rs, theme.rs）

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