# dota2-tips 进度与计划

本地 Dota 2 提示工具。数据只来自官方 Game State Integration。不读取游戏内存，不注入进程。目标是一个不依赖外部运行时的 Windows 程序，用进度条显示规则计时和本机已经拿到的状态。

## 技术决定

- 语言用 Rust。当前采集器使用 `tiny_http`，只监听 `127.0.0.1`。
- 之后的界面用 egui / eframe：透明、无边框、置顶，并可切换鼠标穿透。
- 进度条只保存标题、剩余秒数和总时长。矩形或圆环由配置决定，绘制层不改 GSI 解析。
- 野怪和神符的时间规则放在 `rules.toml`，版本差异不写进界面代码。
- 默认监听 `127.0.0.1:5394`，token 为 `dota2-tips`。配置文件是 `gamestate_integration_dota2_tips.cfg`。

## 当前进度

采集器已经可用：

- 接收 Dota 的 POST，校验 token，拒绝错误 JSON。
- 记录字段路径、类型、示例和出现次数，写入 `captures/fields.txt`。
- 按 `game_state` 保存叶子字段最多的一份样本。
- Ctrl+C 会解开监听并正常退出。
- `cargo run -- --self-test` 可以自检监听、token 和落盘。

界面、规则计时和打包都还没开始。

## 试玩训练场已经确认的数据

最近一次采集收到 646 份载荷。配置里的 `minimap` 有真实单位；`roshan` 和 `couriers` 两个块一直是空对象。

| 需求 | 结论 | 依据 |
| --- | --- | --- |
| 敌方英雄是否出现在小地图，并显示名称 | 可以做 | `minimap` 单位带 `unitname`、`image`、`team`、`xpos`、`ypos`。自己是 `minimap_herocircle_self`，敌方是 `minimap_enemyicon`。空位是 `minimap_plaincircle`，坐标为 0。 |
| 野怪刷新倒计时 | 只能做“下一次整分钟刷新尝试” | 用 `map.clock_time` 和 `map.paused` 本地推算。不知道野区格子是否已经被占。 |
| 赏金符、神符刷新倒计时 | 可以按规则计时 | 同样用比赛时钟。不知道符是否还在地上。 |
| 肉山存活和刷新倒计时 | 只确认了存活时的小地图单位和死亡事件 | 单位名 `npc_dota_roshan`。死亡事件是 `roshan_killed`。没有 `roshan_state`，也没有 `roshan_state_end_seconds`。 |
| 道具冷却 | 本地物品可以做 | 跳刀已出现 `cooldown`、`max_cooldown`、`can_cast`。空槽只有 `name`。 |
| 敌方关键技能冷却 | 当前载荷没有 | 技能块只有自己的 `ability0` 到 `ability5`，没有敌方释放技能的事件。 |

另外：控制状态只有 `stunned` 这类布尔值，没有剩余秒数。信使块是空的，信使只作为小地图单位出现，图标是 `minimap_courier_flying`。英雄从 `minimap` 消失时没有单独的进出视野事件。

## 还要开发

1. 规则进度条。用收到 `map.clock_time` 的时刻做锚点，暂停时冻结，`matchid` 变化时重置。先做野怪刷新尝试、赏金符和神符。
2. 小地图敌方英雄。筛出 `minimap_enemyicon`，用 `unitname` 显示英雄名。空位丢掉。单位从列表消失就移除，不做消失之后的位置推测。
3. 本地道具冷却条。读取 `cooldown` 和 `max_cooldown`，跳过空槽。
4. 肉山。先打一局普通对局或人机，确认 `roshan` 块或 `map.roshan_state_end_seconds`。确认前只表示小地图上有没有肉山，以及是否收到死亡事件。
5. 进度条样式。同一条数据可以画成矩形或圆环。
6. 覆盖窗口。透明、无边框、置顶、鼠标穿透，并用全局热键切换穿透。Dota 使用无边框窗口。
7. 打包。`crt-static` 和 LTO，生成单个 exe。规则默认写进程序，可选的 `rules.toml` 放在 exe 旁边。

## 明确不做

- 敌方技能冷却推断。
- 英雄离开小地图数据之后的位置预测。
- 单个野怪营地是否已空。
- 神符是否仍在地上。
- 控制效果的剩余秒数。

## 下一次采集

打一局普通对局或人机。完全退出并重启 Dota，让新的 `gamestate_integration_dota2_tips.cfg` 生效，启动项保留 `-gamestateintegration`。然后在项目目录执行 `cargo run`。这次要看肉山倒计时字段是否出现，以及视野外的敌方英雄是否还会留在 `minimap` 里。
