# regional-combat 的移植不放在框架仓库

框架仓库只放代理、核心、SDK 和脚手架（最小示例由脚手架生成）；regional-combat 移植成 Mod 后留在自己的仓库，像外部开发者一样依赖 SDK。这样移植过程同时检验了从框架仓库之外开发 Mod 的完整体验，也避免 regional-combat 的特定需求渗进框架。
