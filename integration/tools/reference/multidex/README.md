# 多DEX独立参考

`package-lock.json`固定官方SDK：Pancake V2 1.2.3、V3 3.10.3；源码revision、SHA256、许可头和官方registry integrity核对见source-metadata.json。Node参考库只安装在integration/.runtime，不是服务依赖。

`make -C integration multidex-reference-refresh` 使用npm ci --ignore-scripts重建独立SDK/不变量golden及protoc wire样本。正常测试只读固定向量，不隐式联网。

官方来源：
- https://github.com/pancakeswap/pancake-swap-core/tree/3b214306770e86bc3a64e67c2b5bdb566b4e94a7
- https://github.com/pancakeswap/pancake-v3-contracts/tree/986847948755cba528324d41be19480731c36c2a
- https://github.com/sushiswap/v2-core/tree/a349882c918c752d0363e4c6e02dedcbc48b734e
- https://registry.npmjs.org/@pancakeswap/v2-sdk/1.2.3
- https://registry.npmjs.org/@pancakeswap/v3-sdk/3.10.3

源码只在隔离临时参考中读取，未将原Solidity实现复制进项目。旧Pancake V1及锁定Sushi源码不构成其版本目前活跃部署的声明。完整许可证/SBOM和生产协议目录审计仍属于原工作包退出条件。
