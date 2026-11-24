struct Hero {
    id: &'static str,
    en: &'static str,
    zh: &'static str,
}

const HEROES: &[Hero] = &[
    Hero { id: "abaddon", en: "Abaddon", zh: "亚巴顿" },
    Hero { id: "abyssal_underlord", en: "Underlord", zh: "孽主" },
    Hero { id: "alchemist", en: "Alchemist", zh: "炼金术士" },
    Hero { id: "ancient_apparition", en: "Ancient Apparition", zh: "远古冰魄" },
    Hero { id: "antimage", en: "Anti-Mage", zh: "敌法师" },
    Hero { id: "arc_warden", en: "Arc Warden", zh: "天穹守望者" },
    Hero { id: "axe", en: "Axe", zh: "斧王" },
    Hero { id: "bane", en: "Bane", zh: "祸乱之源" },
    Hero { id: "batrider", en: "Batrider", zh: "蝙蝠骑士" },
    Hero { id: "beastmaster", en: "Beastmaster", zh: "兽王" },
    Hero { id: "bloodseeker", en: "Bloodseeker", zh: "血魔" },
    Hero { id: "bounty_hunter", en: "Bounty Hunter", zh: "赏金猎人" },
    Hero { id: "brewmaster", en: "Brewmaster", zh: "酒仙" },
    Hero { id: "bristleback", en: "Bristleback", zh: "钢背兽" },
    Hero { id: "broodmother", en: "Broodmother", zh: "育母蜘蛛" },
    Hero { id: "centaur", en: "Centaur Warrunner", zh: "半人马战行者" },
    Hero { id: "chaos_knight", en: "Chaos Knight", zh: "混沌骑士" },
    Hero { id: "chen", en: "Chen", zh: "陈" },
    Hero { id: "clinkz", en: "Clinkz", zh: "克林克兹" },
    Hero { id: "crystal_maiden", en: "Crystal Maiden", zh: "水晶室女" },
    Hero { id: "dark_seer", en: "Dark Seer", zh: "黑暗贤者" },
    Hero { id: "dark_willow", en: "Dark Willow", zh: "邪影芳灵" },
    Hero { id: "dawnbreaker", en: "Dawnbreaker", zh: "破晓辰星" },
    Hero { id: "dazzle", en: "Dazzle", zh: "戴泽" },
    Hero { id: "death_prophet", en: "Death Prophet", zh: "死亡先知" },
    Hero { id: "disruptor", en: "Disruptor", zh: "干扰者" },
    Hero { id: "doom_bringer", en: "Doom", zh: "末日使者" },
    Hero { id: "dragon_knight", en: "Dragon Knight", zh: "龙骑士" },
    Hero { id: "drow_ranger", en: "Drow Ranger", zh: "卓尔游侠" },
    Hero { id: "earth_spirit", en: "Earth Spirit", zh: "大地之灵" },
    Hero { id: "earthshaker", en: "Earthshaker", zh: "撼地者" },
    Hero { id: "elder_titan", en: "Elder Titan", zh: "上古巨神" },
    Hero { id: "ember_spirit", en: "Ember Spirit", zh: "灰烬之灵" },
    Hero { id: "enchantress", en: "Enchantress", zh: "魅惑魔女" },
    Hero { id: "enigma", en: "Enigma", zh: "谜团" },
    Hero { id: "faceless_void", en: "Faceless Void", zh: "虚空假面" },
    Hero { id: "furion", en: "Nature's Prophet", zh: "先知" },
    Hero { id: "grimstroke", en: "Grimstroke", zh: "天涯墨客" },
    Hero { id: "gyrocopter", en: "Gyrocopter", zh: "矮人直升机" },
    Hero { id: "hoodwink", en: "Hoodwink", zh: "森海飞霞" },
    Hero { id: "huskar", en: "Huskar", zh: "哈斯卡" },
    Hero { id: "invoker", en: "Invoker", zh: "祈求者" },
    Hero { id: "jakiro", en: "Jakiro", zh: "杰奇洛" },
    Hero { id: "juggernaut", en: "Juggernaut", zh: "主宰" },
    Hero { id: "keeper_of_the_light", en: "Keeper of the Light", zh: "光之守卫" },
    Hero { id: "kez", en: "Kez", zh: "凯" },
    Hero { id: "kunkka", en: "Kunkka", zh: "昆卡" },
    Hero { id: "legion_commander", en: "Legion Commander", zh: "军团指挥官" },
    Hero { id: "leshrac", en: "Leshrac", zh: "拉席克" },
    Hero { id: "lich", en: "Lich", zh: "巫妖" },
    Hero { id: "life_stealer", en: "Lifestealer", zh: "噬魂鬼" },
    Hero { id: "lina", en: "Lina", zh: "莉娜" },
    Hero { id: "lion", en: "Lion", zh: "莱恩" },
    Hero { id: "lone_druid", en: "Lone Druid", zh: "德鲁伊" },
    Hero { id: "luna", en: "Luna", zh: "露娜" },
    Hero { id: "lycan", en: "Lycan", zh: "狼人" },
    Hero { id: "magnataur", en: "Magnus", zh: "马格纳斯" },
    Hero { id: "marci", en: "Marci", zh: "玛西" },
    Hero { id: "mars", en: "Mars", zh: "玛尔斯" },
    Hero { id: "medusa", en: "Medusa", zh: "美杜莎" },
    Hero { id: "meepo", en: "Meepo", zh: "米波" },
    Hero { id: "mirana", en: "Mirana", zh: "米拉娜" },
    Hero { id: "monkey_king", en: "Monkey King", zh: "齐天大圣" },
    Hero { id: "morphling", en: "Morphling", zh: "变体精灵" },
    Hero { id: "muerta", en: "Muerta", zh: "琼英碧灵" },
    Hero { id: "naga_siren", en: "Naga Siren", zh: "娜迦海妖" },
    Hero { id: "necrolyte", en: "Necrophos", zh: "瘟疫法师" },
    Hero { id: "nevermore", en: "Shadow Fiend", zh: "影魔" },
    Hero { id: "night_stalker", en: "Night Stalker", zh: "暗夜魔王" },
    Hero { id: "nyx_assassin", en: "Nyx Assassin", zh: "司夜刺客" },
    Hero { id: "obsidian_destroyer", en: "Outworld Destroyer", zh: "殁境神蚀者" },
    Hero { id: "ogre_magi", en: "Ogre Magi", zh: "食人魔魔法师" },
    Hero { id: "omniknight", en: "Omniknight", zh: "全能骑士" },
    Hero { id: "oracle", en: "Oracle", zh: "神谕者" },
    Hero { id: "pangolier", en: "Pangolier", zh: "石鳞剑士" },
    Hero { id: "phantom_assassin", en: "Phantom Assassin", zh: "幻影刺客" },
    Hero { id: "phantom_lancer", en: "Phantom Lancer", zh: "幻影长矛手" },
    Hero { id: "phoenix", en: "Phoenix", zh: "凤凰" },
    Hero { id: "primal_beast", en: "Primal Beast", zh: "兽" },
    Hero { id: "puck", en: "Puck", zh: "帕克" },
    Hero { id: "pudge", en: "Pudge", zh: "帕吉" },
    Hero { id: "pugna", en: "Pugna", zh: "帕格纳" },
    Hero { id: "queenofpain", en: "Queen of Pain", zh: "痛苦女王" },
    Hero { id: "rattletrap", en: "Clockwerk", zh: "发条技师" },
    Hero { id: "razor", en: "Razor", zh: "剃刀" },
    Hero { id: "riki", en: "Riki", zh: "力丸" },
    Hero { id: "ringmaster", en: "Ringmaster", zh: "百戏大王" },
    Hero { id: "rubick", en: "Rubick", zh: "拉比克" },
    Hero { id: "sand_king", en: "Sand King", zh: "沙王" },
    Hero { id: "shadow_demon", en: "Shadow Demon", zh: "暗影恶魔" },
    Hero { id: "shadow_shaman", en: "Shadow Shaman", zh: "暗影萨满" },
    Hero { id: "shredder", en: "Timbersaw", zh: "伐木机" },
    Hero { id: "silencer", en: "Silencer", zh: "沉默术士" },
    Hero { id: "skeleton_king", en: "Wraith King", zh: "冥魂大帝" },
    Hero { id: "skywrath_mage", en: "Skywrath Mage", zh: "天怒法师" },
    Hero { id: "slardar", en: "Slardar", zh: "斯拉达" },
    Hero { id: "slark", en: "Slark", zh: "斯拉克" },
    Hero { id: "snapfire", en: "Snapfire", zh: "电炎绝手" },
    Hero { id: "sniper", en: "Sniper", zh: "狙击手" },
    Hero { id: "spectre", en: "Spectre", zh: "幽鬼" },
    Hero { id: "spirit_breaker", en: "Spirit Breaker", zh: "裂魂人" },
    Hero { id: "storm_spirit", en: "Storm Spirit", zh: "风暴之灵" },
    Hero { id: "sven", en: "Sven", zh: "斯温" },
    Hero { id: "techies", en: "Techies", zh: "工程师" },
    Hero { id: "templar_assassin", en: "Templar Assassin", zh: "圣堂刺客" },
    Hero { id: "terrorblade", en: "Terrorblade", zh: "恐怖利刃" },
    Hero { id: "tidehunter", en: "Tidehunter", zh: "潮汐猎人" },
    Hero { id: "tinker", en: "Tinker", zh: "修补匠" },
    Hero { id: "tiny", en: "Tiny", zh: "小小" },
    Hero { id: "treant", en: "Treant Protector", zh: "树精卫士" },
    Hero { id: "troll_warlord", en: "Troll Warlord", zh: "巨魔战将" },
    Hero { id: "tusk", en: "Tusk", zh: "巨牙海民" },
    Hero { id: "undying", en: "Undying", zh: "不朽尸王" },
    Hero { id: "ursa", en: "Ursa", zh: "熊战士" },
    Hero { id: "vengefulspirit", en: "Vengeful Spirit", zh: "复仇之魂" },
    Hero { id: "venomancer", en: "Venomancer", zh: "剧毒术士" },
    Hero { id: "viper", en: "Viper", zh: "冥界亚龙" },
    Hero { id: "visage", en: "Visage", zh: "维萨吉" },
    Hero { id: "void_spirit", en: "Void Spirit", zh: "虚无之灵" },
    Hero { id: "warlock", en: "Warlock", zh: "术士" },
    Hero { id: "weaver", en: "Weaver", zh: "编织者" },
    Hero { id: "windrunner", en: "Windranger", zh: "风行者" },
    Hero { id: "winter_wyvern", en: "Winter Wyvern", zh: "寒冬飞龙" },
    Hero { id: "wisp", en: "Io", zh: "艾欧" },
    Hero { id: "witch_doctor", en: "Witch Doctor", zh: "巫医" },
    Hero { id: "zuus", en: "Zeus", zh: "宙斯" },
];

pub fn display_hero(unit: &str, language: &str, hero_name: &str) -> String {
    let id = unit.strip_prefix("npc_dota_hero_").unwrap_or(unit);
    if let Some(hero) = HEROES.iter().find(|hero| hero.id == id) {
        if hero_name == "short" {
            return initials(hero.en);
        }
        return if language == "en" {
            hero.en.to_string()
        } else {
            hero.zh.to_string()
        };
    }
    let pretty = prettify(id);
    if hero_name == "short" {
        initials(&pretty)
    } else {
        pretty
    }
}

pub fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|word| word.chars().find(|ch| ch.is_ascii_alphabetic()))
        .map(|ch| ch.to_ascii_uppercase())
        .collect()
}

fn prettify(id: &str) -> String {
    id.split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn display_item(name: &str) -> String {
    let bare = name.strip_prefix("item_").unwrap_or(name);
    prettify(bare)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_names_use_english_initials() {
        assert_eq!(initials("Phantom Assassin"), "PA");
        assert_eq!(initials("Keeper of the Light"), "KOTL");
        assert_eq!(initials("Queen of Pain"), "QOP");
        assert_eq!(
            display_hero("npc_dota_hero_phantom_assassin", "zh", "short"),
            "PA"
        );
        assert_eq!(
            display_hero("npc_dota_hero_phantom_assassin", "zh", "full"),
            "幻影刺客"
        );
        assert_eq!(
            display_hero("npc_dota_hero_phantom_assassin", "en", "full"),
            "Phantom Assassin"
        );
    }
}
