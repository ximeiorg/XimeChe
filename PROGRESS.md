# XimeChe（曦码·澈输入法）开发进度

## 当前状态
**输入法"偶尔死掉"修复：Wayland 断连可见化 + 整链自动恢复 + Shutdown
段错误修复**（2026-10-05）。此前 P10 语音 M1+M2 完成、面板重写已合 main。

## 本次变更（2026-10-05）：断连死锁与退出崩溃修复

诊断结论（coredump + journal + KWin 源码取证）：
- **断连死锁**：daemon 与 KWin 的 Wayland 连接会静默断开（复制时最易
  触发——应用 disable text-input → KWin deactivate 的竞态窗口），此前
  dispatch 错误只打 debug 日志，conn=None 后永久干等 launcher 重传 fd；
  而 launcher 收完 fd 就 sleep 永不退出，KWin 看 launcher 活着就不重拉
  → 死锁，用户只能去设置里切换输入法恢复
- **KWin 侧约束**（源码实锤）：KWin 只在 IM 客户端 **CrashExit** 时重拉
  launcher（干净退出不重拉）；20 秒内崩溃计数到 5 则 stopInputMethod
  永久停用，计数器无崩溃 20 秒清零
- **Shutdown 段错误**：每次 DBus Shutdown 的 exit(0) 都在 librime 静态
  Service 析构（CleanupAllSessions → ConcreteEngine 释放 Translator）
  时 SIGSEGV，"正常退出"实际全是 CrashExit

修复内容：
- **可见化**：dispatch 错误升 error 落文件日志、handle_unavailable 升
  warn、启动日志带 pid、命令通道断开升 error
- **daemon 自愈**：HealScheduler（首次断连 1s 后动作，自愈杀 launcher
  间隔固定 25s > KWin 20s 清零窗口，永不触发崩溃保护）；断连后先试
  standalone 直连，失败则 pkill -TERM 滞留 launcher 逼 KWin 重拉传新 fd；
  启动后 fd 迟迟不到的死等也纳入自愈
- **launcher 看门狗**：每 3s 查询 org.xime.Xime 在总线上的存活，连续
  3 次消失（约 9s）后 SIGKILL 自杀——必须被信号杀死（CrashExit）KWin
  才会重拉；新 launcher 经 DBus 激活重建 daemon，daemon 崩溃也能全链
  自动恢复
- **clean_exit**：Shutdown/通道断开改为 sleep 150ms（排空 non-blocking
  日志）+ libc::_exit(0)，跳过 librime 静态析构，退出码回到干净的 0

验证：clippy -D warnings 零警告；xime-daemon 64 测试全过（含 4 个
HealScheduler 节奏测试）；实机演练「DBus Shutdown → 无 coredump →
看门狗自杀 → KWin 重拉 → DBus 激活新 daemon → fd 重连」整链恢复。


## 本次变更（2026-10-04）：P10-M2 模型管理归位设置程序

用户指正：候选栏 🎙️ 不该做模型下载——模型管理归 setup（扩展商店
基建所在），候选栏未就绪时只提示引导。重构为「daemon 数据源 + setup
镜像 UI」：
- **speech.rs 全局化**：SpeechBridge 拆成全局 statics（CMD_TX/EVENT_RX/
  VIEW），DBus 线程与 wayland 主循环共享；模型操作命令 DownloadModel/
  DeleteModel/SelectModel（下载在独立线程，134MB 不阻塞 🎙️）；选中
  模型持久化 ~/.config/xime/speech.json；**模型目录对齐 setup 的
  models_dir 约定 ~/.config/xime/models**（已迁移已下载模型）
- **候选栏去下载**：🎙️ 点击时模型未就绪 → 桌面通知「请打开设置程序的
  「语音转文本」页下载」；SpeechState::Downloading/Shutdown 死代码清理
- **DBus 新增 6 方法**：GetSpeechStatus（JSON 快照，字段逐字对齐
  speech_models::SpeechServerStatus）/ DownloadSpeechModel /
  DeleteSpeechModel / SelectSpeechModel / SpeechTestStart / SpeechTestStop
- **libximecore**：voice 页解除 Windows 门控（voice-page feature 即可；
  WinRT 听写保持 windows 双门）；SpeechServerStatus/Entry 补
  Deserialize；**librime-sys2 rpath 合并单条**（lld 下多条 -rpath 互相
  覆盖——曾致 daemon 链接系统 librime 启动崩溃，含 $ORIGIN 语音库位）
- **xime-setup 薄壳**：voice-page feature + 6 个语音回调注入（zbus
  blocking → daemon DBus，对齐现有 fn 指针模式）
- **验证**：GetSpeechStatus 快照（model_ready 命中）、未下载模型选择
  被拒、speech.json 持久化往返；194+25 tests / 两仓 clippy 全绿

## 本次变更（2026-10-04）：P10-M1 语音转文本核心链路

- **依赖**：workspace 引入 libximecore 的 xime-speech（patch 到本地）；
  daemon 增 reqwest(blocking)/tar/bzip2/libloading
- **crates/xime-daemon/src/speech.rs**（新）：
  - 模型管理：首次使用自动下载 ModelScope tar.bz2（UA 必带，ModelScope
    403 掉无 UA 客户端）→ 解压递归找四文件归位 → 校验
    `~/.local/share/xime/models/<id>/`（默认 x-asr-480ms-zh-en-punct-int8
    中英混输带标点 133.9MB）
  - 采集：内嵌 dlopen 绑定 pa_simple_new/read/free（16kHz mono s16ne，
    PipeWire 兼容；构建零系统依赖，缺失给可读错误）
  - 会话：单 worker 独占 StreamingRecognizer（非 Send 共享语义），
    命令进/事件出信箱；端点检测断句自动上屏；SpeechBridge 全 &self
    （内部可变性），主循环 drain_events 消费
- **接线**：🎙️ 卡片点击 = toggle（面板收起让位候选栏反馈）；听写中
  候选栏显示「🎙️ partial」实时文本；下载/装载进度同通道；Committed
  经主循环 commit_string 上屏；**听写中点菜单按钮 = 停止**；失焦自动
  停止；Error 走桌面通知兜底
- **运行库安装**：daemon rpath 加 `$ORIGIN/../share/xime/lib`；
  dev-install.sh 从 sherpa 构建缓存拷 libsherpa-onnx-c-api.so/
  libonnxruntime.so 到 ~/.local/share/xime/lib（ldd 验证命中）；
  系统包（xime-pack）同布局适用
- **验证**：模型下载→解压→四文件校验端到端通过；sherpa 装载 1.85s；
  193 tests（+3 speech）/ clippy 全绿；examples/model_setup.rs（CLI
  装模型）与 panel_snapshot/color_probe 同存供回归
- **待实机验证（需用户）**：麦克风实际说话的识别质量、端点断句体验、
  采集设备选择（默认源）

## 本次变更（2026-10-01 深夜）：候选栏面板完全重写（分支 panel-ximeyao-rewrite）

背景：用户反馈"候选栏菜单功能和 Windows 行为和 UI 都不一样"，要求**完全
重新移植，功能存在也要重新看**。旧实现（SearchPanel 搜索式表情面板 +
ListPanel 列表面板）整体废弃，按 XimeYao 现状重写：

- **xime-ui/menu.rs**（全新）：`PanelPage` 路由（Menu/Clipboard/QuickSend/
  Emoji/Symbol）、6 卡片菜单（📋🚀😀🔣🎙️⚙️）、列表子页（6 行/页 40 字截断
  + 编码列 + 翻页条）、网格子页（8×4 + 底部标签栏 表情 1 行/符号多行 9 列、
  最近使用标签）、`panel_hit` 纯函数（绘制与命中同一几何）；几何常量 1:1
  对齐 XimeYao（PANEL_HEADER_HEIGHT=36 等）
- **xime-ui/iced_view.rs**：draw_panel 按页渲染（页头返回钮/卡片页/列表页/
  网格页/标签栏/翻页条/品牌条）；neutral_row_bg 修复为主题色混合（原硬编码
  白底，暗色主题下错误）
- **xime-wayland**：ImBackend trait 换成 `show_panel(page,list,grid)`/
  `hide_panel()`，v1/v2 高度按页计算、宽度不小于 PANEL_MIN_WIDTH
- **xime-daemon**：PanelState{Closed,Open(page)} + PanelData 权威状态；
  open_panel_page（进页 reload 一次数据：剪贴板开页 pullOnce、表情重载
  插件、符号内置表）/ refresh_grid_cells / close_panel / load_panel_list；
  按键语义对齐 XimeYao 桌面（Esc 关、`;` 上屏分号面板保持、可打印键收起
  继续输入、修饰键不收起）；**`;` 中文态无组合时打开表情页**（重写中曾
  丢失，已从 main 恢复并补上 ascii/组合态门控）；网格点击上屏 + LRU
  recent_usage（32 条，JSON 落盘）+ 面板保持打开
- **测试**：menu.rs 几何/翻页/命中、daemon refresh_grid_cells 联动翻页/
  切标签；全仓 183 tests 通过，clippy -D warnings 干净
- **插队核对项核对结果**：「剪贴板拉取内容不入历史」代码已解决
  （handle_pull → upsert_and_trim 入库 + 回声抑制）；「中文态 Shift+符号键」
  维持待实测（rime-wubi 子模块已带 Shift 保持 commit_code 修复）

## 上次变更（2026-10-01 晚）：单目录模型迁移（详见 DECISIONS.md 同日条目）

- libximecore 新增 `xime_config::ensure_bundled_rime_data`（移植 XimeYao
  `ensure_rime_data`）：首装全量、升级强更非 custom、弃用方案不强更；4 组单测
- daemon/setup 启动时部署随包数据源 → `set_rime_paths(shared==user==rime 目录)`
- `schema_dict` 去掉目录数组回退（P4 的临时补丁），回单目录签名
- 真机迁移：方案文件全量落位 user 目录；userdb 96120 条词库、
  wubi86.custom.yaml、custom_phrase.txt 无损；词表 91397 条 40ms 正常

## UI 冻结审查与修复（2026-10-01 晚，libximecore 7fb9519）

审查全部 notify_* 回调调用上下文，修复两处实锤 UI 线程阻塞：
1. **RimeSyncNow**：iced update() 里同步等 DBus（0.66s~数秒）→ 后台线程 +
   RIME_SYNC_OUTCOME 信箱 + poll_background 回收；syncing 置灰防重入
2. **rfd 文件对话框**（词典恢复/导出/导入）：Linux xdg-portal 对话框是独立
   窗口，阻塞调用冻结设置窗口 → file_dialog 模块后台选路径 + 信箱 +
   IN_FLIGHT 防连点；Windows 保持原生模态不变
其余回调核实全部在 std::thread::spawn 内；已知接受的阻塞（切方案毫秒级
快速路径、daemon levers 操作期间按键暂停）记录在案。

## XimeYao 功能移植（2026-10-01，P0-P9 完成）

| # | 功能点 | 提交 | 验证 |
|---|--------|------|------|
| P0 | 修复 Linux 构建（词典机制解除 Windows 门控 → `any(windows, dict-page)`；sherpa default-members 隔离） | libximecore 9c25bb1 | 两仓构建全绿 |
| P1 | 词典读路径（DBus ListUserDicts/ListDictEntries + with_user_dict_closed） | 70aa2c0 / d3d11ca | wubi86 96120 条 0.25s |
| P2 | 词典写路径（UserDictOp：造词/删除 tombstone/备份/恢复/导出/导入） | a9569d5 | 造词→查→删→导出→备份→导入全链路 |
| P3 | 快捷短语（custom_phrase 整表编辑 + translator patch 注入） | c38b57e | 保存→patch 注入→Deploy |
| P4 | 方案词表只读浏览（BFS 合并 + 签名缓存） | 30ba3b9 | 91397 条，冷读 42ms/缓存 17ms |
| P5 | rime 用户资料同步（UserDictOp::Sync + backup 页启用） | 7e71d3b | sync 快照生成，会话恢复 |
| P6 | 托盘方案切换菜单（当前项 ✓，点击切换） | 5c3852b | 已上线（托盘目视待用户） |
| P7 | 部署结果桌面通知（freedesktop Notifications） | 6f024ce | Deploy 无错（弹窗待目视） |
| P8 | 剪贴板 DB v3→v4（Android 对齐：图片/类型 7 列 + imageHash 索引） | 9ac1b44 | 真机 DB 迁移 v4、老数据完好 |
| P9 | 插件配置加密 Linux keyring（Secret Service，AES-256-GCM 同格式） | libximecore a4d772d | 实机 enc: 加密+解密（KDE Wallet） |

**P10 语音转文本——下一个功能点**（本会话不做：端到端验证需用户对麦克风
说话，无法独立闭环）。实施蓝图：
- 原生库障碍已排除：sherpa-onnx-sys 1.13.8 build.rs 支持自动下载
  linux-x64 预编译库（shared ~XXMB，缓存在 target/sherpa-onnx-prebuilt）
- 链路：daemon 集成 xime-speech（StreamingRecognizer：装载→喂 pcm16→
  partial→finalize）；音频采集需选型（alsa crate 或 pipewire，16kHz mono
  pcm16 块喂流）；voice 页照词典页模式解除 windows 门控（speech_models 的
  server IPC 回调改 DBus 注入）；模型下载走 daemon（dbus 流度上报）
- XimeYao 参考：`winxime-server` speech 模块 + libximecore voice.rs UI

**插队核对项**（XimeYao 修过的 bug，Linux 是否同现待实测）：
「中文态 Shift+符号键无法上屏」「剪贴板拉取内容不入历史」。

**移植过程中实锤的两个跨平台差异**：
- librime levers 导出要求 userdb 独占（同进程会话持有 LevelDB 锁，
  必须关会话→导出→重建；XimeYao P1 读路径没关会话是侥幸）
- XimeChe 双目录模型：方案码表在只读 shared 目录，user 目录只有用户数据
  （XimeYao 单目录无此问题）——schema_dict 需要目录数组回退查找

## P1 实现要点（2026-10-01）

- **数据通道**：daemon DBus `ListUserDicts`（直调 levers iterator，不碰会话，
  实测安全）/ `ListDictEntries`（**必须经 command channel 在 wayland 线程执行**——
  levers 导出要求 userdb 独占，DBus 线程并发导出实锤失败
  "Failed to export user dict"；CLI 独立进程导出 96120 条成功证明 userdb 正常）
- **`RimeEngine::with_user_dict_closed`**：关会话→导出→重建会话，对齐
  librime CAVEAT / 安卓 withUserDictClosed / XimeYao P2。代价：正在输入的
  composition 会丢（词典浏览与打字互斥，如实接受）
- **zbus 陷阱**：object server 不在 tokio 上下文，`spawn_blocking` panic
  → 方法永不回包（busctl 超时实锤）；改 std::thread + tokio oneshot
- setup 薄壳注册 `set_notify_dict_list/entries` 回调走 DBus（JSON 传输，
  libximecore 结果类型补 `serde::Deserialize`）
- **实机验证**：wubi86 total=96120 词条 0.25s 返回；query=ni 命中 163；
  会话重建后 SelectSchema true；daemon 46 测试（+4）、全仓 185 全绿

## 诊断与修复（2026-09-29）：WebDAV 插件「测试连接」报 propfind 请求失败

**现象**：坚果云 WebDAV 配置正确（curl PROPFIND 207 正常），插件测试连接
报 "propfind 请求失败"；daemon push 也报 "请求失败 PUT …/current.json"。

**根因**（两处，均为 ureq 3 / ureq-proto 0.6.0 设计缺陷）：
1. `ureq-proto verify_version` 只放行 9 个 IANA 标准方法，WebDAV 扩展方法
   （PROPFIND/MKCOL/MOVE…）直接 `MethodVersionMismatch`——无任何配置可绕过
2. ureq 3 默认把 ≥400 状态码当 `Err(StatusCode)` → 宿主实现失败路径返回
   null → 插件只看到泛化 "请求失败"，401/404/503 的区分逻辑全部失效

**修复**（libximecore a06f864）：http 层迁移 `reqwest blocking`
（rustls-tls-native-roots + gzip，无方法白名单、任意状态码返回响应对象）：
- xime-plugin host.http.request：共享 OnceLock<Client>（blocking 实例内建
  runtime 线程须复用），超时改按请求 timeout（原 timeout_global 语义）
- xime-setup webdav.rs（云备份）：显式状态码判断（MKCOL 405/409、GET 404、
  PROPFIND 207/404），替代原先对 "HTTP xxx" 错误字符串的 contains 匹配
- xime-setup 商店方案/模型/插件 v2 索引 + download_file 一并迁移

**验证**：探针 PROPFIND → 207；真实插件+用户配置 testConnection → 成功；
145 测试 + clippy -D warnings 全绿；真机重装后 daemon plugin host 2/2、
Sync bridge 1/1、经 launcher fd 连接正常。修复经历两轮 dev-install：
KWin 崩溃重拉比 install 快，需核对进程启动时间晚于二进制 mtime 才算新代码。

## 本次变更（2026-09-28）：剪贴板同步插件下拉选择端到端生效（对齐 Android 端）

**问题**：设置程序「剪贴板同步」页的下拉只列**已启用**的插件（插件被禁用
后就选不到）；且下拉写入的 `plugin_id` daemon 完全不读——同步桥加载全部
已启用的 clipboard_sync 插件并逐个推送/拉取，下拉形同虚设。Android 端
`ClipboardSyncSettingsScreen` 的语义是：下拉列出全部已安装同步插件，选中
即激活（其余停用），引擎只跑选中的那个。

**libximecore 侧**（xime-setup）：
- `scan_clipboard_sync_plugins` 列出全部已安装 clipboard_sync 插件（不再
  过滤 enabled，对齐 Android `getAllInstalledPlugins` 按分类过滤），
  `ClipboardSyncPluginInfo` 新增 `enabled` 字段，下拉中停用项显示
  「（未启用）」后缀
- 下拉选项改为按 id 匹配（原按 name 回查下标，同名插件会选错）
- `select()`：写 plugin_id → `PluginManager::set_enabled(选中项, true)`
  （对齐 Android 单选激活）→ `notify_daemon_reload_plugins()` 立即生效；
  目录直装（无注册记录）插件的 NotFound 忽略（扫描默认视为启用）
- `set_enabled()`（总开关）也通知 daemon，状态消息不再谎称"数秒内生效"

**XimeChe 侧**（daemon clipboard_sync.rs）：
- 新增 `SyncSelection`（`~/.config/xime/clipboard_sync.toml`：`enabled` +
  `plugin_id`，与设置程序共享同一文件）
- `scan_descriptors` 重构为 `resolve_descriptors(root, sel)`：总开关关闭 →
  空；偏好插件有效 → 只加载它；偏好空/失效 → 回落首个已启用项（对齐
  Android `ActivePluginSelection.resolve` 的单插件语义）；push/pull 不再
  遍历全部插件
- 新增 5 个单元测试（开关关闭/偏好生效/空与失效回落/跳过停用与其他类型/解析）

**验证**：libximecore 145 测试 + clippy --all-features -D warnings 全绿；
XimeChe 全 workspace 测试（daemon 42，+5）+ clippy 全绿。**真机已验证**
（2026-09-28 晚 dev-install + DBus 重启）：写 toml enabled+plugin_id →
ReloadPlugins → 日志 `Sync bridge: 1/1`（仅选中插件）；enabled=false →
`0/0`。顺带把滞留的 Lua 时代 kaomoji 插件升级为 4.0.0（QuickJS 版），
plugin host 2/2 全绿；安装 webdav-clipboard-sync 1.1.0 供下拉选择。

## 插件系统迁移：Lua → QuickJS（2026-09-27）

**背景**：xime 3.0（Android）插件系统已从 Lua 换成 QuickJS（插件用 TS 编译成
IIFE main.js），libximecore 作为跨端契约单一来源必须跟进，否则两端插件生态割裂。

**libximecore 侧**（改动集中在 xime-plugin crate）：
- 依赖：mlua → quickjs-rusty 0.14.0（serde+bigint feature，0.14 的 serde 模块
  依赖 bigint 代码路径）；新增 sha1（Android 3.0 的 crypto.hmacSha1 契约）、
  libquickjs-ng-sys（中断处理器签名）
- runtime.rs 整体重写：入口为 IIFE 挂 `globalThis.plugin` 分组命名空间
  （emoji.listCategories、clipboardSync.push、backup.push、settings.schema、
  transform.candidates、panel.state、events.onTextCommitted…）；
  host API 对齐 Android 3.0 面（sdkVersion "3.0.0"、config.getJson、
  crypto 补 hmacSha1/epochSeconds、http.request 返回 {status,headers,body,text}、
  console/TextEncoder/TextDecoder/atob/btoa polyfill、XimeError、受限 require）
- 沙箱：屏蔽 eval/Function（require 用捕获的原始 Function 编译模块）、
  require 仅插件包内相对 .js（裸名回退 libs/，≤2MB，canonicalize 防穿越）、
  64MB 堆上限、契约调用硬超时（15ms transform / 5s 回调 / 180s 业务，
  interrupt handler 实现）且超时后运行时熔断降级
- 同步模型：host API 全部 Rust 侧阻塞实现（JS `await` 普通值合法，Android
  插件写法无感）；插件返回的 Promise 由 Rust 驱动 pending job + 轮询
  `__ximeSettle` 槽阻塞落定（上限防永不落定的 Promise 忙等）
- Rust API 面保持兼容：EmojiItem/SettingField/PluginRuntime::load 等签名不变；
  仅 `call_fn<T: FromLuaMulti>`（Lua 专属泛型）换成 `call_plugin_fn(path, args)`
- manifest.json 支持（from_dir/install_from_zip 优先 JSON，回落 manifest.yaml
  兼容存量包）；default_entry → main.js

**XimeChe 侧**（生产代码零改动，仅测试适配）：
- daemon 的 PluginHost（wayland 线程）/ SyncState（同步桥线程）线程模型
  天然满足 quickjs Context 的 !Send 约束，无需改造
- plugin_host 测试 fixture：Lua xipk → manifest.yaml + main.js（顺带验证
  YAML 兼容回落）；clipboard_sync 测试：FAKE_SYNC_LUA → JS 分组契约，
  call_fn → call_plugin_fn

**验证**：libximecore 33 个 xime-plugin 测试 + workspace 全量（含 xime-setup-lib）
全绿；XimeChe daemon 37 测试全绿；两仓库 clippy -D warnings 全绿。

**已知限制（后续阶段）**：ws/SSE/stream/ASR emit 桥未实现（流式 ASR 插件暂
不可用）；XimeUiNode 完整设置 schema（select/switch 等）待 xime-setup UI 升级；
能力门禁注入（quickSend.list/clipboard.get 真数据）待 daemon 接线；真实
Android .xipk 端到端安装验证需要 xipm 工具链产物。

## 诊断与修复（2026-09-25 晚）：复制后托盘消失、Shift/Ctrl+Space 全部无效

**用户现象**：切换到另一应用复制后，托盘图标直接消失；按 Shift / Ctrl+Space
均无法恢复，要很久才好。

**按键级日志实锤**（DEBUG daemon 全程录制，`/tmp/xime-debug4.log`）：
1. 14:08:57 DEACTIVATE（切应用，正常）→ 之后 **3 分 11 秒完全空白**：
   用户打字/按 Shift/Ctrl+Space，IM 零事件——按键全部直达应用，
   KWin 一直没发 ACTIVATE（聚焦的客户端未启用 text-input）
2. 14:11:45 剪贴板 watcher 正常捕获 2949 字符（复制发生在卡住期间）
3. 14:12:08 ACTIVATE 终于到达；Rime 恢复在 ascii_mode=true（英文，是
   用户 14:07 输版本号时主动切的残留状态）→ 造成"恢复后托盘显示 en"
4. 期间 IM 进程/线程/DBus/剪贴板全线健康，Shift 切换在激活态下工作正常
   （14:07:44 用户主动 Shift 切英文输 "3.0.0-beta2" 全程正常）

**结论**：非 daemon 缺陷。客户端（QQ = Chromium 138.0.7204.35，bug 在
139 修复；KWin bug 493098）聚焦输入框时不重新 enable text-input →
KWin 不激活 IM → 键盘事件绕过 IM。"卡住时 Shift/Ctrl+Space 无效"是
必然：那些键根本到不了 IM。

**曾经走错的方向**（记录避免重蹈）：曾把 Shift 切换误判为根因改为 noop，
用户指正其日常依赖 Shift 切换——已 revert（rime-wubi 040770a）。教训：
"按 X 无反应"要先确认事件是否到达 daemon，而不是怀疑 X 的绑定。

**改善（commit 050de44）**：
- 托盘图标常驻显示（fcitx5 风格）：失活不再隐藏。图标是 IM 未激活时
  唯一可达的控制通道（DBus 不经 Wayland 键盘路径）
- 托盘点击切换时若 IM 未激活 → 自动调用 KWin `forceActivate` 强制激活，
  给用户鼠标一键恢复手段（替代 alt-tab 碰运气）
- daemon 新增 DBus Shutdown 方法（优雅退出，避免 kill 触发 KWin 崩溃保护）

**根治路径**：QQ 升级 Chromium 139+（`strings /opt/QQ/qq | grep -oE
"Chrome/[0-9.]+" | head -1` 验证）；临时规避：卡住时点托盘图标，或
alt-tab 切走再切回。

## 诊断与修复（2026-09-25）：复制后托盘变 EN、"卡死"无法打中文【结论已被晚间诊断取代】

**此节结论错误**：把 Shift 切换误判为根因。实际用户日常依赖 Shift 切换，
且"按 Shift 无反应"的时段 KWin 根本没激活 IM（见上方晚间诊断）。
本节保留的有效信息：
- rime 配置两层结构：`default.custom.yaml` 补丁覆盖 `default.yaml`
  同名段，改 base 不生效（librime 通用坑）
- rime-wubi noop 改动已 revert（040770a），Shift_L/Shift_R 恢复 commit_code
- daemon 新增 DBus `Shutdown` 方法（优雅退出，避免 kill -15 被 KWin 记为
  QProcess::CrashExit 累积触发崩溃保护）

## 诊断与修复（2026-09-09）：开机不自启 + 复制后输入法异常

用户报告两个长期 bug，逐个实锤修复（均有真机证据）：

1. **开机不自启**（fix 6b8bb58）
   - 根因：KWin 在开机时先于 Plasma 托盘拉起输入法，`TrayManager::register()`
     里 `RegisterStatusNotifierItem` 因 `org.kde.StatusNotifierWatcher` 尚未
     上总线而 ServiceUnknown，`?` 传播使 daemon exit(1)。journal 实锤：
     开机 20:10:22 daemon 退出，20:12:23 KWin 二次拉起才成功——中间 2 分钟
     输入法不可用
   - 修复：注册失败改警告 + 后台指数退避重试（1s 起步、封顶 15s），
     托盘服务就绪后自动补注册，daemon 不再退出
2. **复制内容 → 输入法"死掉"**（fix 529308f）
   - 根因：clipboard watcher 的 Dispatch 对创建子对象的 `data_offer` 事件
     未实现 `event_created_child`，wayland-client 派发时 panic，且 panic
     位于不可展开调用栈直接 abort 进程。journal 实锤：systemd-coredump
     xime-daemon + "Missing event_created_child specialization for event
     opcode 0 of zwlr_data_control_device_v1"。ext/zwlr 两后端均已补特化
3. **剪贴板历史始终为空**（fix 4bf960e，调试 2 时顺带发现）
   - 根因：watcher 捕获时 `receive()` 只入队本地缓冲，原代码在阻塞
     `read_to_string` 之后才 `connection.flush()`——compositor 收不到
     receive 请求，源应用永不写 fd，监听线程首次 selection 事件即永久
     挂死（数据库 0 条记录佐证）
   - 修复：flush 提前到阻塞读之前 + 5s 读超时兜底；真机验证 daemon
     启动 24ms 内捕获当前剪贴板并入库
4. **"复制后需切换窗口才能输入中文"= QQ 客户端 bug，非 Xime 问题**（实锤）
   - DEBUG 日志时序证据：用户在 QQ 复制→点回输入框后，KWin 长时间
     （1.5~3.3s，多次）不发 ACTIVATE（期间按键直达应用、无 IM 参与）；
     每一次 ACTIVATE 到达后 daemon 均毫秒级响应处理按键。即 KWin 侧
     就没激活，daemon 侧状态机（失焦清理/重激活）无缺陷
   - 与 2026-08-28 诊断一致：QQ 3.2.29 内置 Chromium 138.0.7204.35 的
     text-input-v3 bug（disable 后不重新 enable，Chromium 139 修复，
     KWin bug 493098）。规避：QQ 内 alt-tab 切走再切回；根治等 QQ 升级
5. **日志系统**：`rolling::never` + 默认 DEBUG 使 `~/.config/xime/xime.log`
   膨胀至 772MB（cosmic_text 渲染日志刷屏）。改为按天轮转
   （xime.log.YYYY-MM-DD）+ 默认 INFO（RUST_LOG 可覆盖），旧文件已清理
6. **运维备忘**：手动 kill launcher/daemon 会被 KWin 记为 "Input Method
   crashed"（QProcess::CrashExit），多次后触发崩溃保护不再自动拉起，
   reconfigure/forceActivate 均无效，需注销重登。调试输入法进程时用
   DBus 退出路径（托盘 Exit → Shutdown）而非 kill

## 本次变更（2026-09-05）③：插件系统应用扩展（参照 Android 版）

遗留待验证：安装真实 webdav-clipboard-sync 插件跑通同步；快捷发送条目需先有 code
（面板 ＋ 按钮添加的条目 code 为空，编码管理 UI 待 xime-setup）。
1. **network.hosts 白名单强制**（libximecore xime-plugin，fail-closed）
   - 未声明网络能力 → host.http.request 全部拒绝；声明 hosts → 仅白名单域名
     （归一化比较：去 scheme/路径/端口、忽略大小写）；allowCustomHosts: true → 放行
   - PluginRuntime::load 签名改为 manifest 驱动；测试 4 个（含 Lua 层拒绝路径）
2. **clipboard_sync 同步桥**（daemon clipboard_sync.rs，对齐 Android ClipboardSyncBridge）
   - 独立线程 SyncBridge：捕获 → push（SHA-256 hash 去重，profile 字段对齐
     Android ClipboardProfile snake_case）；启动/ReloadPlugins/打开剪贴板面板 → pull
     （回声抑制：跳过自己刚推送的 hash；远端条目 upsert_and_trim 入库）
   - Lua 运行时在桥线程内创建；watcher 捕获经回调转发（spawn_watcher_with_callback）
   - 模拟远端插件测试：push 去重/pull 回声抑制/重载 5 个
3. **host.clipboard / host.quickSend 只读 API**（libximecore host_api.rs，能力门禁）
   - ClipboardReadApi/QuickSendReadApi trait（宿主实现、运行时消费，依赖反转）
   - manifest capabilities 强类型解析 RuntimeCaps；声明 clipboard_read/quick_send_read
     且宿主提供实现才注入，否则 host.* 不可见；daemon 侧 ClipboardStore 适配器
4. **text_committed 下行事件**（libximecore deliver_event + daemon 广播）
   - 仅 manifest capabilities.events 订阅且实现 onPluginEvent 的插件被调用
   - daemon 全部上屏路径广播：Rime 提交、表情/符号面板（键盘+点击）、
     剪贴板/快捷发送列表提交（键盘+点击）
5. **快捷发送编码注入候选栏**（daemon wayland.rs，对齐 Android quick-send-demo）
   - Rime 当页候选后追加编码前缀命中条目（comment 显示编码，总位数 ≤9）；
     数字键超出 Rime 候选数的部分宿主接管提交
   - Rime 无候选时完全接管：高亮导航/Return/Space/Esc；字母键落穿 Rime
   - 原始输入 = preedit[..sel_start]；孤儿释放抑制；无候选时清空候选缓存

## 本次变更（2026-09-05）①：主题样式接入 + 亮/暗色模式（移植自 XimeYi UiStyle/ui_colors）
1. **xime-ui 新增 `PanelTheme`**（theme.rs）
   - 字号/圆角/高亮色 + 亮暗两套配色（亮 bg #F5F5F7/暗 bg #24262B，取自 macOS 版 ui_colors）
   - `bar_height()`：候选栏高度随字号自适应（默认 ≤16 保持 36px 命中几何不变，大字号 2×字号+8，上限 72）
   - iced_view 全部硬编码颜色/字号/圆角替换为 theme 驱动；高亮块圆角 = corner_radius-2
2. **ImBackend 接口透传 theme**（v1/v2 同构）：show_candidate_window/show_root_window/candidate_width；
   show_menu_panel 去掉无用的 color 参数；SHM buffer 高度按 bar_height + 面板高度计算
3. **daemon**：从 xime.yaml 构建 theme（font_size/corner_radius/primary_color）；
   `style.candidate_count`（clamp 1..9）限制展示条数；candidate_cache 改存候选+高亮（主题以当前值为准）
4. **亮/暗色检测**：zbus 监听 `org.freedesktop.portal.Settings` 的 color-scheme（KDE/GNOME 标准接口），
   变化 → `DaemonCommand::DarkMode(bool)` → 重建 theme + 候选栏可见时重绘；portal 不可用保持亮色
5. **ReloadStyle 现在重建完整主题**（此前只有 primary_color 生效）
6. 菜单/命中测试几何带 bar_height 参数（menu_button_hit/menu_item_hit/content_item_hit）

## 本次变更（2026-09-05）②：剪贴板 + 快捷发送面板（移植自 XimeYi ximeyi-clipboard + 列表页面板）
1. **新增 `xime-clipboard` crate**
   - store.rs：SQLite 存储原样移植（表 `clipboard_entries`，`user_version=3` 对齐 Android Room v3；
     按 text 去重刷新置顶、上限 1000/20、置顶不裁剪）；db 在 `~/.config/xime/clipboard.db`；测试 11 个
   - watcher.rs：系统剪贴板监听（替代 macOS NSPasteboard 轮询）——独立 Wayland 连接 +
     `ext-data-control-v1`（优先）/`zwlr-data-control-v1`（回退），事件驱动无轮询；
     socketpair 接收 offer 文本（上限 1 MiB），text/plain;charset=utf-8 优先
2. **xime-ui 列表页面板**（menu.rs / iced_view.rs）
   - `PanelView::List { kind, items, highlighted }`，`ListKind::{Clipboard, QuickSend}`
   - 几何：标题栏 40 + 5 行（28+4 间隔）+ 查看全部 30 = 230px；最小宽度 360
   - `list_page_hit`：Back/Clear/More/Row{QuickSend,Remove,None}，绘制与点击共用几何；测试 4 个
   - 渲染：置顶 ★ 标记、超宽截断 `truncate_to_width`（…）、行内 ＋/× 圆角按钮、空态文案
   - `MenuAction::is_available()` 移除（4 个入口全部实现），置灰渲染逻辑删除
3. **ImBackend::show_list_panel**（v1/v2）：仅设置 PanelView 状态，渲染随 show_candidate_window 生效
4. **daemon 接线**
   - main.rs：初始化 store + spawn watcher 线程（常驻）
   - 菜单「剪切板/快捷发送」→ `PanelState::ListOpen(kind)`；空列表不开面板
   - 键盘：Esc 返回菜单页、↑↓ 移动高亮、Return/Space/数字 1-5 提交上屏、普通按键自动收起列表正常输入
   - 指针：行点击上屏（mark_consumed + update_timestamp）、＋ 加入快捷发送、× 删除、清空、← 菜单；
     「查看全部」暂无动作（待 xime-setup 管理页）
   - 提交/删除/清空后面板保持打开并重载列表；列表空时自动关闭

## 诊断记录（2026-08-28）：复制后某些窗口（QQ 最明显）无法输入中文
**现象**：复制内容后在 QQ 聊天窗口打不了中文，托盘图标消失；需切到可输入窗口启用后才恢复。

**根因**（日志 + KWin 6.3.6 源码 + 客户端版本三方实锤）：
1. 复制时点击消息气泡 → 输入框失焦 → QQ 的 Chromium 调 `text-input-v3.disable` → KWin 向 IM 发 DEACTIVATE（journal 可见 `State changed: active=false`，托盘 Passive，与"图标消失"吻合）
2. 点回输入框后 QQ 本应重新 `enable`，但 **QQ 3.2.29 内置 Chromium 138 有 bug：disable 后不再重新 enable**（KWin bug 493098，Chromium 139 修复）；KWin `refreshActive()` 只认客户端 enable → 永不 ACTIVATE
3. 切到其他窗口：那边的应用正常 enable → ACTIVATE 恢复；切回 QQ 时走窗口焦点切换路径才触发 QQ 重新 enable
4. 对照组：VS Code（Chromium/148）正常、Brave 正常——只有旧 Chromium 应用中招
5. journal 佐证：22:32:56 DEACTIVATE 后 15 秒零 ACTIVATE；当日 DEACTIVATE 386 次 vs ACTIVATE 773 次（双 context 异常比例）

**KWin 侧验证过的死路**：DBus `org.kde.kwin.VirtualKeyboard.forceActivate()` 虽存在，但客户端 text-input 处于 disabled 时，KWin 会把中文 commit 丢弃（fake-key 路径仅支持少量 ASCII 键），故 daemon 侧无法单独恢复中文上屏。

**用户级规避**：
- 复制后在 QQ 内 alt-tab 切走再切回（触发 Chromium 焦点路径重新 enable），比切到"能打的窗口"更快
- 等 QQ 升级到 Chromium ≥ 139 的版本；验证方法：`strings /opt/QQ/qq | grep -oE "Chrome/[0-9.]+" | head -1`

**遗留可选功能**（未实现）：daemon 检测死锁态 → DBus forceActivate + zwp_virtual_keyboard 自定义 keymap 合成按键上屏中文（工作量大，需独立功能点）

## 本次变更（2026-08-22）
**菜单面板升级为路由容器：表情/符号网格直接铺在面板区（不占候选栏），剪切板/快捷发送置灰**

## 本次变更（2026-08-22）
1. **Ctrl+Space 启停输入法**（fcitx 风格）
   - 任意状态下 Ctrl+Space 切换全局启停开关（`im_enabled`）
   - 停用：丢弃 rime 组合（`clear_composition` 原始 API）、清空 preedit、隐藏候选栏/菜单/Ctrl 字根、托盘显示英文
   - 停用态按键直接转发不做处理（被消费按下的释放仍抑制，避免孤儿释放）；再次 Ctrl+Space 恢复
   - rime-wubi 配置无 Ctrl+Space 绑定，此前该键被直接转发给应用（功能缺失）
2. **修复内容网格宽度不足**：单元格固定 36px 导致颜文字换行、排列错位
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