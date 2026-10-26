# regional-combat 的逆向发现

框架的游戏能力（读取、寻路与命令、界面绑定、地图着色、存档数据、事件）是从 eu4-regional-combat 的原生实现移植来的，这里保存它当时的逆向记录，作为这些地址和用法的出处。

- 写于 2026-10-26，针对 unrestricted 底本（`97dd63…`）。它与原版 1.37.5（`9ad3efe1…`）节布局相同，文中的 RVA 两者通用。
- 当时的实现是 Frida 脚本（`tools/*.js`）和后来的 Rust 启动器/DLL（`native/`）。文中提到的脚本、探针、`work/` 下的伪代码等，只存在于 eu4-regional-combat 2026-10-27 之前的 git 历史里，已不再维护。
- 框架代码里的 `Dependency` 表和注释引用这里的文件。改动框架里对应的游戏能力时，先读相关的那份。

| 文件 | 内容 | 框架里对应 |
| --- | --- | --- |
| `native-ui-findings.txt`、`native-settings-findings.txt` | 军队面板按钮绑定、点击信号、右键、显示隐藏、文字框 | `core/src/ui.rs` |
| `ui-polish-validation.md` | 悬停提示的输出结构（`0x1453b70`、`0x932f0`） | `core/src/ui.rs` |
| `native-range-preview-findings.txt` | 地图模式刷新与着色 | `core/src/map.rs` |
| `native-save-findings.txt` | 存档、读档流程 | `core/src/saves.rs` |
| `native-scheduler-findings.txt`、`native-fow-findings.txt` | 日更新、暂停、迷雾刷新 | `core/src/game/events.rs` |
| `native-manual-findings.txt`、`native-defeat-findings.txt` | 手动下令、战败 | `core/src/game/events.rs` |
| `native-path-findings.txt`、`native-order-adapter-contract.md` | 寻路、移动、停止 | `core/src/game/orders.rs` |
| `native-geography-findings.md`、`native-hostility-findings.txt`、`native-targets-findings.md`、`native-army-identity-findings.txt` | 省份邻接、海峡、敌对、可见军队、军队身份 | `core/src/game/world.rs` |
| `native-userdir-findings.txt` | `userdir.txt` 的格式 | `core/src/paths.rs` |
