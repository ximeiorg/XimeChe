use std::fs::File;
use std::path::PathBuf;
use xime_setup_lib::speech_models::SpeechServerStatus;
use xime_setup_lib::state::{
    CustomPhraseRow, DictEntriesResult, DictListResult, PhraseListResult, PhraseSaveResult,
    SchemaEntriesResult,
};
use xime_setup_lib::{
    set_notify_deploy, set_notify_dict_backup, set_notify_dict_entries,
    set_notify_dict_entry_write, set_notify_dict_export, set_notify_dict_import,
    set_notify_dict_list, set_notify_dict_restore, set_notify_phrase_list, set_notify_phrase_save,
    set_notify_reload_plugins, set_notify_reload_style, set_notify_schema_entries,
    set_notify_select_schema, set_notify_speech_delete, set_notify_speech_download,
    set_notify_speech_select, set_notify_speech_status, set_notify_speech_test_start,
    set_notify_speech_test_stop, set_notify_sync_user_data,
};

/// 调 daemon 的单字符串参数 DBus 方法并返回 JSON 应答文本。
fn daemon_call_json1(method: &str, a: &str) -> Option<String> {
    let conn = zbus::blocking::Connection::session().ok()?;
    let reply = conn
        .call_method(
            Some("org.xime.Xime"),
            "/org/xime/Xime",
            Some("org.xime.Xime.Controller"),
            method,
            &(a,),
        )
        .ok()?;
    reply.body().deserialize::<String>().ok()
}

fn speech_status_from_json(json: &str) -> Option<SpeechServerStatus> {
    serde_json::from_str(json).ok()
}

/// 语音状态回调（250ms 轮询；daemon 不可达 = offline）。
fn speech_status_cb() -> Option<SpeechServerStatus> {
    daemon_call_json0("GetSpeechStatus").and_then(|json| speech_status_from_json(&json))
}

/// 模型操作回调（下载/删除/选择）：调 daemon 后回读最新状态。
fn speech_model_action_cb(method: &str, id: &str) -> Option<SpeechServerStatus> {
    daemon_call_json1(method, id)?;
    speech_status_cb()
}

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

    // 语音转文本（voice-page）：状态轮询 + 模型管理 + 试听，全部走 daemon
    // DBus（daemon 是语音数据源与操作执行者，设置页只是镜像 UI）。
    set_notify_speech_status(speech_status_cb);
    set_notify_speech_download(|id| speech_model_action_cb("DownloadSpeechModel", id));
    set_notify_speech_delete(|id| speech_model_action_cb("DeleteSpeechModel", id));
    set_notify_speech_select(|id| speech_model_action_cb("SelectSpeechModel", id));
    set_notify_speech_test_start(|| {
        daemon_call_json0("SpeechTestStart").and_then(|json| speech_status_from_json(&json))
    });
    set_notify_speech_test_stop(|| {
        daemon_call_json0("SpeechTestStop").and_then(|json| speech_status_from_json(&json))
    });

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
