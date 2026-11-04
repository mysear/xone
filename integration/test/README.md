# 公共测试数据

`contracts/` 维护跨语言编码、journal 分片、规范化事实、V3 状态/报价等 golden；`evm/baseline.json`、`solana/baseline.json` 为离线合成样本，不代表真实链状态。

服务只在 `test/contracts` 保留其代码实际引用的测试向量，副本内容由 `tools/ci/contracts.py` 与中央维护源比较。不同服务所需向量可以不同，不再强制全量复制。

供手动离线调试和单测复用的样本放在服务 `test/fixtures`；正常启动配置不含 fixture_path 或 fixture mode。集成脚本自行创建具有本次时间戳的临时样本，并显式运行独立调试程序。

更新 golden 必须核对编码或状态语义变化，并同步实际使用的服务副本，不应通过改预期值掩盖不兼容变化。
