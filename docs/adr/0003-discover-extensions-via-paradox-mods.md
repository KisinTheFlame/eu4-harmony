# 通过 Paradox mod 体系发现扩展

扩展随普通 Paradox mod 分发，由 Mod 目录下的 `eu4-harmony/manifest.json` 声明。框架只加载当前合集启用的 Mod 的扩展：读取 `dlc_load.json` 列出的 `.mod` 描述文件，按其中的 `path=` 定位 Mod 目录；只在游戏启动时加载一次，改合集后要重启游戏。玩家因此用创意工坊订阅、用启动器启用，无需另一套 mod 管理方式，Mod 的界面、本地化等资源仍由游戏自己加载。

## Considered Options

- **把清单字段写进 `descriptor.mod`**：用户目录下的 `ugc_*.mod` 由启动器生成，作者加的自定义字段不保证被保留。
- **框架自带 mod 目录和管理界面**：玩家要学第二套启用方式，也用不上创意工坊的订阅和更新。
