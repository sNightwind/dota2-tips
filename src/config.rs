use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// 缺省按旧文件处理，读到 1 时改成 7.42 的神符时间并写成 2。
    #[serde(default = "previous_schema")]
    pub schema: u32,
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
    pub water: BarCfg,
    pub power: BarCfg,
    pub stack: BarCfg,
    pub items: BarCfg,
    pub enemies: EnemyCfg,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BarCfg {
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EnemyCfg {
    pub visible: bool,
    pub missing: bool,
    pub missing_after_sec: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rules {
    pub camp_first_sec: u32,
    pub camp_interval_sec: u32,
    pub bounty_first_sec: u32,
    pub bounty_interval_sec: u32,
    pub water_first_sec: u32,
    pub water_second_sec: u32,
    pub power_first_sec: u32,
    pub power_interval_sec: u32,
    pub stack_left_sec: u32,
    pub stack_right_sec: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema: CURRENT_SCHEMA,
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
            water: BarCfg::on(),
            power: BarCfg::on(),
            stack: BarCfg::on(),
            items: BarCfg::on(),
            enemies: EnemyCfg {
                visible: true,
                missing: true,
                missing_after_sec: 5,
            },
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
        Self { enabled: true }
    }
}

impl Default for EnemyCfg {
    fn default() -> Self {
        Self {
            visible: true,
            missing: true,
            missing_after_sec: 5,
        }
    }
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            camp_first_sec: 60,
            camp_interval_sec: 60,
            bounty_first_sec: 0,
            bounty_interval_sec: 240,
            water_first_sec: 120,
            water_second_sec: 240,
            power_first_sec: 360,
            power_interval_sec: 120,
            stack_left_sec: 53,
            stack_right_sec: 55,
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

    pub fn hero_name_is_both(&self) -> bool {
        self.hero_name == "both"
    }
}

const CURRENT_SCHEMA: u32 = 3;

pub fn beside_exe() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|dir| dir.join("config.toml")))
        .unwrap_or_else(|| PathBuf::from("config.toml"))
}

fn previous_schema() -> u32 {
    1
}

/// 生成缺失的配置，或按当前版本补上新字段并去掉失效字段。
/// 返回值第二个布尔表示文件是否被改写。
pub fn repair(path: &Path) -> Result<(Config, bool), String> {
    if !path.exists() {
        let config = Config::default();
        save(path, &config)?;
        return Ok((config, true));
    }
    let text = fs::read_to_string(path).map_err(|error| format!("无法读取配置：{error}"))?;
    let mut config: Config = toml::from_str(&text).map_err(|error| format!("配置无法解析：{error}"))?;
    normalize(&mut config);
    let changed = stored_value(&text)? != stored_value(&render(&config)?)?;
    if changed {
        save(path, &config)?;
    }
    Ok((config, changed))
}

fn normalize(config: &mut Config) {
    if config.language != "en" {
        config.language = "zh".to_string();
    }
    if !matches!(config.hero_name.as_str(), "full" | "short" | "both") {
        config.hero_name = "full".to_string();
    }
    if config.rules.stack_left_sec > config.rules.stack_right_sec {
        std::mem::swap(&mut config.rules.stack_left_sec, &mut config.rules.stack_right_sec);
    }
    if config.schema < 2 {
        config.rules.bounty_first_sec = 0;
        config.rules.bounty_interval_sec = 240;
        config.rules.water_first_sec = 120;
        config.rules.water_second_sec = 240;
        config.rules.power_first_sec = 360;
        config.rules.power_interval_sec = 120;
    }
    config.schema = CURRENT_SCHEMA;
}

fn stored_value(text: &str) -> Result<toml::Value, String> {
    toml::from_str(text).map_err(|error| format!("配置无法解析：{error}"))
}

fn render(config: &Config) -> Result<String, String> {
    toml::to_string_pretty(config).map_err(|error| format!("配置无法序列化：{error}"))
}

pub fn save(path: &Path, config: &Config) -> Result<(), String> {
    let body = render(config)?;
    let text = format!(
        "# dota2-tips 玩家配置。设置界面会写回这个文件。\n# language: zh 或 en。hero_name: full、short 或 both。\n# 7.42：野区赏金 0:00 起每 240 秒；中路圣水 120 与 240 秒，两侧各一；中路神符 360 秒起每 120 秒，随机一侧一枚。\n\n{body}"
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

    #[test]
    fn repair_removes_obsolete_fields_and_is_stable() {
        let dir = std::env::temp_dir().join("dota2-tips-config-repair");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        fs::write(
            &path,
            "schema = 2\nhero_name = \"short\"\n[bars.camp]\nenabled = true\nstyle = \"ring\"\n",
        )
        .unwrap();
        let (config, changed) = repair(&path).unwrap();
        assert!(changed);
        assert_eq!(config.schema, CURRENT_SCHEMA);
        assert_eq!(config.hero_name, "short");
        assert!(config.bars.water.enabled);
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains("style"));
        let (_, again) = repair(&path).unwrap();
        assert!(!again);
    }
}
