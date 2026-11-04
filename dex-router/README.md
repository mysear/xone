# dex-router — Rust兑换量计算服务

独立状态副本/不可变池图视图、V2精确整数数学、范围内路径搜索、最多两路共享池重算、计划hash、鉴权/配额/期限/证据API。当前仅提供INDICATIVE。

公共协议及 Go/Rust 生成类型统一位于相邻的 `../common/protoc`，服务构建只使用生成类型。`test/contracts` 仅保留本服务使用的测试向量；合成样本位于 `test/fixtures`，正式启动入口只使用 RPC / journal，不读取调试样本。

```bash
make check
make check-config
make run
# 或者
./target/debug/dex-router --config config/config-dev.yaml
```

Make会使用本项目锁文件。`make check`包含格式、静态检查、测试、契约和二进制构建；Go包含race检测。RocksDB需要C++与libclang；Rust固定1.90.0。配置校验失败在外部写入前终止；配置了 API 认证时，显式 token 至少24字符。PG DSN等部署凭据仍保留文件引用。

## 接口

公开诊断：GET /health/live、/health/startup、/health/ready、/metrics。配置了 `api.auth_token` 时，内部接口要求 Authorization: Bearer 凭据。可选凭据通过 `api.auth_token` 显式配置；为空时不启用该认证，不生成开发凭据文件。

POST /v1/quote 接收camelCase字段：chainKey、kind=EXACT_IN、sellToken、buyToken、sellAmount（字符串）、caller、recipient、slippageBps、verification=FAST。响应包含quoteId、SnapshotRef、QuoteView/图/策略版本、整数输入输出/退款、路线及质量范围。

VERIFIED返回INDEPENDENT_SIMULATION_UNAVAILABLE（422）；POST /v1/quotes/{id}/build在先校验有效期/重组后拒绝缺少独立仿真的计划。GET /internal/quotes/{id}/evidence可查看计划与版本证据。不能将该接口用于真实资金可执行报价。

内部状态源由本项目配置的journal.source_urls指定。开发源使用同一个内部测试凭据；生产多租户/隔离凭据和Kafka尚未接通。跨项目联调工具会自动生成临时凭据。

## 范围与证据

完整功能与未完成条件记录于上层 DEVELOPMENT_PROGRESS.md（唯一项目进度记录）。HTTP日志、重启和合成数据只能证明本地实现，不能替代Kafka事务、PG租约、真实链/协议数学、独立执行、多区HA及容量验收。

## 已接通的Kafka开发通道

配置完整的`control`与`journal.kafka`字段后，可选择`journal.transport=kafka`。两个解析器使用PG租约和Kafka stable transactional.id；本地prepared不可见，Kafka COMMIT成功后原子保存state/seq/cursor。路由器取owner `/internal/chains/{chain}/bootstrap` 的同批snapshot/cursor，使用独立replica identity读取完整固定分区，禁自动offset提交。超出有界尾窗时重新恢复当前checkpoint。

当前仅显式开发明文profile已验证；生产TLS/SASL、真实ISR/多区故障和完整reorg协议仍未验收。第7章逐条实现核对及专用基础设施报告位于上层docs/requirements与integration/artifacts/local。`VERIFIED`仍需独立执行后端，不因Kafka接通而开放。

## 环境配置与调试

`config` 只有 `config-dev.yaml`、`config-test.yaml`、`config-prod.yaml` 三个完整配置文件，不使用 manifest、配置版本、文件清单或文件摘要。`make run CONFIG=config/config-test.yaml` 可选择环境；`--check-config` 只校验所选文件，不连接数据源。

开发/测试配置使用本机 RPC 地址，需要配置对应链的 genesis、节点和池目录。生产文件是部署模板：需要填写真实 RPC、Kafka、控制面及认证参数。当前生产 TLS/SASL 能力仍受既有实现范围限制，生产模板缺少部署参数时校验失败。

配置结构、重复键、字段取值检查仍保留。运行中使用的配置/报价上下文指纹为派生业务数据，不是配置包 SHA256 校验；公开指纹不包含 API token。

路由器访问受保护的解析器时，在 `journal.source_auth_tokens` 中按 owner HTTP 基地址配置上游 token；它与路由器自身的 `api.auth_token` 独立。未配置的上游请求不发送 Authorization。集成工具会显式填入本次临时凭据。

正式服务入口为 `cmd/dex-router/main.rs`；测试资源统一在 `test/` 下，按 `contracts`、`fixtures`、`cmd` 分类（仅有调试程序的服务包含 `test/cmd`）。
