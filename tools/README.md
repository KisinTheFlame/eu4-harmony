# 开发工具

在运行中的游戏里试东西、找依赖点、看崩溃用的工具。都只在开发机上用，不进玩家的安装包。

| 工具 | 用途 |
| --- | --- |
| `dev-install.ps1` | 把构建好的代理和核心装进游戏目录（`-Uninstall` 卸载） |
| `game-input.ps1` | 把游戏窗口提到前台，发鼠标和键盘输入：`-Click x,y`、`-Drag x1,y1,x2,y2`、`-Wheel n -At x,y`、`-Type 文字`、`-Keys LEFT,SHIFT+END,CTRL+V` |
| `screenshot.ps1` | 截主屏：`-Out a.png`（整屏按 `-Scale` 缩小）或 `-Crop x,y,宽,高`（原尺寸） |
| `crash-dump.py` | 用系统自带的 dbgeng.dll 对崩溃转储执行调试命令，不用装调试器 |
| `core/examples/disasm.rs` | 读已安装游戏的代码：反汇编、取字节、数函数的参数个数 |
| `trace` 特性 | 记录指定游戏函数的调用：参数、参数指向的文字、调用处、返回值 |

坐标一律是物理像素：两个 PowerShell 脚本都按每显示器 DPI 感知运行，截图上的坐标可以直接拿去点。

## 注意

- **注入的按键不完全等于键盘。** `game-input.ps1` 发一次 Backspace，游戏输入框里会删掉不止一个字；不装框架的原版也一样。要数删了几个字，用真键盘。
- **测试会加载存档时先备份。** 游戏加载存档后会重写它。先把 `save games`、`eu4-harmony\saves`、`settings.txt`、`continue_game.json` 和 `eu4-harmony\settings.txt` 复制到别处，测完删掉新增的文件并还原。
- **崩溃转储**在文档目录 `Europa Universalis IV\crashes\eu4_<日期>_<时间>\minidump.dmp`；框架日志在 `eu4-harmony\logs\`。

## 反汇编

```
EU4_GAME_DIR=<安装目录> cargo run --release -p eu4-harmony-core --features analysis --example disasm -- 参数...
```

参数（数字是十六进制，可带 `0x`；地址都是 RVA）：

- `RVA`：从这里开始的整个函数，终点取自异常目录；目录里没有的叶函数到 `ret` 后的填充为止，最多 40 条；
- `RVA:N`：从 RVA 起 N 条指令，不必是函数开头；
- `bytes:RVA:N`：N 个字节的十六进制，格式同依赖点；
- `args:RVA`：这个函数读几个参数，也就是挂在它上面的观察点至少要转发几个。

同一套分析也做离线检查：`EU4_GAME_DIR=<安装目录> cargo test --release -p eu4-harmony-core -- --ignored` 核对所有依赖点的字节，并核对每个观察点转发的参数不少于函数实际读的。

## 调用追踪

```
cargo build --release --features eu4-harmony-core/trace
```

装上后，启动游戏前把 `EU4HARMONY_TRACE` 设成要追踪的函数 RVA，逗号分隔，例如 `0x15988e0,0x159c600`。每个函数的前 20 次调用（`EU4HARMONY_TRACE_CALLS` 可改）写进框架日志：各参数的值，参数指向文字时附上文字（C 字符串或 `std::string`，按游戏文字解码），调用处，返回值。

参数个数从函数代码里数出来。异常目录里没有的叶函数要写成 `RVA:N` 自己给个数；写少了，函数会读到垃圾，游戏可能崩溃。`trace` 特性只给开发用，玩家的构建里没有这部分代码。
