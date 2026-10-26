# EU4 Harmony

让开发者以原生代码扩展《欧陆风云4》的框架，以及由它加载的 Mod。

## Language

**Mod**:
玩家在 Paradox 启动器中订阅和启用的单位，即 Paradox mod。可以只含游戏脚本和资源，也可以带一个扩展。
_Avoid_: 模组包

**扩展 (Extension)**:
Mod 中由框架加载、在游戏进程内运行的原生代码部分，由该 Mod 的清单声明。每个 Mod 至多一个。
_Avoid_: 插件、模块、DLL mod

**能力 (Capability)**:
框架向扩展提供的一项访问或改变游戏的功能，例如读取军队、下达移动命令、在面板上加按钮、按存档保存数据。扩展只能通过能力接触游戏。
_Avoid_: API、服务、钩子

**安全时点 (Safe point)**:
游戏主线程上可以安全读取和改变游戏状态的时刻。框架只在安全时点调用扩展。
_Avoid_: tick

**扩展存档数据 (Extension save data)**:
扩展随某个存档保存的自有数据，与该存档绑定，存放在存档文件之外。
_Avoid_: sidecar、配套文件

**清单 (Manifest)**:
Mod 中向框架声明其扩展的文件。
_Avoid_: 描述文件（指 Paradox 的 `descriptor.mod`）

**合集 (Playset)**:
Paradox 启动器中为下一次游戏启动启用的一组 Mod。
_Avoid_: 加载列表、mod 列表

**框架 (EU4 Harmony)**:
运行在游戏进程内、发现并加载扩展、向扩展提供能力的程序。
_Avoid_: 单独的 Harmony（易与其他同名项目混淆）、加载器、引擎

**快照 (Snapshot)**:
某个游戏对象在同一时刻的全部字段，扩展一次取回。
_Avoid_: 视图、引用

**按钮区 (Button area)**:
框架在原版面板上预留的一块区域，各扩展的按钮在其中自动排列。
_Avoid_: 工具栏、插槽

**底本 (Baseline)**:
框架支持的一个特定 `eu4.exe` 构建，以完整 SHA-256 标识。
_Avoid_: 版本、基线 exe
