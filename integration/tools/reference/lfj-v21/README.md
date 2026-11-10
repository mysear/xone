# LFJ Liquidity Book v2.1.0 原程序参考输入

本目录保存 `traderjoe-xyz/joe-v2` 的固定 `v2.1.0` 标签源码，`source-metadata.json` 对每个文件固定 SHA-256，另固定 OpenZeppelin v4.8.0 的 IERC20 接口以及官方 Solidity 0.8.10 编译器摘要。原始 MIT 标识及 LICENSE 保留。此目录不由任何业务服务运行时加载。

`node integration/tools/ci/lfj-v21-reference.cjs` 核对全部源码及编译器，编译未修改的原 LBPair 和库。需要已有 `integration/.runtime/stable-reference/node_modules` 中的固定 Ganache/ethers/solc wrapper，并将元数据列出的官方 soljson 文件放入 `integration/.runtime/lfj-solc`；编译器 SHA 不符时拒绝。生成器会检查原编译器产生的完整 storageLayout，不从未知部署猜测槽位。

参考使用本地受控 immutable-args clone、初始存储及五个完整 bin，执行原 `LBPair.swap` 和原 `getSwapOut`。测试 ERC20 维护实际余额并进行真实本地转账；输出比较收款方余额增量，同时保存原程序的 parameters、raw reserves、protocol fees、所有 bin 后余额和 Oracle 样本。96个案例包含两个方向、动态费开关、Oracle开关、三个时间边界、dust、跨bin和流动性不足。结果写入 `integration/artifacts/local/core-lfj-v21-original-reference.json`，保留 `local_profile_implemented=false`。

这些内容是独立参考输入。受控clone/初始账本不证明真实Avalanche历史状态或任一实际部署的来源资格，原程序执行本身也不表示普通Parser、wire和Router已接入该协议。

状态/wire与正常服务测试由 `make -C integration lfj-v21-differential-test` 和
`make -C integration lfj-v21-quote-test` 执行。后者编译独立的测试用模型投影器，
捕获实际API的request/quote/plan/normal baseline，校验请求与计划hash、输入守恒，
再将实际选中路线交给 `lfj-v21-plan-replay.cjs` 的原运行时代码；收款和声明后状态
必须逐字段一致。模型投影器的输出只属于服务模型观察，原程序执行属于单独比较器，
两者不互相替代。对比工具保留无损JSON整数，现有10类反例必须产生指定错误且不写
成功报告。

`lfj-v21-reference-refresh` 依次执行原程序参考、受控状态投影和独立C++wire生成。
新协议测试输入与旧参考分离，现有Uniswap/Curve/Solana等冻结语料不由此命令重写。
