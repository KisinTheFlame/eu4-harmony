# EU4 Harmony

让开发者用 Rust 为《欧陆风云4》写原生 Mod 的框架。框架负责进入游戏进程、读取游戏状态、下达命令、往原版界面注入元素、按存档保存数据；Mod 的扩展只通过框架提供的能力接触游戏，不碰内存地址。设计见 [docs/v1.md](docs/v1.md)，术语见 [GLOSSARY.md](GLOSSARY.md)，决定的理由见 [docs/adr/](docs/adr/)。

支持的游戏：原版 1.37.5，以及与它布局相同的局部补丁 exe。仅单人。

## 安装框架（玩家）

把 `winmm.dll` 放进游戏根目录，把 `eu4-harmony\` 文件夹放进游戏根目录。根目录原本已有 `winmm.dll` 时，先把它移进 `eu4-harmony\`，框架会接着加载它。之后照常从 Paradox 启动器启动，在合集里启用基于框架的 Mod。

卸载：删掉根目录的 `winmm.dll` 和 `eu4-harmony\`，把移走的 `winmm.dll` 放回去。

日志、启动报告（`report.txt`）和扩展存档数据在 `Documents\Paradox Interactive\Europa Universalis IV\eu4-harmony\`。

## 写一个 Mod（开发者）

需要 Windows x64、Rust（MSVC 工具链）和 Visual Studio C++ 生成工具。先在本仓库构建：

```powershell
cargo build --release
```

命令行工具在 `target\x86_64-pc-windows-msvc\release\eu4-harmony.exe`：

```powershell
eu4-harmony new my-mod --name "My Mod"   # 生成 Mod：descriptor.mod、清单、一个按钮、Rust 扩展工程
cd my-mod
eu4-harmony build                         # 编译扩展，把 DLL 放到清单写的位置
eu4-harmony link                          # 在用户目录 mod\ 下登记，之后在启动器里勾选
eu4-harmony package                       # 生成上传用的干净目录 ..\my-mod-package
```

扩展实现 `eu4_harmony_sdk::Extension`，每类事件一个方法；所有能力都是回调参数 `Game` 上的方法。单元测试用 `eu4_harmony_sdk::testing::FakeGame` 代替游戏。示例：[examples/hello](examples/hello)，以及 eu4-regional-combat 仓库里的区域作战 Mod。

## 仓库

| 目录 | 内容 |
| --- | --- |
| `proxy/` | `winmm.dll` 代理：转发 winmm，只在 `eu4.exe` 里加载核心 |
| `core/` | 框架核心：底本校验、`.gui` 拼接、扩展加载、游戏能力、界面绑定、存档数据 |
| `abi/` | 核心与扩展之间的 C ABI |
| `sdk/` | 扩展用的 Rust SDK |
| `cli/` | `eu4-harmony.exe` |
| `examples/hello/` | 最小示例 Mod |
| `assets/` | 随核心一起放进游戏目录 `eu4-harmony\` 的文件（提示窗口的纹理，由 `tools/notice_textures.py` 生成） |
| `tools/dev-install.ps1` | 开发期把构建好的代理和核心装进游戏目录（`-Uninstall` 卸载） |
| `docs/` | 方案、ADR、调研、实测记录；`docs/research/regional-combat/` 是游戏能力所依据的逆向记录 |
