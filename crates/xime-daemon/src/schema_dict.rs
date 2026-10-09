//! 方案词表（只读）读取 —— 设置页「输入方案 → 方案词表」。
//!
//! 移植自 XimeYao `winxime-server::schema_dict`。与「用户词典」（可编辑、有
//! 频率）不同，方案词表是随方案分发的码表文件：主码表（方案 yaml 里
//! `translator.dictionary`）+ `import_tables:` 递归引入的子码表 +
//! `translator.packs` 附加码表，BFS 拼成一份词条集合再过滤。
//!
//! 解析对齐 Android `SchemaDictManager`：码表分元信息段与正文段（单独一行
//! `...` 分隔），正文按空白切列取词/码两列，权重无频率语义（回传 commits 恒 0）。
//! 进程内单条缓存（文件签名 = 名字+长度+mtime；含缺失文件），搜索每敲一个字
//! 都会走到这里，签名不变就不碰大码表。
//!
//! 纯文件操作（不碰会话），DBus 线程直调安全；首次解析大码表几百毫秒，
//! 缓存后毫秒级。

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;

use crate::user_dict::{matches_query, DictEntryRow, DICT_ENTRIES_MAX};

/// 一次方案词表读取的结果（字段名与设置端 `SchemaEntriesResult` 对齐）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SchemaDictRead {
    /// 方案主码表名（解析不到时退化为 schema_id）。
    pub dict_name: String,
    /// 实际读入的码表名（主表在前，import_tables / packs 按读入顺序）。
    pub tables: Vec<String>,
    /// 声明了但文件不存在的码表名（供 UI 提示，不算错误）。
    pub missing: Vec<String>,
    /// 读入的词条总数（不受关键词过滤与条数上限影响）。
    pub total: i32,
    /// 关键词命中数（未受条数上限影响）。
    pub matched: i32,
    /// 过滤后回传的词条（commits 恒为 0）。
    pub entries: Vec<DictEntryRow>,
}

/// 码表文本的解析结果。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CodeTable {
    /// 正文段（单独一行 `...` 之后）的词条：`(词, 码)`，第三列权重忽略。
    pub entries: Vec<(String, String)>,
    /// 元信息段声明的子码表名（`import_tables`，保持声明顺序并去重）。
    pub import_tables: Vec<String>,
}

/// 解析码表文本（纯函数）：元信息段只取 `import_tables`；正文段跳过空行与
/// `#` 注释，按空白切列，至少两列（词/码）才算词条。
pub fn parse_code_table_text(text: &str) -> CodeTable {
    let mut entries: Vec<(String, String)> = Vec::new();
    let mut in_data = false;
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if !in_data {
            if line.trim() == "..." {
                in_data = true;
            }
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut columns = trimmed
            .split([' ', '\t'])
            .filter(|column| !column.is_empty());
        let Some(word) = columns.next() else { continue };
        let Some(code) = columns.next() else { continue };
        entries.push((word.to_string(), code.to_string()));
    }
    CodeTable {
        entries,
        import_tables: parse_import_tables(text),
    }
}

/// 解析 `import_tables` 声明（纯函数）：只扫描第一个单独一行 `...` 之前的部分。
pub fn parse_import_tables(text: &str) -> Vec<String> {
    import_tables_from_lines(text.lines().take_while(|line| line.trim() != "..."))
}

/// `import_tables` 行解析：`import_tables: [a, b]` 行内形式与 `- name` 块状形式。
fn import_tables_from_lines<'a>(lines: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut in_block = false;
    for line in lines {
        let trimmed = line.trim();
        if in_block {
            // 块状列表项：`- name`；遇到第一个不是列表项的行就结束声明。
            let Some(item) = trimmed.strip_prefix('-') else {
                break;
            };
            push_unique(&mut out, strip_quotes(item));
            continue;
        }
        let Some(rest) = key_value(trimmed, "import_tables:") else {
            continue;
        };
        if let (Some(open), Some(close)) = (rest.find('['), rest.rfind(']')) {
            if close > open {
                for item in rest[open + 1..close].split(',') {
                    push_unique(&mut out, strip_quotes(item));
                }
            }
            continue;
        }
        in_block = true;
    }
    out
}

/// 解析方案 `translator.packs`（纯函数）：YAML 列表里的标量名，顺序保留。
pub fn parse_packs(schema_text: &str) -> Vec<String> {
    let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(schema_text) else {
        return Vec::new();
    };
    value
        .get("translator")
        .and_then(|translator| translator.get("packs"))
        .and_then(|packs| packs.as_sequence())
        .map(|list| list.iter().filter_map(yaml_scalar).collect())
        .unwrap_or_default()
}

/// 解析方案主码表名（纯函数）：文本里**第一个** `dictionary:` 键的值
/// （字符集 `[A-Za-z0-9_-]`，行尾注释不会误伤）；解析不到用 `schema_id`。
pub fn resolve_dict_name(schema_text: &str, schema_id: &str) -> String {
    for line in schema_text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        let Some(rest) = key_value(trimmed, "dictionary:") else {
            continue;
        };
        let name = dict_name_charset(strip_quotes(rest));
        if !name.is_empty() {
            return name;
        }
    }
    schema_id.to_string()
}

/// 在（已 trim 的）一行里找独立的 `key`，返回其后的原始文本。
fn key_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let mut from = 0usize;
    while let Some(offset) = line[from..].find(key) {
        let start = from + offset;
        let prev_is_identifier = line[..start]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        if !prev_is_identifier {
            return Some(&line[start + key.len()..]);
        }
        from = start + key.len();
    }
    None
}

/// 剥掉成对的单/双引号（并 trim）。
fn strip_quotes(text: &str) -> &str {
    let text = text.trim();
    let bytes = text.as_bytes();
    if bytes.len() >= 2 {
        let (first, last) = (bytes[0], bytes[bytes.len() - 1]);
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return text[1..text.len() - 1].trim();
        }
    }
    text
}

/// 截取码表名字符集 `[A-Za-z0-9_-]` 的前缀。
fn dict_name_charset(text: &str) -> String {
    text.trim()
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect()
}

/// YAML 标量转字符串。
fn yaml_scalar(value: &serde_yaml::Value) -> Option<String> {
    match value {
        serde_yaml::Value::String(text) => Some(text.clone()),
        serde_yaml::Value::Number(number) => Some(number.to_string()),
        serde_yaml::Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

/// 保序去重地追加一个码表名（空名忽略）。
fn push_unique(out: &mut Vec<String>, name: &str) {
    let name = name.trim();
    if name.is_empty() || out.iter().any(|existing| existing == name) {
        return;
    }
    out.push(name.to_string());
}

/// 文件签名：`None` 表示文件不存在，`Some((长度, mtime 纳秒))` 表示存在。
type FileStamp = Option<(u64, i64)>;

/// BFS 遍历的结果。
struct Traversal {
    tables: Vec<String>,
    missing: Vec<String>,
    entries: Vec<(String, String)>,
}

/// 按 BFS 顺序读入主码表 + 其 `import_tables`（递归）+ `packs`。
/// `visited` 负责去重与环保护（A 引 B、B 引 A 时终止）。
fn traverse<F>(dict_name: &str, packs: &[String], mut read: F) -> Traversal
where
    F: FnMut(&str) -> Option<String>,
{
    let mut queue: Vec<String> = Vec::new();
    push_unique(&mut queue, dict_name);
    for pack in packs {
        push_unique(&mut queue, pack);
    }

    let mut visited: HashSet<String> = HashSet::new();
    let mut tables: Vec<String> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    let mut entries: Vec<(String, String)> = Vec::new();

    let mut index = 0usize;
    while index < queue.len() {
        let name = queue[index].clone();
        index += 1;
        if !visited.insert(name.clone()) {
            continue;
        }
        match read(&name) {
            Some(text) => {
                let table = parse_code_table_text(&text);
                entries.extend(table.entries);
                tables.push(name);
                for import in table.import_tables {
                    if !visited.contains(&import) {
                        push_unique(&mut queue, &import);
                    }
                }
            }
            None => missing.push(name),
        }
    }

    Traversal {
        tables,
        missing,
        entries,
    }
}

/// 取文件签名（mtime 取不到记 0）。
fn stamp_of(path: &Path) -> FileStamp {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|delta| delta.as_nanos() as i64)
        .unwrap_or(0);
    Some((meta.len(), mtime))
}

/// 读码表文件（单目录模型，对齐 XimeYao：方案与用户数据同一目录）。
/// 返回 (文本, 签名)；不存在返回 None。
fn find_table(rime_dir: &std::path::Path, name: &str) -> Option<(String, FileStamp)> {
    let path = rime_dir.join(format!("{name}.dict.yaml"));
    let stamp = stamp_of(&path)?;
    let text = std::fs::read_to_string(&path).ok()?;
    Some((text, Some(stamp)))
}

/// 进程内单条缓存：一次已解析的方案词表 + 读过的文件签名。
struct SchemaDictCache {
    dict_name: String,
    packs: Vec<String>,
    tables: Vec<String>,
    missing: Vec<String>,
    total: i32,
    entries: Vec<(String, String)>,
    /// `tables ∪ missing` 里每个名字的签名：全部一致才算命中。
    stamps: Vec<(String, FileStamp)>,
}

impl SchemaDictCache {
    /// 签名是否仍然成立（缺失的名字也必须仍然缺失）。
    ///
    /// 只做 stat（len+mtime），绝不读文件内容——本函数在设置页搜索的
    /// 每次按键都会被调，此前误用 find_table 会把整本码表读进来只为
    /// 比对签名。
    fn matches(&self, rime_dir: &std::path::Path) -> bool {
        self.stamps.iter().all(|(name, stamp)| {
            let path = rime_dir.join(format!("{name}.dict.yaml"));
            stamp_of(&path) == *stamp
        })
    }
}

/// 方案词表缓存（同一时刻只保留"最近读的那一本"）。
static CACHE: Mutex<Option<SchemaDictCache>> = Mutex::new(None);

/// 读取某方案的词表词条，按关键词过滤并截断。
///
/// 只读；主码表文件缺失/读不出才是 `Err`（import_tables/packs 缺的文件记进
/// `missing`）。缓存签名对不上就完整重读。
pub fn read_schema_dict(
    rime_dir: &std::path::Path,
    schema_id: &str,
    query: &str,
) -> Result<SchemaDictRead, String> {
    let schema_id = schema_id.trim();
    if schema_id.is_empty() {
        return Err("方案 id 为空，无法读取方案词表".to_string());
    }

    let schema_text = std::fs::read_to_string(rime_dir.join(format!("{schema_id}.schema.yaml")))
        .unwrap_or_default();
    let dict_name = resolve_dict_name(&schema_text, schema_id);
    let packs = parse_packs(&schema_text);

    // 先查缓存：签名一致就完全不碰磁盘上的大码表。
    {
        let cache = CACHE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(cached) = cache.as_ref() {
            if cached.dict_name == dict_name && cached.packs == packs && cached.matches(rime_dir) {
                // 直接在借用上过滤、只 clone 命中子集：缓存里是全量词条
                //（五笔码表 7-10 万条），先整表 clone 再过滤是每次按键
                // ~20 万次无谓分配。
                let needle = query.trim().to_lowercase();
                let matched: Vec<DictEntryRow> = cached
                    .entries
                    .iter()
                    .filter(|(word, code)| {
                        needle.is_empty()
                            || word.to_lowercase().contains(&needle)
                            || code.to_lowercase().contains(&needle)
                    })
                    .map(|(word, code)| DictEntryRow {
                        word: word.clone(),
                        code: code.clone(),
                        commits: 0,
                    })
                    .collect();
                return Ok(SchemaDictRead {
                    dict_name: cached.dict_name.clone(),
                    tables: cached.tables.clone(),
                    total: cached.total,
                    matched: matched.len() as i32,
                    entries: matched.into_iter().take(DICT_ENTRIES_MAX).collect(),
                    missing: cached.missing.clone(),
                });
            }
        }
    }

    // 冷读：顺手记录每个文件签名（含缺失的，用 None 表示）。
    let mut stamps: Vec<(String, FileStamp)> = Vec::new();
    let traversal = traverse(&dict_name, &packs, |name| {
        match find_table(rime_dir, name) {
            Some((text, stamp)) => {
                stamps.push((name.to_string(), stamp));
                Some(text)
            }
            None => {
                stamps.push((name.to_string(), None));
                None
            }
        }
    });

    let total = traversal.entries.len() as i32;
    {
        let mut cache = CACHE.lock().unwrap_or_else(|p| p.into_inner());
        *cache = Some(SchemaDictCache {
            dict_name: dict_name.clone(),
            packs,
            tables: traversal.tables.clone(),
            missing: traversal.missing.clone(),
            total,
            entries: traversal.entries.clone(),
            stamps,
        });
    }
    if traversal.tables.is_empty() {
        return Err(format!("主码表 {dict_name}.dict.yaml 不存在或不可读"));
    }
    let rows: Vec<DictEntryRow> = traversal
        .entries
        .iter()
        .map(|(word, code)| DictEntryRow {
            word: word.clone(),
            code: code.clone(),
            commits: 0,
        })
        .collect();
    Ok(finish_read(
        &dict_name,
        &traversal.tables,
        &traversal.missing,
        total,
        &rows,
        query,
    ))
}

/// 过滤 + 截断（关键词语义与用户词典浏览一致）。
fn finish_read(
    dict_name: &str,
    tables: &[String],
    missing: &[String],
    total: i32,
    entries: &[DictEntryRow],
    query: &str,
) -> SchemaDictRead {
    let needle = query.trim().to_lowercase();
    let matched_entries: Vec<DictEntryRow> = entries
        .iter()
        .filter(|e| matches_query(e, &needle))
        .cloned()
        .collect();
    SchemaDictRead {
        dict_name: dict_name.to_string(),
        tables: tables.to_vec(),
        total,
        matched: matched_entries.len() as i32,
        entries: matched_entries.into_iter().take(DICT_ENTRIES_MAX).collect(),
        missing: missing.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_data_section_and_import_tables() {
        let text = "# Rime dictionary\n---
name: wubi86\nversion: \"1\"\nsort: original\nimport_tables:\n  - wubi86_extra\n...\
\n工\ta  \n工具\taahw\t15\n# 注释行\n单列\n";
        let table = parse_code_table_text(text);
        assert_eq!(
            table.entries,
            vec![
                ("工".to_string(), "a".to_string()),
                ("工具".to_string(), "aahw".to_string())
            ]
        );
        assert_eq!(table.import_tables, vec!["wubi86_extra"]);
    }

    #[test]
    fn import_tables_inline_form() {
        assert_eq!(
            parse_import_tables("---\nimport_tables: [a, b]\n...\nx\ty\n"),
            vec!["a", "b"]
        );
        assert!(parse_import_tables("---\nother: 1\n...\n").is_empty());
    }

    #[test]
    fn resolves_dict_name_first_key() {
        let text = "---\nname: wubi86\ntranslator:\n  dictionary: wubi86 # 注释\n...\n";
        assert_eq!(resolve_dict_name(text, "fallback"), "wubi86");
        assert_eq!(resolve_dict_name("---\nno_dict: 1\n...\n", "fb"), "fb");
    }

    #[test]
    fn packs_from_yaml() {
        let text = "---\ntranslator:\n  packs:\n    - p1\n    - p2\n...\n";
        assert_eq!(parse_packs(text), vec!["p1", "p2"]);
        assert!(parse_packs("---\nnothing: 1\n...\n").is_empty());
    }

    #[test]
    fn traverse_bfs_dedups_and_guards_cycles() {
        let files: std::collections::HashMap<&str, &str> = [
            ("main", "---\nimport_tables: [sub]\n...\nm\ta\n"),
            ("sub", "---\nimport_tables: [main]\n...\ns\tb\n"),
            ("pack", "---\n...\npk\tc\n"),
        ]
        .into_iter()
        .collect();
        let t = traverse("main", &["pack".to_string()], |name| {
            files.get(name).map(|s| s.to_string())
        });
        assert_eq!(t.tables, vec!["main", "pack", "sub"]); // packs 先入队
        assert!(t.missing.is_empty());
        assert_eq!(t.entries.len(), 3);
    }

    #[test]
    fn read_reports_missing_tables_and_caps_entries() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("small.schema.yaml"),
            "---\ntranslator:\n  dictionary: small\n  packs:\n    - gone\n...\n",
        )
        .unwrap();
        let mut text = String::from("---\nimport_tables: [absent]\n...\n");
        for i in 0..600 {
            text.push_str(&format!("词{i}\tcode{i}\n"));
        }
        std::fs::write(dir.path().join("small.dict.yaml"), text).unwrap();

        let read = read_schema_dict(dir.path(), "small", "").unwrap();
        assert_eq!(read.dict_name, "small");
        assert_eq!(read.tables, vec!["small"]);
        assert_eq!(read.missing, vec!["gone", "absent"]); // packs 先入队
        assert_eq!(read.total, 600);
        assert_eq!(read.matched, 600);
        assert_eq!(read.entries.len(), DICT_ENTRIES_MAX);

        // 缓存命中 + 关键词过滤（code599 只命中 1 条）。
        let filtered = read_schema_dict(dir.path(), "small", "code599").unwrap();
        assert_eq!(filtered.total, 600);
        assert_eq!(filtered.matched, 1);
        assert_eq!(filtered.entries.len(), 1);
        assert_eq!(filtered.entries[0].code, "code599");
    }
}
