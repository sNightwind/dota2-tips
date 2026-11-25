use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct FieldStat {
    pub types: BTreeSet<String>,
    pub example: String,
    pub seen: u64,
}

#[derive(Debug, Default)]
pub struct Catalog {
    pub posts: u64,
    pub fields: BTreeMap<String, FieldStat>,
}

impl Catalog {
    /// Record every path in `value`. Returns paths that appeared for the first time.
    pub fn observe(&mut self, value: &Value) -> Vec<String> {
        let mut new_paths = Vec::new();
        walk(value, "", self, &mut new_paths);
        self.posts += 1;
        new_paths.sort();
        new_paths
    }

    pub fn write_reports(&self, dir: &Path, latest: &Value) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        write_atomic(
            &dir.join("latest.json"),
            &serde_json::to_vec_pretty(latest).unwrap(),
        )?;
        write_atomic(
            &dir.join("fields.json"),
            &serde_json::to_vec_pretty(&self.to_json()).unwrap(),
        )?;
        write_atomic(&dir.join("fields.txt"), self.to_text().as_bytes())?;
        Ok(())
    }

    pub fn save_sample(&self, dir: &Path, game_state: &str, body: &Value) -> io::Result<PathBuf> {
        let samples = dir.join("samples");
        fs::create_dir_all(&samples)?;
        let name = sanitize_state(game_state);
        let path = samples.join(format!("{name}.json"));
        write_atomic(&path, &serde_json::to_vec_pretty(body).unwrap())?;
        Ok(path)
    }

    fn to_json(&self) -> Value {
        let fields: Vec<Value> = self
            .fields
            .iter()
            .map(|(path, stat)| {
                serde_json::json!({
                    "path": path,
                    "types": stat.types.iter().collect::<Vec<_>>(),
                    "example": stat.example,
                    "seen": stat.seen,
                })
            })
            .collect();
        serde_json::json!({
            "posts": self.posts,
            "field_count": self.fields.len(),
            "fields": fields,
        })
    }

    fn to_text(&self) -> String {
        let mut out = format!(
            "已接收 {posts} 份合法载荷，共 {count} 个字段路径。\n路径按字典序排列。token 的示例值已打码。\n\n",
            posts = self.posts,
            count = self.fields.len(),
        );
        for (path, stat) in &self.fields {
            let types = stat.types.iter().cloned().collect::<Vec<_>>().join(",");
            out.push_str(&format!(
                "{path}\t{types}\tseen={seen}\texample={example}\n",
                seen = stat.seen,
                example = stat.example,
            ));
        }
        out
    }
}

pub fn auth_matches(value: &Value, expected: &str) -> bool {
    value
        .get("auth")
        .map(|auth| contains_string(auth, expected))
        .unwrap_or(false)
}

pub fn game_state(value: &Value) -> Option<&str> {
    value.pointer("/map/game_state").and_then(Value::as_str)
}

fn contains_string(value: &Value, expected: &str) -> bool {
    match value {
        Value::String(text) => text == expected,
        Value::Array(items) => items.iter().any(|item| contains_string(item, expected)),
        Value::Object(map) => map.values().any(|item| contains_string(item, expected)),
        _ => false,
    }
}

fn walk(value: &Value, path: &str, catalog: &mut Catalog, new_paths: &mut Vec<String>) {
    if !path.is_empty() {
        record(catalog, new_paths, path, value);
    }
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for key in keys {
                let child = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                walk(&map[key], &child, catalog, new_paths);
            }
        }
        Value::Array(items) => {
            let child = format!("{path}[]");
            for item in items {
                walk(item, &child, catalog, new_paths);
            }
        }
        _ => {}
    }
}

fn record(catalog: &mut Catalog, new_paths: &mut Vec<String>, path: &str, value: &Value) {
    let is_new = !catalog.fields.contains_key(path);
    let stat = catalog.fields.entry(path.to_string()).or_default();
    stat.types.insert(type_name(value).to_string());
    stat.seen += 1;
    stat.example = redact(path, example_of(value));
    if is_new {
        new_paths.push(path.to_string());
    }
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn example_of(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => truncate(text, 80),
        Value::Array(items) => format!("len={}", items.len()),
        Value::Object(map) => format!("keys={}", map.len()),
    }
}

fn redact(path: &str, example: String) -> String {
    let secret = path.split('.').any(|part| {
        let bare = part.trim_end_matches("[]");
        bare == "token" || bare.starts_with("key")
    });
    if secret {
        "<redacted>".to_string()
    } else {
        example
    }
}

fn truncate(text: &str, max: usize) -> String {
    let one_line = text.replace(['\n', '\r', '\t'], " ");
    let mut end = one_line.len();
    if end > max {
        end = one_line
            .char_indices()
            .map(|(index, _)| index)
            .take_while(|index| *index <= max)
            .last()
            .unwrap_or(0);
        return format!("{}…", &one_line[..end]);
    }
    one_line
}

fn sanitize_state(state: &str) -> String {
    let mut name: String = state
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if name.is_empty() {
        name = "unknown".to_string();
    }
    name
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

impl Default for FieldStat {
    fn default() -> Self {
        Self {
            types: BTreeSet::new(),
            example: String::new(),
            seen: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn records_nested_objects_and_array_elements_once() {
        let value = json!({
            "auth": {"token": "secret"},
            "map": {"clock_time": 12, "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS"},
            "abilities": [{"name": "axe_berserkers_call", "cooldown": 0}]
        });
        let mut catalog = Catalog::default();
        let first = catalog.observe(&value);
        assert!(first.iter().any(|path| path == "map.clock_time"));
        assert!(first.iter().any(|path| path == "abilities[].name"));
        assert_eq!(catalog.fields["auth.token"].example, "<redacted>");
        assert_eq!(
            game_state(&value),
            Some("DOTA_GAMERULES_STATE_GAME_IN_PROGRESS")
        );

        let second = catalog.observe(&value);
        assert!(second.is_empty());
        assert_eq!(catalog.posts, 2);
        assert!(auth_matches(&value, "secret"));
        assert!(!auth_matches(&value, "other"));
    }
}
