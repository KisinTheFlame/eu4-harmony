---
status: superseded by ADR-0011
---

# 通过 winmm.dll 代理进入游戏进程

框架以游戏根目录下的 `winmm.dll` 代理进入 eu4 进程，磁盘上的 `eu4.exe` 保持原样。Windows 加载器解析 eu4.exe 导入表时，对不在 KnownDLLs 中的 DLL 先查找 exe 所在目录；代理把 eu4 用到的 15 个 `waveOut*` / `waveIn*` 函数转发给 System32 的 winmm，同时在进程内启动框架。

## Considered Options

- **用启动器替换 `eu4.exe`**（eu4-regional-combat 0.3.x 的做法）：Steam 验证会还原入口；需要转发 Paradox 启动器参数；占用 `eu4.exe` 文件名，与其他改版 exe 互斥。
- **静态补丁 exe**：多个补丁只能二选一，要分发整个 Paradox exe，每个游戏版本都要重做。
- **`version.dll` / `d3d9.dll`**：eu4 同样静态导入，但已被中文字体插件占用。
- **`bcrypt` / `crypt32` / `d3dx9_43` / `opengl32`**：处在加密或渲染路径上，进程内所有同名导入都会经过代理，导出也多得多。

## Consequences

- `DllMain` 处于加载器锁内，只做最少工作；正式初始化推迟到挂钩的游戏早期函数。
- 根目录已有他人的 `winmm.dll` 时不覆盖：安装说明要求先把它移进 `eu4-harmony/`，代理链式加载它。
- 依赖 eu4.exe 静态导入 `WINMM.dll`；游戏更新若去掉这条导入，需要换入口。
