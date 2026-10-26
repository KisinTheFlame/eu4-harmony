# 向原版窗口运行时注入界面元素：静态可行性研究

日期 2026-10-27。对象：原版 1.37.5 `eu4.exe`（SHA-256 `9ad3efe1af169f40ee577f9dae5debbc87af6fb8b5450fb345ebf110dc4d771a`，映像基址 `0x140000000`）。下文地址都是 **RVA**。

方法：只做静态分析，没有启动游戏或附加进程。用 Ghidra 12.1.4 headless 导入时不跑自动分析（`-noanalysis`），只对选定函数做反编译；字符串、RIP 相对引用、`E8` 直接调用、`.pdata` 函数边界由自写脚本扫描。导入、导出表由 REA 读取（Evidence `ev_0faa7943…`、`ev_2afa8877…`）。文中标 **已验证** 的，是在反编译或字节里直接看到的；标 **推断** 的，是根据结构作出的判断。反编译结果是伪代码，函数原型都是暂定。

## 问题

ADR 0006 要求界面元素在运行时注入、Mod 不覆盖原版 `.gui`。这里要回答两件事：能否在运行时往原版窗口里加元素（首个目标是 `interface/unitpanel_main.gui` 里的 `single_unitpanel`）；三种候选机制中哪种最可行。

## 结论

**可行。推荐机制 1（读文件时拼接文本），把握高。**

- 游戏读 `.gui` 时，整个文件经 PhysFS 一次读进内存缓冲区，然后才交给词法分析器。中间没有缓存或二进制格式，拼接点干净。
- PhysFS 静态链接在 eu4.exe 里，并且 **按名字导出**（`PHYSFS_openRead` 等 107 个函数，版本 3.0.1）。挂钩点可以用 `GetProcAddress` 按名字找到，不依赖 RVA。
- `PHYSFS_openRead` 按搜索路径解析出实际生效的那份文件（Mod 目录以前插方式挂载），所以拼接打在"最终胜出的文件"上，能与其他覆盖同一文件的 Mod 共存。
- 控件由游戏自己的解析和实例化流程创建，生命周期、热重载（`reload` 命令）都沿用原版行为。点击绑定继续用 regional-combat 已在运行时验证过的方式（`root+0x68` 按名查找、`button+0xC8` 信号订阅）。

机制 2（解析后合并定义）和机制 3（运行时实例化）所需的函数都找到了，但都要直接操作游戏内部 C++ 对象布局和所有权，适配面大得多，只作备选。

## GUI 加载链路（已验证）

1. **启动入口**：入口 `0x1a337c8` → `0x1a33654` → `0x1a78630` → `0x8757e0`。之后按某个设置（`puVar17[0xb]`）分两路：
   - 在主线程上直接调用 `0x2003d0`；
   - 或经 `0x17320a0(0x874010, "LoadingThread", app)` 起一个 **LoadingThread** 线程，由它调用 `0x200810`。

   两路最终都进入 `0x202850`（`eu4application.cpp` 中的应用初始化）。
2. **`0x202850` 加载 GUI 定义**：
   - 先加载 `.gfx`，然后显示 `LOADING_GUI_DEF`。
   - 在 `app+0x358` 新建 GUI 管理器（0x550 字节，EU4 子类构造 `0x10b3bd0`，基类构造 `0x14e8640`）。**基类构造把单例写进全局 `0x23494f0`**。
   - 用 VFS 枚举 `0x170ffe0(dir, &vec, ".gui", 0)` 列出界面目录下全部 `.gui`（目录字符串运行时构造，推断是 `interface`）。
   - 逐个文件处理：
     - `0x170dd70(&lexFile, path)` 打开文件；
     - `0x1709570(&lexer, &lexFile, "UNNAMED_SESSION", 0)` 建词法器；
     - `mgr->vtbl[+0x18](mgr, &lexer)` 读入（通用持久化读取 `0x17068b0`）；
     - 读完写日志 `Loaded GUI Definition File: `。
3. **CLexer 文件构造 `0x170cd10`**（`clausewitz/pdx_core/lexer.cpp`，共 109 个直接调用点，游戏的脚本文本都从这里读）：
   - `0x1718300(path, 0, 0x20000)` 打开文件。它是 `PHYSFS_openRead` 唯一的直接调用者，并在其后调用 `PHYSFS_setBuffer(f, 0x20000)`，返回新分配的 `{PHYSFS_File*, bool}`（0x10 字节）。打开过程处于全局自旋锁 `*0x242b9c0` 内。
   - `vec = new std::vector<char>`（0x18 字节）。
   - `len = PHYSFS_fileLength(f)`（调用点 `0x170d079`），`0x416ef0(vec, len)` 调整大小。
   - `PHYSFS_read(f, vec.data, 1, (u32)len)`（调用点 `0x170d0b3`），**不检查返回值**。
   - `0x171ebc0(stream, 0, 0, vec, 1, 0, 0, 1)`（调用点 `0x170d0ee`）把 vector 包成 0x58 字节的内存流（vtable `0x1dc0358`，`+0x38` 存 vector 指针），路径字符串移入 `stream+0x18`。
   - `PHYSFS_close`（`0x170d15e`）。
   - 打开失败时写日志 `CLexer failed to load file `。

   **之后词法分析只读内存，不再碰文件。**
4. **guitypes 块**：
   - 管理器的 token 处理函数 `vtbl[+0x20] = 0x14ea9f0`。遇到 token `0x7e`（`guitypes`）时调用 `0x14ece90(lexer, mgr+0xC8, mgr+0x10)`。
   - token 编号查自静态关键字表：`guitypes`=0x7e、`guiButtonType`=0x7f、`windowType`=0x9b、`positionType`=0xb2。
   - `0x14ece90` 对每个元素：
     - 调用工厂 `(mgr+0x10)->vtbl[+0x10] = 0x14eac80(factory, lexer, tokenId)` 创建 **原型对象**，例如 `guiButtonType` 分配 0x3f0 字节、构造 `0x15b17e0`，`windowType` 分配 0x508 字节、构造 `0x155b8e0`；
     - 原型用 `vtbl[+0x18]` 读入自身，名字取自 `vtbl[+0x40]`；
     - 重名时：若覆盖标志 `mgr+0x228` 为 0，报 `Already Defined. Key: …` 并丢弃新的，即先加载的生效；否则删除旧原型。
     - 最后批量插入类型库，并调用 `+0x20` 完成整理。
5. **类型库 `mgr+0xC8`**（vtable `0x1d8dc00`）：不区分大小写的三叉搜索树。
   - `+0x08` 按 `std::string*` 查找：`0x14ec630`，它再转到 `+0x10` 按 `const char*` 查找 `0x6e4880`，返回原型指针或 0；
   - `+0x30` 插入：`0x14ec950`；
   - `+0x20` 整理：`0x14ec750`。
6. **实例化**：管理器 `vtbl[+0x48] = 0x14e9030` 即 CreateWindow(name)（`clausewitzlib/graphics/gui.cpp`）。
   - 先在类型库查找原型；找不到时报 `Failed to create window "%s"` 并返回 0。
   - 找到则调用 `0x155c260(proto, *(mgr+0xC0), mgr, …)`：分配 0xa80 字节，构造 `0x1543f80`，按原型的子元素表逐个克隆子控件。
   - 最后调用 `mgr->vtbl[+0x98]`（`0x14eb360`）登记。

   军队面板的根窗口就由它创建：`0x144fb00` 里 `panel+0x38 = mgr->vtbl[+0x48](mgr, "single_unitpanel")`，其中 `mgr = *(*(panel+0x48)+0x58)`。
7. **热重载**：
   - 控制台 `reload <file>`：`0x14e16e0` → `0x14e1140` → `0x14eb7d0(*0x23494f0, path)`，后者重新建 CLexer、调用管理器 `vtbl[+0x18]` 读入。
   - 面板构造 `0x144def0` 调用 `0x14e1f60(panel+0x10, proto+0x30)` → `0x14e0d90`，把面板登记进全局表 `*0x242bf58`。推断这张表以原型的来源文件为键，供重载后通知面板重建（对应已知的 `0x1450580`）。

## 机制 1：读文件时拼接文本 —— 推荐

**结论：可行，把握高。** 拼接点就在"整文件读入缓冲区"与"词法分析"之间，是纯数据层面的变换。

### 挂钩方案 A（推荐）：挂 PhysFS 导出函数

eu4.exe 导出了 107 个 `PHYSFS_*` 函数（REA `ev_2afa8877…`）。`PHYSFS_getLinkedVersion` 写死的版本号是 3.0.1。游戏内部调用 PhysFS 用的是 `E8` 直接调用，不经导入表，所以要做 inline hook；入口地址用 `GetProcAddress(GetModuleHandle(NULL), "PHYSFS_…")` 取得。

| 导出 | RVA | 用途 |
|---|---|---|
| `PHYSFS_openRead` | `0x19f7f60` | 唯一直接调用者是 `0x1718300`，所有文件打开都经过这里 |
| `PHYSFS_fileLength` | `0x19f6ec0` | 词法器用它决定缓冲区大小 |
| `PHYSFS_read` | `0x19f8150` | 已弃用的包装，内部在 `0x19f8160` 调用 `readBytes` |
| `PHYSFS_readBytes` | `0x19f8180` | 挂这一个就同时覆盖 `PHYSFS_read` |
| `PHYSFS_seek` / `tell` / `eof` | `0x19f8310` / `0x19f8c20` / `0x19f6e40` | 词法器不用，为安全起见一并模拟 |
| `PHYSFS_close` | `0x19f66c0` | 释放拼接缓冲区 |
| `PHYSFS_setBuffer` | `0x19f8450` | `0x1718300` 打开后会调用；直接透传给真实句柄即可 |
| `PHYSFS_mount` | `0x19f7cb0` | VFS 包装 `0x1714580` 调用时 `appendToPath=0`（前插），后挂载的目录优先 |

做法：

- `openRead` 先调用原函数；若规范化后的路径（不分大小写、统一分隔符）命中目标文件，就用原 `fileLength` / `readBytes` 读出全文，生成拼接结果，以句柄为键保存。
- 之后该句柄上的 `fileLength`、`readBytes`、`seek`、`tell`、`eof` 都从拼接缓冲区返回；`close` 时释放缓冲区并调用原 `close`。

所有权与线程：

- 游戏按我们报告的长度自行分配 vector，我们只往里拷字节。拼接缓冲区始终归框架所有，不需要动游戏的分配器。
- 钩子可能运行在主线程，也可能运行在 LoadingThread（见链路第 1 步）。`readBytes` 等调用不在游戏的锁内，所以钩子必须线程安全。
- 拼接所需的输入必须在 `LOADING_GUI_DEF` 之前准备好，钩子里不能回调扩展代码。这与 v1 的"扩展只在主线程安全时点被调用"一致：扩展的界面声明要么由清单给出，要么在扩展初始化时登记，由框架预先算好。

### 挂钩方案 B（备选）：只作用于词法器

- 前置钩子挂在 `0x170cd10`，记下路径（第 2 个参数，MSVC `std::string*`，结构为 SSO 16 字节 + `size@+0x10` + `cap@+0x18`）。
- 再改道 `0x170d079`（`fileLength`）和 `0x170d0b3`（`PHYSFS_read`）这两个调用点；或者挂 `0x171ebc0`，在 vector 交给内存流之前改写它（需要用游戏自己的 `0x416ef0` 调整大小）。
- 影响面更窄，但依赖 3 个内部 RVA 和 vector 的布局。

### 其他已确认的性质

- **兼容其他 Mod**：
  - 路径由 PhysFS 搜索路径解析，拼接作用于胜出的那份文件。挂载参数（前插）已验证，搜索顺序语义来自 PhysFS 文档；"Mod 晚于本体挂载"属推断。
  - 若别的 Mod 把 `single_unitpanel` 改名或删掉，拼接器找不到目标，应跳过并报告。
- **校验和**：原版 `checksum_manifest.txt` 的 13 个条目都不包含 `interface/`。推断改写 `.gui` 不影响铁人模式和成就的校验和。
- **热重载**：`reload` 命令和 `reloadinterface`（`0xe590d0`）都经 CLexer 重新读文件，拼接会自动重新生效。
- **重名**：同一窗口内子元素重名时，`0x155e3d0` 报 `Guitype '%s' already exists in windowtype '%s'`，后者覆盖前者。框架应给注入的元素名加命名空间，例如 `harmony_<扩展>_<id>`。
- **拼接语法**：在顶层 `guiTypes` 下找到 `name = "single_unitpanel"` 的 `windowType` 块，在它的右花括号前插入元素块。框架需要一个能识别引号字符串和 `#` 注释的小型花括号扫描器。原版文件是无 BOM 的 UTF-8。
- **图形资源**：按钮要用的 sprite 由扩展 Mod 自带新的 `.gfx` 文件提供。新文件不会与其他 Mod 冲突。

## 机制 2：解析后合并定义 —— 可行但脆弱

所需的结构都已找到（已验证）：

- `windowType` 原型（0x508 字节，vtable `0x1d94478`）在 `+0xE8` 有自己的子元素库，结构与管理器的类型库相同。
- 原型的 token 处理函数 `vtbl[+0x20] = 0x155cb60(proto, lexer, tokenId)` 遇到子元素 token 时创建子原型，再调用 `0x155e3d0(proto, child, lexer, addToOrder)`：子原型读入自身、插入 `+0xE8` 子元素库，并把名字加入 `+0x498` 的顺序名表。各类型另有名字表，例如按钮在 `+0x180`。

做法有两种：

- 在 `unitpanel_main.gui` 的 guitypes 读完后，用游戏的内存流 `0x171ebc0` 和词法器 `0x1709570` 把我们的文本包成词法器，逐个元素调用 `proto->vtbl[+0x20]`；
- 或者把 Mod 自带"占位窗口"的子原型挪进 `single_unitpanel`。

问题：

- 依赖词法器和内存流的对象布局（每个 token 缓冲区 0x200 字节、token 编号在 `lexer+0x30` 等）、游戏分配器和多张名字表；
- 原型会在重载时被删除重建（覆盖标志打开时），每次加载或重载后都要重新应用；
- 必须赶在面板实例化之前完成。

它的效果与机制 1 相同，只是 ABI 面大得多。**不推荐**；仅当文件层钩子出于某种原因不能用时才考虑。

## 机制 3：运行时实例化 —— 工厂存在，挂接困难

已验证存在的工厂，都在 GUI 管理器上，单例为 `*0x23494f0`：

| vtable 槽位 | 函数 | 作用 |
|---|---|---|
| `+0x40` | `0x14e8fb0` | 按名字克隆任意类型：调用原型的 `vtbl[+0x70]`，然后登记 |
| `+0x48` | `0x14e9030` | CreateWindow |
| `+0x58` | `0x14e9160` | 创建 `containerWindowType`（0x264） |
| `+0x60` | `0x14e93b0` | 创建 `gridBoxType`（0x275） |

扩展 Mod 只要自带新的 `.gui` 文件定义一个唯一名字的窗口，就能用这些工厂按名字创建。

难点在于"挂到活动面板下面"：

- 活动窗口（0xa80 字节，构造 `0x1543f80`）把子控件放在按类型分开的内部 vector 里（例如 `+0x738` 存指针，`+0x750` 存名字），由构造函数内联填充；
- 子控件还要经 `child->vtbl[+0xe8]` 设置父对象，再经管理器 `vtbl[+0x98]` 登记；
- 没有找到独立的 AddChild 函数。构造中每建一个子控件后调用的 `0x1546e70`，只是 `fixedwindow` 的越界检查（`Illegal object boundaries…`）。

所以要挂接，就得直接写活动窗口的内部 vector，并承担渲染、输入遍历和析构所有权上的未知风险。**不推荐用于注入子元素。**

变体：可以用 CreateWindow 创建框架自有的顶层浮动窗口，每帧跟随军队面板定位，销毁仿照面板析构的做法（`0x6ecc80(mgr, root)` 再调 `root->vtbl[+0x130]`，见 regional-combat 的静态结论）。这适合"贴在面板旁"的独立小窗，不是真正注入。

## 推荐

1. 采用机制 1 的方案 A：挂 PhysFS 导出函数（`openRead`、`fileLength`、`readBytes`、`seek`、`tell`、`eof`、`close`），对白名单路径做文本拼接。初版只开放 `single_unitpanel`。
2. 拼接后按名字绑定行为，沿用 regional-combat 已在运行时验证的方式：`root=*(panel+0x38)`，`root->vtbl[+0x68](name)`，`button+0xC8` 订阅，右键需调用 `vtbl[+0x170]`。
3. 方案 B 作为同一机制的降级实现。机制 3 的浮动窗口变体留作以后的能力。

## 需要运行时验证

1. GUI 定义究竟在哪个线程加载（主线程还是 LoadingThread，取决于 `0x8757e0` 的分支条件）。不论结果如何，钩子都要线程安全。
2. 目标文件传给 `PHYSFS_openRead` 的路径格式（大小写、分隔符、有无前导目录）。需要记录全部 `*.gui` 的 open 调用，确认没有第二条读取路径或缓存。
3. 另装一个覆盖 `unitpanel_main.gui` 的 Mod，用 `PHYSFS_getRealDir` 确认拼接作用在胜出的文件上，并且两边改动同时可见。
4. 拼接后的文本能被正常解析：注入的按钮出现在军队面板中，`error.log` 里没有 `Unexpected token`、`Already Defined`、`Guitype … already exists`。
5. 框架是否能在 `LOADING_GUI_DEF` 之前拿到所有扩展的界面声明（winmm 代理初始化、扩展加载与 GUI 加载之间的时序）。
6. 控制台 `reload interface/unitpanel_main.gui` 和 `reloadinterface` 之后拼接仍然生效，绑定能随面板重建（`0x1450580` 路径）正确重挂。
7. 与其他同样 inline hook PhysFS 的原生插件共存：钩子链和原字节校验。
8. 内联挂钩前校验各导出函数的入口字节；本文只覆盖原版 `9ad3efe1…`，unrestricted 底本需要另行比对。
