![Harmony for Europa Universalis IV](docs/images/banner.jpg)

[![CI](https://github.com/KisinTheFlame/eu4-harmony/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/KisinTheFlame/eu4-harmony/actions/workflows/ci.yml)

[English](README.md) | 简体中文 | [日本語](README.ja.md) | [한국어](README.ko.md)

**从内部把《欧陆风云4》变得更好。** 清晰锐利的中日韩字体、返回主菜单免重启、后期更流畅、修复多项引擎缺陷，并让 Mod 拥有更强扩展能力，同时保持原版游戏规则不变。

> 快速跳转：[面向玩家](#面向玩家) · [面向 Mod 开发者](#面向-mod-开发者) · [面向贡献者](#面向贡献者)

---

## 面向玩家

### 与原版《欧陆风云4》对比

| | 原版游戏 | 使用 Harmony 后 |
| --- | :-: | :-: |
| 清晰显示中/日/韩文字，支持系统字体与简中/日/韩免费内置默认字体及实时预览（无需双字节补丁或位图字体 Mod） | ✗ | ✓ |
| 几秒内返回主菜单，无需重启游戏 | ✗ | ✓ |
| 铁人模式下支持直接局内读档 | ✗ | ✓ |
| 即使 5 速运行，地图拖动与缩放依然流畅，显著减少卡顿 | ✗ | ✓ |
| 存档时的画面停顿缩短至约三分之一 | ✗ | ✓ |
| 游戏启动、读档与后期跑图整体提速 | ✗ | ✓ |
| 修复继承人高频重名、反复读档内存泄漏及宗教改革中心闪退 | ✗ | ✓ |
| 配乐响度均衡，音量滑块调节更贴合人耳听感 | ✗ | ✓ |
| 可选放宽成就条件（非铁人或加载 Mod 时获得成就） | ✗ | ✓ |
| Mod 可提供原生风格界面与辅助自动化，并透明展示权限 | ✗ | ✓ |
| **游戏规则与原版数值完全不变** | ✓ | ✓ |

游戏的中文文本仍来自你订阅的汉化 Mod，照常启用即可；为旧双字节补丁制作的汉化 Mod 以及对应存档均可直接使用。Harmony 支持 **Steam 平台的 Windows 版 EU4 1.37.5**，目前仅支持单人游戏（多人游戏支持正在路上）。

### 安装步骤

**重要提示：**
- **光点订阅不会生效。** 订阅后需把 1 个文件（`version.dll`）复制到游戏目录一次——使用下方一行命令或纯鼠标 3 步均可。
- **杀毒误报说明：** Harmony 以 MIT 协议完全开源，因通过 `version.dll` 加载进 `eu4.exe`，偶发触发启发式警告。你可随时查阅源码、将 EU4 目录加入杀毒排除项，或自行从源码编译。

请先在 Steam 创意工坊订阅 **Harmony** 并等下载完成，再任选一种方式复制 `version.dll`：

#### 方法 A：一行 PowerShell 命令自动复制（最快，可选）

按 **Win + X** → 打开 **终端**（或 **Windows PowerShell**），粘贴下方整行命令并回车。该命令仅在本机查找 Steam 库目录、将已有 `version.dll` 自动备份为 `version.dll.bak` 并复制新文件，不联网下载任何内容：

```powershell
$s=(gp HKCU:\Software\Valve\Steam).SteamPath; $libs=@($s)+((sls '"path"\s+"([^"]+)"' "$s\steamapps\libraryfolders.vdf" -EA 0|%{$_.Matches[0].Groups[1].Value -replace '\\\\','\'})|?{$_}); $g=$libs|%{"$_\steamapps\common\Europa Universalis IV"}|?{Test-Path "$_\eu4.exe"}|select -First 1; $src=$libs|%{"$_\steamapps\workshop\content\236850\3837775887\version.dll"}|?{Test-Path $_}|select -First 1; if(!$g -or !$src){throw "Not found (subscribe in Steam first): game='$g' workshop='$src'"}; $dst="$g\version.dll"; if((Test-Path $dst) -and !(Test-Path "$dst.bak") -and (Get-FileHash $dst).Hash -ne (Get-FileHash $src).Hash){Copy-Item $dst "$dst.bak"}; Copy-Item $src $dst -Force; "Installed to $dst"
```

#### 方法 B：纯鼠标手动复制（3 步）

1. 在 Steam 里右键《欧陆风云4》→ **管理 → 浏览本地文件**，打开游戏目录。
2. 打开 Paradox 启动器 → **Mod 库** → 找到 **Harmony** → 点右侧 **… → 在文件夹中显示**（也可在 Steam 库直接打开 `steamapps\workshop\content\236850\3837775887\`）。
3. 将 `version.dll` 复制到游戏目录，和 `eu4.exe` 放在一起。（Windows 默认可能隐藏后缀，显示为 `version` 和 `eu4`。若已有旧版 `version.dll` 可直接替换，或先改名备份。）

照常从 Paradox 启动器启动游戏即可。Harmony 不需要放进合集；基于 Harmony 的 Mod 照常在合集里启用。生效后，主菜单与局内菜单右下角会出现 **Harmony v…**、**Mod 列表** 和 **设置**。

Steam 会自动保持 Harmony 最新。当更新包含新版 `version.dll` 时，Harmony 会在启动时自动更新游戏目录里的副本并为你重启游戏（仅在游戏目录只读时才会在游戏内提示手动复制）。

#### 手动安装（GitHub Releases）

也可从 [GitHub Releases](https://github.com/KisinTheFlame/eu4-harmony/releases) 下载 `eu4-harmony-<version>-windows-x64.zip`，关闭游戏后解压到游戏目录（与 `eu4.exe` 同级）。本地核心优先于工坊核心，因此 Steam 不会自动更新手动安装版；如需切回工坊自动更新，删掉游戏目录 `eu4-harmony\` 下的 `eu4_harmony_core.dll`、`eu4_harmony_crash_dumps.dll` 和 `fonts\` 即可。普通玩家不需要单独下载命令行工具压缩包（`eu4-harmony-cli-*.zip`）。

#### 卸载

取消订阅 Harmony 并删除游戏目录里的 `version.dll`（如有 `version.dll.bak` 可改名恢复）。如之前在 **Harmony 设置 → 高级** 中启用过 Windows 闪退转储，建议先在那里点击 **关闭 EU4 的 Windows 闪退转储（需管理员权限）**。

### 常见问题（FAQ）

- **我订阅了，为什么进游戏看不到 Harmony？**
  光点订阅不会加载 Harmony。请确认已按上方方法 A 或 B 将 `version.dll` 复制到 `eu4.exe` 同目录，并确认游戏是 Steam Windows 版 1.37.5。
- **杀毒软件把 `version.dll` 或 `eu4_harmony_core.dll` 报毒了，安全吗？**
  Harmony 以 MIT 协议完全开源，因通过 DLL 加载进游戏进程实现功能，偶发触发启发式误报。你可以查阅源码、将游戏目录加入杀毒排除项，或自行从源码编译。
- **之前用过双字节补丁或位图字体 Mod，怎么迁移？**
  用 Harmony 的 `version.dll` 替换旧文件，**关闭旧位图字体 Mod**（不再需要且会影响系统字体渲染），保留汉化文本 Mod 即可。旧存档可直接读取。
- **支持多人联机吗？**
  Harmony 的多人游戏支持正在路上。目前仅支持单人游戏，且主菜单的多人游戏入口会自动禁用。如需联机，请取消订阅 Harmony 或暂时把游戏目录中的 `version.dll` 移走。
- **以后卸载 Harmony，存档还能用吗？**
  仅使用 Harmony 本体不会破坏存档结构，但在游戏中手动输入的中日韩自定义名称按 UTF-8 保存，卸载后这些自定义名称会显示为乱码。若使用了向存档写入自有数据的 Harmony Mod，则不保证无 Harmony 时仍可读取。
- **为什么游戏音量变轻了？为什么开了非铁人成就后旧档没生效？**
  Harmony 将音量滑块改为更符合人耳听觉的平方曲线，原本低于 100 的刻度听起来会偏轻，调高一次即可。非铁人成就需要在开新局前启用，旧档缺少开局追踪列表，无法事后补建开局条件（[原因说明](docs/game/countries/achievements.md#each-achievements-script-conditions)）。

### 故障排查

在主菜单或局内菜单打开 **Harmony 设置 → 关于**，点击 **把排查问题用的文件存到桌面**，它会把 Harmony 日志、白名单游戏日志（不含账号/联机/聊天日志）、最近几次崩溃记录（转储文件会在内存中自动瘦身并脱敏模块路径）、设置和模组列表（不含存档，自动脱敏用户名路径与玩家标识，总大小严格控制在 24 MiB 以内）打包成桌面上的一个 zip。点击「关于」页上的 GitHub Issues 链接并附上该压缩包即可反馈。

你也可以直接在 **Harmony 设置 → 高级** 中点击 **打开日志文件夹**（`Documents\Paradox Interactive\Europa Universalis IV\eu4-harmony\logs\`），详见 [docs/logs.md](docs/logs.md)。

---

## 面向 Mod 开发者

### 编写 Harmony Mod

基于 Harmony 的 Mod 是一个普通的 Paradox Mod，多一个 `eu4-harmony/` 文件夹。其中的 Lua 扩展通过能力接口读取游戏并执行与玩家等价的操作，绝不接触底层内存地址：

```lua
-- eu4-harmony/lua/main.lua：在 eu4-harmony/ui/ 中声明的按钮，
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

从 [GitHub Releases](https://github.com/KisinTheFlame/eu4-harmony/releases) 单独下载命令行工具 `eu4-harmony.exe`：

```powershell
eu4-harmony new my-mod --name "My Mod"   # 生成可直接运行的模板 Mod（含界面、多语言与编辑器类型定义）
cd my-mod
eu4-harmony check                         # 按框架规则静态检查全部文件
eu4-harmony test                          # 用内置假游戏运行 eu4-harmony\tests\*.lua
eu4-harmony link                          # 注册到 Paradox 启动器，然后在合集里启用
eu4-harmony reload --watch                # 游戏运行时监听文件变动并热重载扩展
eu4-harmony types                         # 工具更新后重新生成 LuaLS 编辑器类型定义
eu4-harmony package                       # 打包可直接上传工坊的干净目录
```

敏感权限（`clock`、`files`、`network`）须在 `manifest.toml` 中声明，并会展示在 **Harmony Mod** 窗口供玩家审查。

延伸资料：

- `eu4-harmony wiki`：完整能力参考、钩子与全局函数（`wiki lua`）、清单格式（`wiki manifest`），无需游戏运行。
- `eu4-harmony skills`：各类任务的分步操作指南（`skills mod`、`skills ui`、`skills testing` 等），人和 coding agent 均可直接阅读。
- [cli/skills/mod.md](cli/skills/mod.md)：扩展生命周期、错误处理、时间/内存配额与存档数据。

### 远程控制与自动化

程序或 AI 可通过 `eu4-harmony game <能力> [<参数>]` 像玩家一样操作游戏（甚至后台隐藏运行），输出为 JSON：

```powershell
eu4-harmony start --hidden                 # 后台隐藏启动（需 Steam 运行）
eu4-harmony game menu start FRA            # 以法兰西开新局
eu4-harmony game time advance 30           # 推进 30 天（遇事件提前停下）
eu4-harmony game question list             # 查看等待回答的事件/弹窗
eu4-harmony game army move 545460846646 185
eu4-harmony game screenshot --out shot.png
eu4-harmony game menu return               # 返回主菜单
```

观察模式下（不带国家执行 `menu start`）可用 `session view FRA` / `session leave_view` 切换视角（[docs/game/countries/countries.md](docs/game/countries/countries.md)）。远程控制默认关闭：通过 `eu4-harmony start` 启动会自动为该次运行启用，也可在 **Harmony 设置 → 高级** 中开启 **启用远程控制** 并重启游戏。开启后仅对当前 Windows 用户在本机开放；加 `--as-game` 可切换为游戏视角权限（[ADR requests-act-as-the-player-or-as-the-game](docs/adr/2026-11-11-requests-act-as-the-player-or-as-the-game.md)）。详见 `eu4-harmony wiki remote`。

---

## 面向贡献者

### 文档导航

从 [docs/README.md](docs/README.md) 进入游戏内部逻辑、开发工具、ADR 和研究证据。[GLOSSARY.md](GLOSSARY.md) 定义项目统一术语，[docs/plans/README.md](docs/plans/README.md) 记录尚待完成的工作。

### 从源码构建

需要 Windows 10 (1903+) x64、[rustup](https://rustup.rs/)（自动读取 [rust-toolchain.toml](rust-toolchain.toml)）和 Visual Studio C++ 生成工具。只有运行框架、离线依赖点检查和隔离测试游戏需要安装 EU4 1.37.5。

```powershell
cargo build --release
cargo test
cargo xtask offline-check          # 需要 EU4 1.37.5（通过 Steam 或 EU4_GAME_DIR 查找）
cargo xtask release --no-workshop  # 打包 GitHub 发布 ZIP，不依赖 SteamCMD
```

本地运行隔离测试游戏与场景测试（需保持 Steam 运行且拥有 EU4 1.37.5 全 DLC）：

```powershell
cargo xtask install                # 首次关闭游戏后执行一次，安装可加载测试核心的 version.dll
cargo xtask saves fetch            # 首次执行一次，下载并校验共享测试存档
cargo xtask scenario sweep --owner <name>
```

构建、发布、工坊上传和隔离测试游戏规范见 [docs/dev-tools.md](docs/dev-tools.md)。参与贡献前请先阅读 [AGENTS.md](AGENTS.md)，所有改动通过 Pull Request 合入 `master`。

### 许可证

[MIT](LICENSE)。仅涵盖本仓库自身的代码与文档，不涵盖游戏本体；Mod 开发仍须遵守 Paradox 的 Mod 条款。
