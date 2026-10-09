use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use chrono::Local;
use tracing::{debug, error, info};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use xime_tray::{MenuAction, TrayManager};
use zbus::Connection;

use xime_daemon::{DaemonCommand, WaylandLoop, XimeDaemon};

fn get_log_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    std::path::PathBuf::from(home).join(".config/xime")
}

fn init_tracing() -> WorkerGuard {
    let log_dir = get_log_dir();

    if !log_dir.exists() {
        std::fs::create_dir_all(&log_dir).ok();
    }

    struct LocalTimer;
    impl FormatTime for LocalTimer {
        fn format_time(
            &self,
            w: &mut tracing_subscriber::fmt::format::Writer<'_>,
        ) -> std::fmt::Result {
            write!(w, "{}", Local::now().format("%Y-%m-%dT%H:%M:%S%.6f%:z"))
        }
    }

    // 按天轮转，避免单文件无限增长。
    // 默认级别：三方库 info + 自家 crate 取配置 `log_level`（xime.yaml，
    // 排障时改 debug 即可，无需环境变量）；RUST_LOG 仍可整体覆盖。
    let file_appender = tracing_appender::rolling::daily(&log_dir, "xime.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_timer(LocalTimer);

    let stdout_layer = fmt::layer().with_writer(std::io::stderr).with_ansi(true);

    // RUST_LOG 显式覆盖时句柄置 None（不注册热重载，配置 log_level 无效，
    // 也不能让 ReloadStyle 把用户显式指定的过滤器冲掉）。
    let (filter_layer, handle): (
        tracing_subscriber::reload::Layer<EnvFilter, tracing_subscriber::Registry>,
        Option<tracing_subscriber::reload::Handle<EnvFilter, tracing_subscriber::Registry>>,
    ) = if let Ok(env_filter) = EnvFilter::try_from_default_env() {
        // RUST_LOG 显式覆盖：句柄弃用（不注册），配置 log_level 无效——
        // 否则 ReloadStyle 会把用户显式指定的过滤器冲掉。
        let (layer, _handle) = tracing_subscriber::reload::Layer::new(env_filter);
        (layer, None)
    } else {
        let cfg = xime_config::XimeConfig::load();
        let (layer, handle) =
            tracing_subscriber::reload::Layer::new(xime_daemon::build_log_filter(&cfg));
        // RUST_LOG 未设：注册句柄，ReloadStyle 时配置 log_level 热生效。
        (layer, Some(handle))
    };
    if let Some(handle) = handle {
        let _ = xime_daemon::LOG_FILTER_HANDLE.set(handle);
    }

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(file_layer)
        .with(stdout_layer)
        .init();

    guard
}

/// panic 必须落文件日志：DBus 激活进程的 stderr 落 journal（用户找不到），
/// 而 non_blocking 缓冲在进程暴死时未刷——不补这一条，用户提交的日志里
/// 会丢掉崩溃现场最后几行（恰恰是最关键的）。默认 hook 仍执行，保留
/// journal 侧输出。
fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic payload>".to_string());
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown>".to_string());
        let thread = std::thread::current()
            .name()
            .unwrap_or("<unnamed>")
            .to_string();
        error!(
            "PANIC thread={thread} location={location}: {payload} (pid={}, version={})",
            std::process::id(),
            env!("CARGO_PKG_VERSION")
        );
        default_hook(info);
    }));
}

fn main() -> anyhow::Result<()> {
    // 元数据先于日志初始化：init_tracing 要读 XimeConfig::load()，
    // 其 user_config_path 依赖 config_dir_name（默认元数据也是 xime，但
    // 显式先注入，避免与默认值漂移）。
    let _ = xime_config::set_app_metadata(xime_config::AppMetadata {
        display_name: "曦码·澈输入法",
        config_dir_name: "xime",
        config_file_base: "xime",
        distribution_name: "XimeChe",
        distribution_code_name: "ximeche",
        app_name: "rime.xime.daemon",
        version: env!("CARGO_PKG_VERSION"),
    });

    let _guard = init_tracing();
    install_panic_hook();
    // pid/版本进日志：方便和 coredumpctl / journal 对上，以及判断用户
    // 提交的日志出自哪个版本。
    info!(
        "xime-daemon starting (pid={}, version={})",
        std::process::id(),
        env!("CARGO_PKG_VERSION")
    );

    // 单目录模型（对齐 XimeYao / Xime 3.0）：shared == user == ~/.config/xime/rime。
    // 随包方案数据（dev-install 装到 ~/.local/share/xime/rime-data 或系统的
    // /usr/share/xime/rime-data）降级为「数据源」，启动时部署进 rime 目录：
    // 首装全量、升级只强更内容有变化且非 custom 的文件（保护用户定制与弃用方案）。
    // 旧 shared/user 分离导致方案来源混乱（码表在 shared、用户数据在 user，
    // 词典/词表读取到处回退查找）。
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let rime_dir = std::path::PathBuf::from(&home).join(".config/xime/rime");
    let bundled_sources = [
        std::path::PathBuf::from(&home).join(".local/share/xime/rime-data"),
        std::path::PathBuf::from("/usr/share/xime/rime-data"),
    ];
    xime_config::ensure_bundled_rime_data(&bundled_sources, &rime_dir);
    let paths = xime_config::RimePaths {
        shared_data_dir: rime_dir.clone(),
        user_data_dir: rime_dir,
    };
    info!("rime dir (single): {}", paths.user_data_dir.display());
    let _ = xime_config::set_rime_paths(paths);

    let rt = tokio::runtime::Runtime::new()?;
    let rt_handle = rt.handle().clone();

    rt.block_on(async {
        let connection = Connection::session().await?;

        let (tray, mut toggle_rx, mut action_rx) = TrayManager::register(&connection).await?;
        let tray = Arc::new(tray);

        let (command_tx, command_rx) = mpsc::channel();

        // 剪贴板/快捷发送存储（SQLite，表结构对齐 Android）+ 系统剪贴板监听
        // + 剪贴板同步桥（clipboard_sync 插件，独立线程避免网络阻塞按键）。
        let clipboard_store = xime_clipboard::store::init(xime_clipboard::default_db_dir());
        let (sync_tx, sync_rx) = mpsc::channel();
        xime_daemon::spawn_bridge(clipboard_store.clone(), sync_rx);
        let watcher_sync_tx = sync_tx.clone();
        xime_clipboard::watcher::spawn_watcher_with_callback(
            clipboard_store.clone(),
            Some(Box::new(move |text| {
                let _ = watcher_sync_tx.send(xime_daemon::SyncMessage::Captured(text.to_string()));
            })),
        );

        // 系统亮/暗色模式监听（org.freedesktop.portal.Settings 的
        // color-scheme，KDE/GNOME 均支持）。portal 不可用时保持亮色。
        rt.spawn({
            let connection = connection.clone();
            let command_tx = command_tx.clone();
            async move {
                if let Err(e) = watch_color_scheme(connection, command_tx).await {
                    debug!("Color scheme watcher unavailable: {}", e);
                }
            }
        });

        thread::spawn({
            let tray = tray.clone();
            let rt_handle = rt_handle.clone();
            move || {
                let wayland_loop =
                    WaylandLoop::new(command_rx, tray, rt_handle, clipboard_store, sync_tx);
                wayland_loop.run();
            }
        });

        let daemon = XimeDaemon::new(command_tx.clone());

        connection
            .object_server()
            .at("/org/xime/Xime", daemon)
            .await?;

        connection.request_name("org.xime.Xime").await?;

        info!("DBus service registered at org.xime.Xime");
        info!("Tray registered (background retry if watcher was not up yet)");

        // 托盘常驻显示（fcitx5 风格）：图标是 IM 未激活时唯一的控制入口，
        // 隐藏会让用户在输入法"卡死"（KWin 未激活）时失去恢复手段。
        tray.set_visible(true).await;
        info!("Waiting for Wayland connection from launcher...");

        loop {
            tokio::select! {
                Some(_) = toggle_rx.recv() => {
                    debug!("Toggle request received from tray");
                    command_tx.send(DaemonCommand::ToggleMode).ok();
                }
                Some(action) = action_rx.recv() => {
                    debug!("Menu action received: {:?}", action);
                    match action {
                        MenuAction::ToggleMode => {
                            command_tx.send(DaemonCommand::ToggleMode).ok();
                        }
                        MenuAction::Settings => {
                            let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
                            let setup_path = format!("{}/.local/bin/xime-setup", home);
                            std::process::Command::new(&setup_path)
                                .spawn()
                                .map_err(|e| {
                                    error!("Failed to launch xime-setup: {}", e);
                                    e
                                })
                                .ok();
                        }
                        MenuAction::Deploy => {
                            command_tx.send(DaemonCommand::Deploy).ok();
                        }
                        MenuAction::SelectSchema(schema_id) => {
                            // 结果接收端即弃：托盘切换不关心结果，
                            // wayland 侧 send 失败已被 `let _` 忽略。
                            let (result_tx, _result_rx) = tokio::sync::oneshot::channel();
                            command_tx
                                .send(DaemonCommand::SelectSchema(schema_id, result_tx))
                                .ok();
                        }
                        MenuAction::Exit => {
                            command_tx.send(DaemonCommand::Shutdown).ok();
                            break;
                        }
                    }
                }
            }
        }

        // Exit 到这里。不能让 main 正常返回：libc exit() 会跑 atexit，撞上
        // librime 静态析构段错误（coredump ×3 实锤），且抢在 wayland 线程
        // clean_exit 的 _exit(0) 之前。永久挂起，把进程终结权交给 wayland
        // 线程（DBus 服务对象随之自然失效，shutdown 语义不变）。
        std::future::pending::<()>().await;

        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    })?;

    Ok(())
}

/// 监听 portal 的 `color-scheme` 设置变化，向 daemon 发送 DarkMode 命令。
/// 值语义：0 = 无偏好，1 = 偏好暗色，2 = 偏好亮色。
async fn watch_color_scheme(
    connection: Connection,
    command_tx: mpsc::Sender<DaemonCommand>,
) -> zbus::Result<()> {
    use futures_lite::StreamExt;
    use zbus::zvariant::OwnedValue;

    let proxy = zbus::Proxy::new(
        &connection,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Settings",
    )
    .await?;

    // 初值：Read 返回 (v)，v 为 u32
    let reply: (OwnedValue,) = proxy
        .call("Read", &("org.freedesktop.appearance", "color-scheme"))
        .await?;
    if let Ok(mode) = reply.0.downcast_ref::<u32>() {
        info!("System color scheme: mode={}", mode);
        let _ = command_tx.send(DaemonCommand::DarkMode(mode == 1));
    }

    let mut changes = proxy.receive_signal("SettingChanged").await?;
    while let Some(msg) = changes.next().await {
        let Ok((namespace, key, value)) = msg.body().deserialize::<(String, String, OwnedValue)>()
        else {
            continue;
        };
        if namespace == "org.freedesktop.appearance" && key == "color-scheme" {
            if let Ok(mode) = value.downcast_ref::<u32>() {
                debug!("System color scheme changed: mode={}", mode);
                let _ = command_tx.send(DaemonCommand::DarkMode(mode == 1));
            }
        }
    }
    Ok(())
}
