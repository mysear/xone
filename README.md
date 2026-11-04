# Xone

项目分为三个独立服务、公共协议和本地集成测试：

```text
xone/
  chain-parser-go/       Go项目：EVM解析、Pebble、当前基线、analytics Job
  chain-parser-rust/     Rust项目：Solana数据、账户屏障、RocksDB
  dex-router/            Rust项目：状态副本、数学、图、拆单、报价API
  common/                公共协议源码及 Go/Rust 生成类型
  integration/           跨服务构建、检查、联调、Compose、SQL、公共测试数据及产物
  docs/                  协议说明、配置规范、架构决策与源码来源资料
  XONE_SYSTEM_DESIGN.md   设计基线（原文保留）
  DEVELOPMENT_PROGRESS.md 功能/阶段/WP01–WP26逐项范围与进度
```

每个服务有自己的构建文件、依赖锁、环境配置、代码、测试、Dockerfile 和 README；公共协议和生成类型统一引用 `common/protoc`，服务不再携带协议副本或自行生成。构建需要保留相邻目录布局。两个 Rust 项目内部的 domain/config/observability 等实现副本仍由公共检查防止漂移。

两个解析器的正式入口统一为 `cmd/chain-parser/main.go` 或 `cmd/chain-parser/main.rs`，路由器入口为 `cmd/dex-router/main.rs`；测试资源统一为 `test/contracts`、`test/fixtures`，独立调试程序位于 `test/cmd`。Rust 库 crate 继续使用各自的 `src`。

当前交付是**离线开发闭环**：可以启动三个真实进程、消费两个解析源、执行ExactIn路径/拆单计算、验证重启代际和旧报价失效。真实Kafka/PG开发链路、双副本与旧主隔离已接通并验证；生产TLS/ISR/故障域、独立余额差仿真、真实Yellowstone及更多协议仍未完成。`FAST`返回`INDICATIVE`；`VERIFIED`和交易build明确拒绝，不返回看似可执行的交易。详见进度文档。

项目进度只维护 [DEVELOPMENT_PROGRESS.md](DEVELOPMENT_PROGRESS.md)，其中包含原26工作包、全部协议覆盖、功能与设计条款映射及历史实施计划。

## 验证和联调

```bash
# 每个项目均可独立运行相同入口
make -C chain-parser-go check
make -C chain-parser-rust check
make -C dex-router check

# 三进程联调：临时目录、临时凭据、临时端口；自动停止本次子进程
make -C integration fixture-test

# 公共契约副本检查和机器可读报告
make -C integration contract-test
make -C integration acceptance-report
```

跨项目构建统一由 `integration/Makefile` 调用各服务自己的 Makefile；公共测试数据和检查分别位于 `integration/test`、`integration/tools/ci`。各服务保留自己的单测数据和构建入口，构建引用相邻 common，运行不依赖 integration。

Protobuf 已接入类型生成和跨语言编码测试，当前Kafka数据通道和owner bootstrap已使用Protobuf，HTTP报价/诊断及显式HTTP开发profile使用JSON。具体消息和实现边界见 [服务通信说明](docs/TECHNICAL_REFERENCE.md#service-communication)。

本地集成测试入口为 `make -C integration test`，详见 [integration/README.md](integration/README.md)。

组合启动和 Compose 使用方法见 [integration/README.md](integration/README.md)。

需要Go 1.26、Rust 1.90.0、Protobuf编译器、Python 3；Rust工具链由各项目rust-toolchain.toml固定，Make显式使用rustup的Cargo。RocksDB首次编译需要C++工具链和libclang，耗时明显长于后续构建。单元测试与联调在首次安装依赖后不读取公网RPC。

## 配置和安全边界

服务使用 `--config config/config-dev.yaml`、`config/config-test.yaml` 或 `config/config-prod.yaml`，每个环境文件包含全部所需配置；保留 `--check-config`。无配置包版本、文件清单、逐文件 SHA256 或开发凭据生成步骤。可选 API 认证通过 `api.auth_token` 显式配置；不配置时无需本地 token 文件。配置值仍执行字段和约束检查。

正式解析服务使用 RPC，analytics 使用 journal。合成样本与离线回放由服务内独立 `test/cmd` 调试程序提供，默认构建和运行镜像不包含调试程序。`make -C integration fixture-test` 显式构建/运行调试入口；RPC 重组测试使用正式解析器。开发/测试配置需按本地节点填写链身份和池目录，生产文件为待填写部署参数的模板。

公共协议变更运行 `make -C common/protoc generate`，用 `make -C common/protoc check` 检查两种语言生成物；普通服务构建不需要生成工具。Docker 使用 xone 根目录作为 context，例如 `docker build -f chain-parser-go/Dockerfile .`。


## Kafka / PG 开发集成

三服务可显式配置 `journal.transport=kafka`：解析器使用PG租约和稳定transactional.id，提交prepared→Kafka事务→KV；计算副本取owner完整当前checkpoint+cursor后消费read_committed有界尾窗。开发release为`xone-dev-20261025-r2`，具体条款和未完成项见[第7章核对](DEVELOPMENT_PROGRESS.md#design-traceability)。

```bash
make -C integration journal-integration-test
make -C integration kafka-smoke-test
```

这些目标需要独立的本地PostgreSQL工具、Java及Kafka发行包；测试只绑定loopback，并使用专用临时数据库和topic。当前主机工具位置记录在integration/tools/acceptance/infrastructure.py，其他环境需显式调整工具路径。测试会关闭本次创建的进程，不使用已有开发数据库。

PG开发profile只支持显式明文连接：`control.backend=postgres`、`control.allow_plaintext_development=true`，DSN凭据文件为0600；Go侧要求完整`postgresql://user[:password]@host:port/database?sslmode=disable` URI并禁用PG环境及默认passfile。生产TLS配置尚未接通，不能将明文测试profile用作生产准入。


限定V2 Swap事实与隔离analytics Job现按§7.9.7/§16接入同一journal，使用`service.role=analytics`、`service.mode=journal`和独立配置/state目录；完整契约、运行要求、watermark/删除修订及剩余范围见[规范化事实与分析Job](docs/TECHNICAL_REFERENCE.md#normalized-facts-and-analytics)。当前仍无ClickHouse发布或正式分析API。

路由器访问受保护的解析器时，在 `journal.source_auth_tokens` 中按 owner HTTP 基地址配置上游 token；它与路由器自身的 `api.auth_token` 独立。未配置的上游请求不发送 Authorization。集成工具会显式填入本次临时凭据。

## 优先区块解析与兑换量：标准V3有界报价

Go解析器现可在同一blockHash下验证标准Uniswap V3的runtime code SHA256、factory/token/fee、slot0/liquidity、连续bitmap及每个initialized tick；Rust计算器使用独立V3整数内核，支持双方向、跨tick和空流动性段。证明范围之外拒绝计算完整结果，不套V2公式。

混合V2/V3路径、共享V3物理池顺序重算和有限容量拆单已经接入。参考固定官方SDK 3.10.0的264项数学差分，另有共享池/候选发现回归及真实专用PG/Kafka双副本进程场景。详见[配置、边界与验证范围](docs/TECHNICAL_REFERENCE.md#uniswap-v3-bounded-quotes)。

专项命令为 `make -C integration v3-quote-test`；差分用例为 `v3-differential-test`。当前仍为支持域内的INDICATIVE报价，V3 calldata/独立执行余额差、真实链验收及更多协议不在本地通过声明中。数据分析保持低优先级。


多DEX/不同版本覆盖以[协议版本矩阵](DEVELOPMENT_PROGRESS.md#protocol-coverage)为准，检查脚本直接读取同一文档中的普通进度表。新增Pancake V1/V2/V3、Sushi V2的限定标准版本profile，包括同blockHash code/factory/费用/layout解析与独立参考金额测试。未知版本不会套用通用V2/V3公式；Pancake V3非零LM回调首批明确拒绝。专项运行 `make -C integration multidex-quote-test`，当前仍为本地支持域INDICATIVE，不表示全部链/DEX/版本或生产已完成。
