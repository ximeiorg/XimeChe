//! 按键消费探针：验证 Rime 对特定组合键（如 Ctrl+A）在「非组词态」
//! 与「组词态」的处理结果——排查"某些应用 Ctrl+A 无法全选"。
//!
//! 用克隆的用户配置目录（运行中的 daemon 持有 userdb 锁，探针不能共用）。
//! 用法：cargo run -p xime-daemon --example key_probe

use librime::key::{K_CONTROL_MASK, K_RELEASE_MASK};
use librime::traits::Traits;

fn main() -> anyhow::Result<()> {
    let home = std::env::var("HOME").unwrap();
    // 克隆用户 rime 目录（含 default.custom.yaml 的 key_binder 绑定）
    let probe_dir = std::path::Path::new("/tmp/xime-key-probe");
    let _ = std::fs::remove_dir_all(probe_dir);
    std::fs::create_dir_all(probe_dir)?;
    let src = format!("{home}/.config/xime/rime");
    let status = std::process::Command::new("cp")
        .args(["-r", &format!("{src}/."), probe_dir.to_str().unwrap()])
        .status()?;
    anyhow::ensure!(status.success(), "克隆 rime 目录失败");

    let mut traits = Traits::new();
    traits
        .set_shared_data_dir(probe_dir.to_str().unwrap())
        .set_user_data_dir(probe_dir.to_str().unwrap())
        .set_distribution_name("XimeProbe")
        .set_distribution_code_name("xime-probe")
        .set_distribution_version("0")
        .set_app_name("rime.xime.key-probe")
        .set_min_log_level(4)
        .set_log_dir("/tmp");
    librime::initialize(&mut traits)?;
    librime::start_maintenance(true)?;
    librime::join_maintenance_thread();

    let mut session = librime::create_session()?;
    let schema = session
        .status()
        .ok()
        .map(|s| s.schema_id().to_string())
        .unwrap_or_default();
    println!("当前方案: {schema}");

    // ── 非组词态（空组合）────────────────────────────────────────
    let consumed = session.process_key(0x61, K_CONTROL_MASK as i32); // Ctrl+A 按下
    println!("[空组合] Ctrl+A 按下 consumed={consumed}");
    let consumed_rel = session.process_key(0x61, K_CONTROL_MASK as i32 | K_RELEASE_MASK as i32);
    println!("[空组合] Ctrl+A 释放 consumed={consumed_rel}");

    // ── 组词态（有组合输入）──────────────────────────────────────
    session.process_key('n' as i32, 0);
    session.process_key('i' as i32, 0);
    let input = session.get_input().unwrap_or("");
    println!("[组词中] input={input:?}");
    let consumed = session.process_key(0x61, K_CONTROL_MASK as i32);
    println!("[组词中] Ctrl+A 按下 consumed={consumed}");
    let input = session.get_input().unwrap_or("");
    println!("[组词中] Ctrl+A 后 input={input:?}");

    drop(session.close());
    librime::finalize();
    Ok(())
}
