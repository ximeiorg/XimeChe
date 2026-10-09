//! 快捷短语（`custom_phrase.txt`）的读取与整表保存（设置页「词典 → 快捷短语」）。
//!
//! 移植自 XimeYao `winxime-server::custom_phrase`，数据形态对齐安卓
//! `PersonalDictManager`（文本格式逐字节一致）：
//! - 文件 `<rime_dir>/<表名>.txt`，表名来自方案配置里 `custom_phrase.user_dict`
//!   （默认 `custom_phrase`），由 `xime_config::custom_phrase_dict_name` 统一解析；
//! - 头部 5 行是 rime 文本码表的标准头；每行 `词⇥编码[⇥权重]`，保存时整表重写；
//! - 首次保存**非空表**时往 `<schema_id>.custom.yaml` 注入
//!   `table_translator@custom_phrase` patch（幂等、注入后不摘除——与安卓一致）；
//! - 保存只写文件与 patch，**不做部署**（设置页的「部署方案」按钮显式触发）。
//!
//! 纯文件操作（不碰会话/userdb），DBus 线程直调安全。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 一条快捷短语（字段名与设置端 `CustomPhraseRow` 序列化对齐）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomPhraseEntry {
    pub word: String,
    pub code: String,
    pub weight: Option<i32>,
}

/// 一次读取的结果（字段名与设置端 `PhraseListResult` 对齐）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PhraseListResult {
    pub dict_name: String,
    pub file_name: String,
    pub file_exists: bool,
    pub patch_applied: bool,
    pub entries: Vec<CustomPhraseEntry>,
}

/// 一次整表保存的结果（字段名与设置端 `PhraseSaveResult` 对齐）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PhraseSaveResult {
    pub dict_name: String,
    pub file_name: String,
    pub file_exists: bool,
    pub patch_applied: bool,
    /// 本次保存是否**新**注入了翻译器（是 → 提示需要重新部署）。
    pub patch_added: bool,
    pub entries: Vec<CustomPhraseEntry>,
}

/// rime 文本码表的标准头（`db_name` 固定写 `custom_phrase`，与安卓版一致）。
const STABLEDB_HEADER: &str = "# Rime table\n\
                               # coding: utf-8\n\
                               #@/db_name\tcustom_phrase\n\
                               #@/db_type\ttabledb\n\
                               #\n";

/// 注入方案 custom.yaml 的翻译器配置块（与安卓 `applyCustomPhraseTranslator`
/// 逐行一致；`engine/translators/+` 是 librime 的列表追加补丁语法）。
const TRANSLATOR_PATCH: &str = concat!(
    "  \"engine/translators/+\":\n",
    "    - table_translator@custom_phrase\n",
    "  \"custom_phrase\":\n",
    "    dictionary: \"\"\n",
    "    user_dict: {dict_name}\n",
    "    db_class: stabledb\n",
    "    enable_completion: false\n",
    "    enable_sentence: false\n",
    "    initial_quality: 99"
);

/// 幂等标记：custom.yaml 里已含该子串就不再注入。
const PATCH_MARKER: &str = "table_translator@custom_phrase";

fn phrase_file(rime_dir: &Path, dict_name: &str) -> PathBuf {
    rime_dir.join(format!("{dict_name}.txt"))
}

fn custom_yaml(rime_dir: &Path, schema_id: &str) -> PathBuf {
    rime_dir.join(format!("{schema_id}.custom.yaml"))
}

/// 解析短语表文本（每行 `词⇥编码[⇥权重]`；`#` 头部、空行跳过）。
pub fn parse_phrase_text(text: &str) -> Vec<CustomPhraseEntry> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let mut fields = line.split('\t');
        let Some(word) = fields.next() else { continue };
        let Some(code) = fields.next() else { continue };
        let weight = fields.next().and_then(|w| w.trim().parse::<i32>().ok());
        if word.trim().is_empty() || code.trim().is_empty() {
            continue;
        }
        out.push(CustomPhraseEntry {
            word: word.trim().to_string(),
            code: code.trim().to_string(),
            weight,
        });
    }
    out
}

/// 把整表写成 rime 文本码表（标准头 + 每行 `词⇥编码[⇥权重]`，文件以换行结尾）。
pub fn build_phrase_text(entries: &[CustomPhraseEntry]) -> String {
    let mut text = String::from(STABLEDB_HEADER);
    for entry in entries {
        text.push_str(&entry.word);
        text.push('\t');
        text.push_str(&entry.code);
        if let Some(weight) = entry.weight {
            text.push('\t');
            text.push_str(&weight.to_string());
        }
        text.push('\n');
    }
    text
}

/// 读取某方案的快捷短语表。
pub fn list_phrases(rime_dir: &Path, schema_id: &str) -> Result<PhraseListResult, String> {
    let dict_name = xime_config::schema_manifest::custom_phrase_dict_name(rime_dir, schema_id);
    let file_name = format!("{dict_name}.txt");
    let file = phrase_file(rime_dir, &dict_name);
    let (file_exists, entries) = match std::fs::read_to_string(&file) {
        Ok(text) => (true, parse_phrase_text(&text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (false, Vec::new()),
        Err(e) => return Err(format!("读取 {file_name} 失败：{e}")),
    };
    let patch_applied = patch_is_applied(rime_dir, schema_id);
    Ok(PhraseListResult {
        dict_name,
        file_name,
        file_exists,
        patch_applied,
        entries,
    })
}

/// custom.yaml 里是否已有翻译器注入（幂等标记）。
fn patch_is_applied(rime_dir: &Path, schema_id: &str) -> bool {
    match std::fs::read_to_string(custom_yaml(rime_dir, schema_id)) {
        Ok(text) => text.contains(PATCH_MARKER),
        Err(_) => false,
    }
}

/// 校验整表输入（逐条修剪并拒绝坏行），返回修剪后的整表。
pub fn validate_phrases(entries: &[CustomPhraseEntry]) -> Result<Vec<CustomPhraseEntry>, String> {
    let mut cleaned = Vec::with_capacity(entries.len());
    for entry in entries {
        let word = entry.word.trim();
        let code = entry.code.trim();
        if word.is_empty() || code.is_empty() {
            return Err("词和编码都要填".to_string());
        }
        for (label, value) in [("词", word), ("编码", code)] {
            if value.contains('\t') || value.contains('\n') || value.contains('\r') {
                return Err(format!("{label}不能含制表符或换行"));
            }
        }
        if let Some(weight) = entry.weight {
            if weight <= 0 {
                return Err("权重要是正整数（留空即默认）".to_string());
            }
        }
        cleaned.push(CustomPhraseEntry {
            word: word.to_string(),
            code: code.to_string(),
            weight: entry.weight,
        });
    }
    Ok(cleaned)
}

/// 整表保存某方案的快捷短语：写文件 + 视需要注入方案 patch。
///
/// **不做部署**——`patch_added` 为 true 时调用方要提示用户重新部署。
pub fn save_phrases(
    rime_dir: &Path,
    schema_id: &str,
    entries: &[CustomPhraseEntry],
) -> Result<PhraseSaveResult, String> {
    let entries = validate_phrases(entries)?;
    let dict_name = xime_config::schema_manifest::custom_phrase_dict_name(rime_dir, schema_id);
    let file_name = format!("{dict_name}.txt");

    let file = phrase_file(rime_dir, &dict_name);
    // 用户整张短语表：必须原子写，崩溃/断电时不能只留半截文件。
    xime_config::atomic_write(&file, build_phrase_text(&entries).as_bytes())
        .map_err(|e| format!("写入 {file_name} 失败：{e}"))?;

    // 仅当至少有一条短语才注入（空表会让 rime 为空表建翻译器而报错）；
    // 已注入过就不再动（幂等）。清空短语时也不摘除——与安卓一致。
    let patch_added = !entries.is_empty()
        && !patch_is_applied(rime_dir, schema_id)
        && apply_translator_patch(rime_dir, schema_id, &dict_name)?;
    let patch_applied = patch_is_applied(rime_dir, schema_id);

    Ok(PhraseSaveResult {
        dict_name,
        file_name,
        file_exists: true,
        patch_applied,
        patch_added,
        entries,
    })
}

/// 往方案的 custom.yaml 注入 `table_translator@custom_phrase` 翻译器（文本级合并）。
///
/// - 已有 `patch:` 块 → 插到 `patch:` 行的下一行（其余内容原样保留）；
/// - 没有 → 去掉结尾 `...` 标记后追加 `patch:\n<块>`；
/// - 文件不存在 → 新建只含 patch 块的文件。
///
/// 不解析 YAML、不重排用户已有配置；幂等性由调用方保证。
fn apply_translator_patch(
    rime_dir: &Path,
    schema_id: &str,
    dict_name: &str,
) -> Result<bool, String> {
    let yaml_path = custom_yaml(rime_dir, schema_id);
    let existing = std::fs::read_to_string(&yaml_path).unwrap_or_default();
    let block = TRANSLATOR_PATCH.replace("{dict_name}", dict_name);

    let patched = if let Some(at) = existing
        .lines()
        .position(|line| line.trim_start().starts_with("patch:"))
    {
        let mut lines: Vec<&str> = existing.lines().collect();
        lines.insert(at + 1, block.as_str());
        let mut text = lines.join("\n");
        text.push('\n');
        text
    } else {
        let mut cleaned = existing
            .trim_end_matches(['\n', '\r', ' '])
            .trim_end_matches("...")
            .trim_end()
            .to_string();
        if !cleaned.is_empty() {
            cleaned.push_str("\n\n");
        }
        cleaned.push_str("patch:\n");
        cleaned.push_str(&block);
        cleaned.push('\n');
        cleaned
    };

    // custom.yaml 半截会导致方案部署失败，同样必须原子写。
    xime_config::atomic_write(&yaml_path, patched.as_bytes())
        .map_err(|e| format!("写入 {}.custom.yaml 失败：{e}", schema_id))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(word: &str, code: &str, weight: Option<i32>) -> CustomPhraseEntry {
        CustomPhraseEntry {
            word: word.to_string(),
            code: code.to_string(),
            weight,
        }
    }

    #[test]
    fn parses_header_and_entries() {
        let text = "# Rime table\n\
                    # coding: utf-8\n\
                    #@/db_name\tcustom_phrase\n\
                    #@/db_type\ttabledb\n\
                    #\n\
                    你好\tnh\n\
                    早上好\tzsh\t3\n\
                    \n\
                    半角行 no-tab\n";
        let entries = parse_phrase_text(text);
        assert_eq!(
            entries,
            vec![entry("你好", "nh", None), entry("早上好", "zsh", Some(3))]
        );
    }

    #[test]
    fn build_text_roundtrips_bytes() {
        let entries = vec![entry("你好", "nh", None), entry("早上好", "zsh", Some(3))];
        let text = build_phrase_text(&entries);
        assert!(text.starts_with(STABLEDB_HEADER));
        assert!(text.ends_with("你好\tnh\n早上好\tzsh\t3\n"));
        assert_eq!(parse_phrase_text(&text), entries);
    }

    #[test]
    fn save_injects_patch_once_and_survives_reload() {
        let dir = tempfile::tempdir().unwrap();
        let entries = vec![entry("好", "hk", None)];
        let out = save_phrases(dir.path(), "wubi86", &entries).unwrap();
        assert!(out.patch_added);
        assert!(out.patch_applied);

        let yaml = std::fs::read_to_string(dir.path().join("wubi86.custom.yaml")).unwrap();
        assert!(yaml.contains("patch:"));
        assert!(yaml.contains("table_translator@custom_phrase"));
        assert!(yaml.contains("user_dict: custom_phrase"));

        // 幂等：再保存不再标记 patch_added。
        let out2 = save_phrases(dir.path(), "wubi86", &entries).unwrap();
        assert!(!out2.patch_added);
        assert!(out2.patch_applied);
    }

    #[test]
    fn save_existing_patch_block_inserts_under_it() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("wubi86.custom.yaml"),
            "patch:\n  \"menu/page_size\": 6\n...\n",
        )
        .unwrap();
        let out = save_phrases(dir.path(), "wubi86", &[entry("测", "ce", None)]).unwrap();
        assert!(out.patch_added);
        let yaml = std::fs::read_to_string(dir.path().join("wubi86.custom.yaml")).unwrap();
        let patch_at = yaml.lines().position(|l| l.trim() == "patch:").unwrap();
        assert!(yaml
            .lines()
            .nth(patch_at + 1)
            .unwrap()
            .contains("engine/translators/+"));
        assert!(
            yaml.contains("\"menu/page_size\": 6"),
            "已有 patch 项要保留"
        );
    }

    #[test]
    fn validate_rejects_bad_rows() {
        assert!(validate_phrases(&[entry("好", "hk", None)]).is_ok());
        assert!(validate_phrases(&[entry("", "hk", None)]).is_err());
        assert!(validate_phrases(&[entry("好", "", None)]).is_err());
        assert!(validate_phrases(&[entry("a\tb", "hk", None)]).is_err());
        assert!(validate_phrases(&[entry("好", "hk", Some(0))]).is_err());
        assert!(validate_phrases(&[entry("好", "hk", Some(3))]).is_ok());
    }
}
