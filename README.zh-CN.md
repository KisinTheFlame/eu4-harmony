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
| 每日 AI 运算期间，画面更少长时间停顿（[测量结果](docs/performance.md#why-submission-interval-p99-is-long)） | ✗ | ✓ |
| 存档时的卡顿只剩三分之一 | ✗ | ✓ |
| 启动和读档更快 | ✗ | ✓ |
| 游戏后期跑得更快 | ✗ | ✓ |
| 继承人不再一半概率重名 | ✗ | ✓ |
| 降低崩溃风险，包括[宗教改革中心的悬空引用](docs/game/provinces/reformation-centers.md) | ✗ | ✓ |
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

手动安装可从 [GitHub Releases](https://github.com/KisinTheFlame/eu4-harmony/releases) 下载 `eu4-harmony-<version>-windows-x64.zip`，关闭游戏后解压到游戏目录，和 `eu4.exe` 放在一起；原本有其他补丁的 `version.dll` 时，先把它挪走。这个包同时安装代理和核心。每次更新都需要自己下载并解压：本地核心优先于工坊核心，Steam 不会更新这份本地副本。要恢复工坊更新，删掉本地的 `eu4-harmony\eu4_harmony_core.dll`；要卸载手动安装的框架，删掉这个 DLL 和 `version.dll`。CLI 另有 `eu4-harmony-cli-<version>-windows-x64.zip`，可以放在游戏目录之外运行，玩家不需要安装它。

需要知道的几件事：

- 用 Harmony 玩的游戏里，你输入的名字以 UTF-8 保存，没有 Harmony 时会显示成乱码。
- 会保存自己数据的 Mod 把数据写进存档；这样的存档不保证在没有 Harmony 时还能读取。
- 非铁人成就要在新开局前启用。没有保存待达成成就列表的存档（包括受此前非铁人成就追踪缺陷影响的存档）需要重新开局；打开选项无法重建当初的开局条件。[成就追踪](docs/game/countries/achievements.md#each-achievements-script-conditions)说明了原因。
- 装上 Harmony 后，游戏自己的音量滑块按位置的平方变化，每一格听起来变化差不多大。原来低于 100 的音量会比以前轻，重新调一次即可。

## 出问题时

在主菜单打开 Harmony 的设置，进入**关于**，点**把排查问题用的文件存到桌面**：它把 Harmony 的日志、游戏的日志、现存的崩溃文件夹、你的设置和模组列表（不含存档）打成一个 zip 文件放到桌面上。到 [GitHub Issues](https://github.com/KisinTheFlame/eu4-harmony/issues) 反馈问题，附上这个文件。

日志在 `Documents\Paradox Interactive\Europa Universalis IV\eu4-harmony\logs\`：`report.txt` 是启动报告，`core.log` 记录框架工作，`proxy.log` 说明加载失败。[日志说明](docs/logs.md)介绍保留与收集范围。

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

命令行工具 `eu4-harmony.exe` 可从 [GitHub Releases](https://github.com/KisinTheFlame/eu4-harmony/releases) 单独下载。

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

Mod 就是一个普通的 Paradox Mod，多一个 `eu4-harmony/` 文件夹。处理函数通过 `game` 使用能力；敏感权限在清单中声明，玩家可以在 Harmony Mod 窗口里查看。布局、任务和存档数据的步骤见下方指南。

进一步了解：

- `eu4-harmony wiki`：每项能力的参数和结果、处理函数和全局函数（`wiki lua`）、清单（`wiki manifest`）。不需要游戏运行。
- `eu4-harmony skills`：每类工作的分步指南（`skills mod` 讲做一个 Mod，还有 `skills ui`、`skills testing` 等），写给 coding agent，人也能读。
- [cli/skills/mod.md](cli/skills/mod.md)：扩展如何工作，包括出错、时间与内存限制和存档数据。

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

观察模式下 AI 操控所有国家：`menu start` 不带国家就开这样一局，`session view FRA` 把法兰西设为所看国家，`session leave_view` 退出视角；这时以国家身份行事的能力作用于所看国家，AI 照常操控它，没有所看国家时返回 `no_country`（[docs/game/countries/countries.md](docs/game/countries/countries.md)）。

远程控制只对正在玩的 Windows 用户开放、只限本机。请求默认以玩家身份进行：只读玩家看得到的，只做玩家能做的。加上 `--as-game` 就以游戏身份进行：能读游戏保存的一切，还能像 Mod 的脚本那样改动游戏，做玩家做不到的事（[ADR requests-act-as-the-player-or-as-the-game](docs/adr/2026-11-11-requests-act-as-the-player-or-as-the-game.md)）。没有能力覆盖的地方，它可以用玩家的鼠标和键盘。`eu4-harmony wiki remote` 讲一局怎么进行、每个退出码的含义。

## 文档

[文档入口](docs/README.md)按用途指向能力参考、操作指南、游戏逻辑、开发工具和关键验证证据。[术语表](GLOSSARY.md)定义项目用语；[开放工作](docs/plans/README.md)只保留尚未完成的事项。

## 从源码构建

需要 Windows、[rustup](https://rustup.rs/)（它会按 [rust-toolchain.toml](rust-toolchain.toml) 选 Rust 版本）和 Visual Studio C++ 生成工具。只有运行框架和离线检查需要游戏。

```powershell
cargo build --release
cargo test
cargo xtask release        # 打出玩家包和创意工坊条目的文件夹
```

构建、发布和隔离测试游戏的步骤见[开发工具](docs/dev-tools.md)。测试游戏有自己的用户文件夹、核心和管道；只通过认领启动，不改动玩家的游戏。

## 参与开发

先读 [AGENTS.md](AGENTS.md)：代码、提交、检查和在游戏里测试的规则都在那里，对人和 coding agent 都适用。改动通过 pull request 进入 master。

## 许可证

[MIT](LICENSE)。它只涵盖本仓库自己的代码和文档，不涵盖游戏本身：Mod 能做什么，仍然以 Paradox 对 Mod 的条款为准。
