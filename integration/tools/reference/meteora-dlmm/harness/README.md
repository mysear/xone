# 固定 Meteora MM-only 参考与验证

服务适配标识为 `meteora-dlmm-mm` / `meteora-dlmm/1532e540-mm-1`。来源固定到 `source-metadata.json` 的提交和逐文件摘要；运行时参考是上游随 SDK 保留的 SBF 测试程序，不证明某个主网 ProgramData 或历史银行已经认证。

从仓库根目录运行：

```sh
python3 integration/tools/ci/meteora-reference-fetch.py --fetch
python3 integration/tools/reference/meteora-dlmm/harness/verify_source.py
python3 integration/tools/reference/meteora-dlmm/harness/reference.py
python3 integration/tools/ci/meteora-mm-state.py
python3 integration/tools/ci/meteora-mm-edge-reference.py
python3 integration/tools/acceptance/meteora_mm_reference.py
make -C integration meteora-mm-differential-test
make -C integration meteora-mm-current-check
```

原程序执行继续使用集成工具已有的固定 solders/LiteSVM 运行时与 `integration/test/source/deployment-catalog/solana-rebuild/runtime-lock.json`，缺少这组本地依赖时必须先按现有 Solana rebuild 工具准备；下载 SDK 并不等于准备好执行环境。正常进程检查还需要集成基础设施指定的本地 PostgreSQL、Kafka/JDK 和三服务调试二进制。统一验证会构建当前二进制并启动独立数据库与消息队列。

下载器只恢复清单中的文件，核对大小、Git blob SHA-1 与 SHA-256；已有缓存不匹配时拒绝覆盖。原始银行与服务测试数据、执行报告按仓库规则保存在本地，未纳入版本控制。生成器把实际使用的数据分别复制给 Rust 解析器和 Router，服务测试不跨项目引用。

受控参考明确修改 function_type 为 LiquidityMining、清零限价订单账本、按 bin 与协议费配平 vault，并记录覆盖前后摘要；Clock、WSOL backing 与用户资金也是本地输入。解码器测试保留 LiteSVM 的实际 classic Token 程序代码，但将受控 ProgramData 的部署 slot 重标为 1。正常 RPC 进程保持数学时间戳和账户字节，重标本地 bank slot 并提供合成区块头。

独立参考检查器逐字节比较 pair、Oracle 与声明数组，核对失败回滚、实际付款/收款、vault 转账和 WSOL lamport 守恒，并在普通和优化 Python 模式运行重哈希篡改反例。正常服务计划经 Rust 生产内核投影，再在新的 LiteSVM 中运行原 SBF，比较声明后状态和实际到账。

当前仅为 classic SPL / backed WSOL、无奖励、无订单、InputOnly、已缓存非空 bin 价格、默认 bitmap 内有限数组的 ExactIn。ExactOut的独立范围见下文；OnlyY、Token2022、bitmap 扩展、奖励与限价订单、真实链资格、公开交易构建及独立生产模拟需单独实现和验证。报价仍为 INDICATIVE，净链费用估计保持不可用。

## 固定 MM-only ExactOut

新增 `SwapExactOut2` 有独立目标生成器和输入上限反例：

```sh
python3 integration/tools/ci/meteora-exact-out-reference.py
python3 integration/tools/acceptance/meteora_exact_out_reference.py --reexecute
make -C integration meteora-exact-out-differential-test
make -C integration meteora-exact-out-current-check
```

上述ExactOut范围已通过有限本地验收，详见DEVELOPMENT_PROGRESS.md阶段50。164 个初始目标覆盖双向、微小输入、active-bin 容量上下边界、跨 bin、动态费 filter/decay、Oracle 120 秒与环形边界、费用封顶、零费率和高价/低价舍入。每个成功目标还运行同银行输入上限减一控制；后续目标在原 SBF 的同一银行顺序执行。

固定 SBF 精确转给 recipient 目标数量；部分案例中 bin 账本因整数舍入减少更多输出，差额留在 vault。不能裁剪真实 bin 后状态，也不能把差额算成用户额外到账或协议费。独立校验器核对 raw pair/array/Oracle、payer/recipient、vault、WSOL backing 和输入限制；完整原银行与失败控制保存在本地证据中。该行为不建立任意版本 DLMM 的兼容性。

每条 ExactOut 路由限一个池；有序分配重用相同物理账本时重算各路输入。API 的 `grossBuyAmount` 等于目标，`outputSurplus` 为零，未花预算没有实际转账所以 `refund` 为零。`factory` 承诺字段在 Solana 方案中记录 DLMM Program ID，验证器按 chain 区分地址格式。主网资格、公开签名/构建、独立生产模拟及其他策略仍需单独验证。

## 固定订单策略 ExactOut（阶段53）

已有阶段49/52的MM基础fixture与orders解码配置时，可以独立恢复订单ExactOut证据：

```sh
make -C integration meteora-orders-exact-out-reference-refresh
make -C integration meteora-orders-exact-out-current-check
```

新矩阵单独写入`meteora-orders-exact-out.json`，保留阶段52的ExactIn证据。192原始银行包含161初始成功、31拒绝和16组三步序列，共193成功步骤；161成功初始银行都有原程序最大输入减一失败对照。正常服务检查另外启动隔离PG/Kafka、Parser与双Router，将实际API计划经生产ExactOut投影器重算并在新LiteSVM回放原SBF，验证两种来源故障、恢复及Router重启。

SDK部分bin按层分别反解的实现不等于此固定部署SBF的执行：实际部分bin按总目标反解，耗尽bin保留每层整数输入之和。跨层取整不足时原程序6085拒绝；报价也拒绝。bin或订单fulfilled可比目标实收多，差额保留vault dust。此策略不增加订单放置/取消、Token2022或奖励交叉、公开执行和可信公共来源能力。
