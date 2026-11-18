# XimeChe（曦码·澈输入法）重要决策

## 2026-11-17: 剪贴板存储只留一份（xime.db），本仓删掉自建实现

**问题**：libximecore `cda319b` 把剪贴板从独立的 `clipboard.db` 迁进统一库
`xime.db`（`xime-config::clipboard_store`，整库 schema v7）。XimeChe 还留着
自己的 v3 实现写 `~/.config/xime/clipboard.db`，于是：

- 设置程序（读 `xime.db`）的剪贴板历史停在迁移那一刻的快照；
- daemon（写 `clipboard.db`）的新条目在设置页永远看不到；
- 更隐蔽的是迁移会把旧文件改名成 `clipboard.db.migrated`，而 daemon 已经
  打开了那个 inode，之后的写入全部落进改名后的文件——`xime.db` 里一行都不会
  多出来，界面上只表现为「历史不更新」，不报任何错。

这与 macOS 版 D21（两套 schema 操作同一个库 → 历史一片空白）是同一类问题。

**决策**：

1. **存储归 libximecore，本仓只留平台层**。`xime-clipboard` 的 `store`
   模块改成转出 `xime_config::clipboard_store`（含 `init()` 单例），删掉
   自建的建表/迁移/裁剪实现；`watcher`（data-control 监听）与
   `system_clipboard`（写回）留在本仓。拆法与 XimeYi 一致。
2. **路径不再由本仓拼**。`init()` 调 `clipboard_store::default_db_path()`，
   与设置程序同一个函数——两边各算各的就会再次分叉。
3. **不再直接依赖 rusqlite**。存储来自 xime-config，顺带消掉了
   `libsqlite3-sys` 的 `links = "sqlite3"` 冲突隐患（见 2026-11-09 那次）。
4. **已滞留的数据手动补一次**。`clipboard.db.migrated` 里的条目不会被
   libximecore 的 `import_legacy_rows` 认领（它只看 `clipboard.db`），
   本机做了一次「补缺失 + 备份」的合并；对其它用户，等 libximecore 把
   `*.migrated` 也纳入导入，或在发布说明里提示。

**未做**：`ClipboardStore` 没有跨进程通知机制，daemon 在面板打开时才重读，
设置页删除条目后要重新进页才刷新——当前可接受，真要实时得走 inotify 或
DBus 广播。

## 2026-11-16: 语音输入暂时屏蔽，推理统一走 candle

**问题**：libximecore 把 `xime-speech` 的 sherpa-onnx 升到 2.0.1 后，
`sherpa-onnx-sys` 构建脚本要从 GitHub Releases 下载预编译包；本机网络不可达，
整个 workspace 的 `cargo check` 直接失败。项目已决定本地推理统一使用 candle
（`xime-infer`），sherpa-onnx 不再保留。

**决策**：

1. **彻底删除 sherpa-onnx，不做可选 feature**：它已经在默认构建路径上造成
   硬失败，留一个默认关闭的 feature 只会让问题在下次开启时重演。libximecore
   的 `xime-speech` 整块删除；XimeChe 不再声明 `xime-speech` 依赖。
2. **保留 daemon 的 speech API 占位，不删 wayland 接线**：
   `crates/xime-daemon/src/speech.rs` 保留 `SpeechEvent` / `SpeechState` /
   `toggle` / `drain_events` / `levels_snapshot` / `status_json` 的签名，
   内部只记日志、永不产生事件。这样 candle ASR 接入时只换实现，候选栏
   悬浮层、失焦清理、DBus/设置页的接线都不用重写。
3. **所有用户入口先移除**：设置页 `voice-page`、托盘「语音输入」、候选栏
   🎙️ 卡片、Ctrl+Alt+V 热键、听写模态分支、6 个 DBus 语音方法全部删掉。
   保留入口但点了没反应比入口消失更糟；恢复时随 candle 实现一起加回。
4. **打包不再携带 sherpa 运行库**：dev-install / stage.sh / deb assets /
   CI 的 `libsherpa-onnx*.so`、`libonnxruntime.so` 要求全部移除；daemon 的
   rpath 只保留 librime 子模块 dist。

## 2026-11-14: 按键重复必须由输入法自己合成

**问题**：用户报「按住方向键左右不能连续移动光标，只移动一格」。实机日志
显示 KWin 在 grab 期间**会**发重复按键（`state=Repeated`，~40ms 一条），但
输入法把 `KeyState::Repeated` 当成释放转发，应用只收到首按 + 一连串孤儿
释放。另一些合成器可能不发 `Repeated`，只给 `repeat_info(rate, delay)`。

**决策**：

1. **`Repeated` 按按下处理，并原样转发 `state = 2`**（首要修复）：
   v1/v2 的 `pressed` 同时匹配 `Pressed` / `Repeated`；`KeyEvent` 多带一个
   `repeated` 标志，`ImBackend::forward_key` 改为传 state——v1 原样把 2 转给
   `zwp_input_method_context_v1.key`（协议要求参数与 wl_keyboard 原事件一致），
   v2 的虚拟键盘只定义 0/1，压成按下。把 2 压成 1 时，客户端会把重复事件
   当成「同一个键又按了一次」而忽略，长按仍然不连续。
2. **保留 daemon 侧定时器作兜底**：`ImBackend::repeat_info()` 暴露
   `(rate, delay)`（未收到时默认 25Hz/600ms），`KeyRepeat` 记录按下键、
   serial、首按时间与下次触发时刻；主循环每 1ms 检查，到点用同一
   `handle_key_event` 管线重投 `pressed = true`。只在合成器**不发**
   `Repeated` 且 `rate > 0` 时生效；`rate = 0`（KWin 的 grab 通告）明确表示
   「重复由合成器负责」，此时不合成，避免与 `Repeated` 双发。
3. **兜底定时器只重复「未被输入法消费」的按下**：Ctrl+Space 启停、语音热键、
   面板导航这类按下若重复投递会变成连续触发。代价是组合态下按住字母/退格
   不会由定时器重复（KWin 的 `Repeated` 仍会照常送进 Rime）；方向键、
   非组合态退格等用户实际依赖的长按已覆盖。
4. **修饰键不参与重复，也不打断正在重复的键**：按住方向键再按 Shift 选择
   文本时，方向键必须继续重复；松开 Shift 也不影响。
5. **`rate = 0` 视为系统关闭重复**；`rate` 上限 200Hz、`delay` 限幅
   1..5000ms，异常值不能把主循环变成忙循环或让首重复永远不来。
6. **两种来源都兼容**：合成器发 `Repeated` 时由它驱动（每次重复重置我们的
   delay，定时器不触发）；不发时由 daemon 的定时器驱动。KWin 属于前一种，
   其 grab `repeat_info.rate = 0`，因此定时器不会介入。
7. 失焦、断连、重新激活时清空重复状态，避免把上一个窗口的按住键带过来。

## 2026-11-11: 剪贴板同步的通道契约、系统剪贴板与自动模式

**问题**：设置程序把剪贴板同步配置从单选 `transport` 换成了有序 `channels`
数组（可同时勾直连与插件 = 自动），daemon 仍只读 `transport`；同时远端内容
只进 `clipboard.db`，没有写回系统剪贴板；两台桌面同时跑 daemon 时，配对
输入侧与常驻节点抢 UDP 47822。

**决策**：

1. **`channels` 优先，旧字段只做兼容**：`channels` 非空时完全以它为准，
   为空时才回退 `transport` / `plugin_id`。存量服务器用户升级后行为必须
   零变化，所以旧路径的缺省仍是 plugin。
2. **自动模式不做健康窗口**：同时勾直连与插件时，`peer_connected` 为真就
   只走 P2P，否则立刻压给插件。ximed 的 5s/10s 窗口是为了避免远程服务
   来回切撞限流；剪贴板是单值资源，直连一断就延迟回退，等于把用户刚复制的
   内容关在本地——**宁可多走一次插件，也不要丢内容**。直连恢复后由
   `last_local_text` 补推断开期间的内容（节点缓存里没有）。
3. **系统剪贴板写回用 arboard**：与 ximed、libximecore 设置程序同一实现
   （Linux 走 wl-clipboard-rs 的 data-control），不改 watcher 的
   `blocking_dispatch` 循环去实现 data source。写回的 hash 进一次性回声
   窗口，watcher 再捕获时不推回对端；取走即失效，不影响用户之后复制同一
   段文本。以后若要去掉依赖，再在 watcher 连接上实现 `set_selection`。
4. **UDP 47822 用 SO_REUSEADDR 共享，而不是暂停 daemon 节点**：配对与
   常驻同步应当能并存；ximed 的「配对前 shutdown 节点」是临时绕行。
   修复放在 libximep2p 的 `Watcher::bind`，两端共用同一份库即可。
5. **插件通道 2s 定时拉取**（对齐 Android `ClipboardSyncBridge`）：插件是
   拉取式，自动模式退到插件后没有定时器就收不到远端内容；直连掉线时重置
   计时，下一轮立刻拉一次。

## 2026-10-27: 修饰键必须一对一转发，禁止任何缓存或条件门控

**问题**：修终端 Ctrl+C 时，在 daemon 里加了 `forward_modifiers`，并且
①用全局单值 `Option<u32>` 缓存"上次转发的修饰态"、②只在
`should_forward_key` 为真时顺带转发。结果把系统级 Ctrl 搞死了：用户报
"一切换到这个输入法，所有操作都带有 Ctrl 按下效果"。

**为什么错**：
1. KWin 会为多个窗口**同时**保留多个 `zwp_input_method_context_v1`。
   全局单值缓存意味着 Ctrl 的"按下"可能发给窗口 A、"松开"发给窗口 B，
   A 那侧永久残留 Ctrl 按下态。
2. 门控在"按键是否被转发"上：Rime 消费按键时整段跳过，修饰态变化就丢失。
   丢一次"松开"就是永久卡住——鼠标点击不产生按键事件，永远纠正不回来。
3. 修饰态是**每 context 独立**的状态，用任何全局快照去近似都是错的。

**决策**：在 `im_v1.rs` 的 `wl_keyboard::Event::Modifiers` 分支里，
**收到就转发**给当前 context，不缓存、不判断、不看按键是否被转发：

```rust
if let Some((ctx, serial)) = /* 当前 context + 其 commit_state serial */ {
    ctx.modifiers(serial, mods_depressed, mods_latched, mods_locked, group);
}
```

这就是 fcitx5 的做法（`waylandimserver.cpp::modifiersCallback` 里直接
`ic_->modifiers(serial_, ...)`，并且 `FCITX_UNUSED(serial)` 忽略回调自带的
serial，一律用 commit_state 的 `serial_`）。一对一镜像合成器的真值，
状态天然收敛，不可能卡住。

**约束**：daemon 侧不得再调用 `ImBackend::forward_modifiers`
（该方法保留但不可用），修饰态完全由 wayland 层镜像。

## 2026-10-11: Wayland 事件泵非阻塞化 + sync 楔死探测

**问题**：`dispatch_events()` 用阻塞 `roundtrip()`（内含 `blocking_read`）。
2026-10-11 实锤（gdb 栈 + 7.5 小时零日志）：KWin 侧停止应答后 socket 不报
错、roundtrip 永久挂起——wayland 线程连同按键处理、DBus 命令全部冻结；
而 2026-10-05 的断连自愈只在 dispatch **返回 Err** 时触发，挂起状态下自愈
永远不启动。后果：IM 激活的窗口里所有按键（用户报"Ctrl+A 无法全选"）被
楔死的键盘 grab 吸进黑洞，且托盘一切操作无效，只能整进程 kill。

**决策**：`dispatch_events` 重写为三段式（v1/v2 同构）：
1. `flush()` + `dispatch_pending()`——已到达事件立即派发；
2. `prepare_read()` + `poll(fd, 0)` + `read_events()`——0 超时探可读，
   没有事件立即返回（主循环保持 ~1ms 节奏，按键延迟不变）；
3. **sync 楔死探测**：每 1s 发一次 `wl_display.sync`（回调 Done 时清
   标记），未 ack 超 2s 判定连接楔死，dispatch 返回 `Err(TimedOut)`——
   复用既有 HealScheduler 恢复链（杀 launcher → KWin 重拉 → 新 fd）。

**理由**：
1. 楔死检测必须有"主动探针"：安静（无事件）与死亡（不回 sync）在纯
   被动读上不可区分，sync 是最小代价的心跳；
2. 超时返回 Err 而非线程内自救：conn 所有权在主循环，沿用 2026-10-05
   已验证的恢复链是最小改动；
3. poll(0) 而非长超时阻塞：按键路径的延迟预算是毫秒级。

**验证**：213 tests 全过；真机重启后 v1 Context 事件持续流入（serial
递增）、12+ 探针周期零误报；楔死场景恢复依赖下次复现（错误日志已带
"connection wedged" 标记可取证）。

## 2026-10-01: rime 数据目录迁移单目录模型（对齐 XimeYao）

**问题**：Unix 沿用双目录模型（shared=`~/.local/share/xime/rime-data` 只读装
方案，user=`~/.config/xime/rime` 放用户数据），实际暴露三类问题：
1. 方案来源混乱：方案码表在 shared、部署产物和用户数据在 user，方案词表、
   快捷短语、词典管理等一切"读方案目录"的功能都要做目录回退查找；
2. 升级语义缺失：shared 是安装脚本铺的静态副本，方案包更新后用户目录无感知，
   "用户弃用的方案"、"用户定制"与"待升级文件"无法区分；
3. 与跨端契约漂移：XimeYao（Windows）与 Xime（Android）已先后迁移到单目录
   （XimeYao DECISIONS 2026-09-11 明言"旧 shared/user 分离导致方案来源混乱"），
   libximecore 的跨端功能（方案词表/词典管理）被迫为 Linux 维护特例。

**决策**：
1. `shared_data_dir == user_data_dir == ~/.config/xime/rime`（daemon/setup
   启动时显式 `set_rime_paths`，libximecore 的 `default_rime_paths` 默认值
   不变——macOS 等其他宿主不受影响）；
2. dev-install/install 铺的 `~/.local/share/xime/rime-data`（或系统
   `/usr/share/xime/rime-data`）**降级为随包数据源**：启动时经
   `xime_config::ensure_bundled_rime_data` 部署进 rime 目录——
   - 首装（rime 目录无 `*.schema.yaml`）全量复制（递归含 lua/）；
   - 升级只强更"内容有变化且文件名不含 custom"的文件；
   - 用户弃用的 builtin 方案不强更（以 default.custom.yaml 启用列表为基准）；
3. `schema_dict` 等读路径全部回到单目录签名，删除目录数组回退。

**理由**：
1. XimeYao 同款迁移已验证（其 server 启动部署逻辑逐行移植）；
2. 部署语义解决了双目录下"更新方案包"只能靠重跑安装脚本的问题；
3. 用户数据一次性迁移（首装路径触发全量复制），userdb/词库/custom 全保留。

**验证**：真机迁移后 userdb（96120 条词库）、wubi86.custom.yaml、
custom_phrase.txt 无损；方案词表 91397 条 40ms；libximecore 4 组部署单测。

## 2026-08-15: 插件下载即安装 + daemon 插件宿主 + emoji 候选窗

**问题**：插件下载后需手动"安装"，且 daemon 无插件加载能力，插件安装后不生效。

**决策**：
1. 插件下载即安装：下载到临时 .xipk → 立即解压注册 → 删除临时包，去掉"安装"按钮
2. daemon 新增 `PluginHost`：启动时加载 enabled 插件（复用 libximecore xime-plugin 的 PluginRuntime/mlua 沙箱），提供 emoji 契约查询
3. emoji 候选窗：中文态 `;` 触发，字符搜索，数字键/回车选择上屏（与 Android 版契约一致）
4. 插件管理页（设置侧边栏）：列表 + 启用开关 + 卸载二次确认

**理由**：
1. 两步下载安装对用户无意义，下载即装降低心智负担
2. 插件契约与 Android 版 Xime 一致（getCategories/getEmojis），同一 .xipk 两端复用
3. 分阶段：先 emoji 类（候选窗能直接呈现），speech/clipboard 类后续接入

## 2026-08-15: 应用元数据参数化（AppMetadata）

**问题**：libximecore 是跨平台共享库，内部硬编码 "Xime"/"xime" 名称（配置路径、librime distribution 标识），无法被不同宿主平台复用。

**决策**：
1. xime-config 新增 `AppMetadata` 结构（metadata.rs）：`display_name` / `config_dir_name` / `config_file_base` / `distribution_name` / `distribution_code_name` / `app_name` / `version`
2. `set_app_metadata()` 全局注入，未注入时默认 "Xime" 保持兼容
3. 参数化范围**仅限核心**：配置路径（`~/.config/xime`、`/usr/share/xime`）、librime distribution/app 标识、`xime.yaml` 配置文件名、`Xime::SchemaConfigManager` generator_id
4. xime-setup UI 文案（"Xime 设置"、"关于 Xime" 等）**不改**，避免组件签名大规模改动
5. librime `deploy_all` 新增 `deploy_all_with_config(config_name)`，不破坏原签名
6. XimeChe 注入：`config_dir_name` 沿用 `xime`（兼容既有安装），`distribution_name = XimeChe`，显示名"曦码·澈输入法"

**理由**：
1. 目录沿用 xime 保证老用户配置/词库无缝迁移
2. 只参数化功能相关的名称，UI 文案属于显示层，各平台可自行维护
3. librime 是底层 crate，不能依赖 xime-config，故用独立函数参数而非全局状态

## 2026-08-15: 多合成器适配（v2 协议 + 后端抽象）

**问题**：当前只适配 KWin 的 `zwp_input_method_v1`，GNOME 45+ 只暴露 v2。

**决策**：
1. 完整实现 `zwp_input_method_v2` 后端（参考 fcitx5 waylandimserverv2）
2. 新增 `ImBackend` trait 抽象 v1/v2，daemon 用 `Box<dyn ImBackend>` 操作
3. 连接策略：
   - launcher（KWin）：fd 传入后探测 global，优先 v1，无则 v2
   - 直接连接（GNOME）：daemon 启动时连 `$WAYLAND_DISPLAY`，检测 v2
4. 按键转发：v2 无 forward_key 请求，用 `zwp_virtual_keyboard_v1.key()`（fcitx5 同款）
5. 提交语义：v2 请求 double-buffered，必须 `commit(serial)`（serial=done 计数）
6. 候选窗：v2 用 `zwp_input_popup_surface_v2`（合成器锚定光标），v1 保持 overlay_panel

**理由**：
1. GNOME（mutter 45+）、KWin 6、wlroots（Sway/Hyprland）都暴露 v2，一份后端覆盖大多数桌面
2. fcitx5 验证过的方案，避免踩 v2 语义坑（grab 事件、vk 转发、unavailable 重建）
3. GNOME 无 VirtualKeyboard/launcher 机制，必须支持 daemon 直接连接普通 socket

## 2025-05-12: 架构分离决策

**问题**：单进程架构无法满足 KDE VirtualKeyboard 要求
- KWin 通过 `WAYLAND_SOCKET` fd 启动输入法
- `zwp_input_method` 协议不暴露在普通 `wayland-0` socket 上
- 需要按需启动机制

**决策**：采用 daemon + launcher 分离架构
- launcher：接收 `WAYLAND_SOCKET`，通过 DBus 传递 fd
- daemon：DBus 服务，按需激活，处理 Wayland 事件

**理由**：
1. fcitx5 使用相同架构（fcitx5-wayland-launcher）
2. DBus 按需激活符合 KDE VirtualKeyboard 规范
3. 分离后 launcher 轻量，daemon 可持续运行

## 2025-05-12: DBus Service 配置

**问题**：launcher 调用 `org.xime.Xime.OpenWaylandSocket` 时找不到服务

**决策**：创建 DBus service 文件
- 位置：`resources/dbus/org.xime.Xime.service.in`
- 安装：`~/.local/share/dbus-1/services/` 或 `/usr/share/dbus-1/services/`
- DBus 会按需激活 daemon

**理由**：DBus 激活机制要求 service 文件定义服务名称和可执行路径

## 2025-05-12: zwp_input_method_v1 vs v2

**决策**：优先支持 v1 协议（KWin）

**理由**：
1. KWin 5.27 使用 `zwp_input_method_v1`
2. `WAYLAND_SOCKET` fd 是单次使用，必须先尝试 v1
3. v2 用于 Sway/Hyprland，作为备选

## 2025-05-12: 候选窗口定位

**决策**：使用 `zwp_input_panel_surface_v1.set_overlay_panel()`

**理由**：
1. compositor 自动定位在光标附近
2. 不需要手动计算坐标
3. fcitx5 使用相同方法

## 2025-05-15: 配置系统架构

**问题**：主题颜色修改后不生效，需要重启 daemon

**决策**：创建独立的 `xime-config` crate + DBus `ReloadStyle` 方法

**架构**：
1. `xime-config` crate：共享配置模块，所有 crate 依赖
2. 配置合并：系统默认 (`/usr/share/xime/xime.yaml`) + 用户覆盖 (`~/.config/xime/xime.yaml`)
3. 内置默认：编译时嵌入，确保系统配置缺失时有 fallback
4. DBus IPC：`ReloadStyle` 方法通知 daemon 重新加载配置

**理由**：
1. 共享模块避免代码重复
2. 配置合并允许系统级和用户级配置共存
3. DBus IPC 实现无需重启的实时更新

## 2025-05-15: font_size 类型修复

**问题**：用户配置 `font_size: 14.0` 解析失败

**原因**：`StyleConfig.font_size: i32` 不接受浮点数

**决策**：`font_size` 改为 `f32` 类型

**理由**：
1. YAML 中 `14.0` 是浮点数，`serde_yaml` 严格类型检查
2. `f32` 兼容整数和浮点数输入
3. 后续可支持小数字号（如 14.5）
## 2026-09-05: 候选栏主题样式与亮/暗色模式（移植自 macOS 版）

**问题**：候选栏只有 primary_color 一个配置生效，字号/圆角硬编码；无暗色模式

**决策**：
1. `xime-ui` 新增 `PanelTheme`（字号/圆角/亮暗两套配色），渲染层全部 theme 驱动
2. 亮/暗检测用 `org.freedesktop.portal.Settings` 的 color-scheme（DBus，KDE/GNOME 标准）
3. `ImBackend` 接口直接传 `&PanelTheme`（而非拆散的颜色参数）
4. 候选栏高度随字号自适应（≤16 保持 36px 不变，保证既有命中几何稳定）

**理由**：
1. 对齐 macOS 版 XimeYi 的 UiStyle + ui_colors 双模式设计
2. portal 是跨桌面环境的官方接口，无需分别对接 kde/gtk settings
3. theme 整体传递避免后续每加一个样式字段就改一遍 trait 签名

## 2026-09-05: 剪贴板监听协议选型（data-control，非轮询/外部命令）

**问题**：macOS 版靠轮询 `NSPasteboard.changeCount` 捕获剪贴板；Wayland 下没有等价物

**决策**：后台线程 + 独立 Wayland 连接 + data-control 协议事件驱动监听；
优先 `ext-data-control-unstable-v1`（标准暂定），回退 `zwlr-data-control-unstable-v1`

**理由**：
1. 符合项目「无框架依赖、一切自行掌控」原则（不依赖 wl-clipboard 外部命令）
2. 事件驱动无轮询开销；文本读取用 socketpair + 1 MiB 上限防阻塞
3. KDE（主目标环境）与 wlroots 系均支持；GNOME 两者皆无 → 自动捕获不可用（记录为已知限制，面板功能本身可用）
4. 存储层原样移植 macOS 版：表结构对齐 Android（`clipboard_entries` v3），db 文件三端可互换

## 2026-09-05: 插件系统应用模式对齐 Android 版（能力门禁 + 分层职责）

**问题**：XimeChe 插件系统只有 emoji 查询一条通道，无钩子；剪贴板功能与插件零耦合

**决策**（参照 Android 版 Xime 的三层模式）：
1. 剪贴板本地功能是宿主服务（xime-clipboard），插件只做「同步」——
   clipboard_sync 契约（push/pull/testConnection，libximecore 已有）由 daemon
   SyncBridge 独立线程接线，push 用 SHA-256 hash 去重，pull 做回声抑制
2. 宿主数据对插件只读：host.clipboard / host.quickSend 按 manifest
   capabilities 门禁注入（trait 由宿主实现，依赖反转），未声明能力不可见
3. 下行事件（text_committed）同步投递，仅 capabilities.events 订阅者收到
4. network.hosts fail-closed：未声明网络能力的插件禁止 host.http.request；
   allowCustomHosts: true 视为用户显式授权放行
5. 快捷发送编码注入候选栏用宿主原生实现（Rime 候选后追加 + 数字键分区接管），
   不做 transformCandidates 同步钩子——Wayland 按键是同步循环，Lua 卡顿
   直接冻结输入，风险高于 Android

**理由**：
1. Android 版验证过的职责划分：本地剪贴板稳定在宿主，跨设备同步交给插件生态
2. fail-closed 补上了 host.http 白名单缺失的安全洞（此前任意插件可联网）
3. 同步在独立线程：Lua http 20s 超时不阻塞按键路径

## 2026-10-23: 打包：一个 stage.sh + 各发行版原生构建

**背景**：此前只有 `cargo-deb` 产出的 `.deb`，Fedora / Arch 用户只能自己编译。

**决策**：
1. 安装布局集中到 `packaging/stage.sh`；`.deb` 继续走 `cargo-deb`（已验证过的产物不折腾），
   两边文件清单需要人工同步，并在两处注释里写明。
2. rpm / arch 在各自发行版容器里**从源码编译**，而不是把 Ubuntu 二进制换个壳：
   glibc 版本、依赖关系才对；CI 各 job 并行，墙钟时间不增加。
3. Arch 包直接跑仓库里的 PKGBUILD，等于每次发版都验证 AUR 配方，并由 CI 生成
   `.SRCINFO`（手写容易偏移，AUR 会因为 PKGBUILD 与 `.SRCINFO` 不一致拒收）。
4. 版本号唯一来源是 tag，且必须与 `Cargo.toml` 的 `[workspace.package] version` 一致，
   不一致直接失败——宁可红，不出版本号错乱的包。
5. Flatpak / Snap 明确不做：输入法 daemon 必须作为 KWin 的虚拟键盘客户端工作
   （KWin 通过 `WAYLAND_SOCKET` 传 fd，daemon 再用 `zwp_input_method_v2` 收发按键），
   沙箱里既拿不到这个 fd，也无法被 KWin 选为虚拟键盘。这不是权限问题，是架构冲突。

**理由**：包管理器用户不该被迫自己装 Rust 工具链；而输入法要接入的是 KWin 的
输入法通道，深到任何沙箱格式都覆盖不了。

## 2026-11-18: 语音输入：独立 speechd 进程 + 按住说话（PTT）

**背景**：`136418a` 因 sherpa-onnx 构建依赖联网下载而屏蔽了语音。libximecore
现在用 candle 重做（Qwen3-ASR），并把引擎放进独立进程 `xime-speechd`。

**决策**：
1. 输入法进程只链 `xime-speech-service` 的**客户端半**（`default-features = false`），
   不把 candle/cpal 链进来。原因是三条硬约束：1.9GB 权重常驻等于永不还内存；
   推理崩溃不能连累打字；麦克风授权必须落在真正录音的进程上。
2. 热键做成 **PTT**：按住 Ctrl+Alt+V 说话、松开立即上屏。只在 IM 激活（键盘
   grab）时可达——Wayland 没有全局快捷键协议。未激活时的入口是托盘菜单与
   DBus `ToggleSpeech`（toggle 语义），桌面快捷键绑 DBus 方法即可。
3. 松开 V、左 Ctrl、右 Ctrl、左 Alt、右 Alt 中任一都收尾：用户松修饰键的顺序
   不固定，只认 V 会漏掉「先松 Ctrl」那条路径。
4. 听写期间键盘为模态（吞所有键），Esc = 取消（停止但不上屏）。失焦/停用 IM
   走 `cancel()` 而不是 `release()`：没有 text-input 上下文时 commit 无处落，
   迟到的识别结果会串到下一个焦点窗口。
5. 服务由 daemon **按需拉起**（同目录 → PATH 找 `xime-speechd`），不装 systemd
   unit：模型在服务侧空闲 300 秒自动卸载，进程本身只占几 MB，常驻不心疼；
   按需拉起还避免了「用户没配模型却常驻一个大进程」。

**代价（必须知道）**：
- ~~服务是**非流式**的：单次录音上限 30 秒，没有中间文本，停止后整段识别。~~
  已由下一条「语音输入改为流式」取代。
- 设置页语音页（`voice-page`）尚未打开，模型只能手工放到
  `~/.config/xime/models/<id>/`（`config.json` + 权重 + `vocab.json`/`merges.txt`）。

## 2026-11-18: 语音输入改为流式（逐秒编码 + 滚动重解码 + 稳定前缀定稿）

**背景**：上一条的 PTT 是「说完才出字」。用户要求流式——边说边出字。

**决策**：
1. **不换模型**。Qwen3-ASR 没有 chunk 级增量解码，但它的音频塔是 1 秒一个
   chunk、chunk 之间独立做卷积与位置编码（官方语义，不是实现取巧），所以
   「逐秒编码再拼接」与「整段一次编码」等价。流式只重跑解码器，编码按秒增量付。
2. **定稿用 LocalAgreement-2**：连续两轮结果一致的公共前缀才落盘。落定的字
   永不回改——这是「能直接 commit_string 而不是只敢画在悬浮层里」的前提。
3. **已定稿文本注入 prompt 的 system 段**（Qwen3-ASR 官方的上下文偏置位）。
   模型在窗口接缝处本来会把上一句尾巴重吐一遍，靠它压下去。
4. **窗口到顶后等句读再切**（最多 +2 秒）。硬切会切断词：实测出现过
   「打开轿车自动气」少了「阀」。
5. **参数按后端分档**（`StreamTuning::gpu/cpu`）。实测 8 秒窗口重解码：GPU
   336ms、CPU 4050ms；CPU 追不上说话速度，所以它只能用 2 秒窗口退化成句级。
   宁可退化也不要积压——积压的流式比非流式更糟。
6. **松手不整段重跑**。流式已出字时只补最后不到一秒的尾巴；整段重识别会把
   已经落在输入框里的字改掉，用户会看到文本突然改写。
7. **取消（Esc）语义**：已定稿并上屏的部分保留，未定稿的预览丢弃。

**代价**：
- CPU 后端只能句级（每两秒一段），要逐字必须有 GPU 或换流式模型。
- 每轮重解码都有一次完整的 prefill，模型越大越吃算力；窗口不能超过音频塔的
  13 秒注意力范围。

## 2026-11-18: 设置页语音页 = 宿主注入回调，直连 speechd 的 socket

**决策**：`xime-setup-lib` 的语音页不引任何 IPC 依赖（它要跨平台），页面只画
UI，数据与动作靠宿主注入 7 个 `fn` 回调。XimeChe 的 `crates/xime-setup/src/main.rs`
注入的实现全部走 `~/.config/xime/speech.sock`——与候选栏 🎙️、daemon 是同一条
协议、同一个引擎实例。

1. **页面是编译期门控**：`xime-setup/Cargo.toml` 里不开 `voice-page`，侧栏连
   条目都没有（不是运行期隐藏）。这次才把它打开——之前「没有语音页」就是
   没编进去。
2. **连不上就拉起一次**：设置进程打开语音页时若 speechd 没在跑，按「同目录 →
   PATH」拉起来（异步，本次轮询返回 None，下一轮接上）。用户不必先去输入框
   激活 daemon 才能管理模型。
3. **「下载」如实降级**：协议没有 download op（speechd 只扫本地模型目录）。
   回调返回带原因的 status，页面显示「请用导入模型…」，而不是假冒"服务未运行"。
4. **不把推理编进设置程序**：`xime-speech-service` 以 `default-features = false`
   引入，cpal/candle 不进二进制（设置程序里没有一行推理代码）。

**代价**：
- 每次回调都是新建连接（250ms 轮询 = 每秒 4 次 connect）。服务端每连接一个
  线程，成本可忽略；换来的是设置端不必维护长连接与重连状态机。
- `speech.toml` 的 `language` / `commit` 页面能写但**当前没人读**：speechd 没有
  语种 op，daemon 也没读这份配置。接这两个得先在协议上加参数。

## 2026-11-18: 语音 PTT 键从 Ctrl+Alt+V 换成单键右 Ctrl

**决策**：主路径改成**按住右 Ctrl** 说话（`Ctrl+Alt+V` 保留为备选）。三键组合
在真实键盘上很难按稳——漏一个键就完全没反应，用户第一反应是"快捷键坏了"。

1. **只认右 Ctrl**（`Control_R`, 0xFFE4）：左 Ctrl 的 Ctrl+C/V/A/Z 密度太高，
   拿它当听写键会把日常编辑变成误触发。
2. **250ms 阈值 + 组合撤销**：按下只挂 `ptt_arm`，期间出现任何别的键或鼠标
   点击就撤销（那是 Ctrl+点击/组合键）；撑过 `PTT_HOLD_DELAY` 才 `press()`。
   判定放在主循环节拍里——修饰键不会 repeat，等不到第二个事件。
3. **修饰键不消费**：右 Ctrl 的按下/抬起照常转发给应用（否则会留下"粘住的
   Ctrl"，之后每个键都变成组合）。为此右 Ctrl 的释放分支单独提前 return，
   不能落到"听写中按键全吞"的模态分支（`release()` 后状态是 Transcribing，
   仍然 != Idle，会被吞）。
4. **听写中按别的键 = 取消并转发**：单键 PTT 期间用户很可能想 Ctrl+C，吞掉
   比打断听写更糟。
5. 起播时顺手 `hide_root_window()`：按住 Ctrl 会弹字根窗（既有特性），听写
   期间它只是挡画面。

**代价**：按住右 Ctrl 超过 250ms 的"Ctrl+长按"操作（几乎不存在）会误起播；
某些把右 Ctrl 映射成 Compose 的布局用不了，只能退回 `Ctrl+Alt+V` 或托盘。

## 2026-11-18: 长听写断流（说着说着不再上屏）

**症状**：按住右 Ctrl 说一段时间后，面板还在、波形还在动，但一个字都不再
上屏；松手也只补上最后一句话。用户无法判断「没在录」还是「没在识别」。

**根因（两处叠加，都在 libximecore 的 speech-service）**：

1. **录音缓冲满后静默丢弃新音频**（主因）。`capture.rs` 的音频回调里
   `if buffer.len() >= cap { return; }` —— 缓冲一旦攒满就直接返回，不但不
   收新样本，连 `raw_len` 也不再更新。流式 worker 靠 `read_since(cursor)`
   取增量，于是永远读到空、停在最后一句话上；而电平在 `return` 之前已经
   更新，所以波形照常跳动。`cap` 还算错了：用 `16000 × 30 × (rate/16000)`
   反推设备采样率，44.1 kHz 实际只有 21.7 秒、48 kHz 只有 30 秒。
2. **解码 prompt 里的上文无限增长**。已定稿全文整段塞进 Qwen3-ASR 的
   system 段，听写越久 prompt 越长：每轮解码从几百毫秒涨到几秒（越说越
   卡），最终超出上下文预算直接解码失败——而错误只写进 session.error，
   `publish` 不带出来，客户端完全看不见。

**修法**：
- `capture` 改成滚动缓冲 + 绝对索引（`base` + `raw_len`）：缓冲满了滚掉
  **最旧**的一半，绝不丢新样本；新增 `CaptureTap::trim_before`，流式 worker
  每轮把已取走的样本裁掉，缓冲恒定在 `KEEP_SECS = 45` 秒。游标是绝对下标，
  裁剪不会让它错位。
- 流式 prompt 的上文只取尾部 `CONTEXT_CHARS = 160` 字（`context_tail`）。
- `publish` 把 `session.error()` 写进 `Status.error`；daemon 存到
  `speech::Progress`，面板状态行有值就显示「识别异常：…」，正常时显示
  「正在听写 · 已上屏 N 字」——波形动 = 麦克风有输入，字数涨 = 识别在推进，
  两者分开看，才答得上「到底卡在哪一步」。

**注意**：`dev-install.sh` 检测到 `xime-speechd` 已安装就跳过构建，改语音
引擎相关代码必须 `XIME_SPEECH_REBUILD=1`，否则二进制还是旧的。

## 2026-11-18: deb 分成 CPU / CUDA 两个变体

**决策**：Release 里出两个 deb —— `xime_<ver>_amd64.deb`（CPU 语音）与
`xime-cuda_<ver>_amd64.deb`（CUDA 语音）。两者文件清单相同，只差
`xime-speechd` 的后端，因此声明 `Conflicts/Replaces: xime`，apt 会提示替换。

用 cargo-deb 的 **variants** 实现，而不是两个 Cargo 包：
`[package.metadata.deb.variants.cuda]` 只覆盖 depends/description/conflicts，
`assets` 从主机段继承（包名自动变成 `xime-cuda`）。CI 在打哪个变体之前就把
对应后端的 `xime-speechd` 放到 `target/release/`，同一个文件清单即可复用。

**为什么不用「主包 + 语音附加包」**：那需要 update-alternatives 或让 daemon
去 `/usr/lib/xime/` 找二进制，卸载附加包还得恢复 CPU 版；两个完整包最简单，
用户二选一，语义和商业软件的做法一致。

**CI 要点（`.github/workflows/release.yml` 的 deb job）**：
1. speechd 不在本仓库：CI 显式 `git clone --depth 1 libximecore` 并 patch 到
   本地路径（只 patch 语音/配置相关 crate，**librime 不 patch**，否则要连带
   构建 librime 子模块），然后 `--manifest-path ../libximecore/Cargo.toml`
   单独构建 speechd。
2. CUDA 用 12.4.1：cudarc 0.19 的版本白名单是 12.x–13.4，且 soname 是
   `libcudart.so.12`，覆盖所有 CUDA 12.x 运行时。toolkit 用 Jimver action 装。
3. `CUDA_COMPUTE_CAP=80` 必须钉住：编 CUDA 本身不需要 GPU（nvcc 只是交叉
   编译，cudarc 的 build.rs 仅执行 `nvcc --version`），但 runner 上没有
   nvidia-smi，cudaforge 的 `detect_compute_cap()` 会返回 Err 让构建失败——
   它不会回落到某个默认架构，所以这里必须显式给值。
   cubin 在同一 major 内向上兼容，所以 30/40 系（sm_86/89）能跑，Hopper 及以上
   需要自行重编。Ubuntu 24.04 的 gcc 13.3 超出 CUDA 12.4 的支持范围，装
   gcc-12 并设 `NVCC_CCBIN`。
4. 打包后**用 ldd 断言变体**（CUDA 版必须链到 libcudart，CPU 版必须没有），
   防止两个包复制同一份二进制。

**代价**：rpm / Arch 包暂时仍不含语音守护进程（README 已注明），要补得在各自
容器里装 CUDA/alsa 开发依赖并同样构建 speechd。

## 2026-11-18: CI 常态验证 CUDA + 一条命令本地出包

**问题**：CUDA 后端只在 release 里用，代码在 libximecore——libximecore 的
feature 改名、candle 升级、kernel 编译方式变化，都要等到打 tag 才发现。而且
用户自己想把包装出来时，只有「打 tag」和「照着 README 手动敲一堆命令」两条路。

**做法**：

1. `.github/workflows/ci.yml` 新增 `cuda-speech` job，每次 push/PR 用和 release
   **相同**的工具链（CUDA 12.4 + gcc-12）编一遍 `xime-speechd --features cuda`。
   校验两条：`ldd` 里有 `libcudart`；`cuobjdump -lelf` 里能数出 `sm_80` cubin。
   之所以要第二条——链接成功不等于 kernel 编进去了，只有 cubin 是实据。
   （canary 必须和 release 走同一条路，否则它验的是另一个东西。）
2. `packaging/build-deb.sh`：本地一条命令出 CPU/CUDA 两个 deb，步骤与 CI 对齐，
   含解包自检（后端 + cubin 架构）。产物统一落到 `<target>/deb-out/`，因为两个
   变体共用 `target/debian` 下的文件名，不先挪走会被下一次 `rm -rf` 删掉。

**实现坑**：
- target 目录不能写死 `$REPO/target`：全局 `~/.cargo/config.toml` 设了
  `build.target-dir`，本仓库和 libximecore 会共用一个 target，连
  `install` 都会因为「源和目标为同一文件」失败，脚本里要比较 realpath 再拷。
- `/usr/local/cuda` 在本机指向 13.5，而 cudarc 0.19 白名单只到 13.4 → 脚本改为
  枚举所有 `/usr/local/cuda-*`、只保留白名单内的版本、取最高（这里选到 13.4）。
- 拿包的三条路已写进 README：打 tag / Actions 手动跑（Artifacts）/ 本地脚本。
