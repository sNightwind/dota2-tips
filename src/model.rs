use serde_json::Value;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct Clock {
    pub sample: f64,
    pub at: Instant,
    pub paused: bool,
}

impl Clock {
    pub fn game_now(&self) -> f64 {
        if self.paused {
            self.sample
        } else {
            self.sample + self.at.elapsed().as_secs_f64()
        }
    }
}

#[derive(Debug, Clone)]
pub struct ItemCd {
    pub name: String,
    pub remaining: f64,
    pub max: f64,
    pub at: Instant,
}

impl ItemCd {
    pub fn remaining_now(&self, paused: bool) -> f64 {
        if paused {
            self.remaining.max(0.0)
        } else {
            (self.remaining - self.at.elapsed().as_secs_f64()).max(0.0)
        }
    }
}

#[derive(Debug, Clone)]
pub struct Live {
    pub seen_payload: bool,
    pub clock: Option<Clock>,
    pub enemies: Vec<String>,
    pub items: Vec<ItemCd>,
}

impl Default for Live {
    fn default() -> Self {
        Self {
            seen_payload: false,
            clock: None,
            enemies: Vec::new(),
            items: Vec::new(),
        }
    }
}

impl Live {
    pub fn apply(&mut self, value: &Value) {
        self.seen_payload = true;
        if let Some(map) = value.get("map").filter(|map| map.is_object()) {
            if let Some(clock) = map.get("clock_time").and_then(Value::as_f64) {
                let paused = map.get("paused").and_then(Value::as_bool).unwrap_or(false);
                let previous = self.clock.as_ref().map(|clock| clock.sample);
                let jumped_back = previous.is_some_and(|old| clock + 10.0 < old);
                if jumped_back {
                    self.enemies.clear();
                    self.items.clear();
                }
                self.clock = Some(Clock {
                    sample: clock,
                    at: Instant::now(),
                    paused,
                });
            }
        }
        if let Some(minimap) = value.get("minimap").filter(|value| value.is_object()) {
            self.enemies = enemy_units(minimap);
        }
        if let Some(items) = value.get("items").filter(|value| value.is_object()) {
            self.items = item_cooldowns(items);
        }
    }
}

pub fn cycle_remaining(clock: f64, first: f64, interval: f64) -> (f64, f64) {
    let interval = interval.max(1.0);
    if clock < first {
        return ((first - clock).max(0.0), first.max(interval));
    }
    let into = (clock - first) % interval;
    let remaining = if into < 0.05 { interval } else { interval - into };
    (remaining, interval)
}

pub fn ratio(remaining: f64, total: f64) -> f32 {
    if total <= 0.0 {
        return 0.0;
    }
    (remaining / total).clamp(0.0, 1.0) as f32
}

fn enemy_units(minimap: &Value) -> Vec<String> {
    let Some(map) = minimap.as_object() else {
        return Vec::new();
    };
    let mut names = Vec::new();
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort_by_key(|key| slot_index(key));
    for key in keys {
        let Some(unit) = map[key].as_object() else {
            continue;
        };
        if unit.get("image").and_then(Value::as_str) != Some("minimap_enemyicon") {
            continue;
        }
        let Some(name) = unit.get("unitname").and_then(Value::as_str) else {
            continue;
        };
        if name.is_empty() || names.iter().any(|have| have == name) {
            continue;
        }
        names.push(name.to_string());
    }
    names
}

fn slot_index(key: &str) -> u32 {
    key.trim_start_matches(|ch: char| !ch.is_ascii_digit())
        .parse()
        .unwrap_or(0)
}

fn item_cooldowns(items: &Value) -> Vec<ItemCd> {
    let Some(map) = items.as_object() else {
        return Vec::new();
    };
    let mut keys: Vec<&String> = map
        .keys()
        .filter(|key| key.starts_with("slot") || key.starts_with("teleport") || key.starts_with("neutral"))
        .collect();
    keys.sort_by_key(|key| (slot_kind(key), slot_index(key)));
    let mut out = Vec::new();
    for key in keys {
        let Some(item) = map[key].as_object() else {
            continue;
        };
        let Some(name) = item.get("name").and_then(Value::as_str) else {
            continue;
        };
        if name.is_empty() || name == "empty" {
            continue;
        }
        let remaining = item.get("cooldown").and_then(Value::as_f64).unwrap_or(0.0);
        let max = item.get("max_cooldown").and_then(Value::as_f64).unwrap_or(0.0);
        if remaining <= 0.0 && max <= 0.0 {
            continue;
        }
        out.push(ItemCd {
            name: name.to_string(),
            remaining,
            max: max.max(remaining).max(0.1),
            at: Instant::now(),
        });
    }
    out
}

fn slot_kind(key: &str) -> u8 {
    if key.starts_with("slot") {
        0
    } else if key.starts_with("teleport") {
        1
    } else {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cycle_counts_down_to_the_next_attempt() {
        let (remaining, total) = cycle_remaining(90.0, 60.0, 60.0);
        assert!((remaining - 30.0).abs() < 0.001);
        assert!((total - 60.0).abs() < 0.001);

        let (remaining, _) = cycle_remaining(60.0, 60.0, 60.0);
        assert!((remaining - 60.0).abs() < 0.001);

        let (remaining, _) = cycle_remaining(-30.0, 60.0, 60.0);
        assert!((remaining - 90.0).abs() < 0.001);

        let (remaining, _) = cycle_remaining(100.0, 0.0, 180.0);
        assert!((remaining - 80.0).abs() < 0.001);
    }

    #[test]
    fn enemies_keep_only_enemy_icons() {
        let value = json!({
            "minimap": {
                "o1": {"image": "minimap_plaincircle", "unitname": "", "team": 0},
                "o2": {"image": "minimap_herocircle_self", "unitname": "npc_dota_hero_storm_spirit"},
                "o4": {"image": "minimap_heroimage", "unitname": "npc_dota_hero_juggernaut"},
                "o3": {"image": "minimap_enemyicon", "unitname": "npc_dota_hero_zuus"},
                "o5": {"image": "minimap_enemyicon", "unitname": "npc_dota_hero_zuus"},
                "o6": {"image": "minimap_enemyicon", "unitname": "npc_dota_hero_axe"}
            },
            "items": {
                "slot0": {"name": "empty"},
                "slot1": {"name": "item_mjollnir", "cooldown": 16, "max_cooldown": 36},
                "slot2": {"name": "item_kaya_and_sange"},
                "teleport0": {"name": "item_tpscroll", "cooldown": 0, "max_cooldown": 81}
            }
        });
        let mut live = Live::default();
        live.apply(&value);
        assert_eq!(
            live.enemies,
            vec!["npc_dota_hero_zuus".to_string(), "npc_dota_hero_axe".to_string()]
        );
        assert_eq!(live.items.len(), 2);
        assert_eq!(live.items[0].name, "item_mjollnir");
        assert_eq!(live.items[1].name, "item_tpscroll");
    }

    #[test]
    fn missing_minimap_does_not_clear_enemies() {
        let mut live = Live::default();
        live.apply(&json!({"minimap": {"o1": {"image": "minimap_enemyicon", "unitname": "npc_dota_hero_axe"}}}));
        live.apply(&json!({"map": {"clock_time": 10, "paused": true}}));
        assert_eq!(live.enemies, vec!["npc_dota_hero_axe".to_string()]);
        assert!(live.clock.unwrap().paused);
    }
}
