# XimeChe（曦码·澈输入法）重要决策

## 2026-11-14: 按键重复必须由输入法自己合成

**问题**：用户报「按住方向键左右不能连续移动光标，只移动一格」。实机日志
显示 KWin 在 grab 期间**会**发重复按键（`state=Repeated`，~40ms 一条），但
输入法把 `KeyState::Repeated` 当成释放转发，应用只收到首按 + 一连串孤儿
释放。另一些合成器可能不发 `Repeated`，只给 `repeat_info(rate, delay)`。

**决策**：

1. **`Repeated` 按按下处理**（首要修复）：v1/v2 的 `pressed` 同时匹配
   `Pressed` / `Repeated`。这是 KWin 的实际行为，注释与日志都以此为准。
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
