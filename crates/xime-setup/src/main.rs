use std::fs::File;
use std::path::PathBuf;
use xime_setup_lib::state::{
    CustomPhraseRow, DictEntriesResult, DictListResult, PhraseListResult, PhraseSaveResult,
    SchemaEntriesResult,
};
use xime_setup_lib::{
    set_notify_deploy, set_notify_dict_backup, set_notify_dict_entries,
    set_notify_dict_entry_write, set_notify_dict_export, set_notify_dict_import,
    set_notify_dict_list, set_notify_dict_restore, set_notify_phrase_list, set_notify_phrase_save,
    set_notify_reload_plugins, set_notify_reload_style, set_notify_schema_entries,
    set_notify_select_schema, set_notify_sync_user_data,
};
// 语音输入页（voice-page）：页面只画 UI，引擎在独立的 xime-speechd 进程里，
// 这里注入的回调就是「设置端 → speechd socket」这一跳。
use xime_setup_lib::{
    set_notify_speech_delete, set_notify_speech_download, set_notify_speech_import,
    set_notify_speech_select, set_notify_speech_status, set_notify_speech_test_start,
    set_notify_speech_test_stop, SpeechServerStatus,
};
use xime_speech_service::transport::Client;
use xime_speech_service::{Request, Response};

/// 调 daemon 的无参 DBus 方法并返回 JSON 应答文本。
fn daemon_call_json0(method: &str) -> Option<String> {
    let conn = zbus::blocking::Connection::session().ok()?;
    let reply = conn
        .call_method(
            Some("org.xime.Xime"),
            "/org/xime/Xime",
            Some("org.xime.Xime.Controller"),
            method,
            &(),
        )
        .ok()?;
    reply.body().deserialize::<String>().ok()
}

/// 调 daemon 的双字符串参数 DBus 方法并返回 JSON 应答文本。
fn daemon_call_json2(method: &str, a: &str, b: &str) -> Option<String> {
    let conn = zbus::blocking::Connection::session().ok()?;
    let reply = conn
        .call_method(
            Some("org.xime.Xime"),
            "/org/xime/Xime",
            Some("org.xime.Xime.Controller"),
            method,
            &(a, b),
        )
        .ok()?;
    reply.body().deserialize::<String>().ok()
}

fn dict_list_from_json(json: &str) -> Option<DictListResult> {
    serde_json::from_str(json).ok()
}

fn dict_entries_from_json(json: &str) -> Option<DictEntriesResult> {
    serde_json::from_str(json).ok()
}

/// 调 daemon 的双字符串参数 DBus 方法，错误透传（设置页要展示原因）。
fn daemon_call_json2_err(method: &str, a: &str, b: &str) -> Result<String, String> {
    let conn = zbus::blocking::Connection::session().map_err(|e| e.to_string())?;
    let reply = conn
        .call_method(
            Some("org.xime.Xime"),
            "/org/xime/Xime",
            Some("org.xime.Xime.Controller"),
            method,
            &(a, b),
        )
        .map_err(|e| e.to_string())?;
    reply
        .body()
        .deserialize::<String>()
        .map_err(|e| e.to_string())
}

fn phrase_list_cb(schema_id: &str) -> Option<PhraseListResult> {
    daemon_call_json2_err("ListCustomPhrases", schema_id, "")
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
}

fn phrase_save_cb(schema_id: &str, entries: &[CustomPhraseRow]) -> Option<PhraseSaveResult> {
    let entries_json = serde_json::to_string(entries).ok()?;
    daemon_call_json2_err("SaveCustomPhrases", schema_id, &entries_json)
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
}

/// 调 daemon 的 UserDictOp（op 为 UserDictOp 的 JSON），返回条数；错误透传。
///
/// daemon 的 `UserDictOp` 以 DBus 错误返回失败原因（如「恢复失败：…」），
/// 设置页要把它显示给用户，所以这里不能像早期版本那样 `.ok()` 掉——那样
/// 失败与「返回 0 条」在界面上无法区分。
fn user_dict_op_json_result(op_json: String) -> Result<i64, String> {
    let conn = zbus::blocking::Connection::session().map_err(|e| e.to_string())?;
    let reply = conn
        .call_method(
            Some("org.xime.Xime"),
            "/org/xime/Xime",
            Some("org.xime.Xime.Controller"),
            "UserDictOp",
            &(op_json,),
        )
        .map_err(|e| e.to_string())?;
    reply.body().deserialize::<i64>().map_err(|e| e.to_string())
}

/// [`user_dict_op_json_result`] 的宽松包装：只关心成败/条数的调用方用这个。
fn user_dict_op_json(op_json: String) -> Option<i64> {
    user_dict_op_json_result(op_json).ok()
}

/// 拼一个字符串字段版的 UserDictOp JSON（值做 JSON 转义）。
fn op_json(op: &str, kvs: &[(&str, String)]) -> String {
    let mut s = format!(r#"{{"op":"{op}""#);
    for (k, v) in kvs {
        let v = v.replace('\\', "\\\\").replace('"', "\\\"");
        s.push_str(&format!(r#","{k}":"{v}""#));
    }
    s.push('}');
    s
}

// ---------------------------------------------------------------------------
// 语音输入（voice-page）宿主回调
//
// 引擎跑在 `xime-speechd` 里（模型 1.9GB + 麦克风都归它），设置进程只通过
// `~/.config/xime/speech.sock` 的 JSON-line 协议读写状态。候选栏 🎙️ 走的是
// 同一个 socket —— 所以两边看到的是同一个引擎、同一个当前模型。
// ---------------------------------------------------------------------------

/// 设置端已尝试拉起过 speechd（失败不再重试，否则 250ms 轮询会反复 fork）。
static SPEECH_SPAWNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn speech_dir() -> PathBuf {
    // 与 daemon 同一套约定：xime.db 所在目录就是配置根。
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    PathBuf::from(home).join(".config/xime")
}

fn speech_socket() -> PathBuf {
    speech_dir().join("speech.sock")
}

/// `xime-speechd` 与设置程序同目录安装（`~/.local/bin` 或 `/usr/bin`）。
fn speechd_binary() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("xime-speechd");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join("xime-speechd"))
        .find(|path| path.is_file())
}

/// 连上 speechd；连不上就顺手拉起来一次。
///
/// 设置页要能独立管理模型——用户打开设置不该先去输入框把 daemon 唤起来。
/// 拉起是异步的（模型装载要几秒），本次返回 None，下一轮轮询自然接上。
fn speech_client() -> Option<Client> {
    let endpoint = speech_socket();
    if let Ok(client) = Client::connect(&endpoint.to_string_lossy()) {
        return Some(client);
    }
    use std::sync::atomic::Ordering;
    if SPEECH_SPAWNED.swap(true, Ordering::Relaxed) {
        return None;
    }
    let binary = speechd_binary()?;
    // 后端不传：speechd 启动时自己探测（编了 cuda 且显卡可用就走 GPU）。
    std::process::Command::new(binary)
        .arg("--socket")
        .arg(&endpoint)
        .arg("--models")
        .arg(speech_dir().join("models"))
        .arg("--idle-secs")
        .arg("300")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    None
}

fn speech_call(request: Request) -> Option<Response> {
    let mut client = speech_client()?;
    client.call(&request).ok()
}

/// 协议 `Status` → 设置页镜像（字段是子集，多出来的 level/committed 自动忽略）。
///
/// `ok=false` 时协议把原因放在顶层 `error`，页面读的是 `status.error`，
/// 这里把它搬过去——否则「切换失败」在界面上只会显示成刷新了一下。
fn speech_status_of(response: Response) -> Option<SpeechServerStatus> {
    let error = response.error;
    let mut status = match response.status {
        Some(status) => status,
        None => speech_call(Request::Status)?.status?,
    };
    if status.error.is_none() {
        status.error = error;
    }
    serde_json::to_value(&status)
        .ok()
        .and_then(|value| serde_json::from_value(value).ok())
}

fn speech_status_cb() -> Option<SpeechServerStatus> {
    speech_status_of(speech_call(Request::Status)?)
}

fn speech_select_cb(model_id: &str) -> Option<SpeechServerStatus> {
    speech_status_of(speech_call(Request::Select {
        model_id: model_id.to_string(),
    })?)
}

fn speech_delete_cb(model_id: &str) -> Option<SpeechServerStatus> {
    speech_status_of(speech_call(Request::Remove {
        model_id: model_id.to_string(),
    })?)
}

fn speech_test_start_cb() -> Option<SpeechServerStatus> {
    speech_status_of(speech_call(Request::ListenStart)?)
}

fn speech_test_stop_cb() -> Option<SpeechServerStatus> {
    speech_status_of(speech_call(Request::ListenStop)?)
}

/// 导入：选目录 → 让 speechd 复制进模型根目录（目录结构只有它知道）。
fn speech_import_cb() -> Option<SpeechServerStatus> {
    let dir = rfd::FileDialog::new()
        .set_title("选择语音模型目录（应含 config.json 与 model.safetensors）")
        .pick_folder()?;
    speech_status_of(speech_call(Request::Import {
        path: dir.display().to_string(),
    })?)
}

/// 在线下载：speechd 目前只扫描本地模型目录，没有下载器。
///
/// 如实说明比假装「服务未运行」好——后者会把用户引去修一个不存在的故障。
fn speech_download_cb(model_id: &str) -> Option<SpeechServerStatus> {
    let mut status = speech_call(Request::Status)?.status?;
    if status.error.is_none() {
        status.error = Some(format!(
            "当前版本不支持在线下载：把「{model_id}」的权重目录准备好后用「导入模型…」添加"
        ));
    }
    serde_json::to_value(&status)
        .ok()
        .and_then(|value| serde_json::from_value(value).ok())
}

fn get_lock_file_path() -> PathBuf {
    std::env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .ok()
        .or_else(|| {
            std::env::var("HOME")
                .map(|home| PathBuf::from(home).join(".local/share/xime"))
                .ok()
        })
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("xime-setup.lock")
}

fn try_acquire_singleton_lock() -> bool {
    let lock_path = get_lock_file_path();
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    match File::create(&lock_path) {
        Ok(f) => {
            use nix::fcntl::{Flock, FlockArg};
            use std::mem::forget;
            match Flock::lock(f, FlockArg::LockExclusiveNonblock) {
                Ok(flock) => {
                    forget(flock);
                    true
                }
                Err(_) => false,
            }
        }
        Err(_) => false,
    }
}

fn main() -> iced::Result {
    if !try_acquire_singleton_lock() {
        tracing::info!("xime-setup is already running, exiting...");
        return Ok(());
    }

    set_notify_deploy(|| {
        if let Ok(conn) = zbus::blocking::Connection::session() {
            let _ = conn.call_method(
                Some("org.xime.Xime"),
                "/org/xime/Xime",
                Some("org.xime.Xime.Controller"),
                "Deploy",
                &(),
            );
        }
    });
    set_notify_reload_style(|| {
        if let Ok(conn) = zbus::blocking::Connection::session() {
            let _ = conn.call_method(
                Some("org.xime.Xime"),
                "/org/xime/Xime",
                Some("org.xime.Xime.Controller"),
                "ReloadStyle",
                &(),
            );
        }
    });
    set_notify_select_schema(|schema_id| {
        let Ok(conn) = zbus::blocking::Connection::session() else {
            return false;
        };
        let Ok(reply) = conn.call_method(
            Some("org.xime.Xime"),
            "/org/xime/Xime",
            Some("org.xime.Xime.Controller"),
            "SelectSchema",
            &(schema_id,),
        ) else {
            return false;
        };
        reply.body().deserialize::<bool>().unwrap_or(false)
    });
    set_notify_reload_plugins(|| {
        if let Ok(conn) = zbus::blocking::Connection::session() {
            let _ = conn.call_method(
                Some("org.xime.Xime"),
                "/org/xime/Xime",
                Some("org.xime.Xime.Controller"),
                "ReloadPlugins",
                &(),
            );
        }
    });

    // 词典管理（dict-page）：数据通道 = daemon DBus（levers 导出临时文件在
    // daemon 进程执行，设置进程只收 JSON）。
    set_notify_dict_list(|| {
        daemon_call_json0("ListUserDicts").and_then(|json| dict_list_from_json(&json))
    });
    set_notify_dict_entries(|dict, query| {
        daemon_call_json2("ListDictEntries", dict, query)
            .and_then(|json| dict_entries_from_json(&json))
    });
    // 写路径：单一 UserDictOp 方法（参数为 op 的 JSON，返回条数；失败走 DBus 错误）。
    // 注意：set_notify_* 接收 fn 指针，helper 必须是无捕获的独立函数。
    set_notify_dict_backup(|dict| {
        user_dict_op_json(op_json("backup", &[("dict", dict.to_string())])).is_some()
    });
    set_notify_dict_restore(|path| {
        user_dict_op_json_result(op_json("restore", &[("path", path.to_string())])).map(|_| ())
    });
    set_notify_dict_export(|dict, path| {
        user_dict_op_json_result(op_json(
            "export",
            &[("dict", dict.to_string()), ("path", path.to_string())],
        ))
        .map(|n| n as i32)
    });
    set_notify_dict_import(|dict, path| {
        user_dict_op_json_result(op_json(
            "import",
            &[("dict", dict.to_string()), ("path", path.to_string())],
        ))
        .map(|n| n as i32)
    });
    set_notify_dict_entry_write(|dict, word, code, commits| {
        let json = format!(
            r#"{{"op":"write_entry","dict":"{}","word":"{}","code":"{}","commits":{commits}}}"#,
            dict.replace('\\', "\\\\").replace('"', "\\\""),
            word.replace('\\', "\\\\").replace('"', "\\\""),
            code.replace('\\', "\\\\").replace('"', "\\\""),
        );
        user_dict_op_json(json).map(|n| n as i32)
    });
    // 快捷短语（词典页第二个 Tab）：读取/整表保存，纯文件操作走 DBus JSON。
    set_notify_phrase_list(phrase_list_cb);
    set_notify_phrase_save(phrase_save_cb);
    // 方案词表（输入方案页第三个 Tab）：只读浏览。
    set_notify_schema_entries(|schema_id, query| {
        daemon_call_json2_err("ListSchemaEntries", schema_id, query)
            .ok()
            .and_then(|json| serde_json::from_str::<SchemaEntriesResult>(&json).ok())
    });
    // rime 用户资料同步（同步与备份页「立即同步」，wayland 线程关会话执行）。
    set_notify_sync_user_data(|| user_dict_op_json(op_json("sync", &[])).is_some());

    // 语音输入（voice-page）：注册了状态回调，页面才认为「本平台接入了本地引擎」
    // （否则整页显示"当前平台未接入"）。其余六项对应页面上的按钮。
    set_notify_speech_status(speech_status_cb);
    set_notify_speech_download(speech_download_cb);
    set_notify_speech_delete(speech_delete_cb);
    set_notify_speech_select(speech_select_cb);
    set_notify_speech_test_start(speech_test_start_cb);
    set_notify_speech_test_stop(speech_test_stop_cb);
    set_notify_speech_import(speech_import_cb);

    // 注入应用元数据（目录沿用 xime，librime 分发标识为 XimeChe）。
    let _ = xime_setup_lib::set_app_metadata(xime_setup_lib::AppMetadata {
        display_name: "曦码·澈输入法",
        settings_name: "曦码·澈输入法设置",
        subtitle: "输入法设置",
        tagline: "基于 Rime 引擎的五笔输入法",
        homepage: "https://github.com/ximeiorg/XimeChe",
        config_dir_name: "xime",
        config_file_base: "xime",
        distribution_name: "XimeChe",
        distribution_code_name: "ximeche",
        app_name: "rime.xime.setup",
        version: env!("CARGO_PKG_VERSION"),
    });

    // 单目录模型（对齐 daemon）：shared == user == ~/.config/xime/rime；
    // 随包方案数据在启动时部署进 rime 目录（与 daemon 同一套 ensure 逻辑，
    // 两端看到同一份方案来源）。
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let rime_dir = std::path::PathBuf::from(&home).join(".config/xime/rime");
    let bundled_sources = [
        std::path::PathBuf::from(&home).join(".local/share/xime/rime-data"),
        std::path::PathBuf::from("/usr/share/xime/rime-data"),
    ];
    xime_setup_lib::ensure_bundled_rime_data(&bundled_sources, &rime_dir);
    let _ = xime_setup_lib::set_rime_paths(xime_setup_lib::RimePaths {
        shared_data_dir: rime_dir.clone(),
        user_data_dir: rime_dir,
    });

    xime_setup_lib::run()
}
