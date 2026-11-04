# chain-parser-rust — Rust区块解析服务

Solana来源会话/依赖屏障、skipped范围证明、限定CPMM账户布局、当前账户RPC基线、RocksDB持久状态和HEAD_FIRST。默认使用合成fixture；真实Yellowstone与执行后端未完成。

公共协议及 Go/Rust 生成类型统一位于相邻的 `../common/protoc`，服务构建只使用生成类型。`test/contracts` 仅保留本服务使用的测试向量；合成样本位于 `test/fixtures`，正式启动入口只使用 RPC / journal，不读取调试样本。

```bash
make check
make check-config
make run
# 或者
./target/debug/chain-parser-rust --config config/config-dev.yaml
```

Make会使用本项目锁文件。`make check`包含格式、静态检查、测试、契约和二进制构建；Go包含race检测。RocksDB需要C++与libclang；Rust固定1.90.0。配置校验失败在外部写入前终止；配置了 API 认证时，显式 token 至少24字符。PG DSN等部署凭据仍保留文件引用。

## 接口

公开诊断：GET /health/live、/health/startup、/health/ready、/metrics。配置了 `api.auth_token` 时，内部接口要求 Authorization: Bearer 凭据。可选凭据通过 `api.auth_token` 显式配置；为空时不启用该认证，不生成开发凭据文件。

GET /internal/chains/{chain}/baseline、/status、/journal?after={seq} 提供owner池状态和版本；链ID包含冒号，例如evm:31337。历史与在线namespace分离；本地旧cursor不能直接使服务ready。解析服务不生成路由/用户兑换量。

## 范围与证据

完整功能与未完成条件记录于上层 DEVELOPMENT_PROGRESS.md（唯一项目进度记录）。HTTP日志、重启和合成数据只能证明本地实现，不能替代Kafka事务、PG租约、真实链/协议数学、独立执行、多区HA及容量验收。

## 已接通的Kafka开发通道

配置完整的`control`与`journal.kafka`字段后，可选择`journal.transport=kafka`。两个解析器使用PG租约和Kafka stable transactional.id；本地prepared不可见，Kafka COMMIT成功后原子保存state/seq/cursor。路由器取owner `/internal/chains/{chain}/bootstrap` 的同批snapshot/cursor，使用独立replica identity读取完整固定分区，禁自动offset提交。超出有界尾窗时重新恢复当前checkpoint。

当前仅显式开发明文profile已验证；生产TLS/SASL、真实ISR/多区故障和完整reorg协议仍未验收。第7章逐条实现核对及专用基础设施报告位于上层docs/requirements与integration/artifacts/local。`VERIFIED`仍需独立执行后端，不因Kafka接通而开放。

## 环境配置与调试

`config` 只有 `config-dev.yaml`、`config-test.yaml`、`config-prod.yaml` 三个完整配置文件，不使用 manifest、配置版本、文件清单或文件摘要。`make run CONFIG=config/config-test.yaml` 可选择环境；`--check-config` 只校验所选文件，不连接数据源。

开发/测试配置使用本机 RPC 地址，需要配置对应链的 genesis、节点和池目录。生产文件是部署模板：需要填写真实 RPC、Kafka、控制面及认证参数。当前生产 TLS/SASL 能力仍受既有实现范围限制，生产模板缺少部署参数时校验失败。

配置结构、重复键、字段取值检查仍保留。运行中使用的配置/报价上下文指纹为派生业务数据，不是配置包 SHA256 校验；公开指纹不包含 API token。

显式离线调试：

```bash
make debug-build
./target/debug/fixture-parser-rust --config config/config-dev.yaml --fixtures test/fixtures
```

调试程序与正式服务二进制分离，不包含在运行镜像中；Rust 调试程序仅在启用 `debug-fixtures` feature 时构建。集成测试自行生成临时配置和凭据，并明确选择调试入口。

正式服务入口为 `cmd/chain-parser/main.rs`；测试资源统一在 `test/` 下，按 `contracts`、`fixtures`、`cmd` 分类（仅有调试程序的服务包含 `test/cmd`）。
