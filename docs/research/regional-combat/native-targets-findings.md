# 当前可见敌军与本国敌对占领目标

2026-10-26，批准底本 SHA256 `97dd63a811487fbe2c917f2c15b1e68f7222bef74a833889cf943c8010049d5e`。所有地址为 RVA。本子任务只读取冻结 exe、已有静态和运行记录，未 attach 游戏；helper 尚未游戏内验收。

`tools/native_targets.js` 定义独立的 `createTargetReader(base)`。它不 hook、不提供 RPC、不下令、不修改存档或地图，也不扫描迷雾内单位。与地理读取 helper 分开，调用方负责在同一稳定 world/军队身份下组合数据，取得原生路线和最终发令许可。

## 调用和 FOW 新鲜度契约

```js
const reader = createTargetReader(base);
const result = reader.read({
  army,                       // 当前有效的主陆军指针
  uiThreadId,                 // 已确认原生游戏/UI线程 ID
  sessionId: 'campaign/load-1',
  fow: currentCertificate,    // 本次开始时的证书副本
  getFowState() { return currentCertificate; }
});
```

FOW 证书包含以下字段，缺少任一项或不匹配即返回 `targets_complete:false`、空 provinces/enemies；不会开始枚举军队。

```js
{
  source: 'native-0x8275e0-onLeave',
  confirmed: true,
  inProgress: false,
  fresh: true,
  epoch: 1,                   // 已完成的原生刷新序号，正安全整数
  revision: 0,                // 跟踪器失效/变更序号，非负安全整数
  sessionId: 'campaign/load-1',
  world: '0x...',             // 与当前 NativePointer.toString() 一致
  player: '0x10000b900534143', // 完整有效玩家 Tag，小写十六进制
  day: 56679720              // 当前 world+0x1DD0 的原始 int32 值
}
```

这是调用方对真实观察结果的声明，helper 不会伪造“已安装 FOW hook”。提供者必须在加载/卸载/切国、刷新开始、未纳入已完成刷新证据的 FOW setter 或其他相关可见性变更时失效证书并增加 revision；日推进后没有同一天完成的刷新也不能继续沿用旧证书。sessionId 在读档时必须变化，即使日期和 world 地址重复。只有由真实 `0x8275E0` 正常完成事件、正确 world/tag/day 和稳定时点闭合的结果才能重新设置 fresh。

helper 在开始时同时核对 `input.fow` 和 `getFowState()`，末尾再取一次 live provider，比较复制保存的完整证书，不保留调用方可变对象引用作为旧值。仅保存一次历史 epoch 但不跟踪后续失效，不满足契约。原来的 `native_fow_observer.js` 观察结果可以提供部分证据，但它本身没有这里所需的 day/session/revision/fresh 契约，不能直接把旧 lastCompleted 对象标成完整证书。

入口、出口均要求实际线程匹配且 `world.byte(+0x1E10)==0`；线程正确本身不能排除日更新期间的嵌套 manager 帧。整个读取还检查 world、玩家、原始日期、country 数组、地图 vector、军队 ID/归属/位置和各可见省份单位列表稳定。所有对象与可读内存区间缓存仅活在单次同步 read 中。

## 可见性先于单位枚举

直接使用已经静态及被动运行验证的缓存：

```cpp
hidden = (province.byte(+0x959) & 0x20) &&
         (province.byte(+0x958) & 0x02);
```

只枚举当前合格陆地的可见省份。必须先计算 hidden，确认 false 后才读取 `province+0x350` 单位列表头。隐藏省份不会读取列表头、节点、军队位置、军队 ID、国家、兵力或其它军队字段；没有“最后可见位置”缓存，也不导出隐藏单位数量。普通地图 owner 位可以用于筛选本国领土，但隐藏省份的 controller 不被读取。若证书声称新鲜而本国省份仍 hidden，则拒绝整份目标结果，不以 false 冒充已确认未失陷。

列表节点 payload `+0`、next `+0x10`。每个可见单位必须仍以当前省份为 `army+0xE8`；列表有界并检测循环/重复。主陆军 vtable `0x1C42540` 才作为陆军处理，已知舰队 vtable `0x1CCA7E0` 跳过，其它可见单位类型拒绝当前结果，不猜测。这里只保留在陆地省份的陆军，运输或海域中的单位不作为陆地作战目标。

FOW 位、身份、列表或证书在结束复核时改变，整份结果丢弃；失败返回中没有已收集敌军、目标省份或其部分数量。`status()` 也只保留失败原因/阶段，不存放上次敌军身份。

## 敌军合法敌对及状态过滤

唯一原生调用为已恢复的关系谓词：

```cpp
uint8_t (__fastcall *HostileArmy)(Army* own, Army* candidate); // RVA 0xD54A60
```

使用 Win64 RCX/RDX 两指针、AL 布尔返回。保留原生参数方向。该函数覆盖普通战争表、特殊国家/叛军、关联国家和租借军事身份例外。不能仅按 owner 不同判断敌军。入口 16 字节为：

```text
48 89 5c 24 08 48 89 74 24 10 57 48 83 ec 20 80
```

它可能初始化原生线程 TLS，但不会下达移动或修改任务。参见 `docs/native-hostility-findings.txt`、`work/native-integration/pseudocode1.txt` 的 `0xD54A60`。完整 exe hash 是前置条件，入口前缀只能辅助验证。

候选还必须有非空 regiment 指针 vector（`army+0x30 .. +0x38`，stride 8），不是 `+0x203` 或 `+0x204` 非零的撤退状态，也不是原生流亡判据。这里只用“非空”判断对象仍为有部队的军队，完全不比较兵数或战力。

流亡判据来自此次定点恢复 `0x99CA0`：先确认陆军 vslot+0x80；有效 owner Tag 且 kind 不等于 7、国家索引不等于三个特殊索引时，返回 `army.byte(+0x207)`；其余返回 AL=0。调用者 `0x1A2280` 对 true 走独立通行分支，字符串 RVA `0x1C41D98` 从冻结 exe 直接解析为 `EXILED_ACCESS`，语义闭合。helper 对已经确认的主陆军用同样的字段逻辑，无需再调用另一个原生函数。

原始证据：`work/native-targets/pseudocode1.txt`（`0x99CA0..0x99D07`）。同时分析的 `0xD40AE0` 是军队/国家 modifier 标志查询，会访问更多对象；它不作为流亡或敌对谓词使用。撤退双字节证据见 `docs/native-scheduler-findings.txt`。

普通玩家必须是有效可控制 Tag，index>0、kind!=7；敌军/控制国不继承这个玩家限制。后者允许非负 signed index，包括 0，并验证当前 country 数组槽和 country+0x20 的完整 Tag 一致。kind 1 等特殊类别不会被排除。三个特殊国家索引从当前全局 `0x233D8A4 / 0x233D8D8 / 0x233D8DC` 读取并记录于成功结果 `evidence.specialCountryIndices`，不硬编码具体数值或擅自命名另外两个特殊国家。

受控军队正在撤退或流亡时 `attacker_eligible:false`，不输出可攻击敌军。完整军队状态、交战过程及路径/控制区限制仍属于外层快照和原生命令验证，不能把这里的 `legally_attackable:true` 解释为任何路径或移动命令一定成功。

## 仅本国敌对占领

所有权与控制权是 `province+0x468` / `province+0x470`，由原生存档 token 已确认。必须 owner 完整 Tag 等于玩家；盟友、属国或其他国家所有的省份不会标为收复目标。

纯国家敌对关系采用原生移动到达逻辑中已内联的谓词，不执行含副作用的到达函数：

1. 以 province controller 的有效 Tag 解析当前 controller country。
2. controller 与玩家相同，返回 false。
3. 查询 controller country `+0x1548` 敌对数组，count=`int32(+0x1554)`，stride 16，首 int32 为对方国家索引；包含玩家索引则 true。
4. 控制国或玩家索引等于上述任一原生特殊国家索引时同样 true；其余 false。

精确依据：`work/native-path/pseudocode1.txt`，机器代码 `0xD4FE76..0xD4FED9` 及对应反编译。它先从军队所在省份 `+0x470` 解析控制国，用军队 owner Tag 做上述比较，随后才检查撤退/战斗状态并调用 `0x247C80`。helper 只读这段关系数据，绝不调用移动到达/围城函数。`work/native-path/pseudocode3.txt` 的 `0xD45250` 对控制国关系有独立相同内联比较；不调用其包含要塞/路径的整体函数。

因此异国控制并不自动等于敌对占领：普通中立/友方控制且无敌对记录返回 false。敌对数组数量/跨度/索引异常、owned 省份 controller 不合法或国家对象 Tag 不匹配时，整份结果失败关闭。数组允许重复关系记录，因为原生只做存在性查询，没有唯一性前提。

## 输出及验证边界

成功输出绑定 sessionId/world/player/day/armyId/origin，含普通省份 DTO `id/owned_by_player/occupied_by_hostile`，以及可见敌军 DTO `key{id,generation:0}/province/visible/legally_attackable`。generation=0 是当前底本未恢复独立持久 generation 的占位；外层仍须用 session/lifecycle 防止 ID 重用，不能将此视为 generation 已从游戏读取。

成功时 `targets_complete:true` 只表示这份目标候选读取完成。始终 `orders_permitted:false`；没有路线、地理边、暂停/手动接管/战败调度或完整命令许可。不得直接包装成可发令的完整 `WorldSnapshot`。总省份上限 50,000、可见节点 100,000、关系 count 上限 32,767、同步读取期限 5 秒，拒绝任何部分结果。实际成本由返回 count 指标记录。

验证：`node --check tools/native_targets.js` 通过；`node tests/test_native_targets.cjs` 13 项合成测试通过。涵盖：FOW 先于列表读取的禁止访问断言；战争敌军/叛军与友军分离；撤退/流亡/空军团/舰队排除；本国战争占领、特殊国家占领与中立控制分离；缺失/过期/跨读档/读取中改变的 FOW 证书；日更新开始/期间拒绝；列表循环、未知 vtable、坏关系表与错误线程失败关闭；kind1/index0 目标的完整 Tag 验证。尚未运行此 helper，也未声称特殊军队、所有地图或真实发令验收通过。

## 可运行的被动证据与 manager 队列 probe

`tools/native_targets_probe.js` 由宿主拼接到 `native_targets.js` 后，宿主核验批准 hash 并选择隔离 `/work/game-stage/eu4.exe`。该脚本自身不 attach 进程。它检查唯一 staged 模块、x64 架构与全部入口前缀；若宿主提供 `PROBE_CONFIG.path`，还要求和实际模块完整路径一致。没有版本/地址不明时的兼容回退。

真实观察入口及重新从冻结文件核对的前 16 字节：

```text
manager 0x8137C0: 48 8b c4 88 50 10 55 53 56 57 41 54 41 55 41 56
FOW     0x8275E0: 48 89 5c 24 08 55 56 57 41 54 41 55 41 56 41 57
daily   0x758430: 48 8b c4 55 53 56 57 41 54 41 55 41 56 41 57 48
load    0x5CDC60: 48 89 5c 24 18 55 56 57 48 8b ec 48 81 ec 80 00
setter  0xA0B150: 48 89 5c 24 10 57 48 83 ec 20 4c 8b 41 28 0f b6
```

只 replacement `0x8137C0`，ABI 为 `void(manager*, uint8_t)`。先准确转发原函数，再核对实际 world+0x1E00 等于参数 manager、manager vtable 为 `0x1CB8460`、dailyUpdating=0、loadDepth=0、正确且不变的原生线程，以及处于最外层 manager 回调。满足后才处理一个 RPC 队列任务；不依赖军队面板，也不监听/替换 panelupdate。用 replacement 而非 manager onLeave 的原因是后者的 Interceptor listener guard 会压制嵌套原生观察；这里必须让真实 FOW/daily/load 回调继续可观察。

FOW onEnter 失效旧证书；正常 onLeave 仅在前后 world/player/day/FOW 模式、session 和 revision 都稳定时建立真实 epoch。不主动调用刷新，也不因读取到 cache 就把 fresh 设为 true。若完成发生于 dailyUpdating=1，证书记录真实刷新，但读取仍必须等稳定 manager 帧且 helper 再次确认 dailyUpdating=0；raw day 后续变化会令证书失效。加载期间的 FOW 不发有效证书，load 返回后仍等待一次新的自然 FOW 完成。

world-daily onEnter 立即失效；load onEnter 轮换 session、取消旧任务，onLeave 保持等待新刷新。完整 FOW 以外的 setter 立即失效；刷新期间另一个线程的 setter 也不能冒充属于当前刷新。不同线程并行刷新导致当前证书拒绝。当前玩家/world 的变化轮换 session；读取时发现全局 FOW 开关、日期或身份不同也失效。证书线程必须和已确认的 manager 线程一致。

RPC：

```js
await script.exports.snapshot({
  army: '0x...',      // 宿主从当前原生选择等已知来源取得
  armyId: '0x19d300000036',
  world: '0x...'
});
script.exports.status();        // FOW、当前world、线程、session、队列和cleanup状态
script.exports.cancelpending(); // 仅取消排队请求，不中止已开始的同步native查询
await script.exports.dispose();
```

snapshot 引用必须为非零 64 位十六进制字符串；RPC 线程不调用目标 helper。队列最大 1 项、10 秒内没有稳定 manager 帧则拒绝请求。真正执行时再次比较 caller world、当前 session、army vtable 和完整 army ID；随后在原生线程调用 helper。没有 fresh 证据时返回 helper 的 `targets_complete:false`，不伪造目标列表。`status()` 的当前世界读数是诊断信息，不能替代排队 snapshot 的稳定时点验证。

脚本安装前 `Script.pin()`。dispose 先停止接收、失效证书并取消队列，然后等 manager 和所有观察 hook 的原生调用深度归零。退出 native 回调之后才 revert manager replacement、detach 全部 hook、flush，最后 unpin 并返回 `stopped:true`。不要求 panel 打开；即使没有后续游戏帧，只要已无活动原生调用也可清理。清理失败必须保留 pin，返回错误并允许宿主显式重试；不能用 unload 超时假装完成清理。

验证：probe 的 node 语法检查通过；`node tests/test_native_targets_probe.cjs` 的合成生命周期场景通过，包括原函数先于 helper、嵌套 manager 只处理一次队列、自然 FOW 发证书、独立/异线程 setter 与 FOW 模式改变失效、daily/load 失效、load 中刷新拒绝、引用不符拒绝、活动 hook/manager 内请求 dispose 时等待安全退出，以及 detach 失败保留 pin 后可重试清理。此处测试的是 probe 调度/生命周期；真实敌军读取另由 helper 测试覆盖。probe 尚未游戏内运行，运行结果由主线程另行记录。
