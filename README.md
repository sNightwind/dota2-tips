# dota2-tips

本地 Dota 2 提示窗口。只读取官方 Game State Integration，不读内存，不注入进程。

## 使用

没有 Rust 时，从 [Releases](https://github.com/sNightwind/dota2-tips/releases) 下载 `dota2-tips.exe` 和 `gamestate_integration_dota2_tips.cfg`。

把 cfg 复制到 Dota 的 `game/dota/cfg/gamestate_integration/`。Steam 启动项加上 `-gamestateintegration`，完全退出并重新打开 Dota，然后双击 exe。

默认监听 `127.0.0.1:5394`，token 是 `dota2-tips`。第一次运行会在程序旁边写出 `config.toml`。之后每次启动都会按当前版本补上缺的字段、去掉失效字段。

- 右上角「设置」，或按 `Ctrl+Shift+F8`。拖标题栏移动窗口。`Esc` 关闭设置。
- 语言可选中文或英文。英雄名可选全名、简称，或全名加简称，例如幻影刺客PA。
- 每条倒计时可以单独开关。对局结束、断开或退出游戏后，倒计时停止，显示「等待对局」。

默认时间可以在设置里改：

| 内容 | 时间 |
| --- | --- |
| 野怪 | 1:00 后每 60 秒 |
| 拉野 | 每轮野怪刷新的第 53–55 秒。进入区间后进度条变黄，结束后消失，下一轮开始再出现 |
| 野区赏金 | 0:00 起每 240 秒 |
| 中路圣水 | 2:00 和 4:00，两侧各一枚 |
| 中路神符 | 6:00 起每 120 秒，随机一侧一枚 |

道具冷却走完后显示「已就绪」。「已出现」是当前小地图上的敌方英雄。「未出现」在离开超过设定秒数后才提示，英雄名为黄色，并显示离开后的秒数。两列可以分别关闭。
