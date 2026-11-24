# dota2-tips

本地 Dota 2 提示窗口。只读取官方 Game State Integration，不读内存，不注入进程。

窗口置顶，显示野怪刷新、野区赏金、中路圣水、中路神符和自己的道具冷却。敌方英雄按小地图图标分成「已出现」和「未出现」。

## 直接使用

没有 Rust 时，从 [Releases](https://github.com/sNightwind/dota2-tips/releases) 下载：

- `dota2-tips.exe`
- `gamestate_integration_dota2_tips.cfg`

把 cfg 复制到 Dota 的 `game/dota/cfg/gamestate_integration/`。Steam 启动项加上 `-gamestateintegration`，然后完全退出并重新打开 Dota。双击 exe。第一次运行会在程序旁边写出 `config.toml`。

默认监听 `127.0.0.1:5394`，token 是 `dota2-tips`。

## 窗口

- 右上角「设置」，或按 `Ctrl+Shift+F8`。
- 拖标题栏移动窗口。
- `Esc` 或「关闭设置」回到对局显示。
- 语言可选中文或英文。英雄名可选全名、简称，或全名加简称，例如幻影刺客PA。简称按英文单词首字母。
- 每条倒计时可以单独开关。
- 对局结束、断开连接或退出游戏后，倒计时停止，显示「等待对局」。

## 默认时间

按 7.42，单位是秒，可在 `config.toml` 或设置里改：

| 内容 | 时间 |
| --- | --- |
| 野怪 | 1:00 后每 60 秒一次刷新尝试 |
| 野区赏金 | 0:00 起每 240 秒。0:00 那一波河上赏金点也会刷，之后只在野区 |
| 中路圣水 | 2:00 和 4:00，两侧各一枚。4:00 之后这条不再显示 |
| 中路神符 | 6:00 起每 120 秒，两个点里随机一侧一枚 |

这些条只表示下一次刷新。看不出野区格子是否已空，也看不出神符是否还在地上。

道具冷却走完后显示「已就绪」。没有冷却的物品不列出。

「已出现」是当前小地图上的敌方图标。「未出现」是刚才还在、现在不在的英雄，并显示离开后的秒数。两列可以分别关闭。

启动时会检查 `config.toml`：没有就生成，旧字段删掉，缺的字段补上。设置里的「修复配置」会再做一次。

## 做不到的事

- 敌方技能冷却
- 肉山复活倒计时
- 离开小地图之后的位置
- 控制效果的剩余秒数
- 敌方建筑血量

## 从源码运行

需要 Rust。

```powershell
cargo run
cargo run -- --capture
cargo run -- --self-test
cargo build --release
```

`--capture` 把收到的字段写到 `captures/`。发布版使用静态链接和 LTO。
