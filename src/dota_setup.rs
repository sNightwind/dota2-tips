//! 把 GSI 配置写入本机 Dota，并在 Steam 退出后修改 Dota 2 的启动项。

use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

const GSI_CFG: &str = include_str!("../gamestate_integration_dota2_tips.cfg");
const CFG_NAME: &str = "gamestate_integration_dota2_tips.cfg";
const LAUNCH_FLAG: &str = "-gamestateintegration";
const APP_ID: &str = "570";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CfgPresence {
    Unknown,
    Absent,
    Same,
    Different,
    Unreadable(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Note {
    #[cfg_attr(windows, allow(dead_code))]
    WindowsOnly,
    SteamNotFound,
    DotaNotFound,
    MultiAccount,
    NoAccount,
    LocalconfigMissing,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupStatus {
    pub dota_dir: Option<PathBuf>,
    pub cfg: CfgPresence,
    pub persona: Option<String>,
    pub flag_present: bool,
    pub launch_readable: bool,
    pub steam_running: bool,
    pub steam_found: bool,
    pub notes: Vec<Note>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CfgOutcome {
    Written,
    Already,
    Removed,
    Absent,
    SkippedNoDota,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchOutcome {
    Added,
    AlreadyPresent,
    Removed,
    AlreadyAbsent,
    SkippedSteamRunning,
    SkippedNoSteam,
    SkippedNoAccount,
    SkippedNoFile,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionReport {
    pub install: bool,
    pub status: SetupStatus,
    pub cfg: CfgOutcome,
    pub launch: LaunchOutcome,
}

struct Found {
    steam_found: bool,
    dota_dir: Option<PathBuf>,
    cfg: CfgPresence,
    persona: Option<String>,
    localconfig: Option<PathBuf>,
    launch_options_known: bool,
    flag_present: bool,
    steam_running: bool,
    account_problem: bool,
    notes: Vec<Note>,
}

pub fn probe() -> SetupStatus {
    discover().into_status()
}

pub fn install() -> ActionReport {
    run_action(true)
}

pub fn uninstall() -> ActionReport {
    run_action(false)
}

fn run_action(install: bool) -> ActionReport {
    #[cfg(not(windows))]
    {
        return ActionReport {
            install,
            status: probe(),
            cfg: CfgOutcome::Failed("仅支持 Windows 自动安装".to_string()),
            launch: LaunchOutcome::SkippedNoSteam,
        };
    }
    #[cfg(windows)]
    {
        let found = discover();
        let cfg = if install {
            install_cfg_from(&found)
        } else {
            remove_cfg_from(&found)
        };
        let launch = apply_launch(&found, install);
        ActionReport {
            install,
            status: discover().into_status(),
            cfg,
            launch,
        }
    }
}

impl Found {
    fn empty() -> Self {
        Self {
            steam_found: false,
            dota_dir: None,
            cfg: CfgPresence::Unknown,
            persona: None,
            localconfig: None,
            launch_options_known: false,
            flag_present: false,
            steam_running: false,
            account_problem: false,
            notes: Vec::new(),
        }
    }

    fn into_status(self) -> SetupStatus {
        SetupStatus {
            dota_dir: self.dota_dir,
            cfg: self.cfg,
            persona: self.persona,
            flag_present: self.flag_present,
            launch_readable: self.launch_options_known,
            steam_running: self.steam_running,
            steam_found: self.steam_found,
            notes: self.notes,
        }
    }
}

#[cfg(windows)]
fn discover() -> Found {
    let mut found = Found::empty();
    found.steam_running = steam_running();
    let Some(steam_dir) = steam_dir_from_registry() else {
        found.notes.push(Note::SteamNotFound);
        return found;
    };
    found.steam_found = true;
    found.dota_dir = find_dota_dir(&steam_dir);
    if let Some(dir) = &found.dota_dir {
        let path = cfg_path(dir);
        found.cfg = cfg_presence(&path);
    } else {
        found.notes.push(Note::DotaNotFound);
    }
    match read_utf8(&steam_dir.join("config").join("loginusers.vdf")) {
        Ok(text) => match pick_account(&text) {
            Ok(account) => {
                if !account.persona.is_empty() {
                    found.persona = Some(account.persona);
                }
                let path = steam_dir
                    .join("userdata")
                    .join(account.id.to_string())
                    .join("config")
                    .join("localconfig.vdf");
                if path.is_file() {
                    found.localconfig = Some(path.clone());
                    match read_encoded(&path) {
                        Ok((body, _)) => match launch_state(&body) {
                            Ok(state) => {
                                found.flag_present = state.flag;
                                found.launch_options_known = true;
                            }
                            Err(error) => found.notes.push(Note::Error(error)),
                        },
                        Err(error) => found.notes.push(Note::Error(error)),
                    }
                } else {
                    found.notes.push(Note::LocalconfigMissing);
                }
            }
            Err(AccountPickError::Multi) => {
                found.account_problem = true;
                found.notes.push(Note::MultiAccount);
            }
            Err(AccountPickError::None) => {
                found.account_problem = true;
                found.notes.push(Note::NoAccount);
            }
            Err(AccountPickError::Read(error)) => {
                found.account_problem = true;
                found.notes.push(Note::Error(error));
            }
        },
        Err(error) if error.kind() == ErrorKind::NotFound => {
            found.account_problem = true;
            found.notes.push(Note::NoAccount);
        }
        Err(error) => {
            found.account_problem = true;
            found
                .notes
                .push(Note::Error(format!("无法读取 loginusers.vdf：{error}")));
        }
    }
    found
}

#[cfg(not(windows))]
fn discover() -> Found {
    let mut found = Found::empty();
    found.notes.push(Note::WindowsOnly);
    found
}

fn install_cfg_from(found: &Found) -> CfgOutcome {
    let Some(dir) = &found.dota_dir else {
        return CfgOutcome::SkippedNoDota;
    };
    match write_cfg(dir) {
        Ok(outcome) => outcome,
        Err(error) => CfgOutcome::Failed(error),
    }
}

fn remove_cfg_from(found: &Found) -> CfgOutcome {
    let Some(dir) = &found.dota_dir else {
        return CfgOutcome::SkippedNoDota;
    };
    match remove_cfg(dir) {
        Ok(outcome) => outcome,
        Err(error) => CfgOutcome::Failed(error),
    }
}

#[cfg(windows)]
fn apply_launch(found: &Found, install: bool) -> LaunchOutcome {
    if !found.steam_found {
        return LaunchOutcome::SkippedNoSteam;
    }
    let Some(path) = &found.localconfig else {
        return if found.account_problem {
            LaunchOutcome::SkippedNoAccount
        } else {
            LaunchOutcome::SkippedNoFile
        };
    };
    if found.launch_options_known && install && found.flag_present {
        return LaunchOutcome::AlreadyPresent;
    }
    if found.launch_options_known && !install && !found.flag_present {
        return LaunchOutcome::AlreadyAbsent;
    }
    write_launch(path, install, steam_running)
}

fn cfg_path(dota_dir: &Path) -> PathBuf {
    dota_dir
        .join("game")
        .join("dota")
        .join("cfg")
        .join("gamestate_integration")
        .join(CFG_NAME)
}

fn cfg_presence(path: &Path) -> CfgPresence {
    match fs::read(path) {
        Ok(bytes) if bytes == GSI_CFG.as_bytes() => CfgPresence::Same,
        Ok(_) => CfgPresence::Different,
        Err(error) if error.kind() == ErrorKind::NotFound => CfgPresence::Absent,
        Err(error) => CfgPresence::Unreadable(error.to_string()),
    }
}

fn write_cfg(dota_dir: &Path) -> Result<CfgOutcome, String> {
    let path = cfg_path(dota_dir);
    if fs::read(&path).ok().as_deref() == Some(GSI_CFG.as_bytes()) {
        return Ok(CfgOutcome::Already);
    }
    replace_bytes(&path, GSI_CFG.as_bytes())
        .map_err(|error| format!("无法写入配置文件：{error}"))?;
    Ok(CfgOutcome::Written)
}

fn remove_cfg(dota_dir: &Path) -> Result<CfgOutcome, String> {
    let path = cfg_path(dota_dir);
    match fs::remove_file(&path) {
        Ok(()) => Ok(CfgOutcome::Removed),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(CfgOutcome::Absent),
        Err(error) => Err(format!("无法删除配置文件：{error}")),
    }
}

fn replace_bytes(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = sibling(path, "tmp");
    fs::write(&tmp, bytes)?;
    if fs::rename(&tmp, path).is_ok() {
        return Ok(());
    }
    if path.exists() {
        fs::remove_file(path)?;
    }
    if fs::rename(&tmp, path).is_err() {
        fs::write(path, bytes)?;
        let _ = fs::remove_file(&tmp);
    }
    Ok(())
}

fn find_dota_dir(steam_dir: &Path) -> Option<PathBuf> {
    let mut libs = Vec::new();
    push_unique(&mut libs, steam_dir.to_path_buf());
    for rel in ["config/libraryfolders.vdf", "steamapps/libraryfolders.vdf"] {
        let Ok(text) = read_utf8(&steam_dir.join(rel)) else {
            continue;
        };
        for lib in library_paths_from_vdf(&text) {
            push_unique(&mut libs, PathBuf::from(lib));
        }
    }
    for lib in libs {
        let manifest = lib
            .join("steamapps")
            .join(format!("appmanifest_{APP_ID}.acf"));
        let Ok(text) = read_utf8(&manifest) else {
            continue;
        };
        let Some(installdir) = manifest_installdir(&text) else {
            continue;
        };
        let dir = lib.join("steamapps").join("common").join(installdir);
        if dir.join("game").join("dota").is_dir() {
            return Some(dir);
        }
    }
    None
}

fn push_unique(dirs: &mut Vec<PathBuf>, path: PathBuf) {
    let next = normalize_dir(&path);
    if dirs.iter().any(|dir| normalize_dir(dir) == next) {
        return;
    }
    dirs.push(path);
}

fn normalize_dir(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}

fn write_launch(path: &Path, install: bool, steam_running: impl Fn() -> bool) -> LaunchOutcome {
    if steam_running() {
        return LaunchOutcome::SkippedSteamRunning;
    }
    let (text, encoding) = match read_encoded(path) {
        Ok(value) => value,
        Err(error) => return LaunchOutcome::Failed(error),
    };
    let edited = match edit_launch_text(&text, install) {
        Ok(value) => value,
        Err(error) => return LaunchOutcome::Failed(error),
    };
    let Some(new_text) = edited else {
        return if install {
            LaunchOutcome::AlreadyPresent
        } else {
            LaunchOutcome::AlreadyAbsent
        };
    };
    if steam_running() {
        return LaunchOutcome::SkippedSteamRunning;
    }
    match write_encoded_with_backup(path, &new_text, encoding) {
        Ok(()) => {
            if install {
                LaunchOutcome::Added
            } else {
                LaunchOutcome::Removed
            }
        }
        Err(error) => LaunchOutcome::Failed(error),
    }
}

fn write_encoded_with_backup(
    path: &Path,
    text: &str,
    encoding: TextEncoding,
) -> Result<(), String> {
    let bytes = encode(text, encoding);
    let tmp = sibling(path, "tmp");
    let bak = sibling(path, "bak");
    fs::write(&tmp, &bytes).map_err(|error| format!("无法写入启动项临时文件：{error}"))?;
    if let Err(error) = fs::copy(path, &bak) {
        let _ = fs::remove_file(&tmp);
        return Err(format!("无法备份 localconfig.vdf：{error}"));
    }
    if fs::rename(&tmp, path).is_err() {
        if let Err(error) = fs::remove_file(path) {
            let _ = fs::remove_file(&tmp);
            return Err(format!("无法替换 localconfig.vdf：{error}"));
        }
        if let Err(error) = fs::rename(&tmp, path) {
            let _ = fs::copy(&bak, path);
            return Err(format!("写入启动项失败，已尝试从备份恢复：{error}"));
        }
    }
    Ok(())
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "localconfig.vdf".to_string());
    path.with_file_name(format!("{name}.dota2-tips.{suffix}"))
}

#[derive(Clone, Copy)]
enum TextEncoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
}

fn read_utf8(path: &Path) -> io::Result<String> {
    let bytes = fs::read(path)?;
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
    String::from_utf8(bytes.to_vec()).map_err(|error| io::Error::new(ErrorKind::InvalidData, error))
}

fn read_encoded(path: &Path) -> Result<(String, TextEncoding), String> {
    let bytes = fs::read(path).map_err(|error| format!("无法读取 {}：{error}", path.display()))?;
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        let text = String::from_utf8(bytes[3..].to_vec())
            .map_err(|error| format!("localconfig.vdf 不是 UTF-8：{error}"))?;
        return Ok((text, TextEncoding::Utf8Bom));
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return decode_utf16(&bytes[2..], true).map(|text| (text, TextEncoding::Utf16Le));
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return decode_utf16(&bytes[2..], false).map(|text| (text, TextEncoding::Utf16Be));
    }
    let text =
        String::from_utf8(bytes).map_err(|error| format!("localconfig.vdf 不是 UTF-8：{error}"))?;
    Ok((text, TextEncoding::Utf8))
}

fn decode_utf16(bytes: &[u8], le: bool) -> Result<String, String> {
    if bytes.len() % 2 != 0 {
        return Err("localconfig.vdf 的 UTF-16 长度无效".to_string());
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|chunk| {
            if le {
                u16::from_le_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_be_bytes([chunk[0], chunk[1]])
            }
        })
        .collect();
    String::from_utf16(&units)
        .map_err(|error| format!("localconfig.vdf 的 UTF-16 无法解码：{error}"))
}

fn encode(text: &str, encoding: TextEncoding) -> Vec<u8> {
    match encoding {
        TextEncoding::Utf8 => text.as_bytes().to_vec(),
        TextEncoding::Utf8Bom => {
            let mut bytes = vec![0xEF, 0xBB, 0xBF];
            bytes.extend_from_slice(text.as_bytes());
            bytes
        }
        TextEncoding::Utf16Le => encode_utf16(text, true),
        TextEncoding::Utf16Be => encode_utf16(text, false),
    }
}

fn encode_utf16(text: &str, le: bool) -> Vec<u8> {
    let mut bytes = if le {
        vec![0xFF, 0xFE]
    } else {
        vec![0xFE, 0xFF]
    };
    for unit in text.encode_utf16() {
        let part = if le {
            unit.to_le_bytes()
        } else {
            unit.to_be_bytes()
        };
        bytes.extend_from_slice(&part);
    }
    bytes
}

struct LaunchState {
    flag: bool,
}

fn launch_state(text: &str) -> Result<LaunchState, String> {
    let flag = match locate(text)? {
        Loc::Value { raw, .. } => has_flag(&raw),
        Loc::MissingOptions { .. } | Loc::MissingApp { .. } => false,
    };
    Ok(LaunchState { flag })
}

fn edit_launch_text(text: &str, install: bool) -> Result<Option<String>, String> {
    let edited = if install {
        insert_flag(text)?
    } else {
        delete_flag(text)?
    };
    if let Some(new_text) = &edited {
        let state = launch_state(new_text)?;
        let ok = if install { state.flag } else { !state.flag };
        if !ok {
            return Err("修改启动项后校验失败，未写入文件".to_string());
        }
    }
    Ok(edited)
}

enum Loc {
    Value {
        span: std::ops::Range<usize>,
        raw: String,
    },
    MissingOptions {
        insert_at: usize,
        prefix: String,
    },
    MissingApp {
        insert_at: usize,
        prefix: String,
    },
}

fn insert_flag(text: &str) -> Result<Option<String>, String> {
    match locate(text)? {
        Loc::Value { span, raw } => {
            if has_flag(&raw) {
                return Ok(None);
            }
            let updated = append_flag(&raw);
            Ok(Some(splice(text, span, &quote_vdf(&updated))))
        }
        Loc::MissingOptions { insert_at, prefix } => Ok(Some(insert_text(
            text,
            insert_at,
            &options_line(&prefix, LAUNCH_FLAG),
        ))),
        Loc::MissingApp { insert_at, prefix } => Ok(Some(insert_text(
            text,
            insert_at,
            &app_block(&prefix, LAUNCH_FLAG),
        ))),
    }
}

fn delete_flag(text: &str) -> Result<Option<String>, String> {
    match locate(text)? {
        Loc::Value { span, raw } => {
            let Some(updated) = strip_flag(&raw) else {
                return Ok(None);
            };
            Ok(Some(splice(text, span, &quote_vdf(&updated))))
        }
        Loc::MissingOptions { .. } | Loc::MissingApp { .. } => Ok(None),
    }
}

fn options_line(prefix: &str, options: &str) -> String {
    format!("{prefix}\"LaunchOptions\"\t\t{}", quote_vdf(options))
}

fn app_block(prefix: &str, options: &str) -> String {
    let inner = format!("{prefix}\t");
    format!(
        "{prefix}\"570\"{prefix}{{{inner}\"LaunchOptions\"\t\t{}{prefix}}}",
        quote_vdf(options)
    )
}

fn locate(text: &str) -> Result<Loc, String> {
    let doc = parse_document(text)?;
    let apps = steam_apps(&doc)?;
    let Some(app) = apps.entries.iter().find(|entry| entry.key == APP_ID) else {
        return Ok(Loc::MissingApp {
            insert_at: apps.open_brace + 1,
            prefix: child_prefix(text, apps.open_brace, apps.close_brace),
        });
    };
    let Vals::Obj(app_obj) = &app.value else {
        return Err("localconfig.vdf 里 570 的内容不是对象，已取消修改".to_string());
    };
    let Some(launch) = app_obj
        .entries
        .iter()
        .find(|entry| entry.key == "LaunchOptions")
    else {
        return Ok(Loc::MissingOptions {
            insert_at: app_obj.open_brace + 1,
            prefix: child_prefix(text, app_obj.open_brace, app_obj.close_brace),
        });
    };
    match &launch.value {
        Vals::Str { text: raw, span } => Ok(Loc::Value {
            span: span.clone(),
            raw: raw.clone(),
        }),
        Vals::Obj(_) => Err("localconfig.vdf 里 LaunchOptions 不是字符串，已取消修改".to_string()),
    }
}

fn steam_apps(doc: &[Entry]) -> Result<&Obj, String> {
    let apps = nested_obj(doc, "UserLocalConfigStore")
        .and_then(|obj| nested_obj(&obj.entries, "Software"))
        .and_then(|obj| nested_obj(&obj.entries, "Valve"))
        .and_then(|obj| nested_obj(&obj.entries, "Steam"))
        .and_then(|obj| nested_obj(&obj.entries, "apps"));
    apps.ok_or_else(|| {
        "localconfig.vdf 缺少 UserLocalConfigStore/Software/Valve/Steam/apps，已取消修改"
            .to_string()
    })
}

fn nested_obj<'a>(entries: &'a [Entry], key: &str) -> Option<&'a Obj> {
    let entry = entries.iter().find(|entry| entry.key == key)?;
    match &entry.value {
        Vals::Obj(obj) => Some(obj),
        Vals::Str { .. } => None,
    }
}

fn child_prefix(text: &str, open_brace: usize, close_brace: usize) -> String {
    let inside = &text[open_brace + 1..close_brace];
    if let Some(newline) = inside.find('\n') {
        let line = &inside[newline + 1..];
        let indent = line
            .chars()
            .take_while(|ch| *ch == ' ' || *ch == '\t')
            .count();
        if indent > 0 && line[indent..].starts_with('"') {
            return format!("\n{}", &line[..indent]);
        }
    }
    "\n\t\t\t\t".to_string()
}

fn splice(text: &str, span: std::ops::Range<usize>, piece: &str) -> String {
    let mut out = String::with_capacity(text.len() - span.len() + piece.len());
    out.push_str(&text[..span.start]);
    out.push_str(piece);
    out.push_str(&text[span.end..]);
    out
}

fn insert_text(text: &str, at: usize, piece: &str) -> String {
    let mut out = String::with_capacity(text.len() + piece.len());
    out.push_str(&text[..at]);
    out.push_str(piece);
    out.push_str(&text[at..]);
    out
}

struct Entry {
    key: String,
    value: Vals,
}

struct Obj {
    open_brace: usize,
    close_brace: usize,
    entries: Vec<Entry>,
}

enum Vals {
    Str {
        text: String,
        span: std::ops::Range<usize>,
    },
    Obj(Obj),
}

fn parse_document(text: &str) -> Result<Vec<Entry>, String> {
    let mut index = 0;
    let mut entries = Vec::new();
    loop {
        skip(text, &mut index);
        if index >= text.len() {
            break;
        }
        if text[index..].starts_with('}') {
            return Err(vdf_error(index, "多余的结束括号"));
        }
        let (key, _) = parse_string(text, &mut index)?;
        let value = parse_value(text, &mut index)?;
        entries.push(Entry { key, value });
    }
    Ok(entries)
}

fn parse_value(text: &str, index: &mut usize) -> Result<Vals, String> {
    skip(text, index);
    if text[*index..].starts_with('{') {
        Ok(Vals::Obj(parse_object(text, index)?))
    } else if text[*index..].starts_with('"') {
        let (value, span) = parse_string(text, index)?;
        Ok(Vals::Str { text: value, span })
    } else {
        Err(vdf_error(*index, "应为值或对象"))
    }
}

fn parse_object(text: &str, index: &mut usize) -> Result<Obj, String> {
    skip(text, index);
    if !text[*index..].starts_with('{') {
        return Err(vdf_error(*index, "应为对象"));
    }
    let open_brace = *index;
    *index += 1;
    let mut entries = Vec::new();
    loop {
        skip(text, index);
        if *index >= text.len() {
            return Err(vdf_error(open_brace, "对象未闭合"));
        }
        if text[*index..].starts_with('}') {
            let close_brace = *index;
            *index += 1;
            return Ok(Obj {
                open_brace,
                close_brace,
                entries,
            });
        }
        let (key, _) = parse_string(text, index)?;
        let value = parse_value(text, index)?;
        entries.push(Entry { key, value });
    }
}

fn parse_string(text: &str, index: &mut usize) -> Result<(String, std::ops::Range<usize>), String> {
    skip(text, index);
    if !text[*index..].starts_with('"') {
        return Err(vdf_error(*index, "应为引号字符串"));
    }
    let start = *index;
    *index += 1;
    let mut out = String::new();
    while *index < text.len() {
        let ch = text[*index..].chars().next().unwrap();
        if ch == '\\' {
            *index += 1;
            let Some(next) = text[*index..].chars().next() else {
                return Err(vdf_error(*index, "转义未结束"));
            };
            match next {
                '\\' | '"' => out.push(next),
                'n' => out.push('\n'),
                't' => out.push('\t'),
                other => {
                    out.push('\\');
                    out.push(other);
                }
            }
            *index += next.len_utf8();
            continue;
        }
        *index += ch.len_utf8();
        if ch == '"' {
            return Ok((out, start..*index));
        }
        out.push(ch);
    }
    Err(vdf_error(start, "引号未闭合"))
}

fn skip(text: &str, index: &mut usize) {
    loop {
        let rest = &text[*index..];
        let trimmed = rest.trim_start_matches(|ch: char| ch.is_whitespace());
        *index += rest.len() - trimmed.len();
        if text[*index..].starts_with("//") {
            match text[*index..].find('\n') {
                Some(rel) => *index += rel + 1,
                None => *index = text.len(),
            }
            continue;
        }
        break;
    }
}

fn vdf_error(index: usize, detail: &str) -> String {
    format!("VDF 在第 {index} 字节附近无法解析：{detail}")
}

fn quote_vdf(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn library_paths_from_vdf(text: &str) -> Vec<String> {
    let Ok(doc) = parse_document(text) else {
        return Vec::new();
    };
    let Some(root) = doc
        .iter()
        .find(|entry| entry.key.eq_ignore_ascii_case("libraryfolders"))
    else {
        return Vec::new();
    };
    let Vals::Obj(obj) = &root.value else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    for entry in &obj.entries {
        match &entry.value {
            Vals::Str { text, .. }
                if entry.key.chars().all(|ch| ch.is_ascii_digit()) && !text.is_empty() =>
            {
                paths.push(text.clone());
            }
            Vals::Obj(child) => {
                if let Some(path) = entry_str(child, "path") {
                    if !path.is_empty() {
                        paths.push(path);
                    }
                }
            }
            _ => {}
        }
    }
    paths
}

fn manifest_installdir(text: &str) -> Option<String> {
    let doc = parse_document(text).ok()?;
    let root = doc.iter().find(|entry| entry.key == "AppState")?;
    let Vals::Obj(obj) = &root.value else {
        return None;
    };
    let dir = entry_str(obj, "installdir")?;
    if dir.is_empty() { None } else { Some(dir) }
}

struct AccountPick {
    persona: String,
    id: u32,
}

#[derive(Debug)]
enum AccountPickError {
    None,
    Multi,
    Read(String),
}

fn pick_account(text: &str) -> Result<AccountPick, AccountPickError> {
    let doc = parse_document(text).map_err(AccountPickError::Read)?;
    let Some(root) = doc.iter().find(|entry| entry.key == "users") else {
        return Err(AccountPickError::None);
    };
    let Vals::Obj(users) = &root.value else {
        return Err(AccountPickError::None);
    };
    struct Row {
        id: u32,
        persona: String,
        recent: bool,
        timestamp: u64,
    }
    let mut rows = Vec::new();
    for entry in &users.entries {
        let Vals::Obj(obj) = &entry.value else {
            continue;
        };
        let Some(id) = account_id(&entry.key) else {
            continue;
        };
        let persona = entry_str(obj, "PersonaName").unwrap_or_default();
        let recent = entry_str(obj, "MostRecent").as_deref() == Some("1");
        let timestamp = entry_str(obj, "Timestamp")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        rows.push(Row {
            id,
            persona,
            recent,
            timestamp,
        });
    }
    if rows.is_empty() {
        return Err(AccountPickError::None);
    }
    let mut recent: Vec<&Row> = rows.iter().filter(|row| row.recent).collect();
    if recent.len() == 1 {
        let row = recent.pop().unwrap();
        return Ok(AccountPick {
            persona: row.persona.clone(),
            id: row.id,
        });
    }
    if recent.len() > 1 {
        recent.sort_by_key(|row| row.timestamp);
        let row = recent.pop().unwrap();
        return Ok(AccountPick {
            persona: row.persona.clone(),
            id: row.id,
        });
    }
    if rows.len() == 1 {
        let row = rows.pop().unwrap();
        return Ok(AccountPick {
            persona: row.persona,
            id: row.id,
        });
    }
    Err(AccountPickError::Multi)
}

fn entry_str(obj: &Obj, key: &str) -> Option<String> {
    let entry = obj
        .entries
        .iter()
        .find(|entry| entry.key.eq_ignore_ascii_case(key))?;
    match &entry.value {
        Vals::Str { text, .. } => Some(text.clone()),
        Vals::Obj(_) => None,
    }
}

fn account_id(steam64: &str) -> Option<u32> {
    const BASE: u64 = 76561197960265728;
    let id: u64 = steam64.parse().ok()?;
    u32::try_from(id.checked_sub(BASE)?).ok()
}

fn has_flag(options: &str) -> bool {
    tokenize(options).iter().any(|token| is_flag(token))
}

fn is_flag(token: &str) -> bool {
    token.eq_ignore_ascii_case(LAUNCH_FLAG)
}

fn append_flag(options: &str) -> String {
    let trimmed = options.trim();
    if trimmed.is_empty() {
        LAUNCH_FLAG.to_string()
    } else {
        format!("{trimmed} {LAUNCH_FLAG}")
    }
}

fn strip_flag(options: &str) -> Option<String> {
    let tokens = tokenize(options);
    if !tokens.iter().any(|token| is_flag(token)) {
        return None;
    }
    let kept: Vec<String> = tokens
        .into_iter()
        .filter(|token| !is_flag(token))
        .map(|token| format_token(&token))
        .collect();
    Some(kept.join(" "))
}

fn tokenize(options: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut chars = options.chars().peekable();
    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }
        if ch == '"' {
            chars.next();
            let mut buf = String::new();
            while let Some(next) = chars.next() {
                if next == '\\' {
                    if let Some(escaped) = chars.next() {
                        buf.push(escaped);
                    }
                } else if next == '"' {
                    break;
                } else {
                    buf.push(next);
                }
            }
            tokens.push(buf);
            continue;
        }
        let mut buf = String::new();
        while let Some(&next) = chars.peek() {
            if next.is_whitespace() {
                break;
            }
            buf.push(next);
            chars.next();
        }
        tokens.push(buf);
    }
    tokens
}

fn format_token(token: &str) -> String {
    if token.is_empty()
        || token
            .chars()
            .any(|ch| ch.is_whitespace() || ch == '"' || ch == '\\')
    {
        let mut inner = String::new();
        for ch in token.chars() {
            match ch {
                '\\' => inner.push_str("\\\\"),
                '"' => inner.push_str("\\\""),
                other => inner.push(other),
            }
        }
        format!("\"{inner}\"")
    } else {
        token.to_string()
    }
}

#[cfg(windows)]
fn steam_dir_from_registry() -> Option<PathBuf> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

    let candidates = [
        (HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamPath"),
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\WOW6432Node\Valve\Steam",
            "InstallPath",
        ),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Valve\Steam", "InstallPath"),
    ];
    for (hive, subkey, value) in candidates {
        let Ok(key) = RegKey::predef(hive).open_subkey(subkey) else {
            continue;
        };
        let Ok(raw) = key.get_value::<String, _>(value) else {
            continue;
        };
        let path = PathBuf::from(raw.trim().trim_matches('"'));
        if path.is_dir() {
            return Some(path);
        }
    }
    None
}

#[cfg(windows)]
fn steam_running() -> bool {
    match process_exes() {
        Ok(names) => names
            .iter()
            .any(|name| name.eq_ignore_ascii_case("steam.exe")),
        Err(_) => true,
    }
}

#[cfg(windows)]
#[repr(C)]
struct ProcessEntry32W {
    size: u32,
    cnt_usage: u32,
    pid: u32,
    heap: usize,
    module: u32,
    threads: u32,
    parent: u32,
    pri: i32,
    flags: u32,
    exe: [u16; 260],
}

#[cfg(windows)]
fn process_exes() -> Result<Vec<String>, String> {
    use std::mem::{size_of, zeroed};

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> isize;
        fn Process32FirstW(snap: isize, entry: *mut ProcessEntry32W) -> i32;
        fn Process32NextW(snap: isize, entry: *mut ProcessEntry32W) -> i32;
        fn CloseHandle(handle: isize) -> i32;
    }

    const TH32CS_SNAPPROCESS: u32 = 0x00000002;
    const INVALID_HANDLE_VALUE: isize = -1;

    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE || snap == 0 {
            return Err("无法枚举进程".to_string());
        }
        let mut entry: ProcessEntry32W = zeroed();
        entry.size = size_of::<ProcessEntry32W>() as u32;
        if Process32FirstW(snap, &mut entry) == 0 {
            CloseHandle(snap);
            return Err("无法读取进程列表".to_string());
        }
        let mut names = Vec::new();
        loop {
            let len = entry
                .exe
                .iter()
                .position(|&unit| unit == 0)
                .unwrap_or(entry.exe.len());
            names.push(String::from_utf16_lossy(&entry.exe[..len]));
            entry.size = size_of::<ProcessEntry32W>() as u32;
            if Process32NextW(snap, &mut entry) == 0 {
                break;
            }
        }
        CloseHandle(snap);
        Ok(names)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dota2-tips-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn vdf_lines(lines: &[&str]) -> String {
        let mut text = lines.join("\n");
        text.push('\n');
        text
    }

    fn shell() -> String {
        vdf_lines(&[
            r#""UserLocalConfigStore""#,
            "{",
            "\t\"Software\"",
            "\t{",
            "\t\t\"Valve\"",
            "\t\t{",
            "\t\t\t\"Steam\"",
            "\t\t\t{",
            "\t\t\t\t\"apps\"",
            "\t\t\t\t{",
            "\t\t\t\t\t\"570\"",
            "\t\t\t\t\t{",
            "\t\t\t\t\t\t\"LastPlayed\"\t\t\"1\"",
            "\t\t\t\t\t}",
            "\t\t\t\t\t\"730\"",
            "\t\t\t\t\t{",
            "\t\t\t\t\t\t\"LaunchOptions\"\t\t\"-novid\"",
            "\t\t\t\t\t}",
            "\t\t\t\t}",
            "\t\t\t}",
            "\t\t}",
            "\t}",
            "}",
            "// TAIL测试",
        ])
    }

    fn shell_without_570() -> String {
        vdf_lines(&[
            r#""UserLocalConfigStore""#,
            "{",
            "\t\"Software\"",
            "\t{",
            "\t\t\"Valve\"",
            "\t\t{",
            "\t\t\t\"Steam\"",
            "\t\t\t{",
            "\t\t\t\t\"apps\"",
            "\t\t\t\t{",
            "\t\t\t\t\t\"730\"",
            "\t\t\t\t\t{",
            "\t\t\t\t\t\t\"LaunchOptions\"\t\t\"-novid\"",
            "\t\t\t\t\t}",
            "\t\t\t\t}",
            "\t\t\t}",
            "\t\t}",
            "\t}",
            "}",
            "// TAIL测试",
        ])
    }

    fn with_options(options: &str) -> String {
        shell().replace(
            "\"LastPlayed\"\t\t\"1\"",
            &format!(
                "\"LastPlayed\"\t\t\"1\"\n\t\t\t\t\t\t\"LaunchOptions\"\t\t{}",
                quote_vdf(options)
            ),
        )
    }

    #[test]
    fn embedded_cfg_matches_the_listener() {
        assert!(GSI_CFG.contains("127.0.0.1:5394"));
        assert!(GSI_CFG.contains("dota2-tips"));
    }

    #[test]
    fn adds_flag_once_and_keeps_the_rest_of_the_file() {
        let once = edit_launch_text(&shell(), true).unwrap().unwrap();
        let state = launch_state(&once).unwrap();
        assert!(state.flag);
        assert!(once.contains("TAIL测试"));
        assert!(once.contains("\"730\""));
        assert_eq!(launch_value_of(&once, "730").as_deref(), Some("-novid"));
        assert!(edit_launch_text(&once, true).unwrap().is_none());
        assert_eq!(once.matches(LAUNCH_FLAG).count(), 1);
    }

    #[test]
    fn appends_to_existing_options_without_touching_other_apps() {
        let original = with_options("-novid   -console");
        let edited = edit_launch_text(&original, true).unwrap().unwrap();
        assert_eq!(
            launch_value_of(&edited, "570").as_deref(),
            Some("-novid   -console -gamestateintegration")
        );
        assert_eq!(launch_value_of(&edited, "730").as_deref(), Some("-novid"));
        let prefix = original.split("\"LaunchOptions\"").next().unwrap();
        assert!(edited.starts_with(prefix));
        assert!(edited.ends_with("TAIL测试\n"));
    }

    #[test]
    fn existing_flag_is_case_insensitive_and_not_rewritten() {
        let original = with_options("-novid -GameStateIntegration");
        assert!(edit_launch_text(&original, true).unwrap().is_none());
        assert!(
            edit_launch_text(&with_options("-gamestateintegrationextra"), true)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn empty_options_become_the_flag() {
        let edited = edit_launch_text(&with_options(""), true).unwrap().unwrap();
        assert_eq!(
            launch_value_of(&edited, "570").as_deref(),
            Some(LAUNCH_FLAG)
        );
    }

    #[test]
    fn removes_only_the_flag_and_keeps_other_tokens() {
        let original = with_options("-novid -gamestateintegration -console \"my path\"");
        let edited = edit_launch_text(&original, true).unwrap();
        assert!(edited.is_none());
        let removed = edit_launch_text(&original, false).unwrap().unwrap();
        assert_eq!(
            launch_value_of(&removed, "570").as_deref(),
            Some("-novid -console \"my path\"")
        );
        assert!(edit_launch_text(&removed, false).unwrap().is_none());
        assert_eq!(launch_value_of(&removed, "730").as_deref(), Some("-novid"));
    }

    #[test]
    fn missing_app_block_is_inserted_and_crlf_survives() {
        let text = shell_without_570().replace('\n', "\r\n");
        assert!(!text.contains("\"570\""));
        let edited = edit_launch_text(&text, true).unwrap().unwrap();
        assert!(edited.contains("\r\n"));
        assert!(launch_state(&edited).unwrap().flag);
        assert_eq!(launch_value_of(&edited, "730").as_deref(), Some("-novid"));
        assert!(edit_launch_text(&edited, false).unwrap().is_some());
    }

    #[test]
    fn refuses_to_edit_when_apps_is_missing() {
        let error = edit_launch_text("\"UserLocalConfigStore\"\n{\n}\n", true).unwrap_err();
        assert!(error.contains("apps"));
    }

    #[test]
    fn library_paths_cover_current_and_old_formats() {
        let current = "\
\"libraryfolders\"
{
\t\"0\"
\t{
\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"
\t\t\"apps\"
\t\t{
\t\t\t\"570\"\t\t\"12\"
\t\t}
\t}
}
";
        assert_eq!(
            library_paths_from_vdf(current),
            vec!["C:\\Program Files (x86)\\Steam".to_string()]
        );
        let old = "\
\"LibraryFolders\"
{
\t\"TimeNextStatsReport\"\t\t\"1\"
\t\"ContentStatsID\"\t\t\"2\"
\t\"1\"\t\t\"D:\\\\SteamLibrary\"
}
";
        assert_eq!(
            library_paths_from_vdf(old),
            vec!["D:\\SteamLibrary".to_string()]
        );
    }

    #[test]
    fn manifest_keeps_installdir_spaces() {
        let text = "\"AppState\"\n{\n\t\"installdir\"\t\t\"dota 2 beta\"\n}\n";
        assert_eq!(manifest_installdir(text).as_deref(), Some("dota 2 beta"));
    }

    #[test]
    fn account_pick_uses_most_recent_then_single_user() {
        assert_eq!(account_id("76561197960265770"), Some(42));
        let recent = "\
\"users\"
{
\t\"76561197960265770\"
\t{
\t\t\"PersonaName\"\t\t\"Older\"
\t\t\"MostRecent\"\t\t\"1\"
\t\t\"Timestamp\"\t\t\"10\"
\t}
\t\"76561197960265800\"
\t{
\t\t\"PersonaName\"\t\t\"Newer\"
\t\t\"MostRecent\"\t\t\"1\"
\t\t\"Timestamp\"\t\t\"20\"
\t}
}
";
        let picked = pick_account(recent).unwrap();
        assert_eq!(picked.persona, "Newer");
        assert_eq!(picked.id, 72);
        let single = "\
\"users\"
{
\t\"76561197960265770\"
\t{
\t\t\"PersonaName\"\t\t\"Solo\"
\t}
}
";
        let solo = pick_account(single).unwrap();
        assert_eq!(solo.id, 42);
        let multi = "\
\"users\"
{
\t\"76561197960265770\"
\t{
\t\t\"PersonaName\"\t\t\"A\"
\t}
\t\"76561197960265800\"
\t{
\t\t\"PersonaName\"\t\t\"B\"
\t}
}
";
        assert!(matches!(pick_account(multi), Err(AccountPickError::Multi)));
    }

    #[test]
    fn finds_dota_from_a_library_folder() {
        let root = workspace("library");
        let steam = root.join("steam");
        let lib = root.join("library");
        let dota = lib
            .join("steamapps")
            .join("common")
            .join("dota 2 beta")
            .join("game")
            .join("dota");
        fs::create_dir_all(&dota).unwrap();
        fs::create_dir_all(steam.join("config")).unwrap();
        fs::write(
            lib.join("steamapps").join("appmanifest_570.acf"),
            "\"AppState\"\n{\n\t\"installdir\"\t\t\"dota 2 beta\"\n}\n",
        )
        .unwrap();
        let listed = format!(
            "\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t{}\n\t}}\n}}\n",
            quote_vdf(&lib.to_string_lossy())
        );
        fs::write(steam.join("config").join("libraryfolders.vdf"), listed).unwrap();
        let found = find_dota_dir(&steam).unwrap();
        assert!(found.join("game").join("dota").is_dir());
    }

    #[test]
    fn cfg_write_is_idempotent_and_remove_only_deletes_our_file() {
        let root = workspace("cfg");
        let dota = root.join("dota");
        fs::create_dir_all(dota.join("game").join("dota")).unwrap();
        let other = cfg_path(&dota).with_file_name("gamestate_integration_other.cfg");
        fs::create_dir_all(other.parent().unwrap()).unwrap();
        fs::write(&other, "keep").unwrap();
        assert_eq!(write_cfg(&dota).unwrap(), CfgOutcome::Written);
        assert_eq!(cfg_presence(&cfg_path(&dota)), CfgPresence::Same);
        assert_eq!(write_cfg(&dota).unwrap(), CfgOutcome::Already);
        fs::write(cfg_path(&dota), "stale").unwrap();
        assert_eq!(cfg_presence(&cfg_path(&dota)), CfgPresence::Different);
        assert_eq!(write_cfg(&dota).unwrap(), CfgOutcome::Written);
        assert_eq!(fs::read(cfg_path(&dota)).unwrap(), GSI_CFG.as_bytes());
        assert_eq!(remove_cfg(&dota).unwrap(), CfgOutcome::Removed);
        assert_eq!(remove_cfg(&dota).unwrap(), CfgOutcome::Absent);
        assert_eq!(fs::read_to_string(other).unwrap(), "keep");
    }

    #[test]
    fn launch_file_backup_and_steam_guard() {
        let root = workspace("launch");
        let path = root.join("localconfig.vdf");
        fs::write(&path, with_options("-novid")).unwrap();
        let blocked = write_launch(&path, true, || true);
        assert_eq!(blocked, LaunchOutcome::SkippedSteamRunning);
        assert!(!sibling(&path, "bak").exists());
        assert_eq!(
            launch_value_of(&fs::read_to_string(&path).unwrap(), "570").as_deref(),
            Some("-novid")
        );

        let calls = std::cell::Cell::new(0);
        let raced = write_launch(&path, true, || {
            let seen = calls.get() + 1;
            calls.set(seen);
            seen == 2
        });
        assert_eq!(raced, LaunchOutcome::SkippedSteamRunning);
        assert!(!sibling(&path, "bak").exists());

        assert_eq!(write_launch(&path, true, || false), LaunchOutcome::Added);
        let bak = fs::read_to_string(sibling(&path, "bak")).unwrap();
        assert_eq!(launch_value_of(&bak, "570").as_deref(), Some("-novid"));
        let updated = fs::read_to_string(&path).unwrap();
        assert_eq!(
            launch_value_of(&updated, "570").as_deref(),
            Some("-novid -gamestateintegration")
        );
        assert_eq!(
            write_launch(&path, true, || false),
            LaunchOutcome::AlreadyPresent
        );
        assert_eq!(write_launch(&path, false, || false), LaunchOutcome::Removed);
        let removed = fs::read_to_string(&path).unwrap();
        assert_eq!(launch_value_of(&removed, "570").as_deref(), Some("-novid"));
    }

    #[test]
    fn utf16_launch_file_roundtrips() {
        let root = workspace("utf16");
        let path = root.join("localconfig.vdf");
        let original = with_options("-console");
        fs::write(&path, encode(&original, TextEncoding::Utf16Le)).unwrap();
        assert_eq!(write_launch(&path, true, || false), LaunchOutcome::Added);
        let (text, encoding) = read_encoded(&path).unwrap();
        assert!(matches!(encoding, TextEncoding::Utf16Le));
        assert_eq!(
            launch_value_of(&text, "570").as_deref(),
            Some("-console -gamestateintegration")
        );
        assert!(text.contains("TAIL测试"));
    }

    fn launch_value_of(text: &str, app: &str) -> Option<String> {
        let doc = parse_document(text).ok()?;
        let apps = steam_apps(&doc).ok()?;
        let app = apps.entries.iter().find(|entry| entry.key == app)?;
        let Vals::Obj(obj) = &app.value else {
            return None;
        };
        entry_str(obj, "LaunchOptions")
    }

    #[cfg(windows)]
    #[test]
    fn toolhelp_lists_processes() {
        #[cfg(target_pointer_width = "64")]
        assert_eq!(std::mem::size_of::<ProcessEntry32W>(), 568);
        let names = process_exes().expect("toolhelp");
        assert!(
            names
                .iter()
                .any(|name| name.to_ascii_lowercase().ends_with(".exe"))
        );
    }
}
