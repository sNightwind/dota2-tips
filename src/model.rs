use serde_json::Value;
use std::time::{Duration, Instant};

const STALE: Duration = Duration::from_secs(3);
/// 暂停时往往只剩心跳。配置里的心跳是 5 秒，短于这个值就会在进度条和「等待对局」之间来回切。
const PAUSED_STALE: Duration = Duration::from_secs(20);
const MAX_EXTRAPOLATE: f64 = 0.95;
/// 魔瓶会把冷却在真实剩余和 0 之间来回报。突变要持续这么久才改进度条。
const ITEM_GLITCH_HOLD: Duration = Duration::from_millis(450);

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
            self.sample + self.at.elapsed().as_secs_f64().min(MAX_EXTRAPOLATE)
        }
    }
}

#[derive(Debug, Clone)]
struct PendingReport {
    reported: f64,
    since: Instant,
}

#[derive(Debug, Clone)]
pub struct ItemCd {
    pub name: String,
    pub remaining: f64,
    pub max: f64,
    pub at: Instant,
    pending: Option<PendingReport>,
}

impl ItemCd {
    pub fn remaining_now(&self, paused: bool) -> f64 {
        if paused {
            self.remaining.max(0.0)
        } else {
            (self.remaining - self.at.elapsed().as_secs_f64().min(MAX_EXTRAPOLATE)).max(0.0)
        }
    }

    fn at(name: String, remaining: f64, max: f64, pending: Option<PendingReport>) -> Self {
        Self {
            name,
            remaining: remaining.max(0.0),
            max,
            at: Instant::now(),
            pending,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MissingHero {
    pub unit: String,
    pub gone_at: f64,
}

#[derive(Debug, Clone)]
pub struct Live {
    pub seen_payload: bool,
    pub last_live_packet: Option<Instant>,
    pub in_match: bool,
    pub clock: Option<Clock>,
    pub visible: Vec<String>,
    pub missing: Vec<MissingHero>,
    pub items: Vec<ItemCd>,
}

impl Default for Live {
    fn default() -> Self {
        Self {
            seen_payload: false,
            last_live_packet: None,
            in_match: false,
            clock: None,
            visible: Vec::new(),
            missing: Vec::new(),
            items: Vec::new(),
        }
    }
}

impl Live {
    pub fn showing_match(&self) -> bool {
        if !self.in_match {
            return false;
        }
        let limit = if self.clock.as_ref().is_some_and(|clock| clock.paused) {
            PAUSED_STALE
        } else {
            STALE
        };
        self.last_live_packet.is_some_and(|at| at.elapsed() < limit)
    }

    pub fn apply(&mut self, value: &Value) {
        self.seen_payload = true;
        let was_paused = self.clock.as_ref().is_some_and(|clock| clock.paused);
        if let Some(map) = value.get("map").filter(|map| map.is_object()) {
            if let Some(clock) = map.get("clock_time").and_then(Value::as_f64) {
                let paused = read_paused(map, was_paused);
                let previous = self.clock.as_ref().map(|clock| clock.game_now());
                let jumped_back = previous.is_some_and(|old| clock + 10.0 < old);
                if jumped_back {
                    self.visible.clear();
                    self.missing.clear();
                    self.items.clear();
                }
                self.note_clock(clock, paused);
            }
            if let Some(state) = map.get("game_state").and_then(Value::as_str) {
                let live_now = is_live_match(state);
                if live_now != self.in_match {
                    self.visible.clear();
                    self.missing.clear();
                    self.items.clear();
                }
                self.in_match = live_now;
                if live_now {
                    self.last_live_packet = Some(Instant::now());
                }
            }
        }
        if !self.in_match {
            self.visible.clear();
            self.missing.clear();
            self.items.clear();
            return;
        }
        if let Some(minimap) = value.get("minimap").filter(|value| value.is_object()) {
            let now = self
                .clock
                .as_ref()
                .map(|clock| clock.game_now())
                .unwrap_or(0.0);
            self.note_enemies(enemy_units(minimap), now);
        }
        if let Some(items) = value.get("items").filter(|value| value.is_object()) {
            let previous = std::mem::take(&mut self.items);
            self.items = item_cooldowns(items, &previous, was_paused);
        }
    }

    /// 同一秒内的 `clock_time` 不重锚。暂停时冻在当前时间，避免本地时钟跑过一截再被拉回。
    fn note_clock(&mut self, sample: f64, paused: bool) {
        let previous = self.clock.clone();
        if let Some(prev) = previous {
            if paused && !prev.paused {
                let frozen = prev.game_now();
                let sample = if (sample - frozen).abs() < 1.5 {
                    frozen
                } else {
                    sample
                };
                self.clock = Some(Clock {
                    sample,
                    at: Instant::now(),
                    paused: true,
                });
                return;
            }
            let delta = sample - prev.sample;
            if paused && prev.paused && delta.abs() < 0.5 {
                return;
            }
            if !paused && !prev.paused && (-0.5..0.5).contains(&delta) {
                return;
            }
        }
        self.clock = Some(Clock {
            sample,
            at: Instant::now(),
            paused,
        });
    }

    fn note_enemies(&mut self, current: Vec<String>, game_now: f64) {
        self.missing
            .retain(|hero| !current.iter().any(|unit| unit == &hero.unit));
        for unit in &self.visible {
            if current.iter().any(|name| name == unit) {
                continue;
            }
            if self.missing.iter().any(|hero| hero.unit == *unit) {
                continue;
            }
            self.missing.push(MissingHero {
                unit: unit.clone(),
                gone_at: game_now,
            });
        }
        self.visible = current;
    }
}

pub fn is_live_match(state: &str) -> bool {
    matches!(
        state,
        "DOTA_GAMERULES_STATE_PRE_GAME" | "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS"
    )
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StackWindow {
    pub remaining: f64,
    pub total: f64,
    pub inside: bool,
}

/// 拉野条挂在每轮野怪刷新上。第一波野怪出现后才显示，走到右边界后消失，下一轮开始再出现。
pub fn stack_window(
    clock: f64,
    first: f64,
    interval: f64,
    left: f64,
    right: f64,
) -> Option<StackWindow> {
    let interval = interval.max(1.0);
    if clock < first {
        return None;
    }
    let left = left.max(0.0).min(interval);
    let right = right.max(left).min(interval);
    let offset = (clock - first) % interval;
    if offset >= right {
        return None;
    }
    Some(StackWindow {
        remaining: right - offset,
        total: right.max(1.0),
        inside: offset >= left,
    })
}

pub fn water_remaining(clock: f64, first: f64, second: f64) -> Option<(f64, f64)> {
    if clock < first {
        return Some(((first - clock).max(0.0), first.max(1.0)));
    }
    if clock < second {
        return Some(((second - clock).max(0.0), (second - first).max(1.0)));
    }
    None
}

pub fn cycle_remaining(clock: f64, first: f64, interval: f64) -> (f64, f64) {
    let interval = interval.max(1.0);
    if clock < first {
        return ((first - clock).max(0.0), first.max(interval));
    }
    let into = (clock - first) % interval;
    let remaining = if into < 0.05 {
        interval
    } else {
        interval - into
    };
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

fn read_paused(map: &Value, was_paused: bool) -> bool {
    match map.get("paused") {
        Some(value) => match value.as_bool() {
            Some(flag) => flag,
            None => {
                value.as_i64().is_some_and(|number| number != 0)
                    || value.as_f64().is_some_and(|number| number != 0.0)
            }
        },
        None => was_paused,
    }
}

/// 冷却样本和本地倒计时接近时沿用旧锚点，避免进度条来回跳。
fn item_sample_is_repeat(predicted: f64, reported: f64) -> bool {
    if reported <= 0.05 {
        return false;
    }
    let delta = reported - predicted;
    delta < 1.25 && delta > -0.35
}

fn item_cooldowns(items: &Value, previous: &[ItemCd], was_paused: bool) -> Vec<ItemCd> {
    let Some(map) = items.as_object() else {
        return Vec::new();
    };
    let mut keys: Vec<&String> = map
        .keys()
        .filter(|key| {
            key.starts_with("slot") || key.starts_with("teleport") || key.starts_with("neutral")
        })
        .collect();
    keys.sort_by_key(|key| (slot_kind(key), slot_index(key)));
    let mut used = vec![false; previous.len()];
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
        let reported_max = item
            .get("max_cooldown")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let previous_index = previous
            .iter()
            .enumerate()
            .position(|(index, prev)| !used[index] && prev.name == name);
        if remaining <= 0.0 && reported_max <= 0.0 {
            let Some(index) = previous_index else {
                continue;
            };
            if previous[index].remaining_now(was_paused) <= 0.5 {
                continue;
            }
            used[index] = true;
            out.push(follow_item(
                &previous[index],
                0.0,
                previous[index].max,
                was_paused,
            ));
            continue;
        }
        let max = if reported_max.max(remaining) >= 0.1 {
            reported_max.max(remaining)
        } else if let Some(index) = previous_index {
            previous[index].max
        } else {
            0.1
        };
        if let Some(index) = previous_index {
            used[index] = true;
            out.push(follow_item(&previous[index], remaining, max, was_paused));
            continue;
        }
        out.push(ItemCd::at(name.to_string(), remaining, max, None));
    }
    out
}

/// 和本地倒计时一致就沿用；从就绪进入冷却立刻采纳。魔瓶那种一跳而过的 0 先按住。
fn follow_item(prev: &ItemCd, reported: f64, max: f64, was_paused: bool) -> ItemCd {
    let predicted = prev.remaining_now(was_paused);
    let name = prev.name.clone();
    if reported > 0.4 && predicted <= 0.4 {
        return ItemCd::at(name, reported, max, None);
    }
    if reported <= 0.05 && predicted <= 0.5 {
        return ItemCd::at(name, 0.0, max, None);
    }
    if item_sample_is_repeat(predicted, reported) {
        return ItemCd::at(name, predicted, max, None);
    }
    let pending = match &prev.pending {
        Some(pending) if reports_match(pending.reported, reported) => pending.clone(),
        _ => PendingReport {
            reported,
            since: Instant::now(),
        },
    };
    if pending.since.elapsed() >= ITEM_GLITCH_HOLD {
        return ItemCd::at(name, reported, max, None);
    }
    ItemCd::at(name, predicted, max.max(prev.max), Some(pending))
}

fn reports_match(left: f64, right: f64) -> bool {
    (left <= 0.05 && right <= 0.05) || (left - right).abs() <= 0.35
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
            "map": {"clock_time": 30, "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS", "paused": false},
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
            live.visible,
            vec![
                "npc_dota_hero_zuus".to_string(),
                "npc_dota_hero_axe".to_string()
            ]
        );
        assert_eq!(live.items.len(), 2);
        assert_eq!(live.items[0].name, "item_mjollnir");
        assert_eq!(live.items[1].name, "item_tpscroll");
    }

    #[test]
    fn missing_minimap_does_not_clear_enemies() {
        let mut live = Live::default();
        live.apply(&json!({
            "map": {"clock_time": 10, "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS", "paused": false},
            "minimap": {"o1": {"image": "minimap_enemyicon", "unitname": "npc_dota_hero_axe"}}
        }));
        live.apply(&json!({
            "map": {"clock_time": 12, "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS", "paused": true}
        }));
        assert_eq!(live.visible, vec!["npc_dota_hero_axe".to_string()]);
        assert!(live.clock.unwrap().paused);
    }

    #[test]
    fn leaving_the_match_stops_the_clock_state() {
        let mut live = Live::default();
        live.apply(&json!({
            "map": {"clock_time": 100, "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS", "paused": false}
        }));
        assert!(live.showing_match());
        live.apply(&json!({
            "map": {"clock_time": 110, "game_state": "DOTA_GAMERULES_STATE_POST_GAME", "paused": false}
        }));
        assert!(!live.showing_match());
        assert!(live.visible.is_empty());
    }

    #[test]
    fn missing_heroes_keep_the_clock_when_they_leave() {
        let mut live = Live::default();
        live.apply(&json!({
            "map": {"clock_time": 80, "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS"},
            "minimap": {"o1": {"image": "minimap_enemyicon", "unitname": "npc_dota_hero_axe"}}
        }));
        live.apply(&json!({
            "map": {"clock_time": 95, "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS"},
            "minimap": {"o1": {"image": "minimap_creep", "unitname": "npc_dota_creep_badguys_melee"}}
        }));
        assert!(live.visible.is_empty());
        assert_eq!(live.missing.len(), 1);
        assert!((live.missing[0].gone_at - 95.0).abs() < 0.001);
        live.apply(&json!({
            "map": {"clock_time": 100, "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS"},
            "minimap": {"o1": {"image": "minimap_enemyicon", "unitname": "npc_dota_hero_axe"}}
        }));
        assert!(live.missing.is_empty());
        assert_eq!(live.visible, vec!["npc_dota_hero_axe".to_string()]);
    }

    #[test]
    fn lotus_and_wisdom_follow_their_schedules() {
        let (remaining, _) = cycle_remaining(60.0, 180.0, 180.0);
        assert!((remaining - 120.0).abs() < 0.001);
        let (remaining, _) = cycle_remaining(180.0, 180.0, 180.0);
        assert!((remaining - 180.0).abs() < 0.001);
        let (remaining, _) = cycle_remaining(300.0, 420.0, 420.0);
        assert!((remaining - 120.0).abs() < 0.001);
        let (remaining, _) = cycle_remaining(500.0, 420.0, 420.0);
        assert!((remaining - 340.0).abs() < 0.001);
    }

    #[test]
    fn water_runes_cover_only_two_and_four_minutes() {
        let (remaining, _) = water_remaining(30.0, 120.0, 240.0).unwrap();
        assert!((remaining - 90.0).abs() < 0.001);
        let (remaining, _) = water_remaining(150.0, 120.0, 240.0).unwrap();
        assert!((remaining - 90.0).abs() < 0.001);
        assert!(water_remaining(240.0, 120.0, 240.0).is_none());
    }

    #[test]
    fn heartbeat_without_a_live_state_does_not_keep_the_match() {
        let mut live = Live::default();
        live.apply(&json!({
            "map": {"clock_time": 100, "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS"}
        }));
        let marked = live.last_live_packet;
        live.apply(&json!({"provider": {"name": "Dota 2"}}));
        assert_eq!(live.last_live_packet, marked);
        live.last_live_packet = Some(Instant::now() - Duration::from_secs(5));
        assert!(!live.showing_match());
    }

    #[test]
    fn stack_window_follows_each_camp_cycle() {
        assert!(stack_window(30.0, 60.0, 60.0, 53.0, 55.0).is_none());
        let early = stack_window(70.0, 60.0, 60.0, 53.0, 55.0).unwrap();
        assert!(!early.inside);
        assert!((early.remaining - 45.0).abs() < 0.001);
        let pulling = stack_window(114.0, 60.0, 60.0, 53.0, 55.0).unwrap();
        assert!(pulling.inside);
        assert!(stack_window(116.0, 60.0, 60.0, 53.0, 55.0).is_none());
        assert!(stack_window(120.0, 60.0, 60.0, 53.0, 55.0).is_some());
    }

    #[test]
    fn repeated_clock_does_not_rewind() {
        let mut live = Live::default();
        live.apply(&json!({
            "map": {"clock_time": 100, "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS", "paused": false}
        }));
        std::thread::sleep(Duration::from_millis(80));
        let before = live.clock.as_ref().unwrap().game_now();
        live.apply(&json!({
            "map": {"clock_time": 100, "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS", "paused": false}
        }));
        let after = live.clock.as_ref().unwrap().game_now();
        assert!(before > 100.04, "{before}");
        assert!(after + 0.02 >= before, "rewound from {before} to {after}");
    }

    #[test]
    fn clock_jump_resyncs() {
        let mut live = Live::default();
        live.apply(&json!({
            "map": {"clock_time": 100, "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS", "paused": false}
        }));
        live.apply(&json!({
            "map": {"clock_time": 140, "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS", "paused": false}
        }));
        let now = live.clock.as_ref().unwrap().game_now();
        assert!((now - 140.0).abs() < 0.05, "{now}");
    }

    #[test]
    fn repeated_item_cooldown_does_not_jump_up() {
        assert!(!item_sample_is_repeat(0.2, 0.0));
        assert!(item_sample_is_repeat(11.7, 12.0));
        assert!(!item_sample_is_repeat(1.0, 15.0));

        let mut live = Live::default();
        let payload = json!({
            "map": {"clock_time": 30, "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS", "paused": false},
            "items": {"slot0": {"name": "item_blink", "cooldown": 12, "max_cooldown": 15}}
        });
        live.apply(&payload);
        std::thread::sleep(Duration::from_millis(80));
        let before = live.items[0].remaining_now(false);
        live.apply(&payload);
        let after = live.items[0].remaining_now(false);
        assert!(before < 11.96, "{before}");
        assert!(after <= before + 0.02, "jumped from {before} to {after}");
    }

    #[test]
    fn unpause_does_not_skip_item_cooldown() {
        let mut live = Live::default();
        live.apply(&json!({
            "map": {"clock_time": 10, "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS", "paused": false},
            "items": {"slot0": {"name": "item_blink", "cooldown": 8, "max_cooldown": 15}}
        }));
        live.apply(&json!({
            "map": {"clock_time": 10, "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS", "paused": true},
            "items": {"slot0": {"name": "item_blink", "cooldown": 8, "max_cooldown": 15}}
        }));
        std::thread::sleep(Duration::from_millis(200));
        live.apply(&json!({
            "map": {"clock_time": 10, "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS", "paused": false},
            "items": {"slot0": {"name": "item_blink", "cooldown": 8, "max_cooldown": 15}}
        }));
        let left = live.items[0].remaining_now(false);
        assert!((left - 8.0).abs() < 0.08, "{left}");
    }

    #[test]
    fn paused_clock_stays_put_and_the_match_stays_visible() {
        let mut live = Live::default();
        live.apply(&json!({
            "map": {"clock_time": 100, "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS", "paused": false}
        }));
        std::thread::sleep(Duration::from_millis(40));
        live.apply(&json!({
            "map": {"clock_time": 100, "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS", "paused": true}
        }));
        let frozen = live.clock.as_ref().unwrap().game_now();
        std::thread::sleep(Duration::from_millis(150));
        live.apply(&json!({
            "map": {"clock_time": 100, "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS", "paused": true}
        }));
        let again = live.clock.as_ref().unwrap().game_now();
        assert!((again - frozen).abs() < 0.001, "{frozen} -> {again}");
        live.last_live_packet = Some(Instant::now() - Duration::from_secs(8));
        assert!(live.showing_match());
        live.last_live_packet = Some(Instant::now() - Duration::from_secs(21));
        assert!(!live.showing_match());
    }

    #[test]
    fn missing_pause_flag_keeps_the_frozen_clock() {
        let mut live = Live::default();
        live.apply(&json!({
            "map": {"clock_time": 40, "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS", "paused": 1}
        }));
        assert!(live.clock.as_ref().unwrap().paused);
        let frozen = live.clock.as_ref().unwrap().game_now();
        std::thread::sleep(Duration::from_millis(80));
        live.apply(&json!({
            "map": {"clock_time": 40, "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS"}
        }));
        assert!(live.clock.as_ref().unwrap().paused);
        let again = live.clock.as_ref().unwrap().game_now();
        assert!((again - frozen).abs() < 0.001, "{frozen} -> {again}");
    }

    #[test]
    fn bottle_zero_flicker_does_not_jump_the_bar() {
        let mut live = Live::default();
        let charged = |cooldown: f64| {
            json!({
                "map": {"clock_time": 30, "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS", "paused": false},
                "items": {"slot0": {"name": "item_bottle", "cooldown": cooldown, "max_cooldown": 15, "charges": 2}}
            })
        };
        live.apply(&charged(12.0));
        live.apply(&charged(0.0));
        let left = live.items[0].remaining_now(false);
        assert!(left > 11.0, "{left}");
        live.apply(&charged(11.8));
        live.apply(&charged(0.0));
        let left = live.items[0].remaining_now(false);
        assert!(left > 10.5, "{left}");
    }

    #[test]
    fn drinking_from_a_ready_bottle_starts_the_cooldown() {
        let mut live = Live::default();
        live.apply(&json!({
            "map": {"clock_time": 30, "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS", "paused": false},
            "items": {"slot0": {"name": "item_bottle", "cooldown": 0, "max_cooldown": 15, "charges": 3}}
        }));
        live.apply(&json!({
            "map": {"clock_time": 30, "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS", "paused": false},
            "items": {"slot0": {"name": "item_bottle", "cooldown": 15, "max_cooldown": 15, "charges": 2}}
        }));
        let left = live.items[0].remaining_now(false);
        assert!((left - 15.0).abs() < 0.05, "{left}");
    }

    #[test]
    fn a_cooldown_that_stays_zero_becomes_ready() {
        let mut live = Live::default();
        let packet = |cooldown: f64| {
            json!({
                "map": {"clock_time": 30, "game_state": "DOTA_GAMERULES_STATE_GAME_IN_PROGRESS", "paused": false},
                "items": {"slot0": {"name": "item_bottle", "cooldown": cooldown, "max_cooldown": 15}}
            })
        };
        live.apply(&packet(5.0));
        live.apply(&packet(0.0));
        assert!(live.items[0].remaining_now(false) > 4.0);
        std::thread::sleep(ITEM_GLITCH_HOLD + Duration::from_millis(40));
        live.apply(&packet(0.0));
        assert!(live.items[0].remaining_now(false) <= 0.05);
    }
}
