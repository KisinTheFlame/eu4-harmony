# EU4 Harmony

[![CI](https://github.com/KisinTheFlame/eu4-harmony/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/KisinTheFlame/eu4-harmony/actions/workflows/ci.yml)

[English](README.md) | 简体中文

**一个运行在《欧陆风云4》内部、从里面把游戏变得更好的框架。** EU4 Harmony 能显示中文和其他所有文字，返回主菜单不用重启游戏，让游戏更快而不改变它算出的结果，修复游戏自身的 bug，让程序和 AI 来玩游戏，还让 Mod 用上 Lua 脚本。

> [!NOTE]
> EU4 Harmony 还在早期开发中。目前只支持 **Windows 上的 EU4 1.37.5，仅单人**，也还没有发布版本：请[从源码构建](#从源码构建)。扩展的接口在版本之间仍可能变化。

## 做了什么

### 显示所有文字

- 游戏里的全部文字都用系统字体绘制，中文、日文、韩文和其他所有文字不装字体 Mod 也能显示。
- 正文、标题、地图上的名字和游戏控制台各用自己的字体和字重，在设置里选。
- 输入的名字可以用任何文字。

[docs/unicode-module.md](docs/unicode-module.md)

### 离开一局不用重启

- 返回主菜单只要几秒，不再重启游戏（原来约 45 秒）。
- 铁人局也能在游戏菜单里读取另一个存档：先保存当前游戏；关掉这个设置的话，重新读取同一个存档就能撤销此后的游玩。

[docs/sessions.md](docs/sessions.md)

### 更快的游戏，同一局游戏

规则、数据、AI 都一样，过的日子也一样，只是算得更快：

- Paradox 的在线客户端等待连接时，不再在主线程上睡眠。
- 框架绘制的文字会记住断行结果，不再每帧重新计算。
- 本地存档先不压缩地写出，再在后台压缩，存档卡住游戏的时间更短。不装框架的游戏照样能读。
- 可选：最快速度下，一天要算很久时不在中途插画帧，日子还能再快一些。

在一局 1618 年的游戏里：暂停时每秒 84 帧而不是 63 帧，最快速度下日子快了约一成（[docs/performance.md](docs/performance.md)，[docs/game-speed.md](docs/game-speed.md)）。

### 修复游戏自身的 bug

这些 bug 原版里也有，框架在游戏出错的地方把它改正：

- **继承人名字：** 新继承人约有一半的概率取名单上的第一个名字（明朝是 Zhanji），连性别也跟着那个名字走。现在按权重抽名字。
- **反侵略联盟：** 多个国家的 AI 会在游戏的工作线程上同时退出同一个联盟，没有加锁，可能弄坏游戏的内存。现在退出和加入轮流进行。
- **读档后的成就：** 从游戏菜单读取的存档校验码永远对不上，之后整局都拿不到成就。现在能正确对上。
- **内存：** 游戏从不释放的内存，包括世界重置时（国家的数据、雇佣兵首领、叛军派系、AI 的特工）、修正变化时、界面重建时（地图模式窗口、选中部队栏、大纲条目）、文本框的动画、纹理的透明遮罩，以及这些残留占着的堆空闲页。

游戏仍然留下了什么，见 [docs/sessions.md](docs/sessions.md#known-leaks)。

### 成就规则由你定

两个默认关闭的设置，可以让非铁人局、开了改变游戏校验码的 Mod 的游戏也拿成就。成就的其他规则仍按原版。

### 用 Lua 写 Mod

- 扩展用 Lua 5.4 编写，在各自的沙箱里运行：不用安装什么，也不用编译。
- 国家、省份、军队、招募、外交、事件、时间、镜头和游戏控制台的能力，以及原版界面里的按钮。
- 扩展存档数据存在存档里；文字可以覆盖游戏的每种语言，按玩家的语言显示。
- 权限：扩展只有在清单里声明了，才能用现实时间、文件或网络，玩家能看到每个 Mod 声明了什么。
- 一个命令行工具：生成 Mod、检查 Mod、用假游戏跑它的测试、边改边在运行中的游戏里重载、打包上传。
- 任何装了 Lua 语言服务器的编辑器都有补全和类型检查，另有写给 coding agent 的指南。

见下文[写一个 Mod](#写一个-mod)。

### 远程控制与 AI 对局

- 游戏之外的程序像玩家一样，通过同样的能力玩游戏：从主菜单进入一局再回来，时间、事件、军队、招募、外交、控制台、镜头和截图。
- 没有能力覆盖的地方，它可以用玩家的鼠标和键盘，游戏在前台或被其他窗口挡住都行。
- 游戏可以隐藏启动，桌面上什么都不出现，让 AI 在后台玩，你照常做别的事。
- 只对正在玩的 Windows 用户开放，只限本机，玩家做不到的它也做不到。

见下文[远程控制](#远程控制)。

### 安全的设计

- 框架依赖的游戏里的每个位置，启动时都会和预期的字节核对。遇到不认识的游戏版本，框架整个不进入游戏，用一个消息框说明原因，而不是出错乱跑。
- Mod 脚本出错只结束那一次调用，不影响 Mod 和游戏；陷入死循环的脚本会在时间限制到了之后被停下。

### 研究与工具

- [docs/research/](docs/research/)：从游戏本身研究出的、对游戏代码的了解（军队、外交、存档、主线程、界面、文字等）。
- 堆普查和泄漏轮次，找出每离开一局游戏后内存里还留着什么（[docs/leak-finding.md](docs/leak-finding.md)）。
- 对照原版测量框架给游戏带来的开销（[docs/performance.md](docs/performance.md)）。
- 用 `eu4.exe` 离线核对每个依赖点；另有一套占用机制，让多个工作副本轮流使用同一个游戏，并备份、还原玩家的文件（[docs/dev-tools.md](docs/dev-tools.md)，[docs/game-testing.md](docs/game-testing.md)）。

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
| 字体 | 微软雅黑；地图用楷体，控制台用 Consolas | 正文、标题、地图和控制台用的系统字体。 |
| 返回主菜单时不重启游戏 | 开 | 几秒就回到主菜单。每次返回游戏会留下约 15 MB 内存，直到关闭游戏（[原因](docs/sessions.md#known-leaks)）。 |
| 铁人模式下读档前先保存当前游戏 | 开 | 铁人局也能在游戏菜单里读取另一个存档。关掉后，重新读取同一个存档就能撤销此后的游玩。 |
| 非铁人游戏也可获得成就 | 关 | 非铁人局里也能拿成就，读档重来、用控制台都不影响。 |
| 游戏校验码与原版不同时也可获得成就 | 关 | 开着会改变主菜单上校验码的 Mod 也能拿成就。 |
| 游戏速度：最快速度时 | 画面流畅 | 选"时间更快"后，一天要算很久时不在中途插画帧：游戏后期日子还能再快约一成，但地图会一顿一顿，点击反应也慢一些。 |

字体的修改在下次启动时生效，成就规则在下一次开新局或读档时生效，其余保存后立即生效。成就的其他规则仍按原版。始终开启的那些加速，以及它们为什么不会改变游戏，见 [docs/game-speed.md](docs/game-speed.md)。

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
eu4-harmony new my-mod --name "My Mod"   # 一个能直接用的 Mod：带一个按钮、中英文字和编辑器类型定义
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
