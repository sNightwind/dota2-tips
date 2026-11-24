use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub language: String,
    pub hero_name: String,
    pub bars: Bars,
    pub rules: Rules,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bars {
    pub camp: BarCfg,
    pub bounty: BarCfg,
    pub power: BarCfg,
    pub items: BarCfg,
    pub enemies: EnemyCfg,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BarCfg {
    pub enabled: bool,
    pub style: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EnemyCfg {
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rules {
    pub camp_first_sec: u32,
    pub camp_interval_sec: u32,
    pub bounty_first_sec: u32,
    pub bounty_interval_sec: u32,
    pub power_first_sec: u32,
    pub power_interval_sec: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            language: "zh".to_string(),
            hero_name: "full".to_string(),
            bars: Bars::default(),
            rules: Rules::default(),
        }
    }
}

impl Default for Bars {
    fn default() -> Self {
        Self {
            camp: BarCfg::on(),
            bounty: BarCfg::on(),
            power: BarCfg::on(),
            items: BarCfg::on(),
            enemies: EnemyCfg { enabled: true },
        }
    }
}

impl Default for BarCfg {
    fn default() -> Self {
        Self::on()
    }
}

impl BarCfg {
    fn on() -> Self {
        Self {
            enabled: true,
            style: "rect".to_string(),
        }
    }

    pub fn is_ring(&self) -> bool {
        self.style == "ring"
    }
}

impl Default for EnemyCfg {
    fn default() -> Self {
        Self { enabled: true }
    }
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            camp_first_sec: 60,
            camp_interval_sec: 60,
            bounty_first_sec: 0,
            bounty_interval_sec: 180,
            power_first_sec: 360,
            power_interval_sec: 120,
        }
    }
}

impl Config {
    pub fn language_is_en(&self) -> bool {
        self.language == "en"
    }

    pub fn hero_name_is_short(&self) -> bool {
        self.hero_name == "short"
    }
}

pub fn beside_exe() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|dir| dir.join("config.toml")))
        .unwrap_or_else(|| PathBuf::from("config.toml"))
}

pub fn load_or_create(path: &Path) -> Result<Config, String> {
    if !path.exists() {
        let config = Config::default();
        save(path, &config)?;
        return Ok(config);
    }
    let text = fs::read_to_string(path).map_err(|error| format!("无法读取配置：{error}"))?;
    toml::from_str(&text).map_err(|error| format!("配置无法解析：{error}"))
}

pub fn save(path: &Path, config: &Config) -> Result<(), String> {
    let body = toml::to_string_pretty(config).map_err(|error| format!("配置无法序列化：{error}"))?;
    let text = format!(
        "# dota2-tips 玩家配置。设置界面会写回这个文件。\n# language: zh 或 en。hero_name: full 或 short。style: rect 或 ring。\n# 时间单位是秒。野怪默认 1:00 后每 60 秒，赏金符从 0:00 起每 180 秒，神符从 6:00 起每 120 秒。\n\n{body}"
    );
    write_atomic(path, text.as_bytes()).map_err(|error| format!("无法写入配置：{error}"))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, bytes)?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_roundtrip_and_missing_fields_stay_enabled() {
        let text = toml::to_string_pretty(&Config::default()).unwrap();
        let loaded: Config = toml::from_str(&text).unwrap();
        assert_eq!(loaded, Config::default());

        let partial: Config = toml::from_str("language = \"en\"\n").unwrap();
        assert_eq!(partial.language, "en");
        assert!(partial.bars.camp.enabled);
        assert_eq!(partial.rules.power_first_sec, 360);
    }
}
