# V3 独立参考

固定 `@uniswap/v3-sdk@3.10.0`、`@uniswap/sdk-core@4.2.1`、`jsbi@3.2.5`。package-lock记录全量依赖和integrity；SDK许可为MIT，见LICENSE.sdk。普通项目构建不依赖这些Node包。

`make -C integration v3-reference-refresh` 使用npm ci --ignore-scripts安装到 `integration/.runtime/v3-sdk-oracle`，然后生成官方SDK差分fixture及独立protoc wire fixture。生成器不导入xone数学代码，参考实现不导入xone服务。

官方来源：
- https://github.com/Uniswap/v3-sdk （固定npm release 3.10.0）
- https://github.com/Uniswap/v3-core/tree/v1.0.0/contracts/libraries
- https://registry.npmjs.org/@uniswap/v3-sdk/3.10.0

core参考材料的版本、SHA256和许可头记录在source-metadata.json；未将原Solidity库复制进运行时源代码。源码/依赖许可证的完整生产发布审查仍属于原WP01/SBOM退出标准。
