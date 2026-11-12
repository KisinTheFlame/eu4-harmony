![Harmony for Europa Universalis IV](docs/images/banner.jpg)

[![CI](https://github.com/KisinTheFlame/eu4-harmony/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/KisinTheFlame/eu4-harmony/actions/workflows/ci.yml)

[English](README.md) | 简体中文

**从内部把《欧陆风云4》变得更好。** 处处能显示中日韩文字、不用重启、游戏更快、bug 更少，Mod 也能做到多得多的事，而游戏的规则保持原样。

| | 原版游戏 | Harmony |
| --- | :-: | :-: |
| 中文、日文、韩文正常显示，不需要双字节补丁或字体 Mod | ✗ | ✓ |
| 几秒回到主菜单，不用重启游戏 | ✗ | ✓ |
| 铁人模式里读取别的存档 | ✗ | ✓ |
| 高速运行时，地图的拖动和缩放依然流畅，悬停核心旗帜的省份轮廓也正常显示 | ✗ | ✓ |
| 存档时的卡顿只剩三分之一 | ✗ | ✓ |
| 启动和读档更快 | ✗ | ✓ |
| 游戏后期跑得更快 | ✗ | ✓ |
| 继承人不再一半概率重名 | ✗ | ✓ |
| 降低崩溃风险 | ✗ | ✓ |
| 读档不再让你丢掉成就 | ✗ | ✓ |
| 长局不再越玩越占内存 | ✗ | ✓ |
| 可选：非铁人或开着 Mod 也能拿成就 | ✗ | ✓ |
| Mod 能做的事多得多：加按钮、加窗口、替你操作 | ✗ | ✓ |
| **游戏的规则和数据不变** | ✓ | ✓ |

游戏的中文文本仍然来自汉化 Mod，照常订阅即可；为双字节补丁制作的汉化 Mod 和用它们玩的存档都能直接使用。Harmony 运行于 Windows 上的 EU4 1.37.5，仅支持单人游戏。

## 安装

1. 如果装过让游戏显示中文、日文或韩文的补丁，先按那个补丁自己的说明把它卸载。
2. 在 Steam 创意工坊订阅 **Harmony**。
3. 在 Steam 里右键《欧陆风云4》，选**管理 → 浏览本地文件**，打开游戏目录。
4. 打开 Harmony 的创意工坊目录：同一个 Steam 库里的 `steamapps\workshop\content\236850\<id>\`，在游戏目录往上两级；`<id>` 是 Harmony 创意工坊页面地址末尾的那串数字。把那里的 `version.dll` 复制到游戏目录，和 `eu4.exe` 放在一起。游戏目录里原本已有 `version.dll` 的话，先把它挪到别处，Harmony 不会加载它。

照常从 Paradox 启动器启动游戏。Harmony 不用放进合集；基于它的 Mod 照常在合集里启用。主菜单角落会多出两个按钮：**Harmony 设置**和 **Harmony Mod**，后者列出每个基于 Harmony 的 Mod，以及它在游戏之外能做什么。

Steam 会让 Harmony 保持最新。偶尔有更新需要换新的 `version.dll`，这时 Harmony 会在启动时提示你重新复制一次。

**卸载：** 取消订阅 Harmony，游戏就回到原样。想不留痕迹，再删掉游戏目录里的 `version.dll`；之前挪走过别的 `version.dll` 的话，把它放回去。

需要知道的几件事：

- 用 Harmony 玩的游戏里，你输入的名字以 UTF-8 保存，没有 Harmony 时会显示成乱码。
- 会保存自己数据的 Mod 把数据写进存档；这样的存档不保证在没有 Harmony 时还能读取。
- 装上 Harmony 后，游戏自己的音量滑块按位置的平方变化，每一格听起来变化差不多大。原来低于 100 的音量会比以前轻，重新调一次即可。

## 出问题时

在主菜单打开 Harmony 的设置，进入**关于**，点**把排查问题用的文件存到桌面**：它把 Harmony 的日志、游戏的日志、最近几次崩溃的文件夹、你的设置和模组列表（不含存档）打成一个 zip 文件放到桌面上。到 [GitHub Issues](https://github.com/KisinTheFlame/eu4-harmony/issues) 反馈问题，附上这个文件。

日志在 `Documents\Paradox Interactive\Europa Universalis IV\eu4-harmony\logs\`：`report.txt` 记着游戏启动时 Harmony 发现了什么、做了什么，`core.log` 记录框架工作及经过游戏异常处理的崩溃；只有出了问题才会有 `proxy.log`，它说明 Harmony 为什么根本没启动。框架日志分段限长、汇总重复错误，并保留小型近期事件快照；自动清理不再使用的过期自有日志，不动游戏日志、存档、崩溃转储和你保存的排障包。大文本日志只把注明截断的尾部加入排障包。[日志限制与诊断范围](docs/logs.md)说明了保留内容，以及异常退出后仍可能无法确定的事情。

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

命令行工具 `eu4-harmony.exe` 目前要从源码构建（见[从源码构建](#从源码构建)）：

```powershell
eu4-harmony new my-mod --name "My Mod"   # 一个能直接用的 Mod：带一个按钮、英文和你的语言的文字，以及编辑器类型定义
cd my-mod
eu4-harmony check                         # 像框架那样把所有文件读一遍
eu4-harmony test                          # 用假游戏运行 eu4-harmony\tests\*.lua
eu4-harmony link                          # 注册到启动器，然后在合集里启用
eu4-harmony reload --watch                # 游戏运行时，文件一改动就重载扩展
eu4-harmony types                         # 工具更新后，重新写出编辑器类型定义
eu4-harmony package                       # 生成一个可以直接上传的干净文件夹
```

Mod 就是一个普通的 Paradox Mod，多一个 `eu4-harmony\` 文件夹，文件放在哪里就说明它是什么：

```text
my-mod/
├── descriptor.mod
├── .eu4-harmony/types/        # 编辑器类型定义，由 eu4-harmony new 和 eu4-harmony types 写出
├── .luarc.json                # 让编辑器找到它们
└── eu4-harmony/
    ├── manifest.toml          # id、api_version、description、permissions
    ├── lua/main.lua           # 扩展：一张处理函数表
    ├── ui/<窗口名>/buttons/   # 拼进原版窗口的界面片段
    ├── localisation/          # 文字，每种语言一个 UTF-8 文件
    ├── data/                  # 扩展要读的文件
    └── tests/                 # eu4-harmony test 运行的测试
```

`main.lua` 返回处理函数，比如过了一天时的 `on_day`、按钮被点击时的 `on_button`。每个都拿到 `game`，所有能力都从它调用；被拒绝时返回 `nil, err`，带着游戏给的理由。处理函数可以等一个答复，比如别国对提议的答复，或者等游戏里过去若干天，也可以把几件事作为任务同时进行、再等它们完成。需要现实时间、文件或网络的扩展，要在清单的 `permissions` 里声明，玩家在 Harmony Mod 窗口里能看到。

`province.area` 读取一个州内的自有省份与行政容量占用，包括只设立直辖州、不补完整核心时的新增占用。`country.governing_capacity` 单独实时重算全国占用，以整数千分值返回；两者的区别见[省份机制](docs/game/provinces.md)。

扩展的悬浮提示是可读写的文字描述：`{ text = "Ready" }` 或 `{ key = "MYMOD_DETAILS", values = { N = 3 } }`。赋值替换内容，`nil` 清空内容，窗口重建后也保留这一状态；布局与属性见[窗口指南](cli/skills/ui/windows.md)。

进一步了解：

- `eu4-harmony wiki`：每项能力的参数和结果、处理函数和全局函数（`wiki lua`）、清单（`wiki manifest`）。不需要游戏运行。
- `eu4-harmony skills`：每类工作的分步指南（`skills mod` 讲做一个 Mod，还有 `skills ui`、`skills testing` 等），写给 coding agent，人也能读。
- [docs/plans/mod-capabilities.md](docs/plans/mod-capabilities.md)：框架向 Mod 开放了什么，按领域列出。
- [docs/plans/lua-extensions.md](docs/plans/lua-extensions.md)：扩展如何工作，包括出错、时间与内存限制和存档数据。

## 远程控制：让程序来玩

程序（包括 AI）可以像玩家一样玩完整局游戏，甚至在后台隐藏着玩。`eu4-harmony game <能力> [<参数>]` 向运行中的游戏发一个请求，以 JSON 打印回答：

```powershell
eu4-harmony start --hidden                 # 启动游戏，桌面上什么都不出现；Steam 须在运行
eu4-harmony game menu start FRA            # 以法兰西开新局
eu4-harmony game time advance 30           # 让 30 天过去；有事件要回答时提前停下
eu4-harmony game question list             # 等待玩家回答的事件
eu4-harmony game army move 545460846646 185
eu4-harmony game screenshot --out shot.png
eu4-harmony game menu return               # 回到主菜单
```

观察模式下 AI 操控所有国家：`menu start` 不带国家就开这样一局，`session view FRA` 把法兰西设为所看国家，`session leave_view` 退出视角；这时以国家身份行事的能力作用于所看国家，AI 照常操控它，没有所看国家时返回 `no_country`（[docs/plans/observe-mode.md](docs/plans/observe-mode.md)）。

远程控制只对正在玩的 Windows 用户开放、只限本机。请求默认以玩家身份进行：只读玩家看得到的，只做玩家能做的。加上 `--as-game` 就以游戏身份进行：能读游戏保存的一切，还能像 Mod 的脚本那样改动游戏，做玩家做不到的事（[ADR requests-act-as-the-player-or-as-the-game](docs/adr/2026-11-11-requests-act-as-the-player-or-as-the-game.md)）。没有能力覆盖的地方，它可以用玩家的鼠标和键盘。`eu4-harmony wiki remote` 讲一局怎么进行、每个退出码的含义；设计见 [docs/plans/remote-control.md](docs/plans/remote-control.md) 和 [docs/plans/hidden-start.md](docs/plans/hidden-start.md)。

## 文档

| | |
| --- | --- |
| [docs/plans/v1.md](docs/plans/v1.md) | 第一版的设计 |
| [GLOSSARY.md](GLOSSARY.md) | 项目用的术语 |
| [docs/adr/](docs/adr/) | 难以撤回的决定，以及做出它们的理由 |
| [docs/game/](docs/game/) | 对游戏的认识，每个系统一页 |
| [docs/research/](docs/research/) | 这些页面所依据的、对游戏代码的调研 |
| [docs/performance.md](docs/performance.md) | 框架给游戏带来的开销，实测 |
| [docs/prototype-log/](docs/prototype-log/) | 在运行中的游戏里试过什么、结果如何，每次一个文件 |
| [docs/dev-tools.md](docs/dev-tools.md) | 开发工具 |

每项较大的工作在 [docs/plans/](docs/plans/) 里有自己的方案，和其他文档一起列在 [docs/README.md](docs/README.md) 里。

## 从源码构建

需要 Windows、[rustup](https://rustup.rs/)（它会按 [rust-toolchain.toml](rust-toolchain.toml) 选 Rust 版本）和 Visual Studio C++ 生成工具。只有运行框架和离线检查需要游戏。

```powershell
cargo build --release
cargo test
cargo xtask release        # 打出玩家包和创意工坊条目的文件夹
```

`cargo xtask release` 打出 `target\eu4-harmony\`：一个文件夹，里面是 `version.dll` 和装着核心与命令行工具的 `eu4-harmony\` 文件夹，另有它的 zip。要手动安装一个构建，就把这两样复制到游戏目录，和 `eu4.exe` 放在一起；`cargo xtask install` 就是做这件事的，`--uninstall` 把它们删掉。游戏目录里的核心先于创意工坊的被加载，所以这样装的构建就是实际运行的那个。

游戏通过 `EU4_GAME_DIR` 或 Steam 找到。要在游戏里试构建，就认领一局测试游戏：它从游戏目录启动，有自己的用户文件夹、核心和远程控制管道，既不碰玩家的文件，也不碰游戏目录里装好的框架，几局可以同时跑（[docs/plans/test-games.md](docs/plans/test-games.md)）。游戏目录里要先装一次支持测试游戏的框架，装时玩家自己的游戏要关着（`cargo xtask install`）。

```powershell
cargo build --release --features eu4-harmony-core/dev-test-games
cargo xtask game claim --owner mine --purpose "try my change"
cargo xtask game start --owner mine     # 用这个构建隐藏启动测试游戏
eu4-harmony game --test-game mine session get
cargo xtask game release --owner mine   # 留下日志，删掉其余
```

框架和游戏一样，按游戏自己的 `-userdir` 找用户文件夹。

`cargo xtask --help` 列出其他开发命令。

## 参与开发

先读 [AGENTS.md](AGENTS.md)：代码、提交、检查和在游戏里测试的规则都在那里，对人和 coding agent 都适用。改动通过 pull request 进入 master。

| 目录 | 内容 |
| --- | --- |
| `proxy/` | 把核心加载进 `eu4.exe` 的 `version.dll` |
| `core/` | 框架本身：依赖点与启动、能力、运行在真实游戏上的 Lua 宿主、远程控制、框架界面和 Unicode 模块 |
| `lua/` | 游戏和假游戏共用的 Lua 宿主：沙箱、处理函数、任务、时间限制、扩展按什么顺序收到什么，以及扩展要它的窗口和弹窗是什么样 |
| `remote/` | 能力目录：每项能力的参数、结果和说明 |
| `checks/` | 能力调用要经过的检查，以及游戏的历法 |
| `client/` | 从游戏进程外接触运行中的游戏：远程控制的管道；游戏进程的启动、跟踪到退出码和结束；备份和还原玩家的文件，命令行工具和 `cargo xtask` 共用 |
| `folders/` | 各个程序以同一方式找到的游戏文件夹和用户文件夹，以及用户文件夹里各项的名字 |
| `logging/` | 有界框架日志、重复错误汇总、事件快照及认证字段脱敏 |
| `loc/`、`manifest/` | 本地化文件和 Mod 清单，核心和命令行工具共用 |
| `cli/` | `eu4-harmony.exe`，以及 `new` 生成的模板 Mod |
| `xtask/` | 仓库自己的开发命令（`cargo xtask`） |

## 许可证

[MIT](LICENSE)。它只涵盖本仓库自己的代码和文档，不涵盖游戏本身：Mod 能做什么，仍然以 Paradox 对 Mod 的条款为准。
