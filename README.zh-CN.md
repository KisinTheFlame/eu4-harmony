# EU4 Harmony

[English](README.md) | 简体中文

让开发者用 Rust 为《欧陆风云4》写原生 Mod 的框架。框架负责进入游戏进程、读取游戏状态、下达命令、往原版界面注入元素、按存档保存数据；Mod 的扩展只通过框架提供的能力接触游戏，不碰内存地址。设计见 [docs/v1.md](docs/v1.md)，术语见 [GLOSSARY.md](GLOSSARY.md)，决定的理由见 [docs/adr/](docs/adr/)（这些文档都是英文）。

支持的游戏：原版 1.37.5，以及与它布局相同的局部补丁 exe。仅单人。

## 安装框架（玩家）

框架以一个文件夹（或它的 zip）提供，里面是 `version.dll` 和 `eu4-harmony\` 文件夹，后者装着核心（`eu4_harmony_core.dll`）和命令行工具（`eu4-harmony.exe`）；`tools\release.ps1` 从 release 构建生成这两者。把 `version.dll` 和 `eu4-harmony\` 文件夹放进游戏根目录。根目录原本已有 `version.dll`（通常是中文字体插件，即双字节补丁的加载器）时，先把它移进 `eu4-harmony\backup\`；框架不会加载它，`plugins\` 里这个插件的文件也不再被加载，可以删掉。框架自己负责显示中文和其他文字（[ADR 0010](docs/adr/0010-built-in-unicode-module.md)）。之后照常从 Paradox 启动器启动，在合集里启用基于框架的 Mod。

卸载：删掉根目录的 `version.dll`；有备份的 `version.dll` 时，把它从 `eu4-harmony\backup\` 移回根目录；最后删掉 `eu4-harmony\`。

用框架玩出的存档里，玩家输入或改过的名字是 UTF-8；卸掉框架、换回中文字体插件后，这些名字会显示为乱码。

日志和启动报告（`report.txt`）在 `Documents\Paradox Interactive\Europa Universalis IV\eu4-harmony\`。扩展存档数据直接写在存档里；用会保存数据的扩展玩出的存档，不保证在没有框架时还能读取。

## 远程控制（玩家与 AI）

远程控制让游戏进程之外的程序通过框架的能力使用游戏，就像玩家一样：从主菜单进入游戏再回来，读取游戏，让时间流逝，回答游戏的事件和其他国家的提议，调动军队，招募，外交，游戏控制台，镜头和截图。AI 靠它玩完整的一局，开发者靠它在命令行里试用框架。框架运行时它始终开启，只对正在玩的 Windows 用户开放、只限本机（一个命名管道），并且以玩家身份行事：玩家做不到的，它也做不到（[ADR 0015](docs/adr/0015-remote-control-uses-capabilities.md)、[ADR 0017](docs/adr/0017-remote-control-is-always-on.md)）。

客户端是游戏 `eu4-harmony\` 文件夹里的 `eu4-harmony.exe`。`eu4-harmony game <能力> [<参数>]` 向运行中的游戏发一个请求，以 JSON 打印回答；退出码为 0 表示游戏回答了，1 表示游戏回答了一个错误（`{"code": ..., "message": ...}`，拒绝时带着游戏自己的理由），2 表示命令行有误、什么也没发出，3 表示没有收到回答。参数可以带名字（`--province 167`），也可以按顺序直接写；能力名里的点可以写成空格：

```powershell
cd "<游戏目录>\eu4-harmony"
.\eu4-harmony game help                    # 一局怎么进行、每种错误的含义、全部能力
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
.\eu4-harmony game menu return             # 回到主菜单：游戏会重新启动
```

按组列出的能力：

| 组 | 能力 |
| --- | --- |
| 主菜单 | `menu`（游戏在哪里）、`menu.saves`、`menu.load`、`menu.start`、`menu.return` |
| 会话与时间 | `session`、`time`、`time.pause`、`time.resume`、`time.speed`、`time.advance`（让若干天过去并等待，带回途中的事件） |
| 国家与省份 | `country.get`（国库、人力、稳定度、威望、君主点数、战争、盟友、附属国与宗主国及附属类型、宿敌）、`country.hostile`、`province.list`、`province.get`（名字、所有者、发展度、地形、要塞、建筑、此地能招募什么）、`province.neighbors` |
| 军队 | `army.list`、`army.get`、`army.in_province`、`army.route`、`army.find_route`、`army.can_enter`、`army.strait_open`、`army.can_attack`、`army.move`、`army.stop`、`army.selected` |
| 招募 | `recruit`（在一个省份招募某一类的一个团或一艘船） |
| 事件与提议 | `question.list`、`question.answer`（等待玩家回答的游戏事件）；`offer.list`、`offer.accept`、`offer.decline`（其他国家的提议） |
| 外交 | `diplomacy.war_goals`、`diplomacy.declare_war`（召集盟友参战）、`diplomacy.offer_peace`（白和平）、`diplomacy.propose_alliance` |
| 游戏控制台 | `console.run`（像玩家那样输入一条命令，带回它的输出） |
| 镜头与截图 | `camera`、`camera.look_at_province`、`camera.look_at`、`camera.zoom`；`screenshot`（玩家看到的画面，PNG） |
| 事件流与说明 | `events`（编号的事件：新会话、新的一天、军队被下令或被击败）、`describe`（运行中的游戏给出的全部能力）、`text.language` |

完整的列表，连同每个参数和结果，见 `eu4-harmony game help`（不需要游戏运行），单个能力用 `eu4-harmony game help <能力>`；`eu4-harmony game describe` 则向运行中的游戏询问。每项能力在游戏里如何工作，见 [docs/remote-control.md](docs/remote-control.md)。

## 写一个 Mod（开发者）

需要 Windows x64、Rust（MSVC 工具链）和 Visual Studio C++ 生成工具。先在本仓库构建：

```powershell
cargo build --release
```

命令行工具在 `target\x86_64-pc-windows-msvc\release\eu4-harmony.exe`：

```powershell
eu4-harmony new my-mod --name "My Mod"   # 生成 Mod：descriptor.mod、清单、一个按钮、中英文字、Rust 扩展工程
cd my-mod
eu4-harmony build                         # 编译扩展，把 DLL 放到清单写的位置
eu4-harmony link                          # 在用户目录 mod\ 下登记，之后在启动器里勾选
eu4-harmony package                       # 生成上传用的干净目录 ..\my-mod-package
```

游戏运行时，`eu4-harmony game` 通过远程控制（见上文）向游戏发请求：`eu4-harmony game session` 读出日期和玩家的国家，`eu4-harmony game help` 列出全部能力。

扩展实现 `eu4_harmony_sdk::Extension`，每类事件一个方法；所有能力都是回调参数 `Game` 上的方法。单元测试用 `eu4_harmony_sdk::testing::FakeGame` 代替游戏。最小的示例就是 `eu4-harmony new` 生成的 Mod；完整的例子是 eu4-regional-combat 仓库里的区域作战 Mod。

文字写在 `eu4-harmony/localisation/<名字>_l_<语言>.yml`，格式同游戏的本地化文件，内容是普通 UTF-8，一个语言一份（`english`、`french`、`german`、`spanish`、`simp_chinese`）。界面片段直接引用其中的键，扩展用 `game.text("KEY")` 取文字；框架按玩家语言显示，见 [ADR 0009](docs/adr/0009-localisation-in-the-player-language.md)。

## 仓库

| 目录 | 内容 |
| --- | --- |
| `proxy/` | `version.dll` 代理：转发 version.dll，只在 `eu4.exe` 里加载核心 |
| `core/` | 框架核心：依赖点与启动、框架读取的游戏对象布局（`core/src/objects/`）、`.gui` 拼接、扩展加载、能力，每组一个文件（`core/src/game/`）、远程控制的引擎（`core/src/remote/`）、界面绑定、扩展存档数据、玩家语言、框架设置、框架界面（`core/src/framework_ui/`）、Unicode 模块（`core/src/unicode/`）；`core/localisation/` 是框架自己的文字 |
| `loc/` | 本地化文件的格式、语言列表和查找，核心、SDK 和命令行工具共用 |
| `manifest/` | 模组的 `eu4-harmony/manifest.json` 和描述文件的各行，核心和命令行工具共用 |
| `remote/` | 远程控制的请求与响应、它们的 JSON 形式和每项能力的说明，核心和命令行工具共用 |
| `abi/` | 核心与扩展之间的 C ABI |
| `sdk/` | 扩展用的 Rust SDK |
| `cli/` | `eu4-harmony.exe`；`cli/template/` 是 `new` 生成的模组，其扩展随其余部分一起构建和测试 |
| `tools/` | 开发工具：`dev-install.ps1` 把构建好的代理、核心和命令行工具装进游戏目录（`-Uninstall` 卸载），`release.ps1` 把它们打包给玩家，另有测试运行前后备份与还原玩家文件、输入、截图、反汇编、崩溃转储和调用追踪的工具，见 [tools/README.md](tools/README.md) |
| `docs/` | 方案、ADR、调研、实测记录和[框架对游戏性能的影响](docs/performance.md)；`docs/research/regional-combat/` 是游戏能力所依据的逆向记录 |

代码、注释和文档都用英文。本文件是 [README.md](README.md) 的中文译本，其余文档只有英文版。在本仓库工作的约定（对人和编程代理都适用）见 [AGENTS.md](AGENTS.md)。
