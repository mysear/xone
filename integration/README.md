# 本地集成测试

`integration` 管理跨服务构建、代码一致性检查、公共测试数据、本地基础设施联调和测试报告。

参考依赖的下载内容、第三方构建产物和执行证据保存在本地，不随当前 Git 提交分发；版本清单、验证代码、许可证和准备说明继续保留。首次克隆后运行参考测试前，请先按 [参考缓存说明](tools/reference/README.md) 准备对应依赖及测试数据。

- `compose/`：本地服务组合与生成的配置 bundle，使用各服务自己的 Dockerfile。
- `postgres/schema.sql`：控制面数据库初始化源。
- `runbooks/`：本地故障排查与操作手册。
- `Makefile`：编排三个服务的构建和测试，构建命令全部调用服务自己的 Makefile。
- `tools/ci/`：协议/golden/样本副本、服务职责边界、联调日志检查，以及凭据和 Compose 配置准备。
- `tools/acceptance/`：离线三进程测试、PG/Kafka 集成、双副本与旧主隔离测试、报告生成。
- `test/`：公共 golden 和合成样本维护源；服务保留独立单测及开发模式所需副本。
- `schemas/log-event.schema.json`：集成测试日志字段规范。
- `artifacts/local/`：本地报告和日志，不进入版本控制。
- `.runtime/`：测试基础设施数据和依赖缓存，不进入版本控制。

## 构建与测试

在 `xone` 目录执行：

```bash
# 委托 chain-parser-go、chain-parser-rust、dex-router 各自的 make build
make -C integration build

# 公共副本/边界检查及三个服务自己的契约测试
make -C integration contract-test

# 运行各服务的 make check，并记录退出码和日志
make -C integration check

# 公开主网 RPC 联调，读取八条链的真实池状态
make -C integration public-rpc-test

# 要求八条链都通过正式解析器启动并发布池快照
make -C integration public-rpc-owner-test
make -C integration public-evm-replay-test
make -C integration public-evm-window-test
make -C integration public-bsc-swap-test
make -C integration public-solana-replay-test

# 构建后执行隔离三进程离线联调；自动停止本次子进程
make -C integration fixture-test

# 默认本地测试：check、fixture-test、日志字段校验和报告
make -C integration test
```

`make -C integration` 默认执行 `test`。`acceptance-report` 与 `test` 使用相同的现有离线验收流程。报告写入 `integration/artifacts/local/`。未通过的代码检查或联调会返回失败，不会标记为成功。

主网测试使用各解析器的 `config-dev.yaml`，每次启动分配临时本地 API 端口、凭据和空状态目录。Ethereum 使用 MEV Blocker 与 PublicNode，Solana 使用官方 RPC 与 Pocket Network。两条链均通过正式解析器发布池快照，再按同高度或 slot 核对块哈希、父哈希和时间。Solana 同时检查 Raydium 账户批次的 slot 与块引用一致，并按金库余额扣除累计费用复算两侧净储备。Ethereum 还在该高度复查 DAI/WETH 池的运行时代码、工厂、代币和两侧储备。另六条 EVM 链使用 [主网目标配置](config/chain-targets.json) 中的池地址、工厂、代币和合约代码 SHA-256 读取固定 finalized 高度的状态。BNB Smart Chain 核对 Pancake V2 双边储备，Base、Arbitrum One、OP Mainnet、Polygon PoS 和 Avalanche C-Chain 核对 Uniswap V3 价格、tick 和流动性。读取后再次核对块头。

`public-rpc-owner-test` 为这六条链逐一启动 Go 解析器，使用目录中的前两个 RPC 核验原生区块和回执。V3 读取当前 bitmap word 的全部初始化 tick，主网复验配置使用最多四个并行请求、每个批次最多 16 个 tick。Base 公开端点有批次大小和调用频率限制，验收配置对该链逐条读取。解析器快照须与同高度合约状态匹配。单链复验可在构建后运行 `python3 -B integration/tools/acceptance/public_rpc_smoke.py --all-owners --chain arbitrum`。同一进程的多链复验使用 `--shared-owner --chain base --chain optimism`。这两条链上的 WETH 合约地址相同，池、区块和价格仍需按各自 chain key 校验。每条链打印实际回读端点。Go 采用开发配置的 20 分钟时效限制，Solana 为 2 分钟。公网来源过期、核对不一致或服务启动失败均会返回非零。主网联调均只读，结束时清除本次临时凭据、状态和日志。

`public-evm-replay-test` 从七条 EVM 主网逐链采集正式解析器使用的原始 RPC 响应，按端点封存请求和响应，包含完整的 JSON-RPC 批次。随后用本地回放端点和空状态目录重启解析器，检查每个来源均参与，所需请求均有记录，并比较完整发布快照。输出包含快照 SHA-256、各来源回放请求数以及归档和实际重放的批次数。构建后可用 `python3 -B integration/tools/acceptance/public_rpc_smoke.py --evm-replay --chain arbitrum` 单链复验。

`public-evm-window-test` 默认从 BNB Smart Chain 主网采集正常 Go owner 的启动快照和至少三个连续提交。验收检查同一 generation 内的父块、journal 序号和完整池状态，在第二个公开端点复核首末池状态。封存的 finalized 响应提供首末区块锚点。离线阶段用两个只读本地回放端点按编号解码，在空 Pebble 目录重建 journal，核对两份完整快照、每条记录的编号和状态摘要，以及包含交易事实的 staging 摘要。构建后可用 `python3 -B integration/tools/acceptance/public_evm_window.py --chain polygon --commits 3` 复验其他指定主网。结果记录区块与完整截面摘要，命令退出时清理本次归档和临时库。

`public-bsc-swap-test` 要求同代窗口中至少有一条固定 Pancake V2 池的 Swap。两个配置端点提供原生区块、回执和连续 journal，NodeReal 的固定高度状态读取只作为 BSC 池状态见证，不能替代两个 owner 来源或原生提交证据。另用 PublicNode 的区间日志逐项核对 journal 中的区块哈希、交易及日志索引和四个原始金额。离线重建比较完整事实批次，并选一个非空块走历史读取及原生回执证据复验。结果包含窗口、Swap 和历史数据集摘要。

`public-solana-replay-test` 依次用 Solana 官方端点和 Pocket 作为账户来源，重启同一个 Rust 解析器，每次用另一端点复核 finalized slot 的块哈希、父哈希和时间。两次发布后关闭解析器，从原始 RPC 归档重放两个池快照。输出包含实际端点、slot、读取编号、调用次数、不同响应数及快照摘要。重启后的发布序号和代次由在线服务分配，验收分别核对它们是否推进，以及来源状态能否重放。命令退出时删除本次归档。

Ethereum 连续窗口使用 `make -C integration public-signed-window-test`。从空目录启动，使用 Go 开发配置中的执行层 RPC 以及 PublicNode 信标接口，等待两个不同的签名最终化头。采集时限为 30 分钟。随后冻结响应，重新启动解析器，校验连续 journal、两个 Router 的整数报价、重启恢复和原始字节重放。命令退出时清除采集响应、状态目录和测试报告。仓库固定的信标检查点受七天时效约束；更新时通过 `BEACON_CHECKPOINT_ROOT` 指定经独立确认的主网检查点。整个过程只读。

各服务单独构建和测试仍使用 `make -C <service> build/check/contract-test`。integration 不复制 Go/Cargo 的构建实现，也不是服务编译依赖。

## PostgreSQL / Kafka 集成

```bash
make -C integration journal-integration-test
make -C integration kafka-smoke-test
make -C integration durable-journal-test
```

测试使用专用回环地址实例与临时凭据，并关闭本次创建的进程。基础设施工具位置沿用现有脚本：PostgreSQL 13 位于 `/usr/local/opt/postgresql@13/bin`，JDK 11 位于 `/Library/Java/JavaVirtualMachines/jdk-11.jdk/Contents/Home`，Kafka 3.9.1 位于 `integration/.runtime/deps/kafka_2.13-3.9.1`。数据库初始化使用 `integration/postgres/schema.sql`；本次实例数据写入 `integration/.runtime/infrastructure/`。Kafka 的应用日志和 JVM GC 日志写入实例专属临时目录，实例关闭时自动清除。

## 本地 Compose 与初始化

```bash
# 在 xone 目录执行
make -C integration compose-prepare
docker compose -f integration/compose/compose.yaml up --build
docker compose -f integration/compose/compose.yaml down
```

`compose-prepare` 在 `integration/compose/.runtime/` 生成三个权限为 0600 的服务配置，写入共享的本地开发凭据。解析器沿用开发配置中的主网 RPC 和池目录。Compose 使用各服务自己的 Dockerfile，仅将端口发布到本机回环地址。

数据库初始化源为 `postgres/schema.sql`，操作手册位于 `runbooks/`。所有跨服务配置、准备脚本和测试资源均由 integration 管理。

完整本地开发范围复验入口为 `make -C integration full-local-acceptance`，包含三个服务check、原离线三进程流程、真实PG/Kafka事务/崩溃边界、多副本接管，以及Kafka/HTTP两种重组场景。它仍不表示生产准入。


`make -C integration reorg-journal-test` 专门运行新增重组回归：真实PG/Kafka + 双合成RPC/双路由副本，再运行显式HTTP开发profile。它检查scope撤销、两次多块重组、undo保留、源分歧/重启、合格HEAD_FIRST，以及Kafka屏障后reader故障续已应用cursor。证据为 `artifacts/local/reorg-smoke.json`、`reorg-http-smoke.json` 和 `reorg-reader-recovery.log`。合成RPC不代表已验证真实链或来源故障域独立性。


重组验收现同时运行限定V2规范化事实与隔离Go analytics Job：稳定事实ID及Protobuf BE32、真实Kafka prepared崩溃恢复、只读消费者不fence owner、OHLC更正、删除修订、重启幂等、HEAD_FIRST历史缺口及复杂Swap毒消息隔离。新增专项报告 `artifacts/local/analytics-journal-smoke.json` 含6项进程检查，完整汇总字段 `analytics_journal_scope_passed`。Job运行契约见 `../docs/protocols/normalized-facts-and-analytics.md`。

## 配置与调试边界（2026-10-26）

服务读取单个环境配置文件，不再准备 manifest 或服务目录下的开发凭据文件。验收工具通过 `tools/acceptance/configuration.py` 生成临时 config-test.yaml，将本次 API token 写入该临时文件并设置0600权限。集成运行结束后临时目录清理。

`fixture-test` 和 Kafka 合成样本测试显式选择 `fixture-parser-go` / `fixture-parser-rust`；这些程序通过 `--fixtures` 加载测试样本。RPC 重组及 V3 测试中的 Go 服务仍运行正式二进制，Solana 配套状态由独立调试入口提供。正式 Docker 镜像只带服务二进制和环境配置，不带样本和调试程序。

`compose-prepare` 只生成普通 RPC 服务的单文件配置和集成认证值；启动前需填写容器可访问的 RPC 地址与池目录。公共协议只从 `common/protoc` 引用，生成一致性检查也由那里执行。

路由器访问受保护的解析器时，在 `journal.source_auth_tokens` 中按 owner HTTP 基地址配置上游 token；它与路由器自身的 `api.auth_token` 独立。未配置的上游请求不发送 Authorization。集成工具会显式填入本次临时凭据。

### V3解析及兑换量专项

`make -C integration v3-quote-test` 使用单文件环境配置、正常Go RPC入口、显式Rust调试样本入口和两个完整路由副本，在专用PG/Kafka下验证V3状态读取、双方向/跨tick、空流动性、缺tick/code身份错误、HEAD_FIRST、多块重组/重启和容量拆单。该目标同时运行 `v3-differential-test`：264个固定官方SDK样本、2种共享池顺序和2个候选发现回归。脚本不把合成RPC结果称为真实链余额差分。

`v3-reference-refresh` 是显式联网参考重建入口，使用固定SDK依赖锁，仅安装到隔离runtime。普通构建/测试读取固定参考数据。证据为 `artifacts/local/v3-differential.json` 与 `v3-three-service-smoke.json`，完整汇总新增 `v3_quote_scope_passed`；生产门槛保持关闭。


### 多DEX/不同版本专项

`make -C integration multidex-quote-test` 将6种明确profile（Uniswap V2/V3、Pancake V1/V2/V3、Sushi V2）放在同一合成EVM canonical状态，通过正常Go RPC入口、真实专用PG/Kafka和两个完整路由副本验证版本特有身份/费用/layout、完整receipt、reorg、错误code/LM暂停与恢复。另一Solana样本通过显式调试入口提供，不冒充真实Solana协议上线。

`multidex-differential-test`运行372个新增官方SDK/源码不变量参考样本；`multidex-reference-refresh`才显式联网重建固定参考，普通测试不下载依赖。新证据为 `artifacts/local/multidex-differential.json`、`multidex-three-service-smoke.json`，汇总字段 `multidex_quote_scope_passed`。协议与未实现版本单列于 `../DEVELOPMENT_PROGRESS.md#protocol-coverage`。


EVM完整块现已校验标准receipt 0–4及legacy post-state的`receiptsRoot`。`receipt-root-test`运行service-local独立向量与异常专项；`receipt-root-quote-test`运行专用PG/Kafka双副本共享删尾/共识字段故障；`receipt-root-evidence-check`检查本轮来源与30范围回归绑定。普通测试读取固定参考数据，不执行Node或下载依赖；只有`receipt-root-reference-refresh`显式安装锁定的EthereumJS依赖并重建12组向量及11个合成进程fixture根。该校验依赖RPC声明的区块头承诺，独立区块头/共识验证仍未完成。
