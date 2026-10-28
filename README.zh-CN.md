# EU4 Harmony

[English](README.md) | 简体中文

让开发者用 Rust 为《欧陆风云4》写原生 Mod 的框架。框架负责进入游戏进程、读取游戏状态、下达命令、往原版界面注入元素、按存档保存数据；Mod 的扩展只通过框架提供的能力接触游戏，不碰内存地址。设计见 [docs/v1.md](docs/v1.md)，术语见 [GLOSSARY.md](GLOSSARY.md)，决定的理由见 [docs/adr/](docs/adr/)（这些文档都是英文）。

支持的游戏：原版 1.37.5，以及与它布局相同的局部补丁 exe。仅单人。

## 安装框架（玩家）

把 `version.dll` 和 `eu4-harmony\` 文件夹放进游戏根目录。根目录原本已有 `version.dll`（通常是中文字体插件，即双字节补丁的加载器）时，先把它移进 `eu4-harmony\backup\`；框架不会加载它，`plugins\` 里这个插件的文件也不再被加载，可以删掉。框架自己负责显示中文和其他文字（[ADR 0010](docs/adr/0010-built-in-unicode-module.md)）。之后照常从 Paradox 启动器启动，在合集里启用基于框架的 Mod。

卸载：删掉根目录的 `version.dll` 和 `eu4-harmony\`，把备份的 `version.dll` 放回根目录。

用框架玩出的存档里，玩家输入或改过的名字是 UTF-8；卸掉框架、换回中文字体插件后，这些名字会显示为乱码。

日志、启动报告（`report.txt`）和扩展存档数据在 `Documents\Paradox Interactive\Europa Universalis IV\eu4-harmony\`。

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

扩展实现 `eu4_harmony_sdk::Extension`，每类事件一个方法；所有能力都是回调参数 `Game` 上的方法。单元测试用 `eu4_harmony_sdk::testing::FakeGame` 代替游戏。最小的示例就是 `eu4-harmony new` 生成的 Mod；完整的例子是 eu4-regional-combat 仓库里的区域作战 Mod。

文字写在 `eu4-harmony/localisation/<名字>_l_<语言>.yml`，格式同游戏的本地化文件，内容是普通 UTF-8，一个语言一份（`english`、`french`、`german`、`spanish`、`simp_chinese`）。界面片段直接引用其中的键，扩展用 `game.text("KEY")` 取文字；框架按玩家语言显示，见 [ADR 0009](docs/adr/0009-localisation-in-the-player-language.md)。

## 仓库

| 目录 | 内容 |
| --- | --- |
| `proxy/` | `version.dll` 代理：转发 version.dll，只在 `eu4.exe` 里加载核心 |
| `core/` | 框架核心：依赖点与启动、`.gui` 拼接、扩展加载、游戏能力、界面绑定、扩展存档数据、玩家语言、框架设置、框架界面（`core/src/framework_ui/`）、Unicode 模块（`core/src/unicode/`）；`core/localisation/` 是框架自己的文字 |
| `loc/` | 本地化文件的格式、语言列表和查找，核心、SDK 和命令行工具共用 |
| `abi/` | 核心与扩展之间的 C ABI |
| `sdk/` | 扩展用的 Rust SDK |
| `cli/` | `eu4-harmony.exe` |
| `tools/` | 开发工具：`dev-install.ps1` 把构建好的代理和核心装进游戏目录（`-Uninstall` 卸载），另有输入、截图、反汇编、崩溃转储和调用追踪，见 [tools/README.md](tools/README.md) |
| `docs/` | 方案、ADR、调研、实测记录；`docs/research/regional-combat/` 是游戏能力所依据的逆向记录 |

代码、注释和文档都用英文。本文件是 [README.md](README.md) 的中文译本，其余文档只有英文版。在本仓库工作的约定（对人和编程代理都适用）见 [AGENTS.md](AGENTS.md)。
