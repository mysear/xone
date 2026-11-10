# Slipstream固定上下文参考

固定`aerodrome-finance/slipstream`提交`f8717faaae6e6717db3c8e3850149c01a79c0603`。
原Pool源码与保留Base implementation的源码一致，原编译runtime逐字节匹配。
Factory使用保留的实际Base编译输入；其constructor/legacy-pool迁移与该上游提交
不同，费用getter逻辑保留，不声称二者完整源码相同。源码GPL标识保留。

原程序测试由`slipstream-extended-reference.cjs`编译未修改Pool/Factory/三个模块，
注入由官方编译器layout核对的受控初始state，用实际本地Token/回调付款与收款增量
记录完整price、fee/gauge/reward/staking、ticks和Oracle后状态。Voter使用保留的
真实Base runtime，其Solidity0.8.19执行体重编译匹配；仅mapping状态为受控值。
生成原getter十六进制返回和原before/after header，synthetic envelope另行显式投影。

动态模块有同区块initial-fee和`tx.origin`折扣。原90完整交换/18部分成交及4组12步
携带序列是上下文验证，不是固定fee的V3别名。Source bitmap窗口按新tick重取，
carried窗口保留原已知范围，生成器不能用缺失word猜零。

执行`make -C integration slipstream-context-reference-refresh`重建独立输入，
`slipstream-context-differential-test`测试模型/ABI/wire，`slipstream-context-quote-test`
运行normal Go、实际PG/Kafka及双Router后捕获实际计划，再由单独模型投影器和
原代码重放器核对。`--artifacts-only`仅重建原ABI/layout/runtime辅助输入，不证明
服务功能或原交易执行通过。固定输入原始报告与生产来源资格严格分开。
