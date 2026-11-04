# Balancer V2 双币等权独立参考

固定上游 Balancer V2 revision `e281ff40583c39bf1d46c322334b2b13936e0e33`。
`source-metadata.json`保存 WeightedPool 与 BalanceAllocation 的40个原文件闭包及SHA256。
下载器逐个校验，原文件存放在被忽略的`integration/.runtime/weighted-reference/sources`。

`Harness.sol`部署原始 WeightedPool，实际调用原始 BaseMinimalSwapInfoPool.onSwap、
BasePool扣费/缩放、WeightedMath、FixedPoint以及BalanceAllocation余额变更。
Vault facade仅提供构造/回调入口与原余额库，不是完整Vault，不执行ERC20转账。

在`integration`执行`make weighted-reference-refresh`。编译/执行环境复用
`tools/reference/stable-models/package-lock.json`固定的solc07 0.7.6、ethers5.8.0与Ganache7.9.2，
不需要Curve/Vyper。首次可将该目录的package.json及lock复制到
`.runtime/stable-reference`，再在该运行目录执行`npm ci --ignore-scripts`。
参考生成器不导入业务数学代码；每次保留实际生成脚本、源码清单、harness、依赖锁和
已安装Node依赖文件的执行前后指纹。输出位于`integration/test/contracts/weighted-quotes.json`，
由router自己的契约副本读取。`weighted-wire-golden.py`使用独立protoc编码field27。

模型限定双币不可变50/50、decimals0..18、零资产管理器及零managed余额。
其他权重、Managed/Composable/BPT/外部rate、真实部署及转账执行均需独立验收。
开发完成情况只记录于根目录`DEVELOPMENT_PROGRESS.md`。

任意合法双币不可变权重使用独立的`weighted-static-reference-refresh`目标。
生成器仍部署同一具体WeightedPool，权重作为原始构造器参数传入，并另执行
`StaticMathRef.sol`对原FixedPoint/LogExpMath的可见性包装。新输出采用
`weighted-static-*`命名；旧三个canonical fixture的固定摘要在生成前后及验收中核对。
静态profile的完整数学域、版本切换规则见`docs/TECHNICAL_REFERENCE.md`的
`balancer-weighted2-static`小节；此目录不记录开发进度。
