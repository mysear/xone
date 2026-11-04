# 稳定池独立参考

此目录只保存验收参考的版本清单和依赖锁，不进入三个业务服务的运行时。

在 `xone/integration` 执行 `make stable-reference-refresh`，或执行
`python3 tools/ci/stable-oracle-setup.py --python /path/to/python3.9`。
Vyper 0.3.1 要求 Python 3.9/3.10。Node 依赖通过 `npm ci --ignore-scripts`
安装到 `.runtime/stable-reference`；Python 依赖安装到 `.runtime/curve-vyper`。
已有环境可加 `--reuse-environment`，仍会核对编译器版本和每个源码 SHA256。

生成器从固定 revision 获取官方源码，逐文件校验 `source-metadata.json`。
Solidity 编译器固定为 0.8.19 / 0.7.6，Vyper 固定为 0.3.1，EVM 由
Ganache 7.9.2 执行。生成过程不导入服务的 Rust 数学实现。

- Aerodrome：原 Pool 的 `_f/_d/_get_y/_getAmountOut/_k` 函数原样提取；
  包装层模拟原合约先移出输入手续费，再检查不变量的顺序。
- Curve：编译并部署原 Plain2Basic Vyper，按该版本布局写入初始状态，
  实际执行 `exchange` 并读取内部余额；ERC20 mock 的转账固定成功。
- Balancer：编译历史非 composable StablePool 同 revision 的原 StableMath
  及其依赖，包装层采用原池输入手续费、scale、Vault 余额更新顺序。

180 个独立用例包含双向、不同精度、不同 A/余额比例、dust 与 revert，
比较输出量及两个后状态余额。另有 `stable-allocations.json` 的 16 组连续共享池分配，
覆盖四种模型、双向与两种输入顺序，逐步执行参考并比较后余额。生成的 JSON 位于
`integration/test/contracts/stable-quotes.json` 与 `dex-router/test/contracts/`，
摘要位于 `integration/artifacts/local/stable-golden-generation.json`。

这些用例证明限定数学域的整数结果，不代表真实部署已审计、Token 特殊行为
已覆盖、执行器已完成或生产已验收。Slipstream 的金额当前另以固定 Uniswap
V3 SDK 参考校验，专用 ABI/外部费用模块由解析器回归和进程用例验证，未宣称
Slipstream 完整合约执行差分。
