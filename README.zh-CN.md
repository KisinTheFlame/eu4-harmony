# EU4 Harmony

[English](README.md) | 简体中文

让《欧陆风云4》的 Mod 带上 Lua 脚本的框架。框架负责进入游戏进程、读取游戏状态、下达命令、往原版界面注入元素、按存档保存数据；Mod 的扩展只通过框架提供的能力接触游戏，不碰内存地址。设计见 [docs/v1.md](docs/v1.md)，术语见 [GLOSSARY.md](GLOSSARY.md)，决定的理由见 [docs/adr/](docs/adr/)（这些文档都是英文）。

支持的游戏：原版 1.37.5，以及与它布局相同的局部补丁 exe。仅单人。

## 安装框架（玩家）

框架以一个文件夹（或它的 zip）提供，里面是 `version.dll` 和 `eu4-harmony\` 文件夹，后者装着核心（`eu4_harmony_core.dll`）和命令行工具（`eu4-harmony.exe`）；`tools\release.ps1` 从 release 构建生成这两者。把 `version.dll` 和 `eu4-harmony\` 文件夹放进游戏根目录。根目录原本已有 `version.dll` 时，先把它移进 `eu4-harmony\backup\`，框架不会加载它。框架自己负责显示中文和其他文字（[ADR 0010](docs/adr/0010-built-in-unicode-module.md)）。之后照常从 Paradox 启动器启动，在合集里启用基于框架的 Mod。为框架早先版本做的、扩展是 DLL 的 Mod 不再加载，它们的扩展需要用 Lua 重写。

卸载：删掉根目录的 `version.dll`；有备份的 `version.dll` 时，把它从 `eu4-harmony\backup\` 移回根目录；最后删掉 `eu4-harmony\`。

用框架玩出的存档里，玩家输入或改过的名字是 UTF-8，卸掉框架后会显示为乱码。

日志和启动报告（`report.txt`）在 `Documents\Paradox Interactive\Europa Universalis IV\eu4-harmony\`。扩展存档数据直接写在存档里；用会保存数据的扩展玩出的存档，不保证在没有框架时还能读取。

## 离开一局游戏

返回主菜单时游戏不再重启：几秒钟就回到主菜单，而不是约 45 秒（[ADR 0022](docs/adr/0022-return-to-the-main-menu-in-process.md)）。铁人模式下也能在游戏菜单里读取另一个存档，读取前会先保存当前游戏（[ADR 0023](docs/adr/0023-in-game-load-in-ironman.md)）。两者都是框架设置，在主菜单角落"EU4 Harmony 设置"按钮打开的设置窗口里：关掉"返回主菜单时不重启游戏"，就会像原版一样重启；关掉"铁人模式下读档前先保存当前游戏"，重新读取同一个存档就能撤销此后的游玩（悔棋）。这样离开的每一局，游戏都会留下一些内存，每次返回约 15 MB，直到关闭游戏才释放（[已知泄漏](docs/sessions.md#known-leaks)）；返回很多次之后，重启一次游戏就能全部收回。

## 远程控制（玩家与 AI）

远程控制让游戏进程之外的程序通过框架的能力使用游戏，就像玩家一样：从主菜单进入游戏再回来，读取游戏，让时间流逝，回答游戏的事件和其他国家的提议，调动军队，招募，外交，游戏控制台，镜头和截图。AI 靠它玩完整的一局，开发者靠它在命令行里试用框架。框架运行时它始终开启，只对正在玩的 Windows 用户开放、只限本机（一个命名管道），并且以玩家身份行事：玩家做不到的，它也做不到（[ADR 0015](docs/adr/0015-remote-control-uses-capabilities.md)、[ADR 0017](docs/adr/0017-remote-control-is-always-on.md)）。

客户端是游戏 `eu4-harmony\` 文件夹里的 `eu4-harmony.exe`。`eu4-harmony game <能力> [<参数>]` 向运行中的游戏发一个请求，以 JSON 打印回答；退出码为 0 表示游戏回答了，1 表示游戏回答了一个错误（`{"code": ..., "message": ...}`，拒绝时带着游戏自己的理由），2 表示命令行有误、什么也没发出，3 表示没有收到回答。参数可以带名字（`--province 167`），也可以按顺序直接写；能力名里的点可以写成空格：

```powershell
cd "<游戏目录>\eu4-harmony"
.\eu4-harmony wiki remote                 # 一局怎么进行、每个退出码的含义
.\eu4-harmony game menu                    # 游戏在哪里：main_menu、starting 或 game
.\eu4-harmony game menu start FRA          # 以法兰西开新局，准备好后回答
.\eu4-harmony game question list           # 等待玩家回答的游戏事件
.\eu4-harmony game question answer 2 0     # 用其中一个选项回答
.\eu4-harmony game time advance 30         # 让 30 天过去后暂停；有事件要回答时提前停下
.\eu4-harmony game army list               # 玩家的军队
.\eu4-harmony game army move 545460846646 185
.\eu4-harmony game diplomacy war_goals ENG
.\eu4-harmony game diplomacy declare_war ENG cb_core --province 167
.\eu4-harmony game screenshot --scale 50 --out shot.png
.\eu4-harmony game menu return             # 回到主菜单，主菜单出现后回答
```

按组列出的能力：

| 组 | 能力 |
| --- | --- |
| 主菜单 | `menu`（游戏在哪里）、`menu.saves`、`menu.load`、`menu.start`、`menu.return` |
| 会话与时间 | `session.get`、`time.get`、`time.pause`、`time.resume`、`time.set_speed`、`time.advance`（让若干天过去并等待，带回途中的事件） |
| 国家与省份 | `country.get`（国库、人力、稳定度、威望、君主点数、战争、盟友、附属国与宗主国及附属类型、宿敌）、`country.hostile`、`province.list`（全部省份，或指定的那些）、`province.get`（名字、所有者、发展度、地形、要塞、建筑）、`province.neighbors` |
| 军队 | `army.list`、`army.get`（带剩余路线和不能接受命令的原因）、`army.at`、`army.route_to`、`army.can_step`、`army.range`（军队若干步内能到达哪些省份，以及在其中怎么走）、`army.can_attack`、`army.move`、`army.stop`、`army.selected` |
| 招募 | `recruit.options`（一个省份能招募什么、为什么不能）、`recruit.order`（招募某一类的一个团或一艘船） |
| 事件与提议 | `question.list`、`question.answer`（等待玩家回答的游戏事件）；`offer.list`、`offer.accept`、`offer.decline`（其他国家的提议） |
| 外交 | `diplomacy.war_goals`、`diplomacy.declare_war`（召集盟友参战）、`diplomacy.offer_peace`（白和平）、`diplomacy.propose_alliance` |
| 游戏控制台 | `console.run`（像玩家那样输入一条命令，带回它的输出） |
| 镜头与截图 | `camera.get`、`camera.look_at`（省份或地图位置）、`camera.zoom`；`screenshot`（玩家看到的画面，PNG） |
| 事件流与说明 | `events`（编号的事件：会话开始或结束、新的一天、军队被下令或被击败）、`describe`（运行中的游戏给出的能力目录版本和全部能力）、`text.language` |

完整的列表，连同每个参数和结果，见 `eu4-harmony wiki`（不需要游戏运行）：`eu4-harmony wiki army` 看一组，`eu4-harmony wiki army move` 看单个能力，`eu4-harmony wiki errors` 看每种错误的含义；`eu4-harmony game describe` 则向运行中的游戏询问。每项能力在游戏里如何工作，见 [docs/remote-control.md](docs/remote-control.md)。

## 写一个 Mod（开发者）

Mod 的扩展是 Lua 5.4 脚本，由框架在各自的沙箱里运行（[ADR 0024](docs/adr/0024-extensions-are-lua-in-a-sandbox.md)）：什么都不用装，也不用编译。命令行工具是游戏 `eu4-harmony\` 文件夹里的 `eu4-harmony.exe`（在本仓库的构建里是 `target\x86_64-pc-windows-msvc\release\`）：

```powershell
eu4-harmony new my-mod --name "My Mod"   # 生成 Mod：descriptor.mod、manifest.toml、lua\main.lua、一个按钮、中英文字、编辑器的类型定义、一个 AGENTS.md
cd my-mod
eu4-harmony test                          # 用假游戏运行它的 eu4-harmony\tests\*.lua
eu4-harmony link                          # 在用户文件夹的 mod\ 下注册，然后在启动器里勾选
eu4-harmony package                       # 所有 Lua 和本地化文件都能读通后，生成一个干净的上传文件夹 ..\my-mod-package
```

Mod 自己的文件都在它的 `eu4-harmony\` 文件夹里，放在哪里就说明是什么（[ADR 0029](docs/adr/0029-manifest-in-toml-layout-by-convention.md)）：`manifest.toml`（`id`、`api_version`、`description`、`permissions`），`lua\` 里是扩展、入口是 `main.lua`，`ui\<窗口名>\` 里是界面片段（按钮区的放 `ui\<窗口名>\buttons\`），`localisation\` 里是文字，`data\` 里是扩展要读的文件。

`permissions` 声明扩展在游戏之外能做什么：读取现实时间（`clock = true`）、在自己的数据文件夹里读写文件（`files = true`）、向列出的主机发 HTTP 请求（`network = ["api.example.com"]`）；用了没声明的就会报错（[ADR 0025](docs/adr/0025-sensitive-capabilities-are-declared.md)）。玩家在主菜单角落"EU4 Harmony Mod"按钮打开的 Mod 窗口里，能看到每个 Mod 的名称、版本、介绍和它声明的这些能力。

`main.lua` 返回一张事件函数表，每类事件一个（`on_session`、`on_day`、`on_button`、`on_frame`、`on_save` 等）。每个函数都拿到 `game`，所有能力都按组从它调用，例如 `game.army.move(army, province)`；被拒绝时返回 `nil, err`。事件函数可以等一个能力的答复（比如别国对提议的答复），写起来像 `await`。`eu4-harmony wiki` 列出全部能力和它们的 Lua 写法，`eu4-harmony wiki lua` 列出事件函数和全局函数，`eu4-harmony wiki manifest` 列出清单的字段和权限；`eu4-harmony game` 则通过远程控制（见上文）向运行中的游戏问同样的东西。最小的示例就是 `eu4-harmony new` 生成的 Mod；事件函数、出错、时间限制和扩展存档数据的设计见 [docs/lua-extensions.md](docs/lua-extensions.md)。

`eu4-harmony skills` 是制作 Mod 的指南，写给 coding agent，人也能读：每类工作的步骤，以及怎样算做完（先看 `eu4-harmony skills mod`，再看 `tasks`、`state`、`ui`、`localisation`、`testing`、`game`）。`eu4-harmony new` 生成的 `AGENTS.md` 会让 agent 去读这些指南。

装了 Lua 语言服务器（LuaLS）的编辑器能补全 `game`、各项能力和返回值的字段、事件函数和全局函数，并标出拼错的能力或传错的参数：`eu4-harmony new` 会把类型定义写进 Mod 的 `.eu4-harmony\types\`，并附一个 `.luarc.json`；换了新版工具后用 `eu4-harmony types` 重写一遍。`eu4-harmony package` 不会打包这两样。

文字放在 `eu4-harmony/localisation/<名字>_l_<语言>.yml`，格式同游戏的本地化文件，但内容是普通 UTF-8，每种语言一个文件（`english`、`french`、`german`、`spanish`、`simp_chinese`）。界面片段直接用这些键，扩展用 `text.get("KEY")` 取文字；框架按玩家语言显示，见 [ADR 0009](docs/adr/0009-localisation-in-the-player-language.md)。

## 仓库

| 目录 | 内容 |
| --- | --- |
| `proxy/` | `version.dll` 代理：转发 version.dll，只在 `eu4.exe` 里加载核心 |
| `core/` | 框架核心：依赖点与启动、框架读取的游戏对象布局（`core/src/objects/`）、`.gui` 拼接、在真实游戏这个宿主上加载扩展并调用它的事件函数（`core/src/lua/`）、能力，每组一个文件（`core/src/game/`）、远程控制的引擎（`core/src/remote/`）、界面绑定、扩展存档数据、玩家语言、框架设置、框架界面（`core/src/framework_ui/`）、Unicode 模块（`core/src/unicode/`）；`core/localisation/` 是框架自己的文字 |
| `lua/` | Lua 宿主：扩展的沙箱、事件函数和任务、出错和时间限制；宿主在核心里是运行中的游戏，在命令行工具里是假游戏 |
| `loc/` | 本地化文件的格式、语言列表和查找，核心和命令行工具共用 |
| `manifest/` | 模组的 `eu4-harmony/manifest.toml`、目录约定下其他文件的位置，以及描述文件的各行，核心和命令行工具共用 |
| `remote/` | 能力目录：远程控制的请求与响应、它们的 JSON 形式和每项能力的说明（含扩展是否也有），核心和命令行工具共用 |
| `checks/` | 框架自己对能力调用做的检查，以及每种拒绝的状态和措辞，核心和 `remote/` 共用；以及游戏的历法 |
| `cli/` | `eu4-harmony.exe`；`cli/template/` 是 `new` 生成的模组 |
| `tools/` | 开发工具：`dev-install.ps1` 把构建好的代理、核心和命令行工具装进游戏目录（`-Uninstall` 卸载），`release.ps1` 把它们打包给玩家，另有测试运行前后备份与还原玩家文件、输入、截图、反汇编、崩溃转储和调用追踪的工具，见 [tools/README.md](tools/README.md) |
| `docs/` | 方案、ADR、调研、实测记录和[框架对游戏性能的影响](docs/performance.md) |

代码、注释和文档都用英文。本文件是 [README.md](README.md) 的中文译本，其余文档只有英文版。在本仓库工作的约定（对人和编程代理都适用）见 [AGENTS.md](AGENTS.md)。
