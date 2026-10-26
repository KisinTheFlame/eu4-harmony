# 省份地理邻接、海峡和失陷判定

2026-10-26；静态底本 SHA256：`97dd63a811487fbe2c917f2c15b1e68f7222bef74a833889cf943c8010049d5e`。全部地址为 RVA，调用时须加已核验模块的实际基址。本次只读取冻结 exe、现有分析结果和地图文本，没有附加游戏进程，没有修改主源码。

地理邻接类型、陆地/不可通行省份标志和 owner/controller 偏移均已用原生解析或保存代码核实。范围距离应使用合格邻接边的等权 BFS，每条边计 1；原生边代价 `+0x0C` 以及要塞/控制区寻路规则不参与范围距离。访问权和当前海峡封锁仍是独立准入条件。

## 邻接记录

从当前 world 中有效 `Province* p` 出发：

```text
geometry = *(p + 0x28)
begin    = *(geometry + 0x168)
end      = *(geometry + 0x170)
count    = (end - begin) / 0x24
entry[i] = begin + i * 0x24
```

| 记录偏移 | 类型 | 已证实含义 |
| --- | --- | --- |
| `+0x00` | int32 | 目标省份 ID（To） |
| `+0x04` | int32 | 来源省份 ID（From） |
| `+0x08` | int32 | Through 省份 ID；海峡时为所经海域 |
| `+0x0C` | int32 | 原生移动代价基数；BFS 不使用 |
| `+0x18` | packed data | CSV 坐标相关值；本任务不解释或使用 |
| `+0x1C` | int32 | 邻接类型，见下表 |
| `+0x20` | byte | 创建时清零；完整语义未确认，不擅自命名 |

CSV loader `0x87FA10` 逐项读取 From、To、Type、Through；更新已有记录或为两端创建相反方向记录。创建时明确写 `entry[0]=To`、`entry[1]=From`、`entry[2]=Through`、`entry[7]=type`，并把 vector end 增加 `0x24`。现有行的更新也明确写 `+8` 和 `+0x1C`。`0x96F580` 独立按记录的 `+0`、`+4` 查询两岸省份 controller，与两个端点的定义相符。

| `+0x1C` | 原生 CSV 字面类型 | 范围图处理 |
| --- | --- | --- |
| 0 | 默认；所有未匹配的类型文本落入 0 | 两端必须为可通行陆地，保留普通邻接 |
| 1 | `sea` | 两端为陆地时是跨海峡边；通过当前海峡通行检查后保留 |
| 2 | `river` | 两端为可通行陆地时保留，计 1 步 |
| 3 | `impassable` | 排除 |
| 4 | `canal` | 排除本次陆军范围图 |
| 其他 | 本底本未确认 | 拒绝，不猜测为普通陆地 |

类型映射直接来自 `0x87FA10` 对字符串 `river`/`sea`/`impassable`/`canal` 的比较和赋值；不是根据地图外观推断。原生陆军边过滤 `0x96F860` 也直接拒绝类型 3。类型 4 走单独的控制国条件，不能混作海峡。

文件证据：`work/native-geography/pseudocode1.txt`；`work/native-path/pseudocode1.txt` 的 `0x96BD00` 读取 `+0x0C`；`work/native-path/pseudocode2.txt` 的 `0x96F860`；`work/native-geography/pseudocode2.txt` 的 `0x96F580`。CSV 中 `151;316;sea;1320`、`123;124;sea;1307` 可作为后续 runtime 数据交叉校验样例，不能代替当前 world 的邻接记录，尤其是随机新世界或地图 mod。

## 陆地与不可通行省份标志

```cpp
bool land = (read_u8(geometry + 0x228) & 0x08) != 0;
bool impassableProvince = (read_u8(geometry + 0x229) & 0x08) != 0;
bool eligibleGeography = land && !impassableProvince;
```

完整静态证据链：

1. `0x87B080` 地图配置 reader 的 token `0x28D7` 把省份集合读入 map manager `+0x440`；token `0x2AAA` 读入 `+0x470`。
2. 冻结 exe 的词元表分别保存 `0x28D7 + "sea_starts"` 和 `0x2AAA + "lakes"`。词元在字符串前 4 字节，证据地址分别 `0x2087570`、`0x20C23F0`。
3. 地图对象初始化 `0x87B9E0` 对每个 geometry 设置 `+0x228 bit0x10` 表示属于 lakes；只有既不在 lakes、也不在 sea_starts 的省份才设置 `+0x228 bit0x08`。因此 bit0x08 是陆地标志，不能仅检查“不是海峡”。
4. climate reader `0x886C00` 的 token `0x2D23` 将列出的省份 geometry `+0x229 bit0x08` 置位。词元表 `0x216B590` 是 `0x2D23 + "impassable"`。
5. 原生准入 `0xD447D0` 遇到该 bit 返回 false，并使用文本键 `IMPASSABLE_ACCESS`。`0x96F860` 在处理邻接类型之前也检查这个 bit。

文件证据：`work/native-geography/pseudocode2.txt`、`pseudocode5.txt`、`token-evidence.json`。不可通行省份标志和类型 3 的不可通行边是两个独立限制，应都检查。

## 通行权：可复用军队 HaveAccess，避开完整寻路

主陆军 vtable `0x1C42540` 的 `+0x120` 指向 `0x1A2280`。依据当前静态调用链，可把该入口用于省份准入；它没有走要塞控制区的路径验证，适合在地理过滤后维持军队合法通行条件。

```cpp
using ArmyHaveAccess = uint8_t (__fastcall *)(
    void* army, void* province, uint8_t debug, uint8_t p4,
    uint8_t p5, uint64_t optionalMilitaryTag, void* tooltip,
    uint8_t p8, void* accessCache);

// 与已观察原生平叛调用一致；AL != 0 为准入。
access(army, province, 0, 0, 0, 0x2D2D2D, nullptr, 0, nullptr);
```

`0x2D2D2D` 是已观察原生空 Tag 实参，不能把未知参数随意改成 bool 开关。`p4/p5/p8` 的所有高层命名未完全恢复，保留这个已观察组合。传入存活对象并采用真实 Win64 九参数调用约定，不能用少参数的 NativeFunction 定义。

静态路径如下：

- `0x1A2280` 处理陆地、特殊移动状态、发现等边界，并委派 `0xD447D0`。非陆地分支可询问运输船，故必须先用地理判据排除海/湖，不能把此函数当“无运输”的单独保证。
- `0xD447D0` 的原始日志字符串 `HaveAccess, %s, %d, %d, %d` 位于 `0x1D1A6F0`；它处理发现、impassable、普通所属国、相关国家与 `CONDOTTIERI_ACCESS`，后者字符串在 `0x1D1A730`。这提供函数目的的直接证据。
- `0xD44E00` 处理军队状态、发现以及国家关系；普通分支委派 `0x374C50`。
- `0x374C50` 基于目标 province owner/controller、土地/不可通行标志查询关系 `0x28B220`。其目标省份读取没有要塞、ZoC、邻接路径或原生移动代价。`0x28B220` 继续处理国家间通行、战争、附属/共享权限等；它不接收当前移动路径。
- 完整路径边过滤是另外的 `0x96F860` / `0x96FD60` 策略链，不能把它们的结果拿来裁剪本次“不考虑要塞”的地理范围。

更低层的六参数接口为：

```cpp
using CountryProvinceAccess = uint8_t (__fastcall *)(
    void* country, void* province, uint8_t isNavy, uint8_t debug,
    void* tooltip, void* accessCache); // RVA 0x374C50
// (country, province, 0, 0, nullptr, nullptr) 查询普通陆军国家通行权。
```

该低层函数缺少上层军队流亡/租借/探索等例外，不应替代军队 vslot 的语义。也不能把 `0x28B220` 单个关系结果直接当 province 准入，因为原生还比较 owner 与 controller。

这里的“查询”指不下达移动或改变任务，**并非绝对零内存写入**：`0x374C50` 在 cache=null 时会增加诊断计数 `base+0x2458E70`；上层可能初始化 TLS，并暂时写回日志状态。cache 非空还会写入该 cache，所以传 null。使用已确认原生线程安全时点，不能通过远程工作线程高频调用；在独立实验确认返回与原生界面一致前，不把静态分析声明为 runtime 验证。

证据：`work/native-hostility/pseudocode1.txt` 的 `0x1A2280`；`work/native-geography/pseudocode5.txt` 的 `0xD447D0`、`pseudocode6.txt` 的 `0xD44E00`/`0x28B220`、`pseudocode7.txt` 的 `0x374C50`。`pseudocode3.txt` 中的 `0x1154250` 是搜索过程中排除的无关 parser，不作为地理证据。

## 当前海峡封锁检查

```cpp
using IsStraitBlocked = uint8_t (__fastcall *)(
    uint64_t militaryCountryTag, // RCX，按值，不能传 Country* 或 Tag*
    const void* adjacencyEntry   // RDX，当前存活的 0x24 字节记录
);
// RVA 0x96F650；仅返回 AL 有效，true 表示封锁，false 表示此检查未阻塞。
```

该函数的已观察调用者先通过 army vslot `+0x138` 把有效军队国家 Tag 写到临时缓冲，再把该 qword **按值**传入。不要无条件把玩家 tag 替代军队的有效军事身份，特别是佣兵/租借等特殊状态。

`0x96F650` 只在类型 1、Tag 有效、Through 非零时进入舰队检查。先调用 `0x96F580(tag, entry)`：两端 controller 都是此 tag，或都通过 `0x374A70(controllerCountry, tag, 0x2D2D2D)` 的关系判定时，跳过封锁。然后解析 Through 海域，遍历 `province+0x350` 单位链表，检查 vslot `+0x88`（舰队）、敌对关系、关联国家例外、单位状态 `+0x203` 和非空 regiment 数量。发现合格舰队返回 AL=1。

不能把所有类型 1 记录无条件当作“具有非零 Through 的陆地海峡”。地理过滤先排除海/湖/海岸组合，只有最终准备保留的 land-land type1 需要海峡校验。Through=0 是此原生函数的明确未封锁分支：`0x96F691` 读 `[entry+8]`，`0x96F695` TEST，`0x96F697` 跳到 `0x96F7AC` 的 `XOR AL,AL`。helper 对此仍调用原生 predicate；对非零 Through 要求它索引有效海域，异常报错包含 From/To/type/Through 与海域 land/lake 标志。类型未知或端点坏记录仍拒绝全图。

这条链没有读取要塞或控制区，也没有执行移动。静态语义是只读关系/单位查询；实际调用仍应在已确认的游戏线程稳定时点，先保证 entry、端点、Through 和 country tag 的边界与生命周期。`0x96F580` / `0x374A70` 的完整返回条件不能简化成仅“己方控制两岸”。

入口前缀（16 字节，不能替代完整文件 hash）：

```text
0x96F650: 48 89 5c 24 10 48 89 6c 24 18 48 89 74 24 20 48
0x96F580: 48 89 5c 24 08 57 48 83 ec 20 48 8b 05 e7 08 9d
```

证据：`work/native-path/pseudocode3.txt` 含 `0x96F650` 逐条机器指令与反编译；`work/native-geography/pseudocode2.txt` 的 `0x96F580`；`pseudocode6.txt` 的 `0x374A70`。

## 自有失陷省份

| Province 偏移 | 类型 | 静态身份 |
| --- | --- | --- |
| `+0x468` | CountryTag qword | owner |
| `+0x470` | CountryTag qword | controller |

直接保存证据：writer `0x9D6E60` 从 `province+0x468` 取国家名称，向存档写 token `0x2841`；从 `province+0x470` 取国家名称，写 token `0x2842`。冻结 exe 词元表 `0x20744C0` 为 `0x2841 + "owner"`，`0x20746C8` 为 `0x2842 + "controller"`。独立 UI 函数 `0x1229CF0` 将 `+0x470` 对应国家名称填入 `CONTROLLER` 占位，方向一致。

基础条件是 owner 为玩家、controller 有效且不是玩家；最终“敌对占领”还须 controller 与玩家合法敌对。盟友/其他友方控制不能仅因 controller 不等于 owner 就成为敌对收复目标。仅使用 owner==player，因此不扩展到属国或盟友领土。CountryTag 比较在本底本通常使用 `int16(tag >> 32)` 的国家索引，需先验证完整 Tag/当前国家对象仍有效；不能只看三个字符、裸指针或未经校验的数组索引。

证据：`work/native-geography/pseudocode4.txt`、`pseudocode1.txt`、`token-evidence.json`。本报告没有从 `+0x388` 等未命名字段猜测占领状态；是否正在围城是另一任务状态，不能替代 owner/controller 判定。

## 图构建及验证边界

每次图 snapshot 绑定 world/session 和军队有效身份。begin/end 必须有序、相差为 `0x24` 整数倍且邻接数受省份总数限制；目标和 Through 均按当前省份 ID 索引表验证，不把旧指针保留到读档以后。记录 `From` 应匹配源省份 ID，发现异常则拒绝该 snapshot，不静默指向省份 0。

保留 type 0/1/2 且两端地理合格的边，再叠加已确认的合法通行权和 type 1 海峡条件；权重固定 1。不要调用含要塞规则的整条原生寻路来裁剪范围，也不要使用移动天数、像素直线距离或 `entry+0x0C` 作距离。真实移动目标仍必须交给原生寻路验证，范围图不能赋予穿越要塞的实际移动权限。

当前底本的普通陆军 native 调用已由下述隔离实验验证；随机新世界实例、地图 mod 和所有特殊军队身份仍未覆盖。静态 parser 对普通文本未知类型回退 0，是本底本既有行为；适配器只能消费已装载的记录，不能凭空再解释原始未知类型。当前记录出现未确认枚举则拒绝。邻接记录 `+0x20` 的完整意义仍未知。

## 独立地理读取 helper

`tools/native_geography.js` 仅定义 `createGeographyReader(base)`，没有 attach、hook 或 RPC。调用方先验证完整 exe hash，然后在已确认的 UI/game-thread pump 内同步调用：

```js
const geography = createGeographyReader(Process.getModuleByName('eu4.exe').base);
const graph = geography.read({ panel, uiThreadId });
// 或 geography.read({ army, uiThreadId })，二者只能指定一个。
```

`uiThreadId` 必须是调用方从真实 UI 回调确认的线程 ID，不应以任意工作线程的当前 ID 冒充。helper 验证实际线程与该 ID 相同。每次从全局重新取 world、country manager、地图及军队指针；不跨 read 缓存对象或原生内存区间。read 期间以“最近区间 + 排序区间二分”缓存已确认可读内存页所属区间，降低 `Process.findRangeByAddress` 调用次数；finally 清空。它不保护调用方选择错误时点导致的 native 生命周期竞争。

主线程第 5 次隔离实验明确记录：8 字节请求地址与 outTag/returnedTag 一致，原生写入的完整 Tag 为 `0x10000B900534143`、index=185，但 `Process.findRangeByAddress` 返回 null。该地址属于 Frida 自己的 `Memory.alloc(8)`，并非游戏 country 数组。helper 因而仅对其明确持有的初始/结束两块 Tag 输出缓冲使用 direct read + try/catch；所有游戏来源指针仍保留原来的可读范围验证、有效 Tag 和 country 对象交叉核对。证据由主线程记录于 `work/native-integration/geographic-preview-05.jsonl`，不是本子任务自行 attach。

军事 Tag 的额外证据：vslot `+0x138` 的 `0x1A8D20(army, outTag)` 总是返回 outTag。普通分支复制 `army+0xD0`；特殊分支调用 `0x1A8B70`，后者在状态允许时复制 `army+0x220`，否则输出原生无效 Tag `0x2D2D2D`。helper 拒绝无效结果并验证当前 country 对象，再按值传给 `0x96F650`；读取结束再次获取、比较该 Tag。证据：`work/native-geography/pseudocode8.txt`。

```text
0x1A2280: 48 89 5c 24 08 48 89 6c 24 10 56 57 41 56 48 83
0x1A8D20: 48 89 5c 24 08 57 48 83 ec 20 48 8b da 48 8b f9
0x1A8B70: 48 89 5c 24 18 56 48 83 ec 20 80 3d bb 73 19 02
0xD447D0: 48 89 5c 24 10 48 89 74 24 18 48 89 7c 24 20 55
0xD44E00: 48 89 5c 24 08 48 89 6c 24 10 48 89 74 24 18 48
0x28B220: 44 88 4c 24 20 48 89 54 24 10 48 89 4c 24 08 55
```

返回包含 armyId（十六进制字符串，避免 JS 64 位 ID 精度丢失）、origin、day、world、player、militaryTag、provinces、range_edges 和 count。provinces 是全部合格陆地省份，通行权不允许的省份仍有节点但没有连接；range_edges 方向与当前原生记录一致，不凭空补反向边。每个节点提供普通 core snapshot 的 id、owned_by_player、occupied_by_hostile 字段；后者固定 false，因为本 helper 不查询敌对控制或敌军。

这只是预览图：`geography_complete:true`、`targets_complete:false`、`orders_permitted:false`。没有通用 `complete:true`，没有敌军、路线或发令状态。调用方不得将它冒充完整战斗 snapshot，不能用它下达命令。evidence 记录这些范围、占领占位值、调用线程和原生查询可能更新诊断计数的边界。

count 记录省份/陆地/准入/邻接/保留边数量、nativeAccessCalls、nativeStraitCalls、nativeTagCalls、readableCalls、regionQueries 与 durationMs。最多 50,000 省份、1,000,000 原始邻接；检查指针跨度、ID/index、geometry ID、所有邻接枚举/端点，以及准备保留的 land-land type1 的非零 Through；任何异常拒绝整个结果。当前选择必须为玩家拥有的已确认主陆军 vtable。省份编号索引 0 必须存在且结构有效，但不会成为可选陆地节点。这个严格假定需要 runtime 验证；遇到不兼容地图时应分析证据，不静默忽略坏记录。

read 上限保持 5,000 ms；超过时抛异常，绝不返回部分图。所有 read 异常包含 phase 与 metrics；readable 失败还记录请求 address/size、失败 cursor 以及实际 region 的 base/size/protection。通过 reader.status() 还能取得上次成功或失败的统计。图读取起始与末尾要求 `world.byte(+0x1E10)==0`，末尾同时确认 world、玩家、日期、军队 ID/位置/拥有者/军事 Tag、地图 vector、选中对象和邻接存储未变化。仅线程正确不足以证明稳定边界：`0x758430` 在日更新入口设该字节为 1，正常返回前清零；scheduler 实测存在 dailyUpdating=1 时的嵌套 manager 帧。证据见 `docs/native-scheduler-findings.txt` 和其 `scheduler-trace-01.jsonl` 运行补充。

本子任务完成的验证：`node --check tools/native_geography.js`；`node tests/test_native_geography.cjs` 的 15 个合成测试通过，覆盖地理过滤、均匀有向图、准入拒绝、海峡有效军事 Tag 与封锁、未知类型/坏跨度/错误线程拒绝、世界变化拒绝、带阶段的超时、内存区间缓存不跨 read、type1 海岸/Through=0/非法非零 Through、不可读游戏内存地址诊断、Frida 自分配缓冲对 range API 不可见时的正确读取，以及起始已在日更新/读取期间进入日更新的拒绝。合成测试和以下游戏实验分别记录，不把合成结果冒充游戏内验收。

## 地理预览运行验证通过

主线程运行的 `work/native-integration/geographic-preview-06.jsonl` 已核对。隔离程序 PID 43680，完整文件 hash 与本报告底本一致，读取军队 ID `0x19D300000036`、当前位置 149；所有预览及恢复操作在线程 23700 完成。

| 完整地理读取指标 | 实测 |
| --- | ---: |
| 原始省份 | 4942 |
| 合格陆地节点 | 4027 |
| 原始邻接记录 | 23136 |
| 原生军队准入查询 | 4027 |
| 允许准入节点 | 467 |
| 原生海峡查询 | 24 |
| 有效范围有向边 | 2072 |
| 系统内存区间查询 | 40 |
| 单次完整读取耗时 | 456 ms |

宿主 C++ core 用这份真实地理图、中心省份 149 计算半径 3/5/7，分别得到 28/66/107 个省份。日志的 `preview`、`colored` 和 `enabled` 记录的集合、半径及中心一致；主线程报告截图显示蓝色范围、金色中心。结束记录 `core-remained-disabled=true`，原地图模式 1 恢复成功、原生刷新完成，随后确认 dispose、unload 和 detach 完成。整个实验没有启用自动战斗或下达移动命令。

这确认当前普通陆军场景下“原生地理数据 → 原生合法通行/海峡查询 → core 等权范围 → 游戏地图预览”贯通。日更新起末拒绝检查是在本次成功实验后按 scheduler 证据补入，已通过合成测试；该新增拒绝分支本身尚未再次做游戏内触发实验。敌军/失陷目标完整快照、自动命令、特殊军队身份与其他地图不属于这次通过的范围。
