# EU4 Harmony

[![CI](https://github.com/KisinTheFlame/eu4-harmony/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/KisinTheFlame/eu4-harmony/actions/workflows/ci.yml)

[English](README.md) | 简体中文

**从内部把《欧陆风云4》变得更好。** 处处能显示中文、不用重启、游戏更快、bug 更少，Mod 也能做到多得多的事。

> [!NOTE]
> 早期开发中：只支持 Windows 上拥有全部 DLC 的 EU4 1.37.5，仅单人。还没有发布版本，请[从源码构建](#从源码构建)。

## 为什么用 EU4 Harmony

- **任何文字，处处显示。** 任何文字在整个游戏里都能显示，默认用为你的语言准备的字体，也可以自己选，不用装字体 Mod。
- **不再重启。** 几秒回到主菜单，而不是 45 秒。铁人局也能读取别的存档。
- **更快的游戏，同一局游戏。** 更流畅，最快速度下快约一成，存档时的卡顿只剩约三分之一，游戏本身一点没变。
- **修好原版至今还有的 bug。** 继承人不会再一半概率重名，反侵略联盟不再有崩溃风险，读档也不会再让你丢掉成就。
- **成就规则由你定。** 可选：非铁人或开着 Mod 也能拿成就。
- **Mod 能做更多。** 几行 Lua，Mod 就能读取游戏、添加按钮，并在游戏的各个面板里做你能做的事（建造、发展、研究、选理念和改革、贸易、宣战与议和、每一种外交行动），不允许时给出游戏自己的原因。
- **让 AI 来玩。** 程序可以像你一样替你玩完整局游戏，甚至在后台隐藏着玩。

## 安装（玩家）

1. 拿到玩家包：一个文件夹，里面是 `version.dll` 和 `eu4-harmony\` 文件夹。在有发布版本之前，用 `cargo xtask release` 构建它（见[从源码构建](#从源码构建)）。
2. 游戏根目录里原本已有 `version.dll` 的话，先把它移进 `eu4-harmony\backup\`，框架不会加载它。
3. 把 `version.dll` 和 `eu4-harmony\` 复制到游戏根目录，和 `eu4.exe` 放在一起。
4. 照常从 Paradox 启动器启动游戏，在合集里启用 EU4 Harmony Mod。

主菜单角落里会多出两个按钮：**EU4 Harmony 设置**和 **EU4 Harmony Mod**，后者能看到每个 Mod 的名称、版本，以及它在游戏之外能做什么（它的权限）。

**卸载：** 删掉根目录里的 `version.dll` 和 `eu4-harmony\`；之前备份过 `version.dll` 的话，把它移回去。

需要知道的几件事：

- 用框架玩的游戏里，你输入的名字以 UTF-8 保存，卸掉框架后会显示成乱码。
- 会保存数据的扩展把数据写进存档；这样的存档不保证在没有框架时还能读取。
- 日志和启动报告（`report.txt`）在 `Documents\Paradox Interactive\Europa Universalis IV\eu4-harmony\`。出问题时先看这里。

### 设置

| 设置 | 默认 | 作用 |
| --- | --- | --- |
| 语言 | Windows 的显示语言，不在十二种之内就用英语 | 框架和 Mod 的文字用哪种语言，十二种可选。 |
| 字体 | 按语言准备：拉丁和西里尔字母用 Segoe UI，地图用 Georgia；简体中文用微软雅黑，地图用楷体（[全部](docs/player-language.md#fonts)）；控制台用 Consolas | 正文、标题、地图和控制台用的系统字体。 |
| 返回主菜单时不重启游戏 | 开 | 几秒就回到主菜单。每次返回游戏会留下约 15 MB 内存，直到关闭游戏（[原因](docs/sessions.md#known-leaks)）。 |
| 铁人模式下读档前先保存当前游戏 | 开 | 铁人局也能在游戏菜单里读取另一个存档。关掉后，重新读取同一个存档就能撤销此后的游玩。 |
| 非铁人游戏也可获得成就 | 关 | 非铁人局里也能拿成就，读档重来、用控制台都不影响。 |
| 游戏校验码与原版不同时也可获得成就 | 关 | 开着会改变主菜单上校验码的 Mod 也能拿成就。 |
| 远程控制可以看到全部 | 关 | 关闭时，通过远程控制使用游戏的程序只能读到玩家看得到的内容：战争迷雾下的地图，以及其他国家面板上显示的信息。打开后能读到游戏知道的全部，和 mod 的扩展一直以来一样（[原因](docs/adr/remote-control-reads-the-players-view.md)）。 |
| 游戏速度：最快速度时 | 画面流畅 | 选"时间更快"后，一天要算很久时不在中途插画帧：游戏后期日子还能再快约一成，但地图会一顿一顿，点击反应也慢一些。 |

语言和字体的修改在下次启动时生效，成就规则在下一次开新局或读档时生效，其余保存后立即生效。成就的其他规则仍按原版。始终开启的那些加速，以及它们为什么不会改变游戏，见 [docs/game-speed.md](docs/game-speed.md)。

## 写一个 Mod

Mod 的扩展通过能力读取游戏、在游戏里行事，从不接触内存地址：

```lua
-- eu4-harmony/lua/main.lua：一个在 eu4-harmony/ui/ 里声明的按钮，
-- 选中自己的军队时出现在军队面板上
return {
  on_frame = function(game)
    game.ui.button.visible = game.army.selected() ~= nil
  end,

  on_button = function(game)
    log("army " .. tostring(game.army.selected()) .. " clicked")
  end,
}
```

命令行工具是游戏 `eu4-harmony\` 文件夹里的 `eu4-harmony.exe`：

```powershell
eu4-harmony new my-mod --name "My Mod"   # 一个能直接用的 Mod：带一个按钮、英文和你的语言的文字，以及编辑器类型定义
cd my-mod
eu4-harmony check                         # 像框架那样把所有文件读一遍
eu4-harmony test                          # 用假游戏运行 eu4-harmony\tests\*.lua
eu4-harmony link                          # 注册到启动器，然后在合集里启用
eu4-harmony reload --watch                # 游戏运行时，文件一改动就重载扩展
eu4-harmony package                       # 生成一个可以直接上传的干净文件夹
```

Mod 就是一个普通的 Paradox Mod，多一个 `eu4-harmony\` 文件夹，文件放在哪里就说明它是什么：

```text
my-mod/
├── descriptor.mod
└── eu4-harmony/
    ├── manifest.toml          # id、api_version、description、permissions
    ├── lua/main.lua           # 扩展：一张事件函数表
    ├── ui/<窗口名>/buttons/   # 拼进原版窗口的界面片段
    ├── localisation/          # 文字，每种语言一个 UTF-8 文件
    ├── data/                  # 扩展要读的文件
    └── tests/                 # eu4-harmony test 运行的测试
```

`main.lua` 返回各个事件的事件函数，比如 `on_session`、`on_day`、`on_frame`、`on_button`。每个都拿到 `game`，所有能力都从它调用；被拒绝时返回 `nil, err`，带着游戏给的理由；事件函数还可以等一个答复，比如别国对提议的答复。需要现实时间、文件或网络的扩展，要在清单的 `permissions` 里声明。

进一步了解：

- `eu4-harmony wiki`：每项能力的参数和结果、事件函数和全局函数（`wiki lua`）、清单（`wiki manifest`）。不需要游戏运行。
- `eu4-harmony skills mod`：每类工作的分步指南，写给 coding agent，人也能读。
- [docs/lua-extensions.md](docs/lua-extensions.md)：扩展如何工作，包括出错、时间限制和存档数据。

## 远程控制

`eu4-harmony game <能力> [<参数>]` 向运行中的游戏发一个请求，以 JSON 打印回答：

```powershell
eu4-harmony start --hidden                 # 启动游戏，桌面上什么都不出现
eu4-harmony game menu start FRA            # 以法兰西开新局
eu4-harmony game time advance 30           # 让 30 天过去；有事件要回答时提前停下
eu4-harmony game question list             # 等待玩家回答的事件
eu4-harmony game army move 545460846646 185
eu4-harmony game screenshot --out shot.png
eu4-harmony game menu return               # 回到主菜单
```

远程控制只对正在玩的 Windows 用户开放、只限本机，玩家做不到的它也做不到。没有能力覆盖的地方，它可以用玩家的鼠标和键盘。`eu4-harmony wiki remote` 讲一局怎么进行、每个退出码的含义；设计见 [docs/remote-control.md](docs/remote-control.md) 和 [docs/hidden-start.md](docs/hidden-start.md)。

## 文档

除本文件外，文档都是英文。

| | |
| --- | --- |
| [docs/v1.md](docs/v1.md) | 第一版的设计 |
| [GLOSSARY.md](GLOSSARY.md) | 项目用的术语 |
| [docs/adr/](docs/adr/) | 难以撤回的决定，以及做出它们的理由 |
| [docs/research/](docs/research/) | 对游戏代码的了解 |
| [docs/performance.md](docs/performance.md) | 框架给游戏带来的开销，实测 |
| [docs/prototype-log.md](docs/prototype-log.md) | 在运行中的游戏里试过什么、结果如何 |
| [docs/dev-tools.md](docs/dev-tools.md) | 开发工具 |

每项较大的工作在 [docs/](docs/) 里有自己的方案，列在 [AGENTS.md](AGENTS.md) 里。

## 从源码构建

需要 Windows、[rustup](https://rustup.rs/)（它会按 [rust-toolchain.toml](rust-toolchain.toml) 选 Rust 版本）和 Visual Studio C++ 生成工具。只有运行框架和离线检查需要游戏。

```powershell
cargo build --release
cargo test --release
cargo xtask install        # 把构建装进游戏目录（--uninstall 卸载）
cargo xtask release        # 打出玩家包
```

游戏通过 `EU4_GAME_DIR` 或 Steam 找到。`cargo xtask --help` 列出其他开发命令。

## 参与开发

先读 [AGENTS.md](AGENTS.md)：代码、提交、检查和在游戏里测试的规则都在那里，对人和 coding agent 都适用。改动通过 pull request 进入 master。

| 目录 | 内容 |
| --- | --- |
| `proxy/` | 把核心加载进 `eu4.exe` 的 `version.dll` |
| `core/` | 框架本身：依赖点与启动、能力、运行在真实游戏上的 Lua 宿主、远程控制、框架界面和 Unicode 模块 |
| `lua/` | 游戏和假游戏共用的 Lua 宿主：沙箱、事件函数、任务、时间限制 |
| `remote/` | 能力目录：每项能力的参数、结果和说明 |
| `checks/` | 能力调用要经过的检查，以及游戏的历法 |
| `loc/`、`manifest/` | 本地化文件和 Mod 清单，核心和命令行工具共用 |
| `cli/` | `eu4-harmony.exe`，以及 `new` 生成的模板 Mod |
| `xtask/` | 仓库自己的开发命令（`cargo xtask`） |

## 许可证

[MIT](LICENSE)。它只涵盖本仓库自己的代码和文档，不涵盖游戏本身：Mod 能做什么，仍然以 Paradox 对 Mod 的条款为准。
