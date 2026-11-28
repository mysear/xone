# Xone 开发进度与完成情况

更新：2026-11-28。设计基线：[XONE_SYSTEM_DESIGN.md](XONE_SYSTEM_DESIGN.md) v2.3。

本文件只记录项目进度、已完成范围、剩余功能及验证结果。后续进度只更新本文件；测试原始数据、JSON报告和校验规则由集成工具维护，不再嵌入进度文档。

状态：`PARTIAL_LOCAL`为部分实现并完成本地验证，`LOCAL_SCOPE_VERIFIED`为注明范围通过本地验收，`NOT_STARTED`为未开始；只有完整设计验收通过才可记为`ACCEPTED`。截至阶段256，26 个工作包完整验收为 0，25 个为 `PARTIAL_LOCAL`，WP22 为 `NOT_STARTED`。生产准入关闭。VERIFIED 报价已通过配置启用的 Cancun 固定块单池本地验收。

优先级：区块解析和精确兑换量计算优先，分析功能低优先级。三服务各自独立，公共协议位于common。

导航：[2026-11-21 全范围完成度复核](#full-design-audit-2026-11-21) · [迭代目标](#iteration-goals-2026-11-27) · [剩余功能执行顺序](#remaining-execution-order-2026-11-18) · [最新差距复核](#design-gap-review-2026-11-18) · [本轮开发](#active-work) · [工作包](#work-packages) · [协议覆盖](#protocol-coverage) · [逐项功能](#design-traceability) · [已验证批次](#batch-history) · [剩余功能及优先级](#remaining-priorities)


<a id="full-design-audit-2026-11-21"></a>

## 全设计功能完成度与临时文件清理复核（2026-11-21）

**结论：没有全部完成。** 本次基于提交 `02252b4`，重新检查设计 §1–29、§28 的全部26个工作包、现有协议版本矩阵、正常入口源码和验收门禁。完整设计验收为 **0/26**：24个工作包为 `PARTIAL_LOCAL`，WP18、WP22 为 `NOT_STARTED`。这不是“没有实现功能”：三服务、许多协议数学、限定独立执行、快照/日志/重组恢复均已有可用实现；完整验收同时要求未覆盖功能、真实来源、故障域和上线证据，不可将局部通过外推全包完成。

本次是源码和交付证据审查，不重新执行全部历史数学/链上/生产压测，也不开发下表缺项。WP18的 `NOT_STARTED` 专指真实第二/第三链完整交付及其验收；七条 EVM 链已有原生承诺规则，不能把该状态解释为没有这些链的代码。WP22已有本地优化器，其未启动范围是外部来源/多求解器竞争等高级交付。

|工作包|本次结论|已具备的基础|仍缺功能或验收 / 当前源码依据|
|---|---|---|---|
|WP01|部分完成，当前门禁失败|三服务独立构建、工具链、Make、Dockerfile、公共契约|没有落地的 CI/供应链扫描链路；`contracts.py` 实测因 [Go测试](chain-parser-go/internal/provenance/beacon_fork_handover_test.go) 读取 `../../../integration/` 拒绝，见 [契约检查](integration/tools/ci/contracts.py)|
|WP02|部分完成|金额/链身份/快照类型、Protobuf、副本一致性|全链规范化和跨发布 schema 兼容性尚缺；[生成检查](common/protoc/check.py)只比较当前生成结果|
|WP03|部分完成|大量原始合约/程序、历史主网来源及独立重放|全版本真实分叉、完整bank、独立收付款与恢复的统一矩阵不足；[原始来源](integration/test/source)、[现有验收](integration/tools/acceptance)|
|WP04|部分完成|provider身份、预算、重试、原生承诺、以太坊信标签名来源|独立故障域和其他链最终性未闭合；[最终性配置](chain-parser-go/internal/config/beacon_finality.go)分别限定 evm:1 和 evm:10|
|WP05|部分完成|完整交易/回执、MPT、归档后解码、串行提交和OP批次/存款核验|OP完整执行后状态、真实跨提交通道、Solana可信bank、更多事实/版本；[OP span](chain-parser-go/internal/chain/evm/op_span.go)、[Solana流权威边界](chain-parser-rust/crates/solana-ingest/src/stream.rs)|
|WP06|部分完成|Kafka事务、read_committed、固定分区及不确定结果处理|生产TLS/SASL/ACL、ISR/迁移故障、完整控制事件集；[帧事件](chain-parser-go/internal/journal/frame.go)、[生产连接校验](chain-parser-go/internal/config/config.go)|
|WP07|部分完成|PG server-clock租约、writer fencing、整体重组和撤权|通用独立BLOCK_REVERT等记录、真实多链fork/PG切换/区域演练；[重组](chain-parser-go/internal/reducer/reorg.go)、[副本事件处理](dex-router/crates/state-replica/src/lib.rs)|
|WP08|部分完成|manifest/校验/不可变上传、Go维护回收、有界checkpoint恢复|Rust仅只读retention计划、单对象大小限制、独立恢复目录/分片；[Rust只读计划](chain-parser-rust/crates/solana-ingest/src/service_checkpoint.rs)、[单文件恢复](dex-router/crates/kafka-transport/src/checkpoint.rs)|
|WP09|部分完成|不可变视图、完整屏障、图结构部分共享|Pool/顶层索引完整增量COW、慢请求持有旧版本的总内存预算；[QuoteView和Replica](dex-router/crates/state-replica/src/lib.rs)|
|WP10|部分完成|V2完整交换/余额/储备和限定标准Token执行|税费/rebase/上下文Token行为、全来源与执行模式验收；[Token准入](dex-router/crates/domain/src/lib.rs)、[V2绑定](dex-router/crates/execution-plan/src/bound_v2.rs)|
|WP11|部分完成|V3/Curve/Balancer/V4等独立profile与数学差分|版本/ExactOut/执行能力不等价；任意hook、完整Vault/BPT义务、未覆盖组合；[能力目录](dex-router/crates/protocol-math/src/capabilities.rs)、[ExactOut准入](dex-router/crates/protocol-math/src/exact_out.rs)|
|WP12|部分完成|金额/拆分探针排序、有界候选、图的局部更新|beam/多标签、严格上界、小图oracle和大图容量；[候选搜索](dex-router/crates/routing/src/lib.rs)|
|WP13|部分完成|最多8路整数网格、独立池DP、共享物理池顺序重算、金额守恒|通用共享DAG、完整可信费用净评分、网格外gap；[优化搜索](dex-router/crates/optimizer/src/search.rs)、[费用评分](dex-router/crates/optimizer/src/cost_scoring.rs)|
|WP14|部分完成|V2单路线至8跳准备、部分本地split executor、Solana单池钱包闭环|其他协议/多池/共享DAG执行器、完整EVM钱包签名发送回执；[EVM准备](dex-router/crates/quote-api/src/evm_preparation.rs)、[Solana准备](dex-router/crates/quote-api/src/preparation.rs)|
|WP15|部分完成|严格缺状态拒绝的REVM/SBF、取消/期限/子进程预算、限定真实余额差|可信当前完整bank、全链规则/全协议/复杂计划仿真；[通用verify明确不可用](dex-router/crates/simulation/src/lib.rs)、[本地EVM后端](dex-router/crates/quote-api/src/local_simulation.rs)|
|WP16|部分完成|ExactIn/Out、认证、期限、缓存、single-flight、版本/新鲜度检查|VERIFIED被主动拒绝、全局单token额度不等于租户隔离、statePolicy/allowSources及完整API未齐；[API入口](dex-router/crates/quote-api/src/lib.rs)、[ExactOut](dex-router/crates/quote-api/src/exact_out.rs)|
|WP17|部分完成|health/startup/链状态、SIGTERM、日志指标和开发compose|真实多AZ/PG/Kafka故障、灰度、监控/OTel与批准SLO；[开发compose](integration/compose/compose.yaml)、[单机基础设施](integration/tools/acceptance/infrastructure.py)|
|WP18|完整交付未启动/未验收|七链原生承诺规则、多链配置与部分公开链样本|第二/第三真实链全闭环及费用/版本/恢复/上线验收；阶段242–243已补[Go](chain-parser-go/internal/service/startup.go)与[Rust](chain-parser-rust/crates/solana-ingest/src/service_startup.rs)失败链启动隔离，慢链并发调度仍缺|
|WP19|部分完成|Go analytics有限Job/只读连续worker、cursor/幂等/失败checkpoint|专属DLQ/raw重放、多插件配额、ClickHouse；Rust analytics显式未启用；[Go worker](chain-parser-go/internal/analysis/worker.go)、[Rust角色](chain-parser-rust/crates/solana-ingest/src/service.rs)|
|WP20|部分完成|Solana tonic/流WAL/账户屏障、多种AMM数学和限定SBF/钱包链路|真实同源连续流/fork/完整bank、程序版本与扩展交叉、多池原子交易、生产CU/锁/负载；[流来源](chain-parser-rust/crates/solana-ingest/src/stream.rs)、[SBF披露](dex-router/crates/simulation/src/solana_sbf.rs)|
|WP21|部分完成|本地candles/1、精确OHLC、撤销重算、revision/水位/原子checkpoint|HEAD_PROVISIONAL不等于正式发布；ClickHouse/shadow/发布指针/查询API、迟到与decoder升级；[K线日志](chain-parser-go/internal/analysis/journal.go)、[本地持久化](chain-parser-go/internal/analysis/job.go)|
|WP22|外部来源竞争未启动|本地有限范围启发式、网格/共享状态/可选费用评分|RFQ/聚合器、多独立solver、有效期/调用者约束、通用上界与收益分布；[搜索边界](dex-router/crates/optimizer/src/search.rs)、[报价披露](dex-router/crates/quote-api/src/lib.rs)|
|WP23|部分完成|本地重启/接管/无KV恢复及阻断门禁|生产shadow/逐租户放量、区域灾备/切回、真实RPO/RTO/回滚；[生产门禁](integration/tools/acceptance/gate.py)、[生产评审](integration/tools/acceptance/report.py)|
|WP24|部分完成|HEAD_FIRST、当前基线/有限尾窗、历史故障不阻塞当前链|1h/1d/30d真实停机、范围扩展与资源全矩阵、慢链调度隔离、首个VERIFIED报价；[历史隔离](chain-parser-go/internal/service/history_lifecycle.go)、[恢复目录](dex-router/crates/quote-api/src/remote_checkpoint.rs)|
|WP25|部分完成|严格版本化文件、ConfigProvider、策略hash与secret文件权限|生产Kafka TLS/SASL当前配置不支持；secret修订/轮换、完整schema兼容与配置中心物化；[Go配置](chain-parser-go/internal/config/config.go)、[Rust配置](chain-parser-rust/crates/config/src/lib.rs)|
|WP26|部分完成|Go/Rust结构化JSON、脱敏、有界队列、drop计数与部分事务审计|SDK故障映射、轮转/spool/集中采集/OTel、全控制审计和生产运行手册；[现有日志手册](integration/runbooks/log-investigation.md)|

**检查结果与边界：** 本轮 `test_progress_document.py` 的17项通过，`test_design_delivery_audit.py` 的6项通过；`integration/tools/ci/contracts.py` 失败，失败发生在清理前，不能称当前统一门禁全部通过。为避免制造新的构建缓存，本轮没有执行会重建大型 target 的完整设计交付 runner，也没有重新跑全部历史Go/Rust测试。上一轮阶段238–239的测试和主网归档结果保留为历史证据，不能自动视为本轮全设计验收。源码观察是“实现缺项”，真实bank/来源能力及部署/故障域不足是“证据或环境缺项”，二者分别保留，不能统称为外部阻塞。

**清理记录：** 删除169个未跟踪且无打开文件的路径：3个Rust构建缓存目录、136个Python缓存目录、24个闲置的本项目测试可执行文件和6个`.DS_Store`。按占用块统计释放17581318144字节（约16.37GiB）；清理前后2617个Git跟踪文件逐字节哈希一致。保留源码、锁文件、测试fixtures、固定上游源码、全部原始RPC/流归档、验收报告/日志、基础设施数据与恢复状态。清理不处理项目以外的全局缓存。

另按同一契约规则完整扫描，发现4个测试文件中的5处跨项目路径，而不是只有首个失败文件：[路径清单](integration/artifacts/local/project-audit-20261121/cross-project-paths.json)。本轮修正文档中“最多两路”这一过时摘要，实际当前优化器上限为8路；现有62个登记协议profile仍须分别看ExactIn/ExactOut/准备/仿真和来源资格，不能当作62个全功能上线版本。

[清理完整清单](integration/artifacts/local/project-audit-20261121/cleanup.json) · [本次完成度核查记录](integration/artifacts/local/project-audit-20261121/completeness.json)

用户随后明确授权清理测试结果，本轮追加删除6678个未跟踪输出路径（378个测试运行/状态目录和6300个结果或日志文件），包括已停用的测试专用PG/Kafka数据、历史报告、临时owner状态和RPC录制。追加清理按占用块计52625006592字节（约49.01GiB）；两步累计70206324736字节（约65.38GiB）。第二步清理前后2620个跟踪文件内容一致，正式回归夹具、仍被正常测试引用的输入、参考源码及Solana/SBF运行时保留。历史阶段记录保持原文，其中未跟踪旧报告/录制的本地链接可能因本次授权清理而失效；重放这些旧验收需要重建前置输出，不能据此声称旧记录已在清理后重新复验。已提交的小型审计记录保留。

[测试结果清理范围与统计](integration/artifacts/local/project-audit-20261121/test-results-cleanup.json)

清理后将398个已失效的报告链接改为“已清理”的历史路径说明；进度读取器同时识别该说明，仍按原source/test/evidence绑定校验，不虚构已删除报告。重新运行17项进度检查及6项审计控制均通过，有效本地文档链接缺失为0。五处跨项目测试路径的原有契约失败继续保留；本次仅调整文档维护工具，没有修改三服务业务实现。

<a id="active-work"></a>

## 本轮开发

|工作项|设计条款 / 工作包|状态|已完成|剩余 / 验证|
|---|---|---|---|---|
|I8 标准 V2 共享后缀费用对比（2026-11-28）|§13.6、§14.2、§15；WP13、WP15、WP16|LOCAL_SCOPE_VERIFIED|在同一快照、调用者、估值版本和模拟预算内重放有序拆单与共享后缀。20 项 HTTP 场景通过，独立重算 ExactIn 和 ExactOut 净额并验证预算耗尽、过期估值及执行器缺失|报价披露本地 EVM 执行费对比。完整链费、资金约束和正常报价选择待验收|
|I7 自建 Yellowstone 连续流（2026-11-28）|§7、§18、§28.22；WP04、WP05、WP07、WP24|LOCAL_SCOPE_VERIFIED|正式 Parser 验证同会话连续 finalized bank、真实账户写入、断线后的实际留存补洞及同账本节点重启。留存流从 slot 49 恢复并逐槽提交。完整联测与 26 项 Parser、传输回归通过|继续验收生产独立故障域|
|I8 标准 V2 共享后缀交易模板（2026-11-28）|§14.2–14.5、§15；WP15、WP16|LOCAL_SCOPE_VERIFIED|正常构建使用报价候选哈希触发独立 REVM，校验快照、授权和成交，并将 calldata、候选、绑定和仿真证据纳入构建摘要。ExactIn、ExactOut 各通过 6 项 HTTP 场景，覆盖成功、候选替换、缺少候选选择和过期|模板证据为 `PROVIDED_LOCAL_STATE`。实时状态、完整链费及候选排序待验收|
|I8 标准 V2 共享后缀 ExactOut 执行（2026-11-28）|§13.5–13.6、§14.2、§15；WP14、WP15|LOCAL_SCOPE_VERIFIED|按热储备执行两种分支顺序并一次结算共同后缀。原版 Router02 对照覆盖精确到账、调用者扣款与 6 次失败回滚。独立 REVM 校验两笔出块交易的 gas、全部日志与声明的账户后状态。ExactIn 和 ExactOut 各通过两个 HTTP 仿真场景，含候选哈希替换拒绝|证据权威为 `PROVIDED_LOCAL_STATE`。正常构建和同快照费用排序待接入|
|I8 标准 V2 共享后缀 ExactOut 候选（2026-11-28）|§13.4–13.6、§14.1、WP13、WP14|LOCAL_SCOPE_VERIFIED|正常 ExactOut 报价从目标到账反推共同后缀的输入。按原分支比例分配整数中间目标，按物理池顺序更新前缀储备，核对精确到账和输入预算。16 组 ExactIn 和 ExactOut HTTP 回归通过，含独立整数核算和候选哈希核对|继续绑定 ExactOut 执行、最大输入回滚与候选排序|
|I8 标准 V2 共享后缀 HTTP 仿真（2026-11-28）|§13.6、§14.2、§15、WP15|LOCAL_SCOPE_VERIFIED|配置共享后缀执行器后，正常报价的候选哈希可提交到仿真端点。服务端重建候选并核对快照、交易、费用与授权。独立整数重放匹配实际到账，替换候选哈希被拒绝。两项定向 HTTP 和原有 16 组拆单 HTTP 回归通过|执行证据权威为 PROVIDED_LOCAL_STATE。正常构建和候选竞争待接入|
|I8 标准 V2 共享后缀仿真（2026-11-28）|§13.6、§14.2、WP15|LOCAL_SCOPE_VERIFIED|独立 REVM worker 重建绑定，核对合约字节码、Router02 身份、Swap 日志顺序、接收者到账、调用者扣款和执行器代币余额。两笔已出块交易的 gas、日志和所有声明的账户后状态匹配，代码替换被拒绝。原有四种有序拆单场景通过回归|共享后缀的常规报价筛选、构建和净额排序待接入|
|I8 标准 V2 共享后缀候选（2026-11-28）|§13.4–13.6、§14.1、WP13、WP14|LOCAL_SCOPE_VERIFIED|正常 ExactIn 报价在私有快照重放分支，共同后缀一次消耗全部中间输出。报价披露资金依赖、同一物理池的状态依赖和绑定原计划的候选哈希。16 组正常 HTTP 验收通过，含独立整数重放和哈希核对|候选限于标准 V2 和与分支无物理池重叠的后缀。执行继续使用原有顺序拆单。共享步骤的 calldata、独立执行和候选排序待验收。 [共享后缀](dex-router/crates/optimizer/src/shared_v2.rs)|
|I8 标准 V2 共享后缀执行绑定（2026-11-28）|§13.5–13.6、§14.1、WP14、WP15|LOCAL_SCOPE_VERIFIED|ExactIn 合并执行器绑定同一快照的原始计划、资金流、物理池顺序和交易。两种分支顺序的原版 Router02 对照核对了 calldata、池储备、调用者扣款和接收者到账。8 次失败交易确认错误输入、minOut 及过期均回滚，并保留执行器原有代币余额。Rust 绑定测试拒绝修改后的 calldata 和过期来源状态|继续接入独立仿真、正常报价构建和费用排序。 [执行绑定](dex-router/crates/execution-plan/src/ordered_v2/shared_suffix.rs)|
|I8 标准 V2 路径金额界（2026-11-28）|§13.1–13.4；WP12、WP13|LOCAL_SCOPE_VERIFIED|同一快照中已保留的标准 Uniswap V2 路径，按费后边际兑换率给出 ExactIn 毛输出上界和 ExactOut 所需输入下界。报价披露范围、候选路径数和与所选计划的差距。通过两项单测、16 组 HTTP 独立有理数核对、30 次原版交易重放和 16 组共享池 HTTP 回归|共享池须方向一致，余额须与储备一致。继续验证其他池型和通用共享 DAG。 [金额界实现](dex-router/crates/optimizer/src/constant_product_bound.rs)|
|I8 ExactIn 多标签搜索（2026-11-28）|§13.2–13.4；WP12、WP13|LOCAL_SCOPE_VERIFIED|正向搜索按币种和跳数保留多个物理前缀，用全单和拆分输入的可用报价比较同币种输出率。正常报价绑定搜索版本，返回标签裁剪和前缀计算次数。两项小图穷举对照、16 组候选费用 HTTP、30 次原版交易重放和 16 组共享池 HTTP 报价通过|严格收益上界和通用共享 DAG 待完成。 [搜索实现](dex-router/crates/routing/src/lib.rs)|
|I8 ExactOut 多标签搜索（2026-11-28）|§13.2–13.4；WP12、WP13|LOCAL_SCOPE_VERIFIED|反向搜索按币种和跳数保留多个物理后缀，用目标金额对应的输入率排序。报价披露标签上限和裁剪数，有界测试与小图穷举的最优路径一致。已通过 16 组候选费用 HTTP、30 次原版交易重放和 16 组共享池 HTTP 报价|通用共享 DAG 和严格收益上界待完成。 [搜索实现](dex-router/crates/routing/src/exact_out.rs)|
|I8 OP 已签名交易费用证据（2026-11-28）|§13.1、§14、§15.3；WP13、WP16|LOCAL_SCOPE_VERIFIED|签名执行返回 gas 费、L1 数据费、operator fee、总收费和付款人实际净扣款。费用证据绑定签名信封、交易哈希、回执和来源状态。Regolith 至 Karst 的 36 组正常 HTTP 执行与原版客户端逐笔对比，216 个无效请求被拒绝|报价阶段仍缺少实际签名信封，需要继续完成候选费用覆盖。 [OP HTTP 复验](integration/tools/acceptance/op_api_http.py)|
|I8 候选费用排序结果（2026-11-28）|§13.1、§15.3；WP13、WP16|LOCAL_SCOPE_VERIFIED|ExactIn 和 ExactOut 在发布报价时复算按已审计费用调整的排序值，校验计划、原子单位金额、价格簿及发布代次。费用参与排序时返回币种、带符号原子单位值和费用范围。16 组正常 HTTP 场景与 30 次原版交易重放通过，覆盖负值、超宽输入、预算截断和缺少有效估值|结果标记为 `LOCAL_EVM_ONLY`，完整链费净额待验收。本机库测试编译缺少生成语料。 [HTTP 复验](integration/tools/acceptance/candidate_cost_http.py)|
|I7-R03 Yellowstone 回放保留边界（2026-11-28）|§9.23、§18.7、§23.14|LOCAL_SCOPE_VERIFIED|正常 finalized 流在有历史游标时查询同连接的 `SubscribeReplayInfo`。当所需 slot 早于服务端保留窗口时直接订阅 HEAD_FIRST。流状态记录实际请求和服务端声明的最早回放 slot。44 项定向测试通过。正常 Parser、RocksDB 和实际 PostgreSQL/Kafka 复验覆盖范围发布、撤销、重启、fencing 及有界历史缺口|gRPC 来源为受控服务。真实 Yellowstone 连续流和同源历史 bank 验收待完成。 [传输入口](chain-parser-rust/crates/solana-ingest/src/yellowstone.rs)|
|I7-R04.2 Docker 检查点网络分区（2026-11-28）|§9.23、§18.7、§23.14、§28.26|LOCAL_SCOPE_VERIFIED|Go owner、两个报价容器和实际 PostgreSQL/Kafka 联验。断开 owner 链路后，冷副本通过 peer 在 978 毫秒恢复合格范围，980 毫秒返回 INDICATIVE。断开两条检查点链路时冷副本返回 503。恢复 owner 链路后，227 毫秒取得合格范围，229 毫秒返回相同快照报价。Kafka 网络持续连通|单宿主机隔离网络与受控 EVM RPC 已验收。生产独立故障域与长停机演练继续验收。 [复验工具](integration/tools/acceptance/r04_network_partition.py)|
|I7-R04.2 peer 来源选择与拒绝（2026-11-28）|§9.23、§18.7、§23.14|LOCAL_SCOPE_VERIFIED|并发请求配置的 peer，串行核验 Kafka 提交并激活已对齐的范围。实际 Kafka 上四项恢复集成通过，覆盖首个 peer 挂起、过期、错链、owner 离线和撤销。正常 Go owner 和双报价进程复验在 347 毫秒恢复合格范围、350 毫秒返回 INDICATIVE，正常撤销及全部来源离线时返回 503|本机 PostgreSQL 和 Kafka 联验已完成。生产独立故障域、长期容量、真实同源流仍待验收。 [Kafka 集成](dex-router/crates/quote-api/tests/kafka_retention.rs)|
|I7-R04.2 Cancun 按请求读取仿真状态（2026-11-28）|§9.25、§14、§23.14|LOCAL_SCOPE_VERIFIED|启用 `simulation.evm_cancun_verified_rpc_state` 后，按报价快照读取规范 H 和 H+1 区块头，验证模板声明的账户和存储证明，以链上余额、nonce 和存储值构建本次隔离 REVM 输入。正常 Go owner、PostgreSQL 和 Kafka 联验使用高度 1 的旧模板，在高度 2 恢复后 1,141 毫秒取得 VERIFIED，消耗 gas 209,487，到账 982,981。缺少 PoolManager 存储的模板返回 HTTP 422，9 项配置测试通过|目前支持 Cancun 单池 ERC-20，要求已有 H+1 及配置的账户、代码和存储位置。生产故障域、自动发现完整读取集合、其他执行范围待验收。 [复验工具](integration/tools/acceptance/r04_verified_recovery_process.py)|
|I7-R04.2 VERIFIED 缓存与构建的区块复查（2026-11-28）|§10、§14、§15|LOCAL_SCOPE_VERIFIED|按报价证据复查源块和执行块，并重新计算区块头哈希。Geth 24 组 HTTP 回放覆盖缓存命中、源块和执行块改变后拒绝报价，以及执行块改变后拒绝构建|动态读取的证据继续按同一路径复查。 [复验工具](dex-router/crates/quote-api/examples/singleton_replay.rs)|
|I7-R04.2 原生节点仿真超时与重试（2026-11-28）|§9.26、§14.4、§23.14|LOCAL_SCOPE_VERIFIED|正常 Go owner、PostgreSQL、Kafka 和冷报价副本联验。代理确认收到首个 `eth_simulateV1` 后延迟响应，请求在 15,003 毫秒返回 HTTP 504 `BUDGET_EXHAUSTED`，两项首个 VERIFIED 指标均为空。重试同一请求和快照取得 VERIFIED，支付 1,000,000，到账 982,981，消耗 gas 209,487。冷启动到首个成功 VERIFIED 报价为 16,119 毫秒|请求级动态源状态、真实长停机及独立故障域待验收。 [复验工具](integration/tools/acceptance/r04_verified_recovery_process.py)|
|I7-R04.2 正常进程冷启动 VERIFIED 报价（2026-11-28）|§9.26、§18.7、§23.14、§28.26|LOCAL_SCOPE_VERIFIED|正常 Go owner 从受控 Cancun Geth 读取 Uniswap V4 单池，经实际 PostgreSQL 和 Kafka 发布高度 2、游标 3、代次 1 的检查点。停止初始报价进程后，冷副本在 429 毫秒恢复合格范围、434 毫秒返回首个 INDICATIVE、1,057 毫秒返回首个 VERIFIED。报价进程内部计时为 759 毫秒。REVM 和 Geth 确认支付 1,000,000、到账 982,981、消耗 gas 209,487|固定块冷启动已验收。验证超时、动态源状态、真实长停机和独立故障域仍待验收。 [复验工具](integration/tools/acceptance/r04_verified_recovery_process.py)|
|I7-R04.2 Cancun 固定块 VERIFIED 报价（2026-11-28）|§9.26、§14、§15、§23.14；WP15、WP16、WP24|LOCAL_SCOPE_VERIFIED|显式配置 `simulation.evm_cancun_verified_chain` 后，ExactIn 和 ExactOut 对单池 ERC-20 计划运行隔离 REVM 和 Geth `eth_simulateV1`。验证规范源块、账户及存储证明，比较成功状态、gas、日志和前后代币余额，并在缓存命中和交易构建时复查执行块。Geth 中间交易、末笔交易各通过 24 个回放场景，各含 6 个 ExactIn 和 6 个 ExactOut 的 VERIFIED 报价，覆盖 gas 和金额篡改、重组后缓存及构建拒绝、首个 VERIFIED 计时。Anvil 的 24 个签名回执和 4 项真实 Kafka 恢复回归通过|现行配置需要固定 H/H+1、受支持的单池执行器和 Cancun Geth。动态源状态、Go owner 与 Kafka 正常进程测得的首个 VERIFIED 报价、其他链与多池执行待验收。 [复验工具](dex-router/crates/quote-api/examples/singleton_replay.rs)|
|I7-R04.2 报价进程验证计时（2026-11-28）|§9.26、§23.14、§28.26|LOCAL_SCOPE_VERIFIED|报价进程分别记录首个可用报价和首个成功 VERIFIED 报价，链状态返回对应代次。正常 Go owner、报价进程和实际 PostgreSQL/Kafka 演练停机 20,009 毫秒，重启 owner 后 4,339 毫秒收到 INDICATIVE 报价。VERIFIED 请求返回 HTTP 422，两项 VERIFIED 进程指标为空|真实独立仿真通过后的正向计时待验收。 [复验工具](integration/tools/acceptance/r01_r04_recovery_process.py)|
|I7-R04.2 EVM 仿真源区块校验（2026-11-28）|§14.2、§23.14|LOCAL_SCOPE_VERIFIED|回执交叉校验按规范高度读取源区块，核对本地锚点声明的基础费，再验证账户和存储证明。24 个由 Anvil Cancun 节点出块的 HTTP 回归通过，包括源高度失去规范块时拒绝回执证据|正向报价仍需在同一快照绑定原生节点仿真与独立执行。 [状态验证](dex-router/crates/quote-api/src/evm_receipt/source_state.rs)|
|I7-R04.2 Go owner 退出后的副本接力（2026-11-28）|§9.23、§18.7、§23.14|LOCAL_SCOPE_VERIFIED|正常 Go owner、两个报价进程、本机 PostgreSQL 16 和 Kafka 4.2 完成 `--peer-recovery`。正常关闭发布撤销，冷报价返回 503。owner 重新发布后模拟进程意外退出，冷副本通过健康副本及 Kafka 在 351 毫秒恢复高度 400 的第 2 代范围，在 353 毫秒返回首个 INDICATIVE 报价。游标为 11，历史缺口为 101–399。两个来源均停止后，冷报价返回 503|受控单池和本机中间件。生产故障域与 VERIFIED 报价仍需验收。 [复验工具](integration/tools/acceptance/r01_r04_recovery_process.py)|
|I7-R04.2 报价副本提供恢复检查点（2026-11-28）|§9.23、§18.7、§23.14；WP09、WP24|LOCAL_SCOPE_VERIFIED|健康副本在认证端点提供已应用的规范引用与游标。恢复副本从 Kafka 核验该游标上的完整提交，完成有界尾窗对齐后开放报价。实际 Kafka 4.2 上验证 owner 离线时的冷恢复、报价、继续跟块和撤销后拒绝。四项 Kafka 集成测试与六项检查点配置测试通过|通过 `journal.checkpoint_peer_urls` 和对应的 `source_auth_tokens` 明确配置副本。独立故障域仍需验收。 [副本恢复入口](dex-router/crates/quote-api/src/peer_checkpoint.rs)|
|I7-R04.2 实际停机 1 小时（2026-11-28）|§18.7、§23.14、§28.26|LOCAL_SCOPE_VERIFIED|Go owner 和两个报价进程同时停止 3,600,015 毫秒。Kafka 低水位由 0 提升到 8，旧游标 3 失效。owner 第 2 代在高度 400、游标 11 发布，3,948 毫秒恢复范围，3,957 毫秒收到首个 INDICATIVE 报价。随后同步到高度 401 并继续披露 101–399 的历史缺口|受控单池与实际本机 PostgreSQL/Kafka。运行器固定于 `c73e9a7`，本地报告 `r04-retention-3600s-wallclock.json` 的 SHA-256 为 `48f783df192ff3045b393ddebaf18ba714523c58da6f20d0ddbfe54216fd92e4`。该次演练只请求了 FAST。|
|I7-R04.2 留存恢复的独立报价探针（2026-11-28）|§9.26、§23.14、§28.26|LOCAL_SCOPE_VERIFIED|留存演练分别请求 FAST 和 VERIFIED。正常进程联验实测停机 20,008 毫秒，重启 owner 后 4,439 毫秒完成范围恢复，4,442 毫秒得到 INDICATIVE 报价。VERIFIED 请求返回 HTTP 422，错误为 `INDEPENDENT_SIMULATION_UNAVAILABLE`|首个 VERIFIED 报价耗时为空，独立报价路径仍需验收。 [复验工具](integration/tools/acceptance/r01_r04_recovery_process.py)|
|I7-R04.2 Kafka 中断后恢复已应用范围（2026-11-28）|§9.23、§18.7、§23.14|LOCAL_SCOPE_VERIFIED|从同一报价副本已应用的快照和游标对齐当前 Kafka 水位后恢复报价。中断前已应用的重组屏障继续等候提交。应用失败时清除本地恢复游标并请求来源检查点。隔离 Kafka 4.2 上两项定向测试和三项集成测试通过|读取中断由测试重置读取器并暂停视图。独立故障域仍需联验。 [报价恢复](dex-router/crates/quote-api/src/lib.rs)|
|I7-R04.3 Go 历史隔离回归（2026-11-28）|§18、§23.14、§28.26；WP08、WP20、WP24|LOCAL_SCOPE_VERIFIED|补洞队列、历史 owner 凭据和重组回归使用测试内构造的 V2 快照。Go service 与 reducer 定向 race 检查通过，覆盖在线先就绪、历史缺口独立登记、队列重开、历史权限和重组范围更新|真实来源与长期资源占用仍需验收。 [历史生命周期测试](chain-parser-go/internal/service/history_lifecycle_test.go)|
|I7-R04.2 Go 备用进程接管（2026-11-28）|§18.6、§18.7、§23.14|LOCAL_SCOPE_VERIFIED|两个独立 Go 进程使用各自的本地状态目录，连接实际 PostgreSQL 16 和 Kafka 4.2。主进程持有租约时，备用进程未发布状态。停止主进程后，备用进程以新 writer epoch 在高度 400 发布第 2 代，3,732 毫秒恢复合格范围，3,911 毫秒恢复冷启动副本的首个 INDICATIVE 报价。持续运行的报价副本亦收敛到该引用|同台机器的受控 RPC 联测，跨故障域和首个 VERIFIED 报价待验收。 [复验工具](integration/tools/acceptance/r01_r04_recovery_process.py)|
|I7-R04.3 Go 历史队列重复绑定容量（2026-11-28）|§18、§23.14、§28.26；WP08、WP20、WP24|LOCAL_SCOPE_VERIFIED|回到已有 dataset/revision 后，重复事实及候选共用已持久字节。冲突内容被拒绝，归档 manifest 只按大小净变化计费。八轮绑定往返及 Pebble 重开后容量与覆盖保持一致。队列定向 race 检查和五项运行时检查通过|依赖 EVM baseline fixture 的既有测试仍缺输入。真实长期容量、留存和跨故障域验收继续推进。 [历史队列测试](chain-parser-go/internal/reducer/history_queue_test.go)|
|I7-R04.2 长停机演练资源隔离（2026-11-28）|§23.14、§28.26|LOCAL_SCOPE_VERIFIED|留存联测只创建所需 Kafka topic。验收支持明确选择本机 Kafka 和 JDK，限制 broker 与 CLI 堆内存，容器按固定运行名称管理。实际 PostgreSQL 16、Kafka 4.2 和正常 owner/报价进程的计时烟测通过，实测停机 20,009 毫秒，重启 owner 后 4,276 毫秒收到首个 INDICATIVE 报价|本机受控单池和实际中间件。长停机场景分别记录实际结果。 [复验工具](integration/tools/acceptance/r01_r04_recovery_process.py)|
|I7-R04.2 报价副本按链恢复（2026-11-28）|§18.7、§23.14、§28.26|LOCAL_SCOPE_VERIFIED|正常报价进程为每条链独立轮询来源，Kafka 读取期间释放全局读取器表锁。实际 Kafka 4.2 回归确认 Solana 基线请求挂起时 EVM 可恢复首个报价并应用下一块；同一测试集的过期游标、远端检查点与撤销回归均通过|受控 HTTP owner 和本机 Kafka，生产故障域仍待验收。 [Kafka 回归](dex-router/crates/quote-api/tests/kafka_retention.rs)|
|I7-R04.2 EVM 当前基线切换（2026-11-28）|§18.7、§23.14、§28.26|LOCAL_SCOPE_VERIFIED|普通 EVM RPC 的移动头遇到陈旧主源时，按链 ID、创世块、时间和固定高度锚点校验备用源。正常 Go owner 和报价进程在隔离 PostgreSQL/Kafka 上通过留存恢复，旧游标 3 低于低水位 8，主源返回 6 次陈旧头，备用源恢复至高度 400、游标 11，3,993 毫秒得到首个合格范围，3,997 毫秒得到首个 FAST 请求的 INDICATIVE 报价，随后同步高度 401 并保留历史缺口 101–399|受控 RPC 与实际本机中间件。独立故障域、长期停机和首个 VERIFIED 报价待验收。 [复验工具](integration/tools/acceptance/r01_r04_recovery_process.py)|
|I7-R04.2 Solana 当前基线切换（2026-11-28）|§18、§23.14、§28.26；WP08、WP20、WP24|LOCAL_SCOPE_VERIFIED|HEAD_FIRST 在接受 RPC 基线前检查时间，遇到陈旧基线继续检查后续端点。WAL 保留原始响应，离线重放只选择通过检查的基线。启动隔离套件 9 项通过|2 项需要独立 PostgreSQL/Kafka 的测试未运行。真实可信 peer 和实际长停机验收继续推进|
|I7-R04.2 Kafka 冷启动重锚定（2026-11-28）|§18.7、§23.14、§28.10、§28.26；WP08、WP09、WP24|LOCAL_SCOPE_VERIFIED|实际 Kafka DeleteRecords 使低水位从 0 推进至 8，旧游标 3 失效。空尾部、撤销、重组开始、不完整批次和无 owner 背书的基线均保持不可用。收到 owner 当前快照及其提交游标后，空内存报价副本首个合格范围 70 毫秒、首个 FAST 报价 73 毫秒。历史缺口在报价中披露并在正常区块更新后保留|使用受控 owner HTTP 和隔离单 broker。真实停机、跨故障域及首个 VERIFIED 报价仍待验收。 [回归测试](dex-router/crates/quote-api/tests/kafka_retention.rs)|
|I7-R04.2 正常进程 Kafka 留存丢失恢复（2026-11-28）|§18.7、§23.14、§28.26；WP08、WP09、WP24|LOCAL_SCOPE_VERIFIED|正常 Go owner 和报价进程在独立 PostgreSQL/Kafka 上重启。低水位从 0 推进至 8，游标 3 失效。无 owner 时冷副本返回 503。owner 在高度 400 发布第 2 代、游标 11，约 3.40 秒恢复首个合格范围和 FAST 报价，随后同步到高度 401 并继续披露 101–399 的历史缺口|受控单池 RPC、空区块和等效高度差。历史任务暂停。跨故障域和实际长停机待验证。 [复验工具](integration/tools/acceptance/r01_r04_recovery_process.py)|
|I7-R04.2 报价冷启动对齐 Kafka 有限尾窗（2026-11-28）|§18.7、§23.14；WP08、WP09、WP24|LOCAL_SCOPE_VERIFIED|到达读取器初始化时记录的 Kafka 高水位并完成当前批次后才激活报价视图。独立 Kafka 验证了检查点后的连续区块应用、撤销、重组开始及半批次拒绝。正常 Go owner 与报价进程的留存恢复复验约 3.33 秒出现首个 FAST 报价|公开远端检查点路径共用尾窗检查，原联测编译仍缺 EVM baseline fixture。跨故障域和实际长停机待验证。 [实际 Kafka 回归](dex-router/crates/quote-api/tests/kafka_retention.rs)|
|I7-R04.2 公开检查点与多来源恢复（2026-11-28）|§18.7、§23.14；WP08、WP09、WP24|LOCAL_SCOPE_VERIFIED|实际 Kafka 回归从不可达、陈旧及错误链来源切换到配置的 owner 与远端对象服务。不同凭证分别用于读取目录和快照。先重放到目录下界，再应用其后的完整区块。目录之后已发生的撤销使冷副本保持不可用|HTTP 来源使用本机受控端点。真实故障域与长期停机仍待验收。 [回归测试](dex-router/crates/quote-api/tests/kafka_retention.rs)|
|I7-R04.2 停机计时联测入口（2026-11-28）|§23.14、§28.26；WP08、WP09、WP24|LOCAL_SCOPE_VERIFIED|正常 Go owner 与报价进程全部停止后开始计时。20 秒烟测实测同时停机 20,007 毫秒，重启 owner 后首个合格范围 3,447 毫秒，首个 FAST 报价 3,450 毫秒。留存截断、历史缺口披露及后续块同步通过|链高差由受控 RPC 推进。1 小时、1 天、30 天场景继续通过实际计时执行。 [复验工具](integration/tools/acceptance/r01_r04_recovery_process.py)|
|I7-R04.3 Go history 状态封闭（2026-11-28）|§18、§23.14、§28.26；WP08、WP20、WP24|LOCAL_SCOPE_VERIFIED|Go history queue reload 现在校验作业区间排序、游标、完成计数、租约 token、基线链身份和累计容量。损坏持久状态返回 `HISTORY_QUEUE_STATE`，不进入 claim/status/replay。新增损坏作业重启回归，history queue 可用子集通过|Go reducer 中依赖已删除 `evm/baseline.json` 的既有历史测试仍失败，真实长期容量和 retention 证据待独立环境|
|I7-R04.2 实际 PG/Kafka owner 恢复（2026-11-28）|§18、§23.14、§28.10、§28.26；WP07、WP08、WP20、WP24|LOCAL_SCOPE_VERIFIED|`r03_authority_process.py` 使用实际 PostgreSQL、Kafka 和 normal Rust Parser 通过 publish、revoke、restart、fence 及 1 小时、1 天、720 小时等效 gap、空 owner KV 四组场景。四组均从 `from_slot=null` 开始，发送 2 帧，首个合格 scope 约 1.19–1.81 秒|场景使用受控公开 tonic source 和等效时间推进，`wallclock_downtime_simulated=false`。真实停机、正常 owner 与报价进程的 retention 联测及跨故障域 peer 仍待验收|
|I7-R04.3 历史队列重启校验（2026-11-28）|§18、§23.14、§28.26；WP08、WP20、WP24|LOCAL_SCOPE_VERIFIED|Rust 在解析前限制持久队列原始字节，校验覆盖区间、作业游标和租约。在线基线提交保留损坏记录供核对。3 项持久状态回归及 7 项正常 history runtime 测试通过|旧 `history_queue` 集成套件缺少 `chain-parser-rust/test/fixtures/solana/baseline.json`，长期容量和留存验收仍待完成|
|I7-R04 持久范围预算与历史隔离边界（2026-11-28）|§9、§18、§28.26；WP08、WP09、WP20、WP24|LOCAL_SCOPE_VERIFIED|修复在线 scope 保留容量和 history replay 重启加载的无符号预算下溢，损坏聚合和超限历史记录现在在进入恢复或 READY 前封闭；冷启动从历史目录最多恢复 32 个仍有效的池身份，额外池继续通过显式 scope 请求扩容；Rust history queue 重启读取也按作业、队列、记录和字节预算封闭；新增两项 Go 边界回归，Go bootstrap 全包及 Rust history runtime 7 项通过|缺失的历史 receipt fixture 仍阻塞集成 retention 验收；真实长停机、公开历史 bank 和生产故障域仍待完成|
|I7-R03 真实中间件流恢复基础（2026-11-27）|§7、§18、§28.22；WP04、WP05、WP07、WP20、WP24|LOCAL_SCOPE_VERIFIED|隔离 Docker PostgreSQL/Kafka 回退、Kafka 内外双监听器、固定 protobuf/SDK 构建环境和真实 normal Yellowstone 发布、撤销、重启、Fence、HEAD_FIRST 场景已接入；R03 history runtime 7 项测试通过|证据为受控 tonic 来源与本机真实中间件，真实同源 Yellowstone、版本化历史 bank 完整性、连续断线和生产故障域仍待验收|
|I4-T2022 Token-2022 扩展与转账费收敛（本轮）|§7.3、§8.12、§11.3、§12.2、§12.5、§12.11–12.14；WP02、WP05、WP11、WP20、WP25|PARTIAL_LOCAL|新增共享 `spl_token_2022` 域模块，统一 epoch 费率选择、向上取整、最大手续费、ExactOut 逆解和 `TransferFeeAmount`/`ImmutableOwner` TLV 校验；Meteora 报价域与 Solana 交易准备复用同一解析逻辑|两套 Rust domain 边界测试、格式检查和 dex-router check 通过；固定 bank/协议数学全量回放因缺失 `meteora-token2022.json` fixture 阻塞，执行准备测试因缺失 `evm/baseline.json` 阻塞；Orca adaptive Token-2022、稀疏数组、奖励领取及 Raydium CLMM 订单仍待后续优先级完成|
|I4-Orca adaptive 补充数组窗口（本轮）|§7.3、§8.12、§11.3、§12.2、§12.5、§12.11–12.14；WP02、WP04、WP05、WP11、WP20、WP25|PARTIAL_LOCAL|动态 adaptive profile 接受 3–7 个按 PDA/bitmap 校验的连续数组，并表示同 bank 内明确观测到的 system-owned 空 PDA。双向 ExactIn/ExactOut 各选最多三个方向数组。Go/Rust 校验稀疏 PDA，交易准备与 SBF worker 检查同一账户的 owner 和数据|101 个经典交换用例和 20 个 bank 回放、Go/Rust Protobuf、Go/Rust 配置和 Rust 普通 RPC 专项通过。稀疏分支通过原程序 builder 双向 ExactIn/ExactOut、七数组 RPC 正负例、执行准备编译及 worker 的账户证明校验。显式空 PDA 与缺失依赖分开处理。扩展窗口原 SBF/CPI 余额差、Token-2022 和奖励领取仍待验收|
|I4-Orca adaptive Token-2022 SwapV2|§7.3、§8.12、§11.3、§12.2、§12.5、§12.11–12.14；WP02、WP04、WP05、WP11、WP20、WP25|LOCAL_SCOPE_VERIFIED|绑定同 bank 的 Token-2022 程序、mint、vault、Memo 和 epoch。任一侧或两侧支持 TransferFeeConfig，双向 ExactIn/ExactOut 使用用户毛投入和净收款。SwapV2 复用 SPL/WSOL 资金准备及 ATA 创建，独立 worker 验证用户余额、withheld 与两个 vault 后状态。能力目录为 `xone-protocol-capabilities/27`|Go/Rust wire、普通 RPC 和数学专项通过。公开部署程序在受控 bank 上通过 24 组交易和 24 组阈值回滚，覆盖 epoch 切换、稀疏数组和预注资 ATA。签名提交与回执继续按 I6 验收。|
|Balancer Managed 非零 swap-share 费用接入（I1）|§12.2、§12.11、§12.14；WP11、WP16|PARTIAL_LOCAL|移除 ExactIn 二分逆解。按原 `GIVEN_OUT` 回调计算 BPT 铸币，Vault 按独立的原始金额舍入顺序结算。ExactIn 与 ExactOut 共用 BPT 账本更新|两种模式各 357 个原始案例及 120 个连续步骤通过 Rust 差分，含两类回滚。顶层分派和 Rust Protobuf 后状态回归通过，指定 SDK 头文件后 Quote API 编译通过。可部署版本的真实来源、跨语言联验和正常 API 验收仍待完成|
|Curve MetaUSD direct ExactOut（本轮）|§12.2、§12.11、§12.14；WP11、WP16|LOCAL_SCOPE_VERIFIED|补齐有界单调整数逆解、零目标和不可达目标拒绝、Meta 直接兑换完整 stored/admin balance 后状态，并接入 multi-asset 与 ExactOut 能力目录|自包含 `curve_meta_exact_out_standalone` 两项回归通过；Meta underlying、真实 base LP 链路和生产执行资格仍待完成|
|Curve StableSwap NG ExactOut（本轮）|§12.2、§12.11、§12.14；WP11、WP16|LOCAL_SCOPE_VERIFIED|补齐固定 rate NG 完整 oracle/admin-fee 后状态逆解、零目标和不可达目标拒绝，接入 multi-asset 与 ExactOut 能力目录|自包含 `curve_ng_exact_out_standalone` 两项回归通过；特殊 Token、外部 rate provider、真实来源和生产执行资格仍待完成|
|Curve Plain3/Plain4 ExactOut 边界收敛（本轮）|§12.2、§12.11、§12.14；WP11、WP16|LOCAL_SCOPE_VERIFIED|源整数溢出或流动性不足时统一返回不可达目标，3/4 资产后状态反解回归通过|自包含 `curve_plain_exact_out_standalone` 两项回归通过；真实部署来源和执行资格仍待完成|
|BNB Smart Chain 连续 Swap journal 复验（[阶段259](#bsc-contiguous-swap-journal-replay)，P0-1）|§7、§9、§18、§23、§28，WP03–WP07、WP18、WP24|LOCAL_SCOPE_VERIFIED|2026-11-26 采集 18 个同代连续提交，NodeReal 状态见证、Defibit 原生解析和 PublicNode 10 条 Swap 日志逐项匹配|本轮固定窗口与离线重放完成。连续长期采集、独立共识来源和生产故障域仍待验收|
|固定 EVM 区块读取的有限重试（[阶段258](#fixed-evm-block-read-retry)，P0-1）|§7、§23、§28，WP04、WP05、WP24|LOCAL_SCOPE_VERIFIED|显式 RPC 策略下按区块号读正文、按哈希批读回执遇到 `-32000` 时在原有预算内重试|本地验证覆盖恢复、持续失败和原生证明字节计费。公网非空 Swap 窗口继续验收|
|非 Ethereum 主网连续 journal 重建（[阶段256](#public-evm-contiguous-journal)，P0-1）|§7、§9、§18、§28，WP03–WP08、WP18、WP24|LOCAL_SCOPE_VERIFIED|BNB Smart Chain 与 Polygon 的正式解析器在同一代连续发布，封存响应可在空 Pebble 库重建|覆盖指定池与有限窗口。其余链的连续状态、非空交易事实、独立故障域仍待验收|
|EVM 主网原生见证与 V3 批量归档（[阶段255](#public-evm-witness-batch)，P0-1）|§7、§9、§18、§28，WP03、WP04、WP05、WP18、WP24|LOCAL_SCOPE_VERIFIED|七条 EVM 主网分别通过正式解析器实时采样、双端点核对和完整快照离线重放|验收覆盖每链一个指定池和块。各链的独立共识来源、连续窗口及跨故障域仍待完成|
|Solana 主网账户 RPC 归档重放（[阶段254](#solana-raw-rpc-replay)，P0-1）|§7、§8.4、§8.9、§28，WP03、WP04、WP05、WP24|LOCAL_SCOPE_VERIFIED|正式解析器跨两个主网来源重启，按原始调用顺序重放两个 finalized slot 的池状态，每个块由另一端点复核|覆盖一个固定 Raydium CPMM 池，独立 bank、Yellowstone 与生产故障域仍待验收|
|滞后主网 RPC 的选择与复核（[阶段253](#public-mainnet-stale-head)，P0-1）|§7、§23.4、§28，WP03、WP04、WP05|LOCAL_SCOPE_VERIFIED|首选 EVM 端点报告过期头时，在同一确认级别交叉核对数值高度与哈希，保留正文和回执见证。Ethereum 增加同块储备复核|八链各验证一个池，Solana 独立 bank 和生产故障域仍待验收|
|八条主网的正式解析器和真实池状态（[阶段252](#public-mainnet-pool-owners)，P0-1）|§3、§7、§9、§28，WP03、WP04、WP05、WP18、WP24|LOCAL_SCOPE_VERIFIED|七条 EVM 链和 Solana 在同一次公网验收发布快照，六个新增部署按同块合约状态复核|覆盖每链一个指定池，独立共识来源、全协议和生产验收仍待完成|
|Ethereum 主网连续签名窗口（[阶段251](#public-signed-ethereum-windows)，P0-1）|§7、§18、§28，WP03、WP04、WP05、WP20、WP24|LOCAL_SCOPE_VERIFIED|两个真实窗口各 32 条连续提交、双 Router 整数报价和冷重启、原始响应冻结重放；空目录可复验|执行层完整状态转换、独立故障域、Solana 可信 bank 和生产验收仍待完成|
|Solana 多链启动和运行隔离（[阶段250](#rust-chain-workers)，P0-1）|§7、§8、§28，WP04、WP05、WP07、WP24|LOCAL_SCOPE_VERIFIED|每链独立同步和续租，退出前完成已接收的 Kafka 提交与撤权，两个正式解析器再连主网|独立主网 bank、同源 Yellowstone 和跨故障域验收仍待完成|
|Ethereum 交易发送方声明（[阶段249](#ethereum-rpc-sender)，P0-1）|§7.3、§28.6，WP04、WP24|LOCAL_SCOPE_VERIFIED|两种完整正文入口对主网标准交易按块高和时间恢复签名，核对 RPC 提供的发送地址|连续签名来源窗口、执行后状态及生产验收继续推进|
|主网开发 RPC 与正式解析器（[阶段248](#public-mainnet-dev-rpc)，P0-1）|§3、§7、§28，WP03、WP04、WP05|LOCAL_SCOPE_VERIFIED|8 条链实时核对身份和 finalized 块，Ethereum 与 Solana 正式解析器发布池快照|签名连续窗口、独立 Solana bank、Yellowstone 和生产验收仍待完成|
|Go慢链启动、调度与租约隔离（[阶段247](#go-per-chain-scheduling)，P0-1）|§7.8、§9.24、§9.30、§18、§19.5、§28.6–28.9、§28.26，WP04、WP06、WP07、WP18、WP24|LOCAL_SCOPE_VERIFIED|每链worker与有界唤醒；正常RPC慢链不阻塞健康链启动/追块，实PG/Kafka独立提交/续租/撤权及释放通过|验证为本机受控RPC和实中间件；Rust慢链调度、真实第二/第三链及独立故障域仍缺|
|签名窗口输入规划与近期录制重放入口（[阶段246](#signed-window-input-planning)，P0-1）|§7.1–7.3、§7.9、§9.24、§28.5–28.7，WP03、WP04、WP05、WP24|LOCAL_SCOPE_VERIFIED|实际原始请求模板、最多128高度的有界输入采集规划；传输重试共用原期限/字节预算，近期录制owner及归档重放需显式选择|仅工具和有限正常bank通过；真实32高度连续发布/重放仍未通过，旧失败和超范围窗口不计成功|
|近期签名原始输入与普通双Router/恢复（[阶段245](#recent-signed-v2-bank)，P0-1）|§7.3、§7.9、§9.24、§18、§28.5–28.7、§28.26，WP03、WP04、WP07、WP09、WP24|LOCAL_SCOPE_VERIFIED|当前真实签名bank的完整原始响应有界采集；普通Go验签/MPT后四向整数报价、双副本/冷启动/owner重启与签名拒绝通过，冻结处理无上游读取|明确为近期签名录制模式；真实连续窗口、直连/生产SLO、完整VERIFIED和其他链协议继续推进|
|Ethereum 签名短尾的有界祖先读取（[阶段244](#beacon-bounded-tail-prefetch)，P0-1）|§7.3、§7.9、§28.5–28.7，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|最多4个已核验祖先读取并发，按父链顺序完整验证后一次授权；取消/错误等待全部退出，固定夹具发布门禁不再依赖已清理环境|真实32高度owner、双Router和恢复闭环继续同P0-1验收，预算与来源资格保持原要求|
|Rust 链级启动与失败writer清理（[阶段243](#rust-chain-startup-isolation)，P0-1）|§9.24、§18、§28.6、§28.26，WP04、WP07、WP18、WP24|LOCAL_SCOPE_VERIFIED|逐链隔离/重试RPC和流初始化失败；保留状态重启仅提升失败链，关闭竞态与post-open租约泄漏已修复，实PG/Kafka通过|流任务启动不等于完整bank；串行actor、慢链调度及真实多链全闭环仍缺|
|Go 链级启动与失败租约清理（[阶段242](#go-chain-startup-isolation)，P0-1）|§9.24、§18、§28.6、§28.26，WP04、WP07、WP18、WP24|LOCAL_SCOPE_VERIFIED|单链来源失败不阻断其他链；正常RPC修复可轮询恢复，实PG/Kafka证明post-Acquire失败释放租约|当前仍串行启动/轮询；Rust见阶段243，慢链调度、真实多链完整交付继续推进|
|OP 原生头固定字段（[阶段241](#op-native-header-rules)，P0-1）|§7.3、§28.6、§28.7，WP04、WP05|LOCAL_SCOPE_VERIFIED|从Bedrock高度核对difficulty/nonce/uncle与Gas边界，两个真实块的重算哈希反例拒绝；Go全包/race/vet通过|父块依赖规则、完整执行后状态及各链最终性继续验收|
|Go 测试独立性与必要文件收敛（[阶段240](#go-standalone-fixtures)，P0-1）|§25、§28.3、§28.5，WP01、WP03|LOCAL_SCOPE_VERIFIED|移除5处兄弟项目路径，显式保留20份必要gzip，契约191份公开来源逐字节通过；再清理10.81GiB重复构建环境|CI/供应链、完整工作包验收和其余设计缺项继续推进|
|OP 自动扫描的废弃通道恢复（[阶段239](#op-forward-channel-recovery)，P0-1）|§7.1、§7.2、§7.3，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|Holocene 交易边界恢复、授权证明累计计费和旧证据清理，五个真实通道重放一致|恢复布局为原始 blob 的合成提交边界；真实跨提交正例、混合通道交易和完整执行仍待验收|
|OP 固定派生属性（[阶段238](#op-fixed-payload-attributes)，P0-1）|§7.1、§7.3，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|核对属性存款的固定 gas 上限和 SequencerFeeVault 收款地址，218 块缓存重放一致|完整执行见证、真实跨提交通道和其余链来源仍待验收|
|OP 执行载荷属性（[阶段237](#op-origin-payload-attributes)，P0-1）|§7.1、§7.3，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|由签名 L1 来源核对 L2 的 prevRandao 和父信标块根，跨批次边界复验通过|执行见证来源、真实跨提交通道及完整执行后状态待验收|
|OP 属性存款的 L1 blob 基础费用（[阶段236](#op-origin-blob-fee)，P0-1）|§7.1、§7.3，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|按签名 L1 祖先头重算 blob 基础费用，218 个高度离线复验通过|主网跨提交通道、执行后状态及其他链与协议来源仍待验收|
|OP 批次交界的 L1 来源连续性（[阶段235](#op-origin-boundaries)，P0-1）|§7.2、§7.3，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|普通发布命令重放五个 span，核验四个边界的 epoch 和序号|真实跨提交通道及执行后状态待验收|
|含存款核验的 OP owner 长窗口（[阶段234](#op-owner-full-deposits)，P0-1）|§7.1、§7.3、§7.4，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|补读 L1 回执后跨四个 span 发布 217 条增量，与阶段233和独立离线重放逐条一致|主网跨多笔提交通道、L2 执行派生及各链来源与恢复验收仍待完成|
|OP 普通 owner 连续 218 块验收（[阶段233](#op-owner-full-218)，P0-1）|§7.1、§7.3、§7.4、§7.9，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|4 个 span、1 条基线、217 条连续增量，两次运行逐条一致|含用户存款核验的新 owner 长窗口、跨提交通道和执行派生待验收|
|普通 OP owner 核验用户存款（[阶段232](#op-owner-portal)，P0-1）|§7.1、§7.3，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|owner 在 159778224 自动取得 L1 完整回执并核验用户存款，跨批次 4 条增量纯缓存复验通过|主网跨提交通道、完整执行派生和其他链的来源资格待验收|
|OP 用户存款来源（[阶段231](#op-portal-deposits)，P0-1）|§7.1、§7.3，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|由 4 个以太坊块的完整回执重建 4 笔 OP 用户存款，核对编码和位置|普通 owner 自动读取回执、完整执行派生及其他协议覆盖待验收|
|OP 首笔存款的 L1 基础费用（[阶段230](#op-origin-base-fee)，P0-1）|§7.1、§7.3，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|普通 owner 按签名 L1 祖先区块头核对每个引用 epoch 的基础费用|其余存款输入、L2 执行派生、真实跨提交通道及多链覆盖待验收|
|OP 普通 owner 跨批次连续发布（[阶段229](#op-owner-window)，P0-1）|§7.1、§7.3、§7.9，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|跨两个 span 连续发布 4 条增量，同源离线复验逐条一致|218 块长窗口、主网跨提交通道及执行派生待验收|
|OP 批次按 L1 顺序补读（[阶段228](#op-forward-window)，P0-1）|§7.1、§7.3、§7.9，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|四笔 L1 提交覆盖阶段227的 218 个 OP 块，重跑报告一致|普通 owner 逐块消费、真实跨提交通道及执行派生待验收|
|OP 批次连续刷新（[阶段227](#op-refresh-consecutive)，P0-1）|§7.1、§7.3、§7.9，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|发布批次时锁定已复核的信标文件，两次相邻最终化周期刷新通过|两个 OP span 之间有 218 块待覆盖|
|OP 同高轮询重新核验批次（[阶段226](#op-owner-idle-recovery)，P0-1）|§7.1、§7.3、§7.9，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|在同一 owner 进程撤销 OP 块 159774767 的基线，恢复证明后重新发布|相邻签名周期连续刷新、跨提交发现及 L2 执行派生待验收|
|重组时撤销旧基线（[阶段225](#owner-fork-revocation)，P0-1）|§7.9，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|发现队列损坏时仍发布撤权记录。替代状态不可读期间 baseline 返回 503|已用两个本地 RPC 端点验证恢复。OP 专用批次证明轮换与恢复待验收|
|OP 最新批次自动采集（[阶段224](#op-owner-live-collection)，P0-1）|§7.1、§7.3、§9，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|从当前签名头自动取得提交交易、发送者证明和 blob，核验 70 个 OP 块|连续刷新、跨提交发现、执行派生与重组恢复待验收|
|OP owner 批次证明更新（[阶段223](#op-owner-span-refresh)，P0-1）|§7.1、§7.3、§9，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|从已核验的提交确定 L2 范围，自动读取 24 个块并原子发布批次文件|L1 提交与授权证明的持续获取、执行派生及重组恢复待验收|
|OP owner 批次基线重放（[阶段222](#op-owner-span-replay)，P0-1）|§7.1、§7.3、§9，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|普通进程发布块 159774767 的 USDC/WETH 基线，从归档复验相同池状态并拒绝空批次|持续供应当前批次、执行派生与重组恢复待验收|
|OP owner 发布前核对批次（[阶段221](#op-owner-span-membership)，P0-1）|§7.1、§7.3、§9，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|普通池状态读取按已验证批次核对 L2 块位置和哈希，共用本次证明预算|持续供应当前批次、普通进程发布与重组恢复待验收|
|OP 提交间收件交易覆盖（[阶段220](#op-channel-interval)，P0-1）|§7.1、§7.3，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|核验提交间每个完整 L1 块及两端块内交易范围，拒绝遗漏的授权提交|跨提交正例及 owner 执行派生待验收|
|OP 多笔提交通道（[阶段219](#op-ordered-channel-submissions)，P0-1）|§7.1、§7.3，WP03、WP04、WP24|PARTIAL_LOCAL|逐笔核验授权、提交位置与 blob，跨提交累积有序帧|待真实跨提交样本、期间收件交易覆盖与重置核验|
|OP 批次发送者授权（[阶段218](#op-batch-sender-authority)，P0-1）|§7.1、§7.3，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|从提交块的 SystemConfig 存储证明取得发送者并核验五份 blob|继续覆盖跨提交通道与 owner 派生|
|OP 批次的 L1 来源（[阶段217](#op-span-origin-range)，P0-1）|§7.1、§7.3，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|12 个以太坊头部将 21 个 OP 块引用的 4 个来源连至签名最终化的提交块|继续验证发送者授权与跨提交通道|
|OP 批次用户交易（[阶段216](#op-span-contents)，P0-1）|§7.1、§7.3，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|已从 21 个 OP 块重建并逐字节匹配 1082 笔用户交易的 span|L1 来源、发送者授权、跨提交通道和执行派生仍需验收|
|OP 通道重组（[阶段215](#op-channel-frames)，P0-1）|§7.1、§7.3，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|从一笔已验证提交的五份 blob 得到完整通道与一个 span 批次|下一步核验批次内各块与交易|
|OP Mainnet 批次数据（[阶段214](#op-batch-blob)，P0-1）|§7.1、§7.3，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|在签名确认的以太坊块 26453043 中核验收件交易与五份 blob 的完整数据|下一项从提交数据重组通道并解析 L2 批次|
|普通 OP owner 核验 L1 来源（[阶段213](#op-owner-l1-origin)，P0-1）|§7.1、§7.3、§9，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|OP 块 159771706 发布基线，读取签名最终化头至其来源的 11 个 L1 头部|下一项核验 L1 批次包含证据和 L2 派生|
|OP 块引用的 L1 来源（[阶段212](#op-signed-l1-origin)，P0-1）|§7.1、§7.3，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|从 OP 块 144644465 的交易根核验首笔存款，沿签名确认的以太坊执行链核对 L1 来源|L2 批次派生、重组撤权与恢复待验收|
|OP Mainnet 提款根（[阶段211](#op-message-passer)，P0-1）|§7.1、§7.3、§9，WP03、WP04、WP05|LOCAL_SCOPE_VERIFIED|读取合约状态前，从执行头状态根证明消息传递合约的存储根，与 Isthmus 提款根核对|块 159771104 的在线池基线与本地归档重算一致，L1 派生与最终性待验收|
|OP Mainnet USDC 代理余额（[阶段210](#op-usdc-delegate)，P0-1）|§7.1、§9，WP03、WP04、WP05|LOCAL_SCOPE_VERIFIED|普通 Go owner 发布块 159770518 的 USDC/WETH 基线，离线重算 USDC 余额|下一步从执行状态根验证 Isthmus 消息传递合约的存储根，覆盖在线读取和归档重放|
|普通 owner 在持久化存储上切换信标根（阶段206，P0-1）|§7.1、§7.3、§9，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|旧配置发布基线后停机，新配置从同一存储启动并发布同块新基线|主网块 26451448 的池状态一致，代际、写入者版本及日志序号均从 1 递增至 2|
|主网信标检查点轮换（阶段205，P0-1）|§7.1、§7.3、§9，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|普通 CLI 从旧配置验证后继根，为新配置原子发布证明|主网块 26450712 的新旧最终化结果一致，独立初始根和生产切换待验收|
|主网同步委员会跨 Electra/Fulu 交接（阶段204，P0-1）|§7.1、§7.3、§28.5，WP03、WP04、WP05|LOCAL_SCOPE_VERIFIED|根据参与签名的区块时隙选择状态证明索引，逐头核验执行载荷与签名|历史执行块 23942645 及下一委员会分支通过。Deneb 检查点样本在委员会验证时拒绝|
|从主网归档重算已发布的池状态与兑换事实（阶段203，P0-1）|§7.3、§9、§17，WP03、WP04、WP05|LOCAL_SCOPE_VERIFIED|从签名头反向验证祖先，复用普通 Go 解码器逐块重算并比较派生负载哈希|两个窗口合计 26 个高度，24 个已发布增量逐字节哈希相符|
|从现有 owner 配置选取主网归档节点（阶段202，P0-1）|§7.1、§7.3、§9，WP03、WP04、WP05|LOCAL_SCOPE_VERIFIED|验收与归档检查使用同一对配置端点，报告用来源 ID 标识自定义端点|已复查 256 条归档，新端点的 32 块实测待提供配置|
|池账户证明按端点复用链身份（阶段201，P0-1）|§7.1、§7.3、§9，WP03、WP04、WP05|LOCAL_SCOPE_VERIFIED|同一池读取期间，已验证节点共用链 ID 和创世块身份，每个账户仍按执行块哈希取证|主网连续发布 6 个增量，下一高度的 Pair 历史证明不可用|
|相邻最终化头的原始响应归档（阶段200，P0-1）|§7.1、§7.3、§9，WP03、WP04、WP05|PARTIAL_LOCAL|逐块保存已提交引用，核对双来源区块体、同块账户证明和派生记录|26449944 起连续提交 18/32 个增量，26449964 读取遇 HTTP 429|
|V2 池账户证明共用已验证状态根（阶段199，P0-1）|§7.3、§9、§19，WP03、WP04、WP05|LOCAL_SCOPE_VERIFIED|同一池的 Pair、Factory 和两种 Token 使用同一份已验证执行锚点，每个账户和存储键单独校验|主网 26449753 起连续发布 8 个块，完整窗口待验收|
|普通 Go owner 的 RPC 读取重试（阶段198，P0-1）|§7.1，WP03、WP04|LOCAL_SCOPE_VERIFIED|按链配置重试次数与退避时间，在线和独立历史读取使用各自的客户端|HTTP 408 恢复、429 次数上限、400 终止及缺省行为通过|
|原生区块交易编码复用（阶段197，P0-1）|§7.3、§9、§19，WP03、WP04、WP05|LOCAL_SCOPE_VERIFIED|用一次生成的签名交易原文验证交易根并交付证据|块 26449307 核对 407 笔交易，最终复核后节省 14,514,696 字节预算|
|原生回执见证错误定位（阶段196，P0-1）|§7.3、§22，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|追块失败日志记录见证节点序号、块高、阶段及错误分类|26448990 双节点复读通过，新基线 26449307 达到证明预算上限|
|Ethereum 主网见证节点的交易体复用（阶段195，P0-1）|§7.3、§9、§19，WP03、WP04、WP05|LOCAL_SCOPE_VERIFIED|每个见证节点在回执校验时使用本节点刚验证的交易体，完成后重新读取区块|块 26448796 复测节省 6,066,035 字节预算，连续发布仍待验收|
|相邻最终化头连续发布验收（阶段194，P0-1）|§7.3、§9，WP03、WP04|PARTIAL_LOCAL|普通 owner 运行期间通过 CLI 更新证明并逐条核对发布日志|主网 26448795 → 26448827 在首个增量前撤权，须定位证明预算消耗|
|主网短尾祖先块校验（阶段193，P0-1）|§7.1、§7.3、§9，WP03、WP04、WP05|LOCAL_SCOPE_VERIFIED|反向按高度读取并重算执行头，正向发布复核中间块哈希|主网头回放和发布门禁通过，跨最终化头的连续发布待实测|
|普通 owner 的信标证明刷新（阶段192，P0-1）|§7.1、§7.3、§28.26，WP03、WP04、WP24|LOCAL_SCOPE_VERIFIED|按配置的根采集并验证标准信标响应，持锁替换 owner 的证明文件|两次正常 CLI 刷新及回退、竞争、过期、取消控制通过|
|普通 Go owner 以信标最终性约束主网快照（阶段191，P0-1）|§7.1–7.3、§7.9、§28.26，WP04、WP05、WP07、WP24|LOCAL_SCOPE_VERIFIED|启动及追块以配置的信标根核对执行区块，发布时复核区块哈希和证明时效|主网 DAI/WETH 快照、撤权及同块恢复通过|
|从最终性证据选取 Ethereum 执行块（阶段190，P0-1）|§7.1、§7.3、§28.5，WP03、WP04、WP05|LOCAL_SCOPE_VERIFIED|采集工具核验签名后读取精确高度，在账户证明请求前核对块哈希|主网块 26448285 的在线采集报告与离线重放逐字节相同|
|Ethereum 主网同步委员会交接（阶段189，P0-1）|§7.1、§7.3、§28.5，WP03、WP04、WP05|LOCAL_SCOPE_VERIFIED|从较早的显式信标根依次验证各委员会签名和下一委员会根|实际连续两次交接至周期 1915，10 个交接场景通过|
|Ethereum 主网同步委员会最终性验证（阶段188，P0-1）|§7.1、§7.3、§28.5，WP03、WP04、WP05|LOCAL_SCOPE_VERIFIED|从指定检查点验证同周期聚合签名、最终化分支、执行载荷及部署证据|主网块 26448094 实测 508 名签署者，13 个 CLI 场景通过|
|Ethereum 主网信标检查点与执行区块绑定（阶段187，P0-1）|§7.1、§7.3、§28.5，WP03、WP04、WP05|LOCAL_SCOPE_VERIFIED|离线部署证据对照显式信标根验证执行载荷与当前同步委员会|历史主网块 26355865 和 11 项拒绝控制通过，独立最终性待验证|
|Ethereum 主网离线交易正文复核（阶段186，P0-1）|§7.3、§28.5、§28.7，WP03、WP05|LOCAL_SCOPE_VERIFIED|按位置核对区块交易与签名原文，复核所声明的发送人|两份固定主网部署证据经普通 CLI 通过|
|Ethereum 主网提款列表完整性（阶段185，P0-1）|§7.3、§28.7，WP03、WP05|LOCAL_SCOPE_VERIFIED|在线读取与离线回放校验 Shanghai 提款根、顺序及必填字段|主网块 26355865 和分叉边界通过，独立共识锚点待接入|
|报价页展示选中路线的资金需求（阶段184，P0-3）|§13.1、§15.3，WP13、WP16|LOCAL_SCOPE_VERIFIED|展示实测付款人网络费、包装本金、最低起始余额及排序依据|24 项前端测试与实际浏览器两种报价方向通过|
|单路线包装和授权费用（阶段183，P0-3）|§13.1、§14.2–14.3，WP13、WP15、WP16|LOCAL_SCOPE_VERIFIED|按支出合约的源额度选择精确授权或清零后授权|20 个 HTTP 场景覆盖两种报价方向及价格到期|
|WETH 报价按实际 spender 复核授权（阶段182，P0-3）|§13.1、§14.2，WP13、WP15、WP16|LOCAL_SCOPE_VERIFIED|从源区块分别读取 Router02 和拆单执行器额度，核对独立执行观测|12 个包装报价 HTTP 场景通过|
|正常报价核算包装、授权与兑换（阶段181，P0-3）|§13.1、§14.2–14.3，WP13、WP15、WP16|LOCAL_SCOPE_VERIFIED|从源余额复核整组费用、包装本金及最低起始资金|两种报价方向的 12 个包装 HTTP 场景通过|
|包装与授权组合执行（阶段180，P0-3）|§13.1、§14.2–14.3，WP13、WP15|LOCAL_SCOPE_VERIFIED|拆单 worker 核对完整区块顺序并从包装后状态执行授权、兑换|两种报价方向的三笔和四笔交易与独立出块相符|
|WETH 余额及授权同时不足时的独立回放（阶段179，P0-3）|§13.1、§14.2–14.3，WP13、WP15|LOCAL_SCOPE_VERIFIED|按余额缺口包装，依实际额度执行授权及兑换|两种方向、三种额度的 6 组序列经独立出块与 REVM 对照|
|正常报价核算 WETH 本金与网络费（阶段178，P0-3）|§13.1、§14.2–14.3，WP13、WP15、WP16|LOCAL_SCOPE_VERIFIED|单路线和拆单均从源区块复核持币余额，以两笔网络费比较候选|ExactIn、ExactOut 及兑换时价格到期共 4 个新增 HTTP 场景通过|
|WETH 短缺额的计划绑定和 worker 执行（阶段177，P0-3）|§13.1、§14.1–14.4，WP13、WP15|LOCAL_SCOPE_VERIFIED|按实际卖币余额生成包装交易，验证 Router02 配置和 WETH9 代码，向兑换传递包装后状态|9 项绑定及 worker 回归通过|
|包装本金与兑换费用回放（阶段176，P0-3）|§13.1、§14.1–14.3，WP13、WP15|LOCAL_SCOPE_VERIFIED|从一个最小单位 WETH 起步，按实际输入缺口包装原币并兑换|两种方向各两笔出块交易与 REVM 的状态、日志和费用相符|
|付款人起始原币需求（阶段175，P0-3）|§13.1、§14.1–14.3，WP13、WP15、WP16|LOCAL_SCOPE_VERIFIED|报价披露单笔或有序多笔执行所需的起始余额、来源余额与实测净支出|16 个 HTTP 场景与原币费用边界测试通过|
|正常报价计入清零费用（阶段174，P0-3）|§13.1、§14.1–14.3，WP13、WP15、WP16|LOCAL_SCOPE_VERIFIED|按绑定计划和起始账户余额复核三笔支出，将合计用于候选排序|16 个 HTTP 场景通过，覆盖两种方向及成交时价格到期|
|拆单 worker 执行三笔交易（阶段173，P0-3）|§13.1、§14.1–14.4；WP13、WP15|LOCAL_SCOPE_VERIFIED|依据初始额度重建清零、授权与兑换，逐笔向后传递状态|两种方向均与独立出块费用相符，报价端三笔费用核对待接入|
|拆单授权替换计划（阶段172，P0-3）|§13.1、§14.1–14.3，WP13、WP15|LOCAL_SCOPE_VERIFIED|显式策略生成清零和准确额度授权，按当前 allowance 裁剪操作|两种报价方向、三种额度及错误执行目标均有绑定测试，顺序 worker 接线待完成|
|非零不足授权的三笔交易回放（阶段171，P0-3）|§13.1、§14.2–14.3，WP13、WP15|LOCAL_SCOPE_VERIFIED|按清零、精确授权、兑换顺序对照两种报价方向的独立区块|8 项 V2 路径测试通过，三笔序列待接入 worker 和正常报价|
|正常报价计入调用者授权费（阶段170，P0-3）|§13.1、§14.2–14.3，WP13、WP15、WP16|LOCAL_SCOPE_VERIFIED|零授权拆单按相邻区块依次执行授权和兑换，报价服务复核两笔费用后比较候选|两种方向的成交、兑换时价格过期及原有回归共 12 个 HTTP 场景|
|两笔交易的 worker 核验（阶段169，P0-3）|§13.1、§14.1–14.3，WP13、WP15|LOCAL_SCOPE_VERIFIED|按同一计划生成准确额度的授权交易，向后传递状态并核对两笔费用|ExactIn 和 ExactOut 均通过独立出块对照|
|连续授权与兑换回放（阶段168，P0-3）|§13.1、§14.2、§14.3，WP13、WP15|LOCAL_SCOPE_VERIFIED|从零授权开始推进两笔交易的状态和 nonce|两种报价方向均通过，正常报价待接入|
|调用者授权费用判定（阶段167，P0-3）|§13.1、§14.1、§15；WP13、WP15、WP16|LOCAL_SCOPE_VERIFIED|按付款人、代币、执行目标及报价输入核对 ERC20 授权|两种报价方向与 RPC 准备均已验证，授权不足时待顺序计费|
|共享池拆单报价费用（阶段166，P0-3）|§13.5/14.2/14.3；WP13、WP15、WP16|LOCAL_SCOPE_VERIFIED|正常报价比较两条单路线和两种有序拆单，披露选中计划费用|固定 Shanghai 状态的 8 个 HTTP 场景通过，辅助交易费用待补|
|V2 拆单独立仿真（阶段165，P0-3）|§13.5/14.2/14.3；WP13、WP15、WP16|LOCAL_SCOPE_VERIFIED|真实 worker 重建并执行整笔计划、核对批量合约与 Router02 及完整池状态|4 笔成功、4 笔回滚的原始状态回放及 3 项 worker 拒绝通过，父进程费用接线继续|
|V2 拆单计划绑定（阶段164，P0-3）|§13.5/14.1/14.2；WP13、WP15|LOCAL_SCOPE_VERIFIED|按同一快照重算完整计划、生成单笔交易并重建核对|4 笔实际本地成交的 calldata 一致；待接 worker、父费用核验及正常报价|
|V2 整笔拆单执行（阶段163，P0-3）|§13.1/13.4/13.5/14.2；WP13、WP15、WP16|LOCAL_SCOPE_VERIFIED|固定 Router02、按腿序共享池执行、按付款和收款余额校验|4 笔成功及 4 笔回滚；待接正常报价及父进程费用核验|
|价格采集按 Router 区块选取（阶段162，P0-3）|§15.5/21.3/26.3；WP13、WP15、WP16|LOCAL_SCOPE_VERIFIED|状态返回已校验 bank 锚点，采集和发布前复核|调度 5 项、独立采集 4 项、真实 HTTP 回归通过；不证明共识|
|有界价格轮次调度（阶段161，P0-3）|§15.5/21.3/26.3；WP13、WP15、WP16|LOCAL_SCOPE_VERIFIED|同块采集、外部回执交接、已签候选发布与持久恢复|过期重新采集、丢确认只读回查；当前 bank 对齐和生产签署方未验收|
|本机价格发布操作工具（阶段160，P0-3）|§15.5/21.3/26.3；WP13、WP15、WP16|LOCAL_SCOPE_VERIFIED|候选安装、一次条件提交和结果回读|真实本机 Router 及断连情形定向回放；持续签署与调度未完成|
|持锁 Router 在线价格发布（阶段159，P0-3）|§15.5/21.3/26.3；WP13、WP15、WP16|LOCAL_SCOPE_VERIFIED|在线预检、双摘要条件发布、重启后已生效价格回读|实际 HTTP 回放通过，持续采集调度和可信 bank 未完成|
|独立签署回执与候选簿组装（阶段158，P0-3）|§15.5/21.3；WP13、WP15、WP16|LOCAL_SCOPE_VERIFIED|复算采集消息、Router 原签名阈值校验、离线耐久版本预检|双币种签署回放与拒绝路径；非在线持续发布|
|整簿同块采集与待签署输入（阶段157，P0-3）|§15.5/21.3；WP13、WP15、WP16|LOCAL_SCOPE_VERIFIED|全部批准币种固定同一块、签名消息覆盖同一版本，仅完整且未过期的请求可交接|4 项整簿回放及 6 项原采集器测试通过；持续调度和独立签署发布仍缺|
|正常Router耐久价格发布与重启（阶段156，P0-3）|§15.5/21.3/26.3；WP13、WP15、WP16、WP18|LOCAL_SCOPE_VERIFIED|显式 journal 的启动核对、持久发布、确认丢失与重启恢复|80 项门禁通过，239 次 Rust、82 次 Go；默认模式仍无跨重启保证|
|耐久价格发布journal与离线初始化（阶段155，P0-3）|§15.5/21.3/26.3；WP13、WP15、WP16、WP18|LOCAL_SCOPE_VERIFIED|签名book授权后记录发布高水位|75门禁、235次Rust/80次Go；离线重启回退拒绝，正常Router尚未接入|
|授权价格book版本刷新与报价快照（阶段154，P0-3）|§13.1/15.5/21.3/26.3；WP13、WP15、WP16、WP18|LOCAL_SCOPE_VERIFIED|固定配置来源/签名版本与原子发布|72门禁、229次Rust/80次Go通过；单报价价格快照/旧报价拒绝/进程内防回退；重启耐久性未完成|
|选中候选费用的正常报价同源披露（阶段153，P0-3）|§13.1/15.3/15.5；WP13、WP15、WP16、WP18|LOCAL_SCOPE_VERIFIED|复用选中计划的实际执行/父费用审计/授权换算|68门禁、226次Rust/80次Go通过；16费用披露与4未签名OP未知报价、完整净额/总费不外推|
|批准Chainlink feed的真实来源适配（阶段152，P0-3）|§13.1/14.2/15.3/21.2–21.3；WP02、WP13、WP15、WP16、WP18|LOCAL_SCOPE_VERIFIED|批准部署/原ABI与签名绑定、宽整数换算、有界只读采集/待签署CLI|68门禁/223次Rust/80次Go、48向量/148换算、49 CLI/17原RPC重放通过；持续live刷新/可信bank/完整净额继续|
|正常报价本地逐候选执行费用接线（阶段151，P0-3）|§13.1/13.3/13.5/14.2/14.4/15.3；WP02、WP13、WP15、WP16、WP18|LOCAL_SCOPE_VERIFIED|显式启用单路线V2、逐候选BOUND/实际worker/父费用审计和授权换算、固定deadline|59门禁/215次Rust/78次Go、16报价/30原执行和4 OP未知费用回退及旧61 HTTP全部通过；完整净成本/真实价格继续|
|逐完整候选费用评分内核（阶段150，P0-3）|§13.1/13.3/13.5；WP13、WP15、WP16、WP18|LOCAL_SCOPE_VERIFIED|ExactIn/Out完整候选成本回调、有符号目标、缺成本统一回退与共享状态有界顺序网格|54门禁/208次Rust/77次Go、22优化器/2正常API回退及全部价格/VM/61 HTTP回归通过；正常API成本接线继续|
|配置预言机授权与已执行交易费用换算（阶段149，P0-3）|§13.1/14.2/14.4/15.3；WP02、WP13、WP15、WP16、WP18|LOCAL_SCOPE_VERIFIED|服务端批准签名者、源块/币种/时效绑定、原子整数换算及父费用审计后单笔金额|51门禁/184次Rust/77次Go、独立42向量、13 EVM/4 Karst及36 OP/8旧HTTP通过；真实市价/全候选净评分继续|
|Karst正常本地签名API与完整原Router（阶段148，P0-3/P0-4）|§14.2/14.4/28.17；WP03、WP13、WP15、WP16、WP18|LOCAL_SCOPE_VERIFIED|有界OP Mainnet Karst普通Call、BOUND/父费用源审计及正常build/实际签名simulate|39门禁/176次Rust、四原Router完整后状态、401公开执行/父审计与36 OP/8旧HTTP通过|
|Karst用户执行组件与完整原引擎（阶段147，P0-3/P0-4）|§14.2/14.4/28.17；WP03、WP13、WP15、WP16、WP18|LOCAL_SCOPE_VERIFIED|CLZ/Gas cap及原完整OpEvm签名转换，正常Karst准入保持关闭|35门禁/173次Rust、468原场景/401执行/67拒绝及32 OP/8旧HTTP通过|
|Karst预编译组件与固定现代原参考（阶段146，P0-3/P0-4）|§14.2/14.4/28.17；WP03、WP13、WP15、WP16、WP18|LOCAL_SCOPE_VERIFIED|Osaka MODEXP/P256及Karst BN上限组件，完整VM准入保持关闭|30门禁/171次Rust、9452原组件、历史签名/32 OP/8旧HTTP通过|
|OP Mainnet历史Jovian用户执行（阶段145，P0-3/P0-4）|§14.2/14.4/28.17；WP03、WP13、WP15、WP16、WP18|LOCAL_SCOPE_VERIFIED|原operator单位与预编译上限接入，严格限定历史区间|25门禁/168次Rust、64原签名场景、7056预编译输入/160签名转换、32 OP及8旧HTTP通过|
|OP预编译加密后端与Isthmus限制（阶段144，P0-3/P0-4）|§14.2/14.4/28.17；WP03、WP15、WP16、WP18|LOCAL_SCOPE_VERIFIED|KZG/BLS固定后端、Isthmus BLS输入上限和原Gas检查顺序接入实际执行|24门禁/165次Rust、3558原输入/110签名转换、28 OP及8旧HTTP通过；当前VM/可信bank继续|
|正常历史OP构建与签名模拟（阶段143，P0-3）|§5.1/14.2/14.4/15.3；WP13、WP14、WP15、WP16、WP18|LOCAL_SCOPE_VERIFIED|正常build返回完整RPC待签名交易，simulate认证完整wire并由父进程重验源费用/回执/总预算|19门禁/161次Rust、28正常OP HTTP/168拒绝及8旧Shanghai HTTP通过；预编译完整规则/当前VM/可信估值继续|
|OP费用回执的独立源核验（阶段142，P0-3）|§5.1/14.2/14.4/15.3；WP13、WP15、WP16、WP18|LOCAL_SCOPE_VERIFIED|保留的签名请求与严格源银行重算全部费用、最大资金及付款人净扣，核验费用账户before/after|17门禁/160次Rust、896费用源核验及至少650反例、8正常HTTP通过；正常OP API继续|
|OP签名受限worker协议与实际子进程（阶段141，P0-3）|§5.1/14.2/14.4/15.3；WP14、WP15、WP16、WP18|LOCAL_SCOPE_VERIFIED|完整OP签名及属性经现有stdin/stdout worker认证并执行BOUND|19门禁/159次Rust、56实际OP子进程完整回执/17拒绝及8正常Shanghai HTTP通过；正常OP API继续|
|OP签名BOUND完整Router执行与费用（阶段140，P0-3）|§5.1/7.3/13.1/14.2/15.3；WP13、WP14、WP15、WP16、WP18|LOCAL_SCOPE_VERIFIED|签名上下文接入ExactIn/Out完整原Router，实际Token与后状态及完整OP费用|18门禁/157次Rust、448原Router与896产品执行/完整声明后状态、资金/失败/控制守卫及8原HTTP通过；正常OP接线继续|
|OP签名交易绑定与严格历史执行（阶段139，P0-3）|§5.1/7.3/13.1/14.2/15.3；WP13、WP15、WP16、WP18|LOCAL_SCOPE_VERIFIED|同一完整签名wire派生全部VM字段，七规则OP/Base严格银行与费用账户核验|20门禁/160次Rust、448原场景1337转换/896产品执行/441资金拒绝、180归档公链签名及8原HTTP通过；正常OP/报价接线继续|
|OP完整费用账户与原状态转换核验（阶段138，P0-3）|§5.1/7.3/13.1/14.2/15.3；WP13、WP15、WP16、WP18|LOCAL_SCOPE_VERIFIED|Regolith至Jovian的零value费用、最大资金、operator退款与vault/coinbase账户守恒|24门禁/151次Rust、原Go320场景955状态转换与REVM256场景、8原HTTP通过；正常报价总费接线继续|
|OP现代费用版本来源与升级首块重放（阶段137，P0-3）|§5.1/7.3/13.1/14.2/15.3；WP03、WP05、WP13、WP15、WP18|LOCAL_SCOPE_VERIFIED|普通native/历史重放增加现代fee inputs；四升级首块/后继及当日块的原交易/累计Gas与规则绑定|19门禁/153次Rust/214次Go、八块128原signed用户及当日49用户、60投影与15语义反例通过；完整链费/净排序仍待补|
|OP packed费用与FastLZ独立核算（阶段136，P0-3）|§5.1/7.3/13.1/14.2/15.3；WP03、WP13、WP15、WP16、WP18|LOCAL_SCOPE_VERIFIED|三种精确packed属性、显式Ecotone/Fjord数据费和Isthmus/Jovian operator规则；正常来源核验当日OP块与49原signed users|18门禁/150次Rust/6次Go、605原参考及49原signed用户通过；版本选择/报价接线仍未完成|
|OP Regolith同body费用输入来源与重放（阶段135，P0-3）|§5.1/7.3/13.1/14.2/15.3；WP03、WP05、WP13、WP15、WP16、WP18|LOCAL_SCOPE_VERIFIED|普通native读取/历史重放从已核验body提取deposit属性和恢复签名者的完整user wire，费用计算仍在Rust|18门禁/148次Rust/209次Go通过；三笔原signed来源→Rust对原参考、25原sourceHash及来源/发布/语义反例通过|
|OP Regolith历史L1数据费内核（阶段134，P0-3）|§5.1/13.1/14.2/15.3；WP13、WP15、WP16、WP18|LOCAL_SCOPE_VERIFIED|独立解码Bedrock L1 attributes和完整user wire字节计数，固定原op-geth函数对照三笔公链签名交易与72整数向量|16门禁/147次Rust、75原参考输入及8次原HTTP worker回归通过；正常来源接线/现行模型/净排序仍待补|
|EVM Legacy原生费用来源与净扣款（阶段133，P0-3）|§13.1/14/15.3；WP13、WP14、WP15、WP16|LOCAL_SCOPE_VERIFIED|正常worker补Legacy实际原生币余额测量，API共用源绑定/最大资金/alias守恒；旧无测量回执兼容|14门禁/148次Rust；16原Router bank、8次HTTP/56独立整数向量与源绑定守卫通过|
|失败交易费用前端闭环（阶段132，P0-3）|§13.1/14/15.3；WP13、WP15、WP16|LOCAL_SCOPE_VERIFIED|展示原失败回执费用、回滚及待核验状态，绑定实际交易并拒绝混合成功收付|9门禁/22前端单测/13浏览器检查；四真实签名回执、重载可见和桌面/手机截图复核通过|
|Solana失败签名费用与回滚核验（阶段131，P0-3）|§13.1/14/15.3；WP13、WP14、WP15、WP16|LOCAL_SCOPE_VERIFIED|读取原签名失败交易、核对实际网络费及观察到的原生币/Token回滚，独立结果不冒充成功成交净额|13门禁/52次Rust；3原程序失败/18故障/30审计篡改，八钱包和11成功交易回归通过|
|Solana费用后经济净额前端闭环（阶段130，P0-3）|§13.1/14/15.3；WP13、WP15、WP16|LOCAL_SCOPE_VERIFIED|当前SBF证据及耐久回执分别展示费用后金额、网络费和资本押金；不同交易/来源不展示净额|8门禁/18前端单测/16浏览器检查；5类正常HTTP/SBF/签名回执、重载实际可见及桌面/手机截图复核通过|
|Solana原生同单位费用后净额（阶段129，P0-3）|§12.5/13.1/14/15.3；WP13、WP14、WP15、WP16|LOCAL_SCOPE_VERIFIED|共用整数核算已接独立SBF证据和实际回执；原生SOL/明确解包WSOL按lamport同单位估值，租金资本与费用分列|16门禁/50次Rust、11正常SBF/签名回执与独立原余额审计、20准备家族回归通过|
|Solana提交前费用上下文与耐久凭证（阶段128，P0-3）|§13.1/14/15.3；WP13、WP14、WP15、WP16|LOCAL_SCOPE_VERIFIED|预检继承准备fee slot，费用→账户→签名仿真→发送slot连续；绑定原消息/fee Result凭证耐久保存，旧v1兼容/重试幂等|11门禁/46次Rust，实际八种钱包生命周期、并发一次发送与原SOL/WSOL资金钱包通过|
|Solana准备消息费用来源与原生资金分类（阶段127，P0-3）|§13.1/14/15.3/28.17；WP13、WP14、WP15、WP16|LOCAL_SCOPE_VERIFIED|准备RPC费用绑定具体message/unsigned wire及同请求账户快照，校验总资金与付款人余额；网络费/租金/预期退款/本金分列，不同slot明确披露|14门禁/29次Rust、20实际HTTP/SBF及独立审计通过；完整净估值仍未完成|
|无可信成本时的路由排序回退（阶段126，P0-3）|§13.1/13.3/13.5/15.3；WP13、WP16、WP18|LOCAL_SCOPE_VERIFIED|ExactIn取消未经估值的固定激活费用，按毛输出/已有约束排序；ExactOut最小原输入；两模式明确费用未知及回退目标|16门禁/176次Rust、独立网格/三池/预算及四实际HTTP通过|
|EVM最大资金准入与付款人净扣款（阶段125，P0-3）|§13.1/14/15.3/28.17；WP13、WP15、WP16|LOCAL_SCOPE_VERIFIED|源余额必须覆盖bound gas最大reserve，保留gross fee并单列付款人净扣款/自付tip返回，Legacy净扣款未知|14门禁/146次Rust、56向量、四HTTP/冻结回归通过；完整链费/可靠估值及净排序仍缺|
|EVM费用原生余额守恒与源绑定（阶段124，P0-3）|§13.1/14/15.3/28.17；WP13、WP15、WP16|LOCAL_SCOPE_VERIFIED|EIP1559费用测量的before绑定原worker输入，付款人/coinbase分离或重合分别守恒；未知不默认零，Legacy标不可核验；native atomic单位与executionCoinbase披露|14门禁/144次Rust、56独立向量与四次actual HTTP/源绑定/分离-alias守恒通过；完整链费/估值/净排序仍缺|
|EVM原生费用模型与覆盖披露（阶段123，P0-3）|§13.1/14/15.3/28.15/28.17；WP13、WP14、WP15、WP16|LOCAL_SCOPE_VERIFIED|共享Legacy/EIP1559有效单价、burn/tip/最大reserve/实际费用checked uint256，准备与API复用；验证Gas不超已绑定上限，costs区分已知小计/未实现费用与换算，不伪造net|14门禁/142次Rust、56独立整数向量与实际HTTP两worker通过，16原bank H/H+1/准备/VM回归通过；完整链费与净收益排序仍缺|
|真实V2原字节码收款与完整后状态差分（阶段122，P0-2）|§12.14/28.5/28.12/28.17；WP03、WP10、WP14、WP15|LOCAL_SCOPE_VERIFIED|新固定actual bank+MPT原WETH/DAI/Pair三call本地执行，测真实recipient delta/完整Pair后状态；多1输出原K拒绝；Go原VM结果与独立Rust数学全状态比较|14门禁/16次Go/16次Rust、原VM实际收款及q+1原K回退/完整后状态跨内核通过；合成native资金覆盖/非签名call序列，非完整链费/生产VERIFIED|
|真实V2固定bank独立离线夹具（阶段121，P0-1）|§28.3/28.5/28.6/28.11/28.26；WP01、WP03、WP04、WP05、WP09、WP24|LOCAL_SCOPE_VERIFIED|将阶段120真实32原raw封装为有界gzip+元数据pin；按既有政策原数据留本地，跟踪恢复工具/hash；Go服务内同字节副本可独立正常读取/拒绝证明，双router/owner恢复离线回归|13门禁/83次Go/36次Rust、9守卫及固定actual bank/双router冷恢复/owner升代/五来源反例通过；不伪称本轮新实网资格|
|真实当前V2 normal owner/router与来源拒绝（阶段120，P0-1）|§7/28.5/28.6/28.11/28.26；WP03、WP04、WP05、WP09、WP10、WP24|LOCAL_SCOPE_VERIFIED|固定真实DAI/WETH部署/原始RPC归档，普通Go owner到独立Rust router双方向ExactIn/Out；公开字节离线重放/双副本冷重建/owner重启与五来源反例；拒绝会话原因不冒充预算耗尽|13门禁/81次Go/36次Rust、实际公开源/32原raw与双方向4报价、离线重放/副本冷恢复/升代/五错误来源通过；HTTP开发日志及provider声明finalized，不授予生产VERIFIED/共识/实际执行余额差资格|
|EVM当前头确认策略与披露（阶段119，P0-1）|§7.1/7.3/18/28.6/28.26/28.27；WP04、WP05、WP24、WP25|LOCAL_SCOPE_VERIFIED|Go/双Rust只接受latest/safe/finalized，拒绝数字历史高度/earliest/pending/未知/空；正常source-evidence披露配置策略但不声称共识|19门禁/81次Go/97次Rust，双编译器15配置、当前CLI/无状态写入与不支持safe无降级通过|
|v2-complete native Token余额证明（阶段118，P0-1）|§7/11/12.14/28.6/28.7/28.12；WP03、WP04、WP05、WP10|LOCAL_SCOPE_VERIFIED|实际Token代码与MPT account绑定，Berlin稳定只读指令子集重放balanceOf；SLOAD未证明先停止，最多8slot/9pass/每pass100kGas，复用原native总预算|8门禁/77次Go race通过；原bank两余额/六证明、空root/五证明与受限指令/分支/资源反例通过；转账行为/代理与外部调用/可信共识/生产VERIFIED仍缺|
|v2-complete native Pair/Factory状态承诺（阶段117，P0-1）|§7/12.14/28.6/28.7/28.12；WP03、WP04、WP05、WP10|LOCAL_SCOPE_VERIFIED|正常ReadState复用原native proof session，将原Pair/Factory代码及Pair身份/储备/累计价/锁/双向工厂映射与同block stateRoot绑定|8门禁/488次Go race通过，原合约bank与16类失败/预算/取消及普通native服务回归通过；Token balanceOf、可信共识/完整bank和生产VERIFIED仍缺|
|Go周期checkpoint自动配额维护（阶段116）|§10.5/17.7/18/23.9；WP08、WP09、WP24|LOCAL_SCOPE_VERIFIED|默认关闭auto_reclaim；Go周期worker配额超限有限回收、pending优先且一周期最多一操作，来源/备份/取消保护与手动语义保持|27门禁/127次Go/233次Rust、真实周期/分周期边界/故障恢复/四真实进程退出及实际Kafka回归通过；本轮后按用户要求停止，差距见最新复核|
|显式Go checkpoint回收与中断恢复（阶段115）|§10.5/17.7/18/23.9；WP08、WP09、WP24|LOCAL_SCOPE_VERIFIED|新增默认关闭的reclaim_enabled/有期计划，认证apply/resume/status，先远端恢复核验/耐久意图/隔离rename/逐文件释放/完成回执；pending暂停后续归档|27门禁/117次Go/231次Rust、九阶段/四真实进程退出/正常HTTP/幂等/隔离保护与实际Kafka回归通过；Go自动维护由阶段116补齐，Rust执行/独立存储验收后续|
|Rust只读保留计划与整计划互通（阶段114）|§10.5/18/23.9；WP08、WP09、WP24|LOCAL_SCOPE_VERIFIED|双Rust共享只读plan/资源边界、正常Rust认证HTTP/取消关闭、允许两online owner；直接读取Go同一目录并全计划JSON一致|26门禁/104次Go/227次Rust、两份Rust全JSON/planId一致、两normal HTTP及实际Kafka回归通过|
|有界checkpoint只读保留计划（阶段113）|§10.5/18/23.9；WP08、WP09、WP24|LOCAL_SCOPE_VERIFIED|Go认证预览绑定当前ref/cursor与目录证据，保护当前/全部候选/最近备份，完整解码及同store证书/年龄/读取扫描上限，applied=false|26门禁/103次Go/221次Rust、6对象/2store保护/只读正常HTTP/策略hash及实际Kafka回归通过；执行/Rust计划后续|
|公开checkpoint本地容量门禁（阶段112）|§10.5/18/23.9/28.10；WP08、WP09、WP24|LOCAL_SCOPE_VERIFIED|显式按链public-v2逻辑字节/文件/条目预算、先算待写对象/证书/候选，未知文件计费/符号链接和过深拒绝，超限无公开写入或远端请求|26门禁/97次Go/217次Rust、双owner超限前置拒绝/跨语言实际占用与配置hash/真实Kafka恢复通过|
|Rust恢复目录与独立router追平（阶段111）|§10.5/17.5/18/28.10；WP08、WP09、WP24|LOCAL_SCOPE_VERIFIED|Rust候选提示/不可变对象和证书复核、当前head/cursor目录；共享恢复契约v1/v2，完整baseline generation单调追平|26门禁/93次Go/204次Rust、独立双进程正常HTTP/实际Kafka两baseline追平报价及v1回归通过|
|Rust周期checkpoint与配置摘要（阶段110）|§10.5/18/23.7/28.10；WP01、WP08、WP09、WP24|LOCAL_SCOPE_VERIFIED|Rust online显式周期/串行公平轮转、故障恢复/更新代际、正常shutdown先取消网络、共享默认值与secret-free配置摘要|21门禁/92次Go/194次Rust、双链周期/正常shutdown/5组双Rust配置摘要及API回归通过|
|Rust owner远端checkpoint（阶段109）|§10.5/17.7/28.10；WP08、WP09、WP24|LOCAL_SCOPE_VERIFIED|Rust条件上传/精确回读、不可变完成标记、当前目录、来源守卫与关闭取消，Go独立读取实际HTTP原数据|16门禁/21次Go/66次Rust、实际双HTTP服务/Go解码及切代/关闭撤权通过；Rust周期/旧对象目录后续|
|Rust owner公开checkpoint（阶段108）|§10.5/17.7/28.10；WP08、WP09、WP24|LOCAL_SCOPE_VERIFIED|双Rust公共codec编码/不可变文件发布、正常Rust owner授权管理入口、真实cursor与来源守卫|15门禁/20次Go/60次Rust、正常Rust HTTP与双向3组全宽原字节互通通过；远端/周期待接|
|旧checkpoint有限尾窗追平（阶段107）|§10.5/17.5/18/28.10；WP08、WP09、WP24|LOCAL_SCOPE_VERIFIED|owner绑定旧不可变对象及当前ref/cursor，Rust私有Replica有界重放/最终目录复核后激活|19门禁/152次Go/166次Rust、实际Kafka两后续块私有追平/HTTP报价/撤权及半事务拒绝通过|
|多链checkpoint公平调度（阶段106）|§10.5/18/28.10；WP08、WP09、WP24|LOCAL_SCOPE_VERIFIED|后台按配置链轮转、失败仍推进下一链、每链独立预算与同一可取消通道，手动整批语义保留|7门禁/149次Go race及当前Go程序构建通过；双链故障/持续更新/恢复和串行界限通过|
|周期checkpoint发布（阶段105）|§10.5/17.7/28.10；WP08、WP09、WP24|LOCAL_SCOPE_VERIFIED|正常Go服务显式周期发布，完成后再计时、手动/后台共用可取消通道，关闭取消并等待所有发布|18门禁/148次Go/164次Rust及实际Kafka恢复回归通过；多链公平性、旧快照追平与canonical目录继续|
|当前远端checkpoint冷恢复（阶段104）|§10.5/17.7/28.10；WP08、WP09、WP24|LOCAL_SCOPE_VERIFIED|Go当前owner目录、耐久完成标记、Rust有界下载/目录二次核对和正常恢复来源选择|18门禁/143次Go/162次Rust、实际Kafka冷恢复/正常HTTP报价与损坏暂停通过|
|远端checkpoint对象读写（阶段103）|§10.5/17.7；WP08、WP09、WP24|LOCAL_SCOPE_VERIFIED|配置HTTP条件写入对象API、回读精确字节、先manifest pin后有界下载及正常管理入口|13门禁/142次Go/125次Rust冻结验收通过；可信目录和router正常恢复待补|
|公开Protobuf checkpoint（阶段102）|§9/10.5/17.7；WP08、WP09、WP24|LOCAL_SCOPE_VERIFIED|公开格式绑定完整快照、committed Kafka游标、owner/代际/范围与原字节摘要，不可变先payload后manifest发布|13门禁/74次Go/42次Rust、同轮3组跨语言向量与源码/901固定输入通过；远端存储/来源资格与正常恢复后续|
|显式常驻分析worker（阶段101）|§16/28.21/28.23；WP19、WP21|LOCAL_SCOPE_VERIFIED|Go只读角色journal-continuous、耐久游标周期消费、取消/错误停止及跨语言配置|13门禁/97次Go/81次Rust、实际Kafka连续消费/重启/异常事实拒绝通过；ClickHouse持续事实仍缺|
|EVM原准备独立执行界面（阶段100）|§14/15/21；WP15、WP16、WP25|LOCAL_SCOPE_VERIFIED|按匹配银行显式开放本地执行，浏览器SHA256核对原wire、报价/视图/准备hash、实际收付费率|16门禁/168次Rust、13Node、新旧18Chrome与桌面/移动截图通过；901固定/97参考输入绑定|
|EVM缓存RPC准备到独立worker（阶段99）|§14/15/28.17–28.18；WP14–16|LOCAL_SCOPE_VERIFIED|原API/RPC/组件/wire/独立receipt绑定、双模式显式执行接口、准备替换撤权及有界缓存|15门禁/167次Rust、双模式实际HTTP/worker、错误资金拒绝、901固定/97参考输入通过；界面接线后续|
|EVM来源块与执行块费用上下文（阶段98）|§14.2/14.3/28.17；WP15|LOCAL_SCOPE_VERIFIED|来源H base fee与H+1执行费分开承诺，真实来源观察不覆盖执行环境|8门禁/134次Rust，16原银行/双模式原header费用和错源/缺源/cap通过，901固定输入绑定|
|EVM ExactIn多池与双模式正常HTTP（阶段97）|§14/15/28.16–28.18；WP14–16、WP25|LOCAL_SCOPE_VERIFIED|完整ExactIn简单路径绑定、逐hop源码/源银行、原交易/准备/VM与双模式3hop HTTP|17门禁/168次Rust、6原输入路径/9重算变形和3hop双模式HTTP、901固定/97参考输入通过|
|EVM多池ExactOut原交易执行（阶段96）|§14/28.16–28.17；WP14、WP15|LOCAL_SCOPE_VERIFIED|实际原Router多池交易银行、逐hop身份/状态前后核对、统一共享预算和准备组件|11门禁/135次Rust、12原矿工回执全声明后状态/6目标路径、901固定输入/97参考输入通过|
|EVM ExactOut正常HTTP独立模拟（阶段95）|§14/15/28.17–28.18；WP15、WP16|LOCAL_SCOPE_VERIFIED|正常/simulate选ExactOut计划，同二进制worker、独立证据缓存及过期/撤权/取消处理|12门禁/151次Rust、实际双模式HTTP/worker、899固定输入通过；多池/可信RPC银行继续|
|EVM ExactOut准备与独立执行（阶段94）|§14/28.16–28.17；WP14、WP15|LOCAL_SCOPE_VERIFIED|完整外层目标/最大输入/未用请求预算与内层交易双重重算、同银行资金检查|8门禁/129次Rust、16原银行/16篡改资金/4预算取消、899固定输入通过；正常HTTP执行继续|
|完整交付范围与外部验收边界审计（阶段93）|§7/14.2/18/20/28.25；WP01、WP23、WP24|LOCAL_SCOPE_VERIFIED|逐项区分本地代码差距与正式链/部署输入，配置摘要、当前CLI正负对照、开发Kafka拓扑|5检查/12次审计控制、当前CLI正负对照通过；已确认正式来源与独立故障域部署输入缺失，完整功能/生产仍false|
|EVM原Router单池ExactOut独立执行（阶段92）|§14/28.16–28.17；WP14、WP15|LOCAL_SCOPE_VERIFIED|原swapTokensForExactTokens与完整目标绑定、实际输入/收款、未用预算/退款分列，原gas及完整后状态|8门禁/126次Rust、16原银行/3类revert/有效多池拒绝及899固定输入通过；多池VM/准备API/真实链资格仍缺|
|EVM实际HTTP/worker动态费用（阶段91）|§14.2–14.5/15；WP15、WP16|LOCAL_SCOPE_VERIFIED|API从原REVM receipt有效价计算实际费用，核对base burn/tip，保留max fee承诺|10门禁/122次Rust及实际HTTP/同二进制worker通过，899固定输入绑定；真实当前状态与生产仍缺|
|EVM准备组件与同银行独立执行（阶段90）|§14.1–14.5/28.16–28.17；WP14、WP15|LOCAL_SCOPE_VERIFIED|严格重算组件/unsigned wire、同银行余额/nonce/allowance与实际原Router；getter费用上下文对齐|8门禁/121次Rust、16银行/25重算篡改/10控制及899固定输入通过；真实状态DB/当前链规则/API执行仍缺|
|EVM独立仿真动态费用（阶段89）|§14.2–14.5/28.17；WP15|LOCAL_SCOPE_VERIFIED|仿真传递EIP-1559 cap/tip，记录同次REVM调用者及coinbase余额、实际执行费/base burn/tip|8项门禁/117次Rust、16原Router对照和899固定输入通过；当前链规则/L2费用/可信当前状态仍缺|
|EVM固定块RPC准备与API（阶段88）|§14/15/21；WP04、WP14、WP15、WP16、WP25|LOCAL_SCOPE_VERIFIED|同块RPC身份/完整V2状态与资金/nonce观察、显式API、费用与交易下载；独立组件/RPC/API摘要|15项门禁/51次Rust/12项Node/9项Chrome，899固定输入/46运行时输入同批绑定；独立EVM执行/链特定费用/真实当前来源与生产仍待完成|
|EVM计划绑定与准备组件（阶段87）|§14/15/21；WP14、WP15、WP16|LOCAL_SCOPE_VERIFIED|完整V2 ExactOut绑定、uint256资金/费用、无损数量与unsigned EIP-1559组件|13项门禁/62次Rust/95份独立ABI-RLP签名对照，899份固定输入/185份参考依赖绑定；RPC/API已由阶段88接通；当前链规则/独立VM/钱包继续开发，EVM能力未开放|
|基线图复用与闲时缓存回收（阶段86）|§10/13.2/28.11/28.14；WP09、WP12、WP16|LOCAL_SCOPE_VERIFIED|基线/恢复/scope扩容复用图；三类缓存定时回收、锁外析构与HTTP后台任务关闭|8项门禁/41次Rust测试，899份固定测试输入与46份运行时输入同批绑定；完整池状态COW、内存压力与生产容量继续待完成|
|候选图增量索引与COW（阶段85）|§13.2/28.11/28.14；WP09、WP12|LOCAL_SCOPE_VERIFIED|物理池方向边索引、不可变共享邻接边和状态副本局部发布；10项新增回归|8项门禁/249次Rust测试通过，源码和341份测试输入同批绑定；beam/共享DAG/严格收益界/生产容量仍待完成|
|Raydium CLMM高级经典SPL执行（阶段84）|§12/14/15/23；WP05、WP14、WP15、WP16、WP25|LOCAL_SCOPE_VERIFIED|Bitmap/自定义seed/动态费三个经典SPL模型完整准备、同捕获SBF、显式ATA/WSOL和钱包回执；能力/21|162API/162重启回执/324原交易，33门禁/306次Rust/667项Chrome（181新增）通过；见[阶段84](#plan-2026-11-16-rayclmm-advanced-execution)，其他版本/生产资格仍缺|
|Raydium CLMM经典SPL执行（阶段83）|§12/14/15/23；WP05、WP14、WP15、WP16、WP25|LOCAL_SCOPE_VERIFIED|三个固定经典SPL模型的完整准备、同捕获SBF、显式ATA/WSOL、钱包回执；能力/20|56API/56重启回执/112原交易，31门禁/135次Rust/486项Chrome（129新增）通过；见[阶段83](#plan-2026-11-16-rayclmm-execution)，高级扩展/生产资格仍缺|
|Orca自适应SPL执行链路（阶段82）|§12/14/15/23；WP05、WP14、WP15、WP16、WP25|LOCAL_SCOPE_VERIFIED|三个自适应经典SPL模型的Oracle全字节/写权限、资金准备、同次捕获SBF和钱包回执；能力/19|54API/54重启回执/108原交易、28检查/95次Rust/357项Chrome（121新增）通过；见[阶段82](#plan-2026-11-15-orca-adaptive-execution)，其他版本/Token2022/生产资格仍待完成|
|Orca经典SPL执行链路（阶段81）|§12/14/15/23；WP05、WP14、WP15、WP16、WP25|LOCAL_SCOPE_VERIFIED|静态/奖励经典SPL完整账户验证、原swap、ATA/WSOL组合、同捕获SBF与钱包回执；能力/18|32份API/32重启回执/64原交易，25检查/88次Rust/236项Chrome（81新增）通过；见[阶段81](#plan-2026-11-15-orca-execution)，adaptive/Token2022/生产资格仍待完成|
|Raydium ATA与WSOL资金准备（阶段80）|§12/14/15/23；WP05、WP14、WP15、WP16、WP25|LOCAL_SCOPE_VERIFIED|经典SPL显式ATA/WSOL补足、临时退款/输出解包；同次System/ATA/Rent捕获、原SBF/独立模拟和单次钱包发送回执|10场景/20原交易、21检查/26次Rust/155项Chrome（40新增资金）通过；见[阶段80](#plan-2026-11-15-raydium-funding)，Token2022/其他版本/生产与历史银行资格仍待完成|
|Composable普通币对ExactOut（阶段79）|§12/13/14/15/23；WP12、WP14、WP15、WP16、WP25|LOCAL_SCOPE_VERIFIED|原StableMath目标、任意BPT位置/完整注册向量、BPT拒绝、顺序/网格分配/API/计划/前端|288目标/216成功、72序列/216步、3顺序/3网格，20检查/52次Rust/6项Chrome通过；见[阶段79](#plan-2026-11-15-composable-exact-out)，执行模板仍未完成|
|Managed Snapshot ExactOut（阶段78）|§12/13/14/15/23；WP12、WP14、WP15、WP16、WP25|LOCAL_SCOPE_VERIFIED|实际压缩权重/fee的GIVEN_OUT、任意币对完整物理状态/网格/顺序分配、API/计划/前端|180目标、9状态/36步、3顺序/3网格、20检查/41次Rust/4项Chrome通过；见[阶段78](#plan-2026-11-15-managed-snapshot-exact-out)，执行模板仍未完成|
|静态Infinity Bin ExactOut（阶段77）|§12/13/14/15/23；WP12、WP14、WP15、WP16、WP25|LOCAL_SCOPE_VERIFIED|独立逐bin输出目标、完整树/active尘埃、共享币种账本同步、路由/计划/API/前端|264原场景、72连续步、2顺序/2网格、48真实burn；20检查/72次Rust/4项Chrome通过，见[阶段77](#plan-2026-11-15-infinity-bin-exact-out)；执行模板仍未完成|
|零hook Singleton CL ExactOut（阶段76）|§12/13/14/15/23；WP12、WP14、WP15、WP16、WP25|LOCAL_SCOPE_VERIFIED|双模型目标驱动数学/manager单池计划、容量网格和同池顺序分配、API与前端|644原始场景（424完整输出/220拒绝）、4连续/4同池序列及4容量网格；20检查/51次Rust/8项Chrome通过；见[阶段76](#plan-2026-11-15-singleton-exact-out)，执行模板仍未完成|
|Balancer标准Stable2 ExactOut（阶段75）|§12/13/14/15/23；WP12、WP14、WP15、WP16、WP25|LOCAL_SCOPE_VERIFIED|独立目标求解、费用/缩放与现金，复用固定块状态/路由/计划/API；模型/12|720原数学场景、45序列180步、3网格及3同池顺序分配；20检查/48次Rust/4项Chrome通过；见[阶段75](#plan-2026-11-15-stable-exact-out)，执行模板仍未完成|
|Balancer不可变Weighted ExactOut（阶段74）|§12/13/14/15/23；WP12、WP14、WP15、WP16、WP25|LOCAL_SCOPE_VERIFIED|三类Weighted独立GIVEN_OUT内核、全资产状态/显式token边、最小输入分配、API及能力版本/11|304原回调场景、48连续步、2组网格/2组同池顺序分配；最终17检查、33次Rust、12项Chrome通过，见[阶段74](#plan-2026-11-14-weighted-exact-out)；执行模板及其他协议仍未完成|
|链/DEX解析、路由与兑换量计算收尾|§3/7/8/12/13；[计算收尾计划](#plan-2026-11-08-quote-completion)|IN_PROGRESS|新增最多8路拆单、独立池网格动态规划、共享状态有界搜索、浅池候选与预算报告；Velodrome V2独立版本和216原数学/8序列24步已实现|阶段46–48补齐旧LM、Slipstream及LFJ独立ExactOut注明范围；阶段49–53补齐限定DLMM MM/OnlyY/奖励/订单/Token2022/bitmap、Pump独立ExactOut及PumpSwap/毕业迁移；阶段57–59完成卖出目标及前端账户准备/包装，阶段60已补临时WSOL解包/余款退款，阶段61补限定Token2022收款ATA/withheld联验；策略交叉、其他协议组合/净链费用和第二批链仍有缺口，不宣称全链/全DEX完成|
|R01–R04真实来源、持续采集与范围恢复|§7/8/18/27.3/28；[11项计划](#plan-2026-11-06-r01-r04)|IN_PROGRESS|三服务当前全check及167 Rust定向/174 EVM race、普通23检查/8恢复通过；13 EVM+3 SBF源码精确重建，新Polygon/Arbitrum原VM及normal收款一致|动态目录、同KV恢复、实际PG/Kafka与normal流已接并验；全目录可信历史bank/来源独立性/生产资格仍待闭合，参见当前核对报告（`integration/artifacts/local/r01-r04-current-final-audit.json`，已清理）|
|EVM在线头时间准入与撤销|§7.3/9/23；WP05、WP09、WP25|LOCAL_SCOPE_VERIFIED|在线head与提交前统一新鲜度检查，未来/过期头拒绝、旧引用保留及Tick撤销定向/全量通过|真实进程未来head故障撤销/恢复通过；独立可信头、完整provider矩阵及共识仍未完成|
|旧CLMM验收优化模式拒绝校验|§23/28；WP25|LOCAL_SCOPE_VERIFIED|旧dynamic/seed验收78处显式拒绝保留条件/消息/异常；24项普通/优化定向回归和子进程防回归通过|212项辅助回归普通/优化均通过，独立审查关闭；仅修复验收可靠性，不重新宣称旧协议完整验收|
|Managed V2实际发布版与非零AUM|§7.3/11.3/12.2/12.9/12.11/12.14；WP02–05、WP11–14、WP25|IN_PROGRESS|T34.11和T34.12注明有限范围已验；独立BPT状态/wire、固定H解析、精确转换/Funds路由/实际API、97计划100原交易和完整后状态通过最后同freeze|已启用breaker CB1–CB5有限GIVEN_IN已通过同freeze最终验收；随后rate/mixed及其他会计/参数组合；可信链/ExactOut/公开执行/生产仍未完成，原26完整WP仍0|
|V2完整Swap状态与未同步余额|§7.3/11.3/12.2/12.9/12.14/14；WP02–05、WP10–15、WP25|LOCAL_SCOPE_VERIFIED|独立profile、完整raw/累计价/锁、固定块解析、精确数学及共享状态、BOUND/实际VM后状态已通过限定验收|24原case/70解析投影、273账户1525槽差分、14进程检查/7隔离模拟/42后观察、2故障恢复/重启与双审通过；原Vault等其他协议/真实链及生产范围仍未完成，见[本批计划](#plan-2026-11-03-v2-complete-swap)|
|独立EVM交易与真实余额证据|§14/15、§28.16–18；WP14–16、WP25|LOCAL_SCOPE_VERIFIED|严格REVM/BOUND/真实余额、隔离worker及正常simulate API已完成限定本地验收；10原交易/110账户494槽及50篡改检查通过|双副本7隔离模拟、2来源故障恢复/重启、三服务全量和两阶段审查通过；可信链状态/完整链规则与费用、其他协议/拆单、VERIFIED/build仍未完成，见[本批计划](#plan-2026-11-03-independent-evm)|
|Orca adaptive dynamic arrays组合|§7.3/8.12/11.3/12.2/12.5/12.9/12.11–12.14；WP02–05、WP09、WP11–13、WP20、WP25|LOCAL_SCOPE_VERIFIED|完整dynamic/mixed解析、数学/后状态与旧版本隔离通过限定验收，20原bank/4013byte wire及44篡改反例通过|双副本20请求/39原计划步骤、16来源故障恢复/重启和两阶段审查通过；保留Token2022/补充稀疏/领取边界，见[本批计划](#plan-2026-11-03-orca-adaptive-dynamic)|
|Orca adaptive初始化奖励组合|§7.3/8.12/11.3/12.2/12.5/12.9/12.11–12.14；WP02–05、WP09、WP11–13、WP20、WP25|LOCAL_SCOPE_VERIFIED|独立版本的20bank完整RPC/内核/普通报价、初始化奖励增长/跨tick及共享allocation已完成限定验收|双副本20请求/36步原选中路径重放、13来源故障撤销恢复/重启和两阶段审查通过；reward领取、dynamic arrays/Token2022另列，见[本批计划](#plan-2026-11-03-orca-adaptive-rewards)|
|Curve Crypto活动ramp普通服务|§7.3/11.3/12.2/12.9/12.11–12.14；WP02–05、WP09、WP11–13、WP25|LOCAL_SCOPE_VERIFIED|独立ramp profile、field26起点、固定块解析、普通数学与完整后状态已完成限定验收|28原交换/6序列/22序列内及4次管理调用间连续状态、8公开管理调用、双副本56报价、11来源故障恢复/重启与两阶段审查通过，见[本批计划](#plan-2026-11-02-curve-crypto-ramp)|
|Infinity Bin完整树与普通服务接入|§7.3/11.3/12.2/12.9/12.11–12.14；WP02–05、WP09、WP11–14、WP25|LOCAL_SCOPE_VERIFIED|独立field30状态、完整初始化树/储备/share、原manager模板/Vault immutable与固定块读取、跨Bin普通分派及共享manager账本已接入并定向通过|原manager/Vault正常部署getter与五原bank、原跨Bin四连续交易、16burn场景48交易及树外active校验通过；双副本10请求/10来源故障撤销恢复、重启/独立计划重放与两阶段审查完成，见[本批计划](#plan-2026-11-02-infinity-bin-online)|
|Orca adaptive完整Oracle与普通服务接入|§7.3/8.12/11.3/12.2/12.5/12.9/12.11–12.14；WP02–05、WP09、WP11–13、WP20、WP25|LOCAL_SCOPE_VERIFIED|新独立profile/field51完整Oracle与reserved、真正fee-tier PDA、同finalized bank读取、普通报价完整后状态已接入并定向通过|原pack20 banks/16成交/4拒绝、四组共享池、独立protoc、双副本20请求、11来源故障撤销/新generation恢复、无关EVM报价/重启与VERIFIED/build拒绝通过；仅注明范围，见[本批计划](#plan-2026-11-02-orca-adaptive-online)|
|Curve Crypto完整交换状态与服务接入|§7.3/11.3/12.2/12.9/12.11/12.14；WP02–05、WP09、WP11–13、WP25|LOCAL_SCOPE_VERIFIED|固定d7d04cd9原池及原V4 LP、价格调整/admin mint/gulp、完整状态/wire、固定块解析及普通报价分派已实现|原参考60案例及30场景/双副本60请求、完整live protobuf、五类来源故障撤销/更高generation恢复、Solana保活、router重启及VERIFIED/build拒绝通过；settled A/gamma范围，见[本批计划](#plan-2026-11-02-priority-extensions)|
|Orca adaptive完整交换与Infinity跨Bin内核|§12.2/12.5/12.11/12.14；WP02、WP03、WP05、WP11、WP14、WP20、WP25|PARTIAL_LOCAL|Orca逐group费率/时间/波动及完整fixed-array exact-in内核；Infinity完整初始化树的多bin遍历、费用/储备/active后状态与原子失败|原程序/原BinPool差分已实现并定向测试；Orca经典SPL/fixed-array/未初始化奖励的完整adaptive exact-in内核已补；Orca完整Oracle/限定在线profile已在[后续服务批次](#plan-2026-11-02-orca-adaptive-online)验收；Bin manager/tree来源与静态零hook在线profile已在后续Bin服务批次完成限定验收，见[原内核批次](#plan-2026-11-02-priority-extensions)|
|MetaUSD标准meta/3CRV直接兑换|原§7.3、§11.3、§12.2/12.9/12.11/12.14 / WP02–05、WP09、WP11–13、WP25|LOCAL_SCOPE_VERIFIED|独立clone/原实现准入、固定区块完整余额与base VP依赖、直接兑换及共享池重算、双副本恢复完成|4个实际初始化原clone/32交换与8探针；正式4组服务流程通过；underlying/真实base部署与完整执行仍未完成，见[本批计划](#plan-2026-11-01-metausd-direct)|
|Managed非零BPT费用参考与状态契约|原§7.3、§11.3、§12.2/12.11/12.14 / WP02–05、WP11、WP13、WP25|PARTIAL_LOCAL|原回调费用/铸币完整后状态、同收款方合并、独立wire和固定区块依赖组件已实现，并接入 `balancer-v2-managed-fees` 的 ExactIn 多资产入口|旧源码runtime超EIP170，正常配置/parser及公共报价仍受部署版本门禁限制；ExactOut费用逆解、实际可部署版本服务闭环未完成，见[本批计划](#plan-2026-11-01-managed-bpt-fees)|
|Curve NG标准资产完整服务链路|原§7.3、§11.3、§12.2/12.9/12.11/12.14 / WP02–05、WP09、WP11–13、WP25|LOCAL_SCOPE_VERIFIED|独立版本2–8币固定区块读取、完整余额与EMA、原合约差分、共享池连续兑换及双副本恢复通过|147次原交换、112次双副本报价、三组正式流程与两阶段审查；特殊Token/外部rate和其他Curve模型仍未完成，见[本批计划](#plan-2026-11-01-curve-ng-full-state)|
|进度文档精简|原§28 / WP01|LOCAL_SCOPE_VERIFIED|已删除嵌入JSON、设计原文和重复归档，改为普通表格；检查脚本直接读取表格|28项文档/映射及25项验收辅助测试通过；历史批次明确链接，独立审查问题已关闭|
|EVM区块receipt与日志完整性|原§7.3 / WP04、WP05|LOCAL_SCOPE_VERIFIED|结构、关联、去重和全块日志连续顺序校验完成；19类异常、Go全check/race及审查通过|新旧27份范围回归全部通过；不新增DEX数学支持，后续已补多源与RPC声明root局部核验；独立可信区块头仍待补|
|EVM多RPC完整receipt交叉核验|原§7.3 / WP04、WP05、WP07|LOCAL_SCOPE_VERIFIED|同块完整receipt交叉核验、hash字节等价、严格日志hex、累计预算和流式摘要完成|13项专项、17种来源异常/9种编码边界、PG/Kafka双副本11项及新旧28范围回归通过；两阶段审查关闭，后续已补RPC声明root重算；共同伪造区块头/根与来源独立证明仍缺|
|Solana原始RPC账户证据WAL|原§7.3、§8.9–8.14 / WP03、WP05、WP20、WP24|LOCAL_SCOPE_VERIFIED|正常RPC解码前sync证据、hash链/耐久head、损坏/容量/写错失败封闭、有界后台任务与鉴权status完成|7项存储/2项RPC/异步准入和marker读预算、3种底层log破坏、10项真实双副本与新旧29范围回归通过；仅RPC接收证据，不代表Yellowstone/完整fork已完成|
|EVM完整receipt根校验|原§7.3、§28.7 / WP03、WP05、WP07|LOCAL_SCOPE_VERIFIED|主源/witness标准type0–4及legacy post-state根重算，严格JSON/outcome/gas/bloom校验；共同删尾与前后root变化拒绝|12组独立向量、13项PG/Kafka双副本、49项验收辅助与31项CI/文档检查、新旧30范围回归通过；SPEC/质量审查关闭；独立可信区块头/共识与链专有receipt仍未完成|
|Solana同finalized context有界账户分片|原§8.12、§8.15、§28.22、§28.26 / WP04、WP05、WP20、WP24|LOCAL_SCOPE_VERIFIED|最多32片/35调用；同源同slot及逐片Clock、首末bank与总wire预算；16池134账户及13池CLMM seed通过|11项Rust行为及原解析器回归、10项真实双副本进程、三服务check与10个受影响进程范围通过；审查关闭；仅当前bootstrap，流/历史bank/EnsurePoolScope与WAL轮转仍未完成|
|Balancer V2双币不可变50/50权重池|§7.3/11.3/12.2/12.9/12.11/12.14；WP02、WP03、WP04、WP05、WP09、WP11–WP14、WP16、WP20、WP25|LOCAL_SCOPE_VERIFIED|独立field27状态与版本准入、固定区块解析、原始扣费/缩放/舍入、净输入30%限制、uint256/112检查、共享池后余额；修复快照别名与大跨度重锚定漏撤销|765案例、45组180步原onSwap差分，17项PG/Kafka双副本检查；三服务/common检查、91份契约/8份fixture、91项辅助/31项文档测试通过；SPEC/质量审查与普通/优化模式门禁通过；真实部署及执行待完成|
|Balancer V2双币不可变任意合法权重|§7.3/11.3/12.2/12.9/12.11/12.14；WP02、WP03、WP04、WP05、WP09、WP11–WP14、WP16、WP20、WP25|LOCAL_SCOPE_VERIFIED|独立static版本、最小1%/归一化总和、原整数LogExpMath与powUp分支/误差界、双向完整余额；保留旧equal版本与三份冻结fixture；修复新wire adapter-set身份和可调用回归残留成功|5355案例、315组1260步、240pow向量及18项六池PG/Kafka双副本检查，四组当前进程回归和完整检查通过；多币/Managed/Composable/BPT/rate/V3、部署和执行仍未完成|
|五组协议联合增量（请求未全部完成）|§7.3/11.3/12.2/12.9/12.11/12.14；WP02、WP03、WP04、WP05、WP09、WP11–WP14、WP16、WP20、WP25|PARTIAL_LOCAL|新增九个限定profile、多资产物理池/路由、奖励状态、Advanced CL；Balancer/Curve/Solana/Singleton固定原参考与普通进程子集通过|Managed/BPT费用、Composable缓存/供应、Meta underlying/Lending/NG特殊Token及其他Crypto版本、Token2022/adaptive高级组合/orders、任意hook/动态Bin组合/native结算仍未完成；标准NG/Meta/Crypto及Orca adaptive独立服务的后续完成范围分别列于本文件|

本轮请求未全部完成；当前已实现子集及剩余依赖见[本批Bin服务计划](#plan-2026-11-02-infinity-bin-online)，所有进度继续只在本文件更新。

本批限定本地完成：Infinity Bin静态零hook完整树与普通服务接入；上一批Orca adaptive与Crypto标准settled服务保持原范围，hook/native/完整结算及其他组合未完成。原26工作包完整验收仍为0。

上一批完成（限定本地范围）：R06 / §7.3、§11.3、§12.2、§12.9、§12.11、§12.14 / WP02、WP03、WP04、WP05、WP09、WP11–WP14、WP16、WP20、WP25：Balancer V2 双币不可变50/50 Weighted Pool独立解析与exact-in模型已通过该批本地验收。其他合法双币静态权重已由本轮独立版本补齐；Managed/Composable、真实部署及执行验证仍未完成。

<a id="iteration-goals-2026-11-27"></a>

## 迭代目标（2026-11-27）

按开发优先级推进下列目标。每轮验证、清理本轮无效逻辑并提交推送。表中未展开的功能按设计条款和[剩余功能清单](#remaining-priorities)一并验收。

### I1 Managed fee 完整闭环

- 旧版 ExactIn/ExactOut 原始回调差分已通过，覆盖非零协议费、管理费、同收款方合并、溢出回滚及连续交易。
- 旧版 BPT supply、owner 与协议收款方后状态已通过 Rust Protobuf 往返校验。20230411 AUM 原 Vault 的三个连续状态已通过 Go 固定块解析、Protobuf、Rust 副本和 HTTP 报价联验。
- 20230411 主网工厂创建日志和以太坊区块 21,889,949 的六币池状态已由 dRPC 与 Tenderly 各自读取并通过 Go 原解析器校验。历史状态的 swap 开关关闭。
- 退出标准是原始案例、序列案例、跨语言 wire 和正常 API 全部通过。

### I2 Composable Stable 完整状态

- 已实现缓存状态下的待铸协议费与实际 BPT 供给。V5/V6 的池级收益豁免与不变量增长截断有独立分支。V6 原发布工厂和 Vault 的 12 个固定块样本通过差分，覆盖 2、3、5 个底层代币、swap 费、收益费、豁免及负收益。 [差分命令](integration/tools/reference/balancer-composable-v6/README.md)
- BPT 双向 ExactIn/ExactOut 的回调计算已通过 V6 原 Vault 三池各四笔连续交易差分，包含活动 amp、费前和费后铸币、`lastJoinExitData`、旧 rate、注册余额和钱包结算。原 Vault 的三笔限额回滚通过。
- rate cache 刷新已通过原 Vault 的四个未到期和已到期样本及四笔限额回滚。Rust 验证精确到期边界、32 位到期时间溢出和旧 rate 保留。
- 无 rate provider 的 V6 池已通过五个连续状态的 Go 固定块解析、Protobuf 和 Rust 副本重放。Rust 校验四笔 BPT 交易的完整后状态，并通过 HTTP 验证两笔双向 ExactOut 报价。能力目录为 `xone-protocol-capabilities/25`。
- 带有效 rate cache 的 V6 池已接入固定块报价。Go 按区块哈希读取 provider 地址、runtime 摘要、current/old rate、缓存时限和生效缩放，跨语言快照校验区块时间。两组共十个原 Vault 快照通过 Go 解码、Protobuf 和 Rust 副本重放，覆盖八笔 BPT 后状态及四笔 HTTP ExactOut。
- 以太坊主网 V6 工厂创建事件（区块 21,057,106）和带 rate provider 的已初始化池（区块 21,057,112）已分别通过 dRPC 与 Tenderly 验证。Go 正常解析器校验 Vault 注册余额、缩放、rate cache、费率及 BPT 供给状态；同区块读取的实际供给为 45079866305297054。
- 过期缓存已接入固定块报价。经配置确认 `getRate` 与调用上下文无关的 provider 在同一规范块读取刷新值，Protobuf 传递原始缓存与刷新观察，Rust 在进入普通币对或 BPT 回调前更新缓存及缩放。四个新增原 Vault 快照覆盖普通币对 ExactIn、BPT ExactOut、后状态和 HTTP ExactOut。未知 provider 的过期缓存仍会拒绝。
- 对未证明的 rate provider、Vault 和执行组合保持明确拒绝。

### I3 Curve Meta underlying

- Go Meta 解析入口已读取以太坊 3pool 的完整 base 状态。按固定 runtime 核对 LP 合约、池停用状态和币序，读取 18/6/6 位缩放、`A()`、fee、admin balances 和独立 LP 总供给。区块 21,057,112 的同一组状态通过 dRPC 与 Tenderly，Go/Rust wire 和现有 underlying 数学专项通过。
- 同区块多个 Meta 快照必须对完整 base 状态一致。Go 快照克隆独立复制 base 向量。
- 3pool 单币存入按原合约在各代币原始单位中计算费用和管理员份额。主网区块 21,057,318 的 USDC 存入复现了三项费用、余额、管理员余额和 LP 总供给。Meta 底层兑换使用固定块观察到的 base 虚拟价格。
- 路由已接入 Meta 底层 ExactIn/ExactOut，并在 deposit、withdraw 和底层直兑后更新共享 base 余额、admin balances、LP supply 与虚拟价格。两池的连续路由专项覆盖双向报价、共享状态和六位币的 ExactOut 取整。
- 在以太坊区块 21,057,317 的本地分叉部署锁定的 MetaUSD 实现，使用原 3pool、LP 合约和三个底层代币执行连续的底层存入、取出及直接兑换。四个固定块经 Go 解析、Protobuf 和 Rust 副本重放，三笔用户余额差、完整 Meta/base 后状态、六笔 HTTP 报价与末块 checkpoint 恢复均通过。 [复验脚本](integration/tools/reference/curve-meta/underlying.cjs)
- 完整独立执行继续按 I6 验收。

### I4 Solana 高级协议

- 补齐 Token2022 转账费与账户扩展，Orca adaptive Token2022、稀疏数组和奖励领取。
- 补齐 Raydium CLMM 订单、奖励领取和高级组合。
- 为每个版本提供独立状态、wire、解析、数学、SBF/CPI 和账户余额差验证。
- Yellowstone、版本化 bank、fork 和补洞继续作为独立门禁。

#### 本轮迭代目标（I4-T2022，2026-11-27）

- 将 Token-2022 `TransferFeeConfig` 的 epoch 选择、百分比向上取整、最大手续费封顶和 ExactOut 逆向手续费计算收敛为单一实现。
- 将 Token-2022 账户的 `TransferFeeAmount` 与可选 `ImmutableOwner` TLV 校验复用到报价域模型和 Solana 交易准备，未知、重复、截断和填充扩展拒绝。
- 通过既有固定 bank 回放、domain 边界测试和执行准备回归后提交。由于仓库当前缺失历史 `meteora-token2022.json` 与 `evm/baseline.json` fixture，本轮保留定向测试通过和全量测试阻塞证据，不将 I4 标记为完成。

#### SwapV2 与已入账奖励领取验收

- Orca SwapV2 在受控 bank 上执行公开部署的 Whirlpool、SPL Token、Token-2022 和 Memo。部署账户由 PublicNode 在 finalized slot 474441254 同批读出。Whirlpool 与 Token-2022 ProgramData 分别指向部署 slot 457152296 和 460497271。公开部署字节和本地构造的池、账户及 Clock 分别保留来源记录。 [复验说明](integration/tools/ci/orca-token2022-execution.md)
- 同一受控 bank 的 HTTP 入口和独立 SBF 验证覆盖 6 组 `CollectReward` / `CollectRewardV2`。三个奖励索引分别覆盖全额、部分和空 vault，验证 NFT 所有者、单单位 delegate、Token-2022 到账净额、withheld 和 position/vault/收款账户后状态。另有 24 项 HTTP 拒绝验证和两组冻结收款账户的原 CPI 失败回放。该范围使用独立 Position PDA、已入账奖励和现有收款账户。奖励更新组合与签名回执仍待验收。 [复验说明](integration/tools/ci/orca-token2022-execution.md)

#### Raydium CLMM 限价订单验收

- `raydium-clmm-limit-orders` 已接入 `6d7a0f82` TickState 的订单阶段、待撮合数量、部分成交余量和剩余比例。Rust 正常 finalized RPC 验证了只有订单的活跃 tick 和已清空 tick 的历史阶段。26 项解析测试、Go/Rust Protobuf 往返和生成一致性检查通过。
- 双向 ExactIn、ExactOut 和三种收费模式已通过 216 组原 `swap_internal` 差分、12 笔 HTTP 报价及经典静态原程序回归。验证 180 笔成交的完整状态，含 34 次清空 bitmap 位，并核对 36 组拒绝。 [原程序差分](integration/tools/reference/raydium-clmm/README.md)
- 公开部署程序在受控 bank 上通过 24 组 HTTP 无签名准备和独立 SwapV2 回放，覆盖部分成交与清空、用户 Token CPI 收付、两个 vault、订单 tick 和池 bitmap 后状态。另有 24 组限额回滚和 24 组订单账户变更拒绝。能力目录升至 `/29`，签名提交与真实回执待验收。 [复验说明](integration/tools/reference/raydium-clmm/README.md#deployed-swapv2-limit-order-replay)

#### Raydium CLMM 头寸费用与奖励领取验收

- 通过正常 HTTP 入口准备零流动性变动的 `DecreaseLiquidityV2`。在同一 finalized bank 绑定 Position PDA、头寸 NFT、上下界 tick、奖励 vault、mint 和收款账户。
- 公开部署程序在受控 bank 通过 5 组独立 SBF 回放，覆盖活跃奖励累计、结束后奖励领取、部分与空 vault、费用或奖励关闭时的结算、Token-2022 到账净额与 withheld。逐笔核对用户、vault、Position 和池的后状态。另有 10 组无效 NFT 或奖励账户顺序拒绝，以及 4 组冻结 Token-2022 收款账户的 CPI 回滚。 [复验说明](integration/tools/reference/raydium-clmm/README.md#deployed-position-settlement-replay)

#### Raydium CLMM 高级 SwapV2 组合验收

- `raydium-clmm-advanced` 在一个版本中支持动态费、扩展 bitmap、限价订单和奖励累计。复用现有 Protobuf 字段和同 bank 账户验证，能力目录升至 `xone-protocol-capabilities/30`。
- 双向 ExactIn、ExactOut 和三种收费模式通过 24 组原程序差分。复用 bitmap 位定位处理读写，修复了清空扩展数组中的最后一个订单时误写主 bitmap 的问题。6 组数组清空案例核对了扩展 bitmap 后状态。公开部署程序在受控 bank 上通过 24 组 HTTP 无签名准备及独立 SwapV2 回放，并核对 24 组限额回滚和 24 组账户变更拒绝。 [复验说明](integration/tools/reference/raydium-clmm/README.md#deployed-advanced-swapv2-replay)
- Rust finalized RPC 解析套件 27 项通过，Go 通过 24 组原程序前后状态的 Protobuf 往返。能力目录结构测试通过。基于历史模型文件的能力测试只收集到 2 个有效模型，进度检查的 3 项失败涉及历史报告或本地链接缺失。

#### Orca 奖励更新与领取组合验收

- 正常 HTTP 入口通过 `claim.updateBeforeCollect` 在同一笔交易中先执行 `UpdateFeesAndRewards`，再领取奖励。准备阶段在同一 finalized bank 绑定 Position 的两侧 tick array 和 Clock，并复用 swap 的奖励累计逻辑。
- 原部署程序通过 6 组更新后领取和 6 组直接领取回放。覆盖固定与动态边界数组、视图中的稀疏数组、标准 SPL 与 Token-2022 的全额、部分和空 vault。核对 Whirlpool、Position 全部费用与奖励检查点、奖励 vault、收款账户和 Token-2022 withheld，另复现冻结收款账户的 CPI 失败。SwapV2 的 24 组资金路径和 24 组阈值回滚复验通过。 [复验说明](integration/tools/ci/orca-token2022-execution.md#position-reward-collection)

#### Raydium 高级头寸结算验收

- `raydium-clmm-advanced` 的费用与奖励领取复用 SwapV2 的 custom seed 池 PDA 和扩展 bitmap 校验。头寸边界超出主 bitmap 时，`DecreaseLiquidityV2` 携带扩展账户。
- 高级组合与经典奖励版本分别通过 5 组独立部署程序回放。高级组合覆盖非零 seed、主 bitmap 与扩展 bitmap 两侧的头寸边界、活动订单、动态费状态、Token-2022 奖励净额，以及 5 组扩展 bitmap 数据变更拒绝。签名和真实回执继续按 I6 验收。 [复验说明](integration/tools/reference/raydium-clmm/README.md#deployed-position-settlement-replay)

### I5 V4/Infinity

- 原版 Uniswap V4 PoolManager 在 Cancun 通过 8 组双向 ExactIn、ExactOut、ERC-20 与 native 组合。受控 hook 使用 beforeSwap 覆盖费，以及 beforeSwap 和 afterSwap 的会计差额。逐笔核对调用方、hook、manager 和接收账户余额，另有 8 组未结算 hook 差额的原程序回滚。 [复验说明](integration/tools/reference/singleton-advanced/README.md#v4-poolmanager-hook-and-native-settlement)
- `uniswap-v4-advanced` 与 `pancake-infinity-cl-advanced` 的无 swap 回调快照接入 ExactOut。路由绑定 manager，计划校验仅允许高级 CL 单池路由使用原生币，报价应用快照中的 LP 费和方向性协议费。用原程序的 CL 语料投影为动态费与原生币快照，核对输入、目标输出和报价后状态。4 组正常 HTTP 报价通过，覆盖两个协议和双向原生币，旧版 644 组输入与回滚以及连续和多池回归通过。能力目录升至 `/31`。
- 已接入 `v4-settlement-hook-v1` 的固定区块收集、Go/Rust Protobuf 和正常 HTTP 报价。收集器核对 hook 权限、编译产物及两处不可变的 PoolManager 地址。8 组原 PoolManager 双向 ExactIn、ExactOut、ERC-20 与原生币交易的报价、价格及 tick 后状态一致，另通过 8 组原程序回滚。能力目录为 `xone-protocol-capabilities/32`。 [复验说明](integration/tools/reference/singleton-advanced/README.md#v4-poolmanager-hook-and-native-settlement)
- V4 单池执行合约通过原 PoolManager 的 8 组 ERC-20 与原生币双向 ExactIn、ExactOut，核对实际收款、授权额度、原生币退款和 8 组滑点回滚。正常 HTTP build 按计划绑定执行合约，固定区块核对合约、manager 与代币身份，8 组模板及 8 组执行合约字节码变更拒绝通过。签名、独立执行和真实回执继续按 I6 验收。
- Infinity CL 已知 hook 的报价通过 8 组原版 Manager/Vault 回放，覆盖双向 ExactIn、ExactOut、原生币和 ERC-20。Go 固定块收集核对 hook 位图及其 Manager/Vault 依赖，Rust 核对成交金额、价格和 tick。能力目录为 `xone-protocol-capabilities/33`。
- 固定版本的 Infinity CL Manager、Bin Manager 与 Vault 已通过 16 组双向 ExactIn、ExactOut、ERC-20 与原生币交易，核对 hook、付款方、接收方、Vault、应用储备和结算锁。另通过 16 组 `CurrencyNotSettled` 回滚。继续把快照、hook 身份和报价接入正常服务。 [复验说明](integration/tools/reference/singleton-advanced/README.md#infinity-manager-and-vault-settlement)
- Infinity Bin 已知 hook 支持动态费及原生币和 ERC-20 的双向 ExactIn、ExactOut。8 组跨 bin 成交通过原版 Manager/Vault 和正常 HTTP 报价比对，核对三个 bin 的储备、份额、活跃 ID 与 Vault 应用储备。两个固定块采集回放覆盖 ERC-20 与原生币，验证 Go → Rust Protobuf，并拒绝错误位图、字节码和未知模型。移除了报价采集中的 `maxBinStep()` 读取，能力目录为 `xone-protocol-capabilities/34`。 [复验说明](integration/tools/reference/singleton-advanced/README.md#infinity-manager-and-vault-settlement)
- 静态 Infinity Bin 已用重新生成的原版 Manager 语料复验。ExactOut 的 264 个独立案例含 219 次成交和 45 次拒绝，另有 72 个连续步骤及 6 个有序步骤。Go 固定块采集、两套 Rust 域的完整树与 Protobuf 校验，以及 Rust 数学、路由、优化和 HTTP 专项均通过。独立 Bin wire 生成器自建快照，移除无效的 `maxBinStep()` mock 和跨协议目录总数断言。本次重新生成的语料在隔离目录复验，历史冻结语料的原始文件仍缺失。
- Infinity CL/Bin 正常 HTTP build 已通过 16 组交易模板验收，逐组验证执行合约字节码变更拒绝。固定区块核对执行合约的 Manager、Vault、池类型与代币身份。原版 Manager/Vault 回放通过 16 组实际收款、ERC-20 扣款与原生币退款，另通过 16 组超限回滚与两个未授权回调拒绝。独立执行和真实回执继续按 I6 验收。
- unknown hook 继续保持拒绝。

### I6 独立执行与真实收款差

- 补齐 EVM 固定状态 DB、链规则、REVM、gas 和 L1 fee。
- 补齐 Solana 版本化账户、SBF、Token CPI、CU、账户锁和 blockhash。
- 覆盖 ExactOut 退款、授权、wrap、费用和收款后状态。
- 只有独立余额差和回执验证通过后才开放执行能力。

#### 当前迭代目标（I6）

1. Cancun 内核支持普通调用和同环境余额观察。按 EIP-4844 校验固定区块的 excess blob gas 与派生 blob 基础费，拒绝不支持的 blob 交易。26 项 REVM 执行测试和 10 项 StrictDB 测试通过，覆盖交易间瞬态存储清空、异常头拒绝及 Shanghai 回归。
2. V4、Infinity CL 与 Infinity Bin 单池计划已接入固定 H/H+1 状态回放和正常 Quote API worker。24 组原版合约交易核对了实际付款、收款、原生币退款、gas 和 nonce。另有 24 组限额回滚、24 组缺失 Manager 存储和 24 组 hook 字节码变更拒绝。普通 HTTP 验收覆盖全部 24 组报价、构建与 worker 回放，以及 24 组缺失存储拒绝。构建和回放的交易字节与绑定摘要一致，响应标记为本地状态证据。Quote API 全量单测编译仍受既有 V2 和价格发布 fixture 缺失影响。 [复验说明](integration/tools/reference/singleton-advanced/README.md#quote-api-fixed-state-replay)
3. V4、Infinity CL 与 Infinity Bin 的仿真入口接受钱包提供的 EVM 签名字节。worker 复用 EIP-155、EIP-2930 和 EIP-1559 验签，核对交易哈希、发件人、链、nonce、费用、calldata 和金额，并把交易哈希写入本地回放回执。通过 24 组钱包签名 HTTP 回放和 24 组有效签名但 nonce 错配拒绝。
4. `checkEvmReceipt` 将签名回放与固定 H/H+1 的节点回执、gas、有序日志和前后余额比对。Anvil 1.4.4 Cancun 节点实际出块的 24 组 HTTP 样本通过，另分别通过 24 组错误 gas、日志、余额和重组注入拒绝。当前证据范围为无提款的单交易块，来源标记 `CANONICAL_RPC_OBSERVED`。 [复验说明](integration/tools/reference/singleton-advanced/README.md#quote-api-fixed-state-replay)
5. 多交易块按交易索引读取目标交易前后的状态，核对发送方 nonce、到账余额、gas 和有序日志。24 个由 Geth v1.16.7 出块的三交易样本通过，目标交换位于中间，后续交易再次使用同一发送方。错误 gas、日志、余额、重组及节点忽略交易索引分别通过 24 组拒绝验收。 [复验说明](integration/tools/reference/singleton-advanced/README.md#quote-api-fixed-state-replay)
6. Cancun 固定回放支持 EIP-4788，用户交易执行前应用父信标块根。校验固定系统合约代码及环形缓冲区槽位，节点回执比对同时检查区块头、H 上代码与 H+1 两个槽位。24 组 Geth 样本通过，错误区块根和错误存储证据分别通过 24 组拒绝。13 项仿真 crate 单测通过，6 项需要留存原版 OP 语料的测试保持忽略。
7. Solana SBF 入口现接受可选 `signedTransaction`，复用严格 Ed25519 wire 验证，要求消息、付款人、签名哈希和准备阶段 blockhash 一致，并在 LiteSVM 开启签名校验。签名证据返回 `walletSignature`、交易及消息摘要，未签名证据仍作为提交门禁。现有固定 bank 的正常签名和回执联测继续按完整钱包生命周期验收。
8. Orca 双 Token-2022 受控 bank 已通过两组正常 HTTP 报价、准备和部署程序签名回放，覆盖正向 ExactIn、反向 ExactOut、Token CPI 后状态和两组篡改签名拒绝。ExactOut 计划统一使用共享 Orca 协议判断，保留旧客户端以 JSON null 请求未签名仿真的行为。 [复验说明](integration/tools/ci/orca-token2022-execution.md)
9. 签名仿真与提交预检共用 confirmed RPC blockhash 检查，分别约束区块高度和 RPC slot。签名响应与提交预检记录包含有效性观测，后续费用读取继承 blockhash 观测的 slot 下界。Orca 两组签名 HTTP 回放通过有效期边界，以及过期、未知和陈旧 slot 的六组拒绝。Orca 奖励 HTTP 联测同时通过。钱包提交回归目标的编译依赖尚缺 `dex-router/test/fixtures/evm/baseline.json`。
10. SBF worker 对交易做结构校验，拒绝重复账户键，按 legacy 消息头生成并返回读写账户集合，检查只读账户的回放后状态与原状态一致。Orca 双 Token-2022 两组签名及未签名回放与准备指令的账户权限逐项一致，两个奖励收取 HTTP 目标同时通过。
11. 本地 Agave v3.1.14 实际出块验收通过。Orca adaptive classic SPL ExactIn 经正常 HTTP 报价、准备、独立签名回放和提交，finalized 区块包含同一签名，实收、实付、网络费和付款人 lamport 差与独立回放一致，重开日志后回执相同。旧费用 slot 和签名仿真证据替代原提交证据均被拒绝。 [复验说明](integration/tools/ci/orca-token2022-execution.md#local-validator-receipt)
12. 回执查询继承耐久提交预检的最小 slot，校验 RPC 上下文与返回的交易 slot。查询不到签名时按同一下界检查区块高度。Agave 联测注入陈旧状态响应，API 拒绝且日志字节一致，随后正常完成 finalized 回执。
13. Yellowstone owner 在已提交的同一 bank 上提供账户写入 slot、bank ID、write version 和数据摘要。SBF 仿真按完整 SnapshotRef 与来源凭证匹配 owner，逐项核对与准备 capture 重叠的账户并保留捕获的 rent epoch。正常解析服务通过 bank 账户版本读取、错误分叉和撤销后拒绝。两组 Orca HTTP 仿真通过有效证明，以及错误写入 bank、错误摘要拒绝。解析四个目标串行通过 63 项，首次并行运行有一项恢复测试报 `SCOPE_STORE_CONFLICT`，串行复验通过。
14. Solana 准备统一使用 blockhash 和消息费用读取逻辑，要求账户、blockhash、费用、可选 RPC 仿真的观测 slot 依次不下降。准备结果新增 `blockhashObservedSlot`，提交预检与费用验收保留对历史准备记录的支持。三项 Orca 受控 HTTP 联测通过，覆盖两组陈旧费用响应拒绝。三项费用证据验收及启用 RPC 仿真的 Agave 签名提交和 finalized 回执联测通过。
15. EVM 节点回执支持含提款区块中的非末笔交易，按交易索引读取提款执行前的目标交易前后余额。提款列表与空提款根不一致时拒绝。Geth v1.16.7 的 24 个三交易区块均包含给收款地址的提款，24 个中间交易回执及隐藏提款列表的拒绝均通过。Anvil 1.4.4 的 24 个单交易空提款块回归通过。 [复验说明](integration/tools/reference/singleton-advanced/README.md#quote-api-fixed-state-replay)
16. EVM 节点回执同时核对付款人原生币的前后余额和 nonce，覆盖双 ERC-20 交换。交易索引查询复用同一账户跟踪结果，付款人也是原生币结算账户时复用同一余额。24 个 Geth 和 24 个 Anvil 回执通过，两组各 24 个仅篡改付款人原生余额的反例均被拒绝。
17. OP 原版签名费用语料已从固定摘要的 op-geth 源码重建。覆盖 Regolith 至 Isthmus 的 448 个场景和 1337 次状态转换，严格执行与费用审计的 7 项 Rust 联测通过。原冻结 V2 文件缺失，重建的 24 组 V2 bank 通过独立验证器，文件摘要与旧冻结值不同。
18. 正常 OP HTTP 签名验收新增 `op_api_http.py --export-validated-sources BANK TRACES EXECUTION_VECTORS OUTPUT`，复核重建 V2 bank 及原始跟踪文件后按摘要绑定导出。Regolith 至 Isthmus 的 Legacy 和 EIP-1559 各覆盖 ExactIn、ExactOut，共 28 个正常报价、构建及签名执行与原版 op-geth 的 gas、L1 数据费、operator fee、费用账户及收款差一致，168 个无效请求被拒绝。验收范围为本地历史状态。 [复验工具](integration/tools/acceptance/op_api_http.py)
19. 重建 Jovian 的 64 个原版费用场景，并从固定源码、依赖和 Rust 1.99.0 构建 Karst 原版执行器。验收工具支持导出两个规则的输入，HTTP 验收不再重复写入同一份报价、构建、签名和仿真文件。Regolith 至 Karst 合计 36 个正常签名执行和 216 个拒绝场景通过，其中 Jovian、Karst 各有 4 个正常场景。 [OP HTTP 复验](integration/tools/acceptance/op_api_http.py) · [Karst 原版执行器](integration/tools/acceptance/karst_execution_vectors.py)
20. EVM 回执按原始顺序重算提款 trie 根，再按已声明的 Ethereum 分叉重建区块头并核对区块哈希。Geth 和 Anvil 的 48 个回执通过。提款列表缺失、金额篡改及同步伪造空列表与空根各 24 组均被拒绝。
21. 含提款区块的末笔 EVM 回执按交易索引读取交易前状态，按已绑定的提款列表还原交易后的原生币余额，并把同一组余额覆盖传给 ERC-20 读取。Geth 的 24 个末笔交易回执通过，覆盖同地址累计提款、原生币收款人提款和余额不足以还原时的 24 组拒绝。Geth 中间交易与 Anvil 空提款块各 24 组回归通过。证据范围为 `TRANSACTION_BEFORE_WITHDRAWALS`。 [复验说明](integration/tools/reference/singleton-advanced/README.md#quote-api-fixed-state-replay)
22. 重建 Meteora 原版 Token-2022 的 120 个场景和 168 个执行步骤，投影五个受控钱包 bank。钱包生命周期与独立 SBF HTTP 测试各一项通过。生命周期覆盖八条回执路径，并验证并发提交只发送一次、确切签名字节、费用读数 slot 顺序和日志重启恢复。Solana 测试从原 bank 构造快照，浏览器专用池由浏览器 harness 单独安装。浏览器 harness 编译通过。 [复验说明](integration/tools/reference/meteora-dlmm/harness/README.md#钱包提交与回执恢复)
23. EVM 节点回执按块哈希读取全块回执，按交易顺序重算 `receiptsRoot`，检查区块内连续日志索引，并从相邻累计值核对每笔交易的 gas。Anvil 单笔、Geth 中间交易及 Geth 末笔交易各 24 个回执通过。三组分别验证缺项、gas、日志和 bloom 篡改，两组 Geth 另验证错序拒绝。 [复验说明](integration/tools/reference/singleton-advanced/README.md#quote-api-fixed-state-replay)
24. EVM 回执逐笔核对签名交易原文的哈希，按声明顺序重算 `transactionsRoot`，将回执位置绑定到同块交易正文。Anvil 单笔、Geth 中间交易及 Geth 末笔交易各 24 个回执通过。三组分别检查原文缺失与哈希不符，两组 Geth 另检查有效签名字节与同步伪造交易列表和回执元数据的拒绝。 [复验说明](integration/tools/reference/singleton-advanced/README.md#quote-api-fixed-state-replay)
25. EVM 回执重建来源 H 块头，验证账户存在性、nonce、余额、代码哈希和声明存储的 MPT 证明，回传 `sourceStateRoot`。本地出块夹具排除不存在的账户和零值存储。Anvil 单笔、Geth 中间交易及末笔交易各 24 个回执通过，三组均验证来源根篡改、缺失账户证明和存储值篡改的拒绝。 [复验说明](integration/tools/reference/singleton-advanced/README.md#quote-api-fixed-state-replay)
26. EVM 节点回执沿来源块父哈希核对声明的 `BLOCKHASH` 输入，检查执行块的 256 块窗口并重建经过的区块头。Anvil 单笔、Geth 中间交易和 Geth 末笔交易各通过 24 个回执，三组均拒绝缺失的中间头和篡改的父哈希。独立 REVM 的 26 项执行测试通过，其中一项直接验证历史哈希读取和窗口外返回零。 [复验说明](integration/tools/reference/singleton-advanced/README.md#quote-api-fixed-state-replay)
27. Solana owner、scope、checkpoint、startup 和就绪测试共用受控 CPMM 账户快照，由正式解码器生成池状态并计算 scope 摘要。移除了对缺失 `solana/baseline.json` 的编译期依赖。解析库 51 项与 owner scope 5 项通过，另有 74 项通过，覆盖受控 Yellowstone 服务、范围恢复、账户继承与流投影。解析库中 3 项外部基础设施验收待运行。
28. Cancun 回执验收按执行块 `stateRoot` 重建信标根合约的账户证明和两个环形缓冲区存储证明，核对运行时代码哈希、时间槽与根槽。Anvil 单笔、Geth 中间交易和 Geth 末笔交易各通过 24 个回执，篡改槽值和清空存储证明均被拒绝，回执保留 `executionStateRoot`。三套回归仍覆盖祖先、提款、收据 trie、交易 trie、账户证明和签名交易拒绝。 [复验说明](integration/tools/reference/singleton-advanced/README.md#quote-api-fixed-state-replay)
29. 下一轮进入 I7 R01，补充真实来源部署身份和逐版本代码、升级槽、历史块及可重复收款差分证据。

### I7 真实来源和恢复

- 完成 R01 真实部署身份、代码、升级 slot 和历史块验证。
- 完成 R02 EVM WS/HTTP 双源、staging 有序提交、raw、DLQ 和工厂发现。
- 完成 R03 Yellowstone 连续流、WAL、fork、补洞和乱序账户恢复。
- 完成 R04 EnsurePoolScope、扩容、长停机恢复和历史隔离。

#### 当前迭代目标（R01 六链历史部署，2026-11-28）

- 六份入库归档已在独立检出统一复验。八个固定块的状态证明、原池重放和 Go 回执根核验均通过。六链 11 项常规报价全部返回 HTTP 200，逐项与原池收款一致，回放无未归档 RPC 请求或公网请求。全目录资格、精确源码剩余差异、链费与生产执行资格仍待验收。[统一复验命令](integration/test/source/deployment-catalog/README.md#从已入库的证据统一复验)
- 固定块采集在读取原合约状态前核对主源与见证源的 chain ID 和创世哈希。见证源的响应必须与主源的 finalized 块号、哈希和状态根一致。重试从已有取证序号继续写入。
- Base 63,180,533、BSC 136,138,407、Arbitrum 528,150,486、OP Mainnet 160,113,647、Polygon 98,081,300 和 Avalanche 101,588,444 均取得双源响应。独立验证器重建六个原生区块头和六棵交易 trie，共核对 261 笔交易。Avalanche 样本的执行高度为 101,588,444，块头声明的状态高度为 101,588,438。
- 从六条链的官方部署目录恢复 11 项工厂和 13 个样例池的固定块观测，逐项核对工厂查询、runtime SHA-256 和工厂的 EIP-1967 存储槽。BSC 的旧锚点遇到 429 和缺失历史 trie，改用 NiniCoin 与 1RPC 核对高度 136,139,735 后，两项 Pancake 部署均采集成功。新增的执行输入绑定到各自采集高度。
- 固定回调辅助合约编译器为官方 Solidity 0.8.19。采集时按发布清单的 SHA-256 校验，离线重放复用已校验的文件。OP Mainnet 的两个原池在 160,113,647 的离线重放得到 166,230 和 165,448 个收款原子单位，网络请求为零。独立验证覆盖该块 75 笔交易、19 个账户、52 个存储槽和 99 次实际公开预状态读取。
- Avalanche 采集保留有界后继头序列。独立重建目标块 101,588,444 和两个后继块的头哈希，核对相邻父哈希，确认 101,588,446 提交目标高度的状态根。省略中间块时验证拒绝。预状态采集按 Coreth 物理键请求证明，回放与核对保留 EVM 逻辑键。所探测的两条公共链路均未返回四个目标账户的可用历史证明。
- BSC 136,140,308 两个 Pancake 原池分别得到 55,329,544,335,295,125 和 55,551,507,327,423,870 个收款原子单位。该轮有 16 个预状态账户，15 个尚缺证明。
- Go 使用原生编码重建 Base、OP Mainnet、Arbitrum、Polygon 和 BSC 五块的区块头、交易根及回执根。共核对 261 笔固定块交易及对应回执。BSC 采用原池执行所在高度 136,140,308，Polygon 结论覆盖该样本中与交易正文对应的 74 笔回执。
- Avalanche 已通过同一套 Go 离线联验。按后继头披露的结算起止高度读取 101,588,441–101,588,444 的完整交易和回执，重算 101,588,446 的窗口回执根，并取得适用于目标执行块的已结算状态根。
- 11 个工厂对应的原池均已在固定块执行并取得正数收款。Avalanche 在超时后从同一归档恢复，两个原池分别取得 660 和 134 个收款原子单位，仍缺 11 个预状态账户证明。重试复用已核对的编译结果，保留旧索引并原子发布更新后的索引。
- OP Mainnet Uniswap V3 在高度 160,113,647 补齐当前 tick bitmap 字内 203 个初始化 tick 的证明，211 次原合约 getter 调用全部通过。证明报告通过索引消除重复内容，由 65.7 MB 缩减至 3.6 MB。常规 Go Parser 与 Rust Router 离线报价 166,230 原子单位，与原池实际收款一致。回放无未记录请求，链费模型仍待验收。
- OP Mainnet Velodrome V2 在同一高度验证原池、工厂和 clone 实现的代码。补齐两个存储键后，六次 getter 调用均通过证明验证。常规 Go Parser 与 Rust Router 离线报价 165,448 原子单位，与原池收款一致，回放无未记录请求。
- Base 在 63,180,533 通过同高度 dRPC 补齐 11 个账户的证明。独立验证覆盖 19 个账户、92 个存储槽以及原执行的 142 次预状态读取。三个原池离线重放的收款分别为 166,255、166,299 和 166,349 原子单位，网络请求为零。
- Base Uniswap V3 和 Aerodrome V2 的 24 份追加输入已入库。独立检出从复验包取得原始数据，两条常规报价均返回 HTTP 200，与原池输出一致，RPC 缺项和回放外网请求均为零。同块 22 笔回执通过 Go 验证。 [复验证据](integration/test/source/deployment-catalog/README.md#base-uniswap-v3-与-aerodrome-v2-固定块报价)
- Base Uniswap V3 与 Aerodrome V2 的常规 Parser 和 Router 复演分别核验 263 次和 6 次原合约 getter，报价与原池收款分别在 166,255 和 166,299 原子单位上精确一致。Aerodrome V2 的回放补齐 Base 区块 1 与启动所需的 deposit 回执。两个成功样本均无未记录 RPC 请求，链费模型仍待验收。
- Base 同高度的 Slipstream Factory、Pool 实现、动态 swap 费模块和 Voter 匹配已有上下文模板。部署的 `CustomUnstakedFeeModule` 将 factory 存在槽 0。用官方 Solidity 0.7.6 重编公开输入，848 字节执行体逐字节一致，CBOR 来源摘要不同。MPT 核对了已部署完整代码、factory 和样例池的 50,000 自定义费率。Go、Rust Parser 与 Router 的上下文领域校验接受这一固定 runtime 身份。
- Base Aerodrome Slipstream 在 63,180,533 用固定状态证明重放了 3,264 次 getter，覆盖 3,010 条预言机观测和当前 bitmap 字内的 210 个初始化 tick。常规 Go Parser 与 Rust Router 报价 166,349 原子单位，与原池收款一致，回放无未记录 RPC 请求。证明归档可由本地留存的 51 份原始证明响应、合约代码响应和原执行证据重建，覆盖 20 个账户、4,359 个存储槽和 142 次原始状态读取。链费模型和生产执行资格仍待验收。
- Arbitrum One 在 528,150,486 经另一公共 RPC 取得缺失的 5 个账户证明。独立验证覆盖 10 个账户、23 个存储槽以及原执行的 49 次预状态读取。常规 Parser 和 Router 回放核对 264 次 getter，Uniswap V3 报价与原池收款均为 166,253 原子单位，无未记录 RPC 请求。链费模型仍待验收。
- OP Mainnet 两项和 Arbitrum One 一项固定块验收的 462 份输入已入库。在独立检出中重算状态证明和 78 笔回执，重放三个原池和常规 Parser/Router。Velodrome V2 补齐正常 Parser 启动所需的高度 1 回执，经两端点核对。三条报价均返回 HTTP 200，原池收款与报价一致，RPC 缺项和回放外网请求均为零。 [复验证据](integration/test/source/deployment-catalog/README.md#op-mainnet-与-arbitrum-one-固定块报价)
- Polygon 改用 finalized 高度 98,086,595。独立验证完成 9 个账户、20 个存储槽和 44 次预状态读取，Go 验证同块的 90 笔回执。Uniswap V3 原池离线重放收到 7 原子单位。常规报价还需补齐当前 bitmap 内的 tick 存储证明。
- Polygon 将部署采集与 getter 证明采集并发执行，在 98,087,772 补齐 134 个初始化 tick 的 536 个存储键。142 次固定块 getter 执行通过证明核验。常规 Go Parser 与 Rust Router 返回 HTTP 200，报价与原池收款均为 7 原子单位，回放无未记录 RPC 请求。Go 验证同块的 52 笔回执。
- Avalanche 改用 finalized 高度 101,595,781。后继头 101,595,787 提交该高度的状态根及回执根。独立验证完成 11 个账户、36 个 Coreth 存储槽和 65 次预状态读取，两个原池离线重放分别收到 660 和 134 原子单位。Go 的后继结算窗口回执验证通过。
- Avalanche Uniswap V3 在 finalized 高度 101,597,996 使用后继块 101,598,001 提交的状态根验证 Coreth 五字段账户及物理存储键。136 次 getter 通过 MPT 重放，覆盖当前 bitmap 字内的 128 个初始化 tick。常规 Go Parser 与 Rust Router 返回 HTTP 200，报价与原池收款均为 659 原子单位，无未记录 RPC 请求。Go 重建 101,597,995–101,597,996 的结算窗口回执根。原始输入和复验命令见 [证据目录](integration/test/source/deployment-catalog/README.md#avalanche-uniswap-v3-固定块报价)。精确源码、链费模型和生产执行资格仍待验收。
- Go 服务验收直接读取上述 Avalanche 归档。目录发现、历史读取、归档重放和在线来源复查均通过竞态测试。缩短后继头序列或只修改结算后继头哈希时，已归档证据被拒绝。 [来源复查](chain-parser-go/internal/service/runtime_sources_native_test.go) · [历史重放](chain-parser-go/internal/service/native_history_proof_test.go)
- BSC 在 136,158,531 用公共 RPC 批量采集完整区块和 17 个账户的证明，独立核对 40 个存储槽和两项原池执行使用的 93 次预状态读取。独立来源的 finalized 高度 136,158,554 随后确认原块哈希和状态根，Go 核对同块 99 笔回执。Pancake V2 和 V3 离线重放分别收到 55,371,283,985,433,316 和 55,378,869,052,922,774 原子单位，网络请求为零。接下来继续原合约 getter 与常规 Parser/Router 差分。
- BSC Pancake V2 在 136,159,243 补齐代币、工厂及储备 getter 使用的槽，四次原合约只读执行通过 MPT 核验。常规 Go Parser 与 Rust Router 返回 HTTP 200，报价与原池收款均为 55,361,279,474,212,605 原子单位，回放无未记录 RPC 请求。链费模型和生产执行资格仍待验收。
- BSC Pancake V3 在 136,166,651 验证第四代 LM、三代前序 LM 与 MasterChef 的固定状态，覆盖 2,100 条预言机观测、253 个初始化 tick 和 4,168 次 getter。两源随后核对同高度哈希及状态根。常规 Go Parser 与 Rust Router 返回 HTTP 200，报价与原池收款均为 55,536,458,068,512,518 原子单位，回放无未记录 RPC 请求。Go 验证同块 85 笔回执。七项源码重建中，六项完整 runtime 一致，第一代 LM 的执行体一致且 CBOR 来源摘要有差异。[复验证据](integration/test/source/deployment-catalog/README.md#bsc-pancake-v3-lm-固定块报价)。
- Avalanche LFJ v2.1 在 101,588,444 的原池 clone 固定指向 `0xee5a90098b270596ec35d637b30d908c862c86df`，同高度工厂 getter 返回 `0x3e30fdae04c08fc20fa2fe0cf55c95d99a9c2d8f`。分别核对两套 runtime 及池中的 token X、token Y 和 bin step。负对照拒绝错误 clone 地址、变更的工厂实现摘要和缺失的代币 getter。
- Avalanche LFJ v2.1 在 finalized 高度 101,598,757 验证全部 1,209 个非空 bin，固定状态重放 4,848 次 getter。常规 Go Parser 与 Rust Router 返回 HTTP 200，报价与原池收款均为 134 原子单位，无未记录 RPC 请求。移除一组 bin 证明后，重放因缺节点而拒绝。Go 复核 101,598,759 后继头提交的目标回执根。[复验证据](integration/test/source/deployment-catalog/README.md#avalanche-lfj-v21-固定块报价)。该真实样本的预言机索引为零，精确部署源码、链费模型和生产执行资格仍待验收。
- 依次补齐 11 个部署的原始 runtime、升级语义、固定高度证明、原版执行和常规 Parser/Router 收款差分。区块回执及 Avalanche 目标高度的后继结算头继续独立核验。 [六链固定块复验命令](integration/test/source/deployment-catalog/README.md#fresh-evm-anchors)

#### 当前迭代目标（R03 基础设施闭环，2026-11-27）

- 让 R03 验收在缺少本机 PostgreSQL 13 或 Kafka 安装时，自动使用隔离 Docker 实例，实例只暴露 loopback 端口并在退出时清理。
- 修正 Kafka 容器的内部与外部监听地址，保证容器内 topic 管理命令和宿主机 Rust producer 分别获得稳定的 broker 地址。
- 用固定 protobuf 工具链、macOS SDK 头文件和独立编译日志构建 `normal_yellowstone`，执行真实 PostgreSQL/Kafka 下的发布、撤销、重启、Fence 和 HEAD_FIRST 恢复场景。
- 本轮目标已通过 `python3 integration/tools/acceptance/r03_authority_process.py`，报告状态为 `LOCAL_AUTHORITY_PROCESS_SCOPE_VERIFIED`。该结果覆盖受控 tonic 来源与真实本机中间件，真实同源 Yellowstone、历史 bank 完整性和生产故障域仍按 R01–R04 继续验收。

#### 当前迭代目标（R03 自建同源验收，2026-11-28）

- Agave 4.3.0 本地产块的 Geyser 元数据现使用 PoH 计数。正式 Parser 已在同一 Yellowstone 会话发布三个连续 finalized bank，逐个核对 WAL 游标、原生证明哈希、同源 RPC 历史块及当前六个账户。 [运行命令](chain-parser-rust/crates/bank-source/README.md#same-process-validator-and-parser-acceptance)
- 真实转账写入已通过同会话原生证明、最终区块签名及账户余额对照。14 项原生观察器回归与 17 项 Parser 回归通过。
- 保持节点出块并短时暂停接收，插件回放下界为 slot 44。Parser 从已提交父块后的 slot 49 接续，逐槽提交断线窗口内已最终化的区块。重新签发的原生证明、已提交父块及 WAL 回执匹配后才发布。
- 同一账本的 Agave 和 Yellowstone 进程退出并重启，正式 Parser 撤销旧会话范围，以新 WAL 代际恢复。完整联测在 slot 116 恢复，保留转账后的余额并登记 slot 67–115 的历史缺口。继续验证生产独立故障域。

#### 当前迭代目标（R02 公开 EVM 双源，2026-11-28）

- PublicNode 与 dRPC 的 HTTP 端点均核对 Ethereum 主网 chain ID 和 genesis。断开两条 WebSocket 订阅，等待主网推进后重新订阅。断线前高度 26,510,135、重连后高度 26,510,138 的通知均与两源固定高度 HTTP 响应匹配。
- 生产共用的 HeadTracker 和 staging Pipeline 从 HTTP 补齐高度 26,510,136–26,510,138。每个高度在两源分别重建区块头哈希、交易根及回执根，逐高度核对父块并读取已归档的主源原始数据。收紧补齐窗口到一个块后，请求返回历史缺口，提交观察器未增加记录。
- 在 `chain-parser-go` 运行 `XONE_PUBLIC_EVM_HEADS=1 go test -timeout=5m -count=1 -v ./internal/ingest -run '^TestPublicEthereumHeadRecovery$'`。该次验收使用临时归档和本地提交观察器。生产 owner 日志、持续历史及限流采样、基础设施故障域仍待验收。 [复验测试](chain-parser-go/internal/ingest/heads_public_test.go)
- 剩余请求时间按待探测端点分配。首端点身份或历史读取挂起时，有效备用端点仍可提交自己的能力报告。HTTP 401 或 403 分类为 `access_denied`，相关能力保留为未知，读取可切换到下一端点。关键归档失败仍中止操作。
- 四个固定高度各采样两轮。dRPC 返回四点的区块头、账户余额和哈希固定读取，均通过未知哈希负对照。PublicNode 返回创世和 22,000,000 的区块头，余额请求返回 403。高度 15,000,000 的区块头返回不可用，高度 26,510,136 的区块头、余额和哈希固定读取通过。两源均回应 safe 和 finalized 探测。样本反映 2026-11-28 对指定端点的观测。 [历史能力复验](chain-parser-go/internal/rpc/capabilities_public_test.go)
- 正式服务在会话时限、消息数或重连次数耗尽后按轮询间隔与最大重连退避中的较大值冷却，再开启有界订阅。协议错误、单条消息过大和队列溢出继续由 HTTP 轮询接管。四项续订场景与 HTTP 限频回归通过竞态检查。 [续订回归](chain-parser-go/internal/service/ws_budget_test.go)
- 正式 Go owner 使用 Ethereum 主网 V2 池、两条公开 HTTP 链路及 PublicNode WebSocket。26 秒会话到期后保持 HTTP 采集，Pebble 日志从高度 26,510,212、游标 1 推进到高度 26,510,215、游标 4。两源逐块核对公布的高度、哈希和父哈希。重算 120 条本机原始归档的 payload 摘要，三个 `BLOCK_COMMIT` 均匹配已归档的 staging 记录。此次正常进程使用 `latest` 与本机 HTTP journal。
- `chains[].rpc_rate_limit` 配置每个 HTTP 端点的 `requests_per_second` 和 `burst`。同一端点的在线、见证和历史请求共用额度，历史保留独立归档和身份隔离。额度不足时重新核验身份不会改变已有隔离状态。来源状态接口披露本实例实际请求次数、限额拒绝次数及最近 128 次请求的 P50、P95、P99 延迟。正式入口限额、来源状态及整个 RPC 包的竞态测试通过。 [验收测试](chain-parser-go/internal/service/rpc_retry_test.go)
- 历史能力报告仅采信本轮已通过 chain ID 和 genesis 握手的端点。同轮在线探测复用这次身份验证，双端点集成验收确认各握手一次。原始响应归档失败会中止历史、在线或锚点探测，撤下现有能力并退避。调用取消与存储失败各自保留原始错误，取消后的响应不会被解码。双端点四轮连续归档及 pin 回收、锚点继承基线归档责任和备用端点超时隔离均通过竞态回归。真实磁盘测试配置使用 8 秒探测和 10 秒归档观察期限以覆盖同步写入及完成票据。
- 独立探测与锚点复核均在观察票据落盘后发布能力。上游历史子预算超时仍记入报告，在线验证及同次观察的归档回收可以继续。注入探测和锚点票据写入失败均撤下当前能力，竞态回归确认完成票据写入前不发布新能力，正常双端点四轮归档及基线责任继承通过复验。
- 固定高度锚点复核为仍可用的端点分配剩余请求时间。首端点挂起时，同一总预算内仍可从备端点核对块号和哈希。回归确认超时端点退出锚点资格，备端点继续可用，归档失败仍中止本次复核。
- 本轮公开双源回归在 54.9 秒内完成，将重连前高度 26,511,591 补齐至 26,511,594。两源重建三个目标块的头、交易根和回执根。限制补齐窗口的负对照报告同一历史缺口。

#### 当前迭代目标（R04 恢复预算边界，2026-11-28）

- 持久在线 scope 的保留作业聚合在预算已经超限或状态损坏时必须立即返回 `ErrBudget`，不能依赖无符号减法得到可用余量。
- history replay 重启加载按累计 payload 字节检查上限，超限记录返回 `ErrScopeStoreCorrupt`，不进入后续回放或调度。
- 冷启动从上次在线目录最多恢复 32 个仍存在于当前目录的池身份，额外池只能经带代次条件的 `ensure-pool-scope` 请求加入。
- Rust history queue 的持久状态读取同样检查作业数量、序列化队列大小、已记录条数和历史字节计数，超限统一返回 `HISTORY_RECONCILE_REQUIRED`。
- 通过 Go bootstrap 全包和新增边界回归后，提交本轮代码并继续推进 R04.2 的真实长停机、peer 选择和 retention 证据。

#### 当前基线选择已验证范围（I7 R04.2，2026-11-28）

- Solana 正常 HEAD_FIRST 路径按配置的新鲜度约束选择 RPC 基线。双端点回归验证后续新鲜基线恢复、全源陈旧时保持不可用，以及获选基线的离线重放。
- EVM 普通 RPC 路径已通过备用源身份、时间和固定高度锚点的定向测试。两端点留存联测覆盖陈旧主源、备用源恢复、历史缺口披露和后续区块同步。
- 正常报价进程的 Kafka 恢复已在双链场景验证。Solana 来源挂起时，EVM 独立完成启动、报价和后续块同步。
- Go 备用 owner 已在实际 PostgreSQL/Kafka 上取得新 writer epoch，提供冷启动检查点。运行中的报价副本通过 Kafka 收敛到新代次。

#### 下一迭代目标（I7 R04.2，2026-11-28）

- 冷报价副本并发尝试 owner 和可信 peer 的近期检查点，首条完成身份、Kafka 游标及增量对齐的路径发布视图。使用隔离 Kafka 复现并修复 peer 挂起阻塞健康 owner 的问题，四项恢复集成通过。普通 Go owner、PostgreSQL、Kafka 和两个报价进程复验中，owner 意外退出后冷副本在 331 毫秒恢复可报价范围，333 毫秒返回首个 INDICATIVE。正常撤销和全部来源不可用场景均返回 503。
- 冷恢复在每条链上同时尝试最多四个配置的 owner 地址。隔离 Kafka 回归覆盖首个 owner 与 peer 同时挂起、后续 owner 仍在请求超时前恢复正确检查点。保留 Kafka 尾窗对齐和撤销检查，四项恢复集成通过。
- Cancun 固定 H/H+1 已通过 ExactIn 和 ExactOut 的 VERIFIED 报价、缓存和构建验收。冷启动报价进程已通过 Go owner 与 Kafka 恢复同一快照，并取得首个 VERIFIED 报价。
- 原生节点仿真超时与重试、按请求读取 Cancun 状态均已通过正常进程验收。Geth 交易序号套件复验 24 组，覆盖 ExactIn 和 ExactOut 的 12 组 ERC-20 VERIFIED 报价与存储证明篡改拒绝。Kafka 复验通过首个 peer 挂起、过期、错链和撤销场景。下一轮在隔离故障域复验接力、全部来源离线及恢复后的报价行为。
- 1 小时实际停机演练于 2026-11-28 通过。1 天和 30 天演练已分别于 2026-11-28 02:57:32 和 03:15:22（UTC+8）开始计时。重启后核对同时停机时长、Kafka 低水位与恢复游标、首个合格范围、目标范围和历史缺口。独立 VERIFIED 报价耗时仍待验收。
- Kafka 读取器中断定向回归已通过，覆盖无新区块、一个待追赶区块、重组屏障续读和旧视图失效。生产故障域联验继续覆盖来源全部不可用的就绪状态。
- 普通 Go owner 与两个报价进程的认证副本接力已在本机完成。下一次复验使用独立故障域，保留正常关闭撤销、意外退出与来源全部不可用三个场景。
- 用独立故障域的可信 peer 和当前源复验主来源离线时的基线选择，覆盖过期快照、错误链、撤销和可信源全部不可用。验证 history 队列启动、失败与长期容量，以及故障或历史补齐期间在线水位与报价可用性。

#### 网络分区与历史容量迭代目标（I7 R04.2–R04.3，2026-11-28）

- 单宿主机 Docker 检查点分区验收已通过。构建 Linux release 镜像时补齐了 Kafka 客户端的 CMake 依赖。验收报告保留镜像 ID、恢复来源对应的首个合格范围和报价时间，以及两条链路均断开时的 503。
- 按实际停机时长继续运行 1 天和 30 天演练。1 天演练最早于 2026-11-29 02:57:32（UTC+8）恢复，30 天演练最早于 2026-12-28 03:15:22（UTC+8）恢复。分别核对 Kafka 低水位、恢复游标、历史缺口和报价输出。
- Go 历史队列已通过 2,592,000 块缺口的本地容量测试。8 轮持久写入共覆盖 256 块，达到字节上限后拒绝第 257 块并保留重试原因。再次打开 Pebble 后历史进度、容量和重试原因一致，在线基线继续推进。历史队列与分支复核定向 race 测试通过。
- Rust 历史队列改用受控 bank 构造当前快照，完整 11 项集成测试可直接运行。已覆盖单作业长缺口、字节配额、任务重新领取、RocksDB 重启、历史覆盖保留和来源身份撤销。真实同源 Yellowstone 流和历史 bank 状态完整性继续验收。

### I8 路由与费用

#### 迭代目标（2026-11-28）

1. 在候选计划的固定快照上核对实际 gas、L1 数据费和可用的原生费估值，再按可核对的净额比较路线。用签名交易执行结果验证 OP 费用，并检验证据缺失时的费用状态。
   - 已在正常报价发布时复核并披露经授权估值的 EVM 排序值，OP 已签名执行的链费与原版客户端一致。下一步将候选链费覆盖纳入费用完整性验收。
2. 扩展共享 DAG 和 beam 多标签搜索。用小图穷举对比路线、上界和最优性差距，覆盖拆分、合并及共享池热状态。
   - 两种报价已完成小图穷举、共享池报价回归及标准 V2 路径金额界验收，并已发布共享后缀的资金流与物理池依赖。
3. 接通多池 ExactOut 和拆单执行。在正常报价及构建入口校验精确到账、最大输入、限制回滚、完整费用和快照绑定。

#### 共享 DAG 后续验收（2026-11-28）

- ExactIn 和 ExactOut 均已通过标准 V2 共享后缀的 Router02 对照、绑定检查、独立回放和正常 HTTP 仿真。ExactOut 另校验了最大输入回滚、精确到账、实际扣款和执行器中间代币余额恢复。
- 正常构建已接入两种报价的共享后缀交易模板，覆盖构建成功、过期和候选替换。下一轮将资金预算和链费证据接入同快照候选竞争，并验证实时构建准入。
- 两种报价均披露本地执行费比较结果。20 项 HTTP 场景通过，覆盖独立净额核算、过期估值、模拟预算和共享执行器缺失。继续纳入 L1 数据费和资金约束，验收候选入选后的构建，并用小图穷举验证其他池型的路径金额界、gap 及预算裁剪。

### I9 上线前基础设施

- 完成 Kafka TLS/SASL/ACL、ISR 和事务故障验证。
- 完成 PG 故障切换、灾备和 RPO/RTO。
- 完成 CI、SBOM、许可证、secret 和 schema 兼容检查。
- 完成多租户额度、SLO、压测、OTel 和完整审计。

### I10 低优先级分析与外部来源

- 完成持久 DLQ/raw 回放。
- 完成 ClickHouse 事实、版本表、shadow 发布和 FINAL 查询。
- 完成 RFQ、多 solver、外部来源竞争和收益分布。

当前活动目标为 I7 R04.2，先完成真实长停机、可信 baseline/peer 选择和 retention 隔离，再继续 R01、R02、R03 的真实来源资格与生产故障域联验。I1 至 I3 的上述报价范围已通过原始回放、跨语言 wire、正常 HTTP 报价和真实部署固定块解析。I1 的 e281ff40 原回调参考 runtime 为 27,545 字节，标准 EVM 部署实验确认触发 EIP-170 限制。20230411 发布版 AUM、scheduled、BPT 和 breaker 已接入普通代币对 ExactOut，BPT 和 breaker 还接入 BPT 双向单池 ExactOut。两笔普通交易经过 Go 固定块解析、Protobuf、Rust 副本和 HTTP 报价。三币活动权重、活动费率和非零待结 AUM 另外通过四笔普通交易和一次断路器回滚。BPT 双向通过六笔成功交易和一次断路器回滚，HTTP 使用原始状态投影核对报价。已验证的 Managed 主网历史池关闭 swap，交易差分使用本地原 Vault。独立执行、实时来源和上线门禁继续按 I6、I7、I9 验收。

<a id="work-packages"></a>

## 工作包与设计方案对照

原设计完整要求直接查看对应§28条款，不在这里重复复制设计原文。功能细项在下方逐项映射。

|工作包|功能|方案|状态|已完成范围|待完成范围|
|---|---|---|---|---|---|
|WP01|仓库、构建与质量基线|§28.3|PARTIAL_LOCAL|三项目独立构建、固定Go/Rust工具链、锁文件、Make验证入口、公共契约副本检查；独立性补查：服务测试不读取兄弟项目源码/配置，Go测试本地完整路由设置，集成层逐字节校验共享副本并禁止跨项目测试路径|阶段240修复Go测试跨项目读取，当前contracts检查通过；镜像/SBOM/许可证及secret扫描、真正的CI平台执行尚未验收|
|WP02|链身份、金额和快照契约|§28.4|PARTIAL_LOCAL|Kafka实际record和owner bootstrap已接入Protobuf；uint256恰好32字节大端、块hash原始32字节；RaydiumCLMM dynamic_fee九字段及独立protofield63、signedindex/宽度/PDA/clone、旧profile隔离与独立protoc已验证；本批限定范围整体验收通过；本批新增独立CurveCryptoState field12与原LP账本/定价/来源的跨语言严格契约；本批新增Orca adaptive独立profile及field51完整Oracle/保留字段契约与严格跨语言校验；本批新增独立InfinityBin field30完整tree/share/manager账本与field29树外active锁定dust、跨语言wire/config/clone和来源约束；新增独立Orca adaptive-rewards复用完整Oracle/reward/tick wire，4007byte独立protoc与旧profile隔离通过；新增adaptive-dynamic独立版本、完整mixed layout/bitmap/reward/Oracle与4013byte独立wire，旧fixed-only隔离通过|网络指纹/所有链地址规范化、schema演进破坏检测和更多事实族仍需扩展|
|WP03|历史fixture与独立回放框架|§28.5|PARTIAL_LOCAL|固定合成基线、V2反例、大整数、重复重放、Python独立顺序计算对照；416固定原Pool.swap/CLPool.swap EVM样本、8组原库连续序列与1115主源码hash；动态费原DynamicFeeInfo/SwapState/swap_internal独立native：348案例12连续序列24步、8时间不变/6预算准入探针、2原始bank完整Pool；旧480/968/364/464语料保护，本批限定范围整体验收通过；正常Solana RPC原始响应证据WAL、独立耐久head/损坏封闭与重启新会话验证通过；标准receipt type0–4/legacy post-state的12组EthereumJS独立根向量（含128/130/257索引边界）验证通过；新增固定原Vyper NG完整交换/EMA、独立protobuf及正常服务回归；Managed非零BPT费用原回调参考357案例/30序列120步及常规EVM大小限制失败证据，明确无服务准入；MetaUSD实际clone/initialize/add_liquidity及32直接交换、8原探针、完整Token/raw/admin与正常服务回归；本批新增Crypto原池及原V4 LP的60案例、Orca费率197案例与14连续交易、Infinity原BinPool跨Bin326案例；本批新增固定原program pack+swap/update后的20bank、16成交/4拒绝，26类普通/优化模式故障拒绝；ProgramData为明确合成身份；本批原manager/Vault正常部署getter与5bank/4连续跨Bin交换、原burn16场景48交易（9回退）及原manager降低初始化上限证据通过；新增固定原Orca初始化奖励adaptive参考20bank/16成交4拒绝、12乘法溢出/4global回绕/35outside更新、34普通优化篡改拒绝；新增原dynamic枚举/实际加载器20bank及44篡改反例、100成功2失败×7动态布局内核结果对照|已有公开历史receipt/交易及原源码/原VM样本；仍缺目标版本全矩阵的真实多块分叉/事件顺序篡改、来源许可及独立收款差分的统一验收，不能将局部样本外推全部协议|
|WP04|RPC Provider与链能力|§28.6|PARTIAL_LOCAL|EVM chainId+genesis握手、错链隔离、总超时、逐provider有界尝试、响应体上限；Solana genesis校验；RaydiumCLMM动态profile复用同槽15..19账户finalized整批、ProgramData/Token/Clock及读后block验证，实际2bank样本通过；本批限定范围整体验收通过；同一提供方finalized context有界分片、逐片Clock/首末锚点、总响应预算和整批切源已局部验证；本批新增Crypto原模板/immutable及原LP minter、完整定价/raw/LP状态的同blockHash读取；本批新增Orca adaptive独立fee-tier PDA、254-byte Oracle及Pool/ticks同finalized bank读取和重锚；本批固定hash读取Bin原模板/immutableVault/key/pause/token/app依赖与完整root/L1/L2/所有bin/share，预算和getNext双向校验通过；新增adaptive-rewards同finalized bank/Clock完整账户RPC与重锚失败拒绝，旧版本隔离通过；新增完整dynamic/mixed账户同finalized bank读取和重锚、148/10004精确compact边界与畸形拒绝；normal服务已接方法级provider探测、负对照、共享限流预算、WS唤醒/有界重连与HTTP补扫，参见R01–R04逐项复核|公开provider历史正负覆盖、持续限流/重连、可信头与真实独立故障域矩阵仍待完整验收；不能把URL数量当独立来源|
|WP05|区块读取、解码与完整性组块|§28.7|PARTIAL_LOCAL|EVM固定blockHash读取、token身份、receipt/log完整性、同块读后锚点复核；单链串行提交；RaydiumCLMM DynamicFeeInfo1096..1130九字段实际解析、1130..1544未知padding拒绝、正常RPC动态分派与原完整Pool样本通过；本批限定范围整体验收通过；本轮receipt结构与日志连续性校验完成，Go全check/race及新旧27范围回归通过；多RPC完整receipt/尾部遗漏差异核验与恢复闭环通过；正常Solana RPC原始响应证据WAL、独立耐久head/损坏封闭与重启新会话验证通过；主源/全部witness的consensus receipt根重算、严格可选字段与bloom校验、前后root锚点及共同删尾拒绝完成局部验证；超过100依赖可用最多32片构造同context当前基线，归档/混合协议同样执行屏障；13池CLMM seed状态完整保留，16池134账户进程验证通过；本批新增Crypto完整状态解析及专属版本拒绝边界，旧settled仍拒绝active ramp，独立ramp版本已完成限定在线验收；本批新增initialized Oracle专属profile、Clock/源bank/timestamp绑定和完整账户解析，旧static/rewards拒绝边界不变；本批新增Bin完整树与树外active1000share/dust解析、原getters/storage核对及source重锚，10种进程来源故障撤销/新generation恢复通过；新增classic SPL三fixed数组/初始化Oracle与奖励组合正常解析、13来源故障撤销恢复及完整后状态限定验证通过；新增adaptive-dynamic真实compact账户解析与16来源故障撤销恢复限定验证；normal服务并行staging/有序提交、工厂动态目录、先归档后decode、持久DLQ修复重放及owner责任已接线并有本地进程验证|持续生产归档保留/吞吐、真实工厂目录及更多版本完整事实、可信头/最终性与实网重组发布资格仍需补验，详见R01–R04复核|
|WP06|Canonical Journal、事务与幂等|§28.8|PARTIAL_LOCAL|Go/Rust公共BEGIN/CHUNK/COMMIT契约、完整屏障/校验/Protobuf二进制/32字节大端金额；真实Kafka事务发布、read_committed消费、稳定transactional.id、分区0/单分区检查、prepared与state/cursor原子应用、未知结果从journal确认；prepared对照确定性wire hash；只读analytics Reader不创建producer或fencing|生产TLS/SASL/ACL、DLQ原始证据、topic重建/迁移全链校验、真实ISR/保留损坏/消费者重平衡专项仍未验收|
|WP07|Reducer、分叉与旧主隔离|§28.9|PARTIAL_LOCAL|真实PG互斥租约/server clock/过期不可续租/owner投影/单调版本下界与关键激活审计；两解析器接入Kafka fencing和prepared恢复；限定V2旧分支精确BlockRef倒序撤销与新分支事实在REORG_COMMIT整体提交并供隔离analytics更正；Rust来源错误广播ONLINE_SCOPE_REVOKED，精确同scope/块/epoch/代seq+1，prepare/Kafka/PG/apply持久化与合格高代基线恢复；receipt根/共识字段异常的旧scope撤销、HEAD_FIRST新代恢复、无伪造BLOCK_COMMIT与后续正常提交经13项进程验证|真实链交叉来源/多块历史fixture、通用事实/BLOCK_REVERT独立记录、Solana完整fork、PG故障切换/区域及独立故障域、对象存储fencing和完整控制审计仍未完成/验收|
|WP08|当前基线、归档与HEAD_FIRST空盘恢复|§28.10|PARTIAL_LOCAL|Go/Rust owner Protobuf bootstrap和无KV/有限Kafka恢复；新增公开快照+committed cursor不可变manifest、Go条件HTTP上传/精确回读/完成标记与当前owner目录；正常router有界远端下载/二次资格核对及真实Kafka冷恢复/HTTP报价通过，半上传/损坏拒绝；Go新增周期发布/取消关闭、手动共享通道及双链公平轮转/失败隔离；当前owner资格化旧对象及head/cursor，router私有有限尾窗追平后激活；Rust owner新增同格式本地公开checkpoint、条件远端上传/精确回读/证书/当前及旧对象恢复目录、周期公平轮转/正常shutdown取消与共享配置摘要；独立正常owner/router实际Kafka完整baseline升代追平/HTTP报价通过，两owner新增显式public-v2字节/文件/扫描容量准入和Go周期旧JSON去重；两owner新增认证有界只读保留预览/当前与全部候选保护、最近备份/证书/age/目录证据绑定和Go原目录全计划/planId跨语言一致；Go显式回收/耐久隔离/实际进程崩溃恢复/正常HTTP与pending容量门禁通过，正常HTTP/双向互通，Rust旧KV不自动ready|Rust实际回收/自动维护、canonical checkpoint声明与离线peer发现、跨writer/reorg/scope恢复矩阵、全盘/远端容量与多对象分片、真实独立存储故障域/保留过期和长停机演练待完成；Go自动配额维护由阶段116补齐|
|WP09|Rust版本化状态副本|§28.11|PARTIAL_LOCAL|不可变QuoteView/池图原子发布、数值更新复用拓扑；两独立路由副本读取完整分区、禁自动提交offset、owner当前完整checkpoint+cursor恢复、有限Kafka尾窗；HTTP开发模式所有源失败暂停及同generation恢复；字段24独立state进入双完整路由，错误Solana依赖撤销该链旧视图，EVM链保持可用；新增adaptive-rewards双副本当前bank分发、故障撤销/新generation恢复和router重启通过；新增adaptive-dynamic双副本完整状态、撤销恢复与router重启验证；阶段86补基线/owner checkpoint/scope扩容图复用与闲时缓存回收、锁外实际释放及HTTP后台任务生命周期；阶段104新增当前owner资格化远端对象下载/目录二次核对和实际Kafka冷恢复/正常HTTP报价，统一本地验收通过|完整Pool状态/顶层索引COW、活跃慢请求内存预算与压力、真实保留过期/大型图恢复及生产多实例容量仍需实测|
|WP10|精确V2与基础Token行为|§28.12|PARTIAL_LOCAL|单一V2精确整数内核、先乘后除、方向/费率/大整数、consumed/refund；未知token行为失败；Raydium ceil输入组合费/creator拆分及输出creator费、protocol/fund floor、LP保留费与raw/净储备后状态，u64/u128checked；1604原代码差分；新增独立完整V2版本的raw/储备/累计价/锁及原VM差分完成限定验收|真实链参考合约/独立执行余额差分、税费/rebase等特殊Token未实现或验收|
|WP11|V3、Stable及V4能力|§28.13|PARTIAL_LOCAL|标准Uniswap V3有界证明范围内的独立整数exact-in、逐word手续费舍入、跨tick净流动性、零流动性空段、int256/uint160/uint128边界及乘法溢出fallback；264项固定官方SDK差分（含最低初始化价）与双副本解析报价场景通过；RaydiumCLMM动态profile原spacing分段/时间filterdecay/ceilfee/cap/zeroL/index/逐步费用与完整后Pool，348原native案例及连续状态通过；本批限定范围整体验收通过；Balancer V2双币不可变50/50独立Weighted模型，固定区块及765原onSwap案例/45组180步完整余额与17项双副本进程检查通过；新增不可变双币任意合法归一化权重static版本，原整数LogExpMath/powUp、5355案例/315组1260步/240pow与18项双副本检查通过；五族联合增量已补多币物理状态、限定Managed/Composable、Plain3/4正常读取路由及Meta/NG/Crypto原数学子集、Solana静态奖励和V4/Infinity高级CL/单Bin内核，按各自边界完成本地测试；标准type0 NG2–8币已补齐固定源码模板/immutable读取、完整raw/admin/oracle后状态与原exchange差分及双副本连续报价；非零Managed协议/管理费BPT铸币、完整供应/收款余额与powDown数学已原回调验证，旧源码超限仅参考使用；MetaUSD标准meta/3CRV直接兑换已补齐独立clone版本、base VP只读依赖、完整状态/wire、正常RPC解析/双副本报价及共享池重算；本批新增Crypto完整调价/admin mint/gulp与完整后状态、Orca自适应费率管理器和Infinity跨Bin内核；Infinity静态零hook完整树在线接入已由后续Bin批次完成限定验收；本批Orca完整adaptive内核已接普通报价，完整Oracle/fee/ticks/raw后状态和全额/正输出约束、双副本及来源故障恢复已通过本地验收；本批静态零hook标准ERC20 Bin普通分派、真实tree遍历与树外active、完整储备/fee/app账本及全额正输出通过，完整Vault结算待补；新增Orca adaptive初始化奖励checked乘法溢出归零、wrapping global/outside及普通报价完整后状态与原native差分通过；新增三连续完整dynamic/fixed混合数组的adaptive奖励内核/普通分派及完整原后状态差分|其他Curve变体/Meta underlying/Lending/外部rate与特殊Token，Managed/Composable剩余BPT与rate-cache/V3组合、未知Slipstream模块/LM升级，V4/Infinity回调/管理器完整结算及Bin高级组合仍缺；新增v2-complete已局部补全Swap/raw后状态，旧v2 profile不自动升级；全版本真实部署/执行验收仍缺|
|WP12|金额感知候选图|§28.14|PARTIAL_LOCAL|方向性多重图、物理池ID、金额计算排序、跳数/展开/候选/期限预算、范围内启发式语义；V3与V2混合图、按配置拆单粒度保留仅部分分配可完整消费的路径；RaydiumCLMM动态profile进入已有金额感知图，报价使用动态完整状态；专项回归通过，本批限定范围整体验收通过；阶段85补物理池增量边索引和Arc邻接边共享，128连续变更/256池局部更新及副本/路由/分配/API回归通过|完整银行与顶层索引规模化/压力验证、beam多标签、严格上界、全量小图oracle与跨跳shift反例仍需补充|
|WP13|拆单、共享池与整数守恒|§28.15|PARTIAL_LOCAL|最多8路的有界有序整数网格、独立池子集DP及大范围seed搜索、共享池状态模拟、余数逐跳重算、输入守恒、启用成本提案；共享V3物理池顺序更新sqrt/tick/liquidity，最终path/route输入输出都重算；RaydiumCLMM动态共享池连续消费完整volatility/reference/fee后状态，原序列及分配输入守恒通过；本批限定范围整体验收通过；本批新增Crypto共享池拆单连续重算价格/raw/LP账本验证；本批新增四组Orca adaptive两步共享allocation与原序列后状态匹配，后一步复用Oracle/fee/ticks/raw；本批共享manager/currency依赖冲突拒绝及每步app/protocol fees传播，4步跨物理池原序列和实际拆分计划21步原EVM重放通过；新增四组adaptive-rewards共享allocation复用完整奖励/Oracle/tick，实际选中计划36步原native连续重放通过；新增dynamic四组共享allocation和39个实际选中计划原生步骤验证|通用DAG合并/联合求解、税费、实际Gas/L1费换算与系统oracle gap测量尚未完成|
|WP14|执行计划与calldata|§28.16|PARTIAL_LOCAL|caller/recipient/滑点/截止/快照/策略进入plan hash、单路径V2 ABI构造、目标白名单拒绝；Edge绑定protocol，V3/其他协议路径不能误编码为V2 router calldata；新增singleton profile只生成指示性计划，明确拒绝套V2 calldata，专属执行器/settlement未实现；T32新增标准V2单hop完整BOUND信封/执行器身份/精确calldata与本地独立执行绑定，限定本地验收通过；截至阶段84已完成21/61限定执行准备，Meteora/Pump/Raydium/Orca经典SPL单池的显式ATA/WSOL、unsigned wire、独立SBF和钱包回执按各批范围验证|限定Solana单池准备/签名/广播/回执和v2-complete EVM准备已实现；其余协议执行适配、通用多hop/拆单DAG、资金授权/完整费用/ExactOut退款组合、受审计执行器及EVM钱包交易闭环仍需完成；生产VERIFIED保持关闭|
|WP15|独立交易仿真|§28.17|PARTIAL_LOCAL|验证接口明确拒绝无后端的VERIFIED；固定blockHash eth_call仅标RPC_CALL_CHECKED；新增Shanghai单hop V2的严格REVM本地状态DB、独立真实余额差、执行预算与可终止子进程接口，原合约全状态差分、双副本故障恢复、全量及双审通过限定本地验收；阶段58–84限定Solana原SBF/Token-CPI、实际wire/收款差和钱包回执按独立批次验证|可信历史bank、生产独立验证、多链hardfork/L2费用、未覆盖协议/多池/拆单仿真与规模化容量仍缺，VERIFIED保持关闭|
|WP16|报价API、认证、配额及质量语义|§28.18|PARTIAL_LOCAL|FAST指示性ExactIn、字符串金额、鉴权、单凭据配额、并发/期限、single-flight、证据、缓存版本、旧quote build拒绝；新增认证simulate API、缓存前后新鲜度复核、实际余额/费用/worker证据；限定本地验收通过|多租户独立额度、statePolicy/allowSources 请求契约、任意仿真隔离、成本完整性、VERIFIED可执行闭环与完整业务成功率SLO待完成|
|WP17|多副本、高可用、监控与发布|§28.19|PARTIAL_LOCAL|进程health/startup/按链状态、SIGTERM、日志/报价低基数指标、三个独立Dockerfile及本地compose|多区主备、PG/Kafka故障/磁盘满/节点或区丢失演练、OTel、灰度/发布控制与生产readiness待完成|
|WP18|第二与第三条EVM链|§28.20|PARTIAL_LOCAL|七条 EVM 主网以正式 Go 解析器发布指定池的 finalized 快照。六个新增部署的同高度合约状态与快照一致。已验证每链独立启动、工作队列和租约|逐协议版本与升级审计、各链 hardfork 和 Gas/原生币费用差异、同时负载下缓存与预算隔离及逐链生产 SLO 仍待验收|
|WP19|分析事件与worker基础|§28.21|PARTIAL_LOCAL|现有Go解析二进制analytics角色已接只读Kafka有限Job及显式常驻journal-continuous模式；持久cursor、topic身份、事实去重、毒消息暂停及失败状态checkpoint已实现；不占用在线KV/PG|持续调度/取消/故障停止与实际重启已验；持久DLQ/raw回放、ClickHouse重试/发布、跨插件隔离配额与故障验收待完成|
|WP20|Solana数据与执行适配|§28.22|PARTIAL_LOCAL|Rust RocksDB、账户依赖/完成屏障、来源session/write_version隔离、skipped范围验证；RPC整批限定布局CPMM数据解码；固定6d7a0f82动态费独立profile，classicSPL/backedWSOL、fee0/1/2、seed0..65535及mandatoryextension，原解析/分段exact-in和41真实PG-Kafka双router检查通过；本批限定范围整体验收通过；正常Solana RPC原始响应证据WAL、独立耐久head/损坏封闭与重启新会话验证通过；当前RPC分片bootstrap、失败整批丢弃/切源重新采集、全scope/报价快照引用及WAL新会话恢复局部验证完成；经典SPL静态费Orca/Raydium奖励增长及完整后状态已由联合批次局部验证；本批新增固定Orca原FeeRateManager的逐group/时间/波动原native差分，限定adaptive在线解析由本批独立版本完成；其他组合未完成；本批新增经典SPL三fixed数组/未初始化奖励的Orca adaptive账户解析及正常分派；其余组合和完整SBF执行待补；新增独立adaptive-rewards版classic SPL/三fixed数组/完整Oracle/初始化奖励增长与跨tick状态，20原bank及双router/13故障恢复限定验收通过；新增classic SPL/完整Oracle/三完整dynamic或fixed混合数组与初始化奖励的解析/报价/双副本限定验收；normal tonic/帧WAL/manifest/owner发布桥与认证scope控制已接本地持久后端；截至阶段84的Meteora/Pump/Raydium/Orca执行范围以逐批表为准|真实同源Yellowstone连续性/fork/补洞资格、完整版本化bank与动态scope联验、订单/奖励领取/Token2022交叉及未覆盖协议版本、生产CU/账户锁/交易负载仍待完成|
|WP21|K线、重组更正与历史分析发布|§28.23|PARTIAL_LOCAL|精确有理数OHLC、事件时间排序、撤销最高/最低成交后独立重算、revision、离线原子发布；现有Go二进制analytics journal有限Job及显式常驻只读消费公共事实，无在线KV/PG；HEAD_PROVISIONAL事件时间水位/原子单位和历史缺口；有限预算失败状态持久化|ClickHouse事实/版本表、shadow/发布指针、FINAL及merge前查询、正式分析API/查询配额、迟到/decoder升级回补、真实链与生产故障条款待实现/验收|
|WP22|高级优化与外部来源竞争|§28.24|NOT_STARTED|保留HEURISTIC_WITHIN_SCOPE，不声称全局最优|RFQ/聚合器、多个求解器、有效期/调用者约束、可证明上界及收益分布尚未开发|
|WP23|迁移、灾备与生产放量|§28.25|PARTIAL_LOCAL|本地重启/无KV路由恢复演练入口与报告；明确发布阻断项|真实生产shadow迁移、区域灾备、RPO/RTO、adapter灰度回退及逐租户放量未实施|
|WP24|HEAD_FIRST启动与历史解耦（前置工作）|§28.26|PARTIAL_LOCAL|HEAD_FIRST不以旧KV/cursor/history gap置ready；owner分配代际、PG/Kafka下界恢复、当前完整checkpoint加有界尾窗；未确认分叉持久暂停、可信源确认后HEAD_FIRST恢复；临时源故障恢复不误增canonical epoch；正常Solana RPC原始响应证据WAL、独立耐久head/损坏封闭与重启新会话验证通过；16池134账户当前bootstrap与空内存路由副本/解析器重启恢复通过；不依赖历史补洞，不假定普通RPC提供严格版本化bank；认证EnsurePoolScope及Go32→33池扩容、Rust subset/actor bridge、WAL/manifest owner原子发布已接并有本地验证|1小时/1天/30天真实停机、流WAL溢出/范围扩容/历史隔离的完整公开来源资格、资源配额和首个VERIFIED报价时间仍未完成|
|WP25|配置文件体系与配置中心扩展口（P0前置）|§28.27|PARTIAL_LOCAL|三服务严格单文件配置校验、配置/策略hash与secret权限；新增Kafka/PG配置与有界时长校验；raydium-clmm-dynamic profile与mandatoryextension账户范围准入、原动态参数只从链状态读取，旧profile动态污染拒绝；本批限定范围整体验收通过；本批新增MultiAssetState field12 CurveCryptoState及独立protoc/克隆/来源bank验证；本批新增OrcaWhirlpoolState.adaptive field51，保留reserved32–49，独立protoc与16/16/128保留数据往返验证；本批新增InfinityBin完整状态/可选树外active，原PoolState保留字段不变，独立protoc及旧模型兼容回归通过；新增adaptive-rewards独立版本准入、完整reward/Oracle wire及旧profile拒绝边界和两阶段审查通过；新增adaptive-dynamic独立版本准入与旧fixed-only隔离，跨语言4013byte wire与两阶段审查通过|生产 Kafka TLS/SASL 当前未实现；secret修订id、完整公共配置schema/兼容矩阵/坏输入语料及配置中心bundle物化仍待完善|
|WP26|统一结构化日志与故障排查（P0骨架、持续完善）|§28.28|PARTIAL_LOCAL|双语言JSON/脱敏/有界队列与drop指标；PG租约和baseline激活事务持久审计；启动失败JSON化、Pebble日志适配、正常服务日志检查拒绝非JSON行|Rdkafka等全部SDK故障日志映射、轮转/spool/集中采集、全控制事件审计、采样/聚合/OTel与性能/故障演练仍待完善|

<a id="protocol-coverage"></a>

## 协议版本完成情况

解析、数学与执行分别核对。PARTIAL_LOCAL仅适用于表内限定版本和条件，不代表所有部署或完整执行均已支持。

|协议|适配版本|链族|设计条款|工作包|解析|数学|执行|范围与剩余功能|
|---|---|---|---|---|---|---|---|---|
|uniswap-v2|uniswap-v2/1|EVM|4,7.3,11.3,12.2,12.11,12.14|WP02,WP05,WP10,WP12,WP13,WP18|PARTIAL_LOCAL|PARTIAL_LOCAL|PARTIAL_LOCAL|T32明确Shanghai/原Router02/标准ERC20单hop本地完整交易独立执行已通过限定验收；真实链历史/执行收款差分与生产部署审计；税费/rebase/异常token与执行上下文；DEX版本专属calldata/独立执行；新版本或ABI/代码升级须独立接入；不自动兼容|
|uniswap-v2-complete|uniswap-v2/core-1.0.1-full-swap-1|EVM|7.3,11.3,12.2,12.9,12.14,14|WP02,WP03,WP04,WP05,WP10,WP11,WP12,WP13,WP14,WP15,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|PARTIAL_LOCAL|独立完整Swap/raw/累计价/锁版本，原发布代码固定；解析/数学/共享状态/实际VM及BOUND、普通双副本/完整后观察、全量与双审均已通过限定本地验收；不扩展旧版本，不含Mint/Burn/税rebase/真实部署/链规则/生产VERIFIED|
|uniswap-v3|uniswap-v3/core-1|EVM|4,7.3,11.3,12.2,12.11,12.14|WP02,WP05,WP11,WP12,WP13,WP18|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|真实链历史/执行收款差分与生产部署审计；税费/rebase/异常token与执行上下文；DEX版本专属calldata/独立执行；新版本或ABI/代码升级须独立接入；不自动兼容|
|pancake-v1|pancake-v1/core-1|EVM|4,7.3,11.3,12.2,12.11,12.14|WP02,WP05,WP10,WP12,WP13,WP18|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|真实链历史/执行收款差分与生产部署审计；税费/rebase/异常token与执行上下文；DEX版本专属calldata/独立执行；新版本或ABI/代码升级须独立接入；不自动兼容|
|pancake-v2|pancake-v2/core-1|EVM|4,7.3,11.3,12.2,12.11,12.14|WP02,WP05,WP10,WP12,WP13,WP18|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|真实链历史/执行收款差分与生产部署审计；税费/rebase/异常token与执行上下文；DEX版本专属calldata/独立执行；新版本或ABI/代码升级须独立接入；不自动兼容|
|pancake-v3|pancake-v3/core-1|EVM|4,7.3,11.3,12.2,12.11,12.14|WP02,WP05,WP11,WP12,WP13,WP18|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|真实链历史/执行收款差分与生产部署审计；税费/rebase/异常token与执行上下文；DEX版本专属calldata/独立执行；新版本或ABI/代码升级须独立接入；不自动兼容|
|sushiswap-v2|sushiswap-v2/core-1|EVM|4,7.3,11.3,12.2,12.11,12.14|WP02,WP05,WP10,WP12,WP13,WP18|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|真实链历史/执行收款差分与生产部署审计；税费/rebase/异常token与执行上下文；DEX版本专属calldata/独立执行；新版本或ABI/代码升级须独立接入；不自动兼容|
|uniswap-v4|uniswap-v4/core-1 (source bea7f6d1)|EVM|4,12.2,12.11|WP02,WP04,WP05,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|零hook/静态LPfee<1e6/标准非零ERC20；独立manager/rawPoolId、extsload、方向费与完整tick证明已实现；动态fees/hooks/native/自定义会计、manager unlock/settlement及真实执行未完成；真实部署目录/历史块/特殊Token/生产验收未完成；阶段76补原生正数输出目标、容量拆单/顺序重算、manager计划/API/前端有限验收；完整Manager结算/执行仍未实现|
|curve|每个stable/crypto具体合约版本待固定|EVM|4,12.2,12.11|WP11|NOT_IMPLEMENTED|PARTIAL_LOCAL|NOT_IMPLEMENTED|Plain2/3/4各独立profile已局部实现；标准Meta直接交换及NG已由独立profile接入服务，Crypto完整交换数学及标准settled独立版本的状态/解析/报价已补，经典SPL/fixed-array限定服务验收通过；其他Plain/Meta underlying/Lending、特殊Token、Crypto其他版本与active-ramp在线状态、完整执行仍未完成|
|balancer|V2/V3分别适配，具体pool版本待固定|EVM|4,12.2,12.11|WP11|NOT_IMPLEMENTED|PARTIAL_LOCAL|NOT_IMPLEMENTED|Stable2、Weighted双币/多币及限定Managed/Composable regular另列已实现子集；原BPT数学内核不等于完整callback；非零BPT费用、供应/缓存/rate provider/V3及执行仍未完成|
|aerodrome-v2-volatile|aerodrome-v2-volatile/pool-1|EVM|4,7.3,11.3,12.2,12.11|WP02,WP04,WP05,WP11,WP12,WP13|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|特殊Token/完整执行与部署目录审计；支持fee<=300/10000的标准Pool，factory动态值按同块读取；合成RPC与独立参考已验证，真实链历史块及收款差分仍未完成|
|aerodrome-v2-stable|aerodrome-v2-stable/pool-1|EVM|4,7.3,11.3,12.2,12.11|WP02,WP04,WP05,WP11,WP12,WP13|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|特殊Token/完整执行与部署目录审计；支持fee<=300/10000的标准Pool，factory动态值按同块读取；合成RPC与独立参考已验证，真实链历史块及收款差分仍未完成|
|velodrome-v2-volatile|velodrome-v2-volatile/b3065d8b-1|EVM|3,7.3,11.3,12.2,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP18,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|固定源码标准Token/factory fee≤300/10000；Go固定块解析、原Solidity数学与后余额、共享状态及正常PG/Kafka双副本限定验证；完整Pool.swap/Token行为、OP部署与历史资格未完成|
|velodrome-v2-stable|velodrome-v2-stable/b3065d8b-1|EVM|3,7.3,11.3,12.2,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP18,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|独立_f舍入语义，保留Aerodrome旧_k分支；216原数学/8序列24步及30舍入控制，factory暂停/reorg/恢复通过；不将固定标签当实际OP部署证明|
|lfj-lb-v21|lfj-lb/v2.1.0-fee-oracle-1|EVM|3,7.3,11.3,12.2,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP18,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|独立field31、完整有界bin集/份额、动态费/协议费/Oracle；96原ExactIn/19API38原swap及10负控通过；阶段48新增独立目标输入与显式额外到账，216完整/72拒绝/18连续及181API原Pair/24额外到账/9负控同源码通过；标准Token/同步余额/外部funds限定，真实Avalanche部署资格/公开执行与链费用仍未闭合|
|curve-plain2-basic|curve-plain2-basic/factory-5|EVM|4,7.3,11.3,12.2,12.11|WP02,WP04,WP05,WP11,WP12,WP13|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|仅factory Plain2Basic Vyper0.3.1两币/固定rate_multipliers；其他plain/meta/crypto/NG、动态rate/rebase未支持；合成RPC与独立参考已验证，真实链历史块及收款差分仍未完成|
|balancer-v2-stable2|balancer-v2-stable2/stable-1|EVM|4,7.3,11.3,12.2,12.11|WP02,WP04,WP05,WP11,WP12,WP13|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|双币标准StablePool/零管理余额、固定块amp与scaling；阶段75补独立GIVEN_OUT及路由/输入分配/API/前端，720原StableMath场景和180连续步；参考为原数学/现金库包装，完整Vault/Token转账/Composable/BPT/rate及执行模板未验收|
|aerodrome-slipstream|aerodrome-slipstream/cl-1|EVM|4,12.2,12.11|WP11,WP18|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|6word slot0/10word tick、factory及clone实现身份、两个fee module均为零的金额域；未实现非零外部费用模块、reward/fee-growth忠实重演及完整执行；金额参考目前为原V3 SDK；真实部署目录与Slipstream全合约执行差分未完成|
|aerodrome-slipstream-context|aerodrome-slipstream/f8717faa-module-1|EVM|3,7.3,11.3,12.2,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP18,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|独立field32、固定原模块/Voter/Factory与有界调用方费用上下文、完整staking/reward/tick/Oracle后状态；原ExactIn90完整swap/4序列12步与13实际API通过；阶段47新增独立ExactOut90完整/18部分拒绝/12连续、49实际API原程序/9负控及当前同源码回归通过；其他模块/Token/真实Base来源、链费用与生产执行仍未完成|
|pancake-infinity|其他CL/hook及Bin版本|EVM|4,12.2,12.11|WP11|NOT_IMPLEMENTED|NOT_IMPLEMENTED|NOT_IMPLEMENTED|pancake-infinity-cl/cl-1静态零hook标准域另列PARTIAL_LOCAL；其他CL参数/动态swap hook与native完整结算、Bin其他动态/hook/native组合及完整执行未完成；静态零hook完整树独立服务已在对应版本完成有限验收|
|pancake-v3-lm-context|pancake-v3/98684794-lm-1|EVM|4,7.3,11.3,12.2,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP18,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|固定98684794 Pool/LM/MasterChef、独立field33、完整费用/奖励/tick/Oracle及共享池顺序重算已通过主目录限定验收，13实际API/13原swap及9负控通过；阶段46新增标准Token单池ExactOut80完整/20部分拒绝/12连续及49实际API原swap/9负控通过；第四代范围另列，净链费、可信来源与公开执行仍未完成|
|pancake-v3-bsc-lm-v4|pancake-v3/bsc-p400-lm1000000-mc999-v4|EVM|3,7.3,11.3,12.2,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP18,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|独立field34、三代前置真实tick/flag与按优先级惰性迁移；64原案例/15连续步和13实际API/6迁移的原程序回放已通过；proto深拷贝补修后Go全检/双Rust全目标lint及定向/当前13API原程序回放均通过；标准Token有限ExactIn，不含位置修改/领取/可信历史/净链费/生产执行|
|sushiswap-trident|独立pool模型/版本待固定|EVM|4,12.2,12.11|WP11|NOT_IMPLEMENTED|NOT_IMPLEMENTED|NOT_IMPLEMENTED|不与SushiV2互认；协议/合约或program升级后重新固定身份并验证，未知版本拒绝|
|lfj-liquidity-book|离散bin的具体版本待固定|EVM|4,12.2,12.11|WP11,WP18|NOT_IMPLEMENTED|NOT_IMPLEMENTED|NOT_IMPLEMENTED|bin/动态费用专属模型；协议/合约或program升级后重新固定身份并验证，未知版本拒绝|
|raydium-cpmm|archived cpmm-v1 decoder only; nonzero creator extension rejected|Solana|4,12.2,12.11|WP20|PARTIAL_LOCAL|NOT_IMPLEMENTED|NOT_IMPLEMENTED|仅archive旧零扩展布局有限解析，不报价；当前creator源码profile另列独立支持；此旧标签不自动升级；版本身份/Token扩展/执行不从新profile借用|
|raydium-clmm|其他order/reward/Token与程序协议版本|Solana|4,12.2,12.11|WP20|NOT_IMPLEMENTED|NOT_IMPLEMENTED|NOT_IMPLEMENTED|raydium-clmm-static/token-fee/bitmap/custom-seed/dynamic五个固定6d7a0f82经典SPL范围另列局部实现；初始化奖励/限价订单/Token2022/其他程序版本及完整SBF执行未实现|
|orca-whirlpools|其他Orca fee/oracle/token/reward版本|Solana|4,12.2,12.11|WP20|NOT_IMPLEMENTED|NOT_IMPLEMENTED|NOT_IMPLEMENTED|orca-whirlpools-static/rewards与新adaptive固定cc683ec9各自限定范围另列；adaptive初始化奖励/dynamic数组组合、Token2022/扩展及完整SBF执行未实现|
|meteora-dlmm|1532e540独立MM/InputOnly、OnlyY、bitmap、rewards、orders、token2022版本|Solana|4,7.3,8,11.3,12.2,12.10,12.11,12.14,13|WP02,WP03,WP05,WP09,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|PARTIAL_LOCAL|六个固定MM/OnlyY/bitmap/rewards/orders/选定Token2022版本已有局部解析及精确报价；单池unsigned准备、显式ATA/WSOL、同次RPC捕获与独立原SBF、钱包单次发送和恢复回执已实现。选定转账费不扩大为任意Token扩展；生产可信银行、跨版本/高级策略交叉、host/奖励领取与订单管理仍缺|
|pump|pump-bonding/7a19affaa2c6-sol-1；独立migration版本|Solana|4,7.3,11.3,12.2,12.6,12.10,12.11,12.14|WP02,WP03,WP05,WP09,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|PARTIAL_LOCAL|经典SOL曲线和独立migration模型已有局部精确数学；仅经典SOL曲线开放本地unsigned/显式ATA/独立原SBF/钱包回执，迁移不借用该执行能力。迁移库存/已迁移清零与原migrate账户交接和重复幂等的历史限定验收保留；非SOL/Token2022/返还奖励/自动发现/可信生产执行仍缺|
|pump-amm-classic-wsol|pump-amm/6a7ad34596d8-classic-wsol-1|Solana|4,7.3,12.6,12.10,12.11,12.14|WP02,WP03,WP05,WP09,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|PARTIAL_LOCAL|固定classic+backedWSOL与boost独立profile已有局部报价；单池unsigned、显式ATA/WSOL补足/临时退款/输出解包、同次RPC捕获独立原SBF、钱包单次发送及实际租金/重启回执已实现。本批旧资金22场景回归；v2/Token2022资产/非WSOL/高级费用与共享多池/自动发现、生产可信来源及公开执行仍缺|
|pancake-infinity-bin|pancake-infinity-bin/3c5ef1be-static-1|EVM|7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP09,WP11,WP12,WP13,WP14,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|固定原BinPoolManager/Vault，静态LP fee<=100000、零hook、标准非零ERC20；完整root/L1/L2/all bins/share、active与树外锁定dust、固定块来源和manager共享账本已接入；限定服务本地验收通过；动态hook/override/native、完整Vault unlock/settlement及真实部署未完成；阶段77补完整树ExactOut/真实burn尘埃、共享manager币种账本/分配/计划/API/前端；完整Manager/Vault结算及执行模板仍未完成；阶段77补完整树ExactOut/真实burn尘埃、共享manager币种账本/分配/计划/API/前端；完整Manager/Vault结算及执行模板仍未完成|
|pancake-infinity-cl|pancake-infinity-cl/cl-1 (source 3c5ef1be)|EVM|4,7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP04,WP05,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|零hook/静态LPfee<1e6/标准ERC20；parameters仅tickSpacing<<16，独立6word key/manager getters/pause；每方向protocol fee<=4000；未知参数/hook/dynamic/native/Bin拒绝；Vault约束/管理器unlock/settlement/真实执行及部署审计未完成；阶段76补原生正数输出目标、容量拆单/顺序重算、manager计划/API/前端有限验收；完整Manager结算/执行仍未实现|
|raydium-cpmm-creator|raydium-cpmm/f4b9b263-spl-1|Solana|4,7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP07,WP09,WP10,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|PARTIAL_LOCAL|固定f4b9b263 creator布局/经典SPL域；同次finalized状态/ProgramData/Token/Clock与程序身份校验、creator费与完整后状态；原CP-Swap/Token SBF、RPC捕获独立模拟、显式ATA/WSOL补足/临时退款/解包、钱包单次发送及租金/重启回执通过；真实部署/source-binary/runtime activation、历史bank共识与生产准入、Token2022/其他版本、多路执行和完整链流/WAL/历史补洞仍待完成|
|orca-whirlpools-static|orca-whirlpools/cc683ec9-spl-static-1|Solana|4,7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|PARTIAL_LOCAL|固定cc683ec9静态tier/空System Oracle/经典SPL，固定及动态数组精确数学；单池ExactIn/Out unsigned、显式ATA/WSOL/临时退款解包、完整Pool/tick同捕获原SBF和钱包单次发送/恢复回执已验；1/2/3方向数组原ABI、PDA/代码/Clock、Vault已有SOL捐赠保持原值；adaptive/Token2022/其他程序版本/生产与历史bank共识资格、完整流/WAL/历史补洞及多池执行仍缺|
|raydium-clmm-static|raydium-clmm/6d7a0f82-spl-static-1|Solana|4,7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|PARTIAL_LOCAL|固定6d7a0f82经典SPL/WSOL、静态输入费/default dynamic/legacy seed/无奖励无挂单、基础bitmap与3..7数组；局部解析与精确ExactIn/Out、完整Pool/Observation/数组和费用/奖励后状态已验；阶段83补单池unsigned、同次捕获独立原SBF、显式ATA/WSOL/临时退款解包、实际资金/费用/租金及钱包单次发送和重启回执，三模型56场景统一本地验收通过；附属Token2022程序不开放Token2022资产扩展，bitmap/custom-seed/dynamic与奖励高级组合、订单/奖励领取、真实部署source-binary/runtime activation、历史bank共识/生产、多池执行和完整流/WAL仍待独立验收|
|raydium-clmm-token-fee|raydium-clmm/6d7a0f82-spl-token-fee-1|Solana|4,7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|PARTIAL_LOCAL|固定6d7a0f82经典SPL/WSOL、静态费/default dynamic/legacy seed/无奖励无挂单、基础bitmap与Token0Only/Token1Only原分支舍入；局部解析与精确ExactIn/Out、完整Pool/Observation/数组和费用/奖励后状态已验；阶段83补单池unsigned、同次捕获独立原SBF、显式ATA/WSOL/临时退款解包、实际资金/费用/租金及钱包单次发送和重启回执，三模型56场景统一本地验收通过；附属Token2022程序不开放Token2022资产扩展，bitmap/custom-seed/dynamic与奖励高级组合、订单/奖励领取、真实部署source-binary/runtime activation、历史bank共识/生产、多池执行和完整流/WAL仍待独立验收|
|raydium-clmm-bitmap|raydium-clmm/6d7a0f82-spl-bitmap-1|Solana|4,7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|PARTIAL_LOCAL|新profilemandatory1832extension、14正/负512bit分组与跨base范围，静态mode0/1/2/classicSPL；364原native案例/12序列24步、86源索引/4非法、28进程、两阶段审查与冻结完整回归通过；阶段84补该已声明经典SPL模型的完整扩展/seed/动态费准备、原swap_v2 unsigned、显式ATA/WSOL、同捕获独立SBF和单次钱包发送/重启回执；三个模型162API/162回执/324原交易、33检查/306Rust/667Chrome已统一本地验收。奖励/订单/Token2022及未声明交叉组合、多池、真实历史bank/完整流/WAL和生产资格仍缺；见[阶段84](#plan-2026-11-16-rayclmm-advanced-execution)|
|raydium-clmm-custom-seed|raydium-clmm/6d7a0f82-spl-custom-seed-1|Solana|4,7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|PARTIAL_LOCAL|固定create_customizable_pool_v2/seed1..65535u16LE、mandatorybitmap、staticfee0/1/2/classicSPL；旧profileseed0隔离；464原native案例/48序列96步、18身份错误探针/9身份向量、32进程、双seed状态隔离、两阶段审查和冻结完整回归通过；阶段84补该已声明经典SPL模型的完整扩展/seed/动态费准备、原swap_v2 unsigned、显式ATA/WSOL、同捕获独立SBF和单次钱包发送/重启回执；三个模型162API/162回执/324原交易、33检查/306Rust/667Chrome已统一本地验收。奖励/订单/Token2022及未声明交叉组合、多池、真实历史bank/完整流/WAL和生产资格仍缺；见[阶段84](#plan-2026-11-16-rayclmm-advanced-execution)|
|raydium-clmm-dynamic|raydium-clmm/6d7a0f82-spl-dynamic-1|Solana|4,7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|PARTIAL_LOCAL|固定DynamicFeeInfo9字段/原time-reference波动/spacing分段exact-in，mandatorybitmap/classicSPL/mode0..2/seed0..65535限定范围；348原native案例/12序列24步、8时间不变/6admission探针、2原始bank完整Pool及41进程、两阶段审查和冻结完整回归通过；阶段84补该已声明经典SPL模型的完整扩展/seed/动态费准备、原swap_v2 unsigned、显式ATA/WSOL、同捕获独立SBF和单次钱包发送/重启回执；三个模型162API/162回执/324原交易、33检查/306Rust/667Chrome已统一本地验收。奖励/订单/Token2022及未声明交叉组合、多池、真实历史bank/完整流/WAL和生产资格仍缺；见[阶段84](#plan-2026-11-16-rayclmm-advanced-execution)|
|balancer-v2-weighted2-equal|balancer-v2-weighted2/equal-1|EVM|7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|不可变双币50/50、普通非BPT与零资产管理器；既有765原onSwap/45组180步与PG/Kafka限定验收；阶段74补独立ExactOut、API及前端；配置方Token声明不等于部署证明，执行模板未完成|
|balancer-v2-weighted2-static|balancer-v2-weighted2/static-1|EVM|7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|不可变双币、权重各≥1%且总和1e18；既有5355原onSwap/315组1260步及240pow向量；阶段74补独立ExactOut、API与前端，保留equal旧版；多币另列profile，Managed/Composable/BPT/rate及执行不借用本版本|
|balancer-v2-weighted-multi|balancer-v2-weighted-multi/static-1|EVM|7.3,11.3,12.2,12.9,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|3至20币单物理余额向量、全部token路由边与固定块读取；阶段74补GIVEN_OUT与非首币对路由、最小输入网格及同池顺序重算、API/前端；Managed/Composable另列，完整Vault/Token转账与执行模板未完成|
|curve-plain3-basic|curve-plain3-basic/factory-1|EVM|7.3,11.3,12.2,12.9,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|三币Plain固定源码/布局/原exchange差分、完整后余额及正常RPC/路由通过，Meta/Lending/NG/Crypto不能由此替代|
|curve-plain4-basic|curve-plain4-basic/factory-1|EVM|7.3,11.3,12.2,12.9,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|四币Plain固定源码/布局/原exchange差分、多资产路由与完整后余额通过|
|curve-stableswap-ng|curve-stableswap-ng/3332cd44-standard-1|EVM|7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP09,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|固定原Vyper版本2–8币标准type0；完整raw/admin/effective余额、动态费、packed价格/D/EMA及正常服务接入已开发，三组正式进程回归及限定范围验收通过；rebasing/oracle/ERC4626、clones及真实部署/完整执行仍未完成|
|curve-crypto2-full|curve-crypto2-full/d7d04cd9-standard-1|EVM|7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP09,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|固定d7d04cd9原CryptoSwap2及原V4 LP模板；标准ERC20/settled A-gamma，完整price-scale调整/fee/gulp/LP账本已接入固定块解析和普通报价；正常服务进程与独立审查通过；本版本继续拒绝active ramp，活动ramp由独立版本另列；其他Crypto版本/特殊Token、真实部署与完整执行未完成|
|curve-crypto2-ramp|curve-crypto2-ramp/d7d04cd9-ramp-1|EVM|7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP09,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|固定d7d04cd9原CryptoSwap2与原V4 LP；标准ERC20的活动/到期/停止ramp，完整起点/未来端点及时间；原公开管理调用/完整交换、固定块解析与双副本正常服务限定验收通过；其他版本/特殊Token/真实历史及完整执行未完成|
|curve-metausd-direct|curve-metausd-direct/5272fd2b-fixed-base-1|EVM|7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP09,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|固定MetaUSD实现与EIP1167初始化池，标准meta/3CRV直接兑换、base虚拟价格依赖和完整raw/admin状态；针对性与四组正式流程回归、有限范围验收均通过；underlying、可变base同计划及真实部署/完整执行未完成|
|balancer-v2-managed-snapshot|balancer-v2-managed-snapshot/no-bpt-fees-1|EVM|7.3,11.3,12.2,12.9,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|2至38币当前权重快照，必须swaps enabled且协议/管理swap fee为零；非零费用BPT铸造仍未完成；后续非零费用批次发现所用旧参考源码runtime超EIP170，已有合成解析/零费数学结果不构成真实部署版本验收；阶段78补实际压缩/插值权重与费用的ExactOut/完整余额、任意币对/顺序分配/API/前端；保留原30%输出与开关/零BPT费用门禁，仍不构成生产部署/执行验收|
|balancer-v2-managed-fees|balancer-v2-managed-fees/e281ff40-bpt-1（参考契约，在线拒绝）|EVM|7.3,11.3,12.2,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP13,WP25|NOT_IMPLEMENTED|PARTIAL_LOCAL|NOT_IMPLEMENTED|非零协议/管理费BPT数学、完整供应量/收款方状态与原回调参考已实现；该固定旧源码runtime超过EIP170，正常配置/parser拒绝、在线报价未开放，需可部署实际版本后继续闭环|
|balancer-v2-managed-aum|balancer-v2/20230411-managed-aum-3|EVM|7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|实际20230411原Factory/Vault正常部署、2–50币完整注册/AUM/实际钱包状态已接入；单池、四池目录及跨物理池共享Vault历史限定验收已完成（T34.8/9），新源码需重新冻结验证；旧版本继续拒绝活动schedule。BPT路由、rates/mixed、启用breaker、可信链/公开执行及其他版本仍未完成|
|balancer-v2-managed-scheduled|balancer-v2/20230411-managed-scheduled-1|EVM|7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|独立field40/raw整数clock、2/3币单池及两个三币物理池混合/双活动M1–M4已限定通过；234单池/144多池原状态、实际API两路、原VM真实余额/连续全后状态/新冻结全检与审查一致。ONE±2和固定SWAP/YIELD构造ID按原语义保留，旧aum-3不提升。全部2–50参数/能力组合、BPT/rates/breaker、可信链与public执行仍未完成|
|balancer-v2-composable-regular|balancer-v2-composable-regular/no-rates-1|EVM|7.3,11.3,12.2,12.9,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|仅无rate provider的underlying间swap，完整余额含BPT但路由排除BPT；BPT数学内核有参考，完整待结费用/实际供应量/缓存刷新未完成；阶段79补underlying目标输出/任意BPT位置及完整现金、BPT数学/图/计划/HTTP拒绝、网格/API/前端；完整BPT/缓存/资金执行仍未完成|
|orca-whirlpools-adaptive|orca-whirlpools/cc683ec9-spl-adaptive-1|Solana|7.3,8.12,11.3,12.2,12.5,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP09,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|PARTIAL_LOCAL|完整初始化Oracle/独立fee tier PDA、3fixed arrays、经典SPL和未初始化reward的独立版本；完整source/bank/wire及raw/fee/Oracle/tick后状态已实现；正常进程本地验收通过；其他奖励/数组交叉组合、Token2022与真实来源资格仍缺；阶段82补该固定经典SPL模型的单池ExactIn/Out unsigned、完整初始化Oracle及写权限、ATA/WSOL资金、同捕获原SBF和真实钱包/租金/重启回执；54场景三模型统一本地验收通过，其他版本/Token2022/生产与历史bank共识资格和多池执行仍缺|
|orca-whirlpools-adaptive-rewards|orca-whirlpools/cc683ec9-spl-adaptive-rewards-1|Solana|7.3,8.12,11.3,12.2,12.5,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP09,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|PARTIAL_LOCAL|固定cc683ec9 classic SPL/三fixed arrays/完整Oracle与初始化奖励增长、跨tick与完整后状态，正常RPC/双副本限定验收通过；dynamic arrays、Token2022、reward领取/position结算及真实历史仍未完成；阶段82补该固定经典SPL模型的单池ExactIn/Out unsigned、完整初始化Oracle及写权限、ATA/WSOL资金、同捕获原SBF和真实钱包/租金/重启回执；54场景三模型统一本地验收通过，其他版本/Token2022/生产与历史bank共识资格和多池执行仍缺|
|orca-whirlpools-adaptive-dynamic|orca-whirlpools/cc683ec9-spl-adaptive-dynamic-1|Solana|7.3,8.12,11.3,12.2,12.5,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP09,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|PARTIAL_LOCAL|classic SPL/完整Oracle/三连续fixed或dynamic混合数组及奖励，正常解析/数学/双副本服务限定验收通过；补充/稀疏数组、Token2022、领取/position及真实部署历史仍未完成；阶段82补该固定经典SPL模型的单池ExactIn/Out unsigned、完整初始化Oracle及写权限、ATA/WSOL资金、同捕获原SBF和真实钱包/租金/重启回执；54场景三模型统一本地验收通过，其他版本/Token2022/生产与历史bank共识资格和多池执行仍缺|
|orca-whirlpools-rewards|orca-whirlpools/cc683ec9-spl-rewards-1|Solana|7.3,11.3,12.2,12.9,12.14|WP02,WP03,WP05,WP11,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|PARTIAL_LOCAL|固定cc683ec9奖励经典SPL，完整三奖励信息/tick outside账本、固定及动态数组、双向ExactIn/Out和跨tick后状态；阶段81原legacy swap/同捕获独立SBF、ATA/WSOL显式资金、实际收款/费用租金及重启回执已验；原Pool/所有tick/Vault捐赠/Oracle与失败全账户回滚核对，未借用adaptive/Token2022/其他版本；奖励领取、生产历史bank共识资格、完整流/WAL和多池执行仍缺|
|raydium-clmm-rewards|raydium-clmm/6d7a0f82-spl-rewards-1|Solana|7.3,11.3,12.2,12.9,12.14|WP02,WP03,WP05,WP11,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|PARTIAL_LOCAL|固定6d7a0f82经典SPL/WSOL、静态输入费/default dynamic/legacy seed/基础bitmap奖励版，完整三奖励与tick outside账本；局部解析与精确ExactIn/Out、完整Pool/Observation/数组和费用/奖励后状态已验；阶段83补单池unsigned、同次捕获独立原SBF、显式ATA/WSOL/临时退款解包、实际资金/费用/租金及钱包单次发送和重启回执，三模型56场景统一本地验收通过；附属Token2022程序不开放Token2022资产扩展，bitmap/custom-seed/dynamic与奖励高级组合、订单/奖励领取、真实部署source-binary/runtime activation、历史bank共识/生产、多池执行和完整流/WAL仍待独立验收|
|uniswap-v4-advanced|uniswap-v4/core-advanced-1|EVM|7.3,11.3,12.2,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP14|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|key fee与live LP fee独立，native currency0/无swap回调hook；任意swap hook、自定义会计与真实settlement未完成|
|pancake-infinity-cl-advanced|pancake-infinity-cl/cl-advanced-1|EVM|7.3,11.3,12.2,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP14|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|动态LP费快照/native/callback位校验与CL后状态；本CL版本不兼容Bin；静态零hookBin完整树/跨Bin服务已在独立版本验收，完整manager/Vault结算未完成|

|balancer-v2-managed-bpt|balancer-v2/20230411-managed-bpt-1|EVM|7.3,11.3,12.2,12.9,12.11,12.12,12.13,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP14,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|独立完整状态/固定H解析/原整数与有限BPT路由正在验证；已启用breaker/其他custody/公开执行与完整生产未验收|

|balancer-v2-managed-breaker|balancer-v2/20230411-managed-breaker-1|EVM|7.3,11.3,12.2,12.9,12.11,12.12,12.13,12.14|WP02,WP03,WP04,WP05,WP09,WP11,WP12,WP13,WP14,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|固定slot12/六getter、cached/dynamic精确整数、完整后Pool及四mode source/graph/API与233实际原交易已通过限定本地验收；rate/mixed/广参数/可信真实链与公开执行未验收|

<a id="design-traceability"></a>

## 功能与设计条款逐项对应

验证列指向原始验收报告；源码和测试绑定由集成检查规则维护。历史本地结果不替代后续变更的重新验证。

|设计功能|工作包|状态|已完成|剩余|验证|
|---|---|---|---|---|---|
|7.9.1|WP04,WP07|PARTIAL_LOCAL|head只读header发现同高换hash/父链不连续；先撤销scope；不同配置endpoint确认head/parent一致后恢复|真实链且独立故障域来源证明未验收；loopback双RPC仅协议测试|[reorg-smoke.json](integration/artifacts/local/reorg-smoke.json)|
|7.9.2|WP07|PARTIAL_LOCAL|按parent/hash倒序定位共同祖先，限制旧分支深度、新分支长度、总RPC期限和staging总字节|真实历史fork fixture未验收|[reorg-smoke.json](integration/artifacts/local/reorg-smoke.json)|
|7.9.3|WP06,WP07,WP09|PARTIAL_LOCAL|确认分叉后REORG_BEGIN推进canonical_epoch；producer prepared/Kafka/KV；replica pending barrier立即撤销旧共享句柄|真实公网链/生产Kafka故障域未验收|[reorg-smoke.json](integration/artifacts/local/reorg-smoke.json)|
|7.9.4|WP07|PARTIAL_LOCAL|限定池状态scope通过完整snapshot undo按parent/hash倒序回退；删除被替换旧分支精确hash；保留新分支undo并在同batch清理窗口外状态|normalized facts/event级undo与BLOCK_REVERT对分析广播尚未实现|[reorg-smoke.json](integration/artifacts/local/reorg-smoke.json)|
|7.9.5|WP05,WP07|PARTIAL_LOCAL|新分支按高度顺序读完整receipts与hash固定池状态，祖先内容/head最终身份校验；staging不可见|多协议增量patch和真实链差分fixture未验收|[reorg-smoke.json](integration/artifacts/local/reorg-smoke.json)|
|7.9.6|WP07,WP09|PARTIAL_LOCAL|仅精确匹配begin epoch/seq/generation/writer/scope的REORG_COMMIT原子发布池/图；Kafka读失败续已应用游标；prepared提交后崩溃恢复同cursor与新分支undo|retention丢失时pending屏障仍fail-closed，需owner发布合格新generation；生产内存压力未验收|[reorg-smoke.json](integration/artifacts/local/reorg-smoke.json)|
|7.9.7|WP07,WP21|PARTIAL_LOCAL|限定V2规范化Swap稳定事件ID、旧分支精确撤销清单/新分支事实同journal事务；隔离Go analytics Job重算OHLC与删除修订；同epoch未完成begin基线不复活旧事实|ClickHouse发布/查询、其他事实族/协议、真实链/生产故障域未验收|[analytics-journal-smoke.json](integration/artifacts/local/analytics-journal-smoke.json)|
|7.9.deep-reorg|WP07,WP24|PARTIAL_LOCAL|超窗/缺undo/来源分歧暂停scope；重启不可用旧KV复活；独立当前来源确认后新generation HEAD_FIRST恢复；临时故障同分叉不推进canonical_epoch|最终化假设破坏处置与真实长停机故障演练未验收|[reorg-smoke.json](integration/artifacts/local/reorg-smoke.json)|
|16.1|WP06,WP21|PARTIAL_LOCAL|限定V2 Raw receipt → Normalized Swap → Curated窗口；跨语言公共事实不读取私有在线KV|Raw不可变归档/更多事实族未实现|[analytics-journal-smoke.json](integration/artifacts/local/analytics-journal-smoke.json)|
|16.3|WP06,WP21|PARTIAL_LOCAL|现有Go二进制analytics静态candles/1有限Job；独立配置/state、只读Kafka，无第四服务|Rust/Solana分析插件未实现|[analytics-journal-smoke.json](integration/artifacts/local/analytics-journal-smoke.json)|
|16.4|WP06,WP21|PARTIAL_LOCAL|窗口保留事实重算OHLC、撤销空窗口删除修订、head暂定水位/历史缺口；复杂Swap暂停仅插件|真实链及finalized产品、复杂聚合未验收|[analytics-journal-smoke.json](integration/artifacts/local/analytics-journal-smoke.json)|
|16.8|WP06,WP21|PARTIAL_LOCAL|精确已应用cursor及topic UUID绑定，无跳过保留缺口；状态字节和读取预算，毒消息仅停Job|shadow回补/发布指针/归档与自动窗口retention待实现|[analytics-journal-smoke.json](integration/artifacts/local/analytics-journal-smoke.json)|
|7.7.facts-crash|WP06,WP21|PARTIAL_LOCAL|prepared JSON与Kafka Protobuf规范hash一致；事实和snapshot同cursor恢复|生产网络commit响应精确丢失等演练未验收|[analytics-journal-smoke.json](integration/artifacts/local/analytics-journal-smoke.json)|
|11.3.v3|WP02,WP11|PARTIAL_LOCAL|PoolState新增字段20 V3State，连续bitmap/initialized tick一一对应、价格/tick、gross/net、签名i128 BE16与金额BE32校验；V2状态不混用|其他未覆盖CLMM/Stable变种与V4/Infinity高级状态待实现；已新增singleton字段23的静态无hook范围另列|[v3-differential.json](integration/artifacts/local/v3-differential.json)|
|7.3.v3|WP04,WP05|PARTIAL_LOCAL|同blockHash验证runtime codeSHA256/factory/token/fee/slot0/liquidity/有限bitmap/ticks；整次读取期限、缺tick/lock/不合法ABI/错锚点拒绝|真实独立来源/链上部署身份审计、并行staging和工厂自动发现待验收|[v3-differential.json](integration/artifacts/local/v3-differential.json)|
|12.4.v3|WP11|PARTIAL_LOCAL|Q64.96双方向exact-in逐word/跨tick、zero liquidity、向上fee和delta取整、overflow fallback；MIN合法初始化穿越空段；264项SDK差分|真实链执行收款差分、其他CLMM与自动范围扩展未验收|[v3-differential.json](integration/artifacts/local/v3-differential.json)|
|12.10.v3|WP11,WP12,WP13|PARTIAL_LOCAL|成功只返回全额consumed/refund0和准确post-state；范围/价格/步骤预算耗尽明确拒绝，拆单候选保留实际探测输入输出，最终allocation完整守恒|ExactOut、部分成交产品语义、通用DAG/生产费用模型待实施|[v3-differential.json](integration/artifacts/local/v3-differential.json)|
|12.11.v3|WP05,WP11,WP14|PARTIAL_LOCAL|标准V3 adapter版本/代码身份、逐tick/liquidity/fee双方向报价；V3误用V2 calldata拒绝；双副本当前状态和多块reorg验证|V3执行器、独立执行、真实Gas及更多协议验收未完成|[v3-differential.json](integration/artifacts/local/v3-differential.json)|
|4.dex-version-matrix|WP05,WP10,WP11,WP18,WP20|PARTIAL_LOCAL|逐DEX/版本列parser/math/execution状态，新增PancakeV1/V2/V3/SushiV2，未知版本拒绝|真实链版本部署目录/升级审计、其余DEX版本均保留未实现|[multidex-differential.json](integration/artifacts/local/multidex-differential.json)|
|7.3.dex-versions|WP04,WP05|PARTIAL_LOCAL|同blockHash code/factory/token/reserve/timestamp/slot0/protocolFee/tick/LM，完整receipts与读后锚点；不同ABI版本明确分支|工厂发现/并行staging/raw证据归档/更多版本待实施|[multidex-differential.json](integration/artifacts/local/multidex-differential.json)|
|11.3.dex-version-identity|WP02,WP12,WP13|PARTIAL_LOCAL|V2State字段21及V3 feeProtocol字段20；adapter/fee/chain-family验证，地址形poolID跨legacy/命名版本拒绝大小写别名|真实协议目录审计与通用链地址规范化仍需扩展|[multidex-differential.json](integration/artifacts/local/multidex-differential.json)|
|12.2.dex-fees|WP10,WP11|PARTIAL_LOCAL|三种命名V2固定fee分开；PancakeV3 uint32协议费与交易者fee不重复扣除，零LM标准域；372独立参考|实际token行为/外部回调/真实收款差分未完成|[multidex-differential.json](integration/artifacts/local/multidex-differential.json)|
|12.11.dex-adapter-matrix|WP05,WP11,WP18,WP20|PARTIAL_LOCAL|6profile同block状态和两个完整副本，Pancake/Sushi双方向金额、code/LM失败关闭/reorg/恢复；其余协议逐项缺口|其余Curve/Balancer、Slip外部module/reward、V4/Infinity高级、Orca adaptive/rewards/Token2022、RaydiumCLMM奖励/订单/Token扩展及Meteora/Pump及实际执行仍未实现；已完成限定profile另列|[multidex-differential.json](integration/artifacts/local/multidex-differential.json)|
|11.3.stable-and-slipstream-state|WP02,WP25|PARTIAL_LOCAL|字段22的StableState及V3 implementation/factory身份，Go/Rust模型/精度/fee/A严格准入|更多模型/schema演进和真实部署目录待扩展|[stable-differential.json](integration/artifacts/local/stable-differential.json)|
|7.3.stable-and-slipstream-dependencies|WP04,WP05|PARTIAL_LOCAL|同hash code/clone/call/storage、rate/fee/amp、Vault/Pool pause、Slip双fee module；五profile同块发布与10项进程检查|工厂发现、raw归档、其余协议依赖未实现|[stable-differential.json](integration/artifacts/local/stable-differential.json)|
|12.2.stable-protocol-specific-rounding|WP11|PARTIAL_LOCAL|Aero先移出输入费；Curve dy−1和admin费；Balancer raw输入ceil费、不同scale及上下舍入；180原合约参考|实际token行为和原始交易收款差分/其他stable版本待实施|[stable-differential.json](integration/artifacts/local/stable-differential.json)|
|12.9.stable-shared-physical-pool|WP12,WP13|PARTIAL_LOCAL|16组连续共享stable计算双向/两顺序，路径与分配都用前次post-state更新，旧候选金额重算|通用DAG/跨协议联合求解/实际执行与Gas仍待实施|[stable-differential.json](integration/artifacts/local/stable-differential.json)|
|12.11.requested-actual-version-audit|WP05,WP11,WP20|PARTIAL_LOCAL|Curve/Aero/Balancer/Slip限定profile已实现；V4/Infinity/Orca/Raydium固定源码逐版本审计并列剩余；archive新布局误识别已封闭|其他Curve/Balancer、Slip动态module/reward、V4/Infinity高级与其他Solana实际版本、Orca adaptive/reward/token扩展、完整执行未完成；Orca static与Raydium creator局部实现另列|[stable-differential.json](integration/artifacts/local/stable-differential.json)|
|11.3.singleton-key-state|WP02,WP25|PARTIAL_LOCAL|独立字段23/Core与5/6word key、manager/rawPoolId身份、跨语言配置/wire|真实部署目录和schema演进待完成|[singleton-differential.json](integration/artifacts/local/singleton-differential.json)|
|7.3.singleton-manager-dependencies|WP04,WP05|PARTIAL_LOCAL|同hashV4extsload与Infinitykey/getters/pause，完整ticks和预算准入，136解析子用例|自动发现/raw归档、其他协议事实和真实历史块未完成|[singleton-differential.json](integration/artifacts/local/singleton-differential.json)|
|12.2.singleton-directional-fees|WP11|PARTIAL_LOCAL|方向protocol+LP费与逐word舍入、signedDelta边界、floor maxGross/spacing32767，416原库样本|特殊Token/动态hooks/完整执行收款差分未完成|[singleton-differential.json](integration/artifacts/local/singleton-differential.json)|
|12.9.singleton-shared-state|WP12,WP13|PARTIAL_LOCAL|4混合方向/4同方向原库连续序列匹配，旧候选金额重算，跨manager同key独立|通用DAG/联合求解、实际Gas和执行未完成|[singleton-differential.json](integration/artifacts/local/singleton-differential.json)|
|12.14.singleton-primary-reference|WP03,WP11,WP13|PARTIAL_LOCAL|1115主源码hash、36个实际编译主文件、原Pool.swap/CLPool.swap原生持久状态参考；双向/边界/拒绝/后状态|Shanghai执行，Cancun仅编译；管理器transient/unlock/settlement和真实余额差未验证|[singleton-differential.json](integration/artifacts/local/singleton-differential.json)|
|12.11.singleton-execution-boundary|WP14,WP16|PARTIAL_LOCAL|singleton报价保持INDICATIVE，V2 calldata明确拒绝|专属交易构造与独立管理器执行未实现|[singleton-differential.json](integration/artifacts/local/singleton-differential.json)|
|11.3.raydium-creator-state|WP02,WP25|PARTIAL_LOCAL|独立字段24、规范Solana角色、ProgramData/Token代码与Clock元数据、raw/三费/net一致性|部署目录与真实代码产物审计、schema演进未完成|[raydium-differential.json](integration/artifacts/local/raydium-differential.json)|
|7.3.raydium-finalized-account-set|WP04,WP05,WP20|PARTIAL_LOCAL|十账户finalized批次、Clock.slot=context、program/ProgramData/tokenhash/PDA/标准SPL/WSOL和块锚点复核|不是历史account selector；流/WAL/交易metadata及所有事实未完成|[raydium-differential.json](integration/artifacts/local/raydium-differential.json)|
|12.2.raydium-creator-fee-rounding|WP10,WP20|PARTIAL_LOCAL|ceil组合输入费/floorcreator拆分/ceil输出creator、protocol/fund和raw/net后状态；1604原crate整数参考|Token2022/实际收款/SBF全执行未实现；超组合rate案例是独立压力探测非入口可达证明|[raydium-differential.json](integration/artifacts/local/raydium-differential.json)|
|12.9.raydium-shared-fee-ledger|WP12,WP13|PARTIAL_LOCAL|18连续序列144笔按raw/counters/net/recentepoch更新，同向分配全额重算|通用DAG/Gas/CU及真实交易执行待实施|[raydium-differential.json](integration/artifacts/local/raydium-differential.json)|
|7.3.solana-owner-failure-revocation|WP07,WP09,WP20|PARTIAL_LOCAL|源验证错误持久化范围撤销；先本地notready，再fencedKafka；pending恢复独立新source合格性|Kafka暂停/解析器崩溃/无效源独立恢复本地已验证；完整网络分区/磁盘/多节点生产故障域仍未验收|[raydium-differential.json](integration/artifacts/local/raydium-differential.json)|
|12.14.raydium-primary-reference|WP03,WP10,WP20|PARTIAL_LOCAL|原程序crate native/no-entrypoint，76文件原样/依赖锁/1604样本18序列与13core/12helper，公共字段24独立protoc|未执行SBF/CPI/Tokenprogram/观测写入与真实账户交易约束|[raydium-differential.json](integration/artifacts/local/raydium-differential.json)|
|11.3.orca-q64-array-state|WP02,WP25|PARTIAL_LOCAL|独立字段25/Q64/规范Solana身份、完整arraybitmap/ticks/growth与wire，Go/Rust1045原TickMath向量|其他Orca adaptive/reward/token扩展状态、部署证明未完成|[orca-differential.json](integration/artifacts/local/orca-differential.json)|
|7.3.orca-finalized-static-accounts|WP04,WP05,WP20|PARTIAL_LOCAL|同finalized批次、ProgramData/Token/Clock/nulloracle、actual fixed/dynamic layouts和reward/control拒绝|流/补洞/历史账户/其他token/program版本未完成|[orca-differential.json](integration/artifacts/local/orca-differential.json)|
|12.2.orca-q64-static-rounding|WP11,WP20|PARTIAL_LOCAL|原Q64tick+step/fee/宽度、三数组shift、空区间、growth/owed wrapping、rawchecked和完整消耗|adaptivefees/真实Token收款/SBF执行未实现|[orca-differential.json](integration/artifacts/local/orca-differential.json)|
|12.9.orca-shared-array-poststate|WP12,WP13|PARTIAL_LOCAL|原native10共享序列80步按完整pool重算，含边界左cross后合法B→A、同反向raw/growth/ticks与候选金额重算|DAG/CU/实际交易次序与SBF未完成|[orca-differential.json](integration/artifacts/local/orca-differential.json)|
|12.14.orca-primary-manager-reference|WP03,WP11,WP13|PARTIAL_LOCAL|原program/accountloader/sparsebuilder/manager/update实际函数native原样执行，1111源码文件hash/1045tick向量|没有SBF/validator/CPI/Token程序执行，syntheticproof非部署；自定义maxsteps预算不等于原程序预算|[orca-differential.json](integration/artifacts/local/orca-differential.json)|
|12.11.orca-execution-boundary|WP14,WP16,WP20|PARTIAL_LOCAL|新Orca onlyINDICATIVE，拒绝EVMV2calldata、未知adaptive/token/rewards状态|Solana执行计划/交易/CU/账户锁/真实simulation未实现|[orca-differential.json](integration/artifacts/local/orca-differential.json)|
|11.3.rayclmm-bitmap-observation-state|WP02,WP25|PARTIAL_LOCAL|独立字段26/Q64/16wordbitmap/60tick数组/100observation、原source tick关系与Go/Rust/protoc、2053源向量|高级动态/订单/奖励/扩展Token状态未完成|[rayclmm-differential.json](integration/artifacts/local/rayclmm-differential.json)|
|7.3.rayclmm-finalized-pinned-accounts|WP04,WP05,WP20|PARTIAL_LOCAL|实际原layout/BEseed/PDA、ProgramData/Token/Clock和同finalized账户集；严格时间/静态fee/无奖励订单/经典SPL准入|实时流/WAL/历史补洞/其他program和token版本未完成|[rayclmm-differential.json](integration/artifacts/local/rayclmm-differential.json)|
|12.2.rayclmm-static-input-source-rounding|WP11,WP20|PARTIAL_LOCAL|原Q64tick/60tickbitmap搜索、每步输入ceil/费floor、宽度/增长wrap和checked账本、全额消耗/完整后状态|dynamic fee/fee_on_output/customseed/rewards/orders/bitmapextension/Token2022未实现|[rayclmm-differential.json](integration/artifacts/local/rayclmm-differential.json)|
|12.9.rayclmm-shared-pool-poststate|WP12,WP13|PARTIAL_LOCAL|12序列96步逐步匹配原program完整Pool，包含同反向与共享allocation候选金额重算|DAG/CU/实际交易次序及SBF未完成|[rayclmm-differential.json](integration/artifacts/local/rayclmm-differential.json)|
|12.14.rayclmm-native-original-reference|WP03,WP11,WP13|PARTIAL_LOCAL|6d7a0f82原swap_internal/Clock/RefMut账户无改写native，225原文件核验、480案例/2053向量/887273tick摘要|未运行Anchor完整指令/SBF/CPI/Token；syntheticcodeproof不证明部署；maxsteps保守准入非原程序计量|[rayclmm-differential.json](integration/artifacts/local/rayclmm-differential.json)|
|12.11.rayclmm-execution-boundary|WP14,WP16,WP20|PARTIAL_LOCAL|仅INDICATIVE，原CLMM typed state；拒绝误编码EVMV2calldata/未知模型|Solana交易/CU/账户锁/真实收款/simulation未实现|[rayclmm-differential.json](integration/artifacts/local/rayclmm-differential.json)|
|11.3.rayclmm-token-fee-state|WP02,WP25|PARTIAL_LOCAL|field60显式模式、旧JSON0兼容、独立profile与跨语言wire/PDA不减弱|高级dynamic/rewards/orders/extension/Token2022状态未完成|[rayclmm-token-fee-differential.json](integration/artifacts/local/rayclmm-token-fee-differential.json)|
|7.3.rayclmm-token-fee-accounts|WP04,WP05,WP20|PARTIAL_LOCAL|实际offset390模式1\|2、新旧profile准入、finalized原子账户集/读后block与原准入|流/WAL/历史补洞/高级programtoken版本未完成|[rayclmm-token-fee-differential.json](integration/artifacts/local/rayclmm-token-fee-differential.json)|
|12.2.rayclmm-token-fee-rounding|WP11,WP20|PARTIAL_LOCAL|输出ceil、partialdust完整输入、按feetoken计提growth/账本与rawnetoutput|dynamic/reward/order/extension/Token2022及实际Token收款未实现|[rayclmm-token-fee-differential.json](integration/artifacts/local/rayclmm-token-fee-differential.json)|
|12.9.rayclmm-token-fee-shared|WP12,WP13|PARTIAL_LOCAL|24序列192步原native完整Pool，对应同反向共享状态及allocation候选金额重算|通用DAG/CU/实际交易次序未完成|[rayclmm-token-fee-differential.json](integration/artifacts/local/rayclmm-token-fee-differential.json)|
|12.14.rayclmm-token-fee-reference|WP03,WP11,WP13|PARTIAL_LOCAL|原swap_internal/set_fee_on/RefMut/Clock无改写，968案例/24序列，225原文件与旧480byteidentity留证|native非完整Anchor/SBF/CPI/Token；合成codeproof非部署；预算保守预检|[rayclmm-token-fee-differential.json](integration/artifacts/local/rayclmm-token-fee-differential.json)|
|12.11.rayclmm-token-fee-execution-boundary|WP14,WP16,WP20|PARTIAL_LOCAL|新profileonlyINDICATIVE，拒绝EVMV2calldata，未知收费mode拒绝|Solana交易/CU/账户锁/独立SBFsimulation未实现|[rayclmm-token-fee-differential.json](integration/artifacts/local/rayclmm-token-fee-differential.json)|
|11.3.rayclmm-bitmap-extension-state|WP02,WP25|PARTIAL_LOCAL|独立profile/field61 extension正负各112word，Go-Rust宽度/PDA/bitmapproof/深拷贝及独立protoc高位保持|动态/自定义seed/奖励/订单/Token扩展状态仍未完成|[rayclmm-bitmap-differential.json](integration/artifacts/local/rayclmm-bitmap-differential.json)|
|7.3.rayclmm-bitmap-extension-finalized-account|WP04,WP05,WP20|PARTIAL_LOCAL|真实1832扩展账户、原PDA/pool绑定/owner/discriminator，15..19账户同finalized读取、旧准入保留|真实流/WAL/补洞/fork及其他program版本未完成|[rayclmm-bitmap-differential.json](integration/artifacts/local/rayclmm-bitmap-differential.json)|
|12.2.rayclmm-bitmap-extension-boundary-search|WP11,WP20|PARTIAL_LOCAL|跨基础±512及正负整512组、真实tick域floor边界、空组跳过与最近候选missingproof拒绝，费用与完整后状态不变|dynamic/rewards/orders/Token2022与真实Token执行未实现|[rayclmm-bitmap-differential.json](integration/artifacts/local/rayclmm-bitmap-differential.json)|
|12.9.rayclmm-bitmap-extension-shared-poststate|WP12,WP13|PARTIAL_LOCAL|原native同反向完整Pool连续序列、extensionproof保持与sharedallocation按实际前状态重算|DAG/CU/真实交易次序及全局最优证明未完成|[rayclmm-bitmap-differential.json](integration/artifacts/local/rayclmm-bitmap-differential.json)|
|12.14.rayclmm-bitmap-extension-original-reference|WP03,WP11,WP13|PARTIAL_LOCAL|原TickArrayBitmapExtension RefMut直接传swap_internal，86源flip/check索引、正负/nextbank原structfixture、旧480/968保持字节一致|native非完整Anchor/SBF/CPI/部署，保守参考预算与业务实际预算分别注明|[rayclmm-bitmap-differential.json](integration/artifacts/local/rayclmm-bitmap-differential.json)|
|12.11.rayclmm-bitmap-extension-execution-boundary|WP14,WP16,WP20|PARTIAL_LOCAL|onlyINDICATIVE，明确新旧profile隔离、真实extension变化新报价/移除数组success→reject与恢复测试|交易构造/CU/账户锁/完整SBFsimulation仍未完成|[rayclmm-bitmap-differential.json](integration/artifacts/local/rayclmm-bitmap-differential.json)|
|11.3.rayclmm-custom-seed-state|WP02,WP25|PARTIAL_LOCAL|field62显式seedu16LE、原PDA/width/新旧profile/default0JSON兼容及独立protoc高位往返|动态/奖励/订单/Token扩展状态未完成|[rayclmm-seed-native-source.json](integration/artifacts/local/rayclmm-seed-native-source.json)|
|7.3.rayclmm-custom-seed-accounts|WP04,WP05,WP20|PARTIAL_LOCAL|实际391u16LE/393..397padding，custompool/bump/vaultauthority/obs/arrays/ext派生与15..19同槽finalized/读后block|真实流/WAL/补洞/fork及其他program/token版本未完成|[rayclmm-seed-native-source.json](integration/artifacts/local/rayclmm-seed-native-source.json)|
|12.2.rayclmm-custom-seed-exact-in|WP11,WP20|PARTIAL_LOCAL|独立Q64/fee0..2/extension计算复用原模型，customseed新身份完整postPool与原native参考比较|动态/奖励/订单/Token2022/真实Token收款未实现|[rayclmm-seed-native-source.json](integration/artifacts/local/rayclmm-seed-native-source.json)|
|12.9.rayclmm-custom-seed-physical-pool|WP12,WP13|PARTIAL_LOCAL|同反向完整Pool序列与allocation重算，同config/mints不同seed物理池/状态互不混淆|DAG/CU/真实交易次序及全局最优证明未完成|[rayclmm-seed-native-source.json](integration/artifacts/local/rayclmm-seed-native-source.json)|
|12.14.rayclmm-custom-seed-reference|WP03,WP11,WP13|PARTIAL_LOCAL|原set_seed_index/PoolState.seeds/key/扩展RefMut/swap_internal/Clock无改写参考，独立身份原structfixture与旧数据保护|native非完整Anchor/SBF/CPI/部署，保守预检预算限制保留|[rayclmm-seed-native-source.json](integration/artifacts/local/rayclmm-seed-native-source.json)|
|12.11.rayclmm-custom-seed-execution-boundary|WP14,WP16,WP20|PARTIAL_LOCAL|onlyINDICATIVE/新profile身份独立、禁止EVMV2calldata，错误seed账户失败封闭|交易构造/CU/账户锁/完整SBFsimulation未完成|[rayclmm-seed-native-source.json](integration/artifacts/local/rayclmm-seed-native-source.json)|
|11.3.rayclmm-dynamic-state|WP02,WP25|PARTIAL_LOCAL|dynamic_fee可选九字段/protofield63，新profile要求Some，旧profile隔离、u16/u32宽度/PDA与独立protoc；本批限定范围验证通过|更多状态演进/链版本生产验收待完成|[rayclmm-dynamic-native-source.json](integration/artifacts/local/rayclmm-dynamic-native-source.json)|
|7.3.rayclmm-dynamic-accounts|WP04,WP05,WP20|PARTIAL_LOCAL|原DynamicFeeInfo packed80实际字段/严格padding、原始2bank完整Pool/RPC准入；本批限定范围验证通过|奖励/订单/Token2022/Yellowstone/WAL/fork未完成|[rayclmm-dynamic-native-source.json](integration/artifacts/local/rayclmm-dynamic-native-source.json)|
|12.2.rayclmm-dynamic-exact-in|WP11,WP20|PARTIAL_LOCAL|原reference时间更新、spacing细分动态exact-in与完整后Pool/实际innerstep预算；本批限定范围验证通过|更多原协议状态组合/完整SBF执行未完成|[rayclmm-dynamic-native-source.json](integration/artifacts/local/rayclmm-dynamic-native-source.json)|
|12.9.rayclmm-dynamic-physical-pool|WP12,WP13|PARTIAL_LOCAL|共享池连续消费完整dynamic后状态，路由输入守恒；本批限定范围验证通过|全局优化与执行生产验收仍待完成|[rayclmm-dynamic-native-source.json](integration/artifacts/local/rayclmm-dynamic-native-source.json)|
|12.14.rayclmm-dynamic-reference|WP03,WP11,WP13|PARTIAL_LOCAL|原native独立参考/348案例12连续序列及时间重锚金额不变证明；本批限定范围验证通过|真实部署与Anchor/SBF/Token程序执行未完成|[rayclmm-dynamic-native-source.json](integration/artifacts/local/rayclmm-dynamic-native-source.json)|
|12.11.rayclmm-dynamic-execution-boundary|WP14,WP16,WP20|PARTIAL_LOCAL|独立profile onlyINDICATIVE、禁止EVMV2calldata，依赖异常撤销；本批限定范围验证通过|Solana交易/CU/账户锁/独立simulation未完成|[rayclmm-dynamic-native-source.json](integration/artifacts/local/rayclmm-dynamic-native-source.json)|
|7.3.evm-receipt-integrity|WP04,WP05|PARTIAL_LOCAL|已实现显式交易列表、receipt/log关联、标准索引、全块日志连续顺序及异常数据先于池解析拒绝；Go全check通过|Go全check/race、27范围回归与审查通过；后续已补多源及RPC声明root局部核验；独立可信头与来源审核仍未完成|[验收报告](integration/artifacts/local/receipt-integrity-acceptance.json)|
|7.3.evm-receipt-witnesses|WP04,WP05,WP07|PARTIAL_LOCAL|已实现多配置端点完整receipt/同块前后锚点核对，URL去重/健康门槛、合法hex字节比较、整块预算及流式摘要；race专项通过|PG/Kafka双副本11项、新旧28范围回归及两阶段审查通过；后续已补RPC声明root局部核验；共同伪造头/根、真实供应商独立与共识证明仍未完成|[验收报告](integration/artifacts/local/receipt-witness-acceptance.json)|
|8.9.solana-rpc-source-wal|WP03,WP05,WP20,WP24|PARTIAL_LOCAL|正常RPC解码前sync证据WAL、来源/会话/hash链与耐久head、容量/损坏/写错失败封闭和有界后台任务完成|7项存储/2项RPC、异步准入/marker预算、10项真实双副本和新旧29范围回归通过；两阶段审查关闭；Yellowstone、保留轮转、流WAL连续恢复与完整fork仍未完成|[验收报告](integration/artifacts/local/source-wal-acceptance.json)|
|7.3.evm-receipt-root|WP03,WP05,WP07|PARTIAL_LOCAL|主源/witness typed根、严格字段存在性/互斥outcome、gas/bloom和前后root锚点完成；共同删尾含重算bloom被拒绝；13项进程与30范围回归通过，两阶段审查关闭|仅核对RPC声明root；独立区块头/transactionsRoot/共识、真实部署及链专有receipt仍未完成|[验收报告](integration/artifacts/local/receipt-root-acceptance.json)|
|8.15.solana-current-account-shards|WP04,WP05,WP20,WP24|PARTIAL_LOCAL|同源同finalized context的32片/35调用、Clock/首末bank屏障、总wire预算与WAL顺序完成；16池134账户和13池seed状态通过；10项进程/10范围回归及两阶段审查通过|仅provider声明的当前bootstrap；历史版本化账户/独立bank、Yellowstone/fork/EnsurePoolScope、WAL归档轮转仍未完成|[验收报告](integration/artifacts/local/account-shards-acceptance.json)|
|11.3.balancer-weighted2-equal-state|WP02,WP25|PARTIAL_LOCAL|独立WeightedState/Config/profile，field27与Go/Rust严格模型/版本/权重/费率/身份校验与快照深拷贝|其他权重/Managed/Composable/RateProvider未完成|[本批验收](integration/artifacts/local/weighted-acceptance.json)|
|7.3.balancer-weighted2-equal-block|WP04,WP05,WP20|PARTIAL_LOCAL|pool/Vault/两token代码、注册TWO_TOKEN、暂停、scales/decimals、weights、cash/managed/manager同blockHash读取；保留receipt根和读后锚定；大跨度失败前发布范围撤销|真实部署/历史块及独立header证据未完成|[本批验收](integration/artifacts/local/weighted-acceptance.json)|
|12.2.balancer-weighted2-equal-exact-in|WP11|PARTIAL_LOCAL|原raw扣费ceil、scales、divUp/powUp(ONE)、净输入30%上限、完整余额与uint256/112检查|其他权重/高级模型未完成|[本批验收](integration/artifacts/local/weighted-acceptance.json)|
|12.9.balancer-weighted2-equal-shared|WP12,WP13|PARTIAL_LOCAL|双向连续完整余额与共享物理池allocation候选金额重新计算|通用DAG、全局最优与实际交易次序未完成|[本批验收](integration/artifacts/local/weighted-acceptance.json)|
|12.14.balancer-weighted2-equal-reference|WP03,WP11,WP13|PARTIAL_LOCAL|固定40原文件与原构造器/onSwap/BalanceAllocation参考，765案例与45序列180步差分|合成RPC与Vault facade非真实部署/完整Vault/Token执行，独立执行未完成|[本批验收](integration/artifacts/local/weighted-acceptance.json)|
|12.11.balancer-weighted2-equal-execution-boundary|WP14,WP16|PARTIAL_LOCAL|新profile限制INDICATIVE，拒绝V2 calldata；17项进程检查完整scope与bank/generation|Weighted交易构造、独立EVM执行与真实收款未完成|[本批验收](integration/artifacts/local/weighted-acceptance.json)|
|11.3.balancer-weighted2-static-state|WP02,WP25|PARTIAL_LOCAL|独立static profile复用Weighted字段27，严格权重最小值/总和/adapter矩阵及非对称protoc往返|多币/高级模型未完成|[本批验收](integration/artifacts/local/weighted-static-acceptance.json)|
|7.3.balancer-weighted2-static-block|WP04,WP05,WP20|PARTIAL_LOCAL|同blockHash读取非对称权重并保留pool/Vault/token/cash/manager/receipt及重锚定屏障|部署与独立header仍未完成|[本批验收](integration/artifacts/local/weighted-static-acceptance.json)|
|12.2.balancer-weighted2-static-exact-in|WP11|PARTIAL_LOCAL|整数LogExpMath、原powUp分支和误差界、权重比向下舍入；净输入30%限制与完整uint112余额|ExactOut及高级模型未完成|[本批验收](integration/artifacts/local/weighted-static-acceptance.json)|
|12.9.balancer-weighted2-static-shared|WP12,WP13|PARTIAL_LOCAL|原onSwap的315组1260步完整后余额、同/反向共享池与allocation候选金额重算|通用DAG/全局最优/实际交易次序未完成|[本批验收](integration/artifacts/local/weighted-static-acceptance.json)|
|12.14.balancer-weighted2-static-reference|WP03,WP11,WP13|PARTIAL_LOCAL|5355原onSwap/BalanceAllocation案例与240原pow函数向量，新旧语料身份分离和冻结核对|非完整Vault/Token/部署证明|[本批验收](integration/artifacts/local/weighted-static-acceptance.json)|
|12.11.balancer-weighted2-static-execution-boundary|WP14,WP16|PARTIAL_LOCAL|新profile仅INDICATIVE、同物理池单版本与切换规则；拒绝V2 calldata|VERIFIED/build拒绝与18项双副本检查通过；真实交易执行未完成|[本批验收](integration/artifacts/local/weighted-static-acceptance.json)|
|11.3.multi-asset-physical-pool|WP02,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|新增field28独立多资产状态、Go深拷贝、完整余额向量与全token物理池路由边|原模型差分、端到端及多token共享池完整验收待完成|[限定子集验收](integration/artifacts/local/five-family-acceptance.json)|
|7.3.multi-asset-fixed-block|WP04,WP05,WP11|PARTIAL_LOCAL|多币/Managed零费用/Composable无rate provider读取全部token、权重、scales、cash、Vault与固定块屏障|全组高级状态及真实部署未完成；当前限定进程已验证|[限定子集验收](integration/artifacts/local/five-family-acceptance.json)|
|12.9.multi-asset-topology-and-capacity|WP12,WP13,WP24|PARTIAL_LOCAL|全token拓扑变化重建图、共享单物理状态、Weighted/Managed超额订单尝试小份额并可两路全额成交|通用DAG/三路以上拆单及全局最优未完成|[限定子集验收](integration/artifacts/local/five-family-acceptance.json)|
|12.2.balancer-multi-managed-composable-subsets|WP03,WP11,WP12,WP13|PARTIAL_LOCAL|3–20币Weighted、Managed当前权重与压缩插值/零BPT费；Composable regular和结费后BPT原数学|Managed非零费用供应更新、完整Composable callback/BPT路由/rate缓存仍未完成|[限定子集验收](integration/artifacts/local/five-family-acceptance.json)|
|12.2.curve-extra-model-subsets|WP03,WP11,WP12,WP13|PARTIAL_LOCAL|Plain3/4及Meta直接交换、NG独立舍入/dynamic fee/4626 rate，Crypto gamma/price/oracle/profit支持分支|Meta underlying/旧Lending、NG特殊Token/外部rate，Crypto调整/claim/gulp和Crypto正常解析路由未完成|[限定子集验收](integration/artifacts/local/five-family-acceptance.json)|
|11.3.solana-reward-state|WP02,WP20,WP25|PARTIAL_LOCAL|Orca/Raydium三槽奖励、tick增长tag20、state50/64与Go/Rust零哨兵/克隆/wire|Token2022/adaptive/orders及高级组合仍未完成|[限定子集验收](integration/artifacts/local/five-family-acceptance.json)|
|7.3.solana-reward-accounts|WP04,WP05,WP20|PARTIAL_LOCAL|实际packed账户奖励字段、经典SPL静态费新profile及同finalized读取|Yellowstone/历史重建及Token2022扩展未完成|[限定子集验收](integration/artifacts/local/five-family-acceptance.json)|
|12.2.solana-reward-poststates|WP03,WP11,WP13,WP20|PARTIAL_LOCAL|原native完整后Pool、奖励增长/跨tick/overflow/cap/end-time及连续兑换；双副本预检通过|奖励领取、SBF/Token程序及高级组合未完成|[限定子集验收](integration/artifacts/local/five-family-acceptance.json)|
|11.3.singleton-advanced-state|WP02,WP25|PARTIAL_LOCAL|field29独立key_fee/live费/currency/blockhash，外层快照同hash与配置/runtime能力一致|任意swap回调与自定义会计状态仍需执行|[限定子集验收](integration/artifacts/local/five-family-acceptance.json)|
|12.11.singleton-native-dynamic-subsets|WP03,WP04,WP05,WP11,WP14|PARTIAL_LOCAL|V4/Infinity动态费快照/native currency0、无swap回调hook权限/registration读取与逐步协议费|任意hook/return delta/覆盖费及native实际结算未完成|[限定子集验收](integration/artifacts/local/five-family-acceptance.json)|
|12.2.infinity-single-bin-kernel|WP03,WP11,WP14|PARTIAL_LOCAL|独立单活跃Bin价/储备/协议费舍入，与CL模型分离|本早期内核不注册Pool；后续独立Bin profile已完成manager/tree与跨Bin普通服务限定验收，完整执行仍未完成|[限定子集验收](integration/artifacts/local/five-family-acceptance.json)|
|12.14.five-family-bounded-reference|WP03,WP11,WP14,WP20|PARTIAL_LOCAL|固定上游源文件、原合约/native参考和能力拒绝，原始与业务金额/后状态区分；统一gate明确全部请求未完成|真实部署、独立完整交易/Token收款与五族全部能力未完成|[限定子集验收](integration/artifacts/local/five-family-acceptance.json)|

|11.3.curve-ng-full-state|WP02,WP25|LOCAL_SCOPE_VERIFIED|独立NG profile与field9完整状态；跨语言会计、oracle/来源约束和独立protoc|其他asset类型与更宽时间域|[NG限定范围验收](integration/artifacts/local/curve-ng-acceptance.json)|
|7.3.curve-ng-fixed-block|WP03,WP04,WP05,WP11|LOCAL_SCOPE_VERIFIED|原源码可执行前缀/immutable及同blockHash读取全部依赖、receipt与读后锚定|真实部署、独立可信区块头与特殊Token能力|[NG限定范围验收](integration/artifacts/local/curve-ng-acceptance.json)|
|12.2.curve-ng-full-exchange|WP03,WP11,WP13|LOCAL_SCOPE_VERIFIED|固定原exchange完整余额与packed价格/D/EMA、原signed exp舍入及连续共享池|rebasing/oracle/ERC4626服务状态及Token执行|[NG限定范围验收](integration/artifacts/local/curve-ng-acceptance.json)|
|12.9.curve-ng-capacity|WP11,WP12,WP13|LOCAL_SCOPE_VERIFIED|大额溢出后继续小额候选探测；全余额向量更新与共享池顺序重算|完整小图优化oracle及更大拆单|[NG限定范围验收](integration/artifacts/local/curve-ng-acceptance.json)|
|12.14.curve-ng-service-reference|WP01,WP02,WP03,WP09,WP11,WP16,WP25|LOCAL_SCOPE_VERIFIED|原合约147交换、56标准服务步骤；正常RPC/PG/Kafka双副本、故障恢复及旧流程回归通过|真实历史/生产部署及独立完整交易验证|[NG限定范围验收](integration/artifacts/local/curve-ng-acceptance.json)|

|11.3.managed-bpt-state|WP02,WP25|PARTIAL_LOCAL|field10完整BPT供应量、费用比例和收款方余额；别名、会计、wire/clone及源区块约束|仅参考状态契约；正常在线版本未准入|[限定原回调参考验收](integration/artifacts/local/managed-reference-acceptance.json)|
|7.3.managed-bpt-anchored-ledger|WP04,WP05,WP11|PARTIAL_LOCAL|同blockHash读取费用/cache、owner/collector和BPT余额的组件及异常/别名测试|未接正常parser；待可部署版本模板与完整依赖闭环|[限定原回调参考验收](integration/artifacts/local/managed-reference-acceptance.json)|
|12.2.managed-bpt-full-callback|WP03,WP11,WP13|PARTIAL_LOCAL|原powDown部分不变量、缩放前输出及重建gross输入、费用拆分铸币与完整后状态|以原回调参考为限；joins/exits/AUM收费和BPT路由未完成|[限定原回调参考验收](integration/artifacts/local/managed-reference-acceptance.json)|
|12.14.managed-bpt-original-reference|WP03,WP11|PARTIAL_LOCAL|原ManagedPool初始化和实际onSwap、357案例/30序列120步、mint/cash溢出回退|隔离参考环境显式放宽合约大小；真实部署/收款差分未完成|[限定原回调参考验收](integration/artifacts/local/managed-reference-acceptance.json)|
|12.11.managed-bpt-admission-boundary|WP01,WP02,WP04,WP05,WP11,WP16,WP25|PARTIAL_LOCAL|普通EVM部署失败与编译大小证据；配置/parser及公共quote保持拒绝，有限参考验收不开放服务|取得可部署真实版本后继续固定块/PG-Kafka双副本服务验收|[限定原回调参考验收](integration/artifacts/local/managed-reference-acceptance.json)|

|11.3.metausd-full-state|WP02,WP25|LOCAL_SCOPE_VERIFIED|独立field11保留实现/base依赖与VP、有效/raw/admin余额及源区块；wire/clone校验|特殊Token与其他Meta实际版本|[Meta限定范围验收](integration/artifacts/local/curve-meta-acceptance.json)|
|7.3.metausd-fixed-block|WP03,WP04,WP05,WP11|LOCAL_SCOPE_VERIFIED|精确实现模板/EIP1167、币序/精度/rate、baseVP及完整余额同blockHash读取|真实链可信来源与实际base部署审核|[Meta限定范围验收](integration/artifacts/local/curve-meta-acceptance.json)|
|12.2.metausd-direct-poststate|WP03,WP11,WP13|LOCAL_SCOPE_VERIFIED|固定原直接exchange、双向精确输出与raw/admin完整后状态；原生零输出明确拒绝报价|underlying/deposit/withdraw与完整Token交易|[Meta限定范围验收](integration/artifacts/local/curve-meta-acceptance.json)|
|12.9.metausd-readonly-base|WP02,WP09,WP12,WP13|LOCAL_SCOPE_VERIFIED|配置/快照/图/路径/分配入口拒绝可变base及冲突VP/依赖身份|能更新baseVP的联合多池依赖求解|[Meta限定范围验收](integration/artifacts/local/curve-meta-acceptance.json)|
|12.14.metausd-service-reference|WP01,WP03,WP09,WP11,WP16,WP25|LOCAL_SCOPE_VERIFIED|原初始化clone/32交换+8探针、独立protoc与正常服务与异常恢复测试通过|真实历史/部署共识与独立完整交易仿真|[Meta限定范围验收](integration/artifacts/local/curve-meta-acceptance.json)|

|11.3.crypto-complete-pricing-and-lp-state|WP02,WP25|LOCAL_SCOPE_VERIFIED|独立field12与profile，价格/fee/gamma/LP供应/receiver/raw账本、来源bank和跨语言wire/clone已实现|active A/gamma ramp的在线完整schedule和其他实际版本|[本批验证记录](integration/artifacts/local/priority-extensions-acceptance.json)|
|7.3.crypto-original-runtime-and-fixed-bank|WP03,WP04,WP05,WP11|LOCAL_SCOPE_VERIFIED|原pool模板及160-byte immutable、原V4 LP模板/minter、币序/精度、pricing与raw/supply/receiver同blockHash读取；20类准入测试|真实可信区块来源与实际部署目录；真实部署目录与可信历史块仍需单独验收|[本批验证记录](integration/artifacts/local/priority-extensions-acceptance.json)|
|12.2.crypto-adjustment-admin-mint-and-gulp|WP03,WP11,WP13|LOCAL_SCOPE_VERIFIED|原曲线newton/价格调整接受或拒绝、gulp/donation与原LP checked mint完整后状态，60原案例/44成功/16回退；settled正常报价已接|其他Crypto模型/特殊Token和完整执行|[本批验证记录](integration/artifacts/local/priority-extensions-acceptance.json)|
|12.14.crypto-service-and-source-evidence|WP01,WP03,WP09,WP11,WP16,WP25|LOCAL_SCOPE_VERIFIED|独立protoc、原Pool+原LP正常EIP170部署参考、30正常分派案例、来源及故障工具已实现|真实链部署和独立完整交易执行仍未完成|[本批验证记录](integration/artifacts/local/priority-extensions-acceptance.json)|
|12.9.crypto-admin-lp-mutation-boundary|WP02,WP05,WP09,WP12,WP13,WP25|LOCAL_SCOPE_VERIFIED|Go/Rust快照、图、直接路径与allocation统一拒绝管理员LP mint隐式改变另一候选池raw余额；保留外部receiver及零admin fee可用性；已保留先失败后通过反例|跨物理池LP mint后状态传播仍未建模，保持显式拒绝|[本批验证记录](integration/artifacts/local/priority-extensions-acceptance.json)|
|12.2.orca-adaptive-fee-state-manager|WP02,WP03,WP05,WP11,WP20,WP25|PARTIAL_LOCAL|固定cc683ec9逐group fee/volatility/filter-decay/cap/skip/major timestamp，197原native案例、14连续交易及1215状态快照；完整保留reserved|完整adaptive exact-in数学另列；Oracle账户解析和限定在线profile已在后续服务批次完成；初始化奖励/dynamic arrays、Token2022、SBF交易未完成|[本批验证记录](integration/artifacts/local/priority-extensions-acceptance.json)|
|12.2.orca-adaptive-full-swap-poststate|WP02,WP03,WP05,WP11,WP13,WP20,WP25|PARTIAL_LOCAL|经典SPL/fixed-array/未初始化reward的完整exact-in：逐group费率、Q64步、tick穿越/fee growth/protocol owed、完整Oracle和Pool后状态；101原成功/2失败/两序列12交易；显式partial fill返回remaining|initialized Oracle实际账户解析/限定在线profile已在后续服务批次完成；初始化奖励/dynamic array/Token2022与SBF执行未完成；要求全成交的调用方须验证remaining为0|[本批验证记录](integration/artifacts/local/priority-extensions-acceptance.json)|
|12.2.infinity-complete-initialized-bin-traversal|WP02,WP03,WP05,WP11,WP14|PARTIAL_LOCAL|固定3c5ef1be原BinPool与TreeMath，稀疏前驱/后继、多bin费/储备/active-id完整后状态及原子失败；326原EVM案例|本独立数学API的完整树仍由调用方声明；后续独立Bin profile已补固定块来源与在线准入，hook/override/native完整结算未完成|[本批验证记录](integration/artifacts/local/priority-extensions-acceptance.json)|

|11.3.orca-adaptive-complete-oracle|WP02,WP25|LOCAL_SCOPE_VERIFIED|独立profile与field51、flat constants/variables/16/16/128保留字段、time/width/组合和源bank校验、跨语言clone/wire|真实部署目录、兼容演进与特殊Token|[本批验收](integration/artifacts/local/orca-adaptive-acceptance.json)|
|7.3.orca-adaptive-finalized-bank|WP03,WP04,WP05,WP20|LOCAL_SCOPE_VERIFIED|实际254-byte Oracle、fee tier pool PDA、program/code/Clock/币序/vault/arrays/保留字段同finalized bank读取和重锚，旧profile仍拒绝初始化Oracle|真实validator流/历史bank及部署代码核验|[本批验收](integration/artifacts/local/orca-adaptive-acceptance.json)|
|12.2.orca-adaptive-online-poststate|WP03,WP11,WP13,WP20|LOCAL_SCOPE_VERIFIED|普通分派复用完整adaptive内核、全额/非零输出及rawvault宽度检查、fee/tick/Oracle完整后状态；原pack20 bank16成功4拒绝|初始化奖励/dynamic arrays/Token2022、真实CPI/收款和完整SBF交易|[本批验收](integration/artifacts/local/orca-adaptive-acceptance.json)|
|12.9.orca-adaptive-shared-pool|WP11,WP12,WP13|LOCAL_SCOPE_VERIFIED|多路分配复用同物理池Oracle/fee/tick/raw后状态；重复时间按原state继续|通用DAG、跨协议全局最优及执行成本|[本批验收](integration/artifacts/local/orca-adaptive-acceptance.json)|
|12.14.orca-adaptive-normal-service-reference|WP01,WP03,WP09,WP11,WP14,WP16,WP20,WP25|LOCAL_SCOPE_VERIFIED|独立原program pack/manager与postupdate、真实PDA、四连续序列/20bank、16native成交4失败及26篡改拒绝、独立protoc|真实部署/可信历史、SBF/CPI与完整交易验收未完成|[本批验收](integration/artifacts/local/orca-adaptive-acceptance.json)|

|11.3.infinity-bin-complete-state|WP02,WP25|LOCAL_SCOPE_VERIFIED|独立Bin profile和field30，完整树/储备/share、manager/vault/token身份、预算与源bank；独立protoc、clone与跨语言校验|真实部署目录/兼容演进和其他Bin版本|[本批验收](integration/artifacts/local/infinity-bin-acceptance.json)|
|7.3.infinity-bin-original-manager-tree|WP03,WP04,WP05,WP11|LOCAL_SCOPE_VERIFIED|原manager23852/Vault9247模板与immutable核验、key/pause/registration/token、完整树层级与bin储备/share/getNext双向校验，同blockHash读后锚定|实际可信链历史、自动发现/长期raw与生产来源|[本批验收](integration/artifacts/local/infinity-bin-acceptance.json)|
|12.2.infinity-bin-normal-poststate|WP03,WP11,WP13|LOCAL_SCOPE_VERIFIED|普通报价复用原独立跨Bin内核，完整bin/active/费用与app余额后状态；全额正输出、宽度和预算失败封闭|动态hook/覆盖费/native及真实完整管理器结算|[本批验收](integration/artifacts/local/infinity-bin-acceptance.json)|
|12.9.infinity-bin-shared-manager-currencies|WP02,WP05,WP09,WP12,WP13,WP25|LOCAL_SCOPE_VERIFIED|所有snapshot/graph/path/allocation检查共享manager/currency依赖一致；每步传播app reserves和protocol fees，保留各物理池独立树与储备|更复杂跨协议执行/成本和生产规模验收|[本批验收](integration/artifacts/local/infinity-bin-acceptance.json)|
|12.2.infinity-bin-detached-active-burn|WP02,WP03,WP05,WP11,WP25|LOCAL_SCOPE_VERIFIED|显式tree外active锁定1000share/dust状态；初始active先计算，后续只遍历实际tree；不把已移除节点放回树；maxBinStep不当作既有池交易开关|原burn独立参考16场景48交易与来源复验通过；完整manager settlement仍未验证|[本批验收](integration/artifacts/local/infinity-bin-acceptance.json)|
|12.14.infinity-bin-normal-service-reference|WP01,WP03,WP09,WP11,WP14,WP16,WP25|LOCAL_SCOPE_VERIFIED|原manager/Vault正常部署getter、原BinPool/TreeMath独立交换、五原bank/四双向连续交易和独立protobuf|真实部署/历史及完整manager/Vault结算未验证|[本批验收](integration/artifacts/local/infinity-bin-acceptance.json)|

|11.3.crypto-ramp-complete-schedule|WP02,WP03,WP25|LOCAL_SCOPE_VERIFIED|独立版本与field26、initial A/gamma/time、clone/wire/起点域和旧profile拒绝已补|真实部署/历史与其他Crypto模型|[本批验收](integration/artifacts/local/curve-crypto-ramp-acceptance.json)|
|7.3.crypto-ramp-fixed-bank|WP03,WP04,WP05,WP09|LOCAL_SCOPE_VERIFIED|同blockHash读取slot5–8及原模板/LP/币序/储备，定向Go解析/config通过|真实链历史/部署身份与完整provider审核|[本批验收](integration/artifacts/local/curve-crypto-ramp-acceptance.json)|
|12.2.crypto-ramp-normal-poststate|WP03,WP11,WP13|LOCAL_SCOPE_VERIFIED|普通分派传入原起点及future端点，完整交换/价格/LP/raw后状态|其他模型、特殊Token及完整交易执行|[本批验收](integration/artifacts/local/curve-crypto-ramp-acceptance.json)|
|12.9.crypto-ramp-carried-sequences|WP11,WP12,WP13,WP25|LOCAL_SCOPE_VERIFIED|升降/到期/停止6序列28原交易、22连续后状态，保留future端点和起点直到标记清零后|通用DAG/全局优化与真实执行成本|[本批验收](integration/artifacts/local/curve-crypto-ramp-acceptance.json)|
|12.14.crypto-ramp-normal-service-reference|WP01,WP03,WP09,WP11,WP14,WP16,WP25|LOCAL_SCOPE_VERIFIED|原池/原LP正常EIP170部署、8公开ramp/stop调用、原交易收款与完整后状态及依赖闭包|已完成双副本/当前构建与输入绑定；真实部署/历史与独立完整交易仍缺|[本批验收](integration/artifacts/local/curve-crypto-ramp-acceptance.json)|
|23.optimized-reference-rejection-guards|WP25|LOCAL_SCOPE_VERIFIED|旧dynamic/seed验收78处显式拒绝，24项普通/优化回归通过，新增子进程优化模式防回归|普通/优化全量及两阶段审查通过；旧协议原参考不在此重验|[本批验收](integration/artifacts/local/curve-crypto-ramp-acceptance.json)|
|7.3.live-head-clock-admission|WP05,WP09,WP25|LOCAL_SCOPE_VERIFIED|在线头与提交前统一CheckFresh，未来/过期头读取前拒绝，保持旧引用并由Tick发布撤销|未来头进程拒绝/恢复与完整回归通过；真实provider/可信头与共识|[本批验收](integration/artifacts/local/curve-crypto-ramp-acceptance.json)|

|11.3.orca-adaptive-rewards-state|WP02,WP03,WP25|LOCAL_SCOPE_VERIFIED|独立版本、完整Oracle/奖励slot/tick状态、跨语言wire与旧版本隔离已补|真实部署/历史与更多账户/程序版本|[本批验收](integration/artifacts/local/orca-adaptive-rewards-acceptance.json)|
|7.3.orca-adaptive-rewards-bank|WP03,WP04,WP05,WP09,WP20|LOCAL_SCOPE_VERIFIED|原account pack20bank/16成交4拒绝，fixed arrays与初始化奖励状态来源已补|正常RPC/13来源故障及恢复已通过；真实validator流/历史仍缺|[本批验收](integration/artifacts/local/orca-adaptive-rewards-acceptance.json)|
|12.2.orca-adaptive-rewards-poststate|WP03,WP11,WP13,WP20|LOCAL_SCOPE_VERIFIED|普通分派、checked奖励乘法溢出归零、wrapping global/outside增长与完整价格/fee/Oracle/vault后状态已补|其他高级组合、特殊Token和完整交易执行|[本批验收](integration/artifacts/local/orca-adaptive-rewards-acceptance.json)|
|12.9.orca-adaptive-rewards-shared-pool|WP11,WP12,WP13|LOCAL_SCOPE_VERIFIED|四组共享物理池的两次原连续交易与allocation完整复用奖励/Oracle/tick后状态通过定向验证|通用DAG、实际SBF/CPI执行成本|[本批验收](integration/artifacts/local/orca-adaptive-rewards-acceptance.json)|
|12.14.orca-adaptive-rewards-service-reference|WP01,WP03,WP09,WP11,WP14,WP16,WP20,WP25|LOCAL_SCOPE_VERIFIED|原程序pack/manager/postupdate参考、独立protoc与34普通/优化篡改反例已补|双副本/两阶段审查及当前输入绑定通过；真实部署/SBF/CPI仍缺|[本批验收](integration/artifacts/local/orca-adaptive-rewards-acceptance.json)|

|11.3.orca-adaptive-dynamic-state|WP02,WP03,WP25|LOCAL_SCOPE_VERIFIED|独立动态组合profile、layout/bitmap/奖励/Oracle完整契约已验证|其他账户/程序版本、Token2022/完整交易|[本批验收](integration/artifacts/local/orca-adaptive-dynamic-acceptance.json)|
|7.3.orca-adaptive-dynamic-bank|WP03,WP04,WP05,WP09,WP20|LOCAL_SCOPE_VERIFIED|原compact动态字节及mixed来源，长度/bitmap/flag反例已准备|正常RPC/16来源故障恢复已通过；真实validator流仍缺|[本批验收](integration/artifacts/local/orca-adaptive-dynamic-acceptance.json)|
|12.2.orca-adaptive-dynamic-poststate|WP03,WP11,WP13,WP20|LOCAL_SCOPE_VERIFIED|全动态与mixed数组内核/普通分派及原参考差分通过|补充/稀疏数组、Token2022、领取与完整执行|[本批验收](integration/artifacts/local/orca-adaptive-dynamic-acceptance.json)|
|12.9.orca-adaptive-dynamic-shared-pool|WP11,WP12,WP13|LOCAL_SCOPE_VERIFIED|四组原连续两步与shared allocation完整后状态验证通过|实际选中计划39原生步骤通过；通用DAG/真实成本仍缺|[本批验收](integration/artifacts/local/orca-adaptive-dynamic-acceptance.json)|
|12.14.orca-adaptive-dynamic-reference|WP01,WP03,WP09,WP11,WP14,WP16,WP20,WP25|LOCAL_SCOPE_VERIFIED|原DynamicTick::serialize与load_tick_array_mut，20bank/16成功4拒绝、44篡改反例和4013byte独立wire已通过|完整进程/当前构建和两阶段审查通过；真实SBF/CPI/历史仍缺|[本批验收](integration/artifacts/local/orca-adaptive-dynamic-acceptance.json)|

|14.1.evm-bound-transaction|WP14,WP15,WP25|LOCAL_SCOPE_VERIFIED|BOUND单hop标准V2计划、完整scope/池代码/执行器身份与精确calldata/金额/expiry校验|其他协议、多hop/拆单DAG、授权wrap/refund与真实部署身份审计|[本批限定验收](integration/artifacts/local/evm-execution-acceptance.json)|
|14.2.evm-strict-local-state|WP15,WP25|LOCAL_SCOPE_VERIFIED|严格显式账户/code/storage/blockhash、Shanghai实际REVM执行及H源块/H+1环境绑定；未知状态拒绝|可信链按blockHash读取、版本化缓存、多链hardfork/系统合约和SBF/CPI|[本批限定验收](integration/artifacts/local/evm-execution-acceptance.json)|
|14.3.evm-real-balance-deltas|WP14,WP15,WP25|LOCAL_SCOPE_VERIFIED|原V2十笔实际交易的输入/收款余额差、gas/nonce及全声明后状态独立差分；收款税和捐赠反例|完整链费用、其他Token/协议/多路交易及真实链差分|[本批限定验收](integration/artifacts/local/evm-execution-acceptance.json)|
|14.4.evm-bounded-worker|WP15,WP16,WP25|LOCAL_SCOPE_VERIFIED|同dex-router二进制隔离worker、独立并发、gas/read/byte/input/instruction/memory预算与协作取消/硬超时回收|租户预算/生产CPU内存隔离与压力故障验收|[本批限定验收](integration/artifacts/local/evm-execution-acceptance.json)|
|14.5.evm-execution-evidence|WP14,WP15,WP16,WP25|LOCAL_SCOPE_VERIFIED|认证simulate API固定缓存上下文、不可变本地状态、前后新鲜度和独立执行证据；保持verified=false|受限证据访问/脱敏/retention和具备可信完整数据的VERIFIED/build闭环|[本批限定验收](integration/artifacts/local/evm-execution-acceptance.json)|

|11.3.v2-complete-state|WP02,WP10,WP11|LOCAL_SCOPE_VERIFIED|独立profile/field20、raw余额/储备时间戳/累计价/锁及已知零presence已实现并定向通过，旧版本拒绝扩展|独立wire/最终回归/双审已通过；真实部署/历史另列|[限定验收](integration/artifacts/local/v2-complete-acceptance.json)|
|7.3.v2-complete-fixed-bank|WP03,WP04,WP05,WP10,WP11|LOCAL_SCOPE_VERIFIED|Pair/Factory原代码、双向getPair、raw/累计价/slot12与固定块读取；24原bank/70前后投影和故障定向通过|普通服务及genesis零parent/秒转ms溢出拒绝已通过；可信真实链来源仍缺|[限定验收](integration/artifacts/local/v2-complete-acceptance.json)|
|12.2.v2-complete-settlement|WP10,WP11|LOCAL_SCOPE_VERIFIED|完整双侧amountIn/K/SafeMath/u112、raw/储备与显式时钟的uint32/累计价wrap；20原交换/4原回退及17普通分派定向通过|native再生/全量/双审已绑定通过；不含Mint/Burn/税rebase|[限定验收](integration/artifacts/local/v2-complete-acceptance.json)|
|12.9.v2-complete-shared-state|WP11,WP12,WP13|LOCAL_SCOPE_VERIFIED|两组同订单共享allocation、8步当前clock路径连续复用完整原状态已定向通过|完整DAG可执行calldata与跨协议执行仍未完成|[限定验收](integration/artifacts/local/v2-complete-acceptance.json)|
|14.v2-complete-independent-execution|WP10,WP11,WP14,WP15,WP25|LOCAL_SCOPE_VERIFIED|新版本精确BOUND及独立raw/时间戳/累计价/计量slot12核对、真实overlay后观察已定向通过；24原VM交易273账户/1525槽一致|普通API/隔离进程与双审已通过；可信链和公开VERIFIED/build仍未完成|[限定验收](integration/artifacts/local/v2-complete-acceptance.json)|
|12.14.v2-complete-original-bank|WP03,WP10,WP11,WP13,WP25|LOCAL_SCOPE_VERIFIED|原发布合约公开操作的24case/4序列10步、84交易/79原trace已生成，普通/优化精确再生与49篡改拒绝定向通过|完整源码/五binary/全闭包/双审及进程gate通过；真实部署/历史仍缺|[限定验收](integration/artifacts/local/v2-complete-acceptance.json)|

|11.3.managed-v2-published-state|WP02,WP04,WP05,WP11,WP25|PARTIAL_LOCAL|实际发布版独立状态/profile/config、sourceH、JSON/wire/clone与uint32原storage边界已接入；四副本独立C++ 2895字节通过|派生execution_context与canonical隔离、实际Vault钱包余额完整保留正在补齐；最终冻结与双审未完成|wire初验（`integration/artifacts/local/managed-v2-aum-wire-check.log`，已清理）|
|7.3.managed-v2-fixed-bank|WP03,WP04,WP05,WP11|PARTIAL_LOCAL|固定blockHash/原runtime及links/immutables、真实Factory/Provider/Vault/collector getter与完整供应量/AUM核对；48原snapshot/81投影对照已接入|更多真实链版本/来源证明、实际余额完整保留及最终冻结未完成|原bank精确再生（`integration/artifacts/local/managed-v2-aum-reference-check.log`，已清理）|
|12.14.managed-v2-original-replay|WP03,WP11,WP13,WP25|PARTIAL_LOCAL|正常EIP170原部署27case/292交易/2285对象，原swap与非零AUM/协议分成/零重复/uint32回退；81类原篡改普通与优化模式拒绝通过|普通服务只覆盖当前日期双币样本；其他未来clock状态仍为本地parser/math参考，不能放宽未来头准入|普通保护（`integration/artifacts/local/managed-v2-aum-guards.log`，已清理）<br>优化保护（`integration/artifacts/local/managed-v2-aum-guards-optimized.log`，已清理）|
|12.9.managed-v2-derived-context|WP02,WP11,WP12,WP13|IN_PROGRESS|固定H共享池连续重算通过；独立SPEC发现未来time同clock下一步拒绝及高度回卷后完整context丢失|显式派生执行高度/时间、sourceH不变、canonical/wire拒绝派生状态正在红绿补齐；不能据旧通过宣称完整连续重算|[本批任务](#plan-2026-11-03-managed-v2-aum)|
|23.managed-v2-normal-service|WP02,WP03,WP05,WP11,WP12,WP13,WP25|IN_PROGRESS|正常RPC/PG/Kafka双router首轮6原来源状态/4原swap金额/10来源故障撤销恢复/重启与VERIFIED-build拒绝共15检查通过，live C++ wire一致|后续源码变更使首轮为历史记录；完整钱包和派生上下文收口后需最终冻结全量/独立双审，原生选中计划VM重放未完成|[首轮服务记录](integration/artifacts/local/managed-v2-aum-three-service-smoke.json)|
|12.11.managed-v2-distinct-pause|WP02,WP03,WP04,WP05,WP11,WP13,WP25|IN_PROGRESS|原实际Vault暂停交易后Pool公开AUM collection仍成功，证实单一OR标志不符合原合约；按Pool/Vault各自有效flag和窗口独立建模，新增公开状态版本3|原暂停三case、固定块解析/数学完整后状态与正常服务边界正在补齐，最终同源全检和独立双审尚未完成|原执行发现（`integration/artifacts/local/managed-v2-aum-vault-pause-native-probe.log`，已清理）|

|7.3.managed-v2-current-catalog|WP02,WP03,WP04,WP05,WP11,WP25|LOCAL_SCOPE_VERIFIED|[新范围最终同源验收](integration/artifacts/local/managed-v2-multi-acceptance.json)；真实2026-01-01/01-31的2/3/5/50币四池共同H目录、73代码身份/9800原getter、原4swap/4非零AUM，Go40完整Pool与Rust8原后状态通过；普通双副本10bank/14计划通过|新冻结全检/质量/完整门禁已通过；本地参考不提供真实链trust|[原目录](integration/test/contracts/managed-v2-aum-catalog.json)<br>[四池进程](integration/artifacts/local/managed-v2-catalog-three-service-smoke.json)|
|11.3.managed-v2-shared-vault-facts|WP02,WP03,WP05,WP11|LOCAL_SCOPE_VERIFIED|[新范围最终同源验收](integration/artifacts/local/managed-v2-multi-acceptance.json)；同sourceH角色/代码/参数/Provider外部费率事实、全局钱包与known注册cash合计/重复冲突校验，Go及两Rust公共domain统一；knownunsupported与事实冲突区别，完整银行保全|新范围独立质量与完整冻结验收已通过；未知Vault注册不冒称全目录覆盖|[共享原银行](integration/test/contracts/managed-v2-shared-vault.json)<br>红绿回归（`integration/artifacts/local/managed-v2-shared-role-cash-go.log`，已清理）|
|12.9.managed-v2-shared-wallet-transition|WP02,WP03,WP11,WP12,WP13|LOCAL_SCOPE_VERIFIED|[新范围最终同源验收](integration/artifacts/local/managed-v2-multi-acceptance.json)；原4次A/B/A/B交易两Pool完整后状态、共享global钱包传播/registered账本保留、Somectx与sourceH、整path失败原子回滚、capacity探测与整数optimizer回归通过|普通BPT/join/exit/rate/mixed能力和通用collection传播仍未实现；生产性能另验|路由原生回归（`integration/artifacts/local/managed-v2-shared-funded-scope-regressions.log`，已清理）|
|13.managed-v2-executable-scope|WP02,WP09,WP11,WP12,WP13,WP16|LOCAL_SCOPE_VERIFIED|[新范围最终同源验收](integration/artifacts/local/managed-v2-multi-acceptance.json)；Graph/API显式排除不支持的相关Vault组并保留完整来源ScopeHash，无关V2及weighted-only可报价；Snapshot/Replica/HTTP13回归通过|新范围全量/独立质量门禁已通过；不将scope限制改写为原设计功能删除|范围回归（`integration/artifacts/local/managed-v2-shared-funded-scope-regressions.log`，已清理）|
|12.14.managed-v2-real-selected-split|WP03,WP11,WP12,WP13,WP14,WP25|LOCAL_SCOPE_VERIFIED|[新范围最终同源验收](integration/artifacts/local/managed-v2-multi-acceptance.json)；原signed history恢复source后按真实API路线逐步原Vault.swap，两个actor真实选择2物理池、输入5e21/收款7874328412914279650444，与报价一致；7source/16实际计划/4故障恢复进程通过|新冻结全检/质量/完整门禁已通过；仍无trusted live chain/public executor/VERIFIED-build|[共享真实进程](integration/artifacts/local/managed-v2-shared-three-service-smoke.json)|

|11.3.managed-v2-scheduled-state|WP02,WP11,WP25|LOCAL_SCOPE_VERIFIED|独立scheduled profile及field40、纯整数raw decoder与canonical/derived隔离；独立C++四副本wire通过，ONE±2原getter保持未归一化|M1–M4多物理池原组合、更多token/能力矩阵及完整工作包验收未完成|field40 wire（`integration/artifacts/local/managed-v2-scheduled-wire-check.log`，已清理）|
|7.3.managed-v2-scheduled-source|WP03,WP04,WP05,WP11,WP25|LOCAL_SCOPE_VERIFIED|234原VM状态固定H完整RPC/双Rust域/完整field40 wire及8来源/4构造ID联合篡改拒绝；来源代码/全部immutable/Factory/WETH/必需getter/身份唯一已审查修复|真实链trust、多物理池活动schedule原source及原设计完整验收仍缺|当前原来源与构造拒绝回归（`integration/artifacts/local/managed-v2-scheduled-parser-test.log`，已清理）|
|12.9.managed-v2-scheduled-transition|WP11,WP12,WP13|LOCAL_SCOPE_VERIFIED|234时钟视图/41检查点、53原swap实际收款及全部后状态、2非零AUM collection后状态精确匹配；未来共享Vault clock定向通过|M1–M4多物理池原VM组合及完整能力共享DAG仍未完成|当前原数学与完整后状态（`integration/artifacts/local/managed-v2-scheduled-math-test.log`，已清理）|
|12.14.managed-v2-scheduled-original|WP03,WP11,WP13,WP25|LOCAL_SCOPE_VERIFIED|原53swap/234state/41checkpoint的四独立副本与单物理池55来源/127实际API、53不同原签名交易VM全部真实余额/后观察匹配；干净目录仅注册输入精确再生通过|多物理池活动schedule原source/组合执行/拆单及原26工作包全验收仍未完成|当前干净目录原VM再生（`integration/artifacts/local/managed-v2-scheduled-reference-check.log`，已清理）|

|7.3.managed-v2-scheduled-shared-source|WP02,WP03,WP04,WP05,WP09,WP11,WP25|LOCAL_SCOPE_VERIFIED|两真实三币池mixed/active共同H原观察、72bank/144Pool固定块Go与双Rust域/完整wire已定向通过；来源四类审查问题已修复关闭|更多token/能力/一般参数及真实链trust、完整设计验收仍缺|[当前双物理池限定验证](integration/artifacts/local/managed-v2-scheduled-shared-local-validation.json)|
|12.9.managed-v2-scheduled-shared-original-transitions|WP11,WP12,WP13|LOCAL_SCOPE_VERIFIED|原48swap/4collection双方完整后状态、12连续序列和12 clock-only双方独立view/AUM差分通过|全部能力/参数组合、完整共享DAG与原26WP验收未完成|[当前双物理池限定验证](integration/artifacts/local/managed-v2-scheduled-shared-local-validation.json)|
|12.14.managed-v2-scheduled-shared-selected-split|WP03,WP11,WP12,WP13,WP14,WP25|LOCAL_SCOPE_VERIFIED|52真实API报价关联12不同计划/24新签名原Vault swap，真实扣款/收款及双方完整after匹配；实际大额24步本地连续模型逐字段差分由M4强制执行通过，snapshot/revert完整source恢复|本地顺序参考不证明生产原子执行；BPT/rates/复杂DAG、真实链及public VERIFIED/build仍未完成|[当前双物理池限定验证](integration/artifacts/local/managed-v2-scheduled-shared-local-validation.json)|
|23.managed-v2-scheduled-shared-process|WP04,WP05,WP09,WP11,WP12,WP13,WP16,WP25|LOCAL_SCOPE_VERIFIED|最终72共同来源/CPP、双副本52两路报价、12来源故障撤销恢复/2重启和准入拒绝通过；五binary/完整check/来源控制/实际原VM与大额完整状态差分在新冻结一致|完整生产TLS/故障域/容量/SLO、真实链共识与全设计验收未完成|[当前双物理池限定验证](integration/artifacts/local/managed-v2-scheduled-shared-local-validation.json)|

|11.3.managed-v2-bpt-state|WP02,WP11,WP25|LOCAL_SCOPE_VERIFIED|独立BPT profile/field41、必需控制/成员Unknown、Go/两Rust及C++完整wire/旧版本隔离通过最后同freeze；范围见本批最终验收|一般参数/全部2–50 BPT组合、其他版本/能力及完整WP仍未验收|[状态测试](dex-router/crates/domain/tests/managed_v2_bpt_state.rs)|
|7.3.managed-v2-bpt-source|WP03,WP04,WP05,WP11,WP25|LOCAL_SCOPE_VERIFIED|原145bank/286可支持Pool固定H完整RPC差分、slot9/10/11/13、8来源矛盾拒绝及合成recovery有效零边界通过；实际141共同scope bank/CPP同freeze|真实链trust、enabled breaker/rate/mixed及完整生产来源仍未完成|[来源测试](chain-parser-go/internal/chain/evm/managed_v2_bpt_native_test.go)|
|12.2.managed-v2-bpt-transition|WP11,WP13,WP25|LOCAL_SCOPE_VERIFIED|原100成功/14回退金额及整个保留后Pool，AUM先收取与预铸BPT转账、别名/发送方权限/普通分派通过最后同freeze|其他状态组合、ExactOut、真实链/Token行为与完整设计仍未完成|[完整转换测试](dex-router/crates/protocol-math/tests/managed_v2_bpt_pool.rs)|
|12.9.managed-v2-bpt-routing|WP11,WP12,WP13,WP14|LOCAL_SCOPE_VERIFIED|96原正输出逻辑路径、唯一物理Pool/Funds贯穿、连续/拆单重算及整path原子性通过；实际2物理Managed池两路4步原VM完整after同freeze；跨V2钱包仅单位回归|更广custody/自收付/跨协议执行/复杂DAG及全局最优未完成|[路径测试](dex-router/crates/routing/tests/managed_v2_bpt.rs)|
|12.14.managed-v2-bpt-selected|WP03,WP11,WP12,WP13,WP14,WP25|LOCAL_SCOPE_VERIFIED|250实际API输入精确关联97不同计划/100原签名交易，真实收付款/receipt/immutable及slot9/10/11/control绑定、逐步双方完整Pool与source恢复同freeze通过|本地顺序原参考不等于生产原子执行；可信链、完整Gas/VERIFIED/build与更广组合未完成|[原计划重放](integration/tools/ci/managed-v2-bpt-plan-replay.cjs)|
|23.managed-v2-bpt-process|WP04,WP05,WP09,WP11,WP12,WP13,WP16,WP25|LOCAL_SCOPE_VERIFIED|141共同来源/CPP、双副本250成功报价/30精确业务拒绝、24来源故障撤销恢复/4重启、actual-plan VM与100完整后状态、8命令全检及SPEC→quality同freeze通过|历史本地同链双endpoint，不提供独立provider信任；生产故障域/性能/完整WP未完成|[进程工具](integration/tools/acceptance/managed_v2_bpt_smoke.py)|

|11.3.managed-v2-breaker-state|WP02,WP11,WP25|LOCAL_SCOPE_VERIFIED|field42有序raw word+6getter、required/clone/known0/旧profile隔离与600Pool跨语言/CPP wire及同freeze完整check通过|本版有限GIVEN_IN范围；rate/mixed/其他版本与全部能力组合仍未完成|[状态测试](dex-router/crates/domain/tests/managed_v2_circuit_state.rs)|
|7.3.managed-v2-breaker-source|WP03,WP04,WP05,WP11,WP25|LOCAL_SCOPE_VERIFIED|300原bank/600Pool fixedH解析、slot12/linked library purecall/6getter、来源矛盾拒绝及四mode真实source进程通过|可信真实链trust/共识与持续采集完整性仍未验收|[原source测试](chain-parser-go/internal/chain/evm/managed_v2_breaker_native_test.go)|
|12.2.managed-v2-breaker-transition|WP11,WP13,WP25|LOCAL_SCOPE_VERIFIED|244原case金额/拒绝原因/整个Pool、clock/dust/恢复金额/futureAUM回归及230实际API计划233原交易逐步完整后状态通过|其他会计/一般参数组合、ExactOut与真实Token执行仍未完成|[完整后状态](dex-router/crates/protocol-math/tests/managed_v2_breaker_pool.rs)|
|12.9.managed-v2-breaker-routing|WP11,WP12,WP13,WP14|LOCAL_SCOPE_VERIFIED|516实际成功API、可恢复候选/整path回滚/共享钱包及实际两路四步容量拆单与原VM完整后状态通过|通用多路DAG/净成本/真实链与公开执行仍未完成|[路由测试](dex-router/crates/routing/tests/managed_v2_breaker.rs)|
|12.14.managed-v2-breaker-reference|WP03,WP11,WP13,WP25|LOCAL_SCOPE_VERIFIED|fresh原300bank244case的98,268,052字节逐字节一致；全部postinit签名交易/clock/账户双向归属、实际API原计划重放与独立SPEC/quality通过|原VM有限本地参考；可信部署/真实历史链状态、完整链规则与生产执行仍未验收|[原参考](integration/tools/reference/balancer-managed-v2-breaker/checker.cjs)|
|23.managed-v2-breaker-process|WP04,WP05,WP09,WP11,WP12,WP13,WP14,WP16,WP25|LOCAL_SCOPE_VERIFIED|四mode合计300来源/516成功API/30精确拒绝/24来源故障恢复/4重启、230实际原计划233交易与完整后状态、44+17双模式/全检/worker/结构摘要及最终audit通过|本地限定scope已闭合；真实链/生产准入/公开VERIFIED与build及原完整工作包仍未验收|[进程工具](integration/tools/acceptance/managed_v2_breaker_smoke.py)；[限定最终验收](integration/artifacts/local/managed-v2-breaker-local-acceptance.json)|

|7.1.r01-deployment-evidence|WP02,WP03,WP10,WP11,WP18,WP20|IN_PROGRESS|头/交易根/账户MPT/代码/storage验证与固定H只读采集、真实主网单版本样本通过|全部版本部署/升级目录、可信共识与来源独立性尚缺|[定向验证](chain-parser-go/internal/provenance/evm_test.go)|
|12.14.r01-deployed-source-rebuild|WP02,WP03,WP10,WP11,WP18,WP20|IN_PROGRESS|固定solc/10源文件/完整settings与实际runtime一致，补部署证明绑定反例|其他版本源码重建、真实历史独立执行/收款尚缺|[定向验证](integration/tools/acceptance/tests/test_evm_deployment_rebuild.py)|
|7.1.r02-provider-ws|WP04,WP05,WP07|IN_PROGRESS|能力实测/错误分类限流共享预算、WS提示/有限重连与HTTP事实分离已实现|normal服务已接显式WS预算/通知唤醒与HTTPpoll兜底；真实provider/独立故障域矩阵未验收|[定向验证](chain-parser-go/internal/rpc/policy_test.go)|
|7.2.r02-staging-commit|WP04,WP05,WP07|IN_PROGRESS|有界并行/顺序actor/父hash/失败与取消及模糊提交恢复回归通过|normal适配器两块有界窗口/原prepared-journal actor已接线；真实链/专有可信完整性与进程全验仍缺|[定向验证](chain-parser-go/internal/ingest/pipeline_test.go)|
|7.3.r02-raw-dlq-discovery|WP04,WP05,WP07|IN_PROGRESS|耐久raw/DLQ、正常RPC先归档后decode已接线；工厂候选/当前准入基础已实现|持续归档保留/解除隔离重放及真实工厂发现进程未验收|[定向验证](chain-parser-go/internal/ingest/archive_test.go)|
|7.r03-yellowstone-wal|WP04,WP05,WP07,WP20,WP24|IN_PROGRESS|固定上游proto、实际gRPCloopback与独立帧WAL/重放基础通过|固定单writer及绝对write/idle期限补修已测；真实Yellowstone源/断线连续性证据缺失|[定向验证](chain-parser-rust/crates/solana-ingest/tests/stream_wal.rs)|
|28.22.r03-stream-bank-barrier|WP04,WP05,WP07,WP20,WP24|IN_PROGRESS|账户/tx/meta乱序、bank_id/slot/父hash/fork/replay不ready与独立manifest屏障已实现|可信初始快照下的未变继承/显式skip基础已实现；真实bank/回放补洞来源与online发布仍未闭合|[定向验证](chain-parser-rust/crates/solana-ingest/tests/stream.rs)|
|28.26.r04-ensure-pool-scope|WP08,WP09,WP20,WP24|IN_PROGRESS|Go/Rust认证HTTP固定目录/代际/新鲜度/actor与正常reanchor已接线|取消等待不释放提交actor已修复；动态扩scope/完整生产恢复仍待集成|[定向验证](chain-parser-go/internal/service/scope_control_test.go)|
|28.10.r04-current-baseline-recovery|WP08,WP09,WP20,WP24|IN_PROGRESS|双语言恢复状态机、候选/分片/尾窗与真实文件store/重启撤销旧ready已实现|真实RecoverySource/同online事务prepared集成/长期恢复容量待验收|[定向验证](chain-parser-rust/crates/solana-ingest/tests/scope_recovery.rs)|
|28.26.r04-history-isolation|WP08,WP09,WP20,WP24|IN_PROGRESS|历史独立store/namespace/权限能力/预算和RECOVERY_DATA_ONLY边界已实现|持续history调度/线上source集成与故障实证未闭合|[定向验证](chain-parser-rust/crates/solana-ingest/tests/scope_recovery.rs)|

|28.r01-r04-validation|WP02,WP03,WP04,WP05,WP07,WP08,WP09,WP10,WP11,WP18,WP20,WP24|IN_PROGRESS|精确任务/设计/WP/源码测试对应检查与本地基础回归已接入，逐问题反例和独立复审进行|真实来源/全版本/生产事务及完整进程/恢复验收仍未闭合|[对应关系测试](integration/tools/ci/tests/test_r01_r04_traceability.py)|
|12.10.multi-route-grid|WP12,WP13,WP16,WP25|LOCAL_SCOPE_VERIFIED|最多8路、独立状态子集DP、共享池按顺序重放、独立整数穷举、完整守恒及次数/时间预算已实现|当前三服务完整check及正常进程同源复验通过；限定候选网格外和一般DAG不宣称最优|[本批核对](integration/artifacts/local/core-quote-q1-local-463896a2b08e.json)|
|13.split-probe-ranking|WP12,WP13,WP25|LOCAL_SCOPE_VERIFIED|浅池分配额试算排序，保留最佳已验证整单fallback；普通API暴露候选与分配预算耗尽|增量图/更广beam与网格外收益界未完成|[本批核对](integration/artifacts/local/core-quote-q1-local-463896a2b08e.json)|
|12.14.velodrome-v2|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP18,WP25|LOCAL_SCOPE_VERIFIED|独立b3065d8b协议版本、同块getter/code依赖、wire、原函数输出与后状态；normal Go/PG/Kafka双Router及撤销恢复通过|当前三服务完整check及正常进程同源复验通过；完整swap/实际OP来源资格仍缺|[普通服务](integration/artifacts/local/core-velodrome-local-q1-463896a2b08e.json)<br>[本批核对](integration/artifacts/local/core-velodrome-local-q1-463896a2b08e.json)|
|12.14.lfj-v21|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP18,WP25|LOCAL_SCOPE_VERIFIED|独立field31/Go正常解析/完整bin fee Oracle/4 C++wire及服务联验，19实际API/38原swap真实收款和声明后状态匹配，10反例拒绝|标准同步Token与外部funds限定；真实Avalanche来源、公共执行/链费用与其他LB版本未完成|[正常服务与原计划](integration/artifacts/local/core-lfj-local-q1-463896a2b08e.json)|
|12.14.slipstream-context|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP18,WP25|PARTIAL_LOCAL|独立field32/f8717faa-module-1接入完整有限上下文；108原swap中90完整匹配、18部分成交拒绝，4序列12步、108原银行解析和13实际API/13原swap收款及全后状态通过；旧profile拒绝非零module保留|最终同源全check及5项进程范围已通过；其他module/Token/来源资格与生产执行未完成|[当前检查](integration/artifacts/local/core-quote-current-audit.json)|
|12.14.pancake-lm|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP18,WP25|PARTIAL_LOCAL|独立field33/98684794-lm-1完整状态、固定块解析、原数学后状态、资金上下文及共享物理池路由已合入主目录|主目录全check分阶段验证、六进程回归和13实际API原程序重放已通过；实际后续BSC代际/特殊Token/ExactOut/净链费用/可信来源与生产执行未完成|[限定范围验收](integration/artifacts/local/core-lm-local-q1-20261109.json)|
|12.14.pancake-bsc4|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP18,WP25|PARTIAL_LOCAL|实际第四代与三代前置代码/原数学、field34、固定块解析、资金上下文和共享状态已接；正常API原程序已通过受控范围|当前Go全检/双Rust全目标lint和定向/实际13API原程序回放已通过；完整可信来源/净链费用与其他模型继续|[当前进程](integration/artifacts/local/core-pancake-bsc4-normal-process.json)|
|13.graph-admission-refresh|WP09,WP12,WP13,WP16,WP25|PARTIAL_LOCAL|完整银行的准入变化参与图刷新，空候选停止报价且保留提交水位，旧句柄撤权/恢复；17副本/12API及正常原程序回归通过|通用增量/COW图、严格最优界、费用和其余模型仍未完成|[限定范围](integration/artifacts/local/core-graph-admission-local-20261109.json)|
|13.candidate-capability-disclosure|WP09,WP12,WP13,WP16,WP25|PARTIAL_LOCAL|未知数学与不支持Token行为的源行保留并显式排除，公开candidateScope/graph边一致，能力恢复正确|不等于新增被排除协议/特殊Token支持，净费用/ExactOut和其余模型仍待完成|[限定范围](integration/artifacts/local/core-candidate-scope-local-20261109.json)|
|12.10.exact-out-v2|WP03,WP05,WP09,WP10,WP11,WP12,WP13,WP14,WP16,WP25|LOCAL_SCOPE_VERIFIED|独立精确目标/Pair结算、反向路径、39有序共享网格、独立整单fallback、请求/计划/预算/缓存及45实际API/61原交易回放通过|仅固定原UniswapV2完整状态/标准Token；其他协议/净费用/公共原子执行未完成|[当前限定验收](integration/artifacts/local/core-exact-out-local-q1-20261109.json)|
|12.10.exact-out-bsc4|WP03,WP05,WP09,WP10,WP11,WP12,WP13,WP14,WP16,WP25|LOCAL_SCOPE_VERIFIED|实际BSC第四代独立负输出/完整fee/reward/Oracle/迁移账本、单池模板、输入预算和API、41实际计划原程序回放已通过|限定标准Token与单池回调资金；多跳/其他版本/净费用及公共执行未完成|[限定验收](integration/artifacts/local/core-bsc4-exact-out-local-q1-20261110.json)|
|12.10.exact-out-legacy-lm|WP03,WP05,WP09,WP10,WP11,WP12,WP13,WP14,WP16,WP25|LOCAL_SCOPE_VERIFIED|固定98684794旧LM负输出/完整账本、单池路径/预算/计划与49实际API/49原兑换/9负控已通过；106 Rust执行及三项正常回归通过|标准Token单池回调范围；其他协议/多跳/净费用和公开执行仍缺|[限定验收](integration/artifacts/local/core-legacy-lm-exact-out-local-q1-20261110.json)|
|12.10.exact-out-slipstream|WP03,WP05,WP09,WP10,WP11,WP12,WP13,WP14,WP16,WP25|LOCAL_SCOPE_VERIFIED|独立输出驱动/origin费率与完整Gauge/奖励/Oracle、单池路径/共享分配/预算/API；候选截断显式报告已修复；49实际API/49原兑换/9负控与121 Rust执行通过|限定标准Token/已知origin模块和单池模板；公开执行/净费用及其他版本仍缺|[限定验收](integration/artifacts/local/core-slipstream-exact-out-local-q1-20261110.json)|
|12.10.exact-out-lfj|WP03,WP05,WP09,WP10,WP11,WP12,WP13,WP14,WP16,WP25|LOCAL_SCOPE_VERIFIED|独立原最小输入/真实额外输出、完整bin/fee/Oracle、单池路由/共享分配/可选计划/API；181实际API/原Pair、24额外到账/9负控及136 Rust执行已通过|标准Token单池模板、控制factory和分开预付款；公开原子执行/可信Avalanche来源/净费用仍缺|[限定验收](integration/artifacts/local/core-lfj-exact-out-local-q1-20261110.json)|

上述breaker六条功能的完整冻结验收统一见[限定最终验收](integration/artifacts/local/managed-v2-breaker-local-acceptance.json)，单项验证列保留既有源码/测试绑定。

|12.11.meteora-mm|WP02,WP03,WP05,WP09,WP11,WP12,WP13,WP20,WP25|LOCAL_SCOPE_VERIFIED|固定MM-only field35/普通RPC及bank解析、独立整数ExactIn/完整后状态、路由/共享优化/API；26同源检查/108 Rust及71实际API原SBF、9故障恢复通过|仅注明classic SPL/无奖励订单/无host/InputOnly/默认bitmap有界范围；其余DLMM、ExactOut、真实公开银行及执行未完成|[当前限定验收](integration/artifacts/local/core-meteora-mm-validation-20261110.json)|

|12.10.exact-out-meteora|WP03,WP05,WP09,WP11,WP12,WP13,WP14,WP16,WP20,WP25|LOCAL_SCOPE_VERIFIED|固定MM-only独立输出驱动、真实到账/bin/vault余量完整分账；Solana候选/有序优化/计划/API；164原目标与20后续、65 Rust/19检查、43实际API原SBF及43硬上限/9故障恢复通过|仅已固定策略的单池路由；非MM策略、可信主网身份、公开执行与净链费用仍缺|[当前限定验收](integration/artifacts/local/core-meteora-exact-out-local-q1-20261111.json)|

|12.11.dlmm-only-y-pump|WP02,WP03,WP05,WP09,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|OnlyY 236目标原SBF/双向费用/HTTP；Pump classic/SOL完整账户/wire/RPC/数学/API；23检查/44Rust、38实际计划原SBF、16故障恢复及重启、8次篡改拒绝通过|其余DLMM策略、Pump迁移AMM/Token扩展/ExactOut、真实复合执行和第二批非EVM链尚缺；OnlyY正常进程仅ExactIn|[最终有限验收](integration/artifacts/local/core-family-expansion-acceptance-20261111.json)|

|12.11.solana-expanded|WP02,WP03,WP05,WP09,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|DLMM bitmap/双奖励/订单/Token2022；Pump双向ExactOut/canonical v1/boost、实际migrate交接；33检查/56Rust/45计划原SBF/10故障/5重启通过|仅注明profile；策略交叉/任意Token扩展、公开执行、自动发现与可信银行仍缺；正常进程仅ExactIn|[最终限定验收](integration/artifacts/local/core-solana-expanded-acceptance-20261111.json)|
|12.11.dlmm-orders-exact-out|WP03,WP05,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|独立订单ExactOut：固定原SBF的bin级反解、全bin耗尽分支、MM/processed/open结算及完整后状态；192原始银行/193成功步骤/31拒绝/161输入减一控制|同源码限定验收已通过；订单管理、奖励领取及高级策略交叉仍缺|[本批验证](integration/artifacts/local/core-meteora-orders-exact-out-validation-20261111.json)|
|12.12.quote-execution|WP03,WP05,WP09,WP11,WP12,WP13,WP14,WP15,WP16,WP25|PARTIAL_LOCAL|PumpSwap卖出指定WSOL目标的独立最小输入与三ceil费；Meteora只读账户核对/unsigned transaction；实际Chrome请求生产handler、下载字节和原SBF复核已通过|只覆盖明确模板；自动ATA创建/wrap、多协议构建、生产独立模拟、签名广播/结算等仍待开发|[本批验证](integration/artifacts/local/core-quote-execution-validation-20261112.json)|
|14.ata-simulation|WP05,WP09,WP14,WP15,WP16,WP25|PARTIAL_LOCAL|收款ATA创建与swap同一消息、payer租金/fee合计检查、只读RPC模拟实际收付/报价一致性；浏览器参数变化拒绝旧准备结果|仅经典SPL规范收款ATA；缺失源/Token2022收款创建、SOL包装、生产独立来源、钱包签名/广播结算仍待开发|[本批验证](integration/artifacts/local/core-account-preparation-validation-20261112.json)|
|14.native-funding|WP05,WP09,WP14,WP15,WP16,WP25|PARTIAL_LOCAL|显式输入WSOL原生SOL补资、缺失规范ATA创建、现有账户仅补差额、租金/包装/fee合计检查和只读模拟|报价仍使用WSOL；不自动关闭或unwrap、不签名广播；其他协议/Token扩展及来源资格仍缺|[本批验证](integration/artifacts/local/core-native-funding-validation-20261112.json)|
|14.temporary-wsol|WP05,WP09,WP14,WP15,WP16,WP25|PARTIAL_LOCAL|显式输出解包SOL、隔离输入余款退回、临时账户租金回收/余额核对已通过真实下载与原程序联验|限经典WSOL及Meteora/PumpSwap单池，输出要求caller等于recipient；其他执行模板和链/DEX组合仍缺|[本批验证](integration/artifacts/local/core-temporary-wsol-validation-20261112.json)|
|14.token2022-preparation|WP05,WP09,WP14,WP15,WP16,WP25|PARTIAL_LOCAL|Meteora限定Token2022收款ATA创建、扩展租金、净到账/withheld及SOL退款组合已通过原程序和前端联验|仍限TransferFeeConfig/TransferFeeAmount和ImmutableOwner；其他扩展、协议和链组合待补|[本批验证](integration/artifacts/local/core-token2022-preparation-validation-20261113.json)|
|12.pipeline-foundation|WP03,WP05,WP09,WP11,WP12,WP14,WP15,WP16,WP25|PARTIAL_LOCAL|Raydium creator独立ExactOut、协议子QuoteView/能力标签及独立preparation组件开发与联验|按用户六阶段主线持续补协议适配、unsigned、独立模拟和钱包/回执；不把能力未实现标签当完成|[本批验证](integration/artifacts/local/core-pipeline-foundation-validation-20261113.json)|
|14.raydium-preparation|WP05,WP14,WP15,WP16,WP25|PARTIAL_LOCAL|Raydium程序/config/池/vault/mint/时钟/observation闭包校验、unsigned指令及模拟净收付实现并联验|仅已有经典SPL账户；钱包签名/回执已在阶段65接入，Raydium建户/包装/解包及其他模型继续补|[本批验证](integration/artifacts/local/core-raydium-preparation-validation-20261113.json)|
|15.local-sbf-service|WP05,WP09,WP15,WP16,WP25|PARTIAL_LOCAL|独立原SBF执行缓存unsigned交易、摘要/预算/源银行不变及quote/preparation/view证据绑定实现|固定运行时及已支持模板范围；阶段66增加RPC账户捕获，历史/生产资格单独披露，跨协议/跨链模板仍缺|[验证记录](integration/artifacts/local/core-solana-independent-validation-20261113.json)|
|15.rpc-account-capture|WP05,WP09,WP14,WP15,WP16,WP25|PARTIAL_LOCAL|同次RPC账户闭包原字节和缺失账户服务端缓存，与preparationHash和QuoteView绑定后独立SBF执行|不把minContextSlot/同批context当历史共识银行或生产资格；其他协议模型仍需独立适配|[验证记录](integration/artifacts/local/core-rpc-capture-validation-20261113.json)|
|14.pump-amm-execution|WP05,WP09,WP14,WP15,WP16,WP20,WP25|PARTIAL_LOCAL|PumpSwap经典/boost已有经典SPL账户、声明user上下文，ExactIn/ExactOut实际指令、独立模拟及真实钱包回执开发联验|阶段70补账户创建/包装/退款/解包；其他Token/程序版本、毕业迁移与多池模板仍需完成，不提升全量或生产资格|[验证记录](integration/artifacts/local/core-pump-amm-preparation-validation-20261113.json)|
|14.pump-curve-execution|WP05,WP09,WP14,WP15,WP16,WP20,WP25|PARTIAL_LOCAL|Pump原生SOL曲线已有Token账户、声明user上下文的三种原始指令、独立模拟和钱包回执开发联验|阶段69补经典收款ATA；毕业迁移、其他Token/volume及其他链模板尚需完成，不提升全量或生产资格|[验证记录](integration/artifacts/local/core-pump-curve-preparation-validation-20261113.json)|
|14.pump-native-ata|WP05,WP09,WP14,WP15,WP16,WP20,WP25|PARTIAL_LOCAL|Pump原生买入的收款ATA原子创建、System预存余额补差、租金/交易/网络费分列及回执联验|仅明确经典曲线/同一声明user；其他账户/WSOL转换、毕业迁移和其他模板继续推进|[验证记录](integration/artifacts/local/core-pump-native-ata-validation-20261113.json)|
|14.pump-amm-funding|WP05,WP09,WP14,WP15,WP16,WP20,WP25|PARTIAL_LOCAL|PumpSwap classic/boost ATA、WSOL补差/退款/解包与实际租金回执开发联验|当前单池单路经典SPL与声明user；其他版本、毕业迁移及51个缺失执行模板仍需完成|[验证记录](integration/artifacts/local/core-pump-amm-funding-validation-20261113.json)|
|12.orca-exact-out|WP05,WP09,WP12,WP14,WP15,WP16,WP25|PARTIAL_LOCAL|Orca五类模型的精确输出数学、原始manager完整后状态、路由/网格与API/前端报价联验|仅固定cc683ec9经典SPL及三数组范围；执行准备/unsigned/SBF/钱包模板仍未实现，不以native数学参考替代交易验证|[验证记录](integration/artifacts/local/core-orca-exact-out-validation-20261114.json)|
|12.rayclmm-exact-out|WP05,WP09,WP12,WP14,WP15,WP16,WP25|PARTIAL_LOCAL|Raydium CLMM六模型输出目标、选费/动态/奖励/Bitmap/Seed状态、路由与API/前端开发联验|固定6d7a0f82经典SPL；协议Token选费不含Token2022转账扩展；六模型执行模板仍待补齐|[验证记录](integration/artifacts/local/core-rayclmm-exact-out-validation-20261114.json)|
|12.standard-v3-exact-out|WP03,WP05,WP09,WP12,WP14,WP15,WP16,WP25|PARTIAL_LOCAL|基础Uniswap/Pancake无LM V3目标输出、真实原Pool回调付款、简单路径/网格及API/前端开发联验|基础模型为定价状态投影；只接明确静态classic ERC20，无unsigned/钱包模板；Slipstream和LM上下文另列|[验证记录](integration/artifacts/local/core-v3-exact-out-validation-20261114.json)|
|12.weighted-exact-out|WP12,WP14,WP15,WP16,WP25|PARTIAL_LOCAL|三类不可变Weighted独立GIVEN_OUT内核、物理余额向量和非首token路由、最小输入分配、API/前端|仅单池路线及配置网格；原回调facade无ERC20转账/完整Vault；Managed/Composable及执行模板继续未完成|验证记录（`integration/artifacts/local/weighted-exact-out-check/report.json`，已清理）|
|12.stable-exact-out|WP12,WP14,WP15,WP16,WP25|PARTIAL_LOCAL|Balancer标准Stable2输出目标内核、单池有序路由和最小输入网格、HTTP/前端|原始StableMath包装参考；完整Vault/实际资金转账、Composable/BPT/rate及执行模板仍未实现|验证记录（`integration/artifacts/local/stable-exact-out-check/report.json`，已清理）|
|12.singleton-exact-out|WP12,WP14,WP15,WP16,WP25|PARTIAL_LOCAL|静态零hook V4/Infinity CL原生正数目标、逐word数学与物理状态、容量网格/同池分配、API/前端联验|仅核心定价投影；Manager/Vault/Token资金结算、advanced/hook/native与执行模板仍未完成|验证记录（`integration/artifacts/local/singleton-exact-out-check/report.json`，已清理）|
|12.infinity-bin-exact-out|WP12,WP14,WP15,WP16,WP25|PARTIAL_LOCAL|静态完整树原始BinPool正数目标、burn尘埃与物理投影、网格/顺序分配、API/前端|App/protocol账本为明示投影，完整Manager/Vault/Token结算、动态/hook/执行模板仍未完成|验证记录（`integration/artifacts/local/infinity-bin-exact-out-check/report.json`，已清理）|
|12.managed-snapshot-exact-out|WP12,WP14,WP15,WP16,WP25|PARTIAL_LOCAL|零BPT费用Managed快照实际压缩权重和费率、任意币对输出目标/顺序分配/API/前端|原ManagedPool+测试Vault facade，无完整Vault/资金执行；非零管理/协议swap费用及其他Managed版本另列|验证记录（`integration/artifacts/local/managed-snapshot-exact-out-check/report.json`，已清理）|
|12.composable-exact-out|WP12,WP14,WP15,WP16,WP25|PARTIAL_LOCAL|普通token输出目标、任意BPT位置映射/完整物理向量、BPT图/数学/计划/API拒绝和顺序/网格分配|原StableMath费用/缩放包装，不证明完整ComposablePool/Vault/资金执行；BPT兑换/供应/费账/缓存与执行模板另列|验证记录（`integration/artifacts/local/composable-exact-out-check/report.json`，已清理）|
|14.raydium-funding|WP05,WP14,WP15,WP16,WP25|LOCAL_SCOPE_VERIFIED|Raydium经典SPL显式ATA、WSOL补足/临时退款/解包，原子前后账户捕获、独立SBF、钱包/回执；10场景20原交易/21检查/26次Rust/155项Chrome通过|仅本固定CP-Swap标准经典SPL；Token2022/其他版本/生产与历史bank资格不升级|验证记录（`integration/artifacts/local/raydium-funding-check/report.json`，已清理）|
|14.orca-execution|WP05,WP14,WP15,WP16,WP25|LOCAL_SCOPE_VERIFIED|静态/奖励经典SPL原swap、完整Pool/tick与WSOL捐赠验证、32银行/32API和重启回执、64原交易；25检查/88次Rust/236项Chrome通过|adaptive/Token2022/其他版本和完整链流/生产与历史bank资格不升级|验证记录（`integration/artifacts/local/orca-execution-check/report.json`，已清理）|
|14.orca-adaptive-execution|WP05,WP14,WP15,WP16,WP25|LOCAL_SCOPE_VERIFIED|三个自适应经典SPL执行、完整254字节Oracle/写权限和资金/模拟；54API/54回执/108原交易及28检查/95次Rust/357项Chrome通过|其他程序版本/Token2022、多池和生产与历史bank资格不升级|验证记录（`integration/artifacts/local/orca-adaptive-execution-check/report.json`，已清理）|
|14.rayclmm-execution|WP05,WP14,WP15,WP16,WP25|LOCAL_SCOPE_VERIFIED|静态/奖励/Token-fee经典SPL单池准备、完整Pool/Observation/数组、资金/独立SBF与钱包回执；56API/56回执/112实际wire原交易及31门禁/135次Rust/486项Chrome通过|bitmap/custom-seed/dynamic和Token2022资产、跨池/生产与历史bank资格仍待独立验收|验证记录（`integration/artifacts/local/rayclmm-execution-check/report.json`，已清理）|
|14.rayclmm-advanced-execution|WP05,WP14,WP15,WP16,WP25|LOCAL_SCOPE_VERIFIED|三个高级经典SPL模型完整扩展/seed/动态费准备、同捕获SBF、162API/162回执/324原交易及33门禁/306Rust/667Chrome冻结通过|其他扩展/Token2022、多池、真实历史bank与生产资格仍未完成|验证记录（`integration/artifacts/local/rayclmm-advanced-execution-check/report.json`，已清理）|
|14.wallet-lifecycle|WP05,WP09,WP14,WP15,WP16,WP25|PARTIAL_LOCAL|已有unsigned/独立SBF模板接Wallet Standard、显式发送与实际收付回执/恢复|默认关闭，按链启用；本批仅受控Solana模板，不等于全链全协议生产资格/完整多租户或多区执行验收|[验证记录](integration/artifacts/local/core-wallet-lifecycle-validation-20261113.json)|
|13.incremental-graph|WP09,WP12|LOCAL_SCOPE_VERIFIED|物理池索引、共享邻接边和副本增量发布；增删/多资产/准入/顺序、128变更/256池共享及路由/分配/API回归通过|基线重锚与闲时回收由[阶段86](#plan-2026-11-17-baseline-cache-reclamation)补齐；顶层索引/完整银行处理仍遍历；池状态COW、beam/联合DAG、收益界和生产容量仍待完成|验证记录（`integration/artifacts/local/incremental-graph-check/report.json`，已清理）|
|13.baseline-cache-reclamation|WP09,WP12,WP16|LOCAL_SCOPE_VERIFIED|基线/owner恢复/scope扩容图复用、过期/撤权缓存清理、锁外析构和后台任务关停；23副本/7缓存与HTTP/11API回归通过，钱包8场景全部通过|完整Pool状态/顶层索引COW、活跃慢请求内存预算、长期压力及生产容量仍待补|验证记录（`integration/artifacts/local/cache-reclamation-check/report.json`，已清理）|
|14.evm-preparation-component|WP14,WP15,WP16|LOCAL_SCOPE_VERIFIED|完整目标/Router02绑定、给定观察下资金费用校验、无损数量、nonce耗尽和基础gas下界；19组件/31计划/2能力/10API及95独立对照通过|组件本身仅使用调用方观察；RPC捕获和显式API已由阶段88接通，当前链规则/L2费用、独立VM与钱包仍待补，执行能力仍21/61|验证记录（`integration/artifacts/local/evm-preparation-component-check/report.json`，已清理）|
|13.solana-message-native-cost|WP13,WP14,WP15,WP16|LOCAL_SCOPE_VERIFIED|绑定getFeeForMessage原消息/slot/fee和账户snapshot，资金checked u64，网络费单独小计；租金与swap/wrap本金不当fee|14门禁/29次Rust、20实际HTTP与原SBF及独立审计通过；来源/固定输入/runtime冻结一致|验证记录（`integration/artifacts/local/solana-native-cost-check/report.json`，已清理）|
|13.solana-signed-fee-context|WP13,WP14,WP15,WP16|LOCAL_SCOPE_VERIFIED|提交前费用源context与账户/仿真/发送min slot连续；不可变费用凭证与原v1回执兼容、幂等重试|11门禁/46次Rust，slot/费用拒绝、耐久/旧v1及实际钱包回归通过|验证记录（`integration/artifacts/local/solana-fee-preflight-check/report.json`，已清理）|
|13.solana-native-economics|WP13,WP14,WP15,WP16|LOCAL_SCOPE_VERIFIED|已执行准备交易的native/显式WSOL结算同单位估值；经济净额与押金/原生现金分列，未知币值不伪造net|16门禁/50次Rust、11正常SBF/回执与独立签名/wire/原余额审计通过|验证记录（`integration/artifacts/local/solana-native-economics-check/report.json`，已清理）|
|15.native-economics-view|WP13,WP15,WP16|LOCAL_SCOPE_VERIFIED|已执行交易的原生同单位净额/费用/押金展示；普通币/负净额/旧回执兼容且不伪造FAST净排名|8门禁/16浏览器检查，五类回执实际可见、整数/身份及截图复核通过|验证记录（`integration/artifacts/local/native-economics-view-check/report.json`，已清理）|
|15.solana-failure-fees|WP13,WP14,WP15,WP16|LOCAL_SCOPE_VERIFIED|失败签名交易原wire/slot/err与实际fee、原生币/Token回滚绑定并耐久保存|13门禁/3真实本地失败及独立余额审计通过；可信bank与完整费用估值仍缺|验证记录（`integration/artifacts/local/solana-failure-fee-check/report.json`，已清理）|
|15.failed-fee-view|WP13,WP15,WP16|LOCAL_SCOPE_VERIFIED|失败费用与未核验/矛盾/旧回执独立展示；保留成功成交净额|9门禁/四实际场景/13浏览器检查及实际可见截图复核通过|验证记录（`integration/artifacts/local/failed-fee-view-check/report.json`，已清理）|
|15.evm-legacy-native-fees|WP13,WP14,WP15,WP16|LOCAL_SCOPE_VERIFIED|Legacy原生费测量与付款人净扣款，旧缺测量保留未知|14门禁/16原bank/8次HTTP和56独立向量通过；完整链费与价格换算仍缺|验证记录（`integration/artifacts/local/evm-legacy-native-fee-check/report.json`，已清理）|
|13.cost-unavailable-ranking|WP13,WP16,WP18|LOCAL_SCOPE_VERIFIED|无可信成本及估值时不以route_activation_cost改变报价排序，明确raw目标、费用缺失/未扣及原搜索约束|16门禁/176次Rust、三池与原ExactOut grid/约束预算/四HTTP通过|验证记录（`integration/artifacts/local/cost-unavailable-ranking-check/report.json`，已清理）|
|13.evm-native-payer-cost|WP13,WP15,WP16|LOCAL_SCOPE_VERIFIED|按原bank余额校验最大gas资金准入，费用gross与payer实际net分列，alias自付tip返回|14门禁/146次Rust、56向量/四HTTP通过；完整成本与净排序仍缺|验证记录（`integration/artifacts/local/evm-native-payer-cost-check/report.json`，已清理）|
|13.evm-native-fee-balance-audit|WP13,WP15,WP16|LOCAL_SCOPE_VERIFIED|绝对before绑定原bank输入，fee与实际native debit/beneficiary credit守恒；caller=coinbase净扣burn不重复tip，未知数据/资金操作拒绝|14门禁/同state/修改/平移/alias及四次actual HTTP通过；生产资格与完整成本仍缺|验证记录（`integration/artifacts/local/evm-native-balance-audit-check/report.json`，已清理）|
|13.evm-native-cost-components|WP13,WP14,WP15,WP16|LOCAL_SCOPE_VERIFIED|共享checked uint256原生执行费与最大reserve、Legacy/EIP1559有效单价，API回执Gas/价格/fee组件核对，scope/binding/未定项明确|14门禁/142次Rust/56独立向量/两实际HTTP执行通过；额外链费/授权资金操作/可靠价格/FAST路由净收益仍缺|验证记录（`integration/artifacts/local/evm-native-cost-model-check/report.json`，已清理）|
|12.14.public-v2-original-execution|WP03,WP10,WP14,WP15|LOCAL_SCOPE_VERIFIED|原始实际code+MPT状态、显式local native funding经原WETH deposit/transfer和Pair.swap，实际收款/完整after state、多1输出原K回退；Rust独立比较|14门禁/Go原VM与Rust全状态冻结差分通过；未签名call序列/非未来交易保证/非生产VERIFY；其余方向/费用/更多版本仍需扩展|验证记录（`integration/artifacts/local/public-v2-original-execution-check/report.json`，已清理）|
|7.current-v2-pinned-offline-fixture|WP01,WP03,WP04,WP05,WP09,WP24|LOCAL_SCOPE_VERIFIED|固定actual公开block/runtime/proof/receipt原字节与获取元数据；无网络恢复工具、同字节Go本地副本、封闭loopback及多进程回归|13门禁/83次Go/36次Rust固定bank离线验收通过；数据仍依赖明确外部bundle，缺失须失败，不临时下载或改预期|验证记录（`integration/artifacts/local/current-v2-fixed-check/report.json`，已清理）|
|7.current-v2-public-source-replay|WP03,WP04,WP05,WP09,WP10,WP24|LOCAL_SCOPE_VERIFIED|真实公共当前state→normal owner/router；封装及内层raw身份验证、来源分离只读重放、双方向独立整数oracle、无KV副本/owner升代重启与坏来源拒绝|13门禁/81次Go/36次Rust及实际公开源/离线多进程通过；其他版本/历史重组/实际交易后余额与生产资格仍缺|验证记录（`integration/artifacts/local/current-v2-check/report.json`，已清理）|
|7.evm-current-head-confirmation-policy|WP04,WP05,WP24,WP25|LOCAL_SCOPE_VERIFIED|EVM确认策略限制为当前provider标签，明确拒绝历史游标/pending/未知别名；配置哈希/前置拒绝/来源披露|19门禁、跨语言同轮15配置/hash/拒绝码、当前CLI和普通服务验证通过|验证记录（`integration/artifacts/local/confirmation-policy-check/report.json`，已清理）|
|7.v2-complete-native-token-balance-proof|WP03,WP04,WP05,WP10|LOCAL_SCOPE_VERIFIED|原Token代码与每个实际SLOAD的MPT承诺支持独立只读余额重放；无未证明默认零/任意布局假定，Gas/slot/pass及原session总预算有界|8门禁/77次Go race与241外部模块缓存校验通过；仅固定零caller view，不证明Token转账行为或共识|验证记录（`integration/artifacts/local/v2-native-balance-proof-check/report.json`，已清理）|
|7.v2-complete-native-state-proof|WP03,WP04,WP05,WP10|LOCAL_SCOPE_VERIFIED|原Pair/Factory runtime的SHA与MPT account Keccak双绑定，读取同native session证明逻辑slot并核对getter；失败耗尽session|8门禁/488次Go race、正常同block证明及16类拒绝通过；不授予Token余额/共识资格|验证记录（`integration/artifacts/local/v2-native-state-proof-check/report.json`，已清理）|
|9.automatic-checkpoint-reclamation|WP08,WP09,WP24|LOCAL_SCOPE_VERIFIED|Go显式auto_reclaim/周期/配额条件，来源守卫/单发布通道/总deadline，pending优先且不在同轮重新选择；无eligible不写控制journal|27门禁通过；Rust执行、全盘和远端容量及独立故障域仍缺|验证记录（`integration/artifacts/local/checkpoint-auto-reclaim-check/report.json`，已清理）|
|9.explicit-checkpoint-reclamation|WP08,WP09,WP24|LOCAL_SCOPE_VERIFIED|Go显式manual回收、当前/候选/最近备份保护、原远端字节核验、有限自校验journal/隔离/恢复与幂等、正常HTTP；原预览仍只读|27门禁Go显式执行范围通过；Go自动维护由阶段116补齐，Rust执行/真实独立故障域仍缺|验证记录（`integration/artifacts/local/checkpoint-reclaim-check/report.json`，已清理）|
|9.rust-bounded-retention-plan|WP08,WP09,WP24|LOCAL_SCOPE_VERIFIED|Rust同等当前/全部候选/最近备份/年龄保护，完整对象/cert/owner绑定和预算，独立读取Go同一实际目录全JSON/planId一致|Go显式执行/中断恢复已由阶段115接入，Go自动维护由阶段116补齐；Rust执行和真实独立故障域仍缺|验证记录（`integration/artifacts/local/rust-retention-plan-check/report.json`，已清理）|
|9.bounded-checkpoint-retention-plan|WP08,WP09,WP24|LOCAL_SCOPE_VERIFIED|显式Go local_retention策略，完整对象/同store回读证书/所有候选保护、有界只读计划与planId/目录摘要；错误/取消不改文件|Go显式执行/中断恢复已由阶段115接入，Go自动维护由阶段116补齐；Rust执行和真实独立故障域仍缺|验证记录（`integration/artifacts/local/checkpoint-retention-plan-check/report.json`，已清理）|
|9.public-checkpoint-local-budgets|WP08,WP09,WP24|LOCAL_SCOPE_VERIFIED|两owner显式local_budget，串行gate内有界扫描/待写规划，重复对象不重复计费，超限旧对象保留；Go周期不重复旧JSON|26门禁当前范围通过；配额是显式单writer逻辑文件准入，非磁盘块预留，自动保留/远端容量后续|验证记录（`integration/artifacts/local/checkpoint-budget-check/report.json`，已清理）|
|9.rust-owner-recovery-catalogue|WP08,WP09,WP24|LOCAL_SCOPE_VERIFIED|Rust recovery目录绑定已上传旧对象与当前头，v2允许同owner/writer/canonical/scope/schema/adapter下完整baseline升代，router私有追平再发布|26门禁当前范围通过；writer/重组/范围改变转当前源，真实保留和独立故障域仍缺|验证记录（`integration/artifacts/local/rust-owner-recovery-check/report.json`，已清理）|
|9.rust-periodic-checkpoint-publisher|WP01,WP08,WP09,WP24|LOCAL_SCOPE_VERIFIED|Rust周期one-chain轮转/共享gate/stop watch与正常serve取消；配置允许两online parser、禁止router/analytics，Go/Rust规范默认值/hash|周期/共享摘要本轮通过；旧对象目录已在阶段111补齐，跨writer/实际保留回收/真实故障域仍缺|验证记录（`integration/artifacts/local/rust-periodic-checkpoint-check/report.json`，已清理）|
|9.rust-owner-remote-checkpoints|WP08,WP09,WP24|LOCAL_SCOPE_VERIFIED|正常Rust POST远端条件发布与完成cert、GET当前目录、独立凭据范围/来源切代/关闭中断与半上传/伪成功拒绝|远端本轮通过；周期/旧对象目录已在阶段110–111接入，实际保留回收/独立故障域仍缺|验证记录（`integration/artifacts/local/rust-remote-checkpoint-check/report.json`，已清理）|
|9.rust-owner-public-checkpoints|WP08,WP09,WP24|LOCAL_SCOPE_VERIFIED|Rust owner公共Protobuf checkpoint本地发布，硬链接不可替换/半发布续传/守卫与可取消共享gate，当前Rust原字节向量由Go解码|本地格式本轮通过；远端/周期/目录已在阶段109–111接入，实际保留回收/生产存储资格仍缺|验证记录（`integration/artifacts/local/rust-checkpoint-publication-check/report.json`，已清理）|
|9.bounded-checkpoint-tail-replay|WP08,WP09,WP24|LOCAL_SCOPE_VERIFIED|已完成上传候选只作提示，owner当前头+游标资格；Rust同代连续BLOCK_COMMIT私有重放，精确完整边界/最终ref与目录复核|19门禁当前范围通过；跨代/reorg转当前源、canonical公告/离线peer与真实保留/故障域仍缺|验证记录（`integration/artifacts/local/checkpoint-replay-check/report.json`，已清理）|
|9.fair-checkpoint-chain-scheduling|WP08,WP09,WP24|LOCAL_SCOPE_VERIFIED|单链持续失败不再使其他链饿死，正常服务双链持续发布/代际更新/故障恢复和旧生命周期回归|7门禁当前限定范围通过；旧快照追平、Rust发布和canonical公告继续|验证记录（`integration/artifacts/local/checkpoint-fairness-check/report.json`，已清理）|
|9.periodic-checkpoint-publisher|WP08,WP09,WP24|LOCAL_SCOPE_VERIFIED|Go online-only publish_interval_ms，串行完成后等待，错误重试、代际更新、取消/关闭等待与值无关指标|Go周期本轮通过；Rust周期/多链隔离已在阶段106–110接入，实际保留回收/跨writer恢复仍缺|验证记录（`integration/artifacts/local/checkpoint-worker-check/report.json`，已清理）|
|9.current-remote-checkpoint-recovery|WP08,WP09,WP24|LOCAL_SCOPE_VERIFIED|当前owner只资格化同状态/cursor已上传对象，Rust固定凭据范围下载并二次核对目录，正常Kafka恢复接线|当前对象恢复本轮通过；周期/旧对象尾窗已在阶段105–111补齐，canonical离线目录/真实故障域仍缺|验证记录（`integration/artifacts/local/checkpoint-recovery-check/report.json`，已清理）|
|9.remote-checkpoint-objects|WP08,WP09,WP24|LOCAL_SCOPE_VERIFIED|Go正常入口可上传独立HTTP对象API，半上传可续传、冲突不覆盖，受限下载验证pin及状态/cursor|条件对象API本轮通过；正常router恢复已在阶段104–111接入，canonical离线目录/真实独立故障域仍缺|验证记录（`integration/artifacts/local/remote-checkpoint-check/report.json`，已清理）|
|9.public-bootstrap-checkpoints|WP08,WP09,WP24|LOCAL_SCOPE_VERIFIED|公开OwnerBootstrap和固定摘要manifest，游标无损十进制字符串，Go写入及双Rust校验|公开本地格式本轮通过；远端/正常恢复已在阶段103–111接入，离线目录/真实独立故障域仍缺|验证记录（`integration/artifacts/local/public-bootstrap-check/report.json`，已清理）|
|16.analytics-continuous-worker|WP19,WP21|LOCAL_SCOPE_VERIFIED|现有Go服务内显式常驻只读分析模式，串行有界周期、耐久重启和异常停止|统一门禁通过；大规模事实保留/ClickHouse/shadow及查询继续|验证记录（`integration/artifacts/local/analytics-worker-check/report.json`，已清理）|
|15.evm-prepared-frontend|WP15,WP16,WP25|LOCAL_SCOPE_VERIFIED|EVM双模式原准备界面和浏览器wire摘要/状态/金额核对|统一门禁通过；当前银行捕获/其他链规则与钱包功能继续|验证记录（`integration/artifacts/local/evm-prepared-frontend-check/report.json`，已清理）|
|15.evm-prepared-api|WP14,WP15,WP16|LOCAL_SCOPE_VERIFIED|缓存原RPC准备、双模式同银行worker、原hash/unsigned/receipt重算及32MiB载荷预算|统一门禁通过；界面、实时完整RPC银行/真实链规则和钱包继续|验证记录（`integration/artifacts/local/evm-prepared-api-check/report.json`，已清理）|
|14.evm-source-fees|WP15|LOCAL_SCOPE_VERIFIED|显式源H base fee与H+1执行base fee独立校验和承诺|统一门禁通过；来源仍为显式提供银行，真实RPC状态捕获/chain费用继续|验证记录（`integration/artifacts/local/evm-source-fees-check/report.json`，已清理）|
|14.evm-input-paths|WP14,WP15,WP16,WP25|LOCAL_SCOPE_VERIFIED|ExactIn多池完整计划重算和原wire/VM，双模式正常HTTP与界面准备条件|统一门禁通过；同池重用/共享DAG/RPC完整银行与钱包仍待补|验证记录（`integration/artifacts/local/evm-input-path-check/report.json`，已清理）|
|14.evm-path-execution|WP14,WP15|LOCAL_SCOPE_VERIFIED|原Router简单路径ExactOut逐池检查与完整声明前后银行|统一门禁通过；ExactIn路径绑定/当前RPC与链规则仍待补|验证记录（`integration/artifacts/local/evm-path-execution-check/report.json`，已清理）|
|15.evm-exact-out-api|WP15,WP16|LOCAL_SCOPE_VERIFIED|普通ExactOut HTTP、真实worker、独立缓存证据及费用/未用预算|统一门禁通过；多池/可信RPC银行与钱包继续|验证记录（`integration/artifacts/local/evm-exact-out-api-check/report.json`，已清理）|
|14.evm-prepared-exact-out|WP14,WP15|LOCAL_SCOPE_VERIFIED|ExactOut双层准备承诺、同银行资金和实际原Router目标执行|统一验证通过；普通HTTP准备执行与多池VM后续继续|验证记录（`integration/artifacts/local/evm-prepared-exact-out-check/report.json`，已清理）|
|28.delivery-boundary-audit|WP01,WP23,WP24|LOCAL_SCOPE_VERIFIED|三服务生产配置与当前CLI、开发拓扑和26工作包审计；不复制凭据，不把开发测试升格|当前源码审计通过；所有本地实现差距保留，不将未实现功能改记为外部阻塞|审计记录（`integration/artifacts/local/design-delivery-audit/report.json`，已清理）|
|15.evm-dynamic-fee-api|WP15,WP16|LOCAL_SCOPE_VERIFIED|API校验REVM实际有效价/费用分解，保留原上限，实际HTTP子进程验证|统一门禁通过；仍是给定Shanghai银行，非当前链/完整费用/生产资格|验证记录（`integration/artifacts/local/evm-dynamic-fee-api-check/report.json`，已清理）|
|14.evm-exact-out-execution|WP14,WP15|LOCAL_SCOPE_VERIFIED|原Router02单池ExactOut的独立REVM输入消耗/收款、未用预算、revert费用|统一门禁通过；多池路径和正式准备API集成/当前状态与链规则仍待补|验证记录（`integration/artifacts/local/evm-exact-out-execution-check/report.json`，已清理）|
|14.evm-prepared-execution|WP14,WP15|LOCAL_SCOPE_VERIFIED|完整准备组件重算、原wire摘要、同银行资金与独立VM绑定，getter有效gas price对齐|统一验证通过；限定显式给定状态ExactIn，真实RPC状态DB/ExactOut VM/当前链规则/普通API执行仍待补|验证记录（`integration/artifacts/local/evm-prepared-execution-check/report.json`，已清理）|
|14.evm-dynamic-fees|WP15|LOCAL_SCOPE_VERIFIED|独立Shanghai仿真的cap/tip、真实native pre/post与费用承诺|统一门禁通过；当前源/其他hardfork/L2数据费仍待完成|验证记录（`integration/artifacts/local/evm-dynamic-fees-check/report.json`，已清理）|
|14.evm-rpc-preparation|WP04,WP14,WP15,WP16,WP25|LOCAL_SCOPE_VERIFIED|固定hash方法负对照、代码/完整V2字段、资金/授权/nonce及严格JSON/预算；认证QuoteView API和EVM费用/下载，统一门禁通过|本批是受控RPC观察接线资格，账户资金故障为受控观察；非共识、状态根证明、完整当前费用或独立执行，静态协议能力和生产未升级|验证记录（`integration/artifacts/local/evm-rpc-preparation-check/report.json`，已清理）|

<a id="batch-history"></a>

## 已验证批次摘要

下列均为历史本地验证范围；原始报告保留在integration/artifacts/local，详细原始输出不复制到本文件。

|序号|批次|结果|验证|
|---|---|---|---|
|1|Kafka/PG提交协议与租约隔离|LOCAL_SCOPE_VERIFIED|[kafka-three-service-smoke.json](integration/artifacts/local/kafka-three-service-smoke.json)|
|2|EVM重组撤销、回滚与原子发布|LOCAL_SCOPE_VERIFIED|[reorg-http-smoke.json](integration/artifacts/local/reorg-http-smoke.json)|
|3|V2成交事实与分析重组更正|LOCAL_SCOPE_VERIFIED|analytics-journal-7.json（`integration/artifacts/local/analytics-journal-7.json`，已清理）|
|4|Uniswap V3固定块解析与有界精确输入报价|LOCAL_SCOPE_VERIFIED|[v3-sdk-golden-generation.json](integration/artifacts/local/v3-sdk-golden-generation.json)|
|5|Pancake V1/V2/V3与Sushi V2版本适配|LOCAL_SCOPE_VERIFIED|[v3-differential.json](integration/artifacts/local/v3-differential.json)|
|6|Curve/Balancer/Aerodrome限定稳定池解析与计算|LOCAL_SCOPE_VERIFIED|[stable-three-service-smoke.json](integration/artifacts/local/stable-three-service-smoke.json)|
|7|V4/Infinity CL静态无hook解析与计算|LOCAL_SCOPE_VERIFIED|singleton-progress-consistency.json（`integration/artifacts/local/singleton-progress-consistency.json`，已清理）|
|8|Raydium CP-Swap creator布局解析与计算|LOCAL_SCOPE_VERIFIED|raydium-progress-consistency.json（`integration/artifacts/local/raydium-progress-consistency.json`，已清理）|
|9|Orca静态费、固定/动态tick数组与Q64计算|LOCAL_SCOPE_VERIFIED|orca-progress-consistency.json（`integration/artifacts/local/orca-progress-consistency.json`，已清理）|
|10|[Raydium CLMM静态输入费与基础bitmap](#plan-2026-10-27-raydium-clmm-static)|LOCAL_SCOPE_VERIFIED|[rayclmm-progress-consistency.json](integration/artifacts/local/rayclmm-progress-consistency.json)|
|11|[Raydium CLMM按Token0/Token1收费](#plan-2026-10-28-raydium-clmm-token-fee)|LOCAL_SCOPE_VERIFIED|[rayclmm-token-fee-progress-consistency.json](integration/artifacts/local/rayclmm-token-fee-progress-consistency.json)|
|12|[Raydium CLMM扩展bitmap解析与计算](#plan-2026-10-28-raydium-clmm-bitmap)|LOCAL_SCOPE_VERIFIED|[rayclmm-bitmap-progress-consistency.json](integration/artifacts/local/rayclmm-bitmap-progress-consistency.json)|
|13|[Raydium CLMM非零seed独立池身份](#plan-2026-10-29-raydium-clmm-custom-seed)|LOCAL_SCOPE_VERIFIED|[rayclmm-seed-reference-verification.json](integration/artifacts/local/rayclmm-seed-reference-verification.json)|
|14|[Raydium CLMM动态费与分段精确计算](#plan-2026-10-29-raydium-clmm-dynamic)|LOCAL_SCOPE_VERIFIED|[rayclmm-dynamic-progress-consistency.json](integration/artifacts/local/rayclmm-dynamic-progress-consistency.json)|
|15|EVM区块receipt结构与全块日志顺序完整性|LOCAL_SCOPE_VERIFIED|[验收报告](integration/artifacts/local/receipt-integrity-acceptance.json)|
|16|EVM多RPC完整receipt交叉核验与尾部遗漏差异隔离|LOCAL_SCOPE_VERIFIED|[验收报告](integration/artifacts/local/receipt-witness-acceptance.json)|
|17|Solana RPC原始账户响应耐久证据WAL与失败封闭恢复|LOCAL_SCOPE_VERIFIED|[验收报告](integration/artifacts/local/source-wal-acceptance.json)|
|18|EVM标准receipt根校验与共同删尾拒绝|LOCAL_SCOPE_VERIFIED|[验收报告](integration/artifacts/local/receipt-root-acceptance.json)|
|19|Solana同finalized context有界账户分片与全scope恢复|LOCAL_SCOPE_VERIFIED|[验收报告](integration/artifacts/local/account-shards-acceptance.json)|
|20|[Balancer V2双币不可变50/50权重池](#plan-2026-10-30-balancer-weighted2-equal)|LOCAL_SCOPE_VERIFIED|[本批验收](integration/artifacts/local/weighted-acceptance.json)|
|21|[Balancer V2双币不可变任意合法权重](#plan-2026-10-30-balancer-weighted2-static)|LOCAL_SCOPE_VERIFIED|[本批验收](integration/artifacts/local/weighted-static-acceptance.json)|
|22|[五组协议联合增量：已实现子集](#plan-2026-10-31-five-protocol-families)|PARTIAL_LOCAL|[限定子集验收](integration/artifacts/local/five-family-acceptance.json)|
|23|[Curve NG固定区块服务接入与完整交换后状态](#plan-2026-11-01-curve-ng-full-state)|LOCAL_SCOPE_VERIFIED|[NG限定范围验收](integration/artifacts/local/curve-ng-acceptance.json)|
|24|[Managed非零BPT费用完整交换状态](#plan-2026-11-01-managed-bpt-fees)|PARTIAL_LOCAL|[限定原回调参考验收](integration/artifacts/local/managed-reference-acceptance.json)|
|25|[MetaUSD直接兑换服务闭环](#plan-2026-11-01-metausd-direct)|LOCAL_SCOPE_VERIFIED|[Meta限定范围验收](integration/artifacts/local/curve-meta-acceptance.json)|
|26|[Crypto、Solana高级状态与Infinity跨Bin](#plan-2026-11-02-priority-extensions)|PARTIAL_LOCAL|[本批限定验证](integration/artifacts/local/priority-extensions-acceptance.json)|
|27|[Orca adaptive账户与正常服务接入](#plan-2026-11-02-orca-adaptive-online)|LOCAL_SCOPE_VERIFIED|[本批限定验收](integration/artifacts/local/orca-adaptive-acceptance.json)|
|28|[Infinity Bin完整树与普通服务接入](#plan-2026-11-02-infinity-bin-online)|LOCAL_SCOPE_VERIFIED|[本批限定验收](integration/artifacts/local/infinity-bin-acceptance.json)|
|29|[Curve Crypto活动ramp普通服务](#plan-2026-11-02-curve-crypto-ramp)|LOCAL_SCOPE_VERIFIED|[本批限定验收](integration/artifacts/local/curve-crypto-ramp-acceptance.json)|
|30|[Orca adaptive初始化奖励组合](#plan-2026-11-03-orca-adaptive-rewards)|LOCAL_SCOPE_VERIFIED|[本批限定验收](integration/artifacts/local/orca-adaptive-rewards-acceptance.json)|
|31|[Orca adaptive dynamic arrays组合](#plan-2026-11-03-orca-adaptive-dynamic)|LOCAL_SCOPE_VERIFIED|[本批限定验收](integration/artifacts/local/orca-adaptive-dynamic-acceptance.json)|
|32|[独立EVM交易执行与真实余额证据](#plan-2026-11-03-independent-evm)|LOCAL_SCOPE_VERIFIED|[本批限定验收](integration/artifacts/local/evm-execution-acceptance.json)|
|33|[V2完整Swap状态与未同步余额](#plan-2026-11-03-v2-complete-swap)|LOCAL_SCOPE_VERIFIED|[本批限定验收](integration/artifacts/local/v2-complete-acceptance.json)|
|34|[Managed V2实际发布版与非零AUM](#plan-2026-11-03-managed-v2-aum)|IN_PROGRESS|待本批验证|
|35|[Managed V2 BPT有限GIVEN_IN路由与服务](#plan-2026-11-06-managed-v2-bpt-finite)|LOCAL_SCOPE_VERIFIED|最后同源有限验收（`integration/artifacts/local/managed-v2-bpt-local-acceptance.json`，已清理）|
|36|[Managed V2已启用circuit breaker](#plan-2026-11-06-managed-v2-breaker)|LOCAL_SCOPE_VERIFIED|[限定最终验收](integration/artifacts/local/managed-v2-breaker-local-acceptance.json)；5任务/6功能映射与同freeze完整串行闭合|
|37|[R01–R04真实来源、持续采集与范围恢复](#plan-2026-11-06-r01-r04)|IN_PROGRESS|逐任务核对与实现中；环境探针（`integration/artifacts/local/r01-r04-environment-probe.json`，已清理）|
|38|[链与DEX解析、路由与兑换量计算收尾](#plan-2026-11-08-quote-completion)|IN_PROGRESS|C01/C02/C03及LFJ有限范围已通过[阶段冻结](integration/artifacts/local/core-quote-q1-local-463896a2b08e.json)；R07等计算缺口继续开发，整体请求未全部完成|
|39|[Slipstream调用方与Oracle费用上下文](#plan-2026-11-08-slipstream-context)|LOCAL_SCOPE_VERIFIED|三服务全check和5项正常进程范围同源冻结通过；13实际API原交换及8负控已通过；Polygon/Arbitrum修复验收输出目录后复验通过，未宣称整体剩余计算完成|
|40|[Pancake V3 LM主目录接入](#plan-2026-11-09-pancake-lm)|LOCAL_SCOPE_VERIFIED|独立field33/完整解析与账本/共享路由、80原完整swap/12序列步、13实际API原swap/9负控、六进程及同源分阶段检查通过；[最终报告](integration/artifacts/local/core-lm-local-q1-20261109.json)|
|41|[实际BSC第四代LM与惰性迁移](#plan-2026-11-09-bsc4-lm)|LOCAL_SCOPE_VERIFIED|独立field34/实际当前与前三代原程序；64原swap/15序列步、正常13API/13原swap/6次实际迁移/13负控、Go全检及双Rust全目标lint/定向通过；[最终报告](integration/artifacts/local/core-bsc4-local-q1-20261109.json)|
|42|[状态驱动的路由准入刷新](#plan-2026-11-09-graph-admission)|LOCAL_SCOPE_VERIFIED|3池准入反例修复，空候选水位推进/撤权与恢复；17副本/12API和相关全target lint通过，正常13API原程序/6迁移回归通过；[最终报告](integration/artifacts/local/core-graph-admission-local-20261109.json)|
|43|[候选范围与不可用池披露](#plan-2026-11-09-candidate-scope)|LOCAL_SCOPE_VERIFIED|修复left6/right4候选披露；52 Router/11解析域回归、相关全target lint和正常13API原程序/6迁移回归通过；[最终报告](integration/artifacts/local/core-candidate-scope-local-20261109.json)|
|44|[ExactOut独立输入目标与原Router基准](#plan-2026-11-09-exact-out)|LOCAL_SCOPE_VERIFIED|独立V2完整状态ExactOut数学/反向路径/有序拆单/API；整单探测遗漏反例先红后绿；90原交易/162 Pair转换、39网格/38可行、45实际API/61原交易/7负控、84 Rust执行及Router全目标lint通过；[当前限定验收](integration/artifacts/local/core-exact-out-local-q1-20261109.json)|
|45|[实际BSC第四代LM独立ExactOut](#plan-2026-11-10-bsc4-exact-out)|LOCAL_SCOPE_VERIFIED|独立负amountSpecified与完整LM账本/单池路由/API；48完整/16部分拒绝/4无效/15连续原执行，41实际API/41原swap/31迁移/13负控；97 Rust执行/Router全目标lint与V2正常回归通过；[限定验收](integration/artifacts/local/core-bsc4-exact-out-local-q1-20261110.json)|
|46|[固定98684794旧LM独立ExactOut](#plan-2026-11-10-legacy-lm-exact-out)|LOCAL_SCOPE_VERIFIED|原80完整/20部分拒绝/4无效/12连续，49实际API与原兑换、9负控；20检查/106 Rust及Router全目标lint、BSC4和V2正常回归通过；[限定验收](integration/artifacts/local/core-legacy-lm-exact-out-local-q1-20261110.json)|
|47|[Slipstream调用者费率独立ExactOut](#plan-2026-11-10-slipstream-exact-out)|LOCAL_SCOPE_VERIFIED|90完整/18部分拒绝/12连续原步骤，49实际API与原兑换/9负控、121 Rust执行/24检查及5项正常服务回归通过；候选截断标志红绿修复；[限定验收](integration/artifacts/local/core-slipstream-exact-out-local-q1-20261110.json)|
|48|[LFJ最小输入目标与额外输出](#plan-2026-11-10-lfj-exact-out)|LOCAL_SCOPE_VERIFIED|216完整/72部分拒绝/60额外输出/18连续原步骤，181实际API/原Pair和24额外到账/9负控；136 Rust执行/27检查/7项正常回归同源码通过；[限定验收](integration/artifacts/local/core-lfj-exact-out-local-q1-20261110.json)|
|49|[Meteora DLMM固定MM-only ExactIn](#plan-2026-11-10-meteora-mm)|LOCAL_SCOPE_VERIFIED|独立field35/原完整账本/普通RPC/路由/共享优化/API；95原案例及20连续后续、108 Rust/26检查、71 API/71原SBF和9故障/重启同源通过；[限定验收](integration/artifacts/local/core-meteora-mm-validation-20261110.json)|
|50|[Meteora DLMM 固定 MM-only ExactOut](#plan-2026-11-11-meteora-exact-out)|LOCAL_SCOPE_VERIFIED|独立输出驱动与完整bin/vault账本、单池路径/有序分配/API；65 Rust/19检查、43实际API原SBF与43输入减一及9故障/重启通过；[同源验收](integration/artifacts/local/core-meteora-exact-out-local-q1-20261111.json)|
|51|[DLMM OnlyY、Pump classic/SOL与组合边界](#plan-2026-11-11-dlmm-pump-combinations)|LOCAL_SCOPE_VERIFIED|历史限定验收23检查/44Rust、38计划原SBF及8篡改控制通过；[凭证](integration/artifacts/local/core-family-expansion-acceptance-20261111.json)|
|52|[DLMM扩展与Pump ExactOut/PumpSwap/毕业迁移](#plan-2026-11-11-dlmm-extended-pump-exact-out)|LOCAL_SCOPE_VERIFIED|33检查/56Rust、45实际计划生产投影/原SBF、10来源故障恢复/5重启及迁移创建账户交接；[最终凭证](integration/artifacts/local/core-solana-expanded-acceptance-20261111.json)|
|53|[DLMM订单独立ExactOut与完整业务链路](#plan-2026-11-11-dlmm-orders-exact-out)|LOCAL_SCOPE_VERIFIED|28检查/34次Rust测试；9真实ExactOut API计划生产投影/原SBF、2来源故障恢复及1次Router重启通过；[本批凭证](integration/artifacts/local/core-meteora-orders-exact-out-validation-20261111.json)|
|54|[前端链/币种选择与实时兑换量联调](#plan-2026-11-12-frontend-quote-playground)|PARTIAL_LOCAL|新增`frontend/`零构建联调页和Router `/v1/quote-options`；当前解析链/Token/Pool/协议选择、ExactIn/ExactOut实时请求、路线/快照/图版本/计划/错误展示；后端编译、3项HTTP回归（含元数据接口和既有扩展报价）、前端静态HTTP/语法检查通过；未替代全链/全DEX解析、策略交叉、生产可信源和公开交易执行验收；[本批记录](integration/artifacts/local/phase54-frontend-quote-playground.json)|
|55|[前端状态刷新与路由约束联调](#plan-2026-11-12-frontend-live-quote)|PARTIAL_LOCAL|前端10秒刷新解析状态和view/graph/protocol信息；输入变化自动报价、Abort旧请求、提交maxHops；Router元数据字段和协议/池断言通过；仍受当前ReplicaView及既有协议范围限制；全链/全DEX、可信生产来源和公开执行未完成；[本批记录](integration/artifacts/local/phase55-frontend-live-quote.json)|
|56|[协议报价能力与执行能力边界联调](#plan-2026-11-12-protocol-capabilities)|PARTIAL_LOCAL|quote-options复用协议数学内核公开每个Pool的quoteModes和execution能力；前端展示ExactIn/ExactOut及unsignedPlan/simulation边界；locked check、元数据测试、静态前端检查通过；动态账户预检查、unsigned交易计划、独立模拟仍按后续顺序开发；全链/全DEX范围未完成；[本批记录](integration/artifacts/local/phase56-protocol-capabilities.json)|
|57|[PumpSwap卖出目标、执行账户准备与真实浏览器联调](#plan-2026-11-12-quote-execution)|LOCAL_SCOPE_VERIFIED|24检查/24次Rust执行/13项真实Chrome联调通过；PumpSwap48目标/42成功/14额外到账/42输入减一；Meteora账户核对/unsigned wire实际下载及原SBF699输入1000到账；[本批记录](integration/artifacts/local/core-quote-execution-validation-20261112.json)|
|58|[收款ATA创建、只读模拟与前端闭环](#plan-2026-11-12-ata-simulation)|LOCAL_SCOPE_VERIFIED|17检查/8次Rust测试/17项真实Chrome联调通过；标准收款ATA创建/租金/fee、只读原SBF模拟、真实下载多指令交易与原程序执行/失败回滚；[本批记录](integration/artifacts/local/core-account-preparation-validation-20261112.json)|
|59|[输入WSOL补资与原子包装兑换联调](#plan-2026-11-12-native-funding)|LOCAL_SCOPE_VERIFIED|20检查/9次Rust测试/20项真实Chrome联调通过；输入WSOL ATA创建/差额包装/租金fee总额/保留WSOL和完整原程序回滚；[本批记录](integration/artifacts/local/core-native-funding-validation-20261112.json)|
|60|[临时WSOL解包与余款回收](#plan-2026-11-12-temporary-wsol)|LOCAL_SCOPE_VERIFIED|24项检查/12次Rust测试/24项真实Chrome业务联调通过；两类临时账户真实下载原SBF收付/退租/既有账户不变及失败回滚通过；[本批记录](integration/artifacts/local/core-temporary-wsol-validation-20261112.json)|
|61|[Token2022收款ATA与转账费联调](#plan-2026-11-13-token2022-preparation)|LOCAL_SCOPE_VERIFIED|30项检查/15次Rust测试执行/27项真实Chrome业务联调通过；178/182账户、epoch/cap/100%费率、混合租金退款和13原执行/回滚检查通过；[本批记录](integration/artifacts/local/core-token2022-preparation-validation-20261113.json)|
|62|[六阶段业务链路主线与协议视图](#plan-2026-11-13-six-stage-pipeline)|LOCAL_SCOPE_VERIFIED|本批36检查/34次Rust测试执行/28项真实Chrome联调通过；Raydium 1832单笔+54序列步、61模型目录/协议视图、独立preparation及13原SBF回归通过；六阶段整体仍继续；[验证记录](integration/artifacts/local/core-pipeline-foundation-validation-20261113.json)|
|63|[Raydium动态账户与unsigned交易](#plan-2026-11-13-raydium-preparation)|LOCAL_SCOPE_VERIFIED|41检查/35次Rust测试执行/30项真实Chrome联调通过；双向已有经典SPL账户unsigned、3组原SBF完整Oracle/账本及输入减一回滚通过；[本批验证](integration/artifacts/local/core-raydium-preparation-validation-20261113.json)|
|64|[配置化隔离SBF独立模拟](#plan-2026-11-13-local-sbf-service)|LOCAL_SCOPE_VERIFIED|46检查/39次Rust测试执行/34项真实Chrome联调通过；隔离原SBF、真实CU/收付、运行时/银行摘要及缓存preparation/协议视图证据门禁通过；[验证记录](integration/artifacts/local/core-solana-independent-validation-20261113.json)|
|65|[钱包签名广播与可恢复回执](#plan-2026-11-13-wallet-lifecycle)|LOCAL_SCOPE_VERIFIED|51检查/44次Rust测试执行/40项真实Chrome联调通过；Wallet Standard仅签名、真实Ed25519、实时预检、显式单次发送、八类回执及重启/页面恢复通过；[验证记录](integration/artifacts/local/core-wallet-lifecycle-validation-20261113.json)|
|66|[preparation实时RPC快照与独立SBF](#plan-2026-11-13-rpc-capture)|LOCAL_SCOPE_VERIFIED|51检查/47次Rust测试执行/41项Chrome联调通过；同次捕获/无预装钱包银行、6种当前状态/后续slot与成交后状态重报价、缓存字节预算及完整钱包单次发送回执通过；[验证记录](integration/artifacts/local/core-rpc-capture-validation-20261113.json)|
|67|[PumpSwap经典及boost执行链路](#plan-2026-11-13-pump-amm-execution)|LOCAL_SCOPE_VERIFIED|九组原unsigned/后状态/回滚、五组签名回执及同源码联验通过；[验证记录](integration/artifacts/local/core-pump-amm-preparation-validation-20261113.json)|
|68|[Pump原生SOL曲线执行](#plan-2026-11-13-pump-curve-execution)|LOCAL_SCOPE_VERIFIED|五组买卖原unsigned/完整后状态/回滚、原生扣款和到账/费用分列、真实测试签名与回执及同源码联验通过；[验证记录](integration/artifacts/local/core-pump-curve-preparation-validation-20261113.json)|
|69|[Pump原生收款ATA](#plan-2026-11-13-pump-native-ata)|LOCAL_SCOPE_VERIFIED|九组实际原wire/完整后状态/回滚、缺失/预存ATA租金补差、真实签名成功/失败回执和Chrome同源码联验通过；[验证记录](integration/artifacts/local/core-pump-native-ata-validation-20261113.json)|
|70|[PumpSwap账户与SOL资金](#plan-2026-11-13-pump-amm-funding)|LOCAL_SCOPE_VERIFIED|22组新增资金银行、真实租金/WSOL回执、44组原wire参考与72项Chrome同源码联验通过；[验证记录](integration/artifacts/local/core-pump-amm-funding-validation-20261113.json)|
|71|[Orca五模型精确输出](#plan-2026-11-14-orca-exact-out)|LOCAL_SCOPE_VERIFIED|600原manager场景/600服务流程、8序列、3组网格、101次Rust/91项Chrome同源码联验通过；[验证记录](integration/artifacts/local/core-orca-exact-out-validation-20261114.json)|
|72|[Raydium CLMM六模型精确输出](#plan-2026-11-14-rayclmm-exact-out)|LOCAL_SCOPE_VERIFIED|1266原生场景/服务流程、280单步、6序列/12网格、304次Rust/109项Chrome同源码联验通过；[验证记录](integration/artifacts/local/core-rayclmm-exact-out-validation-20261114.json)|
|73|[标准V3精确输出与回调路由](#plan-2026-11-14-standard-v3-exact-out)|LOCAL_SCOPE_VERIFIED|96实际原Pool场景/80预算控制、连续状态、4两跳/2网格及45次Rust/115项Chrome同源码联验通过；[验证记录](integration/artifacts/local/core-v3-exact-out-validation-20261114.json)|
|74|[Balancer不可变Weighted精确输出](#plan-2026-11-14-weighted-exact-out)|LOCAL_SCOPE_VERIFIED|304原GIVEN_OUT回调场景、48连续步、2组双池网格/2组同池顺序分配、33次Rust和12项Chrome；17项统一检查通过；验证记录（`integration/artifacts/local/weighted-exact-out-check/report.json`，已清理）|
|75|[Balancer标准Stable2精确输出](#plan-2026-11-15-stable-exact-out)|LOCAL_SCOPE_VERIFIED|720原数学场景/180步、3网格/3同池顺序分配、48次Rust/4项Chrome和20项统一检查通过；验证记录（`integration/artifacts/local/stable-exact-out-check/report.json`，已清理）|
|76|[零hook Singleton CL精确输出](#plan-2026-11-15-singleton-exact-out)|LOCAL_SCOPE_VERIFIED|644原始场景/24连续步、4容量网格、51次Rust/8项Chrome和20项统一检查通过；验证记录（`integration/artifacts/local/singleton-exact-out-check/report.json`，已清理）|
|77|[静态Infinity Bin精确输出](#plan-2026-11-15-infinity-bin-exact-out)|LOCAL_SCOPE_VERIFIED|264原场景/48burn、78连续/有序步、2网格、72次Rust/4项Chrome和20项统一检查通过；验证记录（`integration/artifacts/local/infinity-bin-exact-out-check/report.json`，已清理）|
|78|[Managed Snapshot精确输出](#plan-2026-11-15-managed-snapshot-exact-out)|LOCAL_SCOPE_VERIFIED|180目标/9实际状态、36连续/9有序步、3网格及3关闭swap，41次Rust/4项Chrome和20检查通过；验证记录（`integration/artifacts/local/managed-snapshot-exact-out-check/report.json`，已清理）|
|79|[Composable普通币对精确输出](#plan-2026-11-15-composable-exact-out)|LOCAL_SCOPE_VERIFIED|288原目标/216连续步、BPT首中末位置、3顺序/3网格、52次Rust/6项Chrome及20检查通过；验证记录（`integration/artifacts/local/composable-exact-out-check/report.json`，已清理）|
|80|[Raydium ATA与WSOL资金准备](#plan-2026-11-15-raydium-funding)|LOCAL_SCOPE_VERIFIED|10场景/20原交易、21检查/26次Rust/155项真实Chrome（40新增资金）通过；验证记录（`integration/artifacts/local/raydium-funding-check/report.json`，已清理）|
|81|[Orca经典SPL执行链路](#plan-2026-11-15-orca-execution)|LOCAL_SCOPE_VERIFIED|32场景API/独立SBF和重启回执、64原始交易、25检查/88次Rust/236项真实Chrome（81新增）通过；验证记录（`integration/artifacts/local/orca-execution-check/report.json`，已清理）|
|82|[Orca自适应SPL执行链路](#plan-2026-11-15-orca-adaptive-execution)|LOCAL_SCOPE_VERIFIED|54场景API/独立SBF和重启回执、108实际wire原交易、28检查/95次Rust/357项Chrome（121新增）通过；验证记录（`integration/artifacts/local/orca-adaptive-execution-check/report.json`，已清理）|
|83|[Raydium CLMM经典SPL执行](#plan-2026-11-16-rayclmm-execution)|LOCAL_SCOPE_VERIFIED|56场景API/同捕获SBF和重启回执、112原交易，31门禁/135次Rust/486项Chrome（129新增）通过；验证记录（`integration/artifacts/local/rayclmm-execution-check/report.json`，已清理）|
|84|[Raydium CLMM高级经典SPL执行](#plan-2026-11-16-rayclmm-advanced-execution)|LOCAL_SCOPE_VERIFIED|三个模型/三收费模式162API/162回执/324原交易，33门禁/306Rust/667Chrome同源码冻结通过；验证记录（`integration/artifacts/local/rayclmm-advanced-execution-check/report.json`，已清理）|
|85|[候选图增量索引与COW](#plan-2026-11-16-incremental-graph)|LOCAL_SCOPE_VERIFIED|8项门禁/249次Rust测试通过，源码和341份测试输入同批绑定；验证记录（`integration/artifacts/local/incremental-graph-check/report.json`，已清理）|
|86|[基线图复用与闲时缓存回收](#plan-2026-11-17-baseline-cache-reclamation)|LOCAL_SCOPE_VERIFIED|8项门禁/41次Rust测试，899份固定测试输入与46份运行时输入同批绑定；验证记录（`integration/artifacts/local/cache-reclamation-check/report.json`，已清理）|
|87|[EVM计划绑定与准备组件](#plan-2026-11-17-evm-preparation-component)|LOCAL_SCOPE_VERIFIED|13项门禁/62次Rust/95份独立ABI-RLP签名对照，899份固定输入/185份参考依赖绑定；验证记录（`integration/artifacts/local/evm-preparation-component-check/report.json`，已清理）|
|88|[EVM固定块RPC准备与API](#plan-2026-11-17-evm-rpc-preparation)|LOCAL_SCOPE_VERIFIED|15项门禁/51次Rust/12项Node/9项Chrome，899固定输入/46运行时输入同批绑定；验证记录（`integration/artifacts/local/evm-rpc-preparation-check/report.json`，已清理）|
|89|[EVM独立仿真动态费用](#plan-2026-11-17-evm-dynamic-fees)|LOCAL_SCOPE_VERIFIED|8门禁/117次Rust、16原Router对照、899固定输入；验证记录（`integration/artifacts/local/evm-dynamic-fees-check/report.json`，已清理）|
|90|[EVM准备组件与同银行独立执行](#plan-2026-11-17-evm-prepared-execution)|LOCAL_SCOPE_VERIFIED|8门禁/121次Rust、16原银行/25重算篡改/10控制、899固定输入；验证记录（`integration/artifacts/local/evm-prepared-execution-check/report.json`，已清理）|
|91|[EVM实际HTTP/worker动态费用](#plan-2026-11-17-evm-dynamic-fee-api)|LOCAL_SCOPE_VERIFIED|10门禁/122次Rust、实际HTTP/worker、899固定输入；验证记录（`integration/artifacts/local/evm-dynamic-fee-api-check/report.json`，已清理）|
|92|[EVM原Router单池ExactOut独立执行](#plan-2026-11-17-evm-exact-out-execution)|LOCAL_SCOPE_VERIFIED|8门禁/126次Rust、16原银行/3revert/有效多池拒绝、899固定输入；验证记录（`integration/artifacts/local/evm-exact-out-execution-check/report.json`，已清理）|
|93|[完整交付范围与外部验收边界审计](#plan-2026-11-17-design-delivery-audit)|LOCAL_SCOPE_VERIFIED|5检查/12控制、当前CLI正负对照/3份开发拓扑，确认外部验收输入缺失；审计记录（`integration/artifacts/local/design-delivery-audit/report.json`，已清理）|
|94|[EVM ExactOut准备与独立执行](#plan-2026-11-17-evm-prepared-exact-out)|LOCAL_SCOPE_VERIFIED|8门禁/129次Rust、16银行/16篡改/4控制；验证记录（`integration/artifacts/local/evm-prepared-exact-out-check/report.json`，已清理）|
|95|[EVM ExactOut正常HTTP独立模拟](#plan-2026-11-17-evm-exact-out-api)|LOCAL_SCOPE_VERIFIED|12门禁/151次Rust、实际双模式HTTP/worker及证据生命周期；验证记录（`integration/artifacts/local/evm-exact-out-api-check/report.json`，已清理）|
|96|[EVM多池ExactOut原交易执行](#plan-2026-11-17-evm-path-execution)|LOCAL_SCOPE_VERIFIED|11门禁/135次Rust、12原交易完整银行/6目标路径、901固定/97参考输入；验证记录（`integration/artifacts/local/evm-path-execution-check/report.json`，已清理）|
|97|[EVM ExactIn多池与双模式正常HTTP](#plan-2026-11-17-evm-input-paths)|LOCAL_SCOPE_VERIFIED|17门禁/168次Rust、6原输入路径/9变形及双模式3hop HTTP；验证记录（`integration/artifacts/local/evm-input-path-check/report.json`，已清理）|
|98|[EVM来源块与执行块费用上下文](#plan-2026-11-17-evm-source-fees)|LOCAL_SCOPE_VERIFIED|8门禁/134次Rust、16原H/H+1银行与双模式准备；验证记录（`integration/artifacts/local/evm-source-fees-check/report.json`，已清理）|
|99|[EVM缓存RPC准备到独立worker](#plan-2026-11-17-evm-prepared-api)|LOCAL_SCOPE_VERIFIED|15门禁/167次Rust、双模式真实HTTP/原准备wire/错误资金；验证记录（`integration/artifacts/local/evm-prepared-api-check/report.json`，已清理）|
|100|[EVM原准备独立执行界面](#plan-2026-11-17-evm-prepared-frontend)|LOCAL_SCOPE_VERIFIED|16门禁/168次Rust、13Node/18Chrome、wire摘要及移动布局；验证记录（`integration/artifacts/local/evm-prepared-frontend-check/report.json`，已清理）|
|101|[显式常驻分析worker](#plan-2026-11-17-analytics-worker)|LOCAL_SCOPE_VERIFIED|13门禁/97次Go/81次Rust、实际Go服务/独立Kafka连续消费与重启；验证记录（`integration/artifacts/local/analytics-worker-check/report.json`，已清理）|
|102|[公开Protobuf checkpoint](#plan-2026-11-17-public-bootstrap-checkpoints)|LOCAL_SCOPE_VERIFIED|13门禁/74次Go/42次Rust、3组原字节互通；验证记录（`integration/artifacts/local/public-bootstrap-check/report.json`，已清理）|
|103|[远端checkpoint对象读写](#plan-2026-11-17-remote-checkpoint-objects)|LOCAL_SCOPE_VERIFIED|13门禁/142次Go/125次Rust；验证记录（`integration/artifacts/local/remote-checkpoint-check/report.json`，已清理）|
|104|[当前远端checkpoint冷恢复](#plan-2026-11-17-current-remote-checkpoint-recovery)|LOCAL_SCOPE_VERIFIED|18门禁/143次Go/162次Rust、实际Kafka冷恢复/HTTP报价；验证记录（`integration/artifacts/local/checkpoint-recovery-check/report.json`，已清理）|
|105|[周期checkpoint发布](#plan-2026-11-18-checkpoint-worker)|LOCAL_SCOPE_VERIFIED|18门禁/148次Go/164次Rust；验证记录（`integration/artifacts/local/checkpoint-worker-check/report.json`，已清理）|
|106|[多链checkpoint公平调度](#plan-2026-11-18-checkpoint-fairness)|LOCAL_SCOPE_VERIFIED|7门禁/149次Go race、双链持续失败/更新/恢复；验证记录（`integration/artifacts/local/checkpoint-fairness-check/report.json`，已清理）|
|107|[旧checkpoint有限尾窗追平](#plan-2026-11-18-checkpoint-tail-replay)|LOCAL_SCOPE_VERIFIED|19门禁/152次Go/166次Rust、实际Kafka有限尾窗；验证记录（`integration/artifacts/local/checkpoint-replay-check/report.json`，已清理）|
|108|[Rust owner公开checkpoint](#plan-2026-11-18-rust-owner-checkpoints)|LOCAL_SCOPE_VERIFIED|15门禁/20次Go/60次Rust、正常HTTP与双向原wire；验证记录（`integration/artifacts/local/rust-checkpoint-publication-check/report.json`，已清理）|
|109|[Rust owner远端checkpoint](#plan-2026-11-18-rust-remote-checkpoints)|LOCAL_SCOPE_VERIFIED|16门禁/21次Go/66次Rust、正常双HTTP/Go解码；验证记录（`integration/artifacts/local/rust-remote-checkpoint-check/report.json`，已清理）|
|110|[Rust周期checkpoint与配置摘要](#plan-2026-11-18-rust-periodic-checkpoints)|LOCAL_SCOPE_VERIFIED|21门禁/92次Go/194次Rust、双链周期与共享hash；验证记录（`integration/artifacts/local/rust-periodic-checkpoint-check/report.json`，已清理）|
|111|[Rust恢复目录与独立router追平](#plan-2026-11-18-rust-owner-recovery)|LOCAL_SCOPE_VERIFIED|26门禁/93次Go/204次Rust、跨正常owner/router实际Kafka；验证记录（`integration/artifacts/local/rust-owner-recovery-check/report.json`，已清理）|
|112|[公开checkpoint本地容量门禁](#plan-2026-11-18-checkpoint-local-budgets)|LOCAL_SCOPE_VERIFIED|26门禁/97次Go/217次Rust、容量/实际Kafka回归；验证记录（`integration/artifacts/local/checkpoint-budget-check/report.json`，已清理）|
|113|[有界checkpoint只读保留计划](#plan-2026-11-18-checkpoint-retention-plan)|LOCAL_SCOPE_VERIFIED|26门禁/103次Go/221次Rust、只读计划及实际Kafka回归；验证记录（`integration/artifacts/local/checkpoint-retention-plan-check/report.json`，已清理）|
|114|[Rust只读保留计划与整计划互通](#plan-2026-11-18-rust-retention-plan)|LOCAL_SCOPE_VERIFIED|26门禁/104次Go/227次Rust、全计划同目录互通；验证记录（`integration/artifacts/local/rust-retention-plan-check/report.json`，已清理）|
|115|[显式Go checkpoint回收与中断恢复](#plan-2026-11-18-checkpoint-reclaim)|LOCAL_SCOPE_VERIFIED|27门禁/117次Go/231次Rust、真实进程中断/正常HTTP；验证记录（`integration/artifacts/local/checkpoint-reclaim-check/report.json`，已清理）|
|116|[Go周期checkpoint自动配额维护](#plan-2026-11-18-checkpoint-auto-reclaim)|LOCAL_SCOPE_VERIFIED|27门禁/127次Go/233次Rust、正常周期与实际Kafka；收尾差距已记录，本轮后停止；验证记录（`integration/artifacts/local/checkpoint-auto-reclaim-check/report.json`，已清理）|
|117|[v2-complete native Pair/Factory状态承诺](#plan-2026-11-18-v2-native-state-proof)|LOCAL_SCOPE_VERIFIED|8门禁/488次Go race；P0-1限定Pair/Factory状态证明通过；验证记录（`integration/artifacts/local/v2-native-state-proof-check/report.json`，已清理）|
|118|[v2-complete native Token余额证明](#plan-2026-11-18-v2-native-balance-proof)|LOCAL_SCOPE_VERIFIED|8门禁/77次Go race；P0-1限定余额view证明通过；验证记录（`integration/artifacts/local/v2-native-balance-proof-check/report.json`，已清理）|
|119|[EVM当前头确认策略与披露](#plan-2026-11-18-evm-confirmation-policy)|LOCAL_SCOPE_VERIFIED|19门禁/81次Go/97次Rust；P0-1当前头策略通过；验证记录（`integration/artifacts/local/confirmation-policy-check/report.json`，已清理）|
|120|[真实当前V2 normal owner/router与来源拒绝](#plan-2026-11-18-current-v2-public-source)|LOCAL_SCOPE_VERIFIED|13门禁/81次Go/36次Rust；P0-1实际公开当前state与恢复通过；验证记录（`integration/artifacts/local/current-v2-check/report.json`，已清理）|
|121|[真实V2固定bank独立离线夹具](#plan-2026-11-18-current-v2-fixed-fixture)|LOCAL_SCOPE_VERIFIED|13门禁/83次Go/36次Rust；真实bank固定离线通过；验证记录（`integration/artifacts/local/current-v2-fixed-check/report.json`，已清理）|
|122|[真实V2原字节码收款与完整后状态差分](#plan-2026-11-18-public-v2-original-execution)|LOCAL_SCOPE_VERIFIED|14门禁/16次Go/16次Rust；原VM actual收款/完整状态通过；验证记录（`integration/artifacts/local/public-v2-original-execution-check/report.json`，已清理）|
|123|[EVM原生费用模型与覆盖披露](#plan-2026-11-18-evm-native-cost-model)|LOCAL_SCOPE_VERIFIED|14门禁/142次Rust；P0-3原生费用与缺项披露通过；验证记录（`integration/artifacts/local/evm-native-cost-model-check/report.json`，已清理）|
|124|[EVM费用原生余额守恒与源绑定](#plan-2026-11-18-evm-native-balance-audit)|LOCAL_SCOPE_VERIFIED|14门禁/144次Rust；P0-3源绑定/alias守恒通过；验证记录（`integration/artifacts/local/evm-native-balance-audit-check/report.json`，已清理）|
|125|[EVM最大资金准入与付款人净扣款](#plan-2026-11-18-evm-native-payer-cost)|LOCAL_SCOPE_VERIFIED|14门禁/146次Rust，最大资金/净扣款与四HTTP通过；验证记录（`integration/artifacts/local/evm-native-payer-cost-check/report.json`，已清理）|
|126|[无可信成本时的路由排序回退](#plan-2026-11-18-cost-unavailable-ranking)|LOCAL_SCOPE_VERIFIED|16门禁/176次Rust与四HTTP通过；验证记录（`integration/artifacts/local/cost-unavailable-ranking-check/report.json`，已清理）|
|127|[Solana准备消息费用来源与原生资金分类](#plan-2026-11-18-solana-native-cost)|LOCAL_SCOPE_VERIFIED|14门禁/20 HTTP与SBF、独立审计及原资金/捕获/钱包回归通过；验证记录（`integration/artifacts/local/solana-native-cost-check/report.json`，已清理）|
|128|[Solana提交前费用上下文与耐久凭证](#plan-2026-11-18-solana-fee-preflight)|LOCAL_SCOPE_VERIFIED|11门禁/46次Rust，实际签名/资金/旧回执和冻结输入一致；验证记录（`integration/artifacts/local/solana-fee-preflight-check/report.json`，已清理）|
|129|[Solana原生同单位费用后净额](#plan-2026-11-18-solana-native-economics)|LOCAL_SCOPE_VERIFIED|16门禁/50次Rust，11场景SBF/实际签名回执与20准备家族冻结回归通过；验证记录（`integration/artifacts/local/solana-native-economics-check/report.json`，已清理）|
|130|[Solana费用后经济净额前端闭环](#plan-2026-11-18-native-economics-view)|LOCAL_SCOPE_VERIFIED|签名区/回执区分离；8门禁/16浏览器检查及实际可见截图复核通过；验证记录（`integration/artifacts/local/native-economics-view-check/report.json`，已清理）|
|131|[Solana失败签名费用与回滚核验](#plan-2026-11-18-solana-failure-fees)|LOCAL_SCOPE_VERIFIED|13门禁/3原程序失败/18故障/30独立审计篡改、成功交易回归通过；验证记录（`integration/artifacts/local/solana-failure-fee-check/report.json`，已清理）|
|132|[失败交易费用前端闭环](#plan-2026-11-18-failed-fee-view)|LOCAL_SCOPE_VERIFIED|9门禁/22前端单测/13实际浏览器检查及截图复核通过；验证记录（`integration/artifacts/local/failed-fee-view-check/report.json`，已清理）|
|133|[EVM Legacy原生费用来源与净扣款](#plan-2026-11-18-evm-legacy-native-fees)|LOCAL_SCOPE_VERIFIED|14门禁/148次Rust、8次HTTP和56独立整数向量通过；验证记录（`integration/artifacts/local/evm-legacy-native-fee-check/report.json`，已清理）|
|134|[OP Regolith历史L1数据费内核](#plan-2026-11-18-op-regolith-data-fee)|LOCAL_SCOPE_VERIFIED|16门禁/147次Rust、三笔原签名交易/72算术向量与8次正常worker回归通过；验证记录（`integration/artifacts/local/op-l1-data-cost-check/report.json`，已清理）|
|135|[OP Regolith同body费用输入来源与重放](#plan-2026-11-18-op-l1-source-inputs)|LOCAL_SCOPE_VERIFIED|18门禁/148次Rust/209次Go及三原signed跨语言核算通过；验证记录（`integration/artifacts/local/op-l1-source-inputs-check/report.json`，已清理）|
|136|[OP packed费用与FastLZ独立核算](#plan-2026-11-18-op-packed-cost)|LOCAL_SCOPE_VERIFIED|18门禁/150次Rust/6次Go及605原参考通过；验证记录（`integration/artifacts/local/op-packed-cost-check/report.json`，已清理）|
|137|[OP现代费用版本来源与升级首块重放](#plan-2026-11-18-op-packed-source-inputs)|LOCAL_SCOPE_VERIFIED|19门禁/153次Rust/214次Go，真实升级来源/归档重放及原参考跨语言通过；验证记录（`integration/artifacts/local/op-packed-source-inputs-check/report.json`，已清理）|
|138|[OP完整费用账户与原状态转换核验](#plan-2026-11-18-op-chain-cost)|LOCAL_SCOPE_VERIFIED|24门禁/151次Rust，320原Go场景955状态转换、256独立REVM与8原HTTP通过；验证记录（`integration/artifacts/local/op-chain-cost-check/report.json`，已清理）|
|139|[OP签名交易绑定与严格历史执行](#plan-2026-11-18-op-signed-execution)|LOCAL_SCOPE_VERIFIED|20门禁/160次Rust、448原场景1337转换/180归档签名及8原HTTP通过；验证记录（`integration/artifacts/local/op-execution-check/report.json`，已清理）|
|140|[OP签名BOUND完整Router执行与费用](#plan-2026-11-18-op-bound-router)|LOCAL_SCOPE_VERIFIED|18门禁/157次Rust、448原Router/896产品执行与完整声明状态/费用、8原HTTP通过；验证记录（`integration/artifacts/local/op-bound-check/report.json`，已清理）|
|141|[OP签名受限worker协议与实际子进程](#plan-2026-11-18-op-signed-worker)|LOCAL_SCOPE_VERIFIED|19门禁/159次Rust、56实际OP子进程/17拒绝与8正常Shanghai HTTP通过；验证记录（`integration/artifacts/local/op-worker-check/report.json`，已清理）|
|142|[OP费用回执的独立源核验](#plan-2026-11-18-op-fee-measurements)|LOCAL_SCOPE_VERIFIED|17门禁/160次Rust，896源费用核验与8正常HTTP通过；验证记录（`integration/artifacts/local/op-fee-measurements-check/report.json`，已清理）|
|143|[正常历史OP构建与签名模拟](#plan-2026-11-18-op-normal-api)|LOCAL_SCOPE_VERIFIED|19门禁/161次Rust、28正常OP HTTP/168拒绝及8旧Shanghai HTTP；验证记录（`integration/artifacts/local/op-api-check/report.json`，已清理）|
|144|[OP预编译加密后端与Isthmus限制](#plan-2026-11-18-op-precompiles)|LOCAL_SCOPE_VERIFIED|24门禁/165次Rust、3558原输入/110签名转换、28 OP及8旧HTTP；验证记录（`integration/artifacts/local/op-precompiles-check/report.json`，已清理）|
|145|[OP Mainnet历史Jovian用户执行](#plan-2026-11-18-op-jovian)|LOCAL_SCOPE_VERIFIED|25门禁/168次Rust、64原场景、7056输入/160签名转换、32 OP及8旧HTTP；验证记录（`integration/artifacts/local/op-jovian-check/report.json`，已清理）|
|146|[Karst预编译组件与固定现代原参考](#plan-2026-11-19-karst-precompiles)|LOCAL_SCOPE_VERIFIED|30门禁/171次Rust、9452原组件及32 OP/8旧HTTP；验证记录（`integration/artifacts/local/karst-precompiles-check/report.json`，已清理）|
|147|[Karst用户执行组件与完整原引擎](#plan-2026-11-19-karst-execution)|LOCAL_SCOPE_VERIFIED|35门禁/173次Rust、468原场景与历史API；验证记录（`integration/artifacts/local/karst-execution-check/report.json`，已清理）|
|148|[Karst正常本地签名API与完整原Router](#plan-2026-11-19-karst-api)|LOCAL_SCOPE_VERIFIED|39门禁/176次Rust、36正常OP/8旧HTTP与完整原Router通过；验证记录（`integration/artifacts/local/karst-api-check/report.json`，已清理）|
|149|[配置预言机授权与已执行交易费用换算](#plan-2026-11-19-native-valuation)|LOCAL_SCOPE_VERIFIED|51门禁/184次Rust/77次Go及61正常HTTP通过；验证记录（`integration/artifacts/local/native-valuation-check/report.json`，已清理）|
|150|[逐完整候选费用评分内核](#plan-2026-11-19-candidate-costs)|LOCAL_SCOPE_VERIFIED|54门禁/208次Rust/77次Go、22优化器/2 API回退与61 HTTP通过；验证记录（`integration/artifacts/local/candidate-cost-check/report.json`，已清理）|
|151|[正常报价本地逐候选执行费用接线](#plan-2026-11-19-normal-candidate-costs)|LOCAL_SCOPE_VERIFIED|59门禁/215次Rust/78次Go、16报价/30原候选重放及4 OP回退，旧61 HTTP通过；验证记录（`integration/artifacts/local/normal-candidate-cost-check/report.json`，已清理）|
|152|[批准Chainlink feed的真实来源适配](#plan-2026-11-19-approved-chainlink-feeds)|LOCAL_SCOPE_VERIFIED|68门禁/223次Rust/80次Go、8有效/14无效配置、48独立向量/148换算与公开原响应重放通过；验证记录（`integration/artifacts/local/approved-market-feed-check/report.json`，已清理）|
|153|[选中候选费用的正常报价同源披露](#plan-2026-11-19-selected-candidate-cost-disclosure)|LOCAL_SCOPE_VERIFIED|68门禁、226次Rust/80次Go通过；正常报价同源费用披露、未知回退及完整来源/历史回归通过|
|154|[授权价格book版本刷新与报价快照](#plan-2026-11-19-price-book-refresh)|LOCAL_SCOPE_VERIFIED|72门禁、229次Rust/80次Go通过；显式认证刷新/签名版本/快照缓存/旧报价失效及全部历史来源回归通过|
|155|[耐久价格发布journal与离线初始化](#plan-2026-11-19-price-publication-journal)|LOCAL_SCOPE_VERIFIED|冻结报告（`integration/artifacts/local/price-publication-journal-check/runs/5255594d26fa40fcb3e700601e15eb15/report.json`，已清理）：75门禁、6项日志测试、34个离线CLI案例；正常接线接续|
|156|[正常Router耐久价格发布与重启](#plan-2026-11-19-durable-price-router)|LOCAL_SCOPE_VERIFIED|冻结报告（`integration/artifacts/local/durable-price-router-check/runs/4ab92751086041e49f974d0a2d599007/report.json`，已清理）：原 75 项与新增 5 项全过，覆盖双配置摘要、确认丢失及多进程 HTTP|
|157|[整簿同块采集与独立签署输入](#market-book-signing-request)|LOCAL_SCOPE_VERIFIED|本地只读 RPC 与实际 Rust 验证器定向通过；不代表持续 live、签名授权或生产验收|
|158|[独立签署回执与离线候选簿](#market-book-offline-assembly)|LOCAL_SCOPE_VERIFIED|双币种端到端回放、6 项日志组件测试；预检不等于已发布|
|159|[在线条件发布与结果回读](#market-book-online-publication)|LOCAL_SCOPE_VERIFIED|显式日志模式，预检无写入，拒绝调包与过期条件；保持固定 bank 限定|
|160|[本机候选安装与结果核对](#market-book-operator)|LOCAL_SCOPE_VERIFIED|提交未送达与已提交但回执丢失分别核验，无自动重发、无自动回滚|
|161|[有界价格轮次调度](#market-round-scheduler)|LOCAL_SCOPE_VERIFIED|三项调度回放及原有真实 Router HTTP 回归通过，未验证生产 live 部署|
|162|[按 Router 区块锚点采集与复核](#market-router-bank-anchor)|LOCAL_SCOPE_VERIFIED|显式哈希选块、跨字段不一致拒绝及签后变化不发布|
|163|[共享池 V2 整笔拆单执行](#ordered-v2-split)|LOCAL_SCOPE_VERIFIED|ExactIn/ExactOut 两种腿序的实际整笔 Gas、余额、储备与回滚|
|164|[按完整拆单计划生成批量交易](#ordered-v2-binding)|LOCAL_SCOPE_VERIFIED|逐字节对照实际成交调用；拒绝沿用旧腿序金额或其他绑定|
|177|[WETH 包装计划和 worker 执行](#wrapped-input-worker)|LOCAL_SCOPE_VERIFIED|绑定包装本金和两笔交易环境|
|178|[正常报价中的 WETH 本金与费用](#wrapped-quote-funding)|LOCAL_SCOPE_VERIFIED|按候选核对最低原币余额和两笔网络费|
|179|[包装与授权组合回放](#wrapped-approval-replay)|LOCAL_SCOPE_VERIFIED|核对组合交易的中间状态及本金和网络费|
|180|[包装与授权的 worker 核验](#wrapped-approval-worker)|LOCAL_SCOPE_VERIFIED|逐笔绑定本金、额度与费率环境|
|181|[组合资金操作的报价核算](#combined-funding-quote)|LOCAL_SCOPE_VERIFIED|整组网络费参与候选比较|
|182|[按支出合约核对 WETH 授权](#weth-spender-allowance)|LOCAL_SCOPE_VERIFIED|验证单路线和拆单各自的源额度|
|183|[单路线包装与授权费用](#router-wrap-approval)|LOCAL_SCOPE_VERIFIED|将 Router02 的授权和包装费计入候选排序|
|184|[报价页的资金需求](#quote-funding-ui)|LOCAL_SCOPE_VERIFIED|以 wei 展示选中路线的付款人资金需求|
|185|[Ethereum 主网提款列表完整性](#ethereum-withdrawals-integrity)|LOCAL_SCOPE_VERIFIED|同一份历史主网数据经 RPC 读取及离线回放通过|
|186|[Ethereum 主网离线交易正文复核](#ethereum-offline-transaction-body)|LOCAL_SCOPE_VERIFIED|两个主网区块共 349 笔交易经部署证据 CLI 回放通过|
|187|[信标检查点与执行区块](#beacon-checkpoint-execution)|LOCAL_SCOPE_VERIFIED|标准 bootstrap 和既有部署证据经同一 CLI 验证|
|188|[同周期同步委员会最终性验证](#beacon-committee-finality)|LOCAL_SCOPE_VERIFIED|主网 finality_update 与执行层部署证明通过离线核对|
|189|[同步委员会跨期交接](#beacon-committee-transitions)|LOCAL_SCOPE_VERIFIED|两次连续主网交接绑定最终化区块 26448094|
|190|[从最终性证据选取执行块](#beacon-selected-capture)|LOCAL_SCOPE_VERIFIED|两次交接后完成主网 151 笔交易和 Pair 部署证明核对|
|191|[普通 owner 的信标最终性校验](#beacon-owner-admission)|LOCAL_SCOPE_VERIFIED|主网块 26448349 的快照在证明失效后撤权，恢复证明后重新发布|
|209|[OP Mainnet 提款字段与 Pair 证明](#op-mainnet-withdrawals)|LOCAL_SCOPE_VERIFIED|块 159769967 的 66 笔交易、回执根和 Pair 的七项存储证明通过离线复核|
|208|[主网连续发布及重放](#beacon-complete-window)|LOCAL_SCOPE_VERIFIED|主网连续发布 32 个增量，归档重算 33 个高度，32 个派生哈希全部匹配|
|207|[按方法选取账户证明来源](#account-proof-rpc-routing)|LOCAL_SCOPE_VERIFIED|普通 owner 主网基线和三来源归档核对，错误链、创世块和损坏证明回归通过|
|206|[后继配置重启](#beacon-rotation-switch)|LOCAL_SCOPE_VERIFIED|普通 owner 两次运行和基线记录复核通过|
|205|[信标检查点轮换](#beacon-checkpoint-rotation)|LOCAL_SCOPE_VERIFIED|配置根、时效、锁、取消和已有目标回归通过，实际主网证明轮换通过|
|204|[跨分叉委员会交接](#beacon-fork-handover)|LOCAL_SCOPE_VERIFIED|周期 1606 交接至 1607，错误版本、缺少交接、下一委员会分支篡改和历史检查点矛盾均拒绝|
|203|[已发布窗口重放](#owner-window-replay)|LOCAL_SCOPE_VERIFIED|从两个主网窗口的原始节点响应重建，核对每个已发布增量的池与兑换事实|
|202|[归档节点配置](#beacon-owner-execution-config)|LOCAL_SCOPE_VERIFIED|公共节点配置重读归档、来源不匹配、错误链和自定义来源脱敏检查通过|
|201|[池证明的节点身份](#pool-proof-endpoint)|LOCAL_SCOPE_VERIFIED|同源重复请求、换源错误身份和剩余预算通过专项 race 测试|
|200|[跨头发布归档](#beacon-owner-archive)|PARTIAL_LOCAL|583 条原始及派生记录通过封装核验，已发布 19 个高度的覆盖检查通过|
|199|[V2 池账户证明批次](#v2-verified-account-batch)|LOCAL_SCOPE_VERIFIED|账户重复读取、独立证明校验和失败会话回归通过|
|198|[普通读取的 RPC 重试](#owner-rpc-retry)|LOCAL_SCOPE_VERIFIED|启动配置及见证子客户端的重试行为通过|
|197|[原生交易原文与验证预算](#native-body-encoding-budget)|LOCAL_SCOPE_VERIFIED|主网对照及精确字节预算边界通过|
|196|[回执见证错误定位](#beacon-witness-diagnostics)|LOCAL_SCOPE_VERIFIED|回执缺失及五类原因通过回归，保留业务错误码|
|195|[主网回执见证预算](#beacon-witness-budget)|LOCAL_SCOPE_VERIFIED|双节点调用次数、缺失回执及主网预算复测|
|194|[主网连续发布验收](#beacon-owner-advance)|PARTIAL_LOCAL|首次实测定位到回执证明预算，保留失败记录|
|193|[主网短尾祖先块校验](#beacon-owner-ancestry)|LOCAL_SCOPE_VERIFIED|精确高度、父哈希及发布门禁回归通过|
|192|[信标证明刷新](#beacon-owner-refresh)|LOCAL_SCOPE_VERIFIED|主网更新命令及旧证明保留回归通过|
|176|[包装本金与兑换费用回放](#wrapped-input-replay)|LOCAL_SCOPE_VERIFIED|对照包装本金、两笔费用及全部采集状态|
|175|[付款人起始原币需求](#sequence-native-funding)|LOCAL_SCOPE_VERIFIED|按执行顺序计算最大预留资金需求|
|174|[正常报价计入清零费用](#reset-approval-quote)|LOCAL_SCOPE_VERIFIED|分别核算清零、准确授权与兑换费用|
|173|[拆单 worker 执行三笔交易](#reset-approval-worker)|LOCAL_SCOPE_VERIFIED|逐笔核对额度、状态与费用|
|172|[拆单授权替换计划](#reset-approval-plan)|LOCAL_SCOPE_VERIFIED|从绑定计划生成按序执行的授权操作|
|171|[非零不足授权的三笔交易回放](#reset-approval-replay)|LOCAL_SCOPE_VERIFIED|核对清零、精确授权与兑换的完整状态和费用|
|170|[正常报价计入调用者授权费](#approval-quote)|LOCAL_SCOPE_VERIFIED|按候选复核两笔交易的费用及价格有效期|
|169|[两笔交易的 worker 核验](#approval-worker)|LOCAL_SCOPE_VERIFIED|重建授权交易、逐笔核对状态和费用|
|168|[连续授权与兑换回放](#approval-sequence)|LOCAL_SCOPE_VERIFIED|从原始本地出块回执核对两笔费用、日志、账户与存储|
|167|[调用者授权费用判定](#caller-approval-cost)|LOCAL_SCOPE_VERIFIED|授权充足、授权不足和证据未知三种结果|
|166|[共享池拆单正常报价](#ordered-v2-quote)|LOCAL_SCOPE_VERIFIED|父进程回执绑定、候选费用排序及统一毛额回退|
|165|[共享池拆单独立仿真](#ordered-v2-worker)|LOCAL_SCOPE_VERIFIED|显式费用上限、原始回执及完整已采集状态对照、worker 核验|

<a id="plan-2026-10-27-raydium-clmm-static"></a>

## Raydium CLMM静态输入费与基础bitmap：任务完成情况

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|Task1|锁定源码和准入|WP03,WP20|LOCAL_SCOPE_VERIFIED|[rayclmm-golden-generation.json](integration/artifacts/local/rayclmm-golden-generation.json)<br>[raydium-clmm-static-quotes.md](docs/TECHNICAL_REFERENCE.md#raydium-clmm-static-quotes)|
|Task2|独立state/config/wire|WP02,WP25|LOCAL_SCOPE_VERIFIED|[rayclmm-differential.json](integration/artifacts/local/rayclmm-differential.json)<br>rayclmm-wire-generation.log（`integration/artifacts/local/rayclmm-wire-generation.log`，已清理）|
|Task3|实际Rust账户解析|WP04,WP05,WP20|LOCAL_SCOPE_VERIFIED|[rayclmm-differential.json](integration/artifacts/local/rayclmm-differential.json)<br>[rayclmm-three-service-smoke.json](integration/artifacts/local/rayclmm-three-service-smoke.json)|
|Task4|专属数学与原program参考|WP03,WP11,WP12,WP13,WP20|LOCAL_SCOPE_VERIFIED|[rayclmm-golden-generation.json](integration/artifacts/local/rayclmm-golden-generation.json)<br>[rayclmm-differential.json](integration/artifacts/local/rayclmm-differential.json)|
|Task5|集成/审查/回归/进度|WP01,WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP20,WP25|LOCAL_SCOPE_VERIFIED|[rayclmm-review.json](integration/artifacts/local/rayclmm-review.json)<br>[verification.json](integration/artifacts/local/verification.json)<br>[rayclmm-final-source.json](integration/artifacts/local/rayclmm-final-source.json)<br>[rayclmm-progress-consistency.json](integration/artifacts/local/rayclmm-progress-consistency.json)|

<a id="plan-2026-10-28-raydium-clmm-token-fee"></a>

## Raydium CLMM按Token0/Token1收费：任务完成情况

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|Task1|固定source与收费语义|WP03,WP20|LOCAL_SCOPE_VERIFIED|[rayclmm-token-fee-golden-generation.json](integration/artifacts/local/rayclmm-token-fee-golden-generation.json)<br>[rayclmm-token-fee-differential.json](integration/artifacts/local/rayclmm-token-fee-differential.json)|
|Task2|状态/配置/wire先红后绿|WP02,WP25|LOCAL_SCOPE_VERIFIED|[rayclmm-token-fee-golden-generation.json](integration/artifacts/local/rayclmm-token-fee-golden-generation.json)<br>[rayclmm-token-fee-differential.json](integration/artifacts/local/rayclmm-token-fee-differential.json)|
|Task3|实际RPC解析先红后绿|WP04,WP05,WP20|LOCAL_SCOPE_VERIFIED|[rayclmm-token-fee-golden-generation.json](integration/artifacts/local/rayclmm-token-fee-golden-generation.json)<br>[rayclmm-token-fee-differential.json](integration/artifacts/local/rayclmm-token-fee-differential.json)|
|Task4|专属方向数学与独立原program参考先红后绿|WP03,WP11,WP12,WP13,WP20|LOCAL_SCOPE_VERIFIED|[rayclmm-token-fee-golden-generation.json](integration/artifacts/local/rayclmm-token-fee-golden-generation.json)<br>[rayclmm-token-fee-differential.json](integration/artifacts/local/rayclmm-token-fee-differential.json)|
|Task5|进程/审查/完整回归与逐项进度|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP20,WP25|LOCAL_SCOPE_VERIFIED|[rayclmm-token-fee-golden-generation.json](integration/artifacts/local/rayclmm-token-fee-golden-generation.json)<br>[rayclmm-token-fee-differential.json](integration/artifacts/local/rayclmm-token-fee-differential.json)<br>[rayclmm-token-fee-three-service-smoke.json](integration/artifacts/local/rayclmm-token-fee-three-service-smoke.json)<br>[rayclmm-token-fee-review.json](integration/artifacts/local/rayclmm-token-fee-review.json)<br>[rayclmm-token-fee-final-source.json](integration/artifacts/local/rayclmm-token-fee-final-source.json)<br>[rayclmm-token-fee-progress-consistency.json](integration/artifacts/local/rayclmm-token-fee-progress-consistency.json)|

<a id="plan-2026-10-28-raydium-clmm-bitmap"></a>

## Raydium CLMM扩展bitmap解析与计算：任务完成情况

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|Task1|固定原source与索引语义|WP03,WP20|LOCAL_SCOPE_VERIFIED|[rayclmm-bitmap-golden-generation.json](integration/artifacts/local/rayclmm-bitmap-golden-generation.json)<br>[rayclmm-bitmap-differential.json](integration/artifacts/local/rayclmm-bitmap-differential.json)|
|Task2|跨语言状态/config/wire先红后绿|WP02,WP25|LOCAL_SCOPE_VERIFIED|[rayclmm-bitmap-golden-generation.json](integration/artifacts/local/rayclmm-bitmap-golden-generation.json)<br>[rayclmm-bitmap-differential.json](integration/artifacts/local/rayclmm-bitmap-differential.json)<br>rayclmm-bitmap-wire.log（`integration/artifacts/local/rayclmm-bitmap-wire.log`，已清理）|
|Task3|真实账户与RPC先红后绿|WP04,WP05,WP20|LOCAL_SCOPE_VERIFIED|[rayclmm-bitmap-golden-generation.json](integration/artifacts/local/rayclmm-bitmap-golden-generation.json)<br>[rayclmm-bitmap-differential.json](integration/artifacts/local/rayclmm-bitmap-differential.json)<br>rayclmm-bitmap-parser-main.log（`integration/artifacts/local/rayclmm-bitmap-parser-main.log`，已清理）|
|Task4|跨bitmap数学与原program参考先红后绿|WP03,WP11,WP12,WP13,WP20|LOCAL_SCOPE_VERIFIED|[rayclmm-bitmap-golden-generation.json](integration/artifacts/local/rayclmm-bitmap-golden-generation.json)<br>[rayclmm-bitmap-differential.json](integration/artifacts/local/rayclmm-bitmap-differential.json)|
|Task5|集成/审查/完整回归/逐项进度|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP20,WP25|LOCAL_SCOPE_VERIFIED|[rayclmm-bitmap-golden-generation.json](integration/artifacts/local/rayclmm-bitmap-golden-generation.json)<br>[rayclmm-bitmap-differential.json](integration/artifacts/local/rayclmm-bitmap-differential.json)<br>[rayclmm-bitmap-three-service-smoke.json](integration/artifacts/local/rayclmm-bitmap-three-service-smoke.json)<br>[rayclmm-bitmap-review.json](integration/artifacts/local/rayclmm-bitmap-review.json)<br>[rayclmm-bitmap-final-source.json](integration/artifacts/local/rayclmm-bitmap-final-source.json)<br>[rayclmm-bitmap-progress-consistency.json](integration/artifacts/local/rayclmm-bitmap-progress-consistency.json)|

<a id="plan-2026-10-29-raydium-clmm-custom-seed"></a>

## Raydium CLMM非零seed独立池身份：任务完成情况

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|Task1|固定原seed语义|WP03,WP20|LOCAL_SCOPE_VERIFIED|[rayclmm-seed-native-source.json](integration/artifacts/local/rayclmm-seed-native-source.json)<br>[raydium-clmm-custom-seed-quotes.md](docs/TECHNICAL_REFERENCE.md#raydium-clmm-custom-seed-quotes)|
|Task2|跨语言state/wire/config先红后绿|WP02,WP25|LOCAL_SCOPE_VERIFIED|rayclmm-seed-go-green.log（`integration/artifacts/local/rayclmm-seed-go-green.log`，已清理）<br>rayclmm-seed-wire.log（`integration/artifacts/local/rayclmm-seed-wire.log`，已清理）<br>[rayclmm-seed-differential.json](integration/artifacts/local/rayclmm-seed-differential.json)|
|Task3|实际账户/RPC先红后绿|WP04,WP05,WP20|LOCAL_SCOPE_VERIFIED|rayclmm-seed-parser-main.log（`integration/artifacts/local/rayclmm-seed-parser-main.log`，已清理）<br>[rayclmm-seed-three-service-smoke.json](integration/artifacts/local/rayclmm-seed-three-service-smoke.json)|
|Task4|原native金额与共享池|WP03,WP11,WP12,WP13,WP20|LOCAL_SCOPE_VERIFIED|[rayclmm-seed-native-source.json](integration/artifacts/local/rayclmm-seed-native-source.json)<br>[rayclmm-seed-differential.json](integration/artifacts/local/rayclmm-seed-differential.json)|
|Task5|进程/审查/完整回归/逐项进度|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP20,WP25|LOCAL_SCOPE_VERIFIED|[rayclmm-seed-three-service-smoke.json](integration/artifacts/local/rayclmm-seed-three-service-smoke.json)<br>[rayclmm-seed-review.json](integration/artifacts/local/rayclmm-seed-review.json)<br>[rayclmm-seed-final-source.json](integration/artifacts/local/rayclmm-seed-final-source.json)<br>[rayclmm-seed-progress-consistency.json](integration/artifacts/local/rayclmm-seed-progress-consistency.json)|

<a id="plan-2026-10-29-raydium-clmm-dynamic"></a>

## Raydium CLMM动态费与分段精确计算：任务完成情况

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|Task1|固定原动态费语义|WP03,WP20|LOCAL_SCOPE_VERIFIED|[rayclmm-dynamic-native-source.json](integration/artifacts/local/rayclmm-dynamic-native-source.json)<br>[raydium-clmm-dynamic-quotes.md](docs/TECHNICAL_REFERENCE.md#raydium-clmm-dynamic-quotes)|
|Task2|跨语言state/wire/config先红后绿|WP02,WP25|LOCAL_SCOPE_VERIFIED|rayclmm-dynamic-go-green.log（`integration/artifacts/local/rayclmm-dynamic-go-green.log`，已清理）<br>rayclmm-dynamic-wire.log（`integration/artifacts/local/rayclmm-dynamic-wire.log`，已清理）<br>[rayclmm-dynamic-differential.json](integration/artifacts/local/rayclmm-dynamic-differential.json)|
|Task3|实际账户/RPC先红后绿|WP04,WP05,WP20|LOCAL_SCOPE_VERIFIED|rayclmm-dynamic-parser-main.log（`integration/artifacts/local/rayclmm-dynamic-parser-main.log`，已清理）<br>[rayclmm-dynamic-three-service-smoke.json](integration/artifacts/local/rayclmm-dynamic-three-service-smoke.json)|
|Task4|独立动态数学与原program参考先红后绿|WP03,WP11,WP12,WP13,WP20|LOCAL_SCOPE_VERIFIED|[rayclmm-dynamic-native-source.json](integration/artifacts/local/rayclmm-dynamic-native-source.json)<br>[rayclmm-dynamic-differential.json](integration/artifacts/local/rayclmm-dynamic-differential.json)|
|Task5|进程/审查/完整回归/逐项进度|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP20,WP25|LOCAL_SCOPE_VERIFIED|[rayclmm-dynamic-three-service-smoke.json](integration/artifacts/local/rayclmm-dynamic-three-service-smoke.json)<br>[rayclmm-dynamic-review.json](integration/artifacts/local/rayclmm-dynamic-review.json)<br>rayclmm-dynamic-clock-red.log（`integration/artifacts/local/rayclmm-dynamic-clock-red.log`，已清理）<br>rayclmm-dynamic-clock-green.log（`integration/artifacts/local/rayclmm-dynamic-clock-green.log`，已清理）<br>rayclmm-dynamic-resume-green.log（`integration/artifacts/local/rayclmm-dynamic-resume-green.log`，已清理）<br>[full-local-acceptance-resume.json](integration/artifacts/local/full-local-acceptance-resume.json)<br>[raydium-lease-recovery-audit.json](integration/artifacts/local/raydium-lease-recovery-audit.json)<br>[rayclmm-dynamic-final-source.json](integration/artifacts/local/rayclmm-dynamic-final-source.json)<br>[rayclmm-dynamic-progress-consistency.json](integration/artifacts/local/rayclmm-dynamic-progress-consistency.json)|

<a id="plan-2026-10-30-balancer-weighted2-equal"></a>

## Balancer V2双币不可变50/50权重池：任务完成情况

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|Task1|固定原源码、构造器/onSwap与Vault余额参考；限定无managed资金、标准非BPT代币|WP03,WP11|LOCAL_SCOPE_VERIFIED|[源码清单](integration/tools/reference/weighted-model/source-metadata.json)<br>[原合约参考](integration/artifacts/local/weighted-golden-generation.json)|
|Task2|独立WeightedState/Config、版本准入和protobuf field27；三服务契约|WP02,WP25|LOCAL_SCOPE_VERIFIED|[三服务及公共协议完整检查](integration/artifacts/local/verification.json)|
|Task3|固定blockHash读取pool/Vault/weights/scales/decimals/tokeninfo；保留receipt根和读后锚定|WP04,WP05,WP20|LOCAL_SCOPE_VERIFIED|解析与撤销回归（`integration/artifacts/local/weighted-review-green.log`，已清理）<br>[双副本进程](integration/artifacts/local/weighted-three-service-smoke.json)|
|Task4|原始扣费/缩放/舍入/净输入30%上限/uint256中间值与uint112后余额；共享池连续兑换|WP11,WP12,WP13|LOCAL_SCOPE_VERIFIED|765案例及45组180步差分（`integration/artifacts/local/weighted-math-differential.log`，已清理）<br>分配重算测试（`integration/artifacts/local/weighted-routing-tests.log`，已清理）|
|Task5|PG/Kafka双副本、完整scope/重组/撤销恢复、独立审查、三服务检查与统一进度|WP01,WP03,WP09,WP14,WP16,WP25|LOCAL_SCOPE_VERIFIED|[17项进程检查](integration/artifacts/local/weighted-three-service-smoke.json)<br>[审查关闭](integration/artifacts/local/weighted-review.json)<br>[本批验收](integration/artifacts/local/weighted-acceptance.json)|

边界：仅指定源码域的双币不可变50/50模型，固定runtime/代币code哈希与`standard-no-bpt`为配置方准入声明；尚无真实部署/immutable编译产物映射或特殊代币行为证明。原合约参考执行构造器与onSwap及余额打包，Vault facade不执行完整Vault/真实ERC20转账。其他权重、Managed/Composable/BPT/RateProvider/V3、Weighted成交事实分析、独立执行/真实收款及VERIFIED均未实现。本批只推进R06有限模型，不改变原26工作包完整验收为0。

验证结果：三服务与common/protoc完整检查通过，含五个实际执行文件的构建指纹；91份契约与8份fixture副本一致。765参考案例、45组180步连续兑换逐步比较输出及双侧余额；正常Go RPC→PG/Kafka→双路由17项检查、基础与原稳定池两组当前回归通过。新增20项验收辅助、全部91项辅助及31项文档映射测试通过。SPEC与独立质量审查问题已修复，保留失败/通过回归；普通模式和`PYTHONOPTIMIZE=2`证据门禁均通过。

<a id="plan-2026-10-30-balancer-weighted2-static"></a>

## Balancer V2双币不可变任意合法权重：任务完成情况

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|Task1|固定同版本原WeightedPool/LogExpMath/FixedPoint参考；新独立版本与旧50/50边界|WP03,WP11|LOCAL_SCOPE_VERIFIED|[固定40原文件](integration/tools/reference/weighted-model/source-metadata.json)<br>[原构造器/onSwap及pow执行参考](integration/artifacts/local/weighted-static-golden-generation.json)|
|Task2|独立static profile与现有Weighted字段复用；最小权重、归一化总和及Go/Rust/wire一致准入|WP02,WP25|LOCAL_SCOPE_VERIFIED|[三服务及公共协议完整检查](integration/artifacts/local/verification.json)<br>规范adapter-set身份回归（`integration/artifacts/local/weighted-static-adapter-hash-green.log`，已清理）|
|Task3|同blockHash双币权重解析；保留Vault/token/cash/manager/receipt及重锚定屏障|WP04,WP05,WP20|LOCAL_SCOPE_VERIFIED|双profile固定块准入（`integration/artifacts/local/weighted-static-go-contract-parser.log`，已清理）<br>[六池双副本进程](integration/artifacts/local/weighted-static-three-service-smoke.json)|
|Task4|整数LogExpMath和powUp误差界、权重比向下舍入、完整raw后余额与原onSwap连续差分|WP11,WP12,WP13|LOCAL_SCOPE_VERIFIED|5355案例及315组1260步差分（`integration/artifacts/local/weighted-static-differential.log`，已清理）<br>240组原pow向量（`integration/artifacts/local/weighted-static-power-differential.log`，已清理）|
|Task5|当前PG/Kafka双副本集成、旧模型回归、两阶段审查及失败封闭证据和统一进度|WP01,WP03,WP09,WP14,WP16,WP25|LOCAL_SCOPE_VERIFIED|[18项进程检查](integration/artifacts/local/weighted-static-three-service-smoke.json)<br>[两阶段审查](integration/artifacts/local/weighted-static-review.json)<br>[当前源码验收](integration/artifacts/local/weighted-static-acceptance.json)|

范围：同一具体不可变双币WeightedPool源码域，两个权重各至少1%，归一化总和为1e18；新版本`balancer-v2-weighted2-static`，旧equal版本保持仅50/50。复用原有独立Weighted状态类型，不混用Stable/V2公式。多币、Managed/Composable/BPT/RateProvider/V3、真实部署/Token行为和执行验证继续单列未完成；进度只在本文件更新。


验证结果（2026-10-31）：5355原onSwap案例、315组1260步完整后余额与240原pow向量全部通过；新static profile和旧equal profile双向准入/适配版本矩阵、独立field27、固定区块/receipt屏障、共享池分配重算及执行拒绝测试通过。六个不同权重/精度池的PG/Kafka双路由18项检查完成，基础/稳定池/旧等权/新任意权重四组当前进程报告全部通过。三服务与common/protoc完整检查、99份契约及8份fixture副本一致性、107项验收辅助和31项文档测试通过；SPEC与独立质量审查问题关闭。新wire adapter-set去重和可调用回归失败封闭均保留失败/通过回归。证据绑定实际五个构建文件及执行前后输入；普通和PYTHONOPTIMIZE=2门禁通过。旧三个canonical fixture摘要不变。

<a id="plan-2026-10-31-five-protocol-families"></a>

## 五组协议剩余能力：联合开发与测试

本轮目标：按用户要求同时推进Balancer多币及Managed/Composable、Curve其他模型、Solana高级状态、V4/Infinity扩展。各组按固定实际源码版本分别验收，已实现模型与未知/未覆盖模型分开记录；不以注册profile、放开拒绝分支或重用V2/V3公式代替功能实现。任意自定义hook/外部rate/Token扩展只有具备相应状态与实际执行证据才可准入。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|Task1|核对版本矩阵与准入，新增多资产物理池状态、wire/config、全token边与共享后状态|WP02,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|[原设计](XONE_SYSTEM_DESIGN.md)|
|Task2|Balancer多币Weighted、Managed当前权重/费用及Composable/BPT/rate的实际状态和数学|WP03,WP04,WP05,WP11|PARTIAL_LOCAL|[现有Weighted原始参考](integration/tools/reference/weighted-model/source-metadata.json)|
|Task3|Curve Plain多币、Meta/Lending、NG及Crypto各独立模型/精度/费用/后状态与原程序参考|WP03,WP04,WP05,WP11|PARTIAL_LOCAL|[既有Stable参考](integration/tools/reference/stable-models/source-metadata.json)|
|Task4|Solana奖励/动态费/Token2022等高级账户和完整后状态，按实际program版本验证|WP02,WP03,WP05,WP11,WP20,WP25|PARTIAL_LOCAL|[原设计](XONE_SYSTEM_DESIGN.md)|
|Task5|V4/Infinity动态费/native/hook/自定义会计/Bin按实际manager版本与执行证据准入|WP02,WP03,WP05,WP11,WP14|PARTIAL_LOCAL|[原设计](XONE_SYSTEM_DESIGN.md)|
|Task6|五组集成、旧协议回归、固定原合约差分、三服务检查、失败封闭证据、独立审查与统一进度|WP01,WP03,WP09,WP11,WP12,WP13,WP14,WP16,WP20,WP25|PARTIAL_LOCAL|[统一进度](DEVELOPMENT_PROGRESS.md)|
实施顺序：先写失败测试，补模型/状态与原版本参考，接实际读取和纯计算，再逐项对照原设计验证。公共schema/路由由主任务统一修改，各协议工作仅修改各自文件，避免并行覆盖。三个服务仍分别独立；公共协议在common/protoc；进度只更新本文件。当前仅完成注明子集的开发和本地验收，五组整体验收仍未完成，不改变VERIFIED与生产准入状态。

当前组件结果：多币物理状态/全代币边/固定区块依赖测试通过；Balancer固定原参考648 Weighted案例、588 Managed快照案例、756 Composable regular及504 BPT数学案例通过（Composable为原StableMath内核，非完整callback）；Curve固定原exchange共1530案例，NG/Crypto仍有后状态分支边界；Orca/Raydium奖励原native完整后状态与拒绝/增长内核已验证；V4/Infinity高级CL 240例与单Bin180例通过。新wire在独立protoc回归中暴露并修复Go未初始化奖励Pubkey哨兵与高级singleton wire校验路径，当前子集正式全量检查与六组进程回归通过，五组全部功能仍未完成。

该历史批次当时的完整剩余（后续NG/Meta/Crypto/Orca/Bin限定完成范围见各独立计划）：Managed非零协议/管理费导致的BPT供应更新；Composable BPT路由、待结协议费/虚拟实际供应量、yield/rate-cache刷新；Meta underlying和旧Lending存取款；NG EMA与特殊Token余额、Crypto价格调整/claim/gulp；Token2022、Orca adaptive、Raydium订单/奖励领取及高级组合；任意swap hook/自定义会计执行、Infinity Bin manager/tree/跨Bin、native结算。上述均不能记为本轮已完成，不以纯数学原函数参考替代完整执行链路。

验证结论（2026-10-31）：三服务与公共协议完整检查、122份契约和8份fixture副本一致性通过；119项验收辅助和31项文档映射测试通过。当前源码执行六组正式进程回归：基础、既有权重、六模型EVM多币、Orca奖励、Raydium奖励、既有singleton。六模型正常RPC/PG/Kafka/双路由6项检查通过；两奖励模型各5项检查，含原native实时完整后Pool序列、packed账户重解析、错误奖励撤销和副本恢复。V4/Infinity正常高级adapter读取、240 CL/180 Bin原参考、wire/hash/配置拒绝和跨语言回归通过。独立审查所提多币拓扑、配置能力、外层blockhash、容量探测问题均保留先失败再通过的回归；普通与PYTHONOPTIMIZE=2门禁通过。原程序/native参考不等于SBF/Token程序或真实部署执行。

以上联合批次为2026-10-31的历史范围；其中标准NG的完整状态、EMA与服务接入已由后续[NG批次](#plan-2026-11-01-curve-ng-full-state)补齐，特殊Token及其他家族剩余项继续保留。

本轮请求完成情况：**未全部完成**。本计划与本批状态保持PARTIAL_LOCAL，门禁明确all_requested_features_complete=false；以上“通过”只覆盖已实现的限定子集。不得把这些结果写成Balancer Managed/Composable全部、Curve其他模型全部、Solana高级状态全部或V4/Infinity全部完成。生产/VERIFIED保持关闭，原26工作包完整验收仍为0。

<a id="plan-2026-11-01-curve-ng-full-state"></a>

## Curve NG固定区块服务接入与完整交换后状态

目标：补齐R06中已有NG数学到正常Go RPC、统一状态/wire、路由与连续兑换的链路，并补充价格/D预言机EMA和原始/admin余额后状态。固定curvefi/stableswap-ng revision3332cd44656ec64b3c048885e1dd71955254b262与Vyper0.3.10；本批限定2至8币、标准asset type0及固定精度，其他Token/rate-provider能力保留独立未完成。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|Task1|固定原源码/编译布局/immutable资产类型证据；新增独立NG profile及严格跨语言状态|WP02,WP03,WP25|LOCAL_SCOPE_VERIFIED|[原编译布局](integration/tools/reference/curve-ng-state/compiler-layout.json)|
|Task2|单blockHash读取全部资产、stored/admin/effective余额、A/fee/offpeg及EMA字段，保留receipt与读后锚定|WP04,WP05,WP11|LOCAL_SCOPE_VERIFIED|[固定区块NG读取](chain-parser-go/internal/chain/evm/curve_ng.go)|
|Task3|原NG交换、packed价格/D与时间、原signed exp和EMA，完整后状态及共享池序列|WP03,WP11,WP12,WP13|LOCAL_SCOPE_VERIFIED|[既有NG内核](dex-router/crates/protocol-math/src/curve_ng.rs)|
|Task4|独立protoc/原Vyper完整exchange差分与异常拒绝；正常RPC→PG/Kafka→双副本恢复及旧模型回归|WP02,WP03,WP09,WP11,WP13,WP25|LOCAL_SCOPE_VERIFIED|[NG双副本进程回归](integration/artifacts/local/curve-ng-three-service-smoke.json)|
|Task5|三服务及common完整检查、SPEC/独立质量审查、失败封闭证据与统一进度|WP01,WP03,WP11,WP14,WP16,WP25|LOCAL_SCOPE_VERIFIED|[三服务与公共协议检查](integration/artifacts/local/verification.json)|

验证结论（2026-11-01）：三服务与common完整检查、130份契约及8份fixture副本、127项验收辅助和31项CI/文档测试通过。固定原Vyper NG参考21场景147次交换（144成功、3回退），另有44项exp、10项价格与6项D探针；56个标准服务场景在双副本共112次报价，实时完整protobuf、四类scope撤销/更高generation恢复、无关Solana保活及路由重启通过。最终源码和五个实际二进制绑定后，基础、六模型多币、NG七池三组正式进程回归重新执行并封存。SPEC与独立质量审查关闭；报告/审查凭证并发替换与独立测试入口问题已修复；28项新增门禁测试及最终验收在普通/PYTHONOPTIMIZE=2下通过。

本计划五项任务已在注明的标准NG范围内完成本地验收；全部剩余功能尚未完成。旧Plain/Meta/NG数学语料与旧profile边界保持原有范围。NG实际部署与特殊Token行为仍需额外验证；普通精确输入报价保持INDICATIVE，独立交易执行/VERIFIED仍关闭。其余五族未完成项继续在R05/R06/R08中逐项保留。

<a id="plan-2026-11-01-managed-bpt-fees"></a>

## Managed非零BPT费用完整交换状态（原回调参考与服务准入限制）

原计划目标：补齐R06 / §7.3、§11.3、§12.2/12.9/12.11/12.14与WP02–05、WP09、WP11–13、WP25的Managed普通精确输入交换。固定balancer-v2-monorepo revision e281ff40583c39bf1d46c322334b2b13936e0e33；新增独立费用profile，保留旧零费用profile的拒绝边界。完整记录BPT供应量、协议/管理费比例和收款方余额，按原回调计算铸币并在共享池连续兑换中传递后状态；不将join/exit、BPT路由或AUM收费操作记为完成。

范围修正（2026-11-01，原源码/编译证据）：该revision的原Managed runtime在0.7.6与上游0.7.1、已核对的optimizer设置下均超过EIP170大小上限；普通EVM部署已实际失败。原参考仅在显式解除大小限制的隔离环境执行，不能据此开放在线profile。保留完整数学、BPT状态/wire契约及固定区块依赖读取组件；配置与正常parser显式拒绝MANAGED_FEE_RUNTIME_UNAVAILABLE，报价分派不开放。正常RPC/PG/Kafka闭环待取得可部署实际版本后继续，本批不得记为全部完成。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|Task1|固定原合约/编译布局/回调依据；独立profile与完整BPT会计、来源/wire/config及旧版本隔离|WP02,WP03,WP25|PARTIAL_LOCAL|[原Managed合约](integration/tools/reference/balancer-multi/sources/e281ff40583c39bf1d46c322334b2b13936e0e33/pkg/pool-weighted/contracts/smart/ManagedPool.sol)|
|Task2|同blockHash读取Vault全币现金、权重/费用、BPT供应与两类收款方余额及源码身份，保留receipt和重锚定|WP04,WP05,WP11|NOT_IMPLEMENTED|[现有多资产读取](chain-parser-go/internal/chain/evm/multi_asset.go)|
|Task3|原powDown不变量及BPT费用舍入、完整余额/供应/收款方后状态、独立内核连续重放（正常共享池路由未接）|WP03,WP11,WP12,WP13|LOCAL_SCOPE_VERIFIED|[现有Managed数学](dex-router/crates/protocol-math/src/balancer_multi.rs)|
|Task4|实际初始化原ManagedPool并执行onSwap；独立protobuf、异常拒绝与正常RPC/PG/Kafka双副本及恢复|WP02,WP03,WP09,WP11,WP13|PARTIAL_LOCAL|[原回调参考设施](integration/tools/reference/balancer-multi/WeightedHarness.sol)|
|Task5|三服务/common全检查、旧流程回归、SPEC/独立质量审查、失败封闭验收与统一进度|WP01,WP03,WP11,WP14,WP16,WP25|LOCAL_SCOPE_VERIFIED|[有限参考验收](integration/artifacts/local/managed-reference-acceptance.json)|

验证结论（2026-11-01）：三服务/common完整检查、134份契约副本与8份fixture一致性通过；原Managed参考357案例、30组连续序列120步及2个回退guard，合计395次成功观察/82次预期回退。新数学目标8项、旧Managed/Weighted语料与独立测试入口回归、跨语言状态/配置/wire/source/clone及固定区块BPT读取组件通过。135项验收辅助、31项CI/文档、10项生成/来源故障测试及8项新验收/准入测试通过，相关故障与最终门禁在优化模式下仍有效。最终源码与5个实际二进制绑定后，基础、六模型多币、NG七池三组既有服务流程重新执行并封存；这些仅验证兼容性，不计作新Managed服务验收。SPEC与独立质量审查关闭，余额合计越界和验收工具来源分类问题已保留先失败再通过的回归。

本批结论：PARTIAL_LOCAL，全部剩余功能尚未完成。数学参考与状态契约已在注明范围验证，正常Managed服务接入未完成。原26工作包完整验收仍为0，真实部署/独立交易执行及VERIFIED/生产准入保持原有未完成状态；进度仅更新本文件。

<a id="plan-2026-11-01-metausd-direct"></a>

## MetaUSD直接兑换服务闭环

目标：按R06 / §7.3、§11.3、§12.2/12.9/12.11/12.14与WP02–05、WP09、WP11–13、WP25，将已有Meta直接数学接入正常服务。固定curvefi/curve-factory revision5272fd2b1199f185d9cd94822be9262c387dc2c2的MetaUSD，Vyper0.3.1运行码20060字节。新独立版本仅准入标准meta代币/固定3CRV LP、EIP1167初始化池，完整读取base3pool虚拟价格与余额/admin会计。base3pool作为只读依赖，快照/图/重算入口拒绝其同时成为可变物理池，并拒绝同源依赖冲突；不开放exchange_underlying或基础池deposit/withdraw。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|Task1|固定原源码/编译模板/clone与initializer；独立profile、完整Meta状态/wire/config及旧版本隔离|WP02,WP03,WP25|LOCAL_SCOPE_VERIFIED|[固定Meta源码](integration/tools/reference/curve-models/old/MetaUSD.vy)|
|Task2|同blockHash核验实现/Token/basepool代码、币序/精度/VP、stored/admin/effective余额、A/fee与来源身份；保留receipt及重锚定|WP04,WP05,WP11|LOCAL_SCOPE_VERIFIED|[固定区块Meta解析](chain-parser-go/internal/chain/evm/curve_meta.go)|
|Task3|直接coin/LP双向精确输入、完整余额与admin后状态，依赖隔离和共享池连续重算|WP03,WP11,WP12,WP13|LOCAL_SCOPE_VERIFIED|[已有Meta内核](dex-router/crates/protocol-math/src/curve_multi.rs)|
|Task4|实际clone/initialize与原exchange参考、独立protoc、正常RPC/PG/Kafka双副本、异常撤销/恢复和重启|WP02,WP03,WP09,WP11,WP13|LOCAL_SCOPE_VERIFIED|[正常RPC双副本回归](integration/artifacts/local/curve-meta-three-service-smoke.json)|
|Task5|三服务/common全检查、旧流程回归、SPEC/独立质量审查、失败封闭验收与统一进度|WP01,WP03,WP11,WP14,WP16,WP25|LOCAL_SCOPE_VERIFIED|[完整检查](integration/artifacts/local/verification.json)<br>[两阶段审查](integration/artifacts/local/curve-meta-review.json)<br>[限定范围验收](integration/artifacts/local/curve-meta-acceptance.json)|

验证结论（2026-11-02）：三服务与common完整检查、142份契约和8份fixture一致性通过；171项验收辅助、31项CI/文档及36项Meta专项辅助普通/优化模式通过。原Meta参考实际初始化4个clone，32次连续交换与8个探针（6次预期回退）验证原输出和完整Token/raw/admin后状态。正常RPC/PG/Kafka双副本回归覆盖4池/32状态/64请求（62成功、2次原生dust明确拒绝）、实时完整protobuf、5类异常scope撤销与generation1到6恢复、无关Solana保活和路由重启。最终源码与5个实际二进制绑定后，基础、多币、NG、Meta四组正式流程全部重新执行并封存；最终门禁在普通与PYTHONOPTIMIZE=2下通过。SPEC与独立质量审查关闭，配置浅拷贝污染、测试getter/raw余额混淆及状态HTTP断言问题已修正；原生零输出与公共API既有404/NO_ROUTE语义保持一致。

本批五项任务已在注明的Meta直接兑换范围内完成本地验收；全部剩余功能尚未完成。原26工作包完整验收仍为0。真实链部署/独立完整交易、underlying与其他Curve模型仍需单独验收，普通报价保持INDICATIVE，VERIFIED和生产准入不变。

<a id="plan-2026-11-02-priority-extensions"></a>

## Crypto、Solana高级状态与Infinity跨Bin

按用户优先级推进Curve Crypto→Solana高级能力→V4/Infinity扩展，逐项对应R05/R06/R08和§7.3、§11.3、§12.2/12.5/12.11–12.14及WP02–05、WP09、WP11–14、WP20、WP25。原有profile保持严格边界，新能力按固定实际协议源码分别验证，不以单币/静态费近似替代多状态逻辑。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|Task1|CurveCryptoSwap2价格调整、admin mint/gulp完整状态与固定原exchange执行差分；独立版本与服务接入边界|WP02,WP03,WP04,WP05,WP11,WP13,WP25|LOCAL_SCOPE_VERIFIED|[Crypto正常服务验证](integration/artifacts/local/curve-crypto-three-service-smoke.json)|
|Task2|Orca adaptive费率/时间/波动与分段状态、经典SPL/未初始化奖励/fixed-array完整exact-in数学，固定原程序参考及异常拒绝|WP02,WP03,WP05,WP11,WP13,WP20,WP25|LOCAL_SCOPE_VERIFIED|[完整adaptive swap内核](dex-router/crates/protocol-math/src/orca_adaptive_swap.rs)<br>[本批限定验证](integration/artifacts/local/priority-extensions-acceptance.json)|
|Task3|Infinity Bin完整初始化树/多Bin跨越、费用/储备及active-id后状态，独立原BinPool.swap参考；不混用CL模型|WP02,WP03,WP05,WP11,WP14|LOCAL_SCOPE_VERIFIED|[原BinPool跨Bin参考](integration/tools/reference/infinity-bin-multi/InfinityBinMultiReference.sol)<br>[本批限定验证](integration/artifacts/local/priority-extensions-acceptance.json)|
|Task4|已具备完整状态的能力接入正常解析/wire/路由，跨语言、共享池与拒绝/恢复验证；未接入能力单独保留|WP02,WP04,WP05,WP09,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|[Crypto正常服务验证](integration/artifacts/local/curve-crypto-three-service-smoke.json)；本批当时Orca/Infinity在线来源、解析与profile未完成，Orca已在后续服务批次完成限定范围|
|Task5|三服务/common全检查、旧语料/正常流程回归、SPEC及独立质量审查、限定范围验收与统一进度|WP01,WP03,WP11,WP14,WP16,WP20,WP25|LOCAL_SCOPE_VERIFIED|[三服务/common完整检查](integration/artifacts/local/verification.json)<br>[两阶段独立审查](integration/artifacts/local/priority-extensions-review.json)<br>[本批限定验证](integration/artifacts/local/priority-extensions-acceptance.json)|

验证结论（2026-11-02）：三服务/common完整make check、五个实际二进制和153份契约/8份fixture一致性通过。Crypto原池/原V4 LP参考60案例（44成功/16回退）；标准settled普通服务覆盖30原场景/双副本60请求（40成功/20原生失败拒绝），完整live protobuf、五种来源故障撤销/更高generation恢复、无关Solana报价、router重启及VERIFIED/build拒绝通过。共享池重算完整保留price/raw/LP账本；新LP费用可改变另一候选池余额时，快照/图/路径/allocation统一拒绝该未建模依赖，保留零admin fee与外部receiver可用性。Orca费率管理器197原案例/14连续交易/1215状态快照；完整classic-SPL fixed-array adaptive exact-in为101成功/2原生失败/两序列12交易；Infinity原BinPool跨Bin326案例含90回退和2本地预算拒绝。33项CI/文档测试及本批辅助检查通过，相关原参考与篡改反例在普通/优化模式下均有效。

两阶段独立审查及Crypto依赖delta复审已关闭。审查发现的Orca失败案例/连续序列可被同步删项问题已按固定成员manifest、唯一名称和交易数修复，保留先失败后通过证据；16个普通/优化模式篡改反例均拒绝。逐group steps为本地审计信息，独立原程序对照比较兑换量及完整后状态，不宣称获得独立step trace。

本批结论为PARTIAL_LOCAL：Task1仅在Crypto标准settled范围完成正常服务，Task2/Task3在注明数学内核范围完成；Task4在本批当时仍缺Orca initialized Oracle固定slot解析与在线profile、Infinity manager/tree来源和在线接入；Orca已在后续服务批次完成限定范围，Infinity静态零hookBin已在后续服务批次完成限定范围，完整结算仍未完成。Crypto active ramp在线完整schedule在本批当时未完成，已由第29批独立版本完成限定验收；Orca初始化奖励组合已在第30批完成限定验收；其他Crypto版本、dynamic arrays/Token2022、V4/Infinity swap hook/override/native完整管理器结算、真实部署/历史和独立完整交易执行继续保留。原26工作包完整验收仍为0，普通报价保持INDICATIVE；不开放VERIFIED或生产准入。进度仅记录于本文件。

<a id="plan-2026-11-02-orca-adaptive-online"></a>

## Orca adaptive账户与正常服务接入

目标对应R05、§7.3/8.12/11.3/12.2/12.5/12.9/12.11–12.14和WP02–05、WP09、WP11–13、WP20、WP25。沿用固定cc683ec9程序及已验内核，新增独立adaptive profile；保留旧static/rewards拒绝边界。本批先接入经典SPL、fixed arrays、未初始化奖励、完整Oracle的exact-in，完整消耗才作为普通报价；初始化奖励/Token2022/动态arrays和执行验证继续拒绝。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|Task1|独立版本、完整typed Oracle/保留字段及u16/u32/time约束，三服务状态/protoc/config/clone隔离|WP02,WP25|LOCAL_SCOPE_VERIFIED|[独立wire与状态验收](integration/artifacts/local/orca-adaptive-acceptance.json)|
|Task2|正常finalized同slot账户读取，Oracle/Pool/arrays绑定/PDA与部署hash、Clock/trade gate及原布局准入|WP04,WP05,WP20|LOCAL_SCOPE_VERIFIED|[正常RPC双副本](integration/artifacts/local/orca-adaptive-three-service-smoke.json)|
|Task3|正常报价调用完整adaptive内核，raw vault/fee/Oracle/tick后状态、全额检查及共享池重算|WP11,WP12,WP13,WP20|LOCAL_SCOPE_VERIFIED|[共享池与完整后状态](integration/artifacts/local/orca-adaptive-acceptance.json)|
|Task4|固定原program pack与swap参考、独立protobuf、正常RPC/PG/Kafka双副本及来源异常撤销/恢复|WP03,WP09,WP11,WP20,WP25|LOCAL_SCOPE_VERIFIED|[原program pack与服务参考](integration/artifacts/local/orca-adaptive-three-service-smoke.json)|
|Task5|三服务/common完整检查、旧流程回归、SPEC/独立质量审查、源码/构建绑定及统一进度|WP01,WP03,WP11,WP14,WP16,WP20,WP25|LOCAL_SCOPE_VERIFIED|[全服务检查](integration/artifacts/local/verification.json)<br>[独立审查](integration/artifacts/local/orca-adaptive-review.json)<br>[本批限定验收](integration/artifacts/local/orca-adaptive-acceptance.json)|

验证结论（2026-11-02）：三服务/common完整make check、五个实际二进制及160份契约/8份fixture一致性通过。固定原program pack生成2个实际PDA池、4组连续序列、20 banks；16笔原native完整成交、4笔原生数组覆盖耗尽拒绝，12笔初始费率达到100000，14次tick crossing；26项来源/成员/结果篡改在普通/优化模式下均拒绝。正常owner固定finalized读取完整Oracle/Pool/ticks与program/Token/Clock；普通报价复用完整adaptive内核且全额非零输出，所有raw/Oracle/tick/fee后状态与原参考匹配。独立protoc与保留字段、跨语言边界、旧profile拒绝及四组共享池连续分配通过。

正常PG/Kafka双副本运行10个原始bank组合，共20次请求（16成功/4拒绝），按实际选中路线调用原程序执行34步独立重放核对每路及总量守恒；完整live protobuf、11种来源故障撤销/更高generation恢复、无关EVM报价、router重启及VERIFIED/build拒绝均通过。来源Clock.slot只为本地组合bank统一编号，原数学时间和pricing/Oracle不变；不冒充真实RPC共识或历史slot证明。进程路由测试预算为1hop/8候选/64扩展/4parts/两拆分/10秒报价超时，不作为性能SLO验收。旧static/fixed+dynamic正常服务进程另做兼容回归。SPEC与独立质量审查已关闭，最终报告绑定同一源码与实际五个二进制。

本批五项任务仅在注明的经典SPL/三fixed arrays/未初始化reward组合内完成本地验收。Orca初始化奖励与adaptive组合在本批当时未完成，已由第30批独立版本完成限定验收；dynamic/mixed三完整数组在第31批完成限定验收；Token2022、奖励领取及SBF/CPI完整交易仍未完成；Infinity动态组合与完整Vault结算、V4/Infinity hook/override/native结算、Curve其他实际版本仍见R06/R08。原26工作包完整验收仍为0，报价保持INDICATIVE，VERIFIED/build/生产准入关闭。进度仅在本文件更新。

<a id="plan-2026-11-02-infinity-bin-online"></a>

## Infinity Bin完整树与普通服务接入

目标对应R08及§7.3/11.3/12.2/12.9/12.11–12.14、WP02–05、WP09、WP11–14、WP25。固定3c5ef1be原BinPoolManager与完整TreeMath，沿用已验跨Bin内核；新增独立静态fee/零hook/标准非零ERC20 profile，不把局部窗口当完整树、不以CL公式近似Bin。完整树超预算/缺项失败封闭，未知hook/动态fee/native与完整Vault settlement仍不计入本批完成。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|Task1|独立Bin版本、完整树/储备/总share与manager/vault依赖状态、跨语言wire/config/clone及来源校验|WP02,WP25|LOCAL_SCOPE_VERIFIED|[完整状态/wire](integration/artifacts/local/infinity-bin-acceptance.json)|
|Task2|固定blockHash读取原manager/key/slot0/树完整层级/全bin储备、pause/vault/token依赖与读后重锚|WP04,WP05,WP11|LOCAL_SCOPE_VERIFIED|[原manager固定块读取](integration/artifacts/local/infinity-bin-three-service-smoke.json)|
|Task3|普通报价复用跨Bin原数学、完整bin/active后状态和manager共享依赖传播；共享池/跨池分配重算|WP11,WP12,WP13|LOCAL_SCOPE_VERIFIED|[完整后状态与共享manager](integration/artifacts/local/infinity-bin-acceptance.json)|
|Task4|原manager编译/布局/immutable与getter来源、原BinPool交换参考、独立protoc及正常RPC/PG/Kafka双副本/故障恢复|WP03,WP09,WP11,WP14,WP25|LOCAL_SCOPE_VERIFIED|[普通服务双副本](integration/artifacts/local/infinity-bin-three-service-smoke.json)|
|Task5|三服务/common完整检查、旧流程回归、两阶段独立审查、源码/构建绑定与统一进度|WP01,WP03,WP11,WP14,WP16,WP25|LOCAL_SCOPE_VERIFIED|[全量检查](integration/artifacts/local/verification.json)<br>[独立审查](integration/artifacts/local/infinity-bin-review.json)<br>[本批限定验收](integration/artifacts/local/infinity-bin-acceptance.json)|

验证结论（2026-11-02）：三服务/common完整make check、五个实际二进制、170份契约/8份fixture一致性通过。原Manager23852字节/Vault9247字节在普通EIP170限制下部署/getters通过，固定5个完整source bank、两物理池双向4连续交换与完整tree/share/source状态相符。独立protoc、全三层树预算/缺项/ABI/identity拒绝、共享manager/currency来源一致性及每步账本传播通过。新原burn参考16场景48交易（39成功/9回退）验证树外active锁定1000share/dust、空树、跨树后逆向不复活已移除节点；旧326跨Bin原语料仍回归通过。

SPEC审查发现的两项P2已修复并独立复验：1. burn后active不在树仍可留dust，新增detached_active field29并完整解析/计算/wire/clone；2. maxBinStep只限制initialize，降低当前上限不禁用已有池。两者均保留先失败后通过测试。正常RPC/PG/Kafka双副本5bank10请求（8成功/2大输入拒绝），按API实际竞争/拆分路线逐路原EVM重放、完整live protobuf、10类来源故障撤销/更高generation恢复、无关Solana报价、router重启及VERIFIED/build拒绝通过。测试路由预算为1hop/8候选/64扩展/4parts/两拆分/10秒，不作为性能SLO。两阶段独立审查已关闭。质量审查指出的进程输入闭包/时间窗口与manager-limit输出分类问题也已修复并独立复验；进程前后9899项源码/fixture/reference/依赖输入保持一致，最终验收逐项比对当前输入，不复用陈旧成绩。旧singleton正常服务兼容回归通过，证据绑定最终源码与实际二进制。

本批五项任务仅在静态fee/零hook/标准非零ERC20的独立Bin版本内完成本地验收。原getter/source bank使用隔离storage投影；原BinPool/TreeMath/burn执行与普通服务验证不等于完整Manager/Vault lock/transient/Cancun settlement。动态swap hook、覆盖费、native完整执行、真实部署历史与独立收款验证仍未完成；其他Curve/Solana组合继续保留R05/R06/R08。原26工作包完整验收仍为0，普通报价保持INDICATIVE，VERIFIED/build/生产准入关闭。进度只更新本文件。

<a id="plan-2026-11-02-curve-crypto-ramp"></a>

## 第29批：Curve Crypto活动ramp普通服务

对应设计§7.3/11.3/12.2/12.9/12.11–12.14，WP02–05、WP09、WP11–13、WP25，剩余R06。沿用固定d7d04cd9原池/原V4 LP及标准非特殊ERC20边界，增加独立`curve-crypto2-ramp/d7d04cd9-ramp-1`；旧settled profile不扩大准入。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|T29.1|§11.3：field26可选ramp起点、端点/时间域、独立版本、三语言往返与旧版本拒绝已定向通过|WP02,WP03,WP25|LOCAL_SCOPE_VERIFIED|[本批验收](integration/artifacts/local/curve-crypto-ramp-acceptance.json)|
|T29.2|§7.3：同EIP1898锚读取原initial/future packed A/gamma与时间；Go解析/config定向回归及双副本完整状态比对通过|WP04,WP05,WP09|LOCAL_SCOPE_VERIFIED|[本批验收](integration/artifacts/local/curve-crypto-ramp-acceptance.json)|
|T29.3|§12.2/12.9/12.11：普通分派、28原交换完整后状态、22序列内及4次管理调用间连续状态、28bank共享allocation定向通过|WP11,WP12,WP13|LOCAL_SCOPE_VERIFIED|[本批验收](integration/artifacts/local/curve-crypto-ramp-acceptance.json)|
|T29.4|§12.14：固定原公开ramp/stop及交换、独立protoc、双副本/撤销恢复/重启；保留合成RPC证据边界|WP03,WP09,WP11,WP14,WP16,WP25|LOCAL_SCOPE_VERIFIED|[本批验收](integration/artifacts/local/curve-crypto-ramp-acceptance.json)|
|T29.5|§23/28：在线clock与优化模式拒绝校验修复、三服务/common检查、SPEC后质量审查、逐项台账同步|WP01,WP25|LOCAL_SCOPE_VERIFIED|[全量检查](integration/artifacts/local/verification.json)<br>[实际进程](integration/artifacts/local/curve-crypto-ramp-three-service-smoke.json)<br>[独立审查](integration/artifacts/local/curve-crypto-ramp-review.json)<br>[本批验收](integration/artifacts/local/curve-crypto-ramp-acceptance.json)<br>逐项一致性（`integration/artifacts/local/curve-crypto-ramp-progress-consistency.json`，已清理）|

验证结论：三服务/common完整检查、5个实际二进制、177份契约副本与8份fixture一致性通过；212项辅助测试及33项CI/文档测试普通/优化模式通过。原公开ramp/stop参考28交换、6序列、22次序列内及4次管理调用间状态传递、8管理调用通过；普通Go RPC/PG/Kafka与双router完成56报价、完整live protobuf、11类来源故障撤销/更高generation恢复、无关Solana保活、router重启及VERIFIED/build拒绝。共享allocation在28个bank连续复用完整ramp与价格/raw/LP状态；只有首笔金额具备该同一时钟的独立原交易对照，后续同钟重复交换验证路由/分配复用，不扩大为原交易执行证明。

本批不增加服务类型，不放开VERIFIED/build/生产准入；真实历史与部署身份、其他Curve模型、特殊Token及独立交易执行继续列入剩余项。进度只更新本文件。

规格与质量两阶段审查已关闭。公开stop/序列证据必须保留精确成员与8个独立管理交易；固定来源与8855项原参考输入在执行前后比对，包含启动钩子、解释器标准库及实际存在的字节码缓存。进程和两份独立审查均绑定当前源码、5个二进制与12574项完整执行输入，禁止只换工具后沿用旧审查。相关删项/重复/篡改、生成失败旧成功失效及优化模式反例通过。

原参考固定时间线为2026-10-01至2026-10-06，避免多日序列位于当前日期之后；生产新鲜度检查未放宽，在线head与提交前已补同样检查。本地历史参考使用专用宽时效配置，不作为生产新鲜度或性能SLO证明。旧CLMM的78处assert修复仅验证拒绝校验在优化模式下仍执行，不重新接受旧协议原参考或生产范围。

<a id="plan-2026-11-03-orca-adaptive-rewards"></a>

## 第30批：Orca adaptive初始化奖励组合

对应R05，设计§7.3/8.12/11.3/12.2/12.5/12.9/12.11–12.14及WP02–05、WP09、WP11–13、WP20、WP25。固定Orca revision cc683ec92acb5e7606caa3d8e514d9383e9c7c11，新增独立`orca-whirlpools-adaptive-rewards` / `orca-whirlpools/cc683ec9-spl-adaptive-rewards-1`。限定classic SPL、三连续完整fixed arrays、完整initialized Oracle与三奖励slot；覆盖初始化奖励增长、溢出原语义与跨tick增长翻转。旧adaptive/static/rewards版本保留原准入边界。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|T30.1|§11.3：新独立profile、完整Oracle/reward/tick状态契约、4007byte独立wire与旧profile隔离定向通过|WP02,WP03,WP25|LOCAL_SCOPE_VERIFIED|[本批验收](integration/artifacts/local/orca-adaptive-rewards-acceptance.json)|
|T30.2|§7.3/8.12：20原pack bank正常RPC、完整Oracle/reward/tick来源与重锚失败拒绝定向通过|WP04,WP05,WP09,WP20|LOCAL_SCOPE_VERIFIED|[本批验收](integration/artifacts/local/orca-adaptive-rewards-acceptance.json)|
|T30.3|§12.2/12.5/12.9：20bank内核与普通分派完整后状态、四组共享allocation两连续原交易通过定向验证|WP03,WP11,WP12,WP13,WP20|LOCAL_SCOPE_VERIFIED|[本批验收](integration/artifacts/local/orca-adaptive-rewards-acceptance.json)|
|T30.4|§12.14：原程序20bank/16成交4拒绝、奖励/Oracle/tick完整后状态、34普通/优化篡改拒绝已通过定向验证|WP03,WP11,WP25|LOCAL_SCOPE_VERIFIED|[本批验收](integration/artifacts/local/orca-adaptive-rewards-acceptance.json)|
|T30.5|§23/28：三服务/common全量、真实进程/撤销恢复/重启、SPEC后独立质量审查与逐项进度|WP01,WP09,WP14,WP16,WP20,WP25|LOCAL_SCOPE_VERIFIED|[全量验证](integration/artifacts/local/verification.json)<br>[实际进程](integration/artifacts/local/orca-adaptive-rewards-three-service-smoke.json)<br>[独立审查](integration/artifacts/local/orca-adaptive-rewards-review.json)<br>[本批验收](integration/artifacts/local/orca-adaptive-rewards-acceptance.json)<br>逐项一致性（`integration/artifacts/local/orca-adaptive-rewards-progress-consistency.json`，已清理）|

验证结论：三服务/common完整检查、5个实际二进制、184份契约副本与8份fixture一致性通过；219项辅助测试与33项CI/文档测试在普通及优化模式均通过。固定原程序pack/manager/postupdate参考20bank（16成功/4原拒绝），覆盖12次emission乘法溢出、4次global growth回绕、35次reward outside更新、重复时间与filter/decay窗口。34个篡改反例（普通/优化各17）及旧成功收据失效检查通过；独立protoc完整4007byte wire与旧profile隔离验证通过。

正常Rust finalized RPC/PG/Kafka/双router完成20主请求（16报价/4原范围耗尽拒绝）、实际选中分配的36步原程序连续重放、完整live protobuf/Oracle/reward/tick/vault状态、13种来源故障撤销/更高generation恢复、无关EVM报价保活、router重启与VERIFIED/build拒绝。四组两步共享allocation与原完整后状态匹配；恢复使用最新bank的同钟小额原生可成交输入，不回拨Clock。测试用1hop/8candidate/64expansion/4part/2split/10s预算仅用于本地原计划重放，不作为性能SLO。

SPEC及独立质量审查均关闭，无未解决P1/P2；两份审查和进程共同绑定当前源码、5个二进制及12593项执行输入。原ProgramData/Token代码身份为明确合成证明；SPL vault余额变化为名义转账模型，不能推断真实部署、SBF、CPI或validator rollback。原完整native数据集必须精确重生成一致，不以删项后的元数据/计数替代源状态证明。

本批不包含dynamic arrays、Token2022、reward领取/position结算、SBF/CPI完整交易和真实部署/历史准入。解析与数学结果仅INDICATIVE；VERIFIED/build/生产关闭。仅本文件记录进度，尚未具备证据的任务不记为完成。

<a id="plan-2026-11-03-orca-adaptive-dynamic"></a>

## 第31批：Orca adaptive dynamic arrays组合

对应R05、§7.3/8.12/11.3/12.2/12.5/12.9/12.11–12.14及WP02–05、WP09、WP11–13、WP20、WP25。固定cc683ec92acb5e7606caa3d8e514d9383e9c7c11，新增`orca-whirlpools-adaptive-dynamic` / `orca-whirlpools/cc683ec9-spl-adaptive-dynamic-1`。限定classic SPL、三连续完整fixed/dynamic可混合数组、完整Oracle和三奖励slot；继续拒绝Token2022、补充/稀疏/未提供数组、领取/position结算及未验证组合。旧两个adaptive版本保持fixed-only。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|T31.1|§11.3：独立版本、layout/bitmap/tick/reward/Oracle完整契约、4013byte独立wire与旧profile隔离定向通过|WP02,WP03,WP25|LOCAL_SCOPE_VERIFIED|[本批验收](integration/artifacts/local/orca-adaptive-dynamic-acceptance.json)|
|T31.2|§7.3/8.12：20原bank正常RPC、148/10004 compact边界、12种畸形与缺失数组及Oracle/源重锚拒绝通过|WP04,WP05,WP09,WP20|LOCAL_SCOPE_VERIFIED|[本批验收](integration/artifacts/local/orca-adaptive-dynamic-acceptance.json)|
|T31.3|§12.2/12.5/12.9：20原bank内核/普通分派与全后状态、100成功2失败×7布局内核复测、共享重算定向通过|WP03,WP11,WP12,WP13,WP20|LOCAL_SCOPE_VERIFIED|[本批验收](integration/artifacts/local/orca-adaptive-dynamic-acceptance.json)|
|T31.4|§12.14：原DynamicTick序列化/实际数组加载器/manager、20bank全后状态及44普通/优化篡改拒绝通过|WP03,WP11,WP25|LOCAL_SCOPE_VERIFIED|[本批验收](integration/artifacts/local/orca-adaptive-dynamic-acceptance.json)|
|T31.5|§23/28：三服务/common全量、实际进程/恢复重启、两阶段独立审查与逐项进度|WP01,WP09,WP14,WP16,WP20,WP25|LOCAL_SCOPE_VERIFIED|[全量验证](integration/artifacts/local/verification.json)<br>[实际进程](integration/artifacts/local/orca-adaptive-dynamic-three-service-smoke.json)<br>[独立审查](integration/artifacts/local/orca-adaptive-dynamic-review.json)<br>[本批验收](integration/artifacts/local/orca-adaptive-dynamic-acceptance.json)|

验证结论：三服务/common完整检查、5个实际二进制、191份契约副本和8份fixture一致性通过；228项辅助测试及33项CI/文档测试普通/优化模式通过。原DynamicTick枚举序列化、实际load_tick_array_mut和swap manager覆盖全dynamic及dynamic/fixed混合，20bank中16成交/4范围拒绝；12次emission溢出、4次global回绕及35次reward-cross更新，44个篡改反例（普通/优化各22）均拒绝并使旧成功收据失效。4013byte独立protoc、原内核100成功2失败×7动态布局的完整结果/error对照通过；两数组shifted-min-start不在三数组准入内，显式拒绝。

普通Rust finalized RPC完整读取20原bank，148-byte空数组/10004-byte全初始化compact边界、12种畸形与缺失数组/重锚故障均验证。双router完成20主请求（16成功/4拒绝）、实际选中分配39步原程序重放、16来源故障撤销/更高generation恢复、无关EVM保活、router重启及VERIFIED/build拒绝。四组共享allocation顺序传递完整layout/reward/Oracle/tick/vault后状态；恢复沿用最新bank时钟，不回拨原Clock。旧adaptive两个版本仍fixed-only，generic wire仅负责保留已知layout，池版本准入单独执行。

SPEC及独立质量审查关闭，无未解决P1/P2；审查与进程共同绑定源码、5个二进制及12612项执行输入。原加载器MAX view具有足够物理backing，但AccountInfo及发布字节仍保留真实compact长度；拒绝尾随零/非零和MAX padding，不支持初始化/反初始化或隐式重分配。测试用有界路由预算不作为性能SLO或生产容量证明。

不得以fixed字节重标dynamic替代原加载器验证；原参考继续声明synthetic ProgramData/classic SPL名义余额模型，无SBF/CPI/部署历史真实性推断。进度只更新本文件；不把单批完成视为全部设计完成。

<a id="plan-2026-11-03-independent-evm"></a>

## 第32批：独立EVM交易执行与真实余额证据

对应R10/R11，设计§14.1–14.5、§15、§21/23与WP14–16、WP25。将独立EVM执行从当前无后端拒绝推进为可验证的真实字节码执行路径；先固定原Uniswap V2 Factory/Pair/Router02和原ERC20，单hop ExactIn、标准ERC20→ERC20及明确Shanghai执行环境。独立测量caller输入/recipient收款余额和gas，不用报价数学输出填充仿真结果。原有unsupported/VERIFIED/build边界仅在具备同范围完整证据后调整，不因原型存在自动开放。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|T32.1|§14.1/14.5：BOUND信封、完整scope/物理池/代码身份/数学与交易一致性已补；11新测试及12旧测试通过|WP14,WP15,WP25|LOCAL_SCOPE_VERIFIED|[BOUND及独立执行验收](integration/artifacts/local/evm-execution-acceptance.json)|
|T32.2|§14.2/14.4：REVM19.7.0严格数据库、实际执行/overlay与gas/读/输入/指令/取消/时间/内存限制已接入，38项引擎/观察定向通过；同二进制stdin/stdout worker及硬超时回收已实现|WP15,WP25|LOCAL_SCOPE_VERIFIED|[严格执行与双审验收](integration/artifacts/local/evm-execution-acceptance.json)|
|T32.3|§14.2/14.3/12.14：原V2 Factory/Pair/Router02及原ERC20实际10交易（6成功4回退）、110账户/494显式storage、50篡改检查；独立REVM原交易全后状态及真实余额/gas差分已通过|WP03,WP14,WP15|LOCAL_SCOPE_VERIFIED|[原合约全声明后状态与篡改验收](integration/artifacts/local/evm-execution-acceptance.json)|
|T32.4|§14.3/14.4/15：正常服务接入有界独立仿真、缓存锁释放/新鲜度复核、实际交易/状态/费用完整性证据与21项API测试已定向通过|WP14,WP15,WP16,WP25|LOCAL_SCOPE_VERIFIED|[7次隔离执行及故障恢复](integration/artifacts/local/evm-execution-three-service-smoke.json)|
|T32.5|§23/28：三服务回归、实际进程与故障/隔离、两阶段独立审查与逐项进度|WP01,WP14,WP15,WP16,WP25|LOCAL_SCOPE_VERIFIED|[全量](integration/artifacts/local/verification.json)、[两阶段审查](integration/artifacts/local/evm-execution-review.json)、逐项一致性（`integration/artifacts/local/evm-execution-progress-consistency.json`，已清理）|

独立审查修复：只读余额观察不应额外要求caller预付探针gas；已复现交换成功扣费后余额不足探针预付额，以及gas耗尽后原生余额为零的失败路径，专用有界只读路径已修复且保留真实caller/GASPRICE/余额/nonce、不扣观察费用；新增3核心与2原字节码余额边界回归、23包装测试及Clippy定向通过，SPEC/质量审查及最终限定验收均通过；真实swap仍使用完整普通交易校验。

验收：三服务及common全量检查、五二进制、192份契约副本/8份fixture、253项辅助与33项CI测试普通/优化模式均通过。原发布字节码10交易（6成功/4回退）的110账户/494显式storage全声明后状态及真实余额/gas一致，50项篡改和旧成功失效在普通/优化路径通过。双副本12项真实进程检查、7次隔离执行、2次来源故障恢复/重启、三阶段只允许显式状态文件重载，以及绑定完整输入的SPEC/质量审查均通过。状态来源仅为PROVIDED_LOCAL_STATE，费用仅LOCAL_EVM_ONLY；不代表可信链、生产或所有功能完成。

补充核对：独立EVM使用完整账户状态时，应允许原合约自行处理未同步捐赠等raw余额差异，不以raw余额等于reserve作为通用前置。已增加原真实捐赠案例验证；当时V2解析/数学快照尚未表达该raw依赖；其完整next_pool/shared allocation后状态现已由T33独立版本补齐并限定验收，旧版本范围保持。

此为完整独立执行能力的前置增量；多hop/拆单执行器、其他协议与链hardfork/L2费用、SBF/CPI、真实部署/历史与生产准入继续保留。临时原型不作为项目功能完成证据；任何本地fixture执行必须明确标注来源，不能冒充可信真实链或生产VERIFIED。

<a id="plan-2026-11-03-v2-complete-swap"></a>

## 第33批：V2完整Swap状态与未同步余额

目标：补齐WP10/WP11当前V2数学快照未表达的原始余额、Pair锁、储备时间戳及累计价格，并在共享池连续分配中携带真实Swap后状态；对应§7.3、§11.3、§12.2/12.9/12.14及§14。仍限定原Uniswap V2 core 1.0.1、标准ERC20、固定3/1000费率和完整ExactIn。采用独立`uniswap-v2-complete` / `uniswap-v2/core-1.0.1-full-swap-1`，旧V2/Pancake/Sushi版本不自动接纳新状态。

设计取舍：在既有V2State增加field20的可选V2SwapState，保留reserved 3–19及旧wire；不在Pool根结构增添重复身份模型。新版本必须携带raw_balance0/1、block_timestamp_last、price0/1_cumulative_last、unlocked、factory_code_sha256与bank_block_number/hash/unix_timestamp。Pair/Factory runtime均固定原发布版本；旧profile拒绝新增状态，新profile拒绝缺失状态。服务目录仍为三个，Rust公共src副本保持一致。

数学先以储备计算原Router输出，再按真实余额/输入/输出执行原Pair两侧amountIn推断、K条件、uint112容量和SafeMath边界，更新raw/储备、锁与累计价格。累计价格明确使用指定执行时钟；普通路由默认当前bank时钟，同一计划的共享步骤复用同一时钟，不把未知未来块时间当已知事实。Mint/Burn/LP供应、税/rebase、跨协议或未来块成交保证不属于这个新增profile。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|T33.1|公共V2SwapState/wire及独立profile/config：先写Go/Rust缺失/错误版本/宽度/来源/旧wire回归，再实现兼容字段、严格Snapshot锚点校验和同步Rust src副本；模型/wire/config与必需字段presence已实现，Go/Rust回归通过；SPEC发现Go大小写别名覆盖，已红绿修复精确键白名单，SPEC已关闭；质量发现protobuf optional指针与源快照别名，已双向隔离红绿修复并通过质量复核；独立C++ wire含明确schema-only已知零例通过两语言/三服务验证；涉及common/protoc与两套domain/config|WP02,WP04,WP10,WP11|LOCAL_SCOPE_VERIFIED|[模型/独立wire与双审](integration/artifacts/local/v2-complete-acceptance.json)|
|T33.2|Go固定blockHash读取实际Pair/Factory身份、getPair、raw token余额、reserve时间戳、累计价格及锁slot；每次依赖读取同锚点，未知/错码/缺失/宽度/冲突拒绝；涉及chain/evm/v2与adapter|WP03,WP04,WP05,WP10,WP11|LOCAL_SCOPE_VERIFIED|[完整源bank/故障验收](integration/artifacts/local/v2-complete-acceptance.json)|
|T33.3|Rust公共分派及完整Swap后状态：先以捐赠/双向/时钟wrap/容量/K反例红灯，再实现单次与显式执行时钟、两侧输入、uint256/uint112、累计价wrap和共享allocation连续重算；旧模型结果保持|WP10,WP11,WP12,WP13|LOCAL_SCOPE_VERIFIED|[原数学与共享状态验收](integration/artifacts/local/v2-complete-acceptance.json)|
|T33.4|原发布Factory/Pair/Router/Token正常部署及公开操作生成独立bank：双向、输入/输出/双币捐赠、sync/skim、连续交换与时钟边界，记录实际前后账户/slots/getters/交易；普通/优化再生与篡改拒绝；服务测试各用本地fixture|WP03,WP10,WP11,WP13,WP25|LOCAL_SCOPE_VERIFIED|[原参考精确再生及49篡改拒绝](integration/artifacts/local/v2-complete-acceptance.json)|
|T33.5|完整状态绑定到既有独立执行及正常API：不混用来源/时钟，核对实际raw/时间戳/累计价/锁；双副本、来源故障撤销恢复、重启、共享计划原生重放；全量回归、SPEC后质量审查、逐项进度一致性|WP05,WP10,WP11,WP13,WP14,WP15,WP25|LOCAL_SCOPE_VERIFIED|[全量](integration/artifacts/local/verification.json)、[真实进程](integration/artifacts/local/v2-complete-three-service-smoke.json)、[双审](integration/artifacts/local/v2-complete-review.json)、逐项一致性（`integration/artifacts/local/v2-complete-progress-consistency.json`，已清理）|

实施顺序：公共模型/wire为主线前置；原bank生成与Go固定块解析可独立开发，使用同一已声明schema。定向采用`go test ./internal/domain ./internal/config ./internal/chain/evm`及`cargo +1.90.0 test --locked -p xone-domain -p xone-config -p xone-protocol-math -p xone-optimizer -p xone-execution-plan -p xone-simulation`，再运行三服务/common完整check、契约/边界和实际进程。每项先观察对应失败，再实现并验证；最终限定完成必须绑定当前源码、五二进制、全部参考/工具输入与独立双审。

本批限定验收通过：三服务/common完整check、五二进制、197份契约副本/8份fixture、283项辅助及33项CI普通/优化测试通过。原24case/4序列10步、84公开操作交易/79原trace、273后账户1525槽、独立C++三wire例及49篡改检查双模式通过；实际双副本14项检查、7隔离模拟/42后观察、2来源故障撤销恢复/重启及完整输入绑定的SPEC/质量审查均通过。Source只标PROVIDED_LOCAL_STATE，费用只标LOCAL_EVM_ONLY。T32报告自后续源码/输入变化起仅为历史验收；原26工作包完整验收仍为0。全部其余协议、可信链数据、独立执行扩展和生产验收继续按R01–R21推进，不因新增profile减少设计范围。

<a id="plan-2026-11-03-managed-v2-aum"></a>

## 第34批：Managed V2实际发布版与非零AUM（进行中）

目标：按R06及§7.3/11.3/12.2/12.9/12.11/12.14完成实际发布ManagedPool V2的非零AUM、协议分成与完整供应量状态，而不是解除旧不可部署profile的拒绝。版本固定官方balancer-deployments commit b55dc2b8a46bce8f27aed5a3b03f719f662f860a / task 20230411-managed-pool-v2，独立`balancer-v2-managed-aum` / `balancer-v2/20230411-managed-aum-3`。原pool/factory job solc0.7.1 optimizer140，库/math/helper job9999；正常EIP170限制必须持续生效。冻结原参考自身metadata仍为`balancer-v2/20230411-managed-aum-1`且bank字节不变；公开状态升级为aum-3，明确拒绝旧aum-1/2，防止旧Protobuf消费者丢失新增required钱包字段后仍误用同一版本。

前置研究已在临时目录用原Authorizer/Vault/Provider/Factory/库/math/helper及原ERC20完成27成功交易、66运行时检查与139离线校验：80/20权重、1%普通swap fee、1%年AUM、30%协议AUM分成，推进30天后真实铸造collector/manager BPT，再同clock收取为零。该证据尚未纳入项目功能验收；主网eth_getCode/历史仍未知，TestWETH不构成canonical native结算证明。

模型取舍：在MultiAssetState增加独立managed_v2状态，旧managed_fees/managed-snapshot/Composable不混用。报价assets按underlying顺序保存，Vault预铸BPT及其注册位置/cash/managed/实际余额独立保留，避免未实现的BPT路由被误加入图。完整状态包含原Pool/Factory/Vault/Provider/collector及原链接库/math/helper身份，所有token的cash/managed/实际余额与asset-manager，total/virtual/actual supply、pending AUM、年费/lastCollection32、manager/collector BPT账本、Provider fee ID映射与实际cache、swaps/pause/recovery/allowlist及当前权重/费用参数。三个服务保持独立，共享Rust src仍逐字节一致。

普通TOKEN→TOKEN GIVEN_IN先upscale再mulDown(1-fee)，按原WeightedMath计算并保留raw费用与Vault现金/managed边界；不铸BPT、不推进AUM收取时间。独立collection转换按原整数顺序：年BPT向下取整、时间比例向下取整、protocol分成向上取整、manager取余；lastCollection即使零收取也按原函数更新，维护同clock actualSupply连续。仅把已观察的provider映射/cache作为输入，不能用fee常量名称替代真实配置。复用旧pow核心前必须差分验证新输入费舍入及低精度token边界。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|T34.1|固化原build-info完整input/output/来源/许可证、两编译job、links/immutables、正常Vault/Factory/Token部署；生成普通swap、非零AUM/协议比例、重复/零/边界收取、多币/精度与账本原bank，禁止facade/storage injection/无限codesize代替正常链路|WP03,WP11,WP25|LOCAL_SCOPE_VERIFIED|27case/292真实交易、2/3/5/50 underlying、2285内容寻址对象/87代码身份，两个原编译job及正常EIP170原部署完整闭包已固化；精确临时再生与81类普通/优化篡改拒绝通过；原future/u32边界bank字节与时间保持不改；[最终同源限定验收](integration/artifacts/local/managed-v2-multi-acceptance.json)通过，真实链身份另列未完成|
|T34.2|独立ManagedV2状态/profile/config/wire及Snapshot/clone/未知字段/presence；精确验证Vault BPT、virtual/actual/pending供应量与source bank，不把derived state当新canonical，不改旧拒绝范围|WP02,WP04,WP11|LOCAL_SCOPE_VERIFIED|公开aum-3独立模型/config、39字段C++3023字节四副本/15自测与Go/Rust契约通过；required真实钱包、独立Pool/Vault暂停及Recovery有效费率、BPT固定注册/feeIDuint8/u32storage/clone/known0/未知值拒绝已验证；所有成功转换始终Some(ctx)、sourceH不变且canonicalSnapshot/wire拒绝派生；[最终同源限定验收](integration/artifacts/local/managed-v2-multi-acceptance.json)通过|
|T34.3|固定blockHash解析实际runtime/templates/依赖、Vault注册与balances、原AUM/cache/provider映射/完整账本及可成交状态；2–50 underlying按原协议容量，现金不足/暂停/禁swap等按真实语义处理|WP03,WP04,WP05,WP11|LOCAL_SCOPE_VERIFIED|固定blockHash、原runtime/templates/links/immutables与完整Factory/Provider/Vault/getter核对；48原snapshot、81薄投影Pool及新暂停/Recovery前后真实状态逐字段解析通过；BPT/钱包/source/AUM反例与旧版本隔离、正常双副本来源故障闭环通过；[最终同源限定验收](integration/artifacts/local/managed-v2-multi-acceptance.json)；当前四池目录正式服务接入仍属T34.8|
|T34.4|Rust精确AUM preview/collection与新版普通Weighted swap、SafeMath/舍入/别名账本及完整后状态、共享物理池连续重算；先红后绿并与原发布完整Vault实际交易差分|WP11,WP12,WP13|LOCAL_SCOPE_VERIFIED|原fee-after-upscale、2–50币数学、AUM floor/floor/ceil及别名账本完整后状态通过；原8swap/17collection（16成功1回退）/81Pool、同池sharedallocation、futureclock连续性/完整height防退及原暂停与Recovery swap/collection矩阵通过；[最终同源限定验收](integration/artifacts/local/managed-v2-multi-acceptance.json)；跨不同物理池Vault账本仍属T34.9|
|T34.5|正常source→PG/Kafka→双router报价/来源故障撤销恢复/重启、独立原生计划重放与完整状态检查；三服务/common全量、CPP wire、普通/优化反例及SPEC后quality审查和逐项进度一致性|WP02,WP03,WP05,WP11,WP12,WP13,WP25|LOCAL_SCOPE_VERIFIED|三服务/common完整check与五二进制、契约208/fixture8、普通/优化49辅助、15独立wire自测通过；source/PG/Kafka双router15银行/8正向/6暂停拒绝/5Recovery来源、10故障恢复/重启等18检查通过，选中4API路径28原签名交易独立重放且真实余额/完整getter/原receipt一致；当前完整输入绑定的SPEC/quality独立双审通过；[最终同源限定验收](integration/artifacts/local/managed-v2-multi-acceptance.json)；仅本节单池范围|
|T34.6|按原计划补完整Vault实际钱包状态：各underlying实际余额及Vault实际BPT与注册cash/managed分离，JSON/wire/clone/来源准入/整数swap和AUM别名收取后状态逐项保留，原getter投影差分与正常服务验证|WP02,WP03,WP05,WP11,WP13|LOCAL_SCOPE_VERIFIED|Vault实际BPT及每underlying实际钱包作为必需Amount进入JSON/wire/tag6及36/clone/原getter投影，registered cash/managed与global wallet区分；global>cash、u256溢出、gross/raw钱包delta、Vault收款别名AUMmint去重及81完整原Pool与实际服务通过；[最终同源限定验收](integration/artifacts/local/managed-v2-multi-acceptance.json)；跨物理池global传播仍属T34.9|
|T34.7|原Pool与Vault暂停语义分别建模与差分：保留各自有效paused、pause/buffer截止；普通swap分别准入，公开AUM collection不因仅Vault暂停误拒绝，未来clock过期视图更新及完整后状态|WP02,WP03,WP04,WP05,WP11,WP13,WP25|LOCAL_SCOPE_VERIFIED|独立原暂停3case真实状态0/1/1、7个source/after/executionSource完整Pool与数学后状态验证通过；pool/Vault各自暂停及期限37–39已分离，collection仅受Pool暂停约束，Vaultbuffer到期后普通双副本恢复报价、有效flag与source/derived时钟一致；[最终同源限定验收](integration/artifacts/local/managed-v2-multi-acceptance.json)通过|
|T34.8|补齐当前日期可服务的2/3/5/50币原Factory/Vault目录：独立正常EIP170真实交易生成，不改旧长时间/uint32边界bank；每次引用完整共同源H的全部池getter/实际钱包，普通source→PG/Kafka双副本多币报价和逐笔完整后状态/原计划余额重放|WP03,WP04,WP05,WP11,WP12,WP13,WP25|LOCAL_SCOPE_VERIFIED|2/3/5/50币四池共同H原目录已正式固化：10银行/9800原getter、4swap+4非零AUM；Go40完整Pool/Rust8原后状态与三副本精确再生/认证通过。当前源码三服务/common全检、214契约/8fixture与五二进制、106普通/优化辅助和14阶段最终同源门禁及独立双审全部通过；实际四池source→PG/Kafka→双router10银行/14计划/4故障恢复通过。[限定最终验收](integration/artifacts/local/managed-v2-multi-acceptance.json)，仍非真实链trust或生产准入|
|T34.9|不同Managed物理池共享同Vault/underlying的全局钱包overlay：跨池金额路径及拆单按实际执行顺序同步Vault实际余额、暂停依赖和账户别名，保留各自registered cash/managed与BPT供应；原多池实际交易完整后状态及普通服务验证|WP02,WP03,WP11,WP12,WP13,WP14|LOCAL_SCOPE_VERIFIED|跨物理Managed池共享Vault钱包传播/known注册cash合计/同H角色与Provider事实/重复冲突、Scope隔离与整path原子提交均已验证；31路由、11domain与13 optimizer/Replica/HTTP回归通过。资金版原raw `1f917c9951269028d24f927e1d415229f200019881bd05991e8a84f0b49c79e2`/2720562字节四副本精确再生；实际7来源银行/16原VM验证API计划/4来源故障与重启通过，两actor各实际2物理池拆单输入5e21、原VM收款7874328412914279650444。106普通/优化辅助、22门禁篡改回归与三服务/common全检/双审/14阶段最终同源门禁通过。[限定最终验收](integration/artifacts/local/managed-v2-multi-acceptance.json)；mixed/BPT/collection完整面/性能/真实链等余项不删除|
|T34.10|Recovery状态来源与有效AUM费率：按原getManagementAumFeeParams在recovery中返回0、normal时校验配置常规费率；原enable/普通swap/零collection/disable真实状态、完整后状态、普通服务进入/退出及审查闭环|WP02,WP03,WP04,WP05,WP11,WP13,WP25|LOCAL_SCOPE_VERIFIED|原独立7case分支实际enable/swap/零collection/disable成功并精确再生；recovery getter有效0、normal配置费恢复1%、零mint仍更新时间已完整差分；三domain拒绝recovery非零费/零费非零pending；15源bank服务路径含5recovery来源、2次recovery正向及退出恢复，质量P2独立复核关闭；[最终同源限定验收](integration/artifacts/local/managed-v2-multi-acceptance.json)通过，control只标原来源观察|
|T34.11|Managed活动权重/费率schedule后续能力：独立原始packed状态/字段布局（固定20230411为直接64-bit权重和62-bit费率，无有损压缩）、整数渐变与完整后状态/共享clock；独立版本/wire/固定块解析/原执行及普通服务逐项验证|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP25|LOCAL_SCOPE_VERIFIED|当前限定范围已完成：2/3币单池及两个三币物理池scheduled/static、scheduled/scheduled的原固定H、精确状态、连续/clock、实际API拆单/原VM和实际大额全后状态，在同一冻结全检/双阶段独立审查通过。仅该标准underlying有限版本范围，不包含BPT/rates/breaker/全部2–50参数与可信链/生产。此前开发与原型记录（历史）：当前单物理池限定闭环已完成：field40/原固定块234状态、53原swap及2 collection全后状态、55进程来源/127 API报价、15故障恢复/3重启、53不同原签名交易VM重放与干净目录精确再生；三服务/common和223向量/8fixture同源通过，全部已报独立审查问题关闭。仍须按M1–M4完成跨物理池活动schedule原source/执行/拆单组合，T34.11保持IN_PROGRESS。 此前原型与开发记录（历史）：独立tmp原合约参考已生成17真实swap/4管理交易/64状态/21边界检查点并精确再生，6类篡改拒绝，已归档prototypes/2026-11-04-managed-v2-scheduled；仅未集成原型，当前aum-3继续拒活动schedule，独立Rust整数原型已72测试通过，原17swap/1462 getter后状态/21checkpoint及固定packed未来查询精确一致（naive原10失败已红绿修正），归档prototypes/2026-11-04-managed-v2-scheduled-math；仍未集成，真实VM仅双币，三币原VM参考已验证ONE±1权重合计仍真实swap成功：10checkpoint/28原swap/69原状态、13篡改拒绝及byteexact再生通过，归档prototypes/2026-11-04-managed-v2-three-schedule；需新版本按原selected-weight语义实现，不能用静态sum==ONE硬拒或归一化；新tmp-v2已155测试/45原swap/31checkpoint/2442 getter后状态精确一致并归档prototypes/2026-11-04-managed-v2-scheduled-math-v2，但其constructor仍错误限制computed start endpoint sumONE，活动重排可能合法带ONE±1，原manager重排已补真实VM证明：2合法computed-start ONE±1重排、8后续成功swap与2个非法caller-end合计负控回退，byteexact/14篡改拒绝通过，且后续出现ONE±2，归档prototypes/2026-11-04-managed-v2-reschedule；需依原字段来源扩展数学constructor，不能改用简单tolerance；仍未正式支持，正式模型/解析/服务均未完成|
|T34.12|实际20230411发布版BPT join/exit swap路由与完整供给/钱包/控制语义：先固化原执行，再独立版本/状态/精确计算/逻辑BPT边/真实API执行逐项接入|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP14,WP25|LOCAL_SCOPE_VERIFIED|最终有限同源验收（`integration/artifacts/local/managed-v2-bpt-local-acceptance.json`，已清理）：独立BPT状态/wire、固定H解析、原精确转换、Funds图/分配/HTTP与四场景进程已通过；141source/250API/30拒绝/24fault/4重启，97计划100原交易及逐步完整Pool，实际worker7模拟、24 helper双模式、全检/双审均同freeze。仅标准3underlying/两物理池有限GIVEN_IN范围；enabled breaker/rates/mixed/其他custody/ExactOut/真实链与公开执行不计完成|



T34.11正式开发进度（2026-11-05）：Go纯packed decoder及父状态/field40/profile已落盘，原整数checkpoint（含合法重排ONE±2）、flags/零时长/JSON/word bounds/clone/非canonical拒绝定向通过，naive凸组合向下取整的原单位偏差已用overlay先红验证。两Rust镜像的纯decoder和父状态/codec/profile接入已完成，每边58项相关focused通过，旧JSON39字段/原C++3023字节与静态范围保持兼容。Go原RPC dispatcher与固定H槽9/token mapping11读取已接入，新profile的getter/raw/四flags一致性、缺word/短word/reserved/fee篡改定向先红后绿；目前来源测试仍为明确合成bank overlay，正式53原执行参考/Go全状态投影、精确交换及跨池未来clock view、独立scheduled C++wire和正常服务仍开发中，不能记为完整支持。T34.11开始后的旧单池/多池验收仅为已归档历史闭包，新profile必须重新完成自身同源验收。

续作执行清单（2026-11-05；对应下列步骤3–6，进行中）：先为 `managed_v2_scheduled_projection.py` 增加原始53 swap/41 checkpoint及collection投影的失败测试；再仅从认证原bank的固定H getter/raw与原编译身份生成独立Pool期望，不调用Go/Rust报价内核。以该投影补Go原RPC读取、Rust全后状态差分与时钟连续性验证，命令和实际结果随完成记录。已有Go domain/config/evm基线通过；数学/共享Vault新增实现和原参考/wire正在重验，不提前升级状态。审查范围按T34.11原计划，不扩张或删除设计功能。

T34.11续作验证（2026-11-05；步骤3–5局部结果）：补齐三币及重排原VM的完整服务getter，四独立原bank已重生为48,107,754字节/SHA256 `aa7718840cdad8254997dc62e9bb85d8458afeca9f883b26d94a46de918bcd0d`。新增认证原getter/raw→完整Pool投影（不调用Go/Rust报价数学），3项来源/派生/防篡改测试与234状态来源闭包测试通过；Go `TestManagedScheduledEveryOriginalSnapshot` 234子例全部通过；Rust `managed_v2_scheduled_native` 3测试逐项匹配234原时钟视图、41检查点、53原swap的实际收款及全部后状态、2次非零AUM collection全部后状态；原始ONE±2保持未归一化。新增数学8测试/共享Vault6测试通过，独立只读SPEC审查指定数学/state范围无P0/P1/P2；该审查不包含新增投影和服务，后续仍需质量审查/同源门禁。日志均在 `integration/artifacts/local/managed-v2-scheduled-*.log`；统一入口、更多拒绝回归、普通进程服务/来源故障撤销恢复/重启与最终全检仍待完成，T34.11保持IN_PROGRESS。

T34.11统一回归补充（2026-11-05）：`integration/Makefile` 已提供 `managed-v2-scheduled-reference-check/projection-refresh/projection-check/wire-check/helper-test/differential-test/smoke`，四份独立认证投影字节一致（3,128,319字节，SHA256 `d01ea64794bb462fce821cfadd745265c27fe9f881ce306e7ed800e0063c3ae7`）；带生产者Python指纹与正确collection原交易引用。Go原始状态234子例及8故障拒绝均通过，缺词/缺getter/fee污染/reserved位/旧profile提升均不发布；投影5测试普通通过，优化模式复验进行中。服务回放器3项祖先链/固定hash-storage/故障私有副本恢复测试通过；真实服务运行、参考字节再生与最终全检尚在进行，不构成完整功能完成。

T34.11参考与进程验证准备（2026-11-05）：原VM `managed-v2-scheduled-golden.cjs --check` 已对四副本完成新VM字节精确再生，SHA仍为 `aa7718840cdad8254997dc62e9bb85d8458afeca9f883b26d94a46de918bcd0d`；独立C++ field40 wire四副本3660字节/SHA256 `e52eb021fe8f30fbeaab11bb7ae8dd7afb68036fdbdfd1f7930a07e1fc874449`通过。投影5测试及回放3测试普通/优化均通过，进度绑定15测试通过；三服务/common全量check正在执行（Go已通过）。新增正常服务验收和选中API单跳原signed Vault交易重放工具；原sender/recipient/token/path/amount/来源H的15反例先红后绿，并修正原FundManagement数组实际顺序sender,bool,recipient,bool，尚待真实API与新原VM联合运行。多物理池原schedule组合、最终双审与完整同源门禁仍未完成。

T34.11独立质量审查修复与新冻结（2026-11-05）：原始state runtime与已认证original identity缺少绑定、factory依赖getter遗漏仍能投影、显式 `--original-input` 输出覆盖三项均已复现并修复；native runtime/缺factory两项回归66.4秒先红、22.6秒普通及76.4秒优化模式转绿，CLI原输入字节/mtime保护通过。普通服务另补readiness=503时实际FAST请求撤销、恢复报价完整reference/generation绑定和第二RPC初始化失败清理三项先红后绿，6回放/服务保护测试普通/优化通过。四份新原bank仍48,107,754字节，新SHA256 `3c539fcd10b1f473df5f4e663e86254fd047fef67bc9e08291e460e99f2e270a`，四份独立投影3,128,319字节，新SHA256 `1ca2faee2924401135ddfdcc1236e743bb2b6d9f70bc48773c4971a0c09523fb`；前文aa7718/d01ea6等仅历史输入。真实三命名空间进程/原API选中VM联合验收与31项普通/优化helpers已启动。现有全检运行开始于此次输入重生之前，最终必须对新冻结重跑，不能将旧指纹结果视为新验收。

T34.11深层immutable绑定审查（2026-11-05）：双币普通进程历史范围已通过17来源/41报价（含恢复与双副本准入）和5故障恢复，但随后质量审查再复现Pool owner immutable变体及Factory creation/WETH伪造关系，故该未完成总运行已主动安全终止并记 `ABORTED_REVIEW_FINDINGS`，不作新验收。新增owner/两个地址关系3项回归先红；校验器现依据原compiler AST+immutableReferences核对Pool全部immutable与getter/constructor、Vault WETH/暂停窗口、Factory原代码分片地址和剩余窗口。6项认证回归（含234原分支正控）已72.1秒通过，待独立复审确认后再重生最终输入并完整重跑。没有取消设计功能或将局部/历史结果升为ACCEPTED。

T34.11最终冻结复跑已启动（2026-11-05）：完成所有已复现代码/immutable/依赖/身份遮盖/非法hex来源修复，8认证回归普通96.1秒、优化91.9秒通过，234原分支正控仍通过。四原bank48,107,754字节当前SHA256 `cf88ade8a441342ce056a62b041e16288e5f813df1aa2367564656d2243f2010`，四独立投影3,128,319字节当前SHA256 `ff0e82cffa298110e5b658d0ed28fe581831c4efca80cb1cc03c9d193a890ee0`；前文所有旧hash仅历史。新增两Rust domain独立读取各自234原状态、raw words/current getters/代码身份、完整field40 protobuf往返、ONE±2和53派生状态拒绝canonical wire；两个Rust domain镜像各2测试、router原53swap/2collection三项差分均已通过。移除误复制的Rust旧AUM参考，修正真正使用向量的Rust路径，contracts.py已223契约向量/8fixture/3服务通过，没有放宽门禁。首次三服务/common各make check全通过，但其总验证因中途输入变化与上述向量缺口为false；现已在最终冻结上重新启动总验证、实际进程与37项普通/优化helpers，全部未完成之前保持IN_PROGRESS。

T34.11最后固定构造fee ID问题（2026-11-05）：独立审查确认原ManagedPoolSettings构造函数固定SWAP=0/YIELD=2，仅AUM可配置；已对生产Go parser和Node资格校验同时补约束，AUM alias语义保留。Go原runtime+getter+配置hash联合伪造在旧aum/new scheduled各两种ID共4例先红（原代码曾发布成功），修复后8.7秒定向套件转绿；Node两ID反例先红，9项认证（含234正控）128.1秒通过。先前cf88冻结上的进程报告真实通过55来源/127API报价/53独立交易全部原VM收款与后状态、15来源故障恢复和三个router重启；普通/优化37 helper各通过。因为本次生产Go/资格校验变更，上述运行明确降为历史，不作为新冻结最终验收。新参考正在重生，Go完整check与9项优化认证已启动，后续仍需新源码/新参考的进程VM/全检重新一致。

T34.11固定构造约束修复后重新冻结（2026-11-05）：9项来源认证普通128.1秒、优化132.3秒全通过；原input仍为2/3币234state/53swap/41checkpoint，四新原bank48,107,754字节SHA256 `d7b6b26eea6a880e6488fffe00fb2c25c267812b8c7872dc926ad3f2bfc4501e`。Go正常与debug binary均已重建，完整Go check进行中；四独立Pool投影正在重生。此前cf88上的127实际API/53原VM交易验证及三服务/common检查均留作历史，最终资格仍需本次固定fee-ID修复的二进制/新输入重跑，不提前更改T34.11状态。

T34.11独立复审关闭（2026-11-05）：来源审查已独立确认全部已报告问题关闭，最后固定fee ID通过Node两种联合伪造subtest和Go静态/scheduled四子例；AUM仍可配置。数学/shared-state SPEC审查与服务质量审查的已报告问题亦已关闭，限定审查记录见 `integration/artifacts/local/managed-v2-scheduled-review.json`，其中明确各审查覆盖与未覆盖范围；该记录不替代真实进程、新原VM或全设计验收。当前四投影SHA256 `a67c9ef397a9d572e732152404d0c663b8e0eecafcd62dd985971ccf23a8aa81` 对应最新原bank `d7b6b...4501e`。最终新冻结三服务/common、38 helper普通/优化及实际进程/VM正在重跑；没有升级完整工作包或生产准入。

T34.11.6 M1正式开发启动（2026-11-05，进行中）：新增独立 `balancer-managed-v2-scheduled-shared` 原VM harness/资格校验/CLI与精确输入manifest，保留既有单物理池原参考。新参考设计为混合profile（A活动、B旧静态）与双活动两组，各两个真实三币物理池共享18/6/8 underlying；固定原prefix28、正常Factory初始化、公共mint/approve/join、六个共同clock检查点的A/B/A/B原swap及两池非零AUM collection。期望金额仅来自原ERC20实际余额和Vault事件，不调用本地数学。CLI/输出保护/来源闭包3项先红后绿；首次原VM两组已执行，但资格视图漏传Factory/Vault导致未发布，补回归先红后修复，4项入口/观察视图现已通过。完整72bank/48swap/4collection数据正在重新生成并资格校验，M1尚未完成，M2–M4消费者/数学/实际拆单与进程闭环仍待后续；既有单池验证自新增工具/输入变更起只作历史有限证据，不提升原26WP。

T34.11.6 M1–M3新证据（2026-11-05，仍未最终验收）：新四份原参考22,999,112字节/SHA256 `c1a4094d47b63487c72687662511a079a4b0c8b023cc0837cef505c098e79eb5`，包含72共同H bank/144物理池、48真实swap/4非零AUM collection/12 clock。独立M1审查复现重复A交易充数、同clock兄弟权重可变、越界getter被过滤漏验、management/clock断引用4类问题，5反例先红；补齐交易集合一一对应、A/B覆盖/时序、全bank/所有getter ABI与锚点、同clock全部pool视图和完整control/clock/nonce链后，1正控+5反例106.3秒转绿，等待定向复审。四独立Pool投影2,912,236字节/SHA256 `113310f745bc7e4cecb8e7ed43d15bdf9cac5707dd5eb3818dcb1e2a187aa603`；仅认证原getter/编码/供给减法，不生成价格。Go72bank/144 Pool全字段与缺peer原子拒绝/旧profile拒活动已4.1秒通过，两Rust域各2项完整来源/wire/派生隔离通过。Rust原52次swap/collect双方全后状态、12连续A/B/A/B+末尾collection序列、12 clock-only双方各自view/AUM差分3项已2.3秒通过。M4实际API双路拆单/原VM新执行与进程故障/全量同源门禁仍未开始，不将这些定向结果升为完整支持。

T34.11.6 M4首轮与新增强制闭包（2026-11-05）：首轮真实Go/PG/Kafka/双router已通过72共同来源/CPP wire、52次实际两路API报价、两mode共12故障撤销恢复/2重启及public VERIFIED/build拒绝。独立原VM恢复各来源后执行12个不同实际plan/24笔新签名Vault swap，52报价通过明确quote_links关联（不冒称52个独立VM）；每次snapshot/revert后完整source恢复，钱包扣款/收款、事件与完整观察匹配。M1四类来源审查问题已独立6测试/完整Root.check复审关闭，流程审查5 replay/quote guards及20负控plan单测限定通过。为补实际大额每步完整本地状态，新增受信Node报告的只读getter投影（先红缺helper后35秒转绿含3篡改拒绝），以绝对向量路径强制调用Rust `actual_api_selected_two_route_full_poststates --ignored --exact`，12 plan/24步双方完整字段及SourceH/execCtx已2.1秒通过；该项普通unit明确ignored，M4门禁要求实际运行1 passed。新增mapper/调用路径亦独立复审通过。原参考仅登记434输入的干净目录重放已80余秒通过，四原bank bytes/mtime均未改。因新增消费者使指纹变化，四投影已重生为2,912,236字节/SHA256 `7667c76881d60e861517b29e24201708bdf6c8cec5f7aa354fb06b63bd9eef72`（旧113310仅历史），新冻结全check/实际进程+原VM+实际向量Rust/普通优化helpers正在复跑。原VM使用顺序本地交易作有限参考，明确不证明生产原子执行/public构建，原26WP仍未完整验收。

后续R06/BPT路由原源码核对（2026-11-05，仅未集成观察，不表示能力完成）：固定原 `ManagedPool._onSwapMinimal` 的BPT方向首先收AUM，再以actualSupply进行join/exit；BPT join按request.from校验LP allowlist，exit不校验，但两向均要求swap和joinExit开关。隔离原VM实验已真实执行underlying→BPT及BPT→underlying：一单位18币得到1492466352598528379 BPT，随后输入746233176299264189 BPT得到823277个6位币最小单位；join的totalSupply仅因AUM增加69890970086662706，而Vault注册BPT cash按实际输出减少，exit按实际BPT输入增加。该事实来自原Wallet/receipt/rawbank（`integration/artifacts/local/managed-v2-bpt-native-probe.json`），当前生产profile仍不开放BPT路由。后续必须独立profile/capability、真实weighted join/exit舍入/边界、BPT supply/注册与全局钱包/allowlist上下文完整建模，不能复用旧e281ff40超EIP170的swap-share铸费语义，也不能把当前观察当正式验收。

T34.11.6 M1–M4当前有限范围最终闭环（2026-11-05）：源码冻结 `cb1ea485d9daae8d088d66b8e153963db0597d4ac75ab4d39d53fcb5519a509e`；四原bank仍22,999,112字节/c1a4094d...e79eb5，四独立Pool投影2,912,236字节/7667c768...9eef72。三服务/common所有make check和debug builds、229契约向量/8fixture及服务权限边界八命令均0，源码/五binary与进程输入一致。M1 source四类已报问题独立复审关闭；流程/原VM及新增实际报告mapper/强制Rust调用亦独立限定通过。最终72共同来源/CPP、52实际两路API、12故障撤销恢复/2重启、12独立plan/24新签名swap原ERC20钱包/事件与全部观察通过；52报价明确quote_links关联，不冒称52个fresh VM；每mode一ownedVM，每个不同plan snapshot/revert后完整source恢复。实际大额24步双方完整Pool/SourceH/execCtx本地连续模型已由M4强制运行normally-ignored Rust test1 passed；Python实际报告映射测试额外显式1 passed。普通/优化23辅助项各22通过+1依赖真实报告的默认跳过，该路径已由上述实际执行覆盖。仅434登记文件的干净目录原VM重新生成四份原bank逐字节一致且mtime未改。有限证据汇总见 [当前多池验证记录](integration/artifacts/local/managed-v2-scheduled-shared-local-validation.json)，冻结构建验证（`integration/artifacts/local/managed-v2-scheduled-shared-final-verification.json`，已清理），[实际进程与原执行](integration/artifacts/local/managed-v2-scheduled-shared-three-service-smoke.json)。T34.11仅升级LOCAL_SCOPE_VERIFIED（注明版本、2/3币及两三币物理池标准underlying范围）；全部2–50参数、BPT/rate/mixed/breaker组合、生产原子执行、真实链与原26完整WP仍未完成，public VERIFIED/build维持关闭。

T34.11.6跨物理池原执行子计划（与下方原步骤6一一对应；M1–M4已按以下有限2池/3币范围通过本地验证，不是完整设计验收）：

- M1：独立 `managed-v2-scheduled-shared` 原参考沿用固定原Factory/Vault/ERC20与prefix28；创建共享underlying的第二物理池，分别记录活动权重/费率及静态/活动混合profile的共同H getter、slot9/mapping11、角色/全局钱包/各池BPT账；价格期望只取原交易和真实钱包变化。保留当前单池53原case作为独立历史闭包，不把新多池数据混入旧分母。
- M2：两池完整来源投影和Go固定块bank读取、两Rust domain/field40契约、共同Vault的代码/角色/provider/实际钱包一致性；缺任一池依赖时整组失败，其他Vault和Solana不受影响。
- M3：原A/B/A/B及显式collection的共享钱包、各池注册资金/BPT和未来clock全后状态差分；正常API实际选择两条路线/整数拆单后，由原VM按返回顺序执行并核对真实余额，禁止用本地数学生成期望。
- M4：source→PG/Kafka→双router、多池来源故障撤销恢复与重启，重新完整check/普通和优化helpers/独立SPEC与quality，再按新冻结升级该限定范围。该计划不开放BPT/rates/breaker、可信链或public VERIFIED/build，不将已有单池通过称为跨池完成。

T34.11单物理池当前限定范围最终闭环（2026-11-05）：最新冻结源码 `24e16da70c82d50c83b81ee654b392402cba3bb7f5498cd3797f96aabc69a550`；原bank `d7b6b...4501e` / 四Pool投影 `a67c9e...aa81` 与五binary、完整check、38 helper普通/优化、实际进程/原VM和独立审查在同一冻结一致。原始证据汇总见 当前本地验证记录（`integration/artifacts/local/managed-v2-scheduled-local-validation.json`，已清理），完整构建报告已另存为 该范围构建验证（`integration/artifacts/local/managed-v2-scheduled-final-verification.json`，已清理），实际127报价/53不同原签名交易真实余额及完整原观察见 [进程与原VM报告](integration/artifacts/local/managed-v2-scheduled-three-service-smoke.json)。普通与优化helper各38项，其中37通过、1项干净复制VM按默认开关跳过；该项随后以 `XONE_SCHEDULED_FULL_CHECK=1` 单独实际执行163.5秒通过，确认只复制登记依赖即可精确重放四份原bank且不改输入mtime。各独立审查的全部已报问题已关闭。四条逐项功能仅升级为注明的LOCAL_SCOPE_VERIFIED，T34.11整体继续IN_PROGRESS：原步骤6中的M1–M4跨物理池活动schedule原组合仍缺；BPT/rates/mixed/breaker、可信链和public VERIFIED/build等原设计余项不变，原26完整WP验收仍为0。

T34.12 BPT执行计划（依R06及§11.3/12.2/12.9–12.14；2026-11-05继续执行，未完成项保持开放）：

1. B1：固定原发布版，独立固化underlying→BPT和BPT→underlying GIVEN_IN原swap，覆盖已知单位/精度、非零pending AUM、Vault BPT cash/实际钱包、实际/virtual supply、joinExit/swap开关、allowlist/circuit边界及原回退。只以真实余额/事件/完整原getters给出期望，保留已做单次实验为原型而非正式支持。
2. B2：以原实际依赖定型独立profile/capability和必要完整状态，Go/两Rust/common/wire一致，区分逻辑BPT图节点与唯一物理池、注册BPT与全局实际BPT钱包；旧无BPT版本和已知未建模组合继续明确拒绝，不能缺字段默认为关/零。
3. B3：按原WeightedMath单币BPT join/exit顺序实现精确整数、upscale/fee/pow舍入和uint边界，先收AUM再以actualSupply计算；交易由Vault预铸BPT现金转入/转出，不能把输出BPT当额外铸币。正常与连续复用同一内核，保留sourceH/derived与各池独立供给。GivenOut独立语义按R14保留，不能对ExactIn取反或倒数。
4. B4：正式逻辑BPT边和金额感知候选/共享状态重算，原执行证明BPT与其他物理池作为underlying的耦合前不静默解除相关组隔离；单位守恒、拒绝原子性和相邻Token行为按方案验证。
5. B5：普通source→PG/Kafka→双router、实际API计划的原VM真实BPT/underlying余额和全部中间后状态、故障撤销/恢复/重启；三服务/common全检/独立wire/普通优化helpers/独立SPEC和quality在同一冻结后，只升级注明范围。rates/mixed/breaker、真实链、完整Gas和public原子执行等其余原设计要求保留。

T34.12本轮接续（2026-11-05，进行中）：保留工作区既有T34.11/T34.12改动，按B1→B2→B3→B4→B5核对，不更改设计基线或完整验收口径。B2现有两Rust `managed_v2_bpt_state` 测试已实际运行并先红：缺少独立模块/`bpt`字段，证明状态契约尚未实现。接续步骤：先实现common新增可选field41与Go/两Rust必需BPT控制状态、独立profile和旧版本隔离、JSON/wire presence/clone/非法成员拒绝；再完成原参考差分与完整join/exit账本；之后接普通parser、逻辑图/shared重算及B5进程/故障/原VM。同源全检完成前，B1–B5及T34.12均不升级为已验收。

T34.12 B1–B3增量（2026-11-05，未最终验收）：B1原发布正常VM四分支145bank/116尝试（48join成功、51exit成功、14次BPT回退、3普通swap）；修复接续前checker与manifest失配后重生四原副本，51,222,289字节/SHA256 `e32f9cf5c71af87c5d18b14a5c5fef58672ed7e7f0ee92101478f8df355c97c4`。新投影3,130,474字节/SHA256 `a95287f87061029c545e84163612dc163b6464f685eeb87b3a95561396eb0aa5` 保留完整来源、原期望与145bank，4个启用breaker的物理池观察显式列unsupported，不吞掉其他投影错误。B2 common新增可选BPT41，必需joinExit/locked minimum/registered block及显式member false；独立 `balancer-v2-managed-bpt` / `balancer-v2/20230411-managed-bpt-1` 要求raw schedule与完整控制，旧aum/scheduled拒携带新BPT字段；Go深clone/JSON重复大小写null/必需wire及两Rust状态测试通过。独立C++ synthetic wire四副本16,702字节/SHA256 `0cd5dbca6ba0647d297c99a412deddaed008114c0b835034d69ab63520fb542c`，仅语法证据。

B3正式整数与完整Pool转换 `quote_bpt_exact_in_at` 已接原AUM collection/reclock：先mint pending费并按协议ceil拆分，交换自身只转预铸BPT cash；保留完整registered/global wallets、BPT supply、lastChange、控制/raw schedule/sourceH与derived隔离。原无breaker范围98成功（47join/51exit）与13回退逐项比金额及整个保留后Pool通过，补收款manager/collector/Vault别名、sender白名单与recipient分离、坏时钟/零地址/原子拒绝回归，金额内核4测试与完整Pool3测试通过。原VM单单位可能输出零仍按原语义保留，公共报价正输出政策须在后续普通入口验证。新增Make原参考/投影/wire/helper/differential入口；该阶段domain只声明状态能力；后续正常config/parser接入见下条增量，B4/B5仍未完成。三服务完整make check、普通/优化helpers及独立审查尚在执行，不提升T34.12或任何完整WP。

T34.12 B2固定块解析接续（2026-11-05，进行中）：三服务配置新增独立 `managed_v2.bpt.allowlist_subjects` 有限观察范围，新profile要求 `standard-bpt` 与显式配置（可空但不可缺失/null），旧aum/scheduled拒携带它；只声明需观察的主体，不配置允许结果。Go正常ReadState两dispatcher及原身份校验链已接BPT版本，仍按同一EIP1898 H读取/读后重锚；读取原BPT注册lastChange、零地址锁定minimum、schedule/joinExit与每subject原getter，并核对原AST固定slot13 mapping的0/1布尔值。原145bank/286 supported Pool全字段正常入口差分通过；成员getter/raw矛盾、非法slotbool、错误locked、joinExit矛盾、注册u32超宽5反例均整批拒绝。Go/Rust配置positive先红，普通underlying分派测试先红 `MULTI_ASSET_MATH_NOT_IMPLEMENTED` 后正在补接同一内核；Rust parser配置测试继续复用既有EVM语法测试上下文，不宣称该服务承担EVM采集。原B1–B3独立只读审查无P0/P1/P2，新增B2解析范围正在独立复审。配置/parser及普通分派变动后先前总检查只作阶段证据，仍需最终冻结复跑；B4自身BPT逻辑边、调用资金上下文贯穿、B5进程/API/原VM与来源故障闭环尚未完成。

T34.12 B4启动（2026-11-05，进行中）：普通underlying分派已先红并接同一Managed完整内核；新增BPT逻辑边/显式Funds(sender,recipient)与search执行上下文的定向测试已先红（旧routing缺少上下文接口）。实现仍保存唯一物理Pool状态，legacy evaluate/search缺Funds时BPT兑换明确拒绝；有Funds的顺序路径定义中间收款回sender、最终收款为recipient，后续必须以原VM实际计划验证该直连Vault观察语义，不视为公开原子执行能力。正在验证所有原支持方向、重复同池join→exit后状态、兄弟global wallet传播/各自供应不变与失败整path回滚；optimizer/API上下文贯穿、原实际拆单/进程/fault和全部最终冻结门禁尚未完成。新增B2原parser/config及B4源码变更后前述全检均仅阶段证据。

T34.12 B4有限路由/API进展（2026-11-05，未最终验收）：B2三配置正常/拒绝范围和原固定H解析均已通过集中差分；B3扩大原ordinary TOKEN swap比较后，支持范围100成功（47join/51exit/2普通）/14回退整个保留后Pool一致。B4 `Funds(sender,recipient)` 已贯穿logical graph→候选search→allocation全量重新计算→普通HTTP，legacy无Funds入口明确拒BPT；原96正输出BPT swap经过逻辑边金额一致，同池连续join→exit/兄弟Vault wallet传播及独立供给保持/失败整path原子回滚、同物理池两份allocation实际重算通过；HTTP两向原金额与allowlistedcaller成功/未知caller拒绝已先红NO_ROUTE后转绿，仍仅INDICATIVE。普通Managed TOKEN腿的Vault自收付别名尚未建模，已用先红后绿回归明确拒绝，避免错误global wallet delta；不删除该后续功能。正在增加96原route双方完整后Pool比较（quote按sourceblock计算；仅对照原receipt后block的两个触及lastChange作显式坐标归一，原B3真实执行clock比较不变），B4独立只读复审进行中。B5真实source/PG/Kafka/双router、实际选中API计划原VM与故障恢复/重启及最终同源门禁未实现，T34.12继续IN_PROGRESS。

T34.12 B4审查修复与B5启动（2026-11-05，进行中）：独立复审发现两P2均已实际先红：外部V2中间BPT收款遗漏manager/collector已观察钱包，和普通Managed腿Vault自收付地址大小写绕过。新增 `routing/bpt_wallets.rs` 按真实V2 custody在每腿后传播ERC20 BPT资金到其发行Pool的已观察钱包，供给/registered cash不虚构变化；未知其他custody仍明确待实现，未用简单断路代替标准V2功能。Managed scope使用Funds前校验canonical非零EVM地址。manager/collector外部资金→exit整个Pool与大小写/失败原子回归先红后绿，修复正在独立复审；96正输出原逻辑路径双方完整后Pool（明确quote/receipt块坐标差异）已通过。普通HTTP2向/allowlist及大小写拒绝3测试通过。新增actual in-process API导出4计划（settled两向、scheduled join、A BPT→underlying→B BPT），原发布Vault真实5签名交易/真实收付款与每步原checker及source恢复全部通过；仍非真实进程验收。

新增B5三服务工具 `managed_v2_bpt_smoke.py` 及原计划重放 `managed-v2-bpt-plan-replay.cjs`，先红模块缺失/selected输入边界反例后实现，当前正在构建五二进制并准备实际source→Postgres/Kafka→双router、固定H完整protobuf、控制/边界报价与来源故障撤销恢复/重启。工具记录原始bank与全部输入/二进制前后绑定，原VM replay必经且不能跳过；同一冻结check/普通优化helpers/实际进程/native与完整后状态复核/独立双审仍需全部完成。首轮router全量检查遇既有simulation worker8秒启动测试失败，正在单项复核根因，未改断言或放宽预算；Rust parser阶段make check已通过，但后续B2配置变更后仍需最终新冻结重跑。T34.12与原26WP验收状态不变。

T34.12 B5首轮实际进程证据（2026-11-05，未通过）：settled14个完整来源bank/双副本24报价已与原值及C++完整protobuf一致，首个missing-peer-getter来源故障已同时撤销两router并新generation恢复；第二个missing-peer-word未在限定25秒内观察到全部撤销，报告真实FAILED，不算最终验收。复跑只增加owner/两router超时状态诊断，未放宽时限；当前已通过前两类故障，等待完整结果。另原选中API4计划5交易的native结果已增加独立signed transaction/receipt/完整bank及control重验证与逐步Pool投影，正在对每步实际execution block与双方完整模型后Pool再作差分（不是只检查金额）。B4两P2已先红后修并请求定向独立复审。各新源码变动后的旧报告保留为阶段证据；仍需同一冻结上的全检/审查/actual-process/native一致闭包。

T34.12 B5 settled阶段结果（2026-11-05，仍未最终验收）：第二轮真实source/Postgres/Kafka/双router已完成14完整来源bank与C++ protobuf、38实际API报价、6来源故障撤销恢复（每次两router缓存报价拒绝/无关Solana保留）和router重启，实际API选中计划原VM12次签名交易重放通过。因开发期间source/test输入变化，最终前后绑定正确判为FAILED `BPT source/binary changed during run`，明确不作为最终同源验收；后续须冻结新源码/二进制重跑。原第一轮25秒撤销观察失败未放宽断言，补诊断后第二轮原时限6故障均通过，尚不能由此认定时序根因解决。新增原4计划5交易逐步execution-block与双方完整Pool模型差分通过。

simulation回收测试根因已测量：实际请求准备校验binary约3.5秒，后续大Rust测试image的OS创建未返回前8秒启动断言已到；直接probe冷7.6秒/热2.8秒。只将该HTTP取消/owned-permit/cache-lock测试的阻塞子进程换为立即报告PID并exec sleep的最小子进程，保留原8秒启动/生产10秒预算、真实私有worker的协议/outputcap/执行测试；原失败后该测试7.54秒通过。无生产超时放宽，待完整suite复跑。B4跨标准V2 BPT资金与canonical地址修复已回归通过，仍在独立复审；其他custody组合/真实链/公开build及VERIFIED保持未完成。

T34.12最终冻结验证启动（2026-11-05，进行中）：全部五binary已重建，三服务完整check/common契约、18项BPT helpers普通/优化和四分支真实source/PG/Kafka/双router已启动。当前冻结settled已完成14原来源、40实际API报价（含两副本40% BPT两路四步）、6来源故障撤销恢复及router重启，scheduled正在执行；尚无最终passed。原独立in-process5API计划9签名交易（两路四步与两跳）已经原执行checker及逐步双方完整模型after差分通过，工具后续对实际进程选中计划必经相同验证。B4独立复审先后3个P2均已复现红绿并关闭：外部V2 BPT钱包入账、canonical大小写绕过、外部pair自收付储备假设；后一项单腿/中间腿整map不变回归已独立运行通过。未知custody/self-funding完整会计功能仍保留后续范围，不将显式拒绝当完整实现。B5源码/证据独立审查正在进行；各完整工作包验收仍为0。

T34.12最终四场景进程中发现实际续租缺口（2026-11-05，未通过）：冻结settled14source/40API/6fault/重启通过；scheduled83source及145API通过后，missing-peer-getter故障时owner已ready=false，两router仍ready=true并停留中间block75。诊断证明该长catchup的writer只在外层Tick续租，单Tick多块读取耗尽本地authority deadline后，错误分支因Current不可用跳过撤销。该结果明确FAILED，非允许加长25秒门禁。新增TestCatchup两确定性回归已先红：>TTL批次持续续租、慢单块必须留revocation预算/失去authority不得再读；正在补每块前后续租与source context截止于TTL减原min_commit_remaining、publish前续租，并在普通追赶/重组读取共用。生产TTL/门禁时限不放宽，旧冻结全检/进程一律阶段证据，修复后需完整新冻结重跑。该项对应原§7/7.9/23、WP04/WP05/WP07/WP09/WP25，同时记R02/R04；原26WP完整验收仍0。

T34.12 catchup续租修复定向验证（2026-11-05，进行中）：`readCompleteWithLease` 已在普通/重组完整块读取中接入；每块前后续租，来源读取deadline为原renewed+TTL-min_commit_remaining，保留撤销窗口，publish前再次续租。两项TestCatchup（总耗时超过TTL仍保留Current/慢读deadline内拒绝且保留ready供撤销、续租失败不发起来源读取）及原bounded/deep fork测试通过；包含race的Go完整make check通过。writer Postgres权威边界抽为最小interface以作确定性失租测试，不改生产Postgres/Kafka后端。新的全部五binary来源进程已重启，最终四分支正在重跑。独立审查正在复核控制/撤销路径；该修复效果以新的scheduled长追赶实际故障结果为准，不用定向测试代替进程证据。

T34.12撤销延迟根因进一步核对（2026-11-05，仍未通过）：逐块续租后的scheduled83bank/145API仍在首个getter故障复现ownerfalse/routertrue H76，证明前述续租不是该实际问题的完整根因，撤销门禁尚未修复。新代码追踪确认Rust quote-api将Reader.poll的None一律当无消息break，但None也表示已消费BEGIN/CHUNK、unit仍未组全；导致长批次每Tick只取一帧，revocation被积压。新真实encode+Assembler三unit含revocation/单BEGIN不可发布定向测试已先红缺helper，正在实现原max_tail_records帧预算内根据cursor进展持续drain、仅complete Unit推进applied cursor。此前Go续租/来源预算保留为独立加固，不宣称解决远端延迟。另慢head旧实现在0.70秒回归真实红（耗尽400ms lease），将实时head与错误后fork probe同样约束在来源窗口后，TestCatchup/TestSlowLiveHead/原bounded/deepfork含race通过。所有新增修复仍须真进程验证，不加长25秒故障门禁，T34.12持续IN_PROGRESS。

T34.12 Kafka分帧排空行为红绿（2026-11-05，未最终验收）：新 `drain_kafka_units` 在原max_tail_records帧预算内，只有cursor无进展且无完整unit才停止；partial BEGIN/CHUNK保留assembler且继续拉取，只有完整unit及其commit cursor进入Replica。测试真实encode/Assembler的两BLOCK_COMMIT+revocation，恢复旧break行为得到0而期望3真实红，恢复新逻辑转绿；单BEGIN预算仍无complete unit/无applied checkpoint。新Go慢head/fork probe共享来源窗口、逐块renew/commit前renew的race回归均通过。当前重建Go/router优先重跑scheduled长批次25秒故障门禁，未以单测替代进程；旧首轮“续租即完整根因”结论已被新证据否定并更正，独立审查与完整验收仍开放。

T34.12 B5证据审查两P2修复（2026-11-05，进行中）：独立复现after的getOwner与ABI一同伪造、仅slot9与packed/getter矛盾、虚构quote link/index、伪造完整响应身份和输入摘要等反例；原校验曾接受，新增离线反例先红后绿（两项plan测试18.219秒通过）。原checker已导出复用 `validateBoundBank`，selected after持续验证原runtime身份、immutable getter/slot9/slot11/flags/最小锁定；离线报告须由调用方独立提供实际API输入，报告保存输入确定性摘要，逐link核对索引/ID/namespace/source/request/routes且执行quote必须是原输入中的完整响应，允许相同执行签名去重但不允许虚构覆盖。投影前重验，smoke保留原输入文件并显式传入。checker源码/manifest变化后正在按新真实VM重生原bank及四投影，旧原SHA与报告均仅历史，不能保留旧资格标签。

Kafka drain修复后的scheduled反例进一步澄清：25秒诊断时fault_hits=0、owner与两router同处合法H130，故障targetH131尚未读取；这次与此前ownerfalse/router落后H75/76不同，不能当“实际读失败后撤销慢”。来源回放工具现独立记录选择H→首次poison实际被读的catchup（60秒有界），再从首次实际拒绝开始保留原25秒双router共同撤销预算；新增poison计时只首次设置/恢复清除的测试先红后普通与优化各3测试通过。没有延长读失败后的撤销时限，长批次仍完整覆盖；此前真实失败报告继续保留FAILED。新的资格与计时修复均需独立复审和实际进程，T34.12仍未验收。

T34.12重新绑定及scheduled真实故障阶段结果（2026-11-05，进行中）：新增完整after绑定的原参考四份已正常VM重生且byteexact --check通过，51,222,289字节当前SHA256 `017832e20925231b8f374e832bce7fefddebff867e1ab537380e0e05f873aa7f`；四完整Pool投影当前SHA256 `fc8eb3a14977b88acc93ed43ef9728ca815260338596efcae44cd9d6df45700e`，旧e32f/a952身份仅历史。新raw→Go来源286Pool/config→两Rustdomain/wire→原整数/完整后状态/逻辑路由/optimizer/HTTP集中差分通过；20项helper普通202.626秒通过，优化进行中。scheduled新原83来源及正常API已通过，首个真正被读的getter故障两router约646ms撤销并恢复，显示partialframe排空后的实际行为与此前落后H75/H76不同；其余故障及原实际计划重放仍运行，尚非最终验收。

deadline审查补强：原helper仅查询前检查deadline，可能接受25内起、26秒返回503；新回归已先红，新 `wait_for_withdrawal` 固定first-poison monotonic+25，单query预算不超过remaining且返回后再次比较，记录两router各自观测时刻/latency，quote拒绝/Solana额外probe另列。普通及优化各5个serving/deadline测试通过，当前长跑启动时尚未加载该最后helper，仍只算阶段证据；最终必须用最后冻结版本重跑。原两B5 after/input-link P2已独立关闭，此deadline补强和更广raw绑定仍在定向审查，所有生产/完整WP状态不提升。

T34.12 scheduled消费修复实际阶段进展（2026-11-05，尚未最终验收）：新原参考的scheduled83source/158实际API、6类来源故障撤销恢复已完成，记录144–692ms（getter/word/reserved/wallet/fee/futurehead）；正在router重启及实际计划重放。该进程启动后deadline helper和其测试进一步补强，最后输入绑定必须拒绝其作为最终新冻结，原始结果只作阶段记录。helper本轮普通20项202.626秒、优化22项188.129秒均通过，但清单不同，不并称同冻结22项通过；最终需按最后同一22项清单重跑。新strict deadline普通/优化各5测试已通过，仍待独立复审关闭。

T34.12 AUM raw同类矛盾补齐（2026-11-05，进行中）：独立核查确认slot10 raw_word与storage的镜像一致不足以约束getter。原checker新增按60/32/unused布局的AUM解码绑定；原after报告raw费率/收取时间/高位镜像同步篡改3例先红，正常/恢复有效费0正控及5反例通过，普通3测试12.263秒/优化3测试13.332秒。Go正常BPT解析入口同样增加固定H 0xa读取，原3个raw矛盾曾真实发布成功而先红；修复后完整145bank/286支持Pool、8来源反例与合成recovery底层非零/有效零及reserved拒绝共9.318秒通过；合成overlay明确不是原VM recovery新能力证明。checker/manifest变化后四原VM参考已新生成51,222,289字节，当前SHA256 `4cbf03d19a773ba374d8df87e4e3231798f042f576addeb5532300af0bb892f8`，四投影也已按新raw重建；017832/fc8等旧身份只作历史。新原byteexact、新冻结全检/22+新增helper清单/strict deadline全进程与独立复审仍需完成，T34.12未升级验收。

T34.12最后生命周期测试隔离与严格计时阶段（2026-11-05，进行中）：Go slot10新冻结完整make check（含race）通过；23 helper普通286.502秒通过，优化还在执行。新raw4cbf03原VM --check字节一致和独立C++ wire --check均通过。旧router完整检查又暴露两个同类macOS冷image启动耗时测试（stdout期望limit却timeout、stdin取消8秒无PID）；这些监督生命周期test现用实际小型sh→exec sleep/dd或exit7，保持原8/10秒deadline、stdout1MiB/stderr64KiB限额与reap/PID断言，生产worker身份/协议/真实EVM不改。完整local_simulation8passed/1ignored、13.90秒通过；该测试覆盖隔离仍在独立质量复核，旧冻结输入因此失效。

严格first-poison monotonic25秒版进程当前settled14src40API6fault/恢复/重启通过（84–696ms），scheduled83src/首fault128ms已通过，control/limits和实际进程选中计划原VM/整个after仍待完成。源代码后续修改必使该进程最终绑定判历史，不能提前提升状态；最后必须统一冻结再跑。原设计55协议、26WP/192条对应关系持续保留，完整WP仍0。

T34.12新原闭包阶段收口（2026-11-05，未最终验收）：raw `4cbf03...892f8`的byteexact原VM与独立C++wire均通过；四投影当前SHA256 `84307dc10b1a0232c324f9289502a161528e74098df250651832fb2d7d2fdbe4`，来源/槽10正常与合成Recovery边界完整Go差分及race全check通过。最后同一23项BPT helper清单普通286.502秒、优化219.950秒通过。源码审查全部已报告P1/P2（包括strict deadline与slot10）均关闭；生命周期测试小进程替换质量审查通过，无生产预算/语义更改，实际worker冷启动/协议覆盖另由已启动的原evm_execution_smoke执行，不以小进程单测冒称。strict四场景进程现settled14src40API6fault/重启、scheduled83src158API6fault/重启通过，controls正在开始；最后supervisor测试注释/source变化使本轮最终绑定仍只能历史，待统一最终冻结再跑。已将20份完成的红绿日志保全为 阶段红绿证据（`integration/artifacts/local/managed-v2-bpt-red-green-evidence.json`，已清理），不当最终同源验收。

T34.12实际worker与四场景进程阶段证据（2026-11-05，未最终验收）：实际 `evm_execution_smoke` 已12checks/7隔离模拟/2来源fault与显式reload/restart通过，真实worker binary/协议/执行结果覆盖明确，runtime与inputs均匹配当前 `managed-v2-bpt-freeze.json`，不以tiny生命周期单测代替；原公开VERIFIED/build继续关闭。strict BPT四场景已经settled14/40、scheduled83/158、controls25/22（12拒绝）、limits19/30（18拒绝）来源/实际API；总141支持bank/250成功报价/30明确拒绝、24来源fault/4router重启完成，正在全实际选中计划原VM重放。由于本轮startup source fingerprint早于最后supervisor测试与注释，已核实其runtime不匹配当前freeze，即使amount/native通过也只能历史FAILED；inputs一致不补救source不一致。最终canonical verify在当前freeze独立运行，早期已失配的canonical拥有进程组已停止并记录 取消原因（`integration/artifacts/local/managed-v2-bpt-superseded-verification.json`，已清理），不让旧进程覆盖最终报告。新视角只读质量审查正在进行；最后同冻结BPT全流程仍需fresh run。

T34.12最终质量负例P2补强（2026-11-05，进行中）：新视角质量审查复现smoke原 `code!=200` 会将500内部错误错记业务拒绝。已补 `validate_rejected_case`：响应只能含error，零输入仅400/INVALID_REQUEST；非零原revert/零output仅404/NO_ROUTE或400/NO_EXECUTABLE_ALLOCATION；500/503/504/429/无关INVALID_REQUEST或quoteId均拒绝。正式回归先红后普通6项1.048秒、优化6项1.094秒通过，历史30实际负例审计确为20*NO_ROUTE/6*NO_EXECUTABLE/4*zero INVALID_REQUEST，无500实际伪阳性。最后BPT清单因新增该回归变为24项，已重新启动普通/优化同清单；该改动使当下进程输入绑定仍不可最终接受，待质量复核关闭后固定24项freeze再跑。原production/source fingerprint未因这个tool-only修复改变，canonical全检可继续；inputs绑定必须重新核验，不能绕过。

T34.12最终24项与真实worker复验（2026-11-05，进行中）：最后同一24项helper普通174.623秒、优化296.916秒通过，来源/routing/spec及质量已报问题均关闭。固定24输入后BPT四场景已完成全部141source/250API/30拒绝/24故障/4重启，当前正在100实际选中原签名交易后大报告重校验及完整Pool差分，启动runtime/inputs与freeze相同。canonical全检已Go/Rustparser通过、router workspace测试和doc tests通过，contract-test/最终build仍运行。实际worker复验在最后router重启后模拟返回SIMULATION_TIMEOUT，诊断30秒预算内失败且未发布成功，报告已保留 失败现场（`integration/artifacts/local/managed-v2-bpt-worker-final-input24-timeout-history.json`，已清理）；不以旧pass替代、不提高预算，待当前重计算结束后在同一binary/输入串行复验。最后目标保持T34.12与原全部设计功能，尚未验收。

T34.12最终串行验证恢复：前次执行句柄已消失且ps确认无验证进程，磁盘仍为未完成的RUNNING，不视为活跃或成功。已重新核对当前source、五binary和13,013项输入与最后freeze逐项完全一致，将中断验证保全并明确标记INTERRUPTED。当前由持久脚本按verify→原reference/wire→同24项普通/优化helpers→实际worker→BPT四场景顺序运行，阶段命令/退出码与日志保存在 串行验证（`integration/artifacts/local/managed-v2-bpt-final-serial-run.json`，已清理），避免大额原VM和worker冷启动并发；每阶段前后仍比原freeze，不放宽预算或恢复旧失败报告。当前串行verification的Go/Rust parser完整check均已通过，router完整check进行中；之后还需原reference/wire、同24项helpers、实际worker和BPT四场景在本次freeze重新通过。T34.12继续IN_PROGRESS，原26完整WP仍0。

T34.12之后的R06执行次序已确定（均未开始，不改变设计基线）：先补原发布版已启用circuit breaker，再处理rate/mixed及剩余会计组合。breaker必须作为独立版本显式建模每token原slot12完整256-bit word、getter边界与当前weight依赖，原压缩/解压/缓存命中和cache外重算的pow方向、完整join/exit对所有币种价格检查逐项差分；当前BPT旧profile继续拒绝启用breaker。依次固化原controls小额成功/大额回退及schedule/双物理池序列→common/Go/两Rust状态与wire→固定H parser→精确math/完整postPool→实际API和来源fault/恢复/重启，并保留错误拒绝的整path原子性。下一项在本批最终冻结结果明确前只做方案核对，不混入运行中的源码闭包。

R06下一项breaker只读覆盖核对：当前原参考870条token级观察中仅controls/B25–B28共4条启用breaker，全部current weight等于cached reference weight，cache-miss分支为0。已在隔离runtime保留原slot12/6 getter/weight/scaling映射作后续输入准备，未改生产profile、未执行新数学、不算breaker实现。新版本正式验收必须补原VM活动权重导致cache外pow重算、bounds压缩边界/0禁用、join/exit全币种检查与共享执行，不能将现4bank局部范围提升为完整支持。

R06 breaker来源准备增量：在隔离runtime完成固定slot12字段、32-bit缓存解压及lower ceil/upper floor后的getter视图核对，870条原观察逐项相同；其中启用breaker仍仅4条cache-hit，未实现cache-miss pow、未做新交换或production接入，因此下一功能保持未开始。该原型和数据不进入当前BPT源码/输入冻结，最终验收仍按串行命令与严格原证据推进。

T34.12串行复验中的审查记录恢复：已逐文件核对最终review，发现磁盘仍保留负例helper修复前的PENDING/旧摘要，属于前次中断后关闭记录未写回。已依据独立质量审查的实际关闭结论恢复CLOSED并保留旧记录，只有已审过的smoke负例文件摘要更新，其余审查源无变化；再次运行最后6项serving/负例/deadline普通4.417秒、优化3.875秒均通过。review记录恢复不等于集成验收，当前仍等串行验证；源/二进制/输入freeze未修改。

T34.12当前freeze完整验证通过：canonical `verification.json` 已passed，Go/Rust parser/router/common各make check、两debug binary、238契约向量/8fixture/服务边界共8条完整命令exit0；source_unchanged、verification_tool_unchanged为true，source_tree及全部五binary逐项匹配最后freeze。串行报告verification阶段已exit0，正执行原reference/wire byteexact，随后同24项helpers普通优化、真实worker和BPT四场景。此刻只证明完整检查阶段，不提前将T34.12或任何完整WP升级；其余阶段仍须同输入/二进制完成。

T34.12最后串行24项及实际worker通过：当前freeze完整canonical验证source/binary均一致、8命令exit0；原reference145bank/116尝试与C++wire字节复现通过；同一24项helpers普通215.075秒、优化198.819秒通过；实际worker串行12checks/7隔离模拟/2来源fault/reload/restart均通过，runtime/inputs逐项匹配freeze且inputs_unchanged=true。串行报告前四阶段exit0，现只剩BPT四场景真实流程及其97计划/100原交易/逐步完整Pool对照正在新启动，未提前升级任何验收状态。

活动 schedule 正式实现计划（T34.11；保持§11.3/12.2/12.9/12.11–12.14约束，不改变系统设计基线）：

1. 在已接受多池源码闭包保全后新增独立 `balancer-v2-managed-scheduled` / `balancer-v2/20230411-managed-scheduled-1`，旧aum-3的终态权重/费用范围及活动schedule拒绝不变。沿用完整Managed账本，在 `BalancerManagedV2State` 新增可选field40 `schedule`，包含必需 `pool_state_word` 和有序token/state_word；公开新profile要求完整schedule，旧profile明确拒绝它。39旧字段及旧wire字节保持不变，已知零/presence/clone/未知字段/sourceH/derived隔离继续验证。公共Go/两Rust及generated types同步，新增独立C++ wire正反例。
2. 在 `domain` 的新 `managed_v2_schedule` 实现原字段解码与纯整数时钟视图：poolWord四个32-bit时间、两个直接62-bit费率、四flags；tokenWord直接64-bit起止权重和5-bit decimals difference，未用高位显式校验。computed start逐项合法性与caller target sumONE区分，不能要求当前权重或合法重排起点始终sumONE，不能归一化或使用tolerance。对原endpoint/raw/getter/source时钟做一一对应，目标参数必须保留原入口校验。
3. 固定原build-info/AST槽位与mapping定义后，在Go原EIP1898读取增加pool槽9和token mapping基址11的实际raw word，与原schedule/current getters、flags、精度和身份逐项核对；同一blockHash读前后重锚，不以派生状态当新canonical。config/source dispatcher显式按新profile接入，不自动解除旧profile限制；未知字段/缺word/非法layout/错锚点仍失败封闭。
4. 在Rust `protocol-math` 新增独立scheduled输入/精确selected-weight比率实现，保留原upscale→fee/uint256/u112/MAX_IN_RATIO/实际钱包/AUM账本语义；普通分派与共享物理池重算使用同一整数内核。转换时仅recompute给定执行clock的view，raw schedule与sourceH不改、所有成功转换始终Some(ctx)；同Vault兄弟如执行时钟推进，也重算各自费用/权重/AUM/pause view后再传播共享钱包，不更改它们的注册资金与BPT供应。
5. 正式固化双币、三币、活动重排原VM参考（含computed start ONE±1及后续ONE±2、非法caller target原回退），逐项匹配完整swap/collection后状态、最小单位、未来/边界时钟和原management来源观察；manager控制观察与本地控制数学实现继续分开，不将155原型测试当一般参数证明。原参考/pins/许可证、生产者及全部消费者身份绑定，代码/输入变化后旧验收只作历史。
6. 最后接入普通source→PG/Kafka→双router的活动schedule/终态回归、跨池组合、来源故障撤销恢复/重启与真实选中计划原VM余额重放；三服务/common完整check、普通/优化helpers、C++ wire及独立SPEC→quality同源门禁通过后，才升级对应任务状态。BPT/rates/mixed完整会计、breaker与真实链/公开执行等余项仍按原功能表保留。

执行顺序：主线程先基于原源码确定公共状态与精确AUM算术；原发布参考正常链路可独立固化，固定块parser随后按已声明schema并行；普通swap与显式collection分开测试，不允许用swap时虚构BPT铸币。基础Weighted不变量/幂函数仅在版本差分通过后复用。原getter/代码identity不完整、未知rate/breaker/token能力均失败关闭；动态权重/费用、启用breaker、BPT swap/join/exit、实际主网来源及其他Managed版本仍保留后续设计功能，不因本批边界删除。完成状态仍需实际源、原合约、数学/后状态、服务、故障与双审的同一冻结闭包。

独立SPEC审查补充：固定H普通共享重算通过，不代表未来clock派生连续性通过。已复现未来time swap后同clock再次swap/collect因旧source时钟错误拒绝，以及完整执行高度超过2^32后回退到source高度仍被接受；已增加单独派生执行上下文并完成连续clock红绿回归，不能修改sourceH或取消原storage低32位语义。进一步SPEC复现固定H首次转换仍可伪装canonical，已将所有成功转换改为显式derived，新增Snapshot/wire拒绝回归通过。实际钱包缺口已按T34.6贯通模型/原getter/数学/独立wire，保持registered与global wallet分离；仍需最终冻结验收。

本轮续作核对：三个原bank副本均为7905624字节、SHA256 `1ee7ec5dd981994af6dd42eb75717cf83a5b209857883fa2923885ce7181fb66`；精确再生记录见 `integration/artifacts/local/managed-v2-aum-reference-check.log`。新增getter不改变27案例/292交易的历史经济投影。部分原bank因长时间/uint32边界时间推进，时间晚于当前本地日期；正常服务必须继续按未来头拒绝，不能为测试放开时间准入。服务验收需明确可服务样本和边界拒绝范围。

本轮当前已实现单物理池范围历史最终验收通过（2026-11-04；后续T34.8/T34.9代码变动后不作为新范围通过证据，完整源码/报告已归档历史闭包（`integration/artifacts/local/accepted/2026-11-04-managed-v2-single-pool/MANIFEST.json`，已清理））：[同源验收报告](integration/artifacts/local/managed-v2-aum-acceptance.json)绑定三服务/common完整check、五个二进制、208份契约/8份fixture、当前全部原输入、独立SPEC与quality无遗留审查。原27case/292tx/2285对象、旧原稿完整两编译job的精确临时再生、81篡改普通/优化拒绝；独立39字段C++3023字节四副本/15自测；普通/优化各49辅助回归均通过。实际source→PG/Kafka→双router的15原来源状态、8正向报价、6暂停状态拒绝但保持ready、5Recovery有效零费率来源及退出恢复常规费、10来源故障撤销/新generation恢复、无关Solana报价及重启、VERIFIED/build拒绝，共18检查通过。选中4份API单跳计划由独立原Vault在新VM中重放前28真实签名交易，核对真实扣款/收款余额、完整getter/账户、原header/receipt。控制enable/disable只计真实原来源观察，不冒称已实现控制数学内核；独立执行仍是本地参考，不是可信链/生产执行器。

T34.8/T34.9新范围最终同源验收通过（2026-11-04，Asia/Shanghai）：[最终报告](integration/artifacts/local/managed-v2-multi-acceptance.json)14阶段全部PASSED，绑定当前源码/五二进制/全部参考输入、独立SPEC与quality及真实进程原VM证据；106辅助回归普通/优化全部通过。四池10银行/14计划/4故障与共享两池7银行/16计划/4故障通过，两个actor各2物理池拆单及实际收款7874328412914279650444一致；逐来源parser必读RPC/代码身份覆盖、API edge token与实际步骤绑定亦校验。原51/214等更早计数以各自历史快照为准；新接受闭包已保全到历史验收（`integration/artifacts/local/accepted/2026-11-04-managed-v2-catalog-shared/MANIFEST.json`，已清理）。

T34仍未全部完成，T34.8/T34.9仅按上述范围记LOCAL_SCOPE_VERIFIED。T34.11活动权重/费用已按上述标准underlying、2/3币及两个三币物理池有限范围记LOCAL_SCOPE_VERIFIED，BPT路由/启用breaker/混合会计与rates/通用collection传播、其他Managed版本、可信链/生产要求均保留，不因新局部验收减少原设计。T33及更早报告自后续源码/输入变更起为历史验收；原26工作包完整验收仍为0，总体目标仍为全部设计功能完成。

<a id="plan-2026-11-06-managed-v2-bpt-finite"></a>

## Managed V2 BPT有限GIVEN_IN：任务完成情况

设计对应原§7.3、11.3、12.2、12.9–12.14、23及WP02–05、WP09、WP11–14、WP16、WP25。固定20230411正常原Factory/Vault/ERC20与标准18/6/8三underlying、两真实物理池的本地历史范围；不改变系统设计基线或完整工作包门槛。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|B1|固定原版本与完整执行来源：145bank/116尝试，金额来自真实原钱包/事件；constructor/runtime/immutable、raw9/10/11/13及原signed history资格|WP03,WP11,WP25|LOCAL_SCOPE_VERIFIED|最终有限验收（`integration/artifacts/local/managed-v2-bpt-local-acceptance.json`，已清理）|
|B2|独立BPT profile/common41/Go与两Rust、required/unknown/wire/clone，286 Pool原固定H差分及slot10 recovery有效费率、来源错误整批拒绝|WP02,WP04,WP05,WP11,WP25|LOCAL_SCOPE_VERIFIED|最终有限验收（`integration/artifacts/local/managed-v2-bpt-local-acceptance.json`，已清理）|
|B3|原单币整数join/exit、先AUM后转预铸BPT，供应/注册/全局钱包/别名/sourceH与derived隔离、完整后Pool|WP11,WP13,WP25|LOCAL_SCOPE_VERIFIED|最终有限验收（`integration/artifacts/local/managed-v2-bpt-local-acceptance.json`，已清理）|
|B4|唯一物理池逻辑BPT边，sender/recipient贯穿search/optimizer/HTTP；连续/拆单整数重算、known跨V2钱包修复与失败整path原子性|WP11,WP12,WP13,WP14|LOCAL_SCOPE_VERIFIED|最终有限验收（`integration/artifacts/local/managed-v2-bpt-local-acceptance.json`，已清理）|
|B5|source→PG/Kafka→双router/141bankCPP、250报价/30业务拒绝、24fault/4重启；97实际计划100原交易与逐步完整after；同freeze全检/24双模式helper/真实worker及两阶段独立审查|WP03,WP04,WP05,WP09,WP11,WP12,WP13,WP14,WP16,WP25|LOCAL_SCOPE_VERIFIED|最终有限验收（`integration/artifacts/local/managed-v2-bpt-local-acceptance.json`，已清理）|

最后闭合结果：串行五阶段命令全部exit0，canonical8完整命令/source_unchanged/five binary通过；原参考与C++wire byteexact、同24项普通215.075秒/优化198.819秒、真实worker12checks/7隔离模拟/2sourcefault/重载重启，随后BPT四场景全部通过。实际250输入逐项link承诺对应97原VM计划100交易（含一项两路四步），原after重验及每步双方完整Pool差分通过；最后独立核对source/binary/input/日志摘要/审查源均一致。原参考SHA256 `4cbf03d19a773ba374d8df87e4e3231798f042f576addeb5532300af0bb892f8`，四投影 `84307dc10b1a0232c324f9289502a161528e74098df250651832fb2d7d2fdbe4`；串行原记录（`integration/artifacts/local/managed-v2-bpt-final-serial-run.json`，已清理）、独立审查（`integration/artifacts/local/managed-v2-bpt-review.json`，已清理）、红绿记录（`integration/artifacts/local/managed-v2-bpt-red-green-evidence.json`，已清理）。

旧运行的source/input变更失败、中断及worker预算拒绝保持原失败/历史身份，没有改写为通过。当前仅上述有限GIVEN_IN版本范围LOCAL_SCOPE_VERIFIED；enabled breaker、rate/mixed、其他custody与自收付完整会计、一般2–50参数、ExactOut、真实链/完整证明/Gas/public原子执行及生产运维继续按原R01–R21完成。跨V2资金修复有单位/完整本地路径回归，不能外推跨协议原交易完整验收。生产VERIFIED/build保持关闭，原26完整WP验收仍0，整体目标继续。

<a id="plan-2026-11-06-managed-v2-breaker"></a>

## Managed V2已启用circuit breaker：任务完成情况

原设计§7.3、11.3、12.2、12.9–12.14及R06；延续固定20230411原源码/正常Factory/Vault/ERC20，新增独立breaker能力版本。当前BPT旧版本的已启用breaker拒绝保留。缓存原型只有870 getter视图、4启用cache-hit观察，不能充当本批原交换或动态分支验收。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|CB1|独立固化原VM cached/dynamic/boundaries/shared分支，覆盖18/6/8精度、cache miss、原小额成功/大额回退、单/双边0禁用、跨物理池与时间/管理控制链；只取原钱包/事件/完整getters给期望|WP03,WP11,WP25|LOCAL_SCOPE_VERIFIED|fresh原VM300 bank/244 case、98,268,052字节与原参考逐字节一致；cached/dynamic/boundaries/shared及控制/clock/账户归属反例完整通过；[限定最终验收](integration/artifacts/local/managed-v2-breaker-local-acceptance.json)|
|CB2|独立breaker profile与完整有序token slot12 uint256 state；Go/两Rust/common/wire/clone/known0/缺失拒绝、原getter按decimals还原、旧版本隔离；原caller BPT price按raw token单位输入|WP02,WP04,WP05,WP11,WP25|LOCAL_SCOPE_VERIFIED|field42完整raw+6getter、600Pool Go/两Rust/独立CPP wire、clone/known0/缺失拒绝与旧profile隔离通过；20,067字节wire逐字节一致；[限定最终验收](integration/artifacts/local/managed-v2-breaker-local-acceptance.json)|
|CB3|精确cache hit32-bit解压与cache miss原powUp/Down，lower ceil/upper floor、actual supply及scaled balance；regular单边检查与BPT join/exit对全部token双边检查、原后Pool和原子拒绝|WP11,WP13,WP25|LOCAL_SCOPE_VERIFIED|244原case的金额/拒绝原因/完整Pool与时钟、230实际API计划233原交易每步双方完整后状态通过；cache/pow/scaled dust/未来AUM等反例通过；[限定最终验收](integration/artifacts/local/managed-v2-breaker-local-acceptance.json)|
|CB4|普通固定H parser验证raw/getter/library、sourceH/derived时钟、Funds图/search/optimizer/HTTP与共享钱包；未来weight重算bound，保留各物理池注册/供应|WP04,WP05,WP09,WP11,WP12,WP13,WP14|LOCAL_SCOPE_VERIFIED|300来源bank/600Pool固定H经PG/Kafka双router、516成功API、可恢复金额候选与共享钱包/容量两路四步均通过；24故障恢复/4重启通过；[限定最终验收](integration/artifacts/local/managed-v2-breaker-local-acceptance.json)|
|CB5|source/PG/Kafka双router正常/失败/恢复/重启、实际选中计划原VM每步完整Pool、最后freeze全检/双模式helpers/独立SPEC→quality和逐项进度闭合|WP03,WP04,WP05,WP09,WP11,WP12,WP13,WP14,WP16,WP25|LOCAL_SCOPE_VERIFIED|同freeze三服务/common全检、fresh reference/wire、44 breaker+17 progress普通/优化、7真实worker仿真、四mode进程及230计划233原交易/完整Pool、6SPEC与独立quality和结构audit全部闭合；[限定最终验收](integration/artifacts/local/managed-v2-breaker-local-acceptance.json)|

以下按发生顺序保留开发与验收记录；历史运行中/失败状态不覆盖上表最新限定结论。

CB1正式开发：独立 `managed-v2-breaker` 原参考/CLI/精确source manifest已落盘，入口与旧输出保护2测试先红后绿（4.340秒）；native覆盖测试因新原数据缺失先红，当前正常原VM cached/dynamic/boundaries/shared四分支正在生成。新参考记录额外原CircuitBreakerLib纯调用以独立约束cache-miss ratio/getter，预期兑换量只取原钱包/事件；未调用本地quote/pow作为输出oracle。CB2按已固定契约与CB1分离文件实现：每token原word和6getter，optional field42/新profile必须完整；旧profile字节和拒绝保持。进入新批后前批T34.12验收只指其已保全freeze，不能拿旧输入指纹当本批通过。

CB1/CB3阶段进展：第一版原257bank/206swap已通过，随后新增纯公开退出形成的6位币种tiny reserve（无storage injection），原297bank/242swap参考已生成；相同输入启用breaker原356回退、禁用后成功且raw output0，两类BPT exit/regular证据直接证明检查必须用downscale前scaled amount。新native入口/覆盖测试普通6项8.838秒、优化6项5.958秒通过；后又补orphan/原prefix/clock/shared见证精确归属反例，旧接受先红，新图验证108.405秒通过，输入manifest已变，正待SPEC后最终原VM再生和byteexact，不据旧hash当当前资格。原始库视图/比较方向/disabled除零短路/uint256边界3测试通过；内部计算与Vault raw结算已拆分，新增5测试和旧4 BPT/4 regular数学回归通过，旧positive-output政策保留。尚未接完整breaker Pool或普通API。

CB2自审收到后主线程按原源码发现upper合法范围被错误限制>=ONE（原只要求upper<=10ONE且非零upper>=lower），已要求三端先红后修，补upper0.5/lower0与upper0.2/lower0.1正控及反序拒绝。原参考正在补实际upper0.9初始已tripped、小额拒绝而大额join回到界内的金额感知见证，防止后续预先剔除“当前越界但可恢复”的池。CB1/CB2/CB3均保持IN_PROGRESS，未开始CB4/CB5。

CB1来源SPEC审查修复：两P1（控制nonce/nativegas账未验、场景自洽删尾/非swap signed交易失归属）及一P2（clock被Map去重）已正式反例先红；现每control signednonce/所有账户nonce与gas/coinbase转移、clock账户/原钱包不变、初始化后全部交易与case/control双向唯一、独立scenario顺序/完整status/label清单与clock唯一均已补。非法管理操作预期也独立固定，7status同步receipt篡改回归先红后绿；control/clock/completeness反例90.764秒、intent4.937秒通过，新原VM300bank/244swap与来源校验已生成98,268,052字节当前SHA `82e5417a73bbbd84da1858ad414c22e225bcfb91cc14f0f85bb3a0d6b8e471de`，四投影7,724,386字节SHA `07ccfccef704a6981be5c6e8661616d31a18ced4598bcb569eb0070958bb04f4`。当前14项reference/projection/wire辅助普通300.328秒通过，最终byteexact/优化/两阶段审查仍待完成，未升级CB1。

CB2及CB3/4原差分：field42独立C++ synthetic wire四份20,067字节SHA `e164e85de5e99f0ffd98f41f86f0cc7cdd1d3c8b244110ee887b5e4eb9d57aa8`，Go与两Rust完整wire600Pool通过；upper原合法范围已三端红绿修正。Domain独立SPEC发现BPT旧profile在dependency/capability缺state时仍被漏验，扩为四Managedprofile×缺multi/缺managed的两入口红绿，三端已修并focused通过，等待复审。CB3所有原244swap的金额/356错误原因/整个后Pool及26clock后动态view完整差分通过，含BPT与regular内部scaled dust检查和amount-dependent恢复；6原语/4旧BPT/4旧regular与clippy通过。CB4 normal固定H600Pool13.201秒、7来源矛盾原子拒绝2.554秒通过，动态view使用原linked library纯call核对；新独立config三端先红后绿，两Rust各9config/2wire测试通过；普通逻辑路由3测试9.10秒通过，正在扩原shared四步双方完整after和HTTP，CB5尚未开始。

CB4/CB5整合进展：原routing4测试4.60秒通过，新增shared原两路四步逐步双方完整Pool与wallet重算一致；新HTTP金额/356拒绝/可恢复amount/dust0边界正在跑。普通新配置使用独立standard-breaker-bpt政策，保持BPT旧profile不接受；source动态bound由同H原CircuitBreakerLib纯call与固定runtime身份/slot12共同验证，所有600原Pool与7篡改拒绝通过。CB5服务工具开始覆盖所有ordinary TOKEN与BPT操作（含原dustraw0业务拒绝），真实API选中计划将独立恢复原来源后逐步执行新raw与6getter边界，完整after投影再以实际block逐步比较；工具当前仅落盘，尚无进程验收。

CB3独立SPEC发现P1并红绿修复中：BPTquote虽已reclock/AUM后定价，但check_bpt仍读函数入口旧Pool；原boundaries/B86(B)执行时间前推15604秒、1wei join原output0，未交换6位币种upper应被新AUM供给触发，旧代码却成功，已用不改raw的原fixture实际先红。现执行检查统一shadow到current execution view，与pricing同weight/actualSupply（等于收取后supply），保留source H；加入future零output触发及12动态双方向显式reclock等价回归，完整新原/旧BPT/schedule套件正在复跑，未提升CB3状态。

CB4 HTTP scope核对：完整双物理池的ordinary TOKEN请求可能绕开原B池的breaker而用健康A池；不得把原单池revert外推全局NO_ROUTE。单Pool callback拒绝HTTP测试显式限制合法原单池scope，已通过；完整双池服务保留全部候选和实际替代计划，输出须在CB5实际API原VM重放而非硬比原单池回退。三服务契约向量读取修正为真实local引用，247契约/8fixture检查通过。

CB3 P1已定向转绿：原boundaries/B86未改raw、未来执行+15604秒/1wei/output0旧代码成功的反例现正确CIRCUIT_BREAKER_TRIPPED；12个dynamic双方向explicit-reclock等价、全部244原swap/26clock完整后Pool与旧BPT4/旧schedule8一同通过（newPool4测试3.22秒），已请求独立复审。CB2 helper漏BPT旧profile的审查P2已三端红绿修正，验证与capability入口对缺multi/缺managed均失败，circuit11/dependency11 focused通过。CB4原shared四步双方完整after、单池HTTP1测试2.71秒通过；CB5 strict serving6项普通1.812秒/优化1.670秒和actual-plan离线3项13.253秒通过，配置/契约已247向量8fixture。已新增Make reference/projection/wire/helper/differential/smoke入口与input闭包，正在重建当前router后首次cached真实source/PG/Kafka双router+原API计划重放；该first-process仍属开发验证，未开始最终freeze全检/质量验收，不提升CB1–CB5。

CB1–CB3定向复审结果：原来源P1/P1/P2及非法控制status补P2，Domain helper旧BPT声明漏验P2，未来AUM/weight旧视图P1均已各自独立复审关闭；CB1四份当前raw独立普通文件且byte相同、top inputs精确等当前closure，仍需第二次freshVM --check与最终质量/冻结。CB2只结构验证cache-miss的边界明确不外推pow，CB3价格前完整动态视图验证已原参考通过。CB5首cached实际进程已28source、46API、12业务拒绝、5sourcefault阶段通过，正在最后fault/重启及实际API计划原VM；CB4/CB5另独立SPEC正在审查，整批CB1–CB5保持IN_PROGRESS。

CB5首次cached真实进程结果（未最终验收）：28sourceCPP/50API/12业务拒绝/6sourcefault恢复/router重启完成，18实际API原签名交易和after资格重验通过；最后完整模型consumer因环境变量大小写错写未加载向量，原报告准确FAILED且已保留。新增命令接线回归先红后绿，工具统一compare_selected_vectors传XONE_BREAKER_SELECTED_VECTORS；使用已保留真实向量明确执行Rust ignored consumer后1pass/0.29秒，仍不回写旧报告为pass。正跑修正后dynamic真实进程，其余场景和最后同freeze全量/双审未完成。

CB4/CB5独立SPEC结果及动态进程阶段：来源/config/profile/adapter/purelib/after资格/selected-input链路定向SPEC通过，无新增可复现P0/P1/P2；其独立短校验对accountnonce、executionclock、tokenunits、rawcircuit、缺peerafter、未绑定实际response六例全部拒绝，接线与strictdeadline3测试通过。dynamic首真实进程已163source、302实际API、6sourcefault恢复完成，正在重启后actualAPI原VM；CB5最后source/input绑定、完整模型和四场景finalfreeze尚未通过。新一轮源/数学/route/API集中focused与原byteexact都在运行，仍不升级CB1–CB5。

CB5动态真实验证与最终质量准备：dynamic已163source/302API/6fault/restart阶段完成，actual-plan原VM仍运行；原第二次fresh byteexact也仍活跃。完整24项辅助优化轮已出现错误标记但未结束，必须等完整异常定位，不能先报通过或在原进程未终止前重跑。source current closure已再次核对精确一致。新整合质量只读审查已启动，范围跨新状态/source/math/tools的契约与伪阳性，CB1–CB5仍IN_PROGRESS；不得将前批BPT最后freeze当本批证据。

CB5辅助优化轮环境问题已定位（不改门槛）：一次直接python -OO discover未使用正式Make固定PATH，默认Node24.22.2与固定原Node25.3.0不一致，投影资格出现E，整轮仍在结束中；应保留失败并待其终止后按正式Make入口以同清单重跑，不把Node版本校验关掉或将该轮称通过。dynamic actualplanVM与第二freshreference byteexact现均仍活跃，尚无最终资格结果。

CB5 dynamic开发阶段完整通过（有限场景，非整批验收）：163source/302实际API/6fault恢复/router重启、来源signed history174交易重放、144不同实际API计划144新原交易及每步双方完整Pool差分通过；report passed且运行前后source/binary/inputs一致。已保全dynamic-development独立报告/输入/native/vectors/log，开始boundaries真实场景，不把单dynamic通过当四场景完成。原300bank244swap98,268,052bytes fresh --check已byteexact，独立C++ wire也byteexact；优化错Node24整轮终止为FAILED(errors=1，22tests1260.310s，projection setUp未执行两test)，错误环境已保留，正式Make双模式24项正在重跑。新的quality整合审查仍进行中，最后freeze/canonical全检和shared容量actualAPI门禁未完成。

CB5质量结果及全量检查失败记录：整合quality只读审查通过，拒绝原子性/索引保护/current execution view/actual-plan与范围绑定未发现新P0/P1/P2；未把spec重复或长run结果冒称独立复验。当前候选freeze的source/binary与13,041输入再次核对无变化；正式Make同24项普通325.420秒、优化466.516秒通过。Go full check中的EVM包触发默认10分钟包级超时，彼时当前旧snapshot子测仅运行4秒、栈为JSON解码；同时原VM重放占用资源，未证实单测死锁，保留FAILED，待原VM结束后按相同-race/默认预算串行复跑再判断，不改超时门槛。boundaries97source132API18业务拒绝6fault/restart阶段通过，原actual-planVM仍运行；Rust/router full检查仍在后续执行，整批尚未验收。

CB5 boundaries阶段进展：97source/132API/18精确业务拒绝/6fault恢复/router重启通过；actual-plan原VM59计划59交易已成功写出，当前在原after/独立input重验与完整模型比较，尚未最终passed。Rustparser Clippy全target阶段通过，workspace编译/测试继续；Go包级timeout保留失败，仍待原VM结束后不并发重计算的同预算复跑。source/binary/13,041输入与候选freeze再次全部一致；最终CB1–CB5验收仍开放。

CB5 boundaries开发阶段完整通过（有限单场景）：97source/132实际API/18业务拒绝/6sourcefault恢复/router重启、59不同实际计划59原交易及逐步双方完整Pool差分均passed，source/binary/inputs前后一致；已独立保全boundaries-development报告/输入/native/vectors/log。新shared真实流程已启动，重点验证两物理池40%容量实际两路四步API和原完整after。当前cached旧报告仍保留wiring失败而非冒称passed；dynamic/boundaries各自通过不代替最后四mode统一freeze。正式Make24项普通325.420秒/优化466.516秒均通过，原fresh byteexact300bank244swap也通过；Go full package-timeout仍待不并发VM串行复跑，Rust/router full检查继续，不提升CB1–CB5。

CB5 shared开发阶段完整通过：12source/32API/6fault恢复/router重启、9不同实际计划12原交易（含两路4步）及每步双方完整Pool通过，source/binary/inputs前后一致；已保全shared-development文件。本批cached首次wiring失败保持历史，dynamic、boundaries、shared单场景通过仍不足最终四mode统一验收。原VM各阶段现已结束，准备在当前freeze串行canonicalverify→reference/wire→同24helpers普通/优化→actualworker→四mode全流程，Go仍保持原默认10分钟包预算而不并发VM，用新结果判断是否仍有测试性能问题。Rustparserworkspace已通过，旧fullcheck序列后续contract-test仍运行，最后收口前不得伪造passed。

CB5进入最终串行freeze门禁：cached首次wiring失败保持历史；dynamic163/302/144tx、boundaries97/132/59tx、shared12/32/12tx的开发流程各自通过并保全，源码SPEC各项与整合quality已关闭已报问题。当前freeze source SHA `e1af4c58a0b0d72483915f52849aa4d577cd45c22d62894b59e30d27ca383552`、13,041输入和审查文件摘要全部一致。此前Go包级超时后的自有并发check组已明确取消并保留原因，避免继续无效重复；现由持久隔离runtime脚本依次canonicalverify→freshreference/wire→同24helpers双模式→真实worker→四mode统一smoke，阶段失败即停止，Go默认10分钟/各业务25秒窗口不变。完整结果及独立逐项闭合audit通过之前，CB1–CB5仍IN_PROGRESS，原26完整WP仍0。

最终Go门禁的包级预算问题进一步确认：在不并发原VM的canonical串行轮仍触发EVM默认10min，当前TestV2CompleteAllNativeBanksIncludingReverts仅13秒，故不能再仅归因资源竞争。保持所有测试内容/-race/业务时限与每进程Go默认预算，不提高-timeout；新unit工具从checked go list发现全部pkg，nonEVM照常race，EVM从实际go test-race-list枚举Test/Example/Fuzz seed，95顶层按12个一组完整8shards、anchoredregex保留全部subtests。清单唯一/空缺/重复/失败传播/所有包发现与未知参数5测试普通优化通过；实际第一组116.550秒通过，后续仍在运行。此改变明确是把增长语料分进程预算，而不是保持原单包总wallclock预算；没有减少案例或race，当前developmentrun非最后freeze。分组工具正独立审查，最后需新freeze所有8组和canonicalverify完整通过，不因一组过而提前升级。

Go增长语料分组开发验证完成：EVM实际95个顶层测试全覆盖8组均通过（116.550/399.961/49.858/26.161/2.717/7.289/57.564/59.200秒，累计719.300秒，最大单组<原600秒默认预算）；其余所有Go包race、domain/config契约和build也通过，没有删原300bank/600Pool或旧语料subtest。该run启动时Make/分组wrapper尚迭代，最后checked-package-discovery和严格CLI5测试普通/优化已补通过，但需以最终入口重新canonical验证；源码新增Go test tools/Makefile使之前e1af候选sourcehash失效，不沿用旧freeze。分组工具独立审查尚在进行，明确保留每进程与业务时限、改变的是整包语料分进程的总wallclock预算，不能冒称整包仍在10分钟内。

Go分组工具独立审查两P2修复：原ASCII-only解析会漏Test中文/Fuzz中文/Example金额，普通go list也可能漏race-only包。正式新回归先红（Unicode清单只剩ASCII、race条件发现失败），现按Go Unicode letter/decimal-digit/underscore接受合法名，任何test-like非法项显式失败；所有包发现统一go list-race，race-only包保留于race执行命令。7项工具测试普通/优化均通过，定向复审待关闭。当前95名均ASCII、无race-only包，旧8组全过不意味着新工具最终验收；最后需新入口fresh check。Go正常test cache保持，清单恰好一次指分配和命令选择，不宣称每轮物理重执行所有test body；原完整单包600秒与8进程各600秒的总预算区别已明确记录，业务deadline不变。

最终race条件实际枚举已直接核对：Go list-race得到15个package，EVM go test-race-list得到95个Test名字，最终Unicode/race版分8组flatten后与实际清单完全一致且唯一；仅枚举不冒称此步骤物理执行test body。记录保存在actual-race-inventory；分组工具P2修复待独立复审关闭后才重定最终freeze/serial verifier，当前无旧VM或无效fullcheck继续争用资源，旧失败不恢复为passed。

CI分组两P2定向复审已关闭，实际race枚举15pkg/95test/8组完整唯一再次独立核对，7工具回归普通/优化通过。新最终freeze source SHA `817a1d9d4ae8125b920462eb857d77c8ebc4b9385a7b0f1a4964b3d7ce9ea9b8`，13,041输入与所有已审文件一致，CI Make/Python工具补入审查摘要。最后串行canonical Go完整check已通过：7工具自测、checked race package discovery、95/8 EVM race组、其余包、domain/config与build全部成功；Go部分有效cache明确保留不宣称每轮物理重跑。现Rustparser全检中，后续fresh reference/wire、同24helper双模式、实际worker及四mode统一smoke仍待该freeze全部闭合，CB1–CB5继续IN_PROGRESS。

本轮接续核对（2026-11-06）：剩余R01–R21及CB1–CB5逐项保留；上次最终serial报告停留RUNNING，对应进程已不存在，已保全`managed-v2-breaker-final-serial-run-interrupted-20261106.json`，按中断而非通过处理。继续沿原R06/CB计划，在已登记freeze核对源码/五二进制/完整输入后重新串行canonical全检→原参考/wire→同24helpers普通/优化→实际worker→cached/dynamic/boundaries/shared统一服务与原选中计划重放；最终状态以本轮退出码、报告及完整Pool差分为准，不据开发阶段结果升级CB或原工作包。

本轮方案对应检查：进度解析15项回归普通/优化双模式均通过；26个工作包/198条设计功能/56个协议条目及当前CB1–CB5五任务已逐项读取，breaker六条设计映射的源码路径和实际测试定义均存在，无失配。结构检查保存在方案对应报告（`integration/artifacts/local/managed-v2-breaker-progress-consistency.json`，已清理），只证明文档/源码/测试映射，不替代功能验收。本轮Go和Rust parser完整check均已通过（含随后独立contract-test）；dex-router全检中，统一流水线尚未结束。

CB5本轮独立验收复核发现并补修：串行成功记录未绑定当前freeze/完整inputs与精确command；空SPEC/已审文件/同reviewer可能误过；audit失败或中断可能保留旧成功；失败报告计数可能显示verified。验收守卫反例已实际先红（含18个误接受失败），正式新增可复用final/acceptance入口、逐阶段日志及输出摘要/时间关联、完整独立审查清单及启动即撤销旧成功；9项验收/执行器测试普通和优化均通过。进度读取修复及其反例另行推进，最后须新输入freeze/双审/全门禁重新闭合；旧freeze与24helpers门槛不沿用，当前新增后breaker helpers为33项。当前核心canonical全检继续保留实际结果，随后旧串行输入检查应拒绝本次工具改动，不把旧freeze补写为新freeze通过。

CB5补修定向验证：新正式验收/执行器9项与进度17项普通/优化均通过；辅助实际枚举确认为33 breaker+17 progress。进度对失败/运行中/取消/未知state及非布尔passed反例不计verified，保留历史成功报告与指纹兼容。新SPEC复审无新增P1/P2，独立quality整合复核中；本轮最终正式freeze尚未重定，CB1–CB5仍IN_PROGRESS。旧临时run/audit已保全并改为调用正式工具，避免继续使用旧弱守卫。

本轮首canonical结果已保全至`managed-v2-breaker-pre-closure-check`：Go/Rust parser/dex-router完整check、五构建、247契约/8fixture及服务边界检查通过；公共生成检查因PATH误选protoc-gen-go1.36.6失败。独立临时再生证明提交生成物与1.30.0逐字节一致、与1.36.6不一致；正式final入口恢复原生工具选择，公共Make/README版本说明同步1.30.0，独立common check随后通过。未把此轮记为全量通过，正式新freeze仍需重新全检。

CB5 quality新增P2：删除CB5及六breaker设计行仍能通过17项通用progress测试。正补当前计划/六设计ID/精确工作包的结构校验与规范化摘要，并绑定freeze、serial及最终audit；状态/备注正常更新不参与摘要。此项修复与复审完成前不关闭本批。

CB5结构绑定补修完成：精确5任务/6设计ID/逐项WP与paths/tests规范化摘要现绑定freeze、serial、逐阶段及audit前后，删除/重复/错配与摘要缺失/变化均失败封闭，状态/备注/证据链接更新不影响结构。门禁测试20项、progress17项普通/优化通过，实际helper枚举44项；新增对应测试绑定已登记，旧32必审文件加生成器说明/技术参考参与最终复审。候选freeze已准备，独立SPEC/quality结束前仍不运行正式成功闭合。

最终候选核对：source SHA `4f757e1b0bb4d5dafb4c7832cb111b2bed1f96eb6706dd4d2b451afe2945c0f6`、五二进制、13,045输入及结构SHA `3db3fc23bb55fdd9410fc84bf7278ae4dd204343f27d23ac25b8704638cbc3d4`均再次一致；SPEC对最终11个工具/生成说明/技术参考文件摘要和20+17双模式回归已复验通过。新独立quality复核仍在进行，最终freeze/整批成功状态尚未升级。

最终正式freeze启动（2026-11-06）：最终SPEC与独立QUALITY均无新增P1/P2，35已审文件/6SPEC范围/13,045输入和结构摘要已登记；正式`make -C integration managed-v2-breaker-final-check`已启动，依次执行完整verification、fresh原reference/wire、44 breaker+17 progress普通/优化、真实worker、四mode普通服务/actualplan原VM与最终audit。首次canonical生成器失败历史保留；本轮未完成前CB1–CB5继续IN_PROGRESS，不把代码审查与轻量回归当整批通过。

最终正式freeze阶段1通过：source4f757e…同一冻结下三服务/common完整check、五二进制、247契约/8fixture及服务权限边界全部exit0，verification passed且source unchanged。公共生成器原生1.30.0已在整轮common check逐字节复验通过；首轮误选1.36.6失败继续保全。现进入fresh原reference/wire；44+17双模式、worker、四mode与最终audit尚待通过，不升级CB1–CB5。

最终正式freeze阶段2通过：fresh原VM参考300 bank/244 case重新生成98,268,052字节，与原参考逐字节一致（SHA82e5417a…）；独立wire20,067字节（SHAe164e85d…）逐字节一致，阶段exit0。进入44 breaker+17 progress普通/优化，worker/四mode及最终audit尚待通过。

最终正式freeze阶段3通过：44 breaker helpers普通/优化各完整通过，17 progress普通/优化各完整通过，四个OK且阶段exit0；三阶段source/binary/inputs与结构仍匹配。现进入真实隔离worker，随后四mode统一服务/actualplan和最终audit，CB1–CB5仍不提前升级。

最终正式freeze阶段4通过：真实隔离worker报告PASSED，7次REVM仿真/真实收款与2次来源故障恢复检查、同源输入/二进制绑定通过，阶段exit0。四mode统一source/PG/Kafka双router及实际选中计划原VM完整Pool后状态与最终audit仍待完成。

最终正式freeze阶段5进程进展：cached场景28来源bank/50成功API/12预期业务拒绝/6来源故障恢复与router重启通过；dynamic已启动。此为四mode中的单场景进程结果，完整actualplan原VM/双方完整Pool与最终audit仍待全部场景后闭合。

最终正式freeze四mode进程部分全部通过：cached28/50/12拒绝、dynamic163/302/0拒绝、boundaries97/132/18拒绝、shared12/32/0拒绝；合计300来源bank/516成功API/30预期拒绝/24来源故障恢复及4router重启。真实C++来源wire逐bank核对保留。现正在独立重放实际API选中计划，完整原VM/双方Pool后状态和最终audit未完；上述进程结果不单独升级CB1–CB5。

最终限定验收完成（2026-11-06）：正式五阶段与最终audit均exit0，结果为`LOCAL_SCOPE_VERIFIED`。同source4f757e…、五二进制、13,045输入/结构摘要及35已审文件保持一致：300来源bank/600Pool、516成功API/30预期拒绝、24来源故障恢复/4重启；230实际API计划233原交易（包含实际两路四步容量拆单），每步双方完整Pool与原VM差分通过；7真实worker仿真、三服务/common全检、247契约/8fixture、fresh原参考/wire、44 breaker和17 progress普通/优化全部通过。CB1–CB5与六条设计功能及批次36一一更新为限定验收完成；协议整体解析/数学仍PARTIAL_LOCAL、执行NOT_IMPLEMENTED，原26完整WP验收仍0，生产/公开VERIFIED与build保持关闭。剩余R01–R21及rate/mixed/其他会计/ExactOut等要求继续保留，见[限定最终验收](integration/artifacts/local/managed-v2-breaker-local-acceptance.json)。

文档回填后最终复验：17项progress普通/优化与20项门禁回归通过，结构摘要仍与freeze及acceptance一致；精确5任务/6设计映射、全部绑定路径/实际测试定义、26工作包/56协议条目、21剩余事项覆盖全部工作包均校验通过。最终进度一致性报告（`integration/artifacts/local/managed-v2-breaker-progress-consistency.json`，已清理） passed，正式acceptance重新audit仍LOCAL_SCOPE_VERIFIED，git diff --check通过。回填中附加单项报告链接曾被既定evidence规则拦截，已仅修正文档为保留单项原绑定、批次统一引用最终报告，未改冻结工具或放宽校验。

实现约束：slot12依次为lower16、upper16、reference weight64、stored BPT price96、cached lower32、cached upper32；百分比16位字段以左移48解压。初次set时cached ratio来自未压缩的输入bound，cache miss来自压缩后还原bound，因此不能用重新计算的ratio硬校验合法原cache值；同weight用原缓存，不同weight按原方向重新pow，0边界独立关闭。管理输入BPT price=actualSupply×weight/raw balance，原_set先downscale，getter再upscale，6/8 decimals不可按18位币照搬。BPT join/exit改变供应，对未兑换token也须检查双边；所有成功/回退仍保留唯一物理Pool和source H。剩余rate/mixed/其他custody/self-funding/ExactOut/真实链/公开执行与生产完整要求不因本批删除。

<a id="plan-2026-11-06-r01-r04"></a>

## R01–R04：真实来源、持续采集与范围恢复（进行中）

范围严格对应R01–R04及原§7.1–7.3/7.9、§18、§28.5–7/10/20/22/26。既有第36批breaker的限定结论保留其冻结证据；本批任何新代码须重新验证，不能沿用旧指纹。除进度状态外，任务/设计/实现/测试/证据逐项登记在本文件。真实部署审核、共识/来源独立性及Yellowstone来源必须有真实证据；HTTP连通、配置hash、模型单测均不等于完成。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|R01.1|冻结逐chain/protocol/version部署目录，源码/发布二进制/代理实现或ProgramData升级身份与固定历史H逐项匹配|WP02,WP03,WP10,WP11,WP18,WP20|IN_PROGRESS|8链16部署目录保留；13项EVM源码/metadata与3个实际Solana ProgramData均已有精确重建，55/83/404项程序输入与原归档一致；可信历史状态和生产准入资格仍须逐项核验|
|R01.2|固定真实历史块/账户原始证据，独立状态/执行/收款差分及离线可重复回放；来源或版本缺失拒绝|WP02,WP03,WP10,WP11,WP18,WP20|IN_PROGRESS|原Ethereum证据保留；新Polygon H96900773与Arbitrum H522108799完整MPT/原VM→normal Parser/Router收款差分均一致；全目录可信历史bank/费用和生产执行资格尚未闭合|
|R02.1|Provider身份/能力/历史范围/错误分类/限流预算、独立故障域及双源head；WS通知与轮询/重连短缺口一致|WP04,WP05,WP07|IN_PROGRESS|PublicNode 与 dRPC 的主网身份和断线前后 WS/HTTP 区块头一致，HTTP 补齐三块。生产独立故障域与持续限流、历史矩阵待验收|
|R02.2|可信头/交易根/receipt根与链规则逐项核对，限定并行Fetch/Decode staging按链有序原子提交|WP04,WP05,WP07|IN_PROGRESS|native V2 normal Adapter/Factory/history/settled-head及Coreth逻辑/物理storage消费已接，174项EVM race全过；承诺核验不替代可信checkpoint/共识证明|
|R02.3|原始数据不可变归档、解码异常持久DLQ/隔离和重放；工厂事件发现的当前状态准入|WP04,WP05,WP07|IN_PROGRESS|normal工厂目录、immutable archive/DLQ修复重放及owner责任已接；普通进程13项/845 raw/6 DLQ通过；证据流式清单和450真实请求计数另有37/43项边界回归，生产保留/吞吐仍有限|
|R03.1|Yellowstone真实订阅传输、账户/交易/meta/slot来源身份和序号，先耐久WAL后解码/消费；重连与容量失败封闭|WP04,WP05,WP07,WP20,WP24|IN_PROGRESS|受控 tonic→WAL→manifest→owner 以及真实本机 PostgreSQL/Kafka 发布、撤销、重启、Fence 已由 `r03_authority_process.py` 通过；真实同源 Yellowstone 流资格和生产故障域继续核验|
|R03.2|乱序账户/交易/metadata重建、版本化账户、slot/fork/skipped屏障、短缺口补洞；窗口不足HEAD_FIRST并登记history gap|WP04,WP05,WP07,WP20,WP24|IN_PROGRESS|版本化 manifest、非零 tail、fork/skipped/session 屏障与 HEAD_FIRST 已接 normal；真实 PG/Kafka 四类恢复场景及定向回归通过；真实历史 bank 账户完整性未自动成立|
|R04.1|认证EnsurePoolScope控制接口，显式预算/去重/版本隔离、依赖分片和失败撤销；完成当前scope才能ready|WP08,WP09,WP20,WP24|IN_PROGRESS|Go32池首scope→33池扩容、Rust subset/认证/actor bridge已接并回归；旧fence/取消/预算及失败不发布部分ready均有测试；全准入目录与真实源资格联验仍待完成|
|R04.2|当前可信baseline/peer选择、先缓冲流/固定锚点/有界尾窗对齐/generation原子发布，空盘及长停机恢复|WP08,WP09,WP20,WP24|IN_PROGRESS|Go/Rust同owner KV恢复账本及prepared/journal/原子READY已接；本机PG/Kafka共8个等效间隔/空KV场景通过；真实peer、retention及跨故障域资格仍有限|
|R04.3|历史补洞独立namespace/权限/队列/配额，不参与ready、不覆盖online；记录首合格scope/报价及恢复容量|WP08,WP09,WP20,WP24|IN_PROGRESS|Go/Rust独立history namespace/dataset/revision及≤15%墙钟调度已接；Rust queue/runtime18项和normal关闭/暂停/不改online通过，真实历史能力及长期容量/留存持续验收未完成|
|R01-R04.A|逐任务独立SPEC→quality、真实与受控证据明确归属、全检/进程/恢复与完备清单audit|WP02,WP03,WP04,WP05,WP07,WP08,WP09,WP10,WP11,WP18,WP20,WP24|IN_PROGRESS|当前core5f99三服务完整check/proto/247契约边界、23普通进程检查及8恢复场景通过；R01工具218项普通/优化通过、3SBF精确匹配；逐任务真实来源/生产资格仍未全闭合|

实施顺序：先补可信证据与可独立验证的采集/恢复基础；EVM采集、Solana流屏障和scope恢复模块按不重叠文件并行实现，主线程负责真实来源与集成。每项先失败测试、最小实现、定向回归，再正常服务和故障/重启、原始来源与源码冻结。外部来源缺少时继续可完成代码，明确记录缺口而不降低门槛。

环境首查（2026-11-06）：Go/Rust prod示例均为loopback，未含Yellowstone配置。只读公共探针中Solana mainnet genesis返回预期值，两个EVM公共探针HTTP失败；只证明可达性，不证明信任/独立故障域。原始结果见环境探针（`integration/artifacts/local/r01-r04-environment-probe.json`，已清理）。

2026-11-07最终本地验证归档：source SHA `2b61bad02eee97e5c414686e1b3e813d1c53fc1abc53fb34dce38efc3823463f`下Go与Rust解析器最终完整check均exit0，基础Go5包race/Rust115项0skip、Python2+4+17普通/优化通过；公共proto、247契约/8fixture、服务边界和单一真实部署源码重建复验通过。normal进程23检查passed，原始845 raw/6 DLQ留证；运行前后core source一致。全部R01–R04共11任务/11设计映射仍IN_PROGRESS，报告local_checks_passed=true但passed/all_requested_features_complete/production_ready均false，工程与真实输入缺口按上表保留，不宣称四项全部完成。

2026-11-07收口检查：normal进程EVM13项、Solana RPC/WAL10项均passed，原始845 raw/6 DLQ留证；最终基础回归Go5包race、Rust9组115项0skip、Python重建2/逐项4/进度17普通及优化均通过。Go最终完整check exit0；Rust最终完整check亦exit0。R03恢复目标/更高finalized未commit不得ready的两反例已先红后绿并独立复验关闭；旧tip仍可解释保留，processed未来帧不伪改旧账户，Replay不升级资格。历史breaker测试fixture已固定选择自己的批次计划（新批次出现先红3项、修后20项普通/优化通过），未放宽其生产audit。

### 2026-11-07 公有数据边界下的R01–R04持续实施

继续执行全部11项，不改变原方案功能或验收门槛。本轮分离写集推进：R03 Rust正常Yellowstone/流WAL/owner与恢复接线；R02 Go正常provider/当前动态目录；R02.3耐久DLQ重放与有证据归档保留；主线程R04同owner Pebble状态机、prepared/journal恢复与scope接口/历史调度，并负责R01公开部署证据及最终集成。每项先失败测试，再实现，再按SPEC及质量独立审查；正常实例必须消费配置。所有输入维持公开链上资料，认证仅可选。原11任务在完整验收前继续IN_PROGRESS。

审查修复记录（持续更新）：bootstrap独立owner authority、最后I/O后的freshness及失败零结果已SPEC复核关闭；reducer receipt冲突/编码字节与条数容量预留已质量复核关闭，history关闭后访问panic亦先红后绿修复。DLQ别名unknown、incident落盘前崩溃/正预留容量屏障已SPEC复核关闭；history canonical revision、显式dataset scope fingerprint及成功提交后raw pin泄漏三项均经原审查者复现关闭。normal生命周期已接多数组件，未分组baseline/probe/reorg输入的职责交接及完整真实进程验收继续中，不能把模块审查关闭当作所有功能完成。

阶段性实测：Go config/ingest/reducer/service综合定向race一轮通过（2.655/18.971/13.898/41.671秒），随后增加的责任分组/observer仍在接线，当前源码待冻结后重跑全检。Go first-qualified scope/目标池数/恢复计数指标已新增；Rust实际Quote API首合格scope和首成功报价指标经HTTP请求测试通过。已补1小时/1天/30天旧持久水位和容量场景测试，等待所有共享文件稳定后统一复验，不以受控时钟等同真实等待30天。链receipt新增9个真实公开块/823条receipt与独立RLP/MPT oracle匹配、BSC79+OP4实际raw tx body proof；Bor synthetic与当前Avalanche SAE/Nitro/Coreth专属头承诺仍继续实现，严格拒绝边界未放宽。

Rust独立SPEC新增4项：末次PG权威I/O后的流撤权窗口（静态确认待真实PG复现）；terminal jobs永久保护旧WAL导致第二轮封存停滞（正常gRPC/RocksDB已复现）；全catalog依赖挡住subset-first（已复现）；native未freeze废弃bank缓冲不退役（已复现）。分别对应R03.1/R03.2/R04.1/R04.2，原实现者正在按保全复现先红后绿修复；Agave实际补丁同步真实bank生命周期，不以增大容量替代退役。

新增SPEC开放项：provider/catalog审查发现逐endpoint身份探针超时后误标Supported、重组时遗漏仍canonical祖先创建事件、静态scope最终gate未重验providerfreshness，以及旧fence未commit观察/队列保存失败pin责任两项容量边界，均已转原实现者修复。官方V3模板本身未发现额外放宽。Rust owner/RocksDB+native frozen Bank exporter代码已交付，172项定向通过的声明待主线程与独立SPEC复核；完整修改版Agave尚未build/run，已开始用官方v4.3.0源码和1.97.1工具链做实际编译验证，不用补丁适用性冒充完整validator测试。

R02.1/R02.3审查修复交付：provider/catalog 5项原overlay race（40.137秒）及扩大config/ingest/service回归通过，raw维护3项同输入修复不变量race（27.499秒）和后台定向race（85.520秒）通过；两位原审查者分别复核中。另有raw observation归档进度用例间歇未归档已单列追查，不能混入全绿声明。主线程R02.2新增强类型native_proof及7条已知EVM链V2 profile配置校验，先失败后定向race通过（2.405秒），公开目标配置生成器18项普通/优化通过；原有显式profile不自动覆盖。R01公开目录工具63项普通（43.822秒）和优化（46.572秒）通过，仅证明工具回归，不等同16部署均合格。

真实设施开发验收记录：Go normal+两Router+本地隔离PG/Kafka的4种恢复行为全部执行通过（1h/24h/720h等效高度差及720h空owner KV），首合格scope为3652/3757/4116/3977毫秒、首成功报价3661/3770/4125/3988毫秒，期间历史receipt补扫0次、history停用；这些不是实际等待的停机时间。Rust正常tonic/真实RocksDB/PG/Kafka定向发布→read_committed撤销→重启新generation/writer epoch→实际lease失效拒绝测试exit0，1项0ignored，4.49秒。两份总报告因运行中core source变化均保留FAILED，开发记录单独保全，最终须同一freeze重跑。raw ticket原测试10轮5败已定位为22条队列/4条每Tick却只跑4轮，修为沿原预算有界drain后原库不加overlay的5次race通过（25.147秒）；原三项raw生产问题独立quality已关闭。provider/catalog原5项亦经原SPEC/quality复核关闭，后续native新调用链单独审查。

R02.2/R04.3 normal接线补修：新增通过正常NewChainHistoryRuntime构造并读取留存Avalanche真实SAE块的反例，先失败HISTORY_NATIVE_EVIDENCE_SINK_REQUIRED，随后在实际source构造安装native证据责任sink。新正常构造与原proof归档重放两项race通过（5.675秒），native codec/normal定向race亦通过（62.887秒）；不将helper手动安装hook的旧正例当正常服务接线证明。

R02.3 normal工厂proof接线补修：真实SAE证明序列化入archive后，通过Service.verifyRuntimeFactoryProofObserved先复现FACTORY_NATIVE_RESPONSIBILITY_CONTEXT_REQUIRED；三个normal proof投影调用已改用外层ctx，root校验使用原生完整proof，最终target比对后同步核对settling successor。新增目标头不变而后继头改变的拒绝反例与runtime/provider/catalog定向race通过（52.613秒）。Rust原4项修复已交付（22个不同测试通过），原SPEC正在复核；主线程已开始正式Make/Cargo入口的Go与Rust完整check，不以精确clippy-driver替代最终workspace Cargo检查。

原生承诺独立SPEC新增开放项：Bor副来源观察到命中业务地址的未承诺system日志时，正常dual-source路径未拒绝（P1）；正常pool state RPC/保留状态未计入nativeSession总预算（P2）；own-header归档验证返回ProofBytesUsed=0造成重放总量漏扣（P2）。3个独立反例均已保全并交原实现者先红后绿修复。先前两处normal history/Factory接线已由该审查者确认关闭；不把定向绿测覆盖不到的边界视为完整。Go当前Make全检已在EVM分片前明确停止并保全INTERRUPTED记录，修复后重跑；Rust parser与Router规范Make全检继续。

R03/R04审查闭合与真实参考：原Rust四问题已由原SPEC独立26/26复核关闭，新增独立quality继续；main实际PG/Kafka案例扩为1h/24h/720h等效slot差和720h空owner DB，尚待执行结果，不改变live240秒上限。R01三份实际ProgramData/SBF+原Token CPI净收款（CPMM/CLMM/Orca）均经main重新执行原VM离线验证，通过报告分别保全；它们为公开账户快照上的受控新Bank执行，非原历史Agave Bank。继续把正常Parser→Router实际报价与这些原VM输出做差分，并核对完整源码重建，不凭自洽报告提前通过。

R04.2实际Rust恢复行为新增完成：正常Parser/tonic/RocksDB/PG/Kafka单个外部权威场景exit0，1项0ignored共8.91秒；新增1h/24h/720h等效slot gap及720h空owner KV，首合格scope分别1126/1152/1263/917毫秒，每次只发送两帧、from_slot=None，generation升至3/4/5/6且writer epoch递增。仍明确不代表实际等待的停机。main增加164份原始公开source fixture的Go本地testdata字节副本，原跨项目guard保留，247契约/8基础fixture+164source副本及三服务边界通过；copy避免Go测试直接依赖integration。运行中这些源文件变动使R03总报告继续FAILED（行为exit0与冻结失败分别记录），最终须同一freeze重跑。

独立Rust QUALITY新增2P2：owner scope ledger条数未满但字节先满时旧终态publication不回收，连续generation会持续Budget；native崩溃遗留过期request计入目录枚举且不删除，使GC不可达。两项由匹配库的独立overlay复现，原owner3/native4正控通过，原实现者修复中。它们不同于已关闭4项SPEC，不能用SPEC26绿测替代quality闭合。R02原生三项修复已交付：逐witness业务资格、正常state I/O/保留pool共用总预算、own-header实际成本回传；原SPEC复核中，旧预算反例的“要求先成功”setup现被正确拒绝，原文件保全并以同输入验证修后不变量。

R01.2/R03.2真实输入兼容补修：实际三份public RPC快照走正常Parser/Router均在TOKEN_PROGRAM_VERSION_UNSUPPORTED拒绝，已确认Tokenkeg为Upgradeable Loader、代码位于ProgramData，而旧3decoder只接受immutable。main新增显式token_program_upgrade（公开PDA/部署slot）配置，默认缺字段维持旧immutable与原哈希；同bank依赖包含PD，严格验证独立PDA/Program36字节指针与owner/tag/executable、PD owner/tag/slot/authority option以及完整ELF payload hash，绝不以36-byte Program头当代码。先红unknown字段后3decoder正负/未来slot/缺PD/header误hash等回归和config测试均通过；独立SPEC4/4已关闭，fresh quality及正常Parser→Router→原SBF差分继续。两normal二进制已专项build成功，配置从公开TokenPD自动填写，无用户私有输入。

原生证明最后边界：90319字节精确足额现可成功余0，90318拒绝、90320余1，SAE确需后继工作仍必须付费。fresh QUALITY另复现显式1024次重试时请求body仅逻辑扣一次，空503重试可越总预算（P2），原实现者逐attempt扣款修复中。Rust owner ledger字节预留/30代持续推进已由quality6/6复核关闭；native已过期request冷启动会在configured_keys空gate之前卡住清理，新增反例已先红后绿修复，原quality复验待完成，不以已有parent/scope正例代替冷启动证明。

本轮真实native及规范检查：Go完整Make check已在同一Go源码/fixtures摘要3252979e…37dc下exit0，161个EVM顶层case完整14分片、其它包race/fmt/vet/contract/build均通过；输出二进制SHA beda5ab6…b65。该结论只对应此精确Go版本，后续原生MPT公开absence哨兵兼容如修改则另跑。Rust旧完整check暴露首次writer具体错误被seal分支竞态覆盖（TEST_WRITE_FAILED→STREAM_WAL_WRITER_SEALED），已确定性交错先红后绿修复，50次原反例、内部16/unit28/真实WAL5均通过，未调超时，新规范全检继续。

真实Agave v4.3.0三个package build/check与实际Bank integration已同kernel SHA8a2d7543…230d68、Drop patch85b1eefa…241c通过。实际21条原生scoped callbacks、首条write_version0、两份manifest、16个未freeze候选退役、分叉/父继承隔离和重复freeze均有trace/独立字节重验。真实SDK从0开始计数此前触发错误poison，main已移除“0数值=缺metadata”假设；原source/bank/必填JSON/tuple去重/计数/最终fingerprint和已知parent严格递增保持，native13/13、normal原生→grpc、缺字段/重复0/parent staleness及独立SPEC18/18通过，不制造未观察父账户事件。

正常DEX真实输入差分推进：CPMM正常报价和选中金额/minOut原SBF收款delta均337524356，Orca在Token Badge已知bit0兼容后均90094；费用后net字段仍null且理由保留，不作为R01 Token到账差分。CLMM已知unused padding5/6非零兼容和Orca bit0经独立SPEC41/41验证；活动limit orders仍属R05未实现而拒绝。另仅有历史order_phase、无订单数量/ratio的tick由primary源码证明不参与swap，正/活动负例25/25已过；将用显式最近三array新scope验收，旧广窗活动订单失败保留且不宣称完整limit-order能力。所有原账户/Clock均不改写。

接续验收（2026-11-07）：主线程独立fresh normal三Solana case均通过：CPMM337524356、CLMM90091、Orca90094，Router gross与原SBF pool gross及选中amount/minOut接收方Token delta逐整数相等。CLMM显式nearest-three数组为-24180/-24120/-24060；原7-array广窗因活动limit orders仍拒，较大金额404只记录拒绝而不臆断内部原因。生产scopeHash只表示pool ID集合，不表示不同tick窗口；声明由配置、实际依赖、declaration_sha256和原账户摘要另行绑定，不回填不同scopeHash。after-chain-fees net和production build资格仍不冒充已完成。

新增11条EVM reference的v3同H原源码/runtime/代理implementation/MPT和原池实际收款已重放通过，6链90账户241槽；新AVAX99661976→99661982证明13账户32槽/73公开prestate读取。primary Coreth合同storage分区规则为physical[0]=logical[0]&0xfe，之后仍普通keccak路径；旧AVAX04/05/06坏逻辑proof/value记录不改写。Go已证明absence的零hash哨兵/空root零slot兼容经fresh quality关闭，新正常contract logical→physical consumer正在补入；资产分区和未知X-Chain proof不因此支持。余6条已实现EVM normal差分继续；LFJ LB、Velodrome literal及Pancake非零LM等家族边界另列，不把reference通过当R09/R05报价数学已实现。

接续实施记录：设计对应的11项任务状态与说明已同步到当前代码事实；尚未将真实公开来源差距改写为用户私有配置依赖。主线程增加显式启用的Rust正常Yellowstone/真实PostgreSQL与Kafka发布、撤销、重启和authority fencing验收，以及隔离基础设施runner；默认单元测试不会冒称该外部权威路径已验证。当前源码仍变化，编译/进程报告先用于定位集成问题，最终须在同一源码冻结后重跑。

### 2026-11-07 再核对与本轮执行边界

用户已明确按原方案补全R01–R04；目标链/HEAD_FIRST/隔离/失败封闭无需再次确认。本轮先补可直接验证的工程缺口：R02.2正常EVM读取增加显式Ethereum头与交易体承诺校验（header hash、交易hash/chain ID/transactionsRoot、位置、总预算），并以已留存真实Ethereum块离线回放及篡改反例验证；R01.1/R02.1补方案8链的机器可读目标与配置生成入口，协议部署准入与目标清单分开，禁止目标存在即自动准入。R01–R04逐项更新为代码缺口/正常接线缺口/真实来源验收缺口；工程工作不以用户未提供供应商为前置条件。方案原文保持不变，已有11任务状态仍为IN_PROGRESS。

执行验证：新增功能先运行失败用例，再实现和定向回归；正常服务测试、配置校验和进度映射通过后登记具体范围。本轮新源码需要新验证记录，旧source SHA及其完整check仅为历史证据。

### 2026-11-07 当前源码继续开发与实测（本轮持续更新）

本轮修复Rust历史服务测试跨await持锁、R01单独发现测试的模块导入，以及大receipt成功路径在race下误触1秒测试期限；生产请求期限未放宽。Rust history queue/runtime的18项实际队列、tonic/HTTP及原owner状态测试通过，覆盖十亿slot范围有界分页、暂停/禁用/关闭、generation隔离及不修改online引用；已经纳入R01–R04基础回归和设计逐项绑定。

当前core source `5f99ed92d5a4976483a9bc105135e2a60a7e9929bf8985d9a699659d7e797c36`下，Go完整check通过，含全包race和全部174个EVM用例的15个完整分片；普通进程EVM13项、Solana RPC/WAL10项通过，留存845 raw/6 DLQ。真实本机PG/Kafka的Go恢复4场景和Rust正常流owner发布/撤销/重启/Fence及4恢复场景也通过；间隔为1小时/1天/30天等效高度或slot差及空owner KV，明确不宣称真实停机30天。Rust与Router完整check亦均exit0；Rust累计848次测试执行0 ignored，Router累计985次执行、5个原有其他协议专属进程门禁未在unit中运行（统计含contract目标重复）。R01–R04基础回归exit0，167项Rust定向测试0 ignored；新增getter闭包17项普通/优化也通过，未以这些结果替代真实来源资格验收。

R01源码重建取得新结果：任务隔离Linux虚拟机、官方Linux v1.52工具链与实际SBPF target下，CPMM源码`3d69122790e97cd6aff24e8ba95940793773f58c`重建的749848字节与部署payload逐字节一致，余43976字节全零；CLMM源码`6d7a0f82d2f293c553c69aaf943377367c6c4cc6`重建的1498416字节也逐字节一致，余201744字节全零。原失败候选保留。Orca源码`cc683ec92acb5e7606caa3d8e514d9383e9c7c11`通过Linux v1.44原版rustc1.79/Cargo1.79/SBPF构建，1495896字节与部署payload逐字节一致，余8989819字节全零。仅在隔离vendor视图中为三个未参与编译的time依赖manifest添加edition2024解析opt-in，原程序、锁、Rust代码及ELF未修改。三项共55/83/404个源码/manifest/lock输入均与公开原归档核对无差异，三份精确重建与退出/填充校验（`integration/artifacts/local/r01-three-native-sbf-exact-builds.json`，已清理）已汇总，任务自建Linux VM已正常关机。[来源与构建绑定](integration/test/source/deployment-catalog/native-sbf-v5/cpmm-exact.json)及[CLMM实际字节比较](integration/artifacts/local/r01-solana-rebuild/clmm-native-vz-sbpf-comparison.json)各自限定所证明的版本，不自动赋予历史bank/生产资格。

R01 getter依赖采集新增负tick取整、最多1027个storage key、同chain/genesis/H/hash/runtime/stateRoot和原VM/MPT闭包门禁；完整留存证明先实际重跑原VM闭包后可0网络复验，不强求重新访问旧H，17项普通/优化测试已通过并接基础入口。Polygon新H96900773补齐531个key并通过140个原VM getter；正常Parser→Router的gross与原程序实际Token收款增量均为9原始单位，有限差分报告（`integration/artifacts/local/r01-public-polygon-refresh-04-normal-retry1/bound-report.json`，已清理）保留原头时间，未提升为费用后净输出或生产执行。旧H缺失和首次有界观察器到期均原样保留。Arbitrum新H522090937的18账户/1055 storage proof已核对；发现1129个派生cache记录被误算为450次公网请求的预算计数问题，现已分离真实尝试计数并补43项普通/优化回归；新H522108799的18账户/1058 storage proof与原VM全部1114个状态读均通过，1027个getter key/264个getter可0网络重验，正常收款增量224255逐整数一致，见Arbitrum有限差分（`integration/artifacts/local/r01-public-arbitrum-refresh-07-normal/bound-report.json`，已清理）。旧H失败、1024 key限制和历史窗口拒绝均保留。

R01清单核验改用64KiB分块摘要，显式64MiB单原件/512MiB总量预算；37项普通/优化测试通过，原32MiB JSON解释上限不变，旧41MB verbose trace仍只作为不透明留存。原设计文件、旧冻结来源和所有11个任务范围保留；当前仍不能宣称R01–R04全部完成。当前最终核对报告（`integration/artifacts/local/r01-r04-current-final-audit.json`，已清理）记录全部实际命令退出/日志摘要、同core源码、16项部署目录与源码重放、两个新固定H数量差分及剩余资格边界：`local_checks_passed=true`，`all_requested_features_complete=false`、`production_ready=false`。本轮13 EVM源码重放与3 SBF实际重建均通过；原2258份冻结EVM输入和原设计文件保持不变。

<a id="r01-r04-gap-audit-2026-11-07"></a>

### R01–R04逐项缺口复核与责任边界（2026-11-07）

**公有数据边界澄清（2026-11-07）：** 本项目核心是公链DEX区块/交易/日志/账户状态解析与本地最优兑换计算，必需输入为公开链上数据、公开部署身份与可核实的协议规则。用户钱包私钥、助记词、私有订单流、私有业务数据库、商业供应商账号/API Key均不是这些功能开发或验收的必填输入。默认使用公开来源；性能/历史覆盖需要时可自建开源节点/归档/Geyser。受认证的托管RPC或流服务只是可选接入方式，不改变数据公开性质，也不是强制依赖。系统自身PG/Kafka/存储及内部认证属于项目运行设施，应由开发配置和本地验证，不能包装成需要用户提供的私有链上资料。公开来源不足必须准确记录吞吐/历史/一致性能力缺口，不得要求用户用私有信息填补尚未实现的工程。

核对基于方案原文、normal服务构造/调用链、真实持久后端和当前测试结果。Go已接入provider/FactoryDiscovery/ScopeController/同Pebble owner事务及history生命周期；Rust已接入Yellowstone/FrameWal/manifest/同RocksDB owner事务和async发布桥。下表随开发同步，区分代码已实现但待审查/集成与真实公开来源资格差距；不以用户提供供应商或重选目标链作为前置条件。

|任务|当前可验证边界|仍须自行补全的具体功能与配置|真正影响真实环境验收的核对点|
|---|---|---|---|
|R01.1|8链16部署/14样例池目录；13 EVM精确源码/metadata；3 SBF均Linux原生精确重建并绑定原ProgramData升级身份|构建/升级身份对照的新缺口已补齐；仍须与可信历史状态、动态版本升级准入及全目录资格共同闭合，不能把source观察直接提升为生产准入|三个真实ProgramData、升级slot与公开源码/编译参数已由开发自行核对；后续新升级仍需重新取证，不要求钱包私钥或商业账号|
|R01.2|Ethereum原固定块保留；新Polygon/Arbitrum整套原始MPT与原VM全读集证明，normal收款增量分别9/224255逐整数一致|全目录可信历史bank/原链及完整后状态/费用资格仍须补证；当前实际Slipstream模块、Pancake LM等超出已准入profile的样本继续明确拒绝，不删除失败项或替代费用后净额|公开归档/自建开源节点可补历史状态；保留Ganache本地H+1等环境边界与失败，不把本地执行称为已挖出的原链交易|
|R02.1|normal身份/能力/错误/共享预算/WS已接，当前Go全check/race及受控重连/故障用例通过|代码及本地接线已过；继续真实多provider历史正负覆盖、持续限流/重连/故障域及可信头来源矩阵验收|公开来源的真实限流/覆盖和上游故障域需观察证明；多个URL不自动代表独立，认证托管可选|
|R02.2|native V2采集/工厂/history/settled-head已接；Coreth原始physical proof保持严格，logical入口及共享预算实际消费通过|完成真实可信checkpoint/最终性来源绑定、实网重组/发布终态与各链特殊日志承诺的完整资格验收；原生根一致不自动等于共识|真实可信头/checkpoint与最终性仍按来源级别记录；专有承诺一致不能自动升级为共识证明|
|R02.3|normal动态目录、immutable archive/DLQ修复重放和责任事务已接；845 raw/6 DLQ进程通过；证据清单/派生cache预算缺陷已补反例修复|继续完整真实工厂版本/当前依赖矩阵、生产归档保留/容量和吞吐故障验收；未知或未归档原件不得通过GC删除|公开工厂版本/代码由开发留证；最终生产保留和吞吐依真实流量测试，未知输入不得为GC删除|
|R03.1|normal tonic→耐久WAL→manifest→同RocksDB owner/PG/Kafka通过发布、撤销、重启及Fence实测；既有真实Agave Bank证据保留|真实公开/自建Geyser→原生bank证明→normal owner的同源资格验收仍缺；标准交易块或普通RPC不能冒充账户完成证明|自建开源Agave/Geyser使用公有数据；标准流无同bank账户完成证明时必须拒绝严格资格，供应商key不是前提|
|R03.2|版本化账户/父继承/fork/skipped/session/tail/HEAD_FIRST正常路径通过当前全Rust及实际PG/Kafka恢复|继续真实版本化账户快照、短洞/大洞、新会话、缺失/死分叉与同源bank证明的联合验收；历史回放不自行复活ready|核实真实bank来源/写版本/账户完成性；当前RPC/普通交易块不能冒充历史账户快照，公开来源限制如实记录|
|R04.1|Go目录32→33池同owner扩容、Rust认证subset/动态目录/async actor bridge已实现并定向/全包回归|继续全部实际准入目录/版本与可信源资格的scope扩容联验；当前缺依赖、版本漂移和失败均保持撤权|采用已定热池/近期请求默认优先级，无须重问目标链或私有业务信息；缺依赖保持拒绝|
|R04.2|两端同owner ledger/prepared/journal/KV/READY已接；1h/1d/30d等效差及空owner KV各4场景，本机PG/Kafka共8场景通过|真实可信peer/来源选择、retention丢失及跨故障域恢复验收仍有限；当前8个等效间隔/空KV场景不称真实停机30天|本地隔离PG/Kafka由开发自行配置；跨系统恢复以日志为权威，未声称分布式原子事务；等效间隔不称真实停机30天|
|R04.3|双语言独立history持久队列/权限/dataset/revision/≤15%调度与首scope/真实报价指标已接；Rust18项及正常暂停/关闭不阻塞通过|继续真实历史能力、长期回补/保留与持续容量/调度测量；history完全停用时仍先恢复scope/报价这一正常路径已通过|保留/吞吐用公开数据和本地故障压测；15%为实际墙钟调度份额，不冒称OS硬配额|
|R01-R04.A|core5f99三服务完整check/proto/契约/边界、R01 218项普通/优化、3精确SBF、普通23检查和8恢复均通过|当前证据清单/新增源修订及本地实际命令audit已冻结；所有真实来源/生产资格和逐项独立闭合没有完成前维持IN_PROGRESS，不能用本地check或单个样本替代全功能验收|真实公开来源资格逐版本单列；mapping、controlled流程或单个样本通过不能替代所有功能验收|

方案已确定并由开发处理的内容：8链名称/身份和解析owner、各链初期协议目标、HEAD_FIRST默认、history不阻塞ready且无online写权、有限tail、缺证据拒绝、按scope恢复和先验证后准入。当前新增的配置生成器读取`integration/config/chain-targets.json`，自动探测chain ID/完整genesis身份，并生成正常解析器可读取的配置及单独身份观察记录；保留base基础设施设置，状态目录/日志topic按网络隔离，基线年龄不超过120秒、tail不超过128。公开RPC只用作开发探测，未授予任何协议/生产资格；8链完整genesis均已固定并绑定留存来源摘要；部分链只有明确的单源观察，仍保留该证据级别，不提升为共识或独立故障域。配置可从摘要固定的留存身份记录离线生成，不把当前公开RPC可达性作为生成配置的前置条件；离线模式明确没有实时探测。

本轮定向审查发现并修复4项配置问题，均先补反例再修复并经复核关闭：首次生成缺genesis pin可能混用同ID网络；合法大响应预算生成非法raw配额；同链已有RPC被公共默认覆盖；慢速响应可突破单次socket timeout。当前8链均有完整pin和留存来源文件，state/default topic含完整genesis，已有同链topic/RPC保留，32MiB等无法满足raw上限的组合写前拒绝，身份读取由owned worker总期限约束。所有修复仍只覆盖各自功能范围，R01–R04没有因此标记完成。

本轮最终验证：core source SHA `bfb314782b68c7062bdec555597404cd3144a1277dd098143a31692347ec31f8`；Go完整check exit0（全部包race、EVM96项完整8分片、fmt/vet/contract/build）。配置15、对应关系/缺口6、进度17项普通及优化均通过。8链配置从摘要固定的真实genesis记录离线生成，实际Go/Rust正常loader全部exit0；此项不证明运行期健康/协议准入。普通PG/Kafka进程EVM13项、Solana当前RPC/WAL10项通过，留存845 raw/7 DLQ，运行前后core SHA一致；它不包含真实Yellowstone。单Ethereum真实留存H26355865的101交易在线Adapter核对及篡改/位置/预算/取消反例通过，正常服务错误撤销/上一完整提交保留/DLQ通过。Rust源码本轮未改，未把配置加载或上一freeze的Rust全检冒充本轮全检。原设计文件SHA保持不变，所有11任务仍IN_PROGRESS。完整范围与逐项缺口见本轮核对及验证报告（`integration/artifacts/local/r01-r04-followup-audit-report.json`，已清理）。

当前核心功能**没有要求用户提供的私有信息**。开发自行核对的是公开来源的链身份、协议版本、最终性、历史/回放/账户完成能力、限流及故障域；能探测的自动探测，不能证明的如实保留缺口。吞吐/留存由默认预算和压测推进。只有以后用户明确选择其已有的受认证托管端点时，才涉及该可选端点的连接配置；这不是公有数据解析/报价的功能前提。当前动态目录、流发布和恢复事务等缺口仍由开发完成，不能转化为索要用户供应商账号、API Key或私有资料的理由。


所有R01–R04任务继续IN_PROGRESS；开发与验证报告（`integration/artifacts/local/r01-r04-development-report.json`，已清理）保持all_requested_features_complete=false。本地验收报告只陈述其有限范围，不构成四项全部交付。


2026-11-07进程验证进展：真实本机PG/Kafka+normal Go/Rust/双router测试中，EVM13项检查通过，raw显式配额下845条原记录与6条DLQ已保留至独立artifact目录并逐文件SHA留证，包含错误receipt撤销/恢复/重启。Source为受控RPC，header-trust仍非独立共识；Solana正常RPC→WAL进程恢复仍在运行。这些进程检查不当作真实Yellowstone供应方或全版本生产验收，目标仍未关闭。

R03.2新基础：anchor_versioned/anchor_accounts对独立初始manifest、source/session、完整scope与账户指纹逐项核对；已证明无写入才继承父账户Arc并保留原write bank，成功完整scope用于下一父状态。prove_skipped精确绑定父/目标bank和完整中间范围，不消耗Geyser序号、不从null/slot差推断；继承/skip及ready中间态新反例已通过，独立复核关闭；最终Rust基础115项通过。R04版本变更现在必须同步推进单调数值Fence，version-only入口拒绝，原失败下界两种变体与8正控回归已绿，待原SPEC复验。统一foundation已启动；首轮Go全check命令exit0已保全，因其间增补独立raw配置不作最终source验收，最终版本仍要复跑。

检查记录：Rust ingest all-targets Clippy -D warnings已完成一次fresh通过（首次依赖/本地库编译13m25s），最终补修后仍需复跑；Go configWS传输/凭据/资源边界回归、normal WS/取消/存储失败/有序提交定向race及Python2重建绑定+3逐项映射普通/优化通过。所有这些记录仅注明本地范围，真实source/production集成未被替代，R01–R04状态仍IN_PROGRESS。

审查补修状态：R01重建部署绑定和Rust HTTP等待取消导致提交孤立两项已由原SPEC关闭；R02 WS→HTTP密集提示限频反例已先红（20条提示约28ms触发20次读头），补共享poll预算后原SPEC三轮均仅1次并验证取消/heartbeat/多链/兜底未回归。R04零事件baseline与freshness两P2已关闭，版本变化不推进数值Fence的失败下界变体继续补修。R03固定单worker/queue1/inflight1、write/cancel/close与晚ACK23项通过；独立quality补充网络idle晚帧绝对deadline变体，正在补修。统一foundation及真实PG/Kafka恢复/来源实证仍未完成，不升级任务。

外部实测继续：Ethereum原固定块storage补证被公共RPC429拒绝，失败保留；Solana Orca实际program+ProgramData同context finalized账户观察已取得，验证loader owner/executable/升级slot及ProgramData指向/ELF前缀，原payload含分配padding，记录为provider观察而非独立bank或源码重建。初4MiB预算超限已拒绝保留；另一次明确16MiB预算诊断取得约13.98MiB响应，不修改服务原预算/准入。R01全版本及真实独立执行/收款仍未完。早期未自行校验部署proof的rebuild报告已保全并标SUPERSEDED，后续只引用带proof校验的bound-rebuild报告。

逐项进度校验：新11任务/11设计功能/精确WP及源码-测试映射已登记并由3项删除/重复/错配反例验证，检查结果只表示对应关系passed，所有功能完整标志仍false；原26工作包和21剩余事项未删减。真实Ethereum固定块及响应已保留为离线source fixture，编译重建现在先绑定部署proof并拒绝伪造地址/H。统一foundation入口已加入Make，待本轮审查问题关闭后完整运行。

后续进展：Go正常服务已接explicit websocket配置与预算、通知唤醒/HTTPpoll兜底，未从WS提示取事实；采集改为两块有界并行窗口，只有原prepared/journal actor按父链顺序提交，保留lease撤销预算。认证scope/response先归档/实际staging/WS真实loopback定向race通过。独立SPEC已指出并修复Rust提交外层timeout遗留任务和源码重建虚构部署绑定；新反例与实际bound rebuild通过。R03慢盘阻塞async已交固定单writer/有界队列补修；R04新Fence失败撤权、零事件尾窗不能改baseline、新鲜度提交/ready边界三P2补修中。没有将这些代码阶段结果记为R01–R04完成。

新增实证与接线：Uniswap V2该主网runtime已用固定solc0.5.16+commit.9c3226ce.Emscripten.clang及原10源文件/完整compiler settings重新编译，输出与已证明链上code逐字节一致；仅此明确版本，独立交易/收款和共识仍未验证。R03原始gRPC已通过本地真实双向流+认证/idle/消息预算测试，3项通过；流/WAL/projection/旧assembly34项通过，仍要求外部版本化账户bank证明，原fullBlock不会冒充该证明。R04两服务认证EnsurePoolScope真实handler已接配置目录+代际/新鲜度/actor/拒绝与正常reanchor，Go/Rust首测通过；更广动态目录/持久恢复source与在线prepared事务联动仍未闭合。normal Go RPC raw recorder已接服务，持久失败禁止换源绕过，正在回归；WS与工厂候选准入继续补。

开发进展：R01新增Go来源验证/只读capture CLI，实际Ethereum H26355865完整头/101笔交易根及UniswapV2账户MPT/代码哈希通过离线复验，runtime与发布artifact逐字节一致；固定块storage补证遇公共RPC429，失败保留，不把此单版本样本当全部部署/共识/源码重建/真实收款完成。R02 provider能力/分类限流与共享预算、bounded staging有序commit及耐久raw/DLQ基础8文件已实现，rpc/ingest三轮race通过，尚待正常服务接线、WS/工厂与真实来源矩阵。R03固定上游proto commit、TLS/鉴权/bounded原始gRPC捕获传输和独立流WAL/乱序fork屏障开发中；真实订阅来源与版本化账户完成证明仍缺。R04双语言scope恢复状态机已实现基础，耐久store和认证HTTP控制接线开发中，不以trait/fake模型冒称生产恢复。

<a id="remaining-priorities"></a>

## 剩余功能与开发优先级（最新复核：2026-11-18，第116轮收尾）

核对口径：以实际服务入口/协议分派/拒绝分支和原§28验收要求为准，不以文件夹存在、样本数量或配置标签当功能完成。原26工作包完整验收仍为0；已有多个局部解析、数学和执行profile，能力数随独立版本增加而变化，按上方协议表及运行时能力目录逐项核对。旧记录WP18/WP22的状态现分别核对：WP18已有多链身份/receipt/状态证明等限定实现，真实多链端到端上线仍未验收；WP22高级外部来源竞争仍未开发。最新分类总览见[第116轮收尾差距复核](#design-gap-review-2026-11-18)。

“最高/高/中/上线前/低”是剩余事项的安排建议，不改写原§24.1的P0–P8阶段定义。每个协议按固定版本身份→状态/wire→实际解析→精确数学及完整后状态→独立参考→恢复/进程验证逐项完成；数据不足保持失败封闭。

|编号|优先级|设计章节 / 工作包|尚未完成的功能|代码核对与完成门槛|
|---|---|---|---|---|
|R01|最高|§7.1/12.11/12.14；WP02,WP03,WP10,WP11,WP18,WP20|真实部署身份与真实历史块参考；源码、部署二进制、升级slot/合约实现逐版本核对|现有原生/SDK差分多为固定源码+合成RPC；以真实块/账户来源、可重复回放和独立收款差分补证，不把code hash配置当部署审核|
|R02|最高|§7.1–7.3/7.9；WP04,WP05,WP07|EVM持续采集完整性：WS通知/轮询一致、provider能力/限流/双源head、并行staging有序提交、raw归档/持久DLQ、工厂发现|normal服务已接provider方法级能力/负对照/共享预算、WS与HTTP补扫、并行staging有序提交、工厂动态目录、不可变raw/DLQ修复重放及owner责任；本地真实持久后端回归已在R01–R04章节留存。持续公开provider历史/限流/故障域、生产保留/吞吐及可信共识/其他链规则的完整资格仍待补验。|
|R03|最高|§7/18/28.22；WP04,WP05,WP07,WP20,WP24|Solana Yellowstone接入、流WAL连续恢复、补洞、slot/fork屏障、事务/metadata/账户乱序重建|normal tonic订阅、独立帧WAL/manifest、Parser owner发布与async桥已接，本地PG/Kafka发布/撤销/重启/Fence有证据；实际Agave Bank构建和部署重编译分别留存。真实同源Yellowstone流、断线连续性、版本化bank和fork/补洞完整资格仍缺，不把RPC当前bootstrap提升为历史bank证明。|
|R04|最高|§7/18/28.10/28.26；WP08,WP09,WP20,WP24|范围化恢复与扩容：EnsurePoolScope、账户依赖分片、当前基线选择和历史补洞隔离|已接认证EnsurePoolScope、Go32→33池动态扩容、Rust subset与actor bridge、预算/去重/版本隔离和失败撤销；同源finalized context有界分片仍按原预算执行。全目录准入、真实源范围扩容/连续流与版本化账户、长停机恢复及持续历史调度的完整联验仍未完成。|
|R05|高|§11/12.2/12.13/12.14；WP02,WP03,WP10,WP11,WP20,WP25|Solana现有协议扩展：Token2022、转账费/账户扩展，Orca adaptive的Token2022/补充稀疏数组及其他高级组合/奖励领取，Raydium限价订单、奖励领取和高级组合|[orca.rs](chain-parser-rust/crates/solana-ingest/src/orca.rs)已支持独立经典SPL静态费奖励版，旧profile仍拒绝adaptive oracle；独立adaptive版本已补完整Oracle账户/普通分派；新增独立adaptive-rewards版的classic SPL/三fixed数组/初始化奖励增长与跨tick完整状态、正常RPC/双副本服务已完成限定验收；第31批独立dynamic版本已完成三连续完整dynamic/fixed混合数组解析与普通报价/全后状态及双副本限定验收；[raydium_clmm.rs](chain-parser-rust/crates/solana-ingest/src/raydium_clmm.rs)已支持独立静态输入费基础bitmap奖励版，仍拒绝订单及未建模组合；每种版本补独立状态/wire、解析、原数学/完整后状态差分|
|R06|高|§12.2/12.11/12.14；WP02,WP03,WP05,WP11|Curve其他Plain变体/Meta underlying/旧Lending及NG特殊Token/外部rate及Crypto其他版本，Balancer Managed非零BPT费用/Composable待结费用与BPT/rate-cache/V3|[stable.go](chain-parser-go/internal/chain/evm/stable.go)与[weighted.go](chain-parser-go/internal/chain/evm/weighted.go)仅命名限定模型，双币及3–20币Weighted、Managed零swap-share快照、Composable无rate-provider常规交换和Plain3/4已本地验证；非零Managed BPT完整数学/状态契约已补原回调参考，但该旧源码runtime超EIP170，正常服务继续拒绝；实际20230411版本已补独立aum-3状态、固定块解析、非零AUM/钱包及原Pause/Recovery差分，当前T34.11活动schedule单池及M1–M4跨物理池有限闭环已同源通过；BPT有限GIVEN_IN已同freeze验收，breaker独立版本CB1–CB5已通过注明范围的同freeze本地验收；rate/mixed/一般参数/完整执行等其余能力未完成；当前日期2–50目录整合与跨物理池共享Vault全局钱包传播见T34.8/T34.9；Meta直接兑换已补正常服务和只读base依赖；Meta underlying、Crypto其他版本及BPT更广参数/会计/执行仍未完成；Crypto活动ramp独立版本已完成固定块解析、完整schedule/后状态与正常双副本服务限定验收；Crypto标准settled独立版本已补原LP/gulp/调价完整状态及正常解析/报价，本地进程验收通过；标准NG独立profile已完成固定原源码模板/immutable准入、完整状态/EMA及正常RPC与双副本连续报价；[protocol-math](dex-router/crates/protocol-math/src/lib.rs)不支持的数学失败；按实际代码版本逐个补状态、不变量、精度与费用|
|R07|高|§12.11/12.14；WP03,WP05,WP11|Aerodrome/Slipstream非零外部费用模块、LM/reward与忠实费用账本；PancakeV3 LM|独立Slipstream field32/f8717faa-module-1已完成注明范围解析/原数学/正常服务及同源冻结；Pancake LM field33/98684794-lm-1已通过注明范围的主目录验收。未知模块、不同LM代际/迁移和特殊Token仍拒绝；实际BSC generation4已由阶段41独立field34通过注明范围验收；其他升级/未知模块和特殊Token仍拒绝，不能用基础V3或旧LM替代|
|R08|高|§12.2/12.11/12.14；WP02,WP03,WP05,WP11,WP14|V4/Infinity swap hook、自定义会计/覆盖费、native结算、Infinity Bin动态/hook/native组合及完整manager结算|[singleton.go](chain-parser-go/internal/chain/evm/singleton.go)及独立advanced版本已补动态LP费快照/native currency0/无swap回调权限与CL core后状态；Bin已补固定3c5ef1be原manager/tree完整读取、跨bin数学/后状态与静态零hook普通服务限定验收；完整manager/Vault结算未完成；新能力需manager状态、hook依赖和独立执行，unknown hook仍拒绝VERIFIED|
|R09|高|§12.2/12.11/28.22；WP02,WP03,WP05,WP11,WP20|Meteora DLMM、Pump/迁移曲线、LFJ Liquidity Book、Sushi Trident等新模型|LFJ v2.1.0独立field31已有限本地验收，真实Avalanche来源/执行资格仍缺；阶段49–53已补限定DLMM MM/OnlyY、奖励、订单ExactIn/ExactOut、X侧Token2022和扩展bitmap，阶段51–52补Pump classic/SOL独立ExactOut、PumpSwap canonical/boost及原毕业迁移交接，阶段57补PumpSwap卖出目标。余项为策略/Token扩展交叉、更多实际程序版本及Sushi Trident等未接入模型；按独立状态/解析/原程序差分逐项推进|
|R10|高，可并行|§12.13/14/28.16/28.17；WP10,WP14,WP15,WP20|独立执行后端与真实收款余额差：EVM REVM/固定状态DB/链规则；Solana版本化账户及SBF/Token CPI|已有限REVM/实际收款差及正常simulate API；阶段58–84在Meteora/Pump/Raydium/Orca注明范围接入实际unsigned wire、同次账户捕获、原SBF/Token CPI余额差和钱包回执。可信历史bank与生产资格、多链规则/费用、其余协议和拆单执行仍需补齐，VERIFIED保持关闭。|
|R11|高，依赖R10|§14/28.16/28.18/28.22；WP14,WP16,WP20|受审计执行器与可用交易：各协议/拆单DAG calldata、授权/wrap/费用/退款；Solana交易/CU/账户锁/blockhash|已有多个限定Solana profile具备单池执行准备，注明范围包含显式ATA/WSOL补足/退款/解包、原生SOL、SBF与单次钱包发送/重启回执；详见阶段58–84。v2-complete的限定EVM准备与本地仿真已另行补齐；其余执行模板、完整费用/ExactOut组合、多hop/拆单DAG、跨收款人原生结算和生产审计继续待完成；不能把现有Solana钱包链路列为从未实现。|
|R12|中，交易前完成|§12.10/13/28.14/28.15；WP12,WP13,WP16|实际Gas/L1数据费/原生费换算、净输出评分和费用完整性|quote响应netBuyAmountEstimate为null，CHAIN_FEE_MODEL_NOT_IMPLEMENTED；配置route_activation_cost只是提案成本，不是实测链上费用|
|R13|中|§12.10/13/28.14/28.15；WP12,WP13|图增量索引/COW、beam多标签、共享DAG联合求解、小图穷举及收益gap|新增max_splits=1..8、独立V2/V3/Aero物理池子集DP与独立整数穷举；共享池按执行顺序有界多路分配、显式次数/时间预算和网格覆盖标记；默认4路，保留候选/网格外最优性边界。全量/进程验证见[计算收尾计划](#plan-2026-11-08-quote-completion)；通用DAG与严格收益界仍未完成；阶段85已完成物理池增量索引/邻接边COW与本地回归，其余通用DAG/严格收益界和规模化仍缺|
|R14|中|§12.10/12.13/13/28.12–28.16；WP10,WP11,WP13,WP14,WP16|ExactOut完整输入目标/退款语义、转账税/rebase/暂停/黑名单/代理升级能力分级|已有多个限定profile支持独立ExactOut目标，包括已限定的V2/V3/LM/Slipstream/LFJ、Solana、Stable/Weighted/Managed快照/Composable普通币和零hook Singleton/Bin。各版本状态/计划/API/前端范围见独立批次，执行模板与数学分别核对；未准入profile、扩展组合/高级Token行为和完整目标退款/费用仍缺，不对ExactIn结果取倒数。|
|R15|中，规模化前|§9/18/28.10/28.11/28.26；WP08,WP09,WP24|S3/对象存储不可变checkpoint、公共manifest校验、peer选择；大池COW/慢句柄回收和恢复容量|阶段102–116已实现Go/Rust不可变公开protobuf、固定HTTP对象上传/回读、当前/旧对象目录、router私有有限Kafka追平、双owner配额/只读计划及Go手动/自动回收。剩余Rust执行、离线peer/canonical声明、多对象分片/全盘和远端容量、真实独立存储/保留过期、1小时/1天/30天停机、WAL溢出和无源恢复联验|
|R16|上线前必做|§7.5–7.8/18/28.8/28.9/28.19/28.25；WP06,WP07,WP17,WP23|Kafka TLS/SASL/ACL、真实ISR/retention/topic重建与事务故障；PG切换、磁盘/节点/可用区故障、灾备RPO/RTO及shadow放量|现有专用单机PG/Kafka及进程接管测试已做；不等于多区/独立故障域验收，生产准入必须单独验证|
|R17|上线前必做|§4/20/28.3/28.4/28.27/28.28；WP01,WP02,WP25,WP26|CI平台/镜像扫描/SBOM/许可证/secret扫描、schema破坏检测、完整配置兼容矩阵、secret版本和日志采集/审计/OTel|本地make check和结构化日志存在；SDK故障映射、轮转/spool、生产安全与性能故障验收仍缺；必要安全项与最高优先级链路同步建设|
|R18|上线前按链推进|§4/28.18/28.20；WP16,WP18,WP25|第二第三条真实EVM链、独立硬分叉/Gas/费用/owner与缓存隔离验证、版本升级目录|WP18为PARTIAL_LOCAL，已有多链指纹/receipt与状态证明的限定路径；第二第三条真实链端到端上线验收仍缺；每条链单独数据ready、图ready和SLO|
|R19|中，开放API前|§15/28.18/28.19/28.28；WP16,WP17,WP26|多租户额度、内部/任意仿真隔离、取消资源回收、业务成功率SLO/压测和完整可观测性|当前共享认证/限流/single-flight/deadline和基础指标已实现；多租户/费用/执行质量及规模化故障容量未验收|
|R20|低|§16/28.21/28.23；WP19,WP21|持续分析调度、其他协议事实、持久DLQ/raw回放、ClickHouse事实/版本表/shadow发布/FINAL查询、迟到和decoder回补|[RunJournalWorker](chain-parser-go/internal/analysis/worker.go)复用RunJournalJob，已实现正常只读持续调度、取消/失败停止/重启及持久cursor；剩余是分析专属持久DLQ/raw重放、ClickHouse/发布/查询及完整事实族；采集侧raw/DLQ已存在；保持不阻塞报价|
|R21|低|§13/28.24；WP22|RFQ/外部来源竞争、多求解器、适用模型上界和收益分布|WP22仍NOT_STARTED；待本地数学、执行和成本口径稳定后做，不能用收益目标绕过正确性|

R06后续版本核对：已只读取得官方balancer-deployments固定b55dc2b8a46bce8f27aed5a3b03f719f662f860a的20230411-managed-pool-v2完整build-info，按原solc0.7.1/optimizer140及独立库job9999精确重编译；原Factory在正常EIP170限制下创建24561-byte pool，并用24577-byte控制合约验证限制有效。该部署实验仍使用构造facade，未完成原Vault/ERC20初始化与交换闭环，也未取得主网eth_getCode。该实际发布版的常规swap不走旧swap-share BPT铸费路径，需独立版本建模预铸BPT/virtual及actual supply、非零AUM与协议分成；旧e281ff40版本的拒绝不解除，不能用新版本冒充旧费用语义。下一阶段依此补实际发布版本与非零费用能力，仍属R06/WP11未完成项。

本轮按用户要求先补R06中的Curve Crypto，再补R05的Solana高级能力与R08的V4/Infinity扩展，逐个实际版本推进完整状态、解析、精确计算和进程验证。R01真实历史/部署基准、R02数据证据闭环及R03/R04真实流/恢复同步保留为验收依赖，R10独立仿真可并行。R09新增家族排在已有家族补齐之后；R20/R21保持低优先级。任何面向生产的放量都必须满足R16–R19，不能因其排列靠后跳过。

历史核对修正（2026-10-29）：WP19已有只读Kafka分析Job和持久cursor/失败checkpoint；WP08已有Rust owner bootstrap与router无KV恢复，缺的是对象存储/公共manifest/长期恢复；Raydium CLMM动态费、扩展bitmap和非零seed已局部完成，不再列为整体未开发。历史核对证据见代码与剩余范围审计（`integration/artifacts/local/remaining-features-audit.json`，已清理）；2026-11-01的NG增量、源码与构建绑定见[NG限定范围验收](integration/artifacts/local/curve-ng-acceptance.json)。

<a id="plan-2026-11-08-quote-completion"></a>

## 链与DEX解析、路由、兑换量计算收尾（进行中）

用户确认按计算优先继续，R01–R04来源与生产验收保留其真实状态，不能由数学测试代替。当前以原§3第一批8链矩阵为直接接入清单，原方案第二批6链及后续协议仍作为明确扩展缺口，不因配置生成器或通用EVM路径而记作已实现。目标是逐协议版本完成完整状态、同锚点解析、路由和精确兑换量，再补净收益所需链费用；当前全部要求尚未完成。LFJ新增的限定功能已独立闭合，其他实际DEX扩展和费用工作继续推进。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|C01|小图独立整数oracle；三路以上拆单、余数守恒、共享池顺序、取消/次数预算与公开质量边界|WP12,WP13,WP16,WP25|LOCAL_SCOPE_VERIFIED|新增三/四路全部网格与独立CPMM公式对照、八路独立DP、共享八路/预算与路径元数据反例；普通API三路原子单位输出与费用未知保留；最终三服务全检/当前进程复验通过；[阶段冻结](integration/artifacts/local/core-quote-q1-local-463896a2b08e.json)|
|C02|候选覆盖：分配量试算、浅池保留和整单最优fallback；图/链/物理池隔离|WP10,WP12,WP13,WP25|LOCAL_SCOPE_VERIFIED|新增浅池反例；排序使用小额与整单两个结果，仍保留最佳完整整单路径；候选上限/有限试算对最优性边界公开，增量图/更广搜索未闭合；[阶段冻结](integration/artifacts/local/core-quote-q1-local-463896a2b08e.json)|
|C03|Velodrome V2 stable/volatile独立版本、固定块解析、wire、原数学/连续后状态及普通服务|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP18,WP25|LOCAL_SCOPE_VERIFIED|固定b3065d8b原Pool源码，单独_f舍入分支；216原Solidity案例/42拒绝、8序列24步、30极端舍入控制通过；Go读取/严格配置/跨语言wire与Router分配已接，真实PG/Kafka双副本进程已通过；参考非完整swap/OP部署资格；[阶段冻结](integration/artifacts/local/core-quote-q1-local-463896a2b08e.json)|
|C04|逐链初期DEX缺口：实际Slipstream费用模块/Pancake LM、现有Solana/Curve/Balancer/Singleton高级组合及后续新模型|WP02,WP03,WP04,WP05,WP11,WP18,WP20,WP25|IN_PROGRESS|LFJ当前限定版本已另列C04.LFJ；Slipstream调用方/费用上下文及固定98684794 Pancake LM已通过限定主目录验收；后续真实BSC LM代际/迁移和其他协议组合继续，不解除未知模块/账户扩展/hook拒绝|
|C04.LFJ|LFJ v2.1.0完整状态/正常解析/跨语言wire/路由与实际API原程序差分|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP18,WP25|LOCAL_SCOPE_VERIFIED|96原LBPair.swap与2序列6步；Go96银行及严格配置/4 C++wire包含显式零，normal Go/真实本机PG/Kafka/双Router、reorg/撤销恢复/重启通过；19实际API输入与38原swap真实Token增量/完整后状态一致，10反例拒绝；[有限本地报告](integration/artifacts/local/core-lfj-local-q1-463896a2b08e.json)|
|C05|净收益费用与输入目标边界；实际链解析→路由→输出差分及最终覆盖清单|WP12,WP13,WP14,WP16,WP18,WP20,WP25|IN_PROGRESS|毛输出已存在有限真实快照差分；CHAIN_FEE_MODEL_NOT_IMPLEMENTED仍保留，ExactOut/特殊Token/第二批链另需对应能力，不由本批多路拆单宣称支持|

实现记录：优化器新增`optimize_with_limits`，硬时间/取消错误仍返回BUDGET_EXHAUSTED；次数预算耗尽有完整可成交方案时返回显式exhausted结果，无完整方案时不误报不存在可成交路径。`exhaustiveWithinGrid`只覆盖传入候选、有限分配网格、每个候选最多使用一次及配置max_splits，不表示全市场、所有原子单位或任意DAG最优。不同Vault钱包/Meta依赖/manager全局会计不走独立池DP。路径执行继续重放并检查完整输入/输出及代币连续性，重复物理池身份不得折叠。

参考和测试：`integration/tools/reference/velodrome-v2`保存原始GPL源码、提交/摘要和独立Solidity包装器；`make -C integration velodrome-v2-reference-refresh`重建原数学，`velodrome-v2-differential-test`回归Go解析与Router数学，`velodrome-v2-quote-test`运行受控RPC→normal Go→真实本机PG/Kafka→双Router。该服务用例的Solana保活为既有fixture-parser-rust，仅验证链隔离，不冒称新增主网流证据。


本批LFJ增量：固定26份上游v2.1.0源码、OpenZeppelin IERC20接口及官方Solidity0.8.10摘要，检查原storageLayout后执行原LBPair.swap/实际本地Token转账。96案例包含48成功/48拒绝；独立field31模型已接入Go正常固定块getter/storage/code读取、共享契约与Router，保留动态费、协议费、全部有界bins/份额和Oracle样本。4份C++编码覆盖显式零bank观察，缺失与null不冒充零。19实际API报价已原样关联38次原运行时代码交换，真实收款增量和全部声明后状态匹配；10个篡改反例均拒绝。早期草稿和失败JS整数检查记录保留；最终比较器采用BigInt无损整数并验证10^18与10^18+1不可混同。该原计划重放是固定时钟、受控银行和标准Token资金注入的本地Q1证据，不代表真实Avalanche历史/共识或生产执行。第一批8链16目录的现有模型与限制见计算覆盖核对（`integration/artifacts/local/core-quote-functional-coverage.json`，已清理）；原目录与来源资格不改写。


LFJ当前有限联验冻结：`da935d675c6c89bedb2e068a9cecca1f51ce6877a8ec27dc7dffe7e9eb53fe0f`。normal source共有952次固定锚点读取、25次receipt读取，公开quote始终INDICATIVE/net费用未知；不支持内部池地址自资助、未同步Token余额和未知Token行为。完整三服务回归将在本批余项完成后的最终源码上统一重跑，中途被后续LFJ实现替代的旧全检明确记为SUPERSEDED，不冒充当前最终通过。


2026-11-08阶段冻结：core `463896a2b08ef1a8be5da11a7c4bd32e4e8b5f6598778b9a9d9593d315c5bca3`。Go完整check/race 176个EVM用例/15完整分片通过；Rust Parser 854次执行、0ignored，Router 1014次通过、5项既有可选门禁ignored，均含契约目标重复；Router完整目标日志/最终build成功留存，host完成回执过期，因此不把其exit0标作直接观察。255契约/10基础及新源码fixture/164公共source副本、proto与边界检查通过。当前二进制串行复验Velodrome、LFJ及Polygon/Arbitrum原getter→normal链路均通过，后两者输出分别9/224255。阶段报告只闭合注明的本地Q1增量，all_requested_features_complete/production_ready仍false，不改写R01–R04资格。

R07继续推进：固定Slipstream Pool原运行时代码与保留真实Base implementation逐字节一致；实际动态费模块/Factory执行体在绑定immutable后与原源码编译体一致，CBOR元数据单独区分。真实Base Voter代码经Solidity0.8.19重编译执行体核对，gauges/isAlive为上下文无关public mapping getter；本地仅设置受控mapping值。108原swap中90完整、18部分成交；独立数学及完整后状态/类型与费用策略草稿已匹配90完整并拒绝18部分成交，4组/12步同块费率序列通过。草稿尚未接入普通解析/wire/路由，旧SLIPSTREAM_FEE_MODULE_UNSUPPORTED继续保留，不能将参考执行或代码体匹配当作新功能完成。


<a id="plan-2026-11-08-slipstream-context"></a>

## Slipstream调用方与Oracle费用上下文（注明范围已验证，2026-11-09）

按既定计算优先继续补R07，不解除旧profile门禁。固定f8717faa Pool、保留真实Base Factory源码、原custom/dynamic模块、经重编译执行体核对的真实Base Voter代码；新adapter为`aerodrome-slipstream/f8717faa-module-1`、独立field32。Voter仅使用原public mapping getter，原银行状态为受控输入；不将源码/代码匹配或本地交易当作可信历史/生产资格。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|S39.1|完整上下文状态、required zero/false、严格JSON与独立C++ wire，derived执行态不作canonical来源|WP02,WP03,WP11,WP25|LOCAL_SCOPE_VERIFIED|四独立C++输入、两Rust域/Go编解码及缺字段/null/重复/派生源反例通过；最终三服务全check通过|
|S39.2|固定块完整ABI、bytecode execution template/immutable、注册池键和有界classic EOA费用输入|WP02,WP03,WP04,WP05,WP11,WP18,WP25|LOCAL_SCOPE_VERIFIED|Go108原银行/模块code模板重算hash等拒绝通过；旧SLIPSTREAM_FEE_MODULE_UNSUPPORTED原门禁保留；Actor列表外上下文拒绝|
|S39.3|初始费用/同块观察写入/tx.origin折扣、完整staking/gauge/reward/tick/Oracle后状态与共享顺序分配|WP11,WP12,WP13,WP25|LOCAL_SCOPE_VERIFIED|108原swap内90完整及4组12步通过，18部分成交拒绝；shared allocation及跨协议内部钱包Alias拒绝通过；来源窗口与携带窗口分别记录|
|S39.4|普通Go/真实本机PG/Kafka/双Router和实际API原运行时代码重放及明确负控|WP03,WP04,WP05,WP11,WP12,WP13,WP18,WP25|LOCAL_SCOPE_VERIFIED|13实际API/13原swap真实收款与完整后状态一致；8指定错误负控拒绝；生产/净费用/VERIFIED不升级，最终同源进程通过|
|S39.5|当前三服务全检、项目契约/边界、进程工具/二进制/参考输入冻结和完整缺口核对|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP18,WP25|LOCAL_SCOPE_VERIFIED|统一runner将实际make退出码/完整日志摘要/前后源码hash保留到本地JSON；五进程范围已核对源码/二进制并通过，整体请求仍未全部完成|

阶段39过程边界：接入准备脚本曾在Rust公共源文件上重复应用，已保留重复版本并从包含全部原未提交修改的阶段38工作树备份精确修复五个文件，再只应用一次变更；公共Rust实现副本一致、全workspace编译与当前定向通过。旧阶段38冻结证据不被该过程改写。正常阶段切换还发现来源读取按新tick中心重取bitmap，而携带执行态仅能保留已知窗口；已从原程序补齐相应原始字节，未将未知word猜为零。

阶段39结束时只开放注明上下文与标准Token的有限ExactIn计算；Pancake V3 LM、其他Solana/Curve/Balancer/Singleton高级组合、需要净收益时的链费用和第二批链等仍须继续开发。`all_requested_features_complete=false`与生产资格关闭保持，不能把阶段39接入或最终全检通过等同全部请求已完成。


以下为2026-11-08隔离开发历史，已由阶段40主目录进度续接。阶段39期间的后续R07实现已在隔离副本`/tmp/xone-pancake-lm-next-20261108`推进：固定Pancake提交`986847948755cba528324d41be19480731c36c2a`，100个有效原银行中80完整成交/全后状态匹配、20部分成交拒绝，另4个有意不一致银行的原回滚与解析拒绝保留；4序列12步通过。新增独立field33、Go完整固定块解析及代码/immutable模板、两Rust域和严格wire/JSON、路由/共享状态及钱包Alias拒绝；100正常银行解析和9坏来源反例、4 C++wire通过。隔离目录13实际API/13原swap的真实收款和全部声明后状态一致，9个实际计划/原程序负控拒绝；当前格式和全检版源码`201705bc08c2fa6baffaec6c93c73a91c03057d25066cbc8369cf9e6533c6c6e`正在三服务全check，先前进程报告注明为格式前`d19767996acf9029d33195a0c3deb8034dd2d0351c6f595aea40e67905ca334d`，不冒充当前版复验。隔离状态（`integration/artifacts/local/core-pancake-lm-staging-state.json`，已清理）。截至2026-11-08记录：主目录仍保持阶段39冻结的`374ce438e77db25642fe7bac24d52b597328fc16ebbf1556e6878786dcaff012`；Go和Rust解析器全check已观察退出码0，Rust864次执行且0忽略；Router全check和最终同源进程刷新继续运行。须待阶段39完整冻结后按基线逐文件合入LM增量，并以最新源码/二进制重验；此时不将主目录LM接入或整体请求标记完成。


<a id="plan-2026-11-09-pancake-lm"></a>

## Pancake V3 LM解析、路由和完整兑换量（2026-11-09，注明范围已验证）

优先完成原§7.3/12.9–12.14/13.2–13.5要求的业务链路。本批使用固定`986847948755cba528324d41be19480731c36c2a`源码版本，新增独立`pancake-v3-lm-context`/`pancake-v3/98684794-lm-1`，公共field33；旧Pancake V3版本继续按其原能力拒绝非零LM。全部64项增量先逐文件验证原未提交工作树摘要并备份，再合入主目录，不覆盖其他原修改。[精确合入清单](integration/artifacts/local/core-pancake-lm-integration.json)。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|S40.1|Go及双Rust域完整LM/费用/奖励/Oracle状态、严格JSON和独立protobuf编码|WP02,WP03,WP11,WP25|LOCAL_SCOPE_VERIFIED|field33及required zero/false、缺字段/null/重复/派生状态拒绝；4独立C++wire，主目录双Rust/Go域与严格配置测试已通过|
|S40.2|固定blockHash完整Pool/Factory/LM/MasterChef/Token读取及代码模板与immutable绑定|WP02,WP03,WP04,WP05,WP11,WP18,WP25|LOCAL_SCOPE_VERIFIED|100有效原银行、4原程序回滚银行及9类来源篡改；主目录Go全检/原Getter银行已通过|
|S40.3|逐step协议/LP费、LM奖励双层floor/时间截断/增长回绕、tick/Oracle/raw余额后状态与共享物理池路由|WP11,WP12,WP13,WP25|LOCAL_SCOPE_VERIFIED|原80完整swap全后状态、20部分成交和4不一致银行拒绝、4序列12步；拒绝内部钱包Alias，主目录原数学与共享状态测试已通过；另补两协议双方向错误输出币拒绝及不变状态反例|
|S40.4|正常Go→实际本机PG/Kafka→双Router、实际API计划原运行时代码重放|WP03,WP04,WP05,WP11,WP12,WP13,WP18,WP25|LOCAL_SCOPE_VERIFIED|主目录最新二进制13实际API/13原swap收款与全部声明后状态一致，9负控、异常来源撤销/恢复与Router重启通过；[最终报告](integration/artifacts/local/core-lm-local-q1-20261109.json)|
|S40.5|主目录三服务全check、公共契约、原参考/工具/二进制同源绑定与进度回填|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP18,WP25|LOCAL_SCOPE_VERIFIED|同源Go全check退出0；Router全check退出0/1048执行/5既有忽略；Rust677工作区+197契约分阶段通过/构建退出0，原退出2保留；267契约、60 CI与6辅助测试通过；[最终报告](integration/artifacts/local/core-lm-local-q1-20261109.json)|

前置阶段39已收尾：阶段39最终报告（`integration/artifacts/local/core-stage39-final-20261109.json`，已清理）验证三服务全check源码一致，复用前先核对五二进制及每个旧日志/报告摘要，再补跑Polygon与Arbitrum实际正常服务回归；5项进程范围全部通过。原错误输出目录被`OWNED_NORMAL_OUTPUT`拒绝，已修复为合法`r01-public-`前缀，5项辅助测试包含目录所有权、二进制改变与日志篡改拒绝；保留旧失败报告。

本批仍限注明代码版本、标准ERC20及完整消费的ExactIn。原程序来源为受控原合约银行与合成固定时钟封套；真实BSC候选已观察到后续LM代际/惰性迁移，不能套用本版本。特殊Token、ExactOut、可信历史/独立来源、链费用后的最优净收益、第二批链及其他协议扩展仍按原剩余清单继续。`all_requested_features_complete=false`、`production_ready=false`和VERIFIED关闭保持。


阶段40复核补修：发现新LM/Slipstream专用分派跳过原通用输出代币校验，手工构造错误`Edge.to`可被误认为成交。已用LM反例实际复现：旧代码返回`999000749`而未拒绝；保留修复前失败日志（`integration/artifacts/local/core-lm-output-direction-red-20261109.log`，已清理）。现将所有双币内核的方向/输出校验前移到协议分派之前，新增两协议、双方向、同输入币/第三币输出的拒绝与状态不变反例。此前尚未完成的Rust全检和等待中的进程刷新已显式中断保留，不作为当前验收通过；按修复后源码重新检查和冻结。


构建缓存补修：修复版Rust解析器工作区677次执行全部通过后，独立contract目标命中了field33生成前的旧protobuf缓存而编译失败；源码与全工作区使用的wire均含field33，内容摘要未变。原因是隔离合入采用`copy2`保留了较旧mtime，Cargo窄目标将旧编译物误判为新鲜。合入工具已改为保留字节/权限但使用新mtime，并新增旧时间戳回归；本次仅刷新生成Rust文件mtime，补跑`make -C chain-parser-rust contract-test build`，不隐去原全check退出2或冒称原命令整体退出0。


阶段40最终结果：源码`31295d87bb32ae676223c3bb134c5c50a4bfa4b3fe9b153c817a0ac53b85752d`前后一致，五个服务二进制及LM计划投影器/回放脚本按摘要绑定。Go全`make check`退出0；Router全`make check`退出0，1048次执行通过、5个既有专用API/VM入口忽略（本批LM回放独立执行）；Rust解析器原`make check`在677次工作区执行通过后因旧mtime的窄目标wire缓存退出2，修复后`contract-test build`退出0、197次契约执行通过，原失败与分阶段凭证均保留，不记作原全命令退出0。

[最终有限业务报告](integration/artifacts/local/core-lm-local-q1-20261109.json)与六进程同源回归（`integration/artifacts/local/core-lm-final-process-20261109.json`，已清理）均通过。Slipstream、Velodrome、LFJ、Pancake LM以及Polygon/Arbitrum正常来源回归全部通过。新LM实际normal Go/本机PG与Kafka/双Router产生13条API计划，原Pool/LM/MasterChef执行13次swap的真实Token收款及完整声明后状态一致；9类模型/原程序篡改被拒绝，代码来源经当轮重新编译核验。双副本、固定块读取、坏来源撤销、恢复和重启均已覆盖。原80完整swap、20部分成交拒绝、4不一致银行拒绝、12序列步、4独立C++wire和两协议方向反例已通过。267公共契约副本/10基线fixture/164公开来源fixture保持一致；文档/CI及辅助普通/优化检查随后按最终报告再复验。

方向一致性：解析仍归各自owner服务，计算归dex-router；金额使用原子整数和原协议舍入；候选/拆单保持同快照、同物理池顺序状态及实际最终输入输出，不宣称有限候选/网格结果为全市场最优。当前只完成这批固定版本的有限业务功能；原R01–R21剩余清单、26工作包完整验收仍0、`all_requested_features_complete=false`与`production_ready=false`保持。下一步按原优先级补实际BSC后续LM代际/迁移、现有协议高级组合和净链费用模型，再推进未接入模型与第二批链。


<a id="plan-2026-11-09-bsc4-lm"></a>

## 实际BSC第四代LM与惰性迁移（2026-11-09，注明范围已验证）

用户要求持续补齐原设计链/DEX解析、路由和兑换量计算，本阶段继续R07，保持旧98684794版本边界。新增`pancake-v3-bsc-lm-v4` / `pancake-v3/bsc-p400-lm1000000-mc999-v4`与公共field34，分别保存真实账本、初始化标志、本地tick迁移标志及三代前置原始tick/标志。按third→second→first优先级读取，只有实际跨tick才写入本地；第一代没有lmTicksFlag，明确保存为空，不伪造观察值。算术内部共享已验证的V3/费用/奖励原语，独立BSC身份不进入旧field33来源。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|S41.1|当前BSC Pool/LM/MasterChef/Factory及三代前置代码和原源码身份|WP02,WP03,WP04,WP05,WP11,WP18,WP25|LOCAL_SCOPE_VERIFIED|公开源码已自行取得，当前三合约完整运行时重编译匹配；前置/Factory执行体重编译匹配；第一代存在CBOR metadata差异，第二/三代与Factory完整字节匹配，单独记录，不声称全字节完全相同|
|S41.2|field34、严格状态/配置、required false/zero、独立C++wire及源快照深拷贝|WP02,WP03,WP11,WP25|LOCAL_SCOPE_VERIFIED|4独立C++向量，缺字段/null/旧profile替代及别名拒绝；Go深拷贝前置数组问题已由反例发现并修复，修复后当前定向/Go全检通过|
|S41.3|固定blockHash完整来源与前置code/immutable/getter闭包、全scope原子拒绝|WP02,WP03,WP04,WP05,WP11,WP18,WP25|LOCAL_SCOPE_VERIFIED|Go64有效银行、4坏银行及来源篡改拒绝已通过；每代按实际布局和immutable核对，未知版本继续拒绝|
|S41.4|惰性迁移、fee/reward/tick/Oracle/raw余额完整数学与共享池顺序路由|WP11,WP12,WP13,WP25|LOCAL_SCOPE_VERIFIED|原64案例包含48完整/16部分成交，另4无效银行，5序列15步；实际前三代原程序版完整后状态、共享状态与资金Alias/方向反例通过|
|S41.5|normal Go/实际PG/Kafka/双Router、实际API计划原程序重放、故障恢复与回归|WP03,WP04,WP05,WP11,WP12,WP13,WP18,WP25|LOCAL_SCOPE_VERIFIED|当前源码/二进制normal13API/13原swap、6迁移/13负控、恢复重启及Go全检/双Rust全目标lint/定向已通过；[最终报告](integration/artifacts/local/core-bsc4-local-q1-20261109.json)|

来源与边界：前三代和Factory源码来自公开Sourcify记录，固定原编译器和settings离线重编译，并逐项区分执行体与CBOR metadata。受控参考部署使用原当前Pool/LM/MasterChef/Factory与三代原getter代码、明确合成银行和标准Token资金；没有把源码重建或本地执行升级为可信主网历史/共识证明。本阶段不覆盖LM领取/增减仓等非swap操作。其余链、DEX高级组合和净链费用仍按原清单推进，`all_requested_features_complete=false`和生产资格关闭保持。


阶段41当前回放：实际三代前置代码版原参考64swap（48完整、16部分成交）、4不一致银行及5序列15步已匹配；Go64有效getter银行/4拒绝、独立C++field34和深拷贝、Rust数学/配置/共享池定向通过。normal Go/实际本机PG与Kafka/双Router的13实际API计划重放13原swap、真实收款和完整声明后状态一致，实际触发6次tick迁移、13类篡改拒绝；来源撤销/恢复与Router重启通过。当前全workspace检查发现protobuf结构直接复制内部mutex，已改`proto.Clone`并补输入message不被修改的测试；旧正常进程只作修复前证据，待新源码/二进制重验。专用Rust检查脚本首次因PATH误选rustc1.83失败，已显式固定1.90重跑，失败日志保留；不改变服务/协议配置规则。


阶段41最终结果：源码`96e2632c9df0d9729134615209280322be03f4c5e8bef1db75b22af4e8d74f2f`在Go全检、双Rust全workspace/所有target格式与Clippy（warnings拒绝）、当前新版本定向、重构建与正常服务进程之间保持一致。Go完整`make check`退出0，182项EVM race覆盖16完整分片；包含新增4个Go域/配置/解析顶层测试组及旧版本回归。双Rust当前定向和所有target编译检查退出0；未将此前阶段40全Rust单元统计冒称本轮全workspace单元复跑。早期旧rustc环境失败、protobuf mutex复制vet失败及修复前/来源摘要变化报告均保留。

[第四代最终有限报告](integration/artifacts/local/core-bsc4-local-q1-20261109.json)核对当前源码、五个服务二进制、计划投影器/脚本、fixture和原程序来源摘要；13条normal实际API/13次原swap、真实收款与完整后状态一致，实际6次tick迁移，13负控全部拒绝，坏来源撤销/恢复及Router重启通过。[当前分阶段回归](integration/artifacts/local/core-bsc4-post-clone-revalidation-20261109.json)、[双Rust全目标lint](integration/artifacts/local/core-bsc4-final-lint-20261109.json)、[Go完整检查](integration/artifacts/local/core-bsc4-go-fixed-check-20261109.json)均通过。共享契约副本273份、10基线fixture、164公开来源fixture一致；4独立C++wire、60 CI/进度和专用原运行时绑定负控普通/优化检查通过。第一代前置仅执行体匹配，CBOR metadata差异保留，其他前置/Factory完整字节匹配。

本阶段完成固定实际第四代swap所需数据解析、惰性迁移、路由/完整兑换量计算与注明范围的本地原程序验证。真实历史bank/共识、完整Token行为、链费用、ExactOut、其他协议和第二批链仍未完成；没有提升全量设计或生产资格。后续继续原R05/R06/R08/R09/R12–R14的核心业务缺口。


<a id="plan-2026-11-09-graph-admission"></a>

## 状态驱动的路由准入刷新（2026-11-09，注明范围已验证）

继续原§10.4/13.2/15.5的状态→候选图→有效报价要求。实际反例在合法完整snapshot和连续BLOCK_COMMIT中改变共享Vault分组，币种、pool id及协议标签保持相同；旧副本只比较币种拓扑而复用旧graph，预期3池被排除而实际0池。修复前失败日志保留在`integration/artifacts/local/core-graph-admission-red-20261109.log`。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|S42.1|完整银行准入变化参与图版本与候选刷新，不因币种拓扑相同复用过期排除集合|WP09,WP12,WP13,WP25|LOCAL_SCOPE_VERIFIED|3池准入反例修复后通过；恢复后候选重建、旧图不被修改|
|S42.2|所有候选关闭时保留完整已提交银行和水位、撤销旧报价、readiness失败；新合格银行正常恢复|WP09,WP12,WP13,WP16,WP25|LOCAL_SCOPE_VERIFIED|副本与HTTP当前定向通过；空图view拒绝而canonical银行可继续追尾，原suspend/reorg屏障不能绕过|
|S42.3|真实HTTP handler的排除集合/缓存撤权/双方向恢复、正常服务和原程序回放回归|WP03,WP09,WP12,WP13,WP16,WP25|LOCAL_SCOPE_VERIFIED|17副本/12API和相关全target lint通过；当前normal13API/13原swap及6迁移/13负控回归通过；[最终报告](integration/artifacts/local/core-graph-admission-local-20261109.json)|

实现边界：只刷新实际完整银行导致的准入变化；新graph按完整物理池/共享依赖校验，错误来源仍拒绝原子替换。全部候选关闭的已提交银行可推进last_reference，公开view保持非ready；旧句柄撤销，新合格连续块可恢复，而显式suspend或reorg仍遵守原代际/重新锚定门禁。原全量/COW图索引、成本模型、其他协议和链仍未完成。


阶段42最终验证：副本17次执行（含3个新反例与原重组/暂停/恢复屏障）、真实in-process HTTP handler与API/恢复12次执行、相关crate全部target Clippy退出0；旧反例明确记录left0/right3，修复后实际候选/排除集合、旧报价失效、全空503与提交水位/恢复均通过。[当前验证流水线](integration/artifacts/local/core-graph-admission-validation-20261109.json)在前后同源码下重构建，再用正常Go/实际本机PG与Kafka/双Router捕获13API原计划重放13原swap，收款、全部声明后状态、6迁移和13篡改拒绝均通过。[最终报告](integration/artifacts/local/core-graph-admission-local-20261109.json)保留独立不可覆盖进程凭证及阶段41原程序来源绑定。

该反例的准入变化来自明确合法的完整模型银行，HTTP测试不冒充公开真实部署切换；normal进程原程序回归确认本次副本变更没有破坏已实现协议计算。没有扩大为任意共享DAG/COW/全市场最优或生产能力，其余设计业务缺口继续。


<a id="plan-2026-11-09-candidate-scope"></a>

## 候选范围与不可用池披露（2026-11-09，注明范围已验证）

继续原§12.13/13.2/13.3/15.3的能力和搜索范围要求。接口反例已实际复现：未知数学协议池与simulation-only Token行为池不进入graph边，但响应candidateScope仍包含这两行。修复为统一推导完整银行排除集合，明确PROTOCOL_MATH_UNSUPPORTED/TOKEN_BEHAVIOR_UNSUPPORTED，保留共享Vault更强排除原因；源池行不删除。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|S43.1|未知数学与不支持Token行为的显式候选排除，完整源scope保留|WP09,WP12,WP13,WP16,WP25|LOCAL_SCOPE_VERIFIED|API修复前反例left6/right4已保留，修复后API/图/恢复回归通过|
|S43.2|图边/公开candidateScope/excludedPools一致，能力状态变化后正确恢复|WP09,WP12,WP13,WP16,WP25|LOCAL_SCOPE_VERIFIED|新增graph与Replica能力撤销/恢复测试，不改变已建模Token费profile的计算规则|
|S43.3|相关域/数学/路由/API回归及当前正常服务原程序回放|WP03,WP09,WP11,WP12,WP13,WP16,WP25|LOCAL_SCOPE_VERIFIED|52 Router/11解析域、相关全target lint通过；normal13API/13原swap、6迁移/13负控回归通过；[最终报告](integration/artifacts/local/core-candidate-scope-local-20261109.json)|

本批只修正计算引擎的实际搜索范围披露，不能把未搜索的源行用于最优性保证，也不把排除语义等同其协议已实现。其余链/DEX、净费用和ExactOut业务缺口继续。


阶段43最终结果：源码`c8e4267332eeff1748233f7ce7be542d627844a729424a2cc49583cd6c148047`前后一致；52次Router域/图/副本/API/整数网格回归、11次解析域共享依赖回归和相关所有target Clippy退出0。修复前API实际candidateScope包含6池而实际可搜索仅4池，修复后两类不可用源行完整保留且分别明确排除，原Vault更强原因不被覆盖。[当前验证流水线](integration/artifacts/local/core-candidate-scope-validation-20261109.json)重构建三服务后，normal Go/实际本机PG与Kafka/双Router13实际API计划原样重放13原swap，收款/完整后状态、6迁移和13负控均通过。[最终报告](integration/artifacts/local/core-candidate-scope-local-20261109.json)保留当前不可覆盖进程凭证，并沿用已核验的原协议来源；不将旧源码全检冒称本轮复跑。

<a id="plan-2026-11-09-exact-out"></a>

## ExactOut独立输入目标与原Router基准（2026-11-09，注明范围已验证）

继续原§12.10/13.1/13.5/14/23.9的ExactOut独立门槛。不得对ExactIn结果取倒数或沿用最大输出后的旧后状态。先按固定原UniswapV2 Router02执行约束，从所需输出反向计算每hop输入，再用原Pair完整结算按执行顺序校验；其他协议必须各自通过门槛，不能统一贴支持标签。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|S44.1|原Router02.getAmountsIn/精确输出交易、完整Pair后状态/真实收款/输入上限反例|WP03,WP10,WP11,WP13,WP25|LOCAL_SCOPE_VERIFIED|6池/双向1–3hop：90原交易、162 Pair转换、2无效目标；39有序网格方案/38可行，原最小输入440077088837282106916853；原Factory双向getPair与全部getter/code/state投影校验通过|
|S44.2|独立ExactOut数学接口与精确目标结算，原子整数/舍入/容量/时间/原始余额|WP10,WP11,WP13,WP25|LOCAL_SCOPE_VERIFIED|独立目标/原floor+1、完整Pair结算与162转换、ExactIn过量反例及0/容量/输入上限拒绝测试通过；补齐来源闭包后的当前同源复验已通过|
|S44.3|金额感知反向路径、正向完整重放、目标输出守恒/最大输入预算与共享池|WP12,WP13,WP25|LOCAL_SCOPE_VERIFIED|90路径和39个有序分配/完整最终池状态匹配；独立反向候选/整单fallback/共享池顺序重算/输出精确守恒与次数取消测试通过；逐路径只允许单Factory和外部标准资金|
|S44.4|独立请求/计划/API与有效性、限流/取消和明确能力错误|WP12,WP13,WP14,WP16,WP25|LOCAL_SCOPE_VERIFIED|独立/v1/quote-exact-out、ExactOutRequest/Plan与xo缓存已接；4项真实HTTP handler测试通过，包含预算/滑点上限/零退款、鉴权/非法请求/重复请求/重组撤权及失败probe后的整单保留；build/模拟继续按能力拒绝|
|S44.5|normal来源/实际计划原Router执行、实际输入与精确收款、后状态及回归|WP03,WP05,WP09,WP12,WP13,WP14,WP16,WP25|LOCAL_SCOPE_VERIFIED|正常Go/实际PG/Kafka/双Router45实际API计划，61原Router交易真实扣款/精确收款及全部Pair后状态匹配；7类篡改拒绝、故障撤销/恢复和Router重启通过；未使用预算不计链上退款|

当前固定原UniswapV2完整状态/标准Token的独立ExactOut报价业务层、正常服务实际计划原执行与同源有限验收已完成；原EXACT_IN入口继续明确拒绝EXACT_OUT。原程序基准使用已固定的93份发布artifact/源码/许可证与标准Token、本地签名交易，未修改AMM字节码。max输入是预算上限，不等于已转入执行器的资金；必须区分unusedInputBudget与实际退款。完整来源、费用后目标及其他协议能力仍待各自验收。

阶段44首轮限定验收：源码`c2af56fe500d02e69cd9936931724c201daa24aae2208a4eb17326d0ac062ae8`在首轮14项验证流水线（`integration/artifacts/local/core-exact-out-initial-validation-20261109.json`，已清理）、最终构建与正常进程回放之间前后一致。82次Rust测试执行（10次新ExactOut、72次既有数学/路径/拆单/计划/API回归）、Router完整workspace全部target Clippy -D warnings和格式检查退出0；原始来源反例4项普通/优化共8次通过；进度17项普通/优化共34次及三服务契约复制检查通过。原93份发布artifact/源码/许可证未修改；90原交易/162 Pair转换、2无效目标和39有序网格/38可行已核对。网格原最小输入`440077088837282106916853`与正常HTTP三路结果一致。

当前normal Go/实际本机PG与Kafka/双Router的45实际API计划直接执行为61笔原Router02交易，真实发送方扣款、接收方精确到账、每步及最终全部声明Pair后状态一致；5类模型/公开响应篡改与2类原程序重放篡改共7负控通过，故障撤销/恢复及重启保持另一链可用。首轮限定验收（`integration/artifacts/local/core-exact-out-initial-local-q1-20261109.json`，已清理）保留不可覆盖进程凭证、API原输入/模型投影/原执行记录与摘要。原getter补采前正常接入失败、测试配置超界、数组比较顺序问题和无效负控值均已保留；早期被中断的预检不计作最终lint通过。

执行边界：旧EXACT_IN请求/计划保持；独立`/v1/quote-exact-out`仅允许固定原`uniswap-v2-complete`，反向原floor+1后按选择的目标正向完整结算。单路径同Factory、简单Token/唯一物理池，多路共享池按顺序重算；返回所需输入、用户预算、滑点上限及unusedInputBudget，原Router只拉取实际金额，因此refund为0。只声明给定候选/有限有序网格内的原始输入最小值；链费用后的net输入为空，公共build/模拟仍按能力拒绝，多路原程序验证为独立本地交易。

本阶段没有完成原设计的全部链和DEX业务。其他协议ExactOut、完整Token能力、高级池/新DEX与第二批链、净链费用，以及可信公开来源/独立共识和生产执行继续按R01–R21推进；`all_requested_features_complete=false`与`production_ready=false`保持。

搜索边界补修已完成：首轮有限原银行验收后，额外受控合法模型反例复现：Pair输入raw比reserve少1时，小目标1触发K拒绝，但整单目标4可用原floor+1输入1001完整结算；旧搜索错误地因小额probe失败丢弃整单和其多跳后缀。left0/right1失败日志已保留；修复为probe失败时独立试算整单目标，既保留可行完整路径，也保留可扩展的多跳后缀。双方向/单跳/多跳和实际HTTP反例已通过，原90路径/原生网格均回归。该受控状态不声明为公开历史标准Token缺口银行；它只证明完整模型允许状态下的搜索可行性边界。

阶段44补修后最终验收：当前源码`8742cb9681a8c096b796522c210e155746b32a1c758313bb9607ac5a5d88c9af`在[当前14项流水线](integration/artifacts/local/core-exact-out-validation-20261109.json)、最终二进制构建与完整正常服务原程序回放之间前后一致。84次Rust执行（12次ExactOut、72次既有回归）和Router全workspace/所有target格式、Clippy -D warnings退出0；45实际HTTP计划/61原Router交易、完整声明后状态与真实精确到账、7负控、来源故障恢复和重启全部通过。[当前限定验收](integration/artifacts/local/core-exact-out-local-q1-20261109.json)绑定当前源码/二进制、原银行与网格、不可覆盖进程凭证及probe先红后绿证据；首轮`c2af56fe`报告保留为历史范围，不冒充当前源码。

最终范围仍是原UniswapV2完整状态/标准Token的独立ExactOut原始输入计算；其他协议/链与高级组合、净链费用及公共原子执行未全部完成，原R01–R21和26工作包的整体状态不提升。

<a id="plan-2026-11-10-bsc4-exact-out"></a>

## 实际BSC第四代LM独立ExactOut（2026-11-10，注明范围已验证）

继续原§12.10/12.13/13.1/13.5/14的独立ExactOut能力门槛。仅扩展已固定实际BSC Pool400/LM1000000/MasterChef999与三代惰性迁移版本；原解析/field34保持，负amountSpecified按输出舍入/fee逐步正向推进，不用ExactIn反算替代。曲线完成后共享完整fee/reward/tick/Oracle账本结算；旧ExactIn回归单独验证。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|S45.1|原Pool/LM/MasterChef负amountSpecified执行、实际付款/收款与完整getter来源|WP03,WP10,WP11,WP13,WP25|LOCAL_SCOPE_VERIFIED|64原案例中48完整、16部分；4不一致银行拒绝、5序列15步；原代码/编译任务身份与输入扣款/实际到账、来源篡改普通/优化共8次及最终同源验证已通过|
|S45.2|独立ExactOut价格/step/fee舍入、完整费率/奖励/Oracle与惰性迁移后状态|WP10,WP11,WP13,WP25|LOCAL_SCOPE_VERIFIED|48原完整case/15连续全字段和部分/无效拒绝测试通过；V3/旧LM/BSC共10项原ExactIn与曲线回归通过，当前源码完整结算验证通过|
|S45.3|单物理池执行模板、资金alias与完整目标/最大输入、共享池连续分配|WP12,WP13,WP25|LOCAL_SCOPE_VERIFIED|单池路由/候选/API已接；顺序复用同物理池前两步分配匹配原输入和最终状态；混合多跳明确拒绝，单池限定/内部资金/取消/循环/预算反例、原输入和全后状态测试通过|
|S45.4|独立ExactOut API/计划/预算/缓存/失效、原ExactIn兼容与执行能力|WP12,WP13,WP14,WP16,WP25|LOCAL_SCOPE_VERIFIED|8模式共16方向银行API/计划、输入不足/部分目标/内部资金/重组与未实现执行拒绝通过；旧V2/计划/API回归及最终源码验证通过|
|S45.5|normal Go/实际PG/Kafka/双Router实际API原负输出回放、恢复与反例|WP03,WP05,WP09,WP12,WP13,WP14,WP16,WP25|LOCAL_SCOPE_VERIFIED|normal Go/实际PG/Kafka/双Router41实际API/41原swap、31迁移/13负控、故障恢复/重启；V2正常45API/61原交易/7负控回归通过；[当前报告](integration/artifacts/local/core-bsc4-exact-out-local-q1-20261110.json)|

来源和执行边界：受控银行仍使用原Pool/LM/MasterChef及三代原getter代码、已记录合成storage和标准Token回调资金；不是可信公开历史/共识或主网部署资格。单池参考调用的输入限额在报价层核对，公开build/模拟仍拒绝，不声称已有可公开使用的带输入上限原子执行器。其他协议/版本ExactOut、净链费用和全部剩余链/DEX仍未完成，整体完整标志保持false。

下一版本参考准备（尚未开放报价）：固定98684794旧LM版本已用独立负amountSpecified原程序生成100案例（80完整、20部分）、4不一致银行和12连续步骤；实际输入扣款/收款、原始getter与代码/来源摘要的离线校验通过。新数据仅存于integration参考目录，尚未复制到服务或扩大旧profile的ExactOut准入；旧版报价实现与正常服务验收仍待后续独立完成。

阶段45最终结果：源码`4342ea8328ffdb9a7e44f5e91fcb0f8b4b43dbae0419a54fd1e399a7e7d4572f`在[18项统一验证](integration/artifacts/local/core-bsc4-exact-out-validation-20261110.json)、最终构建与两项正常进程回放之间保持一致。97次Rust执行（8次新BSC ExactOut、12次V2 ExactOut及77次曲线/账本/路径/分配/计划/API回归）、Router全workspace/所有target格式与Clippy -D warnings退出0；来源4类反例普通/优化共8次、进度17项普通/优化共34次与三服务契约复制检查通过。原BSC64案例48完整、16部分拒绝，4不一致银行拒绝及5序列15原步骤的完整声明后状态、输入扣款和精确到账通过。

当前normal Go/实际本机PG与Kafka/双Router对原getter银行进行正常解析，经真实HTTP产生41个ExactOut计划，按负amountSpecified原程序重放41次实际swap；实际输入、精确收款、全部声明后状态及31次实际tick迁移一致，13类模型/原程序篡改拒绝、来源撤销/恢复和重启通过。相同源码还完成V2的45实际API计划/61笔原Router交易与7负控回归。[最终限定验收](integration/artifacts/local/core-bsc4-exact-out-local-q1-20261110.json)保留当前源码/二进制/工具/原数据摘要及两项不可覆盖进程凭证。

初期命令误选不存在的测试target、进程预检期间补充源码导致摘要变化，以及API大JSON宏递归上限失败均保留为失败记录，不计作最终通过；响应已拆为独立quality对象，未扩大编译器宏上限。最终验证只采用后续完整通过的当前源码。旧98684794版本本轮只有独立负输出参考及来源反例准备完成，尚未开放其ExactOut报价，不能计入S45正常支持范围。

本阶段只完成实际BSC第四代LM的单池、标准Token、原始输入目标ExactOut计算；公开build/模拟、带链上最大输入保护的执行器、跨协议多跳、净费用、其他链/DEX和生产资格仍未完成。整体`all_requested_features_complete=false`和`production_ready=false`保持，原26工作包完整验收不提升。

<a id="plan-2026-11-10-legacy-lm-exact-out"></a>

## 固定98684794旧LM独立ExactOut（2026-11-10，限定范围已验证）

继续原§12.10/12.13/13.1/13.5/14与R14的独立版本门槛。固定`pancake-v3-lm-context` / `pancake-v3/98684794-lm-1`仅增加独立ExactOut准入，保留原field33完整解析与版本身份。与实际BSC第四代的profile、LM布局及迁移字段保持各自边界；这项旧版本没有惰性前置迁移。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|S46.1|原Pool/LM/MasterChef负输出、实际资金与完整原getter投影闭包|WP03,WP10,WP11,WP13,WP25|LOCAL_SCOPE_VERIFIED|100原案例80完整/20部分，4不一致银行、4序列12步；新增不写源文件的逐字节重新投影检查，来源5组篡改普通/优化10次通过|
|S46.2|独立精确输出与fee/reward/Oracle完整状态及连续交换|WP10,WP11,WP13,WP25|LOCAL_SCOPE_VERIFIED|共享已验证负输出曲线与对应旧版本完整结算；80原完整/12步和部分/无效/零/宽度/步数边界通过|
|S46.3|单池回调模板、资金限制、共享池顺序分配、候选与整单预算|WP12,WP13,WP25|LOCAL_SCOPE_VERIFIED|单池路由/资金alias和连续同池分配已接；多跳/混合路径拒绝，预算不足保持原子拒绝|
|S46.4|API/计划/缓存/重组、重新哈希多跳计划拒绝与旧能力回归|WP12,WP13,WP14,WP16,WP25|LOCAL_SCOPE_VERIFIED|10状态20方向API原输入/目标/完整后状态与重新哈希多跳拒绝通过，9次新增执行及统一回归完成|
|S46.5|normal Go/实际PG/Kafka/双Router实际API原负输出回放与故障恢复|WP03,WP05,WP09,WP12,WP13,WP14,WP16,WP25|LOCAL_SCOPE_VERIFIED|normal Go/实际PG-Kafka/双Router49实际API与49原兑换/9负控通过；BSC4 41原兑换/31迁移/13负控、V2 45 API/61原交易/7负控同源码通过|

当前只扩大固定原版本的报价业务；链上输入上限执行器/公开build和模拟仍未实现，不能把报价maxSellAmount冒充已有链上保护。有限标准Token回调资金、本地受控storage及合成RPC头不证明可信公开历史/共识。其他协议/链、新DEX与高级组合、净费用继续按原R01–R21推进；整体完成与生产标志保持false。


阶段46最终结果：源码`362472aa8cec000e16ffd5d1a969c1a5aef30533484e4e1037ca14366ab58067`保持到20项统一检查、最终构建及三项正常服务回放结束。106次Rust执行、Router全workspace/所有target格式与Clippy -D warnings退出0；来源5组反例普通/优化10次、进度17项普通/优化34次和三服务契约复制检查通过。旧LM49实际API/49原兑换/9负控、BSC4 41原兑换/31迁移/13负控，以及V2 45 API/61原交易/7负控均由[限定验收](integration/artifacts/local/core-legacy-lm-exact-out-local-q1-20261110.json)绑定同一源码/二进制与三项不可覆盖进程凭证。其他链/DEX、公开执行与净链费用继续待完成；原26完整工作包不提升。

<a id="plan-2026-11-10-slipstream-exact-out"></a>

## 固定Slipstream调用者费率独立ExactOut（2026-11-10，限定范围已验证）

按原§12.10/12.13/13.1/13.5推进`aerodrome-slipstream-context` / `aerodrome-slipstream/f8717faa-module-1`的独立输出目标计算，复用field32固定块解析及完整原Pool/Factory/模块身份。保留调用者origin、Gauge分成、奖励、跨tick与Oracle上下文；动态模块没有origin证据的请求不能按默认费率报价。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|S47.1|原负输出、实际付款/到账和逐getter投影闭包|WP03,WP05,WP11,WP25|LOCAL_SCOPE_VERIFIED|3费率模块×3质押比例×2方向×6输出规模，90完整/18部分；4序列12步；228前后银行/12960原ABI读取一致|
|S47.2|独立输出驱动曲线、Gauge/奖励/fee/Oracle完整状态|WP10,WP11,WP13,WP25|LOCAL_SCOPE_VERIFIED|90完整/18部分拒绝/12连续原步骤及完整账本、9次新增Rust与统一121次执行通过|
|S47.3|origin绑定、单池回调模板、有序共享池和输入预算|WP12,WP13,WP25|LOCAL_SCOPE_VERIFIED|必须显式使用请求caller上下文，动态模块未知origin/内部资金/多跳及部分成交拒绝|
|S47.4|API/计划/缓存/撤销及已有ExactOut/ExactIn回归|WP12,WP13,WP14,WP16,WP25|LOCAL_SCOPE_VERIFIED|18模块/质押/方向银行、重新哈希多跳拒绝、候选实际预算响应及旧4能力回归通过|
|S47.5|normal Go/实际PG/Kafka/双Router原负输出回放及故障恢复|WP03,WP05,WP09,WP12,WP13,WP16,WP25|LOCAL_SCOPE_VERIFIED|normal Go/实际PG-Kafka/双Router的3模块×3质押、49实际API/49原兑换/9负控及故障恢复/重启通过；5项正常服务回归同源码完成|

参考为固定原代码与受控初始化storage，引用合成RPC头用于本地重放；不是可信公开Base历史bank或主网资格。caller为有证据的origin，实际Token付款仍由参考回调合约完成；公开build/独立模拟、链上最大输入保护和原子拆单执行器保持待完成。整体完成与生产标志保持false。

阶段47验证安排：旧LM当前批正在执行Router全workspace/所有target Clippy。阶段47保持API/曲线/路径/有序分配/计划的全部注明回归，新增9次Rust测试，并检查全workspace库/二进制/示例及全部新增测试的Clippy；其余未修改测试target不重复宣称新源码下的全目标lint。正常服务最终凭证必须使用合入后的同一源码与二进制。

阶段47额外修复：ExactOut候选数超过maxCandidates时原实现会截断列表但未设置budgetExhausted，现已补齐，并公开实际candidateSearch预算；已有原V2多路径反例先在旧实现退出101，再在修复后退出0，保留红/绿日志；合入后的标准Cargo V2回归和同源码正常服务已通过。


阶段47最终结果：源码`11afffeefa15944851fb7d389722131c3bc7070b15e0fafd8a905773456fafde`在[24项统一验证](integration/artifacts/local/core-slipstream-exact-out-validation-20261110.json)、最终构建与5项正常进程回放之间保持一致。121次Rust执行（新增Slipstream9次、旧LM9次、BSC4 8次、V2 12次及83次曲线/账本/路径/分配/计划/API回归）、全workspace库/二进制/示例和新增/本批修改测试的Clippy -D warnings、全workspace格式检查退出0。该批没有把未修改的其余测试target计作新源码的全目标lint；旧阶段46的完整全目标结果保留其自身源码身份。来源6组普通/优化12次、进度17项普通/优化34次及三服务契约复制277项通过。

normal Go/实际专用PG与Kafka/双Router完成3种费用模块、3种质押状态和两个方向的49实际API/49原兑换，输入扣款、精确到账及全部声明后状态一致；9类模型/原程序篡改拒绝、模块代码撤销/恢复与Router重启通过。同源码还完成Slipstream ExactIn 13原兑换/8负控、旧LM49原兑换/9负控、BSC4 41原兑换/31迁移/13负控，以及V2 45 API/61原交易/7负控。[限定验收](integration/artifacts/local/core-slipstream-exact-out-local-q1-20261110.json)保留5份不可覆盖的进程凭证。首次进程预检错误地跨副本查询quote，本次已按副本保存引用并完整重跑通过；首次失败日志未删除、不计为通过。

本阶段仍仅为固定原版本/标准Token/有限已知origin的单池指示性报价。可信公开链bank、链上输入限额保护、原子多路执行器、公开build/模拟、净费用及其他链/DEX仍待完成；整体完成与生产标志保持false，原26工作包完整验收不提升。

阶段47结束时的下一版本参考准备（以下为当时状态；阶段48结果见后）：固定v2.1.0原getSwapIn/getSwapOut与Pair实际转账生成288个目标，216完整目标、72不完整目标；60个完整目标有额外到账，input-1原getter均不足；6序列18步骤。独立暂存输出驱动数学已对156恰好目标、60额外到账和72部分目标以及18连续步骤通过预检。上述数据仅在integration原始/草稿参考中；实际API需显式表达实际到账与额外输出，不能将它们裁剪成目标数，阶段47当时的服务LFJ ExactOut准入保持关闭，后由阶段48独立验收开放注明范围。


<a id="plan-2026-11-10-lfj-exact-out"></a>

## 固定LFJ v2.1最小输入目标与额外输出（2026-11-10，限定范围已验证）

继续原§12.10/13.1/13.5：按原getSwapIn独立求覆盖收款目标的最小输入；原Pair仅接受输入，bin整数舍入可能令真实到账大于目标。实际到账、目标和额外输出必须分别保留，完整池后状态按真实到账结算；不裁剪实际收款、不伪造池余额。现有四类可精确给出目标的模板保持原数字及序列化兼容；新增可选字段只在存在额外到账时出现，公开grossBuyAmount描述实际到账、minBuyAmount描述用户目标。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|S48.1|原getSwapIn、输入-1控制、实际Token付款/收款和完整账本|WP03,WP05,WP11,WP25|LOCAL_SCOPE_VERIFIED|288目标216完整/72不足，60有额外到账，216个input-1均不足；6序列18步；受控clone/factory sentinel和分开预付款，不是原Router或公开原子执行|
|S48.2|独立输出驱动输入/fee步进、精确实际到账与完整后状态|WP10,WP11,WP13,WP25|LOCAL_SCOPE_VERIFIED|独立输入/fee和216实际后状态、60额外到账、72部分拒绝/18连续已通过标准Cargo|
|S48.3|单池模板/资金边界、目标与实际流分离、共享状态与最小输入分配|WP12,WP13,WP14,WP25|LOCAL_SCOPE_VERIFIED|路径target_output与分配gross_buy_amount为仅非零surplus出现的可选字段；单池/共享状态/真实输出评分及原四类序列化回归通过|
|S48.4|API实际gross/目标min/额外输出、预算/缓存/反例及已有能力回归|WP12,WP13,WP14,WP16,WP25|LOCAL_SCOPE_VERIFIED|新API/伪造额外输出反例和10次新执行通过；原四类38次回归、最终136次执行及同源码验收通过|
|S48.5|normal Go/实际PG-Kafka/双Router/原Pair付款到账与故障恢复|WP03,WP05,WP09,WP12,WP13,WP16,WP25|LOCAL_SCOPE_VERIFIED|normal Go/实际PG-Kafka/双Router181实际API/181原Pair，24次额外到账与EOA真实扣款/完整后状态、9负控及撤销恢复/重启通过；27检查/7项正常服务回归同源码通过|

LFJ ExactOut固定版本的注明计算范围已通过本地验收；原有ExactIn同源码复验通过。整体完成与生产标志保持false；可信公开链状态、公开执行/原子资金保护及净费用仍待独立完成。

阶段48来源表示：原Oracle样本按原始索引顺序记录，执行重算保持携带顺序；canonical来源投影按普通Go解码器的当前/下一Oracle索引顺序读取。两者按真实样本ID对应同一原始storage，不通过改值抹平表示差异。首次正常进程预检在该表示比较处失败，修正投影后重新验证，失败日志保留。

阶段48标准Cargo新增10次执行通过，覆盖288目标（216完整/72不足/60额外到账）、18连续步骤、实际输出评分、共享状态、API以及重新哈希的额外输出隐藏反例；原四类38次定向回归通过。普通来源5类反例通过，540银行/17280原getter及storage独立核对通过。normal服务181实际API与原Pair正向/负向预检通过；最终同源码验收仍进行中。


阶段48最终结果：源码`cdd28fb43af6cba43fe42708943a71234abf9ae11499b4420c8fc25e84efe372`在[27项统一验证](integration/artifacts/local/core-lfj-exact-out-validation-20261110.json)、最终构建及7项正常进程回放中保持一致。136次Rust执行（LFJ新增10次、原四类ExactOut共38次及88次曲线/账本/路径/分配/计划/API回归）通过；全workspace库/二进制/示例及新增/本批修改测试Clippy -D warnings、全workspace格式检查退出0。其余未修改测试target不宣称已重复全目标lint。来源5组普通/优化10次、进度17项普通/优化34次及三服务契约复制278项通过。

normal Go/实际专用PG-Kafka/双Router产生181条LFJ目标API计划，原Pair执行181次；输入-1原getter不足、EOA真实付款、recipient实际到账和完整声明后状态一致，24条非零额外输出逐条核对通过，9负控及来源撤销/恢复/重启通过。同源码还完成LFJ ExactIn 19 API/38原swap/10负控、Slipstream ExactOut49原swap/9负控和ExactIn13原swap/8负控、旧LM49原swap/9负控、BSC4 41原swap/31迁移/13负控、V2 45 API/61原交易/7负控。[限定验收](integration/artifacts/local/core-lfj-exact-out-local-q1-20261110.json)绑定7份不可覆盖进程凭证与最终来源/工具/二进制摘要。

首次统一验证在lint发现冗余借用和不必要闭包，修正后重新执行全部27检查，旧报告保留为独立失败文件；未把修正前136次执行冒充最终源码结果。最后汇总修正了旧ExactIn报告使用original_native_replay字段的引用，首次汇总失败日志保留；最终凭证仍读取通过的同源码报告。公开build/独立模拟、链上输入上限与原子预付款执行器、真实公开Avalanche bank、净链费用及其他链/DEX仍待完成。整体完成与生产标志保持false，原26工作包完整验收不提升。


后续新家族参考准备（阶段48结束时尚未实现Meteora服务能力，阶段49进展见后）：固定MeteoraAg/dlmm-sdk提交`1532e540db41ac405dbf4c5bb2ddfceaa1f98ff1`（上游提交时间2026-11-04），按完整Git tree的blob身份与SHA-256保留138份SDK/IDL/SBF测试artifact和账户fixture。只读IDL逐字节解码3份904-byte池及其10136-byte数组，通过全部声明长度。实际保留SBF摘要`942f609d03440911f15c40e8df1733d808670768fd9042af1de956964fb2f0e6`在已有固定solders/LiteSVM中成功执行Token CPI和收款；默认原fixture含有非零限价订单，未冒充普通bin状态。

新增MM-only受控参考明确把function_type设为LiquidityMining、清零限价订单账本、按LP bin/协议份额重建vault余额，并记录原/覆盖账户摘要；原SBF在5种费率/时间设置、双方向和7金额共70案例中35成功/35拒绝，10条序列30原步骤通过。改写的Clock、WSOL mint/backing与用户资金均属受控本地银行；不证明主网部署二进制身份或可信历史。当时参考仅保留于integration，Meteora解析/公共状态/报价/路由和正常服务尚未开发，不能升级其协议覆盖状态或整体完成标志。


<a id="plan-2026-11-10-meteora-mm"></a>

## Meteora DLMM 固定 MM-only ExactIn 链路（2026-11-10）

设计对照：§4/7.3/8/11.3/12.2/12.11/12.14/13/23，WP02/03/05/09/11/12/13/20/25。延续原方案的链与DEX解析、路由和兑换量优先级；使用独立Solana DLMM状态与整数内核，不复用LFJ的价格、动态费或Oracle实现。设计方案保持不变。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|M49.1|固定原程序、IDL、账户、Token运行时与显式受控银行|WP02,WP03,WP05,WP09,WP11,WP12,WP13,WP20,WP25|LOCAL_SCOPE_VERIFIED|固定1532e540提交/942f609d SBF/53e587ee Token代码及138文件；95原case、20连续后续与独立物理转账/原始字段检查通过；不是主网资格|
|M49.2|独立domain/公共wire/普通RPC及已提交bank解析|WP02,WP03,WP05,WP09,WP11,WP12,WP13,WP20,WP25|LOCAL_SCOPE_VERIFIED|field35、程序/PDA/Token/Clock/原始账本及普通RPC/bank入口通过；三服务复制与公共生成代码、未知代码/混模型拒绝通过|
|M49.3|MM ExactIn整数内核及完整声明后状态|WP02,WP03,WP05,WP09,WP11,WP12,WP13,WP20,WP25|LOCAL_SCOPE_VERIFIED|58完整初始case/37拒绝，另20连续后续完整swap；95原case的注明整数/账本范围通过，包含120秒滚动/环扩容/缓存与fee边界|
|M49.4|路由、有序共享状态、API计划及撤销恢复|WP02,WP03,WP05,WP09,WP11,WP12,WP13,WP20,WP25|LOCAL_SCOPE_VERIFIED|实际独立PG-Kafka/双Router的71 API计划与71原SBF真实收款/完整声明后状态一致；9故障撤销/新代恢复及Router重启通过|
|M49.5|统一最终验证、现有家族回归与凭证冻结|WP02,WP03,WP05,WP09,WP11,WP12,WP13,WP20,WP25|LOCAL_SCOPE_VERIFIED|26检查/108 Rust、双Rust全目标check、所列库/二进制/示例及新测试Clippy、Go域测试和最终构建通过；来源及进程凭证同源冻结|

明确范围：`meteora-dlmm-mm` / `meteora-dlmm/1532e540-mm-1`，固定保留的原SBF、permissionless并使用base-factor种子的pair、固定53e587ee classic Token代码/backed WSOL、无奖励、function_type=LiquidityMining、零限价订单账本、无host-fee账户、InputOnly费用、默认bitmap内最多32声明数组及4096步；输入必须全部成交，跨越未提供数组、扩展bitmap或特殊分支失败封闭。当前提供ExactIn，ExactOut独立门槛尚未实现。

参考中的function_type、订单账本、vault配平、Clock、WSOL backing及用户资金是明确受控覆盖；正常解码测试还保留原Token可执行代码，显式将两个本地ProgramData部署slot设为1以适应受控Clock。普通进程只重标本地slot与合成区块头，不冒充主网ProgramData来源或可信历史银行。完整声明后状态指pair/arrays/Oracle原始字节及vault真实余额；原SBF额外核对用户实际付款/收款及WSOL lamport守恒，不代表一般账户模拟器。

当前状态：注明MM-only ExactIn范围已通过同源码统一检查与正常服务本地验收。[当前统一报告](integration/artifacts/local/core-meteora-mm-validation-20261110.json)与[正常进程报告](integration/artifacts/local/core-meteora-mm-normal-process.json)已完成并绑定本节最终结果；不能沿用其他家族的历史凭证冒充当前源码验收。整体all_requested_features_complete=false、production_ready=false；原26工作包完整验收仍0。

阶段49准入自审补强：classic Token的执行代码也固定为原参考实际使用的53e587ee摘要，不能通过替换配置摘要准入未知代码；新增配置与规范状态两处未知版本反例。此前统一验证保留为中断凭证，修改后的最终统一验证必须重新运行，不能沿用中断前结果。正常进程测试关闭低优先级历史worker，仅验当前账户解析/路由，历史覆盖不包含在本批声明中。

阶段49补充原SBF边界复核：额外25个Oracle/缓存/费用银行覆盖0/1/119/120/121秒、满环/尾部回绕/扩容/单样本、零费率、费用封顶与波动率封顶。原程序确认Oracle在恰好120秒滚动，修正为>=120；有流动性却无缓存价格的银行原程序拒绝，不再按SDK辅助函数补价后给出可执行报价。25个案例中23个完整成交、2个无缓存价格拒绝；新增原始字节/真实收款验证与Rust完整后状态差分已通过。此前探测发现边界错误的报告保持passed=false并保留。

Oracle扩容参考进一步使用正确的未初始化零记录，解析器同时拒绝active_size与非零未初始化尾记录矛盾的银行。新增边界差分现已通过，覆盖23完整后状态与2缓存价格拒绝；相关原Oracle计数器与真实到账篡改也必须在重哈希后拒绝。

阶段49正常服务当前结果：源码51fe374be5d29fb07454ac87f61a61814f7d2a6eee2dbbea8591b0bf3276a7ed的Rust正常RPC/独立PG-Kafka/双Router已产生71条冻结API计划，原SBF新银行执行71次，实际付款/收款和pair/arrays/Oracle/vault声明后状态一致；9类来源故障撤销及新代恢复、Router检查点与tail重启通过。完整原SBF逐笔前后银行已保存于进程报告指向的original_capture，未只保留摘要。该时点全目标编译/旧家族回归与统一凭证尚在运行；最终结果见后。

阶段49最终结果：源码`51fe374be5d29fb07454ac87f61a61814f7d2a6eee2dbbea8591b0bf3276a7ed`在全部26项统一检查、最终构建与正常进程运行前后保持一致。[当前限定验收](integration/artifacts/local/core-meteora-mm-validation-20261110.json)状态为LOCAL_SCOPE_VERIFIED，绑定不可覆盖的同源进程凭证（`integration/artifacts/local/core-meteora-mm-process-51fe374be5d29fb07454ac87f61a61814f7d2a6eee2dbbea8591b0bf3276a7ed-4624f5a16e047699d3a44405801a58724ed5bcb9e6b1a9078f64580940f6dfb5.json`，已清理）。108次Rust执行通过，包含解析器新增及既有54次、Meteora数学/路由/共享优化/API 9次及45次LFJ/V2/V3/候选/拆单/API回归。两个Rust工作区的workspace/all-targets编译检查退出0；Parser所改domain/config/ingest库及新测试、Router workspace库/二进制/示例及本批新测试的Clippy -D warnings退出0。未宣称对全部未修改测试目标重复全目标Clippy。Go domain测试/额外vet、公共代码再生核对、281契约数据复制/10 fixture/164公开来源文件，以及17项进度测试普通/优化34次通过。

原参考主矩阵70个初始case（35完整/35拒绝）及20连续后续完整swap；补充25边界case（23完整/2拒绝），合计95初始case、78完整swap步骤与37拒绝。13组来源篡改在普通/优化模式拒绝通过，零费率/费用封顶/波动率封顶与0/1/119/120/121秒Oracle环形/增长/回绕边界均有原SBF依据。旧错误Oracle边界探测、缓存价格反例、扩容不一致输入与中断前统一凭证保留，没有当作最终通过结果复用。

最终正常Rust RPC/实际独立PG-Kafka/双Router再验71条API与71次新的原SBF执行，真实payer扣款、recipient到账、pair/arrays/Oracle完整原始字节及vault余额均匹配；9类故障撤销/恢复和检查点-tail重启通过。进程报告保留完整原程序前后银行、公开API请求/计划、生产内核投影及工具/输入/二进制摘要。旧进程回执只有在源代码、全部二进制/工具/输入摘要及71条三组完整数据均一致时才可复用；本次最终进程步骤实际重新运行通过。

设计方向与验收边界不变。只提升本批注明MM-only业务范围；OnlyY、奖励/订单、Token2022、bitmap扩展、Meteora独立ExactOut、可信公开Solana bank/ProgramData、公开执行与净链费用、Pump及其余链/DEX仍未完成。整体all_requested_features_complete=false、production_ready=false，原26工作包完整设计验收仍0。


<a id="plan-2026-11-11-meteora-exact-out"></a>

## Meteora DLMM 固定 MM-only ExactOut（2026-11-11）

对照设计 §7.3/8/11.3/12.2/12.10/12.11/12.14/13/23.9。沿用阶段49字段35的固定程序、Token、账户及银行准入；为 `SwapExactOut2` 单独按输出目标遍历 bin，逐段向上舍入输入与费，同次计算得到真实付款和后状态。所有原程序计算与进程回放使用受控本地银行。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|M50.1|固定原SBF输出目标、实际付款/收款及硬输入上限|WP03,WP11,WP20,WP25|LOCAL_SCOPE_VERIFIED|164初始目标（142成功/22拒绝）、20后续成功步骤、142个输入减一失败；原银行新执行与完整账户字节核对通过|
|M50.2|独立整数计算及舍入余量的完整后状态|WP11,WP12,WP25|LOCAL_SCOPE_VERIFIED|原程序精确向收款人转入目标；25步bin扣减超出到账，余量留在vault，不误记为用户到账或协议费。路由重算原字段和账本|
|M50.3|Solana单池候选、有序分配、API与计划边界|WP09,WP12,WP13,WP14,WP16|LOCAL_SCOPE_VERIFIED|区分大小写的base58和u64限额、原program身份、链族/资金别名与多跳拒绝；HTTP逐个对照142成功目标；缓存随状态撤销|
|M50.4|同源码回归与正常Rust/PG/Kafka/双Router原计划回放|WP05,WP16,WP20,WP25|LOCAL_SCOPE_VERIFIED|最终19检查/65 Rust及5类既有EVM ExactOut回归通过；43实际API/43原SBF/43硬输入控制、9类故障撤销恢复与Router重启通过；来源与完整回执同源冻结|

本批仅覆盖阶段49注明的 classic SPL/backed WSOL、无奖励/订单/host fee、InputOnly、默认bitmap范围；每条路由单池。报价为INDICATIVE，净链费用为null。未提升整体设计完成、公开执行、生产准入与可信历史银行标志。

阶段50首轮预检：独立数学已对164初始目标与20后续步骤逐字节匹配原SBF，142成功/22拒绝和142个硬输入减一控制通过，25步骤保留真实vault舍入余量。解析状态与字段35保持阶段49版本。正常进程首轮在vault-authority恢复处遇到KAFKA_TAIL_DEADLINE；同次日志观察到较大系统时间跳变，保留失败日志与失败报告，不以该次运行充当验收。最终统一验证仍在进行，不能据组件测试宣称完整生产能力。

阶段50最终结果（2026-11-11）：源码`24c770a8f443e69e61ecd2a5c8dc5ebc68ee4f3b8d127115b084287e8d34607d`在统一验证、最终构建与正常服务前后保持一致。[19项统一检查](integration/artifacts/local/core-meteora-exact-out-validation-20261111.json)全部退出0，[最终限定验收](integration/artifacts/local/core-meteora-exact-out-local-q1-20261111.json)状态为LOCAL_SCOPE_VERIFIED并绑定不可覆盖的完整进程凭证。65次Rust执行包含17次Meteora新增/ExactIn回归和48次V2、LFJ、Slipstream、旧LM及BSC4 ExactOut回归。Router workspace库/二进制、新生产投影示例和本批新测试Clippy -D warnings退出0；格式、三服务数据复制282项/10fixture/164公开来源文件、17项进度测试普通/优化34次及当前Parser/Router/Go辅助二进制构建通过。本批未重复宣称双工作区全目标Clippy或此前阶段49的当前源码全目标编译。

164个初始输出目标中142成功/22拒绝，10条三步序列合计30原步骤（20后续成功），共162成功步骤；142成功初始目标的同银行输入上限减一全部原程序失败且完整回滚。25步出现bin实际减少多于收款目标的整数差额，按原SBF原样留在vault，未裁剪bin账本或增加recipient/协议输出费。0/1/119/120/121秒Oracle、环形回绕/扩容、零费率、fee封顶、filter/decay、active-bin容量边界与正负高价bin均有原程序依据。最后补强的验收工具重新执行全部初始及连续原银行和142个硬输入控制，并在普通/优化模式通过8组重哈希篡改反例；该补强在最终汇总中独立绑定，未用旧工具通过冒充新版本。

最终正常Rust finalized RPC/独立PG-Kafka/双Router采集43条实际API计划（21种银行双副本及重启后1条）。43条计划按计划内max_sell_amount在新LiteSVM运行原SwapExactOut2，真实付款、精确recipient到账、pair/array/Oracle完整字节及vault余额一致；43个实际计划输入减一控制均失败且无账本变化。9类来源故障撤销/恢复、新onlineGeneration及Router检查点-tail重启通过；撤销期间ExactOut旧计划不再报价，无关EVM范围仍可用。完整原SBF前后银行、失败输入控制、公开请求/计划与生产内核投影均保留在进程报告引用的本地数据中。

首轮进程的KAFKA_TAIL_DEADLINE及系统时间跳变日志/失败JSON按摘要另存；最终19项验证重新正常执行通过，没有沿用该失败运行或更改退出条件。资金/计划校验同时使用Solana大小写敏感公钥和u64上限；计划中的factory命名字段在Solana明确绑定Program ID，EVM字段序列化与原有语义由五组旧ExactOut回归验证。

本批完成固定MM-only单池路由的ExactOut注明计算业务。其他DLMM策略（OnlyY、奖励/订单、Token2022、bitmap扩展）、Pump、其他链与DEX组合、公开构建/原子执行/独立生产模拟、可信公开银行和净链费用仍待完成。报价继续为INDICATIVE、全市场最优不作保证；整体all_requested_features_complete=false、production_ready=false，原26工作包完整设计验收仍0。


<a id="plan-2026-11-11-dlmm-pump-combinations"></a>

## DLMM策略、Pump与链/DEX组合续作（2026-11-11）

对照设计§3/4/7.3/8/11.3/12.2/12.10/12.11/12.14/13/23。用户要求继续DLMM策略、Pump和其他链/DEX组合，本轮分别记录实现与证据，不把兼容性矩阵等同于全部目标链上线。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F51.1|DLMM OnlyY 独立策略版本、双向费用/原SBF与解析/HTTP|WP03,WP05,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|新profile meteora-dlmm-mm-only-y/1532e540-mm-only-y-1；236初始案例197成功/39拒绝，237成功步骤及双向ExactIn/ExactOut账本差分和197成功HTTP通过；33实际API计划/原SBF、16双Router银行、9来源故障及重启恢复通过|
|F51.2|Pump固定程序和IDL、native SOL/classic账户/费用/曲线解析|WP02,WP03,WP05,WP11,WP20,WP25|PARTIAL_LOCAL|固定官方SDK4.3.1、docs提交639b87fb及公开读取的Pump/Fee程序7a19affaa2c6/4f2955d0；166-byte curve/1088 global/4097费用配置与17账户闭包、字段36、原SBF真实Token/System CPI已执行|
|F51.3|Pump受限ExactIn/毕业撤销/单池路由/API及正常服务|WP09,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|classic/SOL卖出和固定SOL买入独立整数内核与完整费用账本；60原银行含毕业/失败与连续步，9成功HTTP初始目标通过；正常RPC-PG/Kafka/双Router的5实际计划通过Rust生产投影及原SBF完整账本回放，7故障、完成曲线撤销/源保留、重启通过|
|F51.4|其他链/DEX组合的可执行资产及隔离边界|WP09,WP12,WP13,WP18,WP20,WP25|PARTIAL_LOCAL|Pump原生SOL使用独立native:solana:sol节点；没有wrap模板不伪造与WSOL边；单池资金模板、跨链图拒绝与7个首批EVM chain-key数学复用测试通过；不是新部署或第二批链适配验收|
|F51.5|同源码验证、原始来源与最终进度绑定|WP03,WP25|PARTIAL_LOCAL|最终23检查/44 Rust执行通过；双Rust全工作区all-targets及指定Clippy、Go domain/build、原参考重执行、正常Pump及OnlyY共38计划回放；最终凭证及8次篡改控制通过|

OnlyY使用独立profile而不放宽InputOnly profile。卖X时费用从输出Y扣，卖Y时从输入Y扣；固定SBF的OnlyY ExactOut输出舍入差额计入协议Y费用，和阶段50 InputOnly保留vault差额不同。状态始终保留真实pair/array/Oracle和vault，不能套用SDK对其他版本的处理。

Pump profile `pump-bonding-sol-classic` / `pump-bonding/7a19affaa2c6-sol-1` 使用明确的原生SOL资产节点；不把native SOL等同WSOL。仅classic Token、SOL报价、非mayhem/cashback/holder-reward、无自定义creator费用、未毕业曲线、预存在137-byte user-volume账户、关闭跟踪且无奖励活动的单用户单池模板。费用表与买回拆分按原Fee程序CPI核对；交易手续费由不同测试付款人承担以独立测量用户真实交易支出。原始引用还保留精确token买入指令，但本轮服务只开启sell及buy_exact_sol_in的ExactIn接口，不宣称Pump独立ExactOut或迁移AMM已实现。完成曲线从候选图排除，虚拟/真实储备偏移、零实物Token和complete标志矛盾的状态拒绝。

Pump程序与费用程序来自公开finalized RPC的原ProgramData字节与单次账户观测，源码/账户均固定摘要；其后银行明确构造用户、curve、Token账户和费接收账户。原SBF执行能证明该受控模板的实际算术/账本，不证明可信历史银行、供应商独立性或生产资格。Pump ProgramData含约10MiB数据，正常RPC测试显式设置32MiB读取预算；不绕过既有响应限额。公共IDL/NPM来源只是额外参考，模型以固定原SBF实际输入/收款及完整后状态为有限验收依据。

尚需继续的范围：DLMM奖励/订单/host fee/Token2022/扩展bitmap，Pump非SOL报价/Token2022/现金返还/holder奖励/毕业迁移AMM和独立ExactOut，真实SOL-wrap及多程序复合执行、第二批Sui/Aptos/TRON等链原生适配，其他Curve/Balancer/V4等组合与净费用。all_requested_features_complete=false，production_ready=false，原26工作包完整验收仍0。

阶段51组合准入补强：发现两个Pump池可以共享creator/协议/买回费用账户，普通逐池独立余额会丢失共享状态依赖。新增共享账户原始观测一致性检查；未具备联合费用账本的多Pump池标记PUMP_SHARED_FUND_ACCOUNT_NOT_MODELED并保留完整源行，不能当作独立拆单流动性。共享观测矛盾返回PUMP_SHARED_ACCOUNT_CONFLICT。此前统一验证中断凭证保留；修正后的最终同源码检查重新运行。


阶段51最终有限验收：同一服务源码指纹 `f56bc7c4a3f2bbad4c2108aa1669bbe23a7541293fb9d8e04c7751dd54e8c50d`。`validate_family_expansion.py` 的23项检查全部通过，实际44次Rust测试执行；包含旧InputOnly/ExactOut回归、OnlyY/Pump解析/数学/API/路由、两个Rust工作区`--workspace --all-targets`、Parser相关及Router工作区/新测试Clippy `-D warnings`、公共protobuf生成核对/共享副本、Go domain测试及Go/Rust服务与Pump投影器构建。原参考在本轮重新执行：Pump 60初始目标/22成功步骤/44拒绝步骤；OnlyY 236初始目标/237成功步骤/39拒绝、107输入减一失败控制。

正常进程使用明确构造且固定原程序的银行、local finalized RPC、实际独立PostgreSQL/Kafka、两个Router；OnlyY 16原银行产生33实际ExactIn计划，9故障撤销/恢复与checkpoint-tail重启通过；Pump卖出及固定SOL输入买入产生5计划，经生产Rust投影与原SBF完整费用账户前后状态核对，7故障撤销/恢复、完成曲线源保留且双Router不报价、无关EVM范围仍可报价及重启通过。合计38实际计划原SBF回放。OnlyY正常进程没有运行ExactOut，其ExactOut权威仅为同源码原程序完整状态差分和HTTP矩阵；OnlyY正常计划的Python投影不得称作生产Rust模型投影。

`family_expansion_acceptance.py` 对最终源码、工具/输入/二进制摘要、验证日志、冻结进程凭证、完整银行前后状态、计划哈希/上下文/快照及真实用户原始账户收付逐项复核。4类篡改在普通及`-OO`模式共8次拒绝通过：费用账本、原账户状态、报告收款与真实余额冲突、重复银行替换缺失覆盖。统一入口为 `make -C integration family-expansion-current-check`。首次最终汇总遇到重复附加摘要导致macOS文件名过长，已缩短冻结文件名并保留失败日志，重新验收通过；服务源码/正常进程证据未变。

凭证：[最终有限验收](integration/artifacts/local/core-family-expansion-acceptance-20261111.json)、[23项同源码验证](integration/artifacts/local/core-family-expansion-validation-20261111.json)、[Pump正常进程](integration/artifacts/local/core-pump-bonding-normal-process.json)、[OnlyY正常进程](integration/artifacts/local/core-meteora-only-y-normal-process.json)。最终验收引用内容摘要命名的冻结副本，后续同名报告不能替代本次证据。阶段51完成以上有限范围；更广设计缺口仍按上列保留，`all_requested_features_complete=false`、`production_ready=false`，原26工作包完整设计验收仍0。

<a id="plan-2026-11-11-dlmm-extended-pump-exact-out"></a>

## DLMM扩展与Pump ExactOut/PumpSwap/毕业迁移续作（2026-11-11，阶段52）

对照设计§7.3/12.5/12.6/12.9/12.10/12.11/12.14/23。沿用阶段51源码并保留原有证据为历史版本；本轮服务修改后不得复用阶段51凭证声称当前源码验收。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F52.1|Pump独立ExactOut和曲线完成状态|WP09,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|按原buy指令输出驱动求SOL成本，独立于buy_exact_sol_in；原始18银行含毕业边界及7成功输入减一控制已生成；数学/HTTP及33项同源最终验证通过。卖出使用原sell最小输出约束；独立枚举双ceil费周期余数并反解曲线，逐候选证明最小输入，显式报告实际outputSurplus；不是反复ExactIn搜索。|
|F52.2|DLMM扩展bitmap的账户/跨范围路由与数学|WP03,WP05,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|独立bitmap profile、完整原始扩展账户及PDA/所有者/位序准入；构造重绑PDA/小步长跨正负默认边界的原SBF银行，56原案例/至少6实际跨数组边界、完整差分与9正常计划回放通过。|
|F52.3|DLMM奖励/订单/Token2022|WP03,WP05,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|双奖励槽、订单三层与X侧Token2022完整账本/实际收款/解析/HTTP通过，各9正常计划原SBF及故障/重启通过；未声明策略交叉能力。|
|F52.4|PumpSwap与毕业迁移依赖切换|WP02,WP03,WP05,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|发射曲线/迁移后canonical v1及boost独立状态，原migrate创建/重复幂等/3后续swap交接及真实余额上限控制通过；标准配置可解析正虚拟定价余额，未增加提款能力或自动发现。|

本阶段注明范围的本地验收已通过，all_requested_features_complete=false、production_ready=false，原26工作包完整设计验收仍0。


阶段52实现范围与参考复验（最终限定验收已通过）：

- DLMM独立profile：`meteora-dlmm-mm-bitmap`完整1576-byte扩展/PDA/正负位序，InputOnly/default+extension范围；`meteora-dlmm-mm-rewards`最多15个本次有流动性的bin、双奖励槽、结束时间封顶/逐bin分摊及完整增长；`meteora-dlmm-orders`InputOnly ExactIn的MM→processed→open三层、有序费用分摊、fulfilled计数/处理量/remaining/age滚转；`meteora-dlmm-token2022`仅X为固定Token2022 TransferFeeConfig/TransferFeeAmount、Y为classic/backedWSOL，双向ExactIn/ExactOut、older/newer epoch/0..10000bps/封顶/withheld及完整raw状态。各profile独立，不把它们的笛卡尔组合默认为支持；订单ExactOut、OnlyY与高级策略混合、Token2022-Y/任意hook/其他扩展、奖励领取/订单管理尚未开放。
- DLMM原程序共408初始银行、442成功步骤/110拒绝步骤，包含双奖励槽；Token2022使用固定原代码`495e9d7680dd555cb126a6a8e5464af5be9b01f02f2cd70634352722d22e3cad`，Clock epoch进状态/protobuf，Vault withheld与可用余额分别保存。MeteoraState在既有字段35的嵌套消息中新增字段20 bitmap、21 Token2022，保留原reserved 14..19；Go/Rust公共类型同步。
- Pump ExactOut买入由原buy直接指定token量，floor+1及协议/creator分别ceil；18银行7成功有原程序SOL硬上限减一控制；卖出20目标18成功且均有token输入减一失败控制，覆盖net(g)因双ceil在周期边界下降及9个实际输出盈余。反求有32候选修正预算，超出预算拒绝；实际盈余进入公开plan/allocation/API，不能截成目标值。发射曲线与PumpSwap仍只接受单池、声明用户一致的资金模板。
- PumpSwap独立`pump-amm-classic-wsol` / `pump-amm/6a7ad34596d8-classic-wsol-1`，固定公开取得的原代码`6a7ad34596d83b4c435b32f458a56e718e9f82eb4ad38e4316616d15d24d8ff7`和既有Fee代码；复用字段36的程序/费用/完整账户载体，但profile、adapter、program/PDA、287-byte Pool/949-byte Global/12账户及19依赖闭包均单独严格验证。classic X/backedWSOL Y、canonical迁移PDA、v1 sell/buy_exact_quote_in ExactIn和buy ExactOut。27初始案例18成功，22成功步骤含连续账本；v2累积费桶、mayhem/cashback/holder/custom creator fee/非WSOL和池间联合费用账本仍拒绝。
- 新`pump-bonding-sol-migration`版本区别迁移专用token库存与真实可交易储备；完整状态验证包括虚拟/真实偏移、complete标志、迁移后虚拟储备清零及零vault。旧profile仍保持原无额外库存银行准入。真实原migrate已执行Pump→PumpSwap→Token/Token2022/System/ATA CPI，原生费用、boost、LP创建均由原程序处理；再次执行为无状态变化的成功。3类后续swap直接使用该次创建的真实Pool及两vault，逐字节交接检查通过。canonical目标解析只返回身份，报价必须等待源银行实际具备目标全部依赖；没有自动发现/EnsurePoolScope或公开迁移交易构建的验收声明，没有SOL↔WSOL隐式边。

原始复验与篡改检查：参考复验（`integration/artifacts/local/solana-expanded-reference-reexecution-20261111.log`，已清理）、参考控制（`integration/artifacts/local/solana-expanded-reference-controls-20261111.log`，已清理）。统一入口`validate_solana_expanded.py`已完成两Rust工作区全目标、Clippy、旧策略回归、5类正常RPC-PG/Kafka/双Router及实际计划回放；当前结果见本节最终凭证。all_requested_features_complete=false、production_ready=false，原26工作包完整设计验收仍0。

阶段52最终校验发现并修正两个迁移边界：已迁移曲线的四项储备全部清零，不能继续强制迁移前偏移；原migrate还将boost对应余额写入AMM的virtual_quote_reserves，定价/费用档使用real+virtual，扣款仍受真实vault约束。新增严格boost策略版本及原程序InsufficientRealQuoteReserves反例、canonical版本解析正虚拟余额、混合Curve SOL钱包/AMM WSOL ATA不误判共享余额的回归。完成曲线保留但不阻断目标AMM，普通多活跃Pump共享资金账户仍排除。此前失败的同源验证保留，修正后重新验证。

阶段52位图覆盖补强：正常进程预检发现原正侧ExactIn成功案例不足四组，已补充5000输入与150000输出目标；bitmap原程序矩阵扩为56案例34成功/22拒绝，包含正负侧实际跨default/extension边界成功成交；数学差分显式检查至少6次跨数组边界，正常进程选择双向1000/5000输入四组，不以失败的大额交易代替跨界成交。此前运行凭证保留为中断/预检，当前源码重新验证。


阶段52最终同源码限定验收（2026-11-11）：源码指纹 `253d2fbdee7a39572dfe2e58c18ebb4453ea1226a5d87091b215e59f3c84cfe8`。33项检查全部通过，实际56次Rust测试执行；覆盖全部新解析/完整数学/HTTP/路由/优化器及旧InputOnly/OnlyY/ExactOut回归、两个Rust工作区`--workspace --all-targets`、相关Clippy `-D warnings`、Go domain及构建、公共类型生成和共享源码/fixture核对。参考重执行与4类篡改控制在普通/`-OO`模式共8次拒绝通过。此前文档索引、迁移终态/boost及正常工具路径/跨界样本补强的失败或中断凭证已保留，未被计为本次成功检查。

最终构建的bitmap、rewards、orders、Token2022和PumpSwap各4原银行×双Router+1重启计划，共45份真实API计划。正常finalized RPC、实际隔离PostgreSQL/Kafka、两个Router及不相关Go EVM范围都已启动验证；每类程序版本/Clock两种来源故障撤销、恢复及checkpoint-tail重启通过，合计10组故障/5次重启。45计划均经生产Rust `solana-plan-projector`重算，再执行固定原SBF；完整原始前后银行、全部模型后状态、实际用户Token收付、mint/withheld守恒及WSOL backing核对通过。正常进程只运行ExactIn，负侧bitmap边界与高级ExactOut由原程序/解析/数学/HTTP验证；不能把这些正常进程结果写成正常ExactOut执行验收。

迁移另由真实原migrate CPI/重复幂等/3类创建池后续swap及原程序InsufficientRealQuoteReserves反例验证。完成/清零曲线保留源行并撤销路由；真实AMM账户进入观测银行后才开放边。Curve SOL钱包和AMM WSOL ATA分开认定可变资金账户，完成曲线不误阻断目标市场；共享活跃实际资金账户的多池仍排除。没有可信公共历史银行、自动发现/EnsurePoolScope、隐式SOL包装或公开交易执行的资格声明。

凭证：[最终限定验收](integration/artifacts/local/core-solana-expanded-acceptance-20261111.json)、[33项当前源码验证](integration/artifacts/local/core-solana-expanded-validation-20261111.json)。最终验收固定所有检查日志和五类进程凭证/输入/二进制/工具摘要，引用内容摘要命名的冻结副本；同名后来报告不能替代本次证据。统一复验入口 `make -C integration solana-expanded-current-check`。

仍未开放：独立策略的全部交叉组合、OnlyY高级组合、订单ExactOut及订单放置/取消/奖励领取、Token2022-Y/任意hook或其他扩展、Pump/PumpSwap非classic资产/非SOL或非WSOL/v2费用桶及高级creator/cashback/holder/mayhem策略、动态用户账户创建、共享多池联合账本、迁移自动发现/公开构建与执行、真实wrap复合执行、净链费用与可信来源资格。对应设计工作包仍PARTIAL_LOCAL；`all_requested_features_complete=false`、`production_ready=false`，原26工作包完整设计验收仍0。


<a id="plan-2026-11-11-dlmm-orders-exact-out"></a>

## DLMM订单独立ExactOut与完整业务链路（2026-11-11，阶段53）

对照设计§7.3/12.5/12.9/12.10/12.11/12.14/23，补充阶段52明确保留的订单ExactOut缺口。阶段52凭证继续作为历史限定验收；本轮修改服务源码和fixture后，不以旧凭证证明当前源码。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F53.1|订单ExactOut输出驱动数学与完整账本|WP11,WP12,WP20,WP25|PARTIAL_LOCAL|192固定原始SBF银行，193成功步骤/31拒绝；部分bin按总输出反推一次，耗尽bin按各层全填计算，重放MM/processed/open及原手续费、fee growth、订单fulfilled/remaining/age、vault和oracle；完整后状态逐字节差分通过。|
|F53.2|订单ExactOut账户解析与准入|WP03,WP05,WP11,WP25|PARTIAL_LOCAL|复用orders独立profile，不修改公共字段；新增独立fixture银行解析并保留program/owner/layout与旧profile冒名拒绝；192订单ExactOut银行及原MM解析/拒绝回归通过。|
|F53.3|路由、共享池分配与API计划|WP12,WP13,WP20,WP25|PARTIAL_LOCAL|ExactOut数学/单池路由/计划白名单一致接入；完整路由重放、失败原子回滚和内部账户/复合路径拒绝通过；16组三步有序共享池分配/过期候选重定价、全部目标HTTP/预算反例及重哈希篡改/过期视图撤销通过。|
|F53.4|原程序反例和同源码正常服务验证|WP03,WP05,WP13,WP20,WP25|PARTIAL_LOCAL|161成功原银行均有最大输入减一失败控制；原交易回滚、用户Token实收实付、Token守恒/WSOL backing和参考篡改控制；正常RPC/PG/Kafka/双Router、四银行×双Router+重启共9计划的生产投影/原SBF回放、2种来源撤销恢复及1次重启通过。|
|F53.5|进度记录与剩余范围|WP20,WP25|PARTIAL_LOCAL|保留SDK与实际固定SBF不一致的发现和失败试验凭证；统一入口`make -C integration meteora-orders-exact-out-current-check`；整体剩余需求未全部完成。|

实际SBF SHA256固定为`942f609d03440911f15c40e8df1733d808670768fd9042af1de956964fb2f0e6`，SDK固定commit为`1532e540db41ac405dbf4c5bb2ddfceaa1f98ff1`。SDK的各层独立反解不能直接作为部署程序数学：实际部分bin先按总目标输出反解，然后依次消耗MM、processed、open；全bin耗尽保留各层输入向上舍入之和。跨层输入损失导致欠付时原程序返回6085，报价同样失败；不能向上补输入后声称原指令ExactOut可执行。成功交易中bin物理扣减可大于目标到账，完整原始账本保留差额，不把vault dust改成用户收入或协议费用。

本轮为192银行保留独立`meteora-orders-exact-out.json`和原始SBF证据，不重写阶段52的orders ExactIn矩阵。包括双向、三层空/非空组合、层边界前后、超额不可成交、连续1000→100→1000目标序列；原程序初始161成功、31拒绝，共193成功步骤，161输入减一控制均失败且银行不变。首次差分暴露的“SDK逐层反解多报价”和“将所有订单舍入盈余拒绝”的错误假设已修正，对应失败日志保留。

阶段53最终同源码限定验收（2026-11-11）：源码指纹`5f54b5c66c2a93b7cc1247b7550f35fc7acf6434c40def45a22c708c4c176b25`，28项检查全部通过，实际34次Rust测试执行。包括两个Rust工作区全目标check、相关Clippy `-D warnings`、Go domain/构建、公共类型/301份合同复制检查、原SBF重新执行、普通/`-OO`各5项参考/篡改控制、进度文档两模式各17项测试。正常服务实际启动隔离PostgreSQL/Kafka、Rust Parser、双Router和不相关Go EVM范围，四原银行双向1000/6000输出目标×双Router加重启共9份ExactOut API计划；9份均由生产ExactOut投影器重算并在新LiteSVM执行原SBF，完整后状态与真实收付一致。9计划均检查输入预算减一拒绝，program代码/Clock-slot两种故障撤销和恢复及Router checkpoint-tail重启通过。最终报告固定检查日志摘要、工具/原始输入摘要及内容摘要命名的正常进程冻结凭证；前期失败凭证保留。此阶段正常进程确实执行ExactOut，与阶段52仅ExactIn的历史结果分开。`all_requested_features_complete=false`、`production_ready=false`，原26工作包完整设计验收仍0。仍缺订单放置/取消、奖励领取、DLMM高级策略全部交叉/OnlyY组合、Token2022-Y/任意hook、Pump/PumpSwap其他资产与高级费用策略、动态用户账户、共享多池联合账本、自动迁移发现/EnsurePoolScope、公开构建/执行与真实wrap、净链成本和可信公共历史银行，其他第二批链适配器也未完成。


<a id="plan-2026-11-12-frontend-quote-playground"></a>

## 前端链/币种选择与实时兑换量联调（2026-11-12，阶段54）

本批对应设计§3/7/8/12/13/23，目标是给现有解析器和`dex-router`增加可直接联调的前端入口。页面不建立第二套兑换数学，所有兑换量、路线、共享状态、ExactIn/ExactOut舍入和计划数据均来自Router生产内核。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F54.1|当前解析状态元数据|从Router当前ReplicaView聚合chain、ready、Token、Pool、protocol、adapterVersion；响应只暴露联调所需的标识和版本|LOCAL_SCOPE_VERIFIED|`/v1/quote-options`元数据HTTP测试通过；无可用状态时返回未就绪链，不伪造Token或池|
|F54.2|ExactIn/ExactOut实时报价页|链、卖出Token、买入Token、报价类型、金额、最大卖出、caller、recipient、滑点输入；调用`/v1/quote`或`/v1/quote-exact-out`并展示输入/输出、路线、快照、图版本、计划哈希和错误|LOCAL_SCOPE_VERIFIED|浏览器静态资源通过Node语法检查和Python静态服务器HTTP检查；前端不复制任何协议数学|
|F54.3|跨源联调和安全边界|Router加入CORS响应层，前端可从独立静态服务器请求；API token支持Bearer输入并本地保存，交易构建/发送不在联调页开放|PARTIAL_LOCAL|Router quote-api locked check、元数据HTTP测试、前端HTML/JS资源获取通过；生产CORS来源白名单和完整部署策略仍需配置化|
|F54.4|剩余解析/路由功能现状|继续使用已完成的各链/DEX parser、pool validation、route search、optimizer和ExactOut计划；联调页覆盖当前已发布池，未知链/池由Router返回错误|PARTIAL_LOCAL|本批未将历史限定验收改写成全链/全DEX完成；阶段52/53的原程序差分和进度凭证继续作为历史范围证据|

联调步骤：先启动`dex-router`并保证解析器已发布可报价ReplicaView；再执行`cd frontend && npm run dev`，浏览器访问`http://127.0.0.1:5173`。可通过`?api=`切换Router地址；需要认证时填写API Token。页面默认调用`/v1/quote-options`，选择链和Token后调用ExactIn/ExactOut接口。错误会保留Router原始错误消息，方便定位解析状态、路由候选或金额约束问题。

本批源码和静态资源验证记录见`integration/artifacts/local/phase54-frontend-quote-playground.json`。整体`all_requested_features_complete=false`、`production_ready=false`保持不变；仍需完成设计文档列出的全链/全DEX来源覆盖、其余策略交叉、可信历史bank、生产来源隔离、动态账户/交易构建执行、CORS配置化以及真实链容量和安全验收。


<a id="plan-2026-11-12-frontend-live-quote"></a>

## 前端状态刷新与路由约束联调（2026-11-12，阶段55）

阶段55继续对照设计§3/7/8/12/13/23，补齐前端与实时解析状态之间的联调反馈。前端不另行实现兑换公式，所有结果仍由Router的解析快照、图、路由搜索、优化器和ExactOut计划生成。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F55.1|解析状态可观测|`/v1/quote-options`增加viewId、graphVersion、snapshotRef、poolCount和protocols；前端展示当前链对应版本|LOCAL_SCOPE_VERIFIED|quote-options HTTP测试验证ready、view、graph、pool和protocol字段；locked quote-api check通过|
|F55.2|实时兑换量联调|前端10秒刷新状态；链/Token/金额/账户/滑点/maxHops变化后500ms自动请求；新请求Abort旧请求，避免过期结果覆盖最新输入|PARTIAL_LOCAL|Node语法检查、静态服务器资源检查和现有3项quote-api扩展HTTP回归已通过；浏览器真实服务仍需有可报价ReplicaView|
|F55.3|路由约束可视化|新增最大跳数输入并写入QuoteRequest.constraints，展示实际返回路线、协议、Token序列、输入和输出|PARTIAL_LOCAL|请求构造与后端已有约束校验复用；当前测试覆盖单池/既有协议，跨池生产图需继续扩展|

本批固定验证记录见`integration/artifacts/local/phase55-frontend-live-quote.json`。`all_requested_features_complete=false`、`production_ready=false`保持不变；剩余全链/全DEX解析、未覆盖策略交叉、可信生产来源、动态账户、公开构建/执行和真实链容量安全验收仍需继续。


<a id="plan-2026-11-12-protocol-capabilities"></a>

## 协议报价能力与执行能力边界联调（2026-11-12，阶段56）

阶段56开始按“协议解析/数学能力 → 动态账户预检查 → unsigned交易计划 → 独立模拟 → 下单执行”的顺序推进。第一步不提前开放交易执行，而是把每个解析Pool实际可用的报价模式和执行边界显式传给前端。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F56.1|协议报价能力来源|Router使用`xone-protocol-math::exact_out::supports_protocol`生成Pool的`quoteModes`；ExactIn沿当前可报价池开放|LOCAL_SCOPE_VERIFIED|quote-api locked check和quote-options HTTP测试通过，ExactOut能力不由前端硬编码|
|F56.2|执行能力边界|Pool元数据增加`execution.quote=true`、`unsignedPlan=false`、`simulation=false`；前端展示模式和执行状态|PARTIAL_LOCAL|静态前端资源和JS语法检查通过；后续动态账户/unsigned/simulation按序实现|
|F56.3|前端联调反馈|池列表显示报价模式，用户可以看到当前协议是否支持ExactOut，避免把报价接口误认为下单接口|PARTIAL_LOCAL|Python静态服务器资源获取通过；真实ready ReplicaView联调继续沿用阶段54/55流程|

本批仍保持`all_requested_features_complete=false`和`production_ready=false`。下一步按设计顺序实现动态账户预检查，先覆盖只读账户存在性、owner/program、Token账户和所需协议账户列表，不直接创建或发送交易。


<a id="plan-2026-11-12-quote-execution"></a>

## PumpSwap卖出目标、执行账户准备与真实浏览器联调（2026-11-12，阶段57）

按已确认顺序推进设计§7.3/12.5/12.6/12.9–12.14/14/15/28.16：先补协议计算缺口，再做只读执行账户准备，接着生成待签名消息，并用实际下载消息在原程序本地执行。原有解析owner和Router分工保留，报价仍绑定同一QuoteView。新增前端不是新的核心计算服务。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F57.1|PumpSwap卖出指定输出目标|WP03,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|对三项分别ceil手续费按10000周期枚举余数，独立反解CP最小输入，再用前向内核结算；48受控原程序目标含42成功/6无效目标和42原程序输入减一失败，14实际额外到账；数学差分、路由/优化/API/计划同源回归通过。|
|F57.2|Meteora动态执行账户只读核对|WP05,WP09,WP14,WP16,WP25|PARTIAL_LOCAL|prepare按quoteId/planHash/QuoteView及过期约束；RPC仅取配置，核对genesis、finalized/minContextSlot、Clock、Pool/Oracle/bin/bitmap/Token2022字节及代码版本；派生ATA或接受指定已存在Token账户，核对mint/owner/frozen/balance/WSOL backing/payer费用；缺失账户返回blockers，不自动创建。|
|F57.3|单池待签名Solana消息|WP14,WP15,WP16,WP25|PARTIAL_LOCAL|execution-plan生成legacy message和零签名槽transaction，账户权限合并、program/指令顺序、recent blockhash/有效块高、fee quote、1232-byte大小限制；前端可下载JSON；保持signed=false/submitted=false/simulationRequired=true。实际输出字节在本地签名后运行原SBF并核对699输入/1000到账、bin/订单/oracle后状态和输入减一失败。|
|F57.4|前端真实联调和状态撤销|WP09,WP12,WP16,WP25|PARTIAL_LOCAL|修复Router实际snake_case/nested path展示、原始大整数、重复认证change与旧metadata竞态、旧quote/preparation撤销；图中排除池不再声明可报价；会话凭据按origin存放；Chrome真实API、缺失ATA/冻结账户/新view/撤销恢复、延迟响应竞争、自动报价和移动布局验证通过。|
|F57.5|CORS配置与证据可靠性|WP01,WP16,WP25|PARTIAL_LOCAL|api.cors_allowed_origins三服务配置兼容；Rust共享config逐字节一致；默认仅本地5173，指定origin和未知origin浏览器测试；统一入口固定源码/前端/辅助工具/原程序输入/检查日志/浏览器产物摘要，不将静态资源HTTP200算作浏览器业务通过。|

阶段54–56的资源HTTP/Node语法检查仅是静态检查，不是完整浏览器业务联调。阶段56原始locked check因依赖增补缺少锁更新而失败，随后仅offline/unlocked check和单项元数据测试通过；当时摘要中的“locked check通过”不准确。本批保留旧凭证并用更新锁后的统一locked检查纠正，不复用旧成功摘要证明当前源码。

PumpSwap卖出目标按至少实际到账处理：三ceil fee导致净输出不保证单调，不能直接二分ExactIn；按周期候选反推CP输入并验证后状态，保留显式outputSurplus与target_output，计划校验同步允许这个注明卖出方向。数学不改变已解析状态结构，normal和boost仍各自验证，测试高价格银行明确为受控配置且WSOL余额有lamport backing。

执行准备当前仅为已支持的Meteora单池模板，prepare返回“ready”仅表示账户检查与待签名消息已完成，不等于链上执行成功或生产VERIFIED；调用只读RPC，不签名/广播。当前不自动创建ATA、wrap/unwrap、签名钱包、提交交易或生产独立模拟。已有公开build/VERIFIED准入不由prepare开放。原SBF执行是隔离本地测试，测试密钥不用于用户账户。

阶段57最终同源码有限验收（2026-11-12）：源码指纹`282e47e86c62d1936e3b184683a479be518346d1c5b497e429c27054aaa98fc3`，24项检查全部通过，实际24次Rust测试执行，13项真实Chrome业务联调通过。包括共享配置/公共协议/303份合同副本、旧数学/路由/优化器/API回归、准备与CORS反例、相关Clippy `-D warnings`、Go配置和Rust解析配置、原SBF重执行、普通/`-OO`参考篡改控制、前端数值/路线单元测试与文档双模式检查。浏览器在明确测试harness中调用生产Api handler和数学，不stub报价，使用受控固定原银行；normal Parser→PG/Kafka来源流水线不在本批新增范围声明内。下载的真实unsigned transaction由solders解析，使用确定性测试密钥本地签名执行固定SBF，699输入/1000到账、完整bin/订单/oracle和输入减一失败核对通过；测试显式关闭固定测试blockhash的过期检查，不把控制银行当作公网历史资格。测试harness/浏览器静态服务器已结束。原始失败日志与阶段56证据更正保留，最终报告绑定源码/辅助工具/日志/浏览器截图/真实下载文件与本地原执行摘要。原26工作包完整设计验收仍0，`all_requested_features_complete=false`、`production_ready=false`。后续顺序：缺失账户创建与wrap模板、其他协议unsigned构建、生产独立模拟、钱包签名/广播/确认/回执及真实链来源资格；并行于这些执行项的其余链/DEX解析与策略组合缺口继续保留，不能被新增前端或单池消息代替。


<a id="plan-2026-11-12-ata-simulation"></a>

## 收款ATA创建、只读模拟与前端闭环（2026-11-12，阶段58）

按阶段57保留的下一步顺序，对照设计§12.14/14/15/28.16推进动态账户准备与模拟。新增功能使用同一quoteId、planHash和QuoteView，不把前端或RPC响应当作新的兑换量数学来源。当前不签名或广播公网交易。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F58.1|规范标准收款ATA创建|WP14,WP16,WP25|PARTIAL_LOCAL|prepare新增可选createDestinationAta；仅在收款账户缺失且地址为recipient/mint/Token program派生ATA时允许，固定Associated Token Program代码核对，加入CreateIdempotent指令；源账户仍必须已有，Token2022收款创建另行拒绝。|
|F58.2|多指令与费用资金约束|WP14,WP25|PARTIAL_LOCAL|execution-plan统一编译多指令legacy message，保留账户权限/单签名/包大小限制；getMinimumBalanceForRentExemption与getFeeForMessage合并payer资金要求，分别报告rent/fee/total；租金不足不返回交易。|
|F58.3|只读实际交易模拟|WP05,WP09,WP15,WP16,WP25|PARTIAL_LOCAL|prepare可选simulate调用配置RPC的simulateTransaction，固定原blockhash、sigVerify=false/replaceRecentBlockhash=false；context slot、err、账户布局/WSOL backing及实际输入输出均核对同一报价，金额不一致拒绝；独立生产可信模拟资格未开放。|
|F58.4|前端创建/模拟选项及异步失效|WP09,WP16,WP25|PARTIAL_LOCAL|新增创建标准ATA和只读模拟选项、显示租金/fee/模拟到账并下载完整多指令待签名交易；独立prepareSeq防止修改准备参数后旧结果恢复下载；17项Chrome真实业务复验通过。|
|F58.5|原程序、租金、回滚和证据|WP03,WP14,WP15,WP25|PARTIAL_LOCAL|固定原SBF运行浏览器下载字节，699输入/1000到账，2039280租金/2044280总费用，Token守恒及完整bin/订单/oracle；输入上限减一失败时新ATA一并回滚；重哈希改地址/指令/费用仍拒绝，普通及-OO各3项重哈希篡改控制通过。|

模拟测试RPC实际调用LiteSVM的simulate_transaction执行固定原程序，核对源银行不变；不是预设成功回包。原始执行参考另对下载交易以确定性测试密钥本地签名执行，核对真实payer租金/费用扣款、创建WSOL账户backing、实际Token收付与后状态。原blockhash仅在隔离参考中显式跳过过期检查，不据此声称公网历史来源或生产模拟资格。程序/账户版本、当前QuoteView和报价过期检查仍贯穿准备开始与返回。

本批普通模拟结果为RPC_SIMULATED，independentProductionVerification=false、signed=false、submitted=false；公开VERIFIED/build准入不因勾选模拟而改变。阶段58最终同源码有限验收（2026-11-12）：源码指纹`e8ef710a9a2594b02be813d211cd929aed23bcbf2202fbd79cbdb7db8920c517`，17项检查、8次Rust测试执行、17项真实Chrome业务联调全部通过。304份合同/工具副本、公共协议、严格Clippy、既有单指令准备与DLMM扩展报价回归通过。浏览器实际检查/创建/模拟/下载，租金不足、模拟金额不符、冻结账户、准备参数变化晚返回拒绝、新QuoteView/撤权恢复、ExactIn/ExactOut及移动布局通过；下载字节在本地原SBF执行并逐项核对。原始失败日志保留。所有harness/静态服务器已结束，未向公网提交交易。最终凭证绑定源码、运行时/工具、检查日志及浏览器下载/截图摘要。仍缺SOL/WSOL包装模板、源账户资金准备、Token2022收款创建、多协议unsigned构建、生产独立模拟/来源、签名广播确认/回执与其他链/DEX解析组合。`all_requested_features_complete=false`、`production_ready=false`，原26工作包完整设计验收仍0。


<a id="plan-2026-11-12-native-funding"></a>

## 输入WSOL补资与原子包装兑换联调（2026-11-12，阶段59）

按阶段58下一步顺序，对照设计§12.9/12.14/14/15/28.16扩展执行准备。SOL原生余额与WSOL仍为不同报价节点；本批只在明确以WSOL卖出的单池报价后，通过prepare加入包装资金指令，不在数学图里补造隐式边。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F59.1|显式输入WSOL资金准备|WP14,WP16,WP25|PARTIAL_LOCAL|wrapSourceSol可选标记要求输入mint为WSOL且Token program为经典SPL；校验现有owner/mint/native backing，按执行计划requiredInput（ExactOut为slippage后的计划上限）与现有余额差额补资，非WSOL拒绝。|
|F59.2|缺失输入ATA与原子包装指令|WP14,WP15,WP25|PARTIAL_LOCAL|仅为caller规范WSOL ATA创建；按create/input transfer/SyncNative/可选收款ATA/swap顺序编译单笔unsigned message；输入已有时不创建，不会重复包装全部金额。|
|F59.3|完整SOL资金及模拟收付|WP05,WP09,WP15,WP25|PARTIAL_LOCAL|payer余额需覆盖包装金额+所有新建账户租金+网络费；模拟以包装后资金为起点核对实际swap输入/输出；ExactOut未用部分保留WSOL并显式展示expectedResidualWsol/sourceWsolRetained。|
|F59.4|原程序反向银行与失败回滚|WP03,WP14,WP15,WP25|PARTIAL_LOCAL|固定原DLMM订单反向1000目标银行，规范输入ATA初始缺失；原程序参考核对包装1472、swap消耗1464、到账1000、剩余8及SOL费用，完整后状态和输入上限减一失败回滚验证通过。|
|F59.5|前端联调与历史回归|WP09,WP16,WP25|PARTIAL_LOCAL|前端增加显式WSOL补资选项并按实际执行能力启用，显示包装/剩余余额/租金/fee，选项变化撤销旧下载；真实Chrome下载+原程序执行、现有ATA创建/报价同源回归通过。|

输入WSOL账户不自动关闭，也不把ExactOut剩余资金当作SOL退款。初轮测试纠正了把请求总预算当作实际补资上限的假设：预算2000/目标1000/50bps滑点对应计划最大输入1472，实际消耗1464，残余8；未用预算536由未包装的528和残余WSOL8组成，不能把536全部描述为已包装余额。失败日志保留。仅补差额，已有足额WSOL时不增加转账/SyncNative。缺失source只能是规范ATA，非WSOL调用拒绝；现有账户必须owner匹配，不为其他用户或任意mint补资。wrapSourceSol未选中时保持阶段58缺失/不足输入账户的明确阻塞行为。缺失收款ATA选项可与输入WSOL创建组合，总租金按实际创建个数累加；消息仍受账户权限/签名/1232byte约束。

阶段59首轮同源码有限验收（2026-11-12，证据保存方式更正前）：源码指纹`c632df0a687a1a3c7f603ac4ac0ffbddeefdf943d49f356cf9bff17f49d8fc71`，20项检查、9次Rust测试执行、20项真实Chrome联调全部通过。304份合同/工具副本和11份fixture副本、公共协议、严格Clippy、既有单/多指令准备和DLMM报价回归、原SBF模拟与真实下载wire执行、普通/-OO各4项wrap篡改控制及各3项收款ATA控制、前端unit/syntax和进度文档双模式检查均通过。原始反向SBF银行、缺失输入ATA/余额100/余额3000、ExactIn/ExactOut、输入/收款ATA同时创建及不合理输入反例都覆盖；浏览器明确选择补资后才允许准备，切换选项撤销旧下载，原程序再次核对wrap1472/实际输入1464/输出1000/余额8、租金2039280/总支出2045752、完整账本与失败回滚。所有harness/静态服务器已退出，最终凭证固定源码/工具/日志及浏览器截图/下载文件摘要；normal Parser→PG/Kafka和生产来源资格不在本批新增验收声明内。所有测试只在受控银行/测试harness和固定原SBF本地运行，不广播公网。仍缺unwrap/账户关闭模板、Token2022创建、其他协议unsigned构建、钱包签名/广播/确认结算、独立生产来源/模拟及其余链/DEX解析组合；`all_requested_features_complete=false`、`production_ready=false`，原26工作包完整设计验收仍0。


阶段59证据保存更正：最终复核发现旧收款ATA参考工具沿用`phase58-ata-original.json`，本次复验重写后原始字节摘要发生变化，导致阶段58报告中的原始执行文件摘要不再匹配；旧文件字节未单独冻结，无法再核验其差异仅来自键序；实际下载交易字节仍与阶段58完全相同，业务重执行结果通过；这不能代替对旧原始报告字节的核验。旧验收原文件已另存并将活动记录标记`HISTORICAL_ARTIFACT_REPLACED`，不再用它作当前源码证据。阶段59改为每次独立目录保存wrap/ATA/已有账户三类原执行记录并固定摘要，原生参考捕获键序也固定排序；修正后重新统一复验。阶段58“已通过”属于当时的历史结论，不代表旧原始报告文件仍完整可取。

阶段59证据保存修正后最终同源码有限验收（2026-11-12）：源码`a767377942c5eed4079d545a85ce97e6f0f7b81b7c4e69989d2ca96166562bae`，20项检查、9次Rust测试执行、20项真实Chrome业务联调全部通过。每次独立目录固定三类原执行、真实下载wire、浏览器截图和日志摘要；新运行不再写入阶段58原始报告。最终输入WSOL补资/原子swap收付与全部反例仍通过，未广播公网，测试服务已结束。


<a id="plan-2026-11-12-temporary-wsol"></a>

## 临时WSOL解包与余款回收（2026-11-12，阶段60）

按阶段59后续顺序，对照设计§12.9/12.14/14/15/28.16补充显式结算动作。报价继续使用WSOL节点；只有prepare明确选择后才添加SOL资金指令，金额计算、最优路由和解析服务职责保持原设计。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F60.1|单签名临时WSOL账户模板|WP14,WP15,WP25|PARTIAL_LOCAL|以caller、planHash和输入/输出角色确定seed，System CreateAccountWithSeed后InitializeAccount3；官方编码比对和地址占用拒绝。|
|F60.2|输出解包与输入余款退回|WP14,WP16,WP25|PARTIAL_LOCAL|unwrapDestinationSol要求WSOL输出、caller等于recipient且未指定目标账户；refundWrappedSource要求显式wrapSourceSol且未指定源账户；只关闭本次新建账户。|
|F60.3|原子资金与模拟收付核对|WP05,WP09,WP15,WP25|PARTIAL_LOCAL|交易前余额仍需覆盖包装+租金+fee；关闭临时账户后以payer SOL差额扣除fee/持久租金还原真实兑换额，要求临时账户消失，金额偏差拒绝。|
|F60.4|真实下载wire与原SBF回滚|WP03,WP14,WP15,WP25|PARTIAL_LOCAL|浏览器实际下载两种交易，独立核对seed/权限/close目标/官方指令，执行原SBF检查既有WSOL不变、真实SOL收付和完整池后状态；输入上限减一必须回滚。|
|F60.5|前端选项与历史回归|WP09,WP16,WP25|PARTIAL_LOCAL|前端按币种/账户/recipient启用选项，展示退租及payer余额变化；选项变化撤销旧下载；保留阶段58/59流程回归。|

输入示例：请求预算2000、目标1000、滑点50bps，执行计划最大输入1472、真实消耗1464。临时账户关闭转回2039288 lamports，其中租金2039280、未用输入8；payer实际SOL净变动-6464（1464兑换成本+5000网络费）。用户已有3000 WSOL的规范ATA和原输入账户均需保持原字节。

输出示例：SPL输入699、WSOL报价输出1000，经显式解包实际收到1000 lamports SOL；关闭账户同时退租2039280，合计关闭转回2040280，payer净变动-4000（1000收款减5000网络费）。退租不计兑换收益。账户已被占用、显式账户覆盖、收款人与caller不同、未关闭或native差额不符均拒绝。ExactIn和独立ExactOut使用相同模板。

复验入口：make -C integration temporary-wsol-current-check。每次运行独立保存真实浏览器下载、只读模拟、五类原程序执行记录和日志，保留失败记录且不覆盖历史凭证。最终同源码有限验收：源码`bcda23963eea12acd05140b71397c1b15dbd281087508f384436170c87228869`，24项检查、12次Rust测试执行、24项真实Chrome业务联调全部通过。304份合同/工具副本、公共协议、严格Clippy与前端模型/桌面及移动布局检查通过。浏览器实际下载的两类临时账户交易和既有账户、收款ATA、普通包装三类交易均在原SBF执行；精确收付、完整池后状态、既有3000 WSOL不变、金额/close篡改和失败回滚通过。文案复核后重新在最终源码复验，旧报告独立保留；harness和静态服务器均已退出，未提交公网交易。仍保留多协议unsigned、Token2022账户创建、钱包签名广播/确认、生产独立来源/模拟及其他链/DEX缺口；all_requested_features_complete=false、production_ready=false。


<a id="plan-2026-11-13-token2022-preparation"></a>

## Token2022收款ATA与转账费联调（2026-11-13，阶段61）

按阶段60剩余项，对照设计§12.8/12.9/12.13/12.14/14/28.16继续Meteora单池prepare。沿用现有固定Token2022 TransferFeeConfig数学/解析，不引入未知扩展或额外服务；quote实际输出始终为转账费后可用余额。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F61.1|Token2022规范收款ATA创建|WP14,WP16,WP25|PARTIAL_LOCAL|CreateIdempotent按recipient/mint/实际Token程序派生，指令传入Token2022程序；只允许已验证版本及TransferFeeAmount+ImmutableOwner。|
|F61.2|扩展长度与逐账户租金|WP14,WP15,WP25|PARTIAL_LOCAL|经典165-byte账户与Token2022扩展182-byte ATA分别查询租金；混合临时WSOL/收款ATA按实际长度合计，临时退租与持久收款租金分开。|
|F61.3|净到账、epoch与withheld核对|WP05,WP09,WP15,WP25|PARTIAL_LOCAL|固定完整mint、程序、epoch和vault原字节；模拟同时核对用户净到账与目标withheld增长，源withheld不得变化；未知/重复/截断/错误type/冻结账户拒绝。|
|F61.4|原SBF矩阵与失败回滚|WP03,WP14,WP15,WP25|PARTIAL_LOCAL|五个原银行覆盖双向、epoch前后、费用cap及100%费率；实际wire执行与输入减一回滚，另覆盖WSOL退款+Token2022创建组合；原余额+withheld守恒和完整池后状态核对。|
|F61.5|前端真实下载联调及回归|WP09,WP16,WP25|PARTIAL_LOCAL|前端显示Token2022转账费及账户字节数；真实Chrome检查普通创建/撤销/退款组合下载；保留阶段58–60的已有账户、ATA、包装、解包与退款回归。|

示例：epoch1目标净到账1000，实际WSOL输入1502；Token2022转账gross1026，其中26进入withheld。创建ATA需要182字节/2157600 lamports，普通创建+swap总原生资金2162600（含5000网络费）。组合临时输入退款时计划上限1510、实际用1502、余8和临时租金2039280退回；总预付租金4196880，持久收款ATA租金2157600，payer净SOL变化-2164102。withheld不算用户可用兑换收益。

复验入口：make -C integration token2022-preparation-current-check。原始fixture银行、运行时、浏览器下载及逐次执行凭证只保留本地；必要源码、测试和文档入库。最终同源码有限验收：源码`e62c0b4895e5f01576f5860b446da2c0e8cce2a593d21c285b3ca47cf4fbc480`，30项检查、15次Rust测试执行、27项真实Chrome业务联调全部通过。305份合同/工具副本、公共协议、严格Clippy、前端模型/语法及普通/-OO文档和篡改控制通过。五个原银行+退款组合共6组API wire、2组浏览器实际下载及5组历史账户/包装/解包wire合计13原程序成功执行，均核对对应输入上限减一失败回滚；完整池/bin/oracle/Token2022 vault原字节、可用余额+withheld守恒通过。现有178-byte与182-byte账户、未知/重复/截断/type/冻结拒绝及精确租金通过。首轮编译入口和浏览器跨Router地址认证脚本问题均已修正，失败/被替代记录独立保留，不计通过；最终运行下载与原执行文件按独立目录冻结。桌面/移动布局已复核；harness/静态服务器已结束，未向公网提交交易。多协议unsigned构建、其他Token扩展及策略交叉、钱包签名广播/确认、可信生产来源/模拟、其余链/DEX与净链费用模型继续列为剩余；all_requested_features_complete=false、production_ready=false。


<a id="plan-2026-11-13-six-stage-pipeline"></a>

## 六阶段业务链路主线（2026-11-13，阶段62起持续推进）

本主线按用户指定顺序实施：明确协议解析/数学→稳定QuoteView与能力标签→独立动态账户preparation→unsigned交易→独立模拟→钱包签名/广播/确认/回执。每个阶段和协议分别记录实际范围；能力标签中的未实现状态不算功能完成，单批通过也不等于六阶段全部完成。未知模型/账户扩展继续明确拒绝；不通过缩小标签或把RPC模拟改名为独立模拟关闭缺项。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F62.1|补明确协议的解析和数学范围|WP03,WP05,WP11,WP12,WP25|PARTIAL_LOCAL|本批为固定f4b9b263 Raydium CPMM creator经典SPL补独立ExactOut及完整费用账本，1832原native-helper向量583成功/1249拒绝的数学和路由预检通过，另增加18个三步序列/54步有序账本回放；既有1604 ExactIn回归通过。其他协议/版本和高级交叉仍继续逐项补。|
|F62.2|稳定协议QuoteView与能力标签|WP09,WP12,WP16,WP25|LOCAL_SCOPE_VERIFIED|按protocol+adapter划分不可变子视图，绑定父SnapshotRef/图版本/完整池状态hash/能力模型；报价返回实际所用protocolViewIds，前端校验；未知池保留诊断且不阻塞合格协议。|
|F62.3|动态账户解析独立为execution preparation|WP14,WP15,WP16,WP25|PARTIAL_LOCAL|新增xone-execution-preparation内部组件，迁出Meteora RPC/账户/程序/epoch/资金/Token2022/WSOL准备；quote-api保留授权、缓存计划校验和前后QuoteView门禁；无新增核心服务。其他协议preparation适配器仍需继续。|
|F62.4|仅生成unsigned transaction|WP14,WP16,WP25|IN_PROGRESS|既有Meteora限定模板迁入独立层并回归；Raydium及其他协议模板按各自账户闭包/指令构建后接入，不能用数学支持标签开放交易。|
|F62.5|独立模拟|WP05,WP15,WP25|IN_PROGRESS|保留已有有限REVM后端、原SBF受控参考及普通RPC只读模拟的区别；各新增交易模板需实际wire、真实收款和完整后状态验证，普通RPC成功不自动获得生产VERIFIED。|
|F62.6|最后接钱包签名/广播/确认/回执|WP14,WP16,WP25|PARTIAL_LOCAL|阶段65已对已实现/独立模拟的Solana单签名模板接Wallet Standard用户确认、真实Ed25519与消息不变、显式广播及可恢复确认/失败/过期/回执更新；默认关闭且按链配置；其他协议/链仍需各自模板和来源验收。|

阶段62当前实现与测试：Raydium ExactOut直接按原swap_base_output输出驱动求输入，creator费用在输入或输出侧分别逆解，交易/协议/fund/creator费用、raw vault及净储备完整更新；不是反复ExactIn搜索。原native helper复用固定上游CurveCalculator和PoolState，不导入业务数学；它不是SBF交易证明。当前路由限单池有序分配，保留最大输入/内部资金/原子回滚约束。公共状态契约未改变。

协议子视图与父QuoteView共享撤销令牌，reorg/范围撤权一起失效；普通快照的历史不可变句柄保持原设计。能力标签含具体adapter/model版本、ExactIn/ExactOut范围、preparation/unsigned/RPC模拟/独立模拟/钱包是否实现；后续每个能力按证据开放。前端使用返回的协议视图ID核对当前元数据，参数和状态变化仍撤销旧下载。

复验入口：make -C integration pipeline-foundation-current-check。当前为开发中记录，同源码检查/真实浏览器/原程序回归完成后回填本批结果；六阶段主线保持未完成，all_requested_features_complete=false、production_ready=false。

阶段62本批最终有限验收：源码`49c9d63d92859897c3d842aa848c771a9473e832905d5d392a9014de68089c7c`，36项检查、34次Rust测试执行、28项真实Chrome联调全部通过。Raydium原native helper1832目标（583成功、1249拒绝）及18序列54步、最大输入减一、共享物理池分配/HTTP计划均通过；61模型能力目录、现有fixture模型标签、协议视图稳定性/状态绑定/未知池诊断/撤销通过。Meteora执行准备迁出后，已有账户/ATA/WSOL包装/解包退款/Token2022及13组实际下载原SBF/失败回滚全部回归通过。306份合同与工具副本、公共协议、严格Clippy、前端模型/桌面/移动布局、普通/-OO进度与篡改控制通过；失败/被替代预检记录保留，不计本批通过。所有测试服务已退出，无公网交易。

下一阶段继续F62.4–F62.6，不结束六阶段主线。独立SBF预算原型已在实际Token2022+WSOL退款unsigned字节验证：预算1 CU明确ComputationalBudgetExceeded、预算1400000 CU正常执行70140 CU/fee5000/净到账1000，源银行未提交变化；这仍是服务接入前原型，不能把它标为接口/钱包完成。其余协议preparation/交易模板和未覆盖数学组合继续按能力目录逐项补齐，整体all_requested_features_complete=false、production_ready=false。


<a id="plan-2026-11-13-raydium-preparation"></a>

## Raydium动态账户与unsigned交易（2026-11-13，阶段63）

继续六阶段主线第3–4项，为已增加独立数学的Raydium CPMM固定f4b9b263 creator经典SPL模型补execution-preparation适配器。账户依赖读取与校验由独立层负责，quote-api继续绑定缓存计划/QuoteView；仅构建待签名交易，不包含钱包密钥或实际广播。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F63.1|Raydium动态依赖闭包|WP05,WP14,WP25|PARTIAL_LOCAL|校验程序/ProgramData与经典Token代码、池/config固定布局/PDA、完整费用账本、raw vault/用户余额、mint/decimals、Clock slot/epoch/time及observation身份；错误依赖拒绝。|
|F63.2|双向ExactIn/ExactOut unsigned|WP14,WP16,WP25|PARTIAL_LOCAL|按固定原swap_base_input/swap_base_output指令、13个角色账户、正确代币/vault方向及计划maxIn/minOut编译单签名legacy交易；已有源/目标账户、payer fee和blockhash检查。|
|F63.3|原SBF模拟与完整后状态|WP15,WP25|PARTIAL_LOCAL|真实RPC测试以保留原SBF/Token CPI执行待签名字节；目标1000的双向实际输入4/344448与独立数学一致；完整池费用账本、vault、用户余额、Token守恒及输入减一回滚通过预检。|
|F63.4|原Oracle写入比对|WP05,WP15,WP25|PARTIAL_LOCAL|发现未越15秒边界仍会累计价格并更新last_update_timestamp，不能假定不变；直接调用固定原源码ObservationState::update，对4075-byte实际SBF后状态逐字节比对。|
|F63.5|能力标签与前端实际下载|WP09,WP16,WP25|PARTIAL_LOCAL|能力模型/2增加accountCreation/nativeFunding并区分Raydium已有账户范围；禁用未实现建户/包装/解包选项。前端同时在元数据刷新与报价返回时核对协议视图，父ID未变但能力视图变更也撤销旧结果；真实Chrome预检通过。|

本批使用保留的原程序/账户银行，用户资金账户为明确控制覆盖。独立native helper为数学投影使用localnet链标签，池状态/金额不改写；实际API与unsigned/SBF测试保留来源链和程序身份，不以投影标签声称链部署或历史共识资格。swap2类Meteora与CPMM账户/程序各自校验，不混用SPL程序代码版本。

仅对已存在的经典SPL Token账户开放本Raydium模板；缺失/不足账户返回blocker，不假造资产，不静默加入未建模指令。createDestinationAta/wrapSourceSol/refundWrappedSource/unwrapDestinationSol在Raydium中显式拒绝；Meteora既有模板继续回归。普通模拟仍RPC_SIMULATED、independentProductionVerification=false；独立模拟运行时服务接入与钱包/广播/确认/回执仍按六阶段后续实现。

复验make -C integration raydium-preparation-current-check；最终同源码有限验收：源码`1eebd84144b09042493000f64ca1d5ad7d1f14f4049fd56b5483774c3234b4e1`，41项检查、35次Rust测试执行、30项真实Chrome联调全部通过。Raydium浏览器下载+双向API wire共3原SBF交易与对应最大输入减一回滚，完整Pool/vault/Oracle和真实用户收付通过；13组既有Meteora/Token2022/WSOL原执行回归通过。306份合同及工具副本、公共协议、严格Clippy、前端模型/桌面移动布局、普通/-OO篡改和进度检查通过。编译/测试辅助参数与oracle不变假设的失败记录保留，修复后按最终源码重新统一复验，不计旧失败为通过。harness和静态服务器均已退出，无公网交易。all_requested_features_complete=false、production_ready=false，六阶段主线继续。

阶段63继续六阶段主线：Raydium既有账户preparation/unsigned模板已完成注明范围；其他选项/协议仍按能力独立补齐。下一项为配置化隔离原SBF模拟服务及证据缓存绑定，再接钱包签名/广播/确认/回执，不将本批RPC_SIMULATED当独立生产VERIFIED。


<a id="plan-2026-11-13-local-sbf-service"></a>

## 配置化隔离SBF独立模拟（2026-11-13，阶段64）

继续六阶段主线第5项，将原SBF预算原型接入Router服务。新增可选api.solana_runtime_manifest，默认空/不改变既有配置摘要，两个解析服务不得加载该执行运行时；三服务配置保持字段一致。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F64.1|显式运行时与银行manifest|WP05,WP15,WP25|PARTIAL_LOCAL|manifest绑定Python可执行文件、Solders0.29.0运行时成员、账户银行文件SHA256及chain/genesis；43个运行时文件校验后复制至私有临时目录，不复制缓存字节码/任意路径。银行仅作为受控本地执行来源。|
|F64.2|独立子进程与预算|WP15,WP25|PARTIAL_LOCAL|使用独立-I/-S/-B Python进程，仅stdin接收本地bank+actual unsigned bytes；原SBF/Token CPI执行，真实CU1..1400000限制、100..15000ms deadline、并发1..8和输入/输出字节限制；异常/超时杀进程。|
|F64.3|快照和金额校验|WP05,WP09,WP15,WP25|PARTIAL_LOCAL|选用完整Pool模型匹配银行；原账户/程序/Clock/bin/vault与模型核对；模拟前后银行不提交变更；验证真实用户净收付、payer原生变化、费用租金及Token2022 withheld，不接受Router自己重算代替实际VM执行。|
|F64.4|preparation缓存与API证据|WP09,WP15,WP16,WP25|PARTIAL_LOCAL|只接收缓存preparationHash，不允许客户端提交任意模拟交易；接口绑定quoteId/planHash/QuoteView/协议视图，前后检查当前视图/过期，证据绑定runtime/bank/worker/wire摘要。缓存按256项上限和期限清理。|
|F64.5|前端独立模拟与拒绝场景|WP16,WP25|PARTIAL_LOCAL|页面增加独立本地模拟按钮，仅在当前池runtime银行可用且准备完成时启用；选项/报价变化清空旧证据，旧响应不得恢复；原SBF子进程、manifest变更、真实CU不足及API换报价/撤权已预检。|

新接口POST /v1/quotes/{id}/preparations/{preparationHash}/simulate-independent。返回LOCAL_SBF_SIMULATED，含actualInput/actualOutput/feeLamports/payerLamportDelta/unitsConsumed/sourceBankUnchanged、完整VM后账户数据摘要及evidenceHash；signed=false、submitted=false、independentProductionVerification=false。独立执行是对实际wire运行原程序；“本地独立”与“可信生产来源”仍是两项独立要求，生产VERIFIED/build不因本接口而放开。

当前manifest对应已保留的Meteora经典/Token2022和Raydium经典银行，模拟仅用于固定受控状态；原blockhash过期检查仅在隔离bank显式跳过，不能作为当前网络可提交证据。运行时隔离用于避免污染主进程并执行超时/输出/VM预算，不宣称宿主OS安全沙箱或完整共识实现。

测试数据/manifest生成：integration/tools/ci/solana-sbf-runtime-manifests.py，生成本地服务专用fixture副本，非提交文件。旧阶段60下载wire仅投影新增DTO字段并重算fixture commitment，原wire字节不改写、旧验收凭证不覆盖。复验make -C integration solana-independent-current-check；最终同源码有限验收：源码`6c475c5b77de9062690a7f658342f05e751399b7d54d09ffb61a9bf18bc83429`，46项检查、39次Rust测试执行、34项真实Chrome业务联调全部通过。配置三服务兼容/禁止解析服务运行、308份合同工具副本、公共协议、严格Clippy和普通/-OO文档及反例通过。Meteora解包699→1000、Token2022退款1502→1000、Raydium4→1000的实际缓存wire在隔离原SBF执行，真实CU42762/70140/24999；CU1失败、runtime/bank/Python摘要变化、路径逃逸、金额篡改/银行未变、换quote和视图撤权拒绝均通过。前端3协议独立按钮、参数变化撤销旧证据、父/协议视图一致及桌面移动布局通过；原16组交易/回滚参考继续通过。早期字段投影与依赖/Go导入问题已修复，失败预检保留不计通过；最终运行按独立目录冻结下载/源码/工具/运行时/银行摘要。测试服务已结束，无公网交易。六阶段主线及其余协议/钱包/回执继续，all_requested_features_complete=false、production_ready=false。

阶段64之后继续F62.6钱包生命周期：已做Wallet Standard只签名接口原型和精确消息校验方案，尚未接入正式签名/广播/回执接口，不提前标记完成。其他协议范围及可信生产来源/执行资格仍保留，六阶段主线继续。


<a id="plan-2026-11-13-wallet-lifecycle"></a>

## 钱包签名广播与可恢复回执（2026-11-13，阶段65）

按六阶段顺序，在实际unsigned与配置化独立SBF执行后接第6项。这里只对已实现/已模拟的单签名legacy Solana模板开放；原61模型目录仍分别披露执行缺项，不把其他协议/链未实现项标为完成。默认transaction_submission_enabled=false，必须显式配置独立运行时、receipt journal、允许链和pending上限。两个解析服务不允许运行执行/签名广播功能。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F65.1|Wallet Standard用户签名|WP14,WP16,WP25|PARTIAL_LOCAL|发现/连接钱包、账户和chain能力选择，只调用solana:signTransaction；不调用signAndSend、不生成/保管用户私钥；拒签、切换账户、过期及旧签名晚返回清空发送资格。|
|F65.2|精确消息与真实签名|WP14,WP15,WP25|PARTIAL_LOCAL|客户端检查钱包未修改消息，服务端用ring Ed25519核对签名、唯一payer及完整legacy消息字节与已缓存unsigned相同；计划/preparation/quote/独立证据及当前视图门禁。|
|F65.3|显式广播与实时预检|WP14,WP15,WP16,WP25|PARTIAL_LOCAL|用户单独勾选发送确认；按链显式配置、fresh genesis/blockheight/blockhash/fee/账户读取和sigVerify=true模拟，重新核对收付/withheld/原生余额；随后fsync SUBMITTING再sendTransaction，maxRetries=0。|
|F65.4|持久化日志与恢复|WP09,WP16,WP25|PARTIAL_LOCAL|独占文件锁、0600创建、文件及父目录同步、递增序号+前哈希/记录哈希链、16MiB/4096记录/pending预算；损坏完整记录拒绝，未完成尾部截断后同步；IO失败停止追加。按签名幂等，网络不明确不盲重发。|
|F65.5|确认与真实结算回执|WP15,WP16,WP25|PARTIAL_LOCAL|getSignatureStatuses处理processed/confirmed/finalized/failed/expired/unknown；getTransaction字节与签名/slot核对，按pre/post token/native实际余额验证目标/输入限额，遗漏meta保持RECEIPT_PENDING，收付不符标SETTLEMENT_MISMATCH，不能按报价补造成功。|
|F65.6|前端全链路与重启反例|WP16,WP25|PARTIAL_LOCAL|连接→准备→独立模拟→只签名→服务验证→用户确认→发送→回执；会话仅保存最后签名/Router选择，不存私钥/签名字节；刷新恢复不重签不重发；8类后台状态/重启、消息篡改/拒签/晚返回与浏览器实际发送已预检。|

新接口：POST /v1/quotes/{id}/preparations/{hash}/verify-signature、POST .../submit（confirm=true）、GET /v1/transactions/{signature}。verify只验证不发送；重复submit按已存在签名返回日志状态，发送结果不明确通过签名查询。日志保留可公开查询的已签名字节用于严格回执匹配，文件私有0600、不存私钥、不写主日志；本地单实例锁不替代多区/多租户生产执行协调。

回执只在RPC返回匹配的已签名wire/slot/metadata且实际收付款满足指令目标后记成功；ExactIn要求原输入量和滑点最小输出，ExactOut要求计划maxIn内和目标净到账。普通确认可随后回到UNKNOWN，最终成功/失败/结算不符不回退；EXPIRED表示当前节点在有效期后未观察到，不伪称全网从未打包。

测试“广播”仅向loopback fixture发送，由固定测试密钥签名的真实wire在原SBF本地执行；不连接真实用户钱包、不向公网sendTransaction。SDK仅测试辅助钱包路径，不进入生产密钥管理；Wallet Standard第三方源码只读核实，不打包下载依赖。本批最终同源码有限验收：源码`a30eee92f6dd0ecb59d117d3af30dc9201802121b766efc7b2a798bfcfb23814`，51项检查、44次Rust测试执行、40项真实Chrome联调全部通过。八类后台状态均在真实测试签名/原SBF单次发送后重启验证，缺证据/错误签名/未确认/默认禁用/错误genesis/实时模拟金额变化拒绝；重复提交及恢复最多一次sendTransaction。前端5项钱包单测、拒签/改消息/晚返回撤销、未确认不发、最终实际4→1000/fee5000及刷新恢复不重签不重发通过；用户余额、Token2022、签名后预检、原16组SBF/回滚继续通过。308份合同/12工具副本、公共协议、Go/Rust配置、严格Clippy、普通/-OO反例和文档、移动/桌面布局通过。浏览器CORS测试钱包辅助路由和刷新Router选择问题已修正，失败记录保留不计通过；最终独立目录固定下载、发送日志、回执、截图、源码/工具/运行时/银行摘要。测试服务已结束，不涉及真实用户钱包/公网发送。复验make -C integration wallet-lifecycle-current-check，六阶段全量与生产资格仍按具体协议范围保留未完成项。

阶段62–65六类基础业务能力在注明的Solana模板中已完整打通：数学→协议视图/标签→独立preparation→unsigned→隔离原SBF模拟→钱包签名/显式发送/确认/实际回执恢复。此结论不等于设计清单的所有链/DEX/组合都完成；61模型中未实现的execution preparation/交易模板仍以能力标签和剩余清单披露，Math范围也未扩展到未知hook/Token/版本。已知整体缺项继续推进，all_requested_features_complete=false、production_ready=false。


<a id="plan-2026-11-13-rpc-capture"></a>

## preparation实时RPC快照与独立SBF（2026-11-13，阶段66）

继续六阶段主线的实际缺口：阶段64独立SBF只匹配预装Pool/钱包银行，不能作为在线任意钱包账户的完整执行链路。本批新增显式rpc_capture运行时模式，从已经通过动态账户校验的同一次preparation getMultipleAccounts结果捕获完整闭包，银行不再要求提前写入manifest。仍使用固定隔离运行时，后端不从客户端接受任意bank或任意wire。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F66.1|同次preparation账户快照|WP05,WP14,WP25|PARTIAL_LOCAL|独立preparation组件新增AccountSnapshot与captured返回，收集唯一一次同context账户批读、null缺失账户、chain/genesis/slot；账户校验和unsigned成功后才缓存，对外仅摘要，不返回原bank/可执行程序字节。|
|F66.2|完整动态依赖闭包|WP05,WP14,WP15,WP25|PARTIAL_LOCAL|加入Rent sysvar、收款人和程序事件/authority等指令依赖；模拟要求每个编译消息key都在捕获的present/absent集合中。ATA创建收款人遗漏反例已发现并补齐，禁止用VM默认账户掩盖RPC闭包缺口。|
|F66.3|capture-only独立SBF执行|WP15,WP25|PARTIAL_LOCAL|manifest rpc_capture=true可无预装bank；与preparation公开摘要、slot、Pool、chain/genesis精确绑定，再对实际原始账户执行。Rent从捕获17字节恢复，原程序/ProgramData代码与部署slot核对，System/ATA/Memo等VM内置程序需与捕获可执行身份一致。|
|F66.4|快照原子性与内存预算|WP09,WP15,WP16,WP25|PARTIAL_LOCAL|快照使用Arc不可变缓存、单项8MiB和累计账户JSON序列化字节32MiB及256项上限；独立模拟不重新RPC读账户、不合并不同context结果。参数/QuoteView/期限改变按原门禁撤销；snapshotHash参与preparation与模拟evidence承诺。|
|F66.5|实时推进和钱包全链路回归|WP09,WP16,WP25|PARTIAL_LOCAL|测试未预装源地址、Meteora/Token2022/Raydium及slot推进后的Pool+Clock；改变当前RPC后旧捕获银行不变、新prepare拒绝旧报价；无预装bank模式下真实Chrome独立模拟/签名/单次发送/实际回执恢复统一联验。|

返回RPC_CAPTURE_SBF_SIMULATED、accountSource=PREPARATION_RPC_CAPTURE、historicalBankVerified=false、independentProductionVerification=false，并绑定捕获银行hash、runtime/worker/wire及后账户摘要。前端明确展示“RPC账户快照，非历史银行证明”。一次getMultipleAccounts响应是单context账户集合，不代表已获历史slot的共识银行/完整链特征；本批仍要求捕获Clock与已选Pool模型slot/时间/epoch一致。捕获blockhash有效性在签名后实时RPC预检另查，隔离VM跳过当前网络过期检查不能用于提交资格证明。

每个runtime仍显式绑定chain/genesis、已核对的固定Python/Solders文件和真实CU/deadline/字节预算。本批拓展账户来源，不扩展未知程序、其他协议/Token模型或完整链执行资格。原固定本地bank模式继续可用，公开VERIFY/build资格不改变。复验make -C integration rpc-capture-current-check；最终同源码有限验收：源码`9f8dd4845e0d50795bfa8861e5751755be5d3723a6e047898100dd1478e48d4f`，51项检查、47次Rust测试执行、41项真实Chrome联调全部通过。六种原生状态场景含未预装源地址、经典/Token2022/Raydium、下一slot及原程序完整成交后状态重报价；当前RPC变化不改变旧捕获银行、模拟不refetch、视图撤销后拒绝、capture-only银行/租金/程序/VM内置身份/摘要/缺依赖反例均通过。24份实际大小preparation快照触发32MiB账户JSON字节预算回收，未达到256条前已淘汰、最新条目仍保留。capture-only运行时下Chrome准备→独立执行→用户签名→服务校验→单次loopback发送→最终4/1000/fee5000回执→刷新恢复全部通过；原固定银行、Token2022/WSOL和16原SBF/回滚及八类生命周期继续回归通过。308合同/12工具副本、三服务配置、公共协议、严格Clippy、普通/-OO反例与文档检查、桌面/移动布局通过。捕获收款人依赖遗漏、测试越过原池容量、初始编译参数等失败均保留并修复，不计旧失败为通过；原始执行/日志/回执/下载/截图与最终源码、工具、运行时及银行摘要按独立目录冻结。测试服务已退出，未提交公网交易。六阶段整体仍按协议缺口推进，all_requested_features_complete=false、production_ready=false。

阶段66后续核对：按已通过阶段65目录审计（能力代码本批未变），61模型中16支持独立ExactOut、7具备preparation/交易模板，仍有54模型未接入执行准备；具体模型不能因基础六类链路已通而记完成。下一批先补现有数学明确的PumpSwap经典/boost与Pump曲线执行适配。原SBF预研已观察PumpSwap sell/buy/buy_exact_quote_in可由user兼网络payer只用单签名，wire889/957字节，完整后状态与原参考一致（用户另扣5000网络费）；该结果仅为原型，不升级功能能力标签。持续目标保持active。


<a id="plan-2026-11-13-pump-amm-execution"></a>

## PumpSwap经典及boost执行链路（2026-11-13，阶段67）

继续六阶段主线，把已有解析/数学明确的PumpSwap classic-wsol和boost版本接入execution-preparation、真实unsigned、隔离SBF和钱包回执，不另建服务/图。沿用固定6a7ad345 PumpSwap、4f2955d0 Fee程序及经典SPL代码；caller/recipient必须为模型声明user，用户兼任网络费payer；不把数学支持扩大为任意未来程序/Token行为。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F67.1|动态账户及费用身份校验|WP05,WP14,WP20,WP25|PARTIAL_LOCAL|完整模型Pool/global/fee/mint/vault/creator/protocol/buyback/volume原字节与lamports、主/Fee/Token/ATA程序版本、Clock精确匹配；实际账户资金/冻结/币种/owner核对。全部实际指令key捕获在同RPC快照内。|
|F67.2|三种原unsigned指令|WP14,WP16,WP25|PARTIAL_LOCAL|sell固定输入/最小WSOL输出；buy按目标base/最大quote输入；buy_exact_quote_in固定quote/最小base输出。原IDL角色/权限/双Token程序/creator与buyback尾账户、volume关闭选项逐项构建，单签名wire889/957字节。|
|F67.3|独立执行与完整后状态|WP15,WP20,WP25|PARTIAL_LOCAL|经典和原毕业迁移创建的boost池双方向×ExactIn/ExactOut共8银行，加高价最小输入/额外到账1银行。真实API wire在原PumpSwap/Fee/Token CPI执行，模型全部12依赖后状态、费用拆分/vault、WSOL backing/Token守恒与失败完整回滚独立比对。|
|F67.4|最小到账与实际额外到账|WP14,WP15,WP16,WP25|PARTIAL_LOCAL|卖出目标1000可实际到账48185，使用原sell的固定base/minQuote语义；prep保留quotedOutput=48185、minBuyAmount=1000、requiredInput=1。最小到账加入preparation哈希，最终回执按计划min而不是将surplus改作最低到账；旧日志仍保持原兼容计算。|
|F67.5|运行时和有界完整依赖|WP05,WP15,WP25|PARTIAL_LOCAL|Pump程序ProgramData分配10485760原字节，主/Fee/Token/ATA完整RPC闭包约15MiB；单批读取/捕获/银行/worker输入有界16MiB，总缓存JSON字节仍32MiB，不截断原程序/拼接不同slot账户。|
|F67.6|能力版本与前端钱包回执|WP09,WP16,WP25|PARTIAL_LOCAL|能力模型/4仅对两已实现PumpSwap模板开放现有账户preparation/unsigned/独立模拟/钱包代码；自动建户/资金转换继续false，并明确SINGLE_POOL_SINGLE_ROUTE，前端不能因拆单/多跳中含某个可准备edge就放开按钮。真实Chrome展示目标1000/实得48185/额外47185，准备/独立模拟/签名/单次发送/最终实付实收及刷新恢复已预检。|

原buy与buy_exact_quote_in的track_volume关闭，与现有数学/12账户模型一致；未知cashback/hook/Token2022和不同volume策略继续按模型准入拒绝。经典SPL用户账户可显式传入或按规范ATA派生，缺失/不足账户返回blocker；本批尚不加入createDestinationAta/wrapSourceSol/refundWrappedSource/unwrapDestinationSol指令，明确拒绝且前端禁用。该限制仍是后续功能，不能按“标签false”替代开发。

测试银行为受控canonical及真实原migrate创建的boost状态；高价bank的WSOL储备为显式控制且完整backing，不能宣称主网可达或当前市场收益。声明user使用固定测试密钥，仅loopback RPC执行，用户额外支付5000网络费，与Token实收付分开；不向公网提交交易。原程序源码/IDL/binaries只读固定，不提交下载包、银行、运行时或临时回执。

数据恢复：integration/tools/ci/pump-amm-execution-bank.py；复验make -C integration pump-amm-preparation-current-check。最终同源码有限验收：源码`09d3bea3332787a5fc908e32eef0f71371a9d8f7a5106d50ed800433f741790a`，56项检查、51次Rust测试执行、46项真实Chrome联调全部通过。九组canonical/boost双向ExactIn/ExactOut及额外到账原wire/完整后状态、Token守恒/WSOL backing和失败回滚参考通过；五组真实测试签名、单次loopback发送、重启恢复及minBuyAmount回执通过。费用程序/Pool/Clock变更、冻结/错误币种/缺失/余额不足和不支持选项拒绝通过；既有Meteora经典/Token2022/WSOL、Raydium、隔离SBF边界、八类生命周期、严格Clippy和普通/-OO反例及文档继续回归。309合同/12工具副本/164公共来源、前端模型和钱包单测、桌面/移动布局通过。运行产物与源码/工具/运行时/银行摘要已冻结；仅受控钱包与loopback发送，未提交公网交易。当前61模型中9个具备执行准备模板，52个仍缺少执行准备；Pump原生SOL曲线、自动创建/包装/解包和其他链/DEX模板继续推进。六阶段全量目标保持active，all_requested_features_complete=false、production_ready=false。

<a id="plan-2026-11-13-pump-curve-execution"></a>

## Pump原生SOL曲线执行（2026-11-13，阶段68）

继续六阶段主线，固定7a19affaa2c6 Pump/Fee/经典SPL版本，复用现有native:solana:sol图节点及明确的ExactIn/ExactOut数学；不将SOL当WSOL、也不另建执行服务。仅经典曲线、已有用户Token账户、caller/recipient/payer等于声明user；毕业迁移仍是独立后续指令。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F68.1|完整曲线动态账户|WP05,WP14,WP20,WP25|PARTIAL_LOCAL|十个模型依赖原字节/owner/lamports、Pump/Fee/Token/ATA程序、Clock与user资金核对；原生侧强制user System账户，Token侧校验mint/owner/冻结和余额。|
|F68.2|原生资金及网络费|WP14,WP15,WP16,WP25|PARTIAL_LOCAL|sourceNativeSol/destinationNativeSol绑定资产及别名账户；买入资金覆盖交易最大输入和网络费，实际扣费后恢复交易净额；卖出实际到账与费用/用户净变动分列，最低到账入准备哈希。|
|F68.3|原unsigned三指令|WP14,WP16,WP25|PARTIAL_LOCAL|固定IDL构建sell、buy和buy_exact_sol_in、bonding-curve-v2/buyback尾账户，关闭volume和partial选项；user单签名，实际wire657/725字节。|
|F68.4|独立SBF及回滚|WP15,WP20,WP25|PARTIAL_LOCAL|五组实际程序银行：卖出/买入双向ExactIn/ExactOut及目标1000实得10971的最小输入；完整模型后状态、Token/SOL守恒、手续费和失败回滚独立参考；实际API五组及七反例、原程序完整后状态/回滚联验通过。|
|F68.5|钱包回执及前端|WP09,WP16,WP25|PARTIAL_LOCAL|签名后实时模拟、单次loopback发送、重启幂等和实际原生余额回执联验；能力模型/5显式nativeSolTrading，nativeFunding仍指自动SOL/WSOL转换；Chrome显示原生资产和净余额。|

银行生成integration/tools/ci/pump-curve-execution-bank.py直接执行保留固定原程序，不依赖临时成功报告；复验make -C integration pump-curve-preparation-current-check。最终同源码有限验收：源码`c27be25a2476eb7ff3585504ef1fb80ed2c9a1b8b2597fc5695f8dd43fbbb8b4`，31项检查、26次Rust测试执行、51项真实Chrome联调全部通过。五组实际API原wire、模型全部后状态、Token/SOL守恒、网络费和失败完整回滚参考通过；五组原生买卖真实测试签名、单次loopback发送、重复提交及重启回执通过。独立模拟、signed RPC预检和meta回执均从lamports恢复交易净额，最后一组实际1/10971、费用5000、用户净增5971及最低到账1000分列。既有Meteora/Token2022/WSOL/PumpSwap/Raydium及八类钱包状态、RPC捕获/缓存预算、严格Clippy、普通/-OO反例和文档、桌面/移动布局继续回归通过。310合同/12工具副本/164公共来源通过。初始手动预检运行时路径缺失及拒绝场景复用过期报价的测试问题已修正；每个拒绝场景取得新报价，不改生产期限/超时。失败记录保留，不计入通过；最终运行固定源码、工具、银行/运行时、下载、回执与截图摘要。当前61模型中10个具备执行准备模板，其余51模型仍缺少模板。经典曲线自动收款ATA、PumpSwap自动账户/资金转换、毕业迁移及其他链/DEX继续推进；用户签名测试只向loopback发送，未提交公网交易。六阶段全量目标保持active，all_requested_features_complete=false、production_ready=false。

<a id="plan-2026-11-13-pump-native-ata"></a>

## Pump原生收款ATA（2026-11-13，阶段69）

继续补执行账户功能，经典Pump原生SOL买入可按用户明确选择加入幂等收款ATA创建，与原buy/buy_exact_sol_in在同一user单签名交易原子执行；数学/报价约束不变，不加入隐式SOL/WSOL转换。处理地址完全缺失、已有空System账户预存部分或足额租金；对非规范ATA、其他owner/executable/非空System数据继续拒绝。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F69.1|原子ATA创建与兑换|WP14,WP16,WP20,WP25|PARTIAL_LOCAL|固定原ATA/System/SPL/Pump指令，canonical owner/mint/PDA与六个创建meta核对，先创建再兑换、单签名wire767字节，失败回滚账户和租金。|
|F69.2|预存余额与租金补差|WP05,WP14,WP15,WP25|PARTIAL_LOCAL|按minRent=2039280减已存在System账户余额并饱和到零；不存在/预存1/预存2039281分别需2039280/2039279/0。准备结果公开最小租金、预存和本次补差，额外租金不冒充交易输入。|
|F69.3|独立及signed模拟|WP14,WP15,WP20,WP25|PARTIAL_LOCAL|原生创建前的规范空System账户按零Token初值处理，after必须是正确mint/user的初始化SPL账户；非原生/未创建分支保持原校验，完整账户捕获和原SBF执行。|
|F69.4|实际签名与回执|WP09,WP16,WP20,WP25|PARTIAL_LOCAL|四个创建场景双向ExactIn/ExactOut与预存余额原型后状态/回滚通过；实际API wire、独立证据、显式发送、重复提交/重启和真实3→1000/fee5000/不同净SOL变动联验通过。|
|F69.5|能力与Chrome联调|WP09,WP16,WP25|PARTIAL_LOCAL|能力模型/6仅经典曲线加入收款ATA；前端只对买入Token开放，native输出禁用。显示预存/补差、下载create+swap、独立模拟、钱包回执及刷新恢复联验通过；回执显示实际用户SOL净变动。|

银行复用生成工具integration/tools/ci/pump-curve-execution-bank.py；复验make -C integration pump-native-ata-current-check。最终同源码有限验收：源码`1d1b4e54c0709645134a57a83001b53c2e6325e7d8f4f8cfb00a97908c82ebe7`，22项检查、6次Rust测试执行、56项真实Chrome联调全部通过。九组实际API wire含原五组和四个创建/预存场景，完整原程序后状态、Token/SOL守恒、最小输出/输入控制及失败原子回滚参考通过。普通/-OO各10个反例通过，含租金冒充交易输入、漏计租金、重编译ATA其他owner。实际API签名发送及重启回执核对补差2039280/2039279/0，实付3/到账1000/fee5000与用户净变动-2044283/-2044282/-5003分列；受控发送时故障注入的已创建ATA兑换失败回执为FAILED且不重发。已有ATA显式勾选不重复创建/收租金，原生输出拒绝ATA选项。Chrome缺失账户阻止下载、显式创建+原swap下载、捕获独立SBF、用户仅签名/确认发送/实际净SOL回执和刷新恢复通过，既有所有浏览器联调继续通过。310合同/12工具副本/164公共来源、严格Clippy、前端模型和钱包单测及普通/-OO文档通过。新测试初始发送配置缺少挂起上限被严格拒绝，补齐测试显式配置后通过；未改生产校验，失败记录保留不计通过。最终源码、运行时/工具/银行、日志、原始执行/回滚、下载/签名回执和截图摘要已冻结。本批仍仅经典曲线/声明user、规范经典SPL收款ATA，不扩展WSOL资金转换、其他Token/volume或毕业迁移。10/61具备执行准备模板，其余51模型仍缺；下一批继续PumpSwap账户/包装/退还/解包及其他明确协议模板。未向公网发送，六阶段全量保持active，all_requested_features_complete=false、production_ready=false。


<a id="plan-2026-11-13-pump-amm-funding"></a>

## PumpSwap账户与SOL资金（2026-11-13，阶段70）

按六阶段依赖补齐现有PumpSwap classic/boost模板的账户生命周期与SOL/WSOL资金操作。数学、固定程序/IDL版本和单池单路范围不变，新增私有classic_funding组件，供独立execution preparation生成显式创建/补差/同步/关闭指令。只有本次派生且原先不存在的临时账户允许关闭；不能关闭用户已有账户，退款/解包上下文不满足时拒绝。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F70.1|规范ATA与预存余额|WP05,WP14,WP15,WP25|PARTIAL_LOCAL|缺失/空System规范ATA显式初始化，按真实最小租金补差；WSOL预存超过租金的余额作为已有Token初值，不算成交。|
|F70.2|包装/退款/解包|WP14,WP16,WP20,WP25|PARTIAL_LOCAL|从SOL仅补输入预算差额；临时输入关闭退还余量/租金，临时输出关闭解包；单签名实际wire899–1149字节、所有依赖同批捕获。|
|F70.3|独立模拟与原子回滚|WP14,WP15,WP20,WP25|PARTIAL_LOCAL|原程序、完整12账户后状态、全指令及有效权限、WSOL backing/Token守恒和保留创建前缀的失败控制；独立worker从原始before/after推导初值和租金。|
|F70.4|真实回执租金归因|WP09,WP16,WP20,WP25|PARTIAL_LOCAL|从执行meta推导实际persistent rent/初始WSOL，处理预存变化、已提前初始化ATA；不能用准备估算掩盖成交变化。临时账户必须先缺失后关闭。|
|F70.5|能力与前端联调|WP09,WP16,WP25|PARTIAL_LOCAL|能力模型/7为classic/boost开放账户创建和方向限定SOL资金；Chrome下载/独立执行/仅签名/显式发送/实际回执与刷新恢复。|

银行由integration/tools/ci/pump-amm-execution-bank.py恢复，共31组，保留原9组并增加classic/boost各11组资金场景；复验make -C integration pump-amm-funding-current-check。本批于2026-11-13启动，2026-11-14完成同源码有限验收：源码`06583817468ca0743bdb7a736d417f910d4a578e02784613ed78dbcd67a7a15a`，34项检查、26次Rust测试执行、72项真实Chrome联调全部通过。22组新增资金实际API wire、9组原PumpSwap、9组原生曲线及4组浏览器下载wire，共44组固定IDL/原程序执行参考与完整后状态、守恒/backing、失败前缀原子回滚通过；新增wire899–1149字节、单签名，低于1232字节上限。普通/-OO各20个资金/原生/ATA反例通过，含重编译资金重定向、关闭已有账户及预存余额冒充成交。

22组资金场景真实测试钱包签名、服务校验、单次loopback发送及重启回执恢复通过，另4组冻结现有vault的受控故障注入回执为FAILED且不重发。准备/独立模拟从原始账户推导初始化前余额与租金，signed预检严格拒绝租金变化；最终meta回执按实际租金和有效WSOL初值核算，不把准备估算当实际支出。四个Rust单元测试覆盖预存低于/高于租金、ATA提前幂等创建、临时账户缺失/关闭及SyncNative前额外SOL的归因。Chrome四条新流程（包装、退款、解包、缺失收款ATA）分别验证能力选项、下载、独立执行、仅签名、显式发送、实际回执和刷新恢复；既有56项浏览器回归继续通过。原PumpSwap/原生Pump、Meteora经典与Token2022/WSOL通用账户、八类钱包生命周期及RPC捕获/字节缓存预算回归通过。310合同/12工具副本/164公共来源、公共协议、严格Clippy、前端模型/钱包单测与普通/-OO文档检查通过。

初始并行测试误共用回执文件触发RECEIPT_JOURNAL_IN_USE，测试文件名加入独立RPC身份后通过，生产文件锁保持原校验。新增浏览器选择器、能力旧版本断言和ExactIn隐藏控件填写问题已修复；失败/中断日志均保留且不算通过。最终源码前后摘要、工具/运行时/银行、日志、原始执行/回滚、实际下载/回执、截图及harness二进制摘要已冻结并复核。本批仅扩展已有PumpSwap经典/boost单池单路SPL模板的账户和资金操作；执行准备仍10/61，其余51模板、Pump毕业迁移及未覆盖数学/Token/版本继续推进。未向公网发送；all_requested_features_complete=false、production_ready=false。


<a id="plan-2026-11-14-orca-exact-out"></a>

## Orca五模型精确输出（2026-11-14，阶段71）

按六阶段前两项补数学与稳定协议能力：固定cc683ec9的静态、奖励、自适应、自适应奖励、混合动态数组五个经典SPL模型接入原生ExactOut分支。通过输出指定量逐步递减、双向Q64价格舍入和输出封顶更新，按原程序ExactOut分支直接计算输入与后状态。沿用已有严格解析和完整账户/数组状态，保留三数组与步数预算边界。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F71.1|精确输出数学|WP12,WP15|PARTIAL_LOCAL|固定/非固定delta顺序、输出ceil/floor、费用上取整、u64/128/256边界、精确目标封顶与完整原始manager后状态；旧ExactIn保留回归。|
|F71.2|费用/奖励/数组|WP05,WP12,WP15|PARTIAL_LOCAL|跨tick/空流动性、奖励增长溢出/环绕、外部增长翻转、自适应Oracle/费用组及混合布局均对照固定原程序；零限价要求完整目标。|
|F71.3|路由和最佳输入|WP09,WP12,WP14,WP15|PARTIAL_LOCAL|单池目标路由、重复池按前次完整状态重算、三个自适应模型双池四份额原生穷举网格最小值、预算不足与原子拒绝。|
|F71.4|QuoteView与计划|WP05,WP14,WP16|PARTIAL_LOCAL|模型/8增加五个ExactOut模式；精确目标计划绑定Orca program factory，API仅用当前快照报价且不读取用户动态执行账户，旧视图撤销后不得构建。|
|F71.5|前端联调与能力边界|WP16,WP25|PARTIAL_LOCAL|五模型双向ExactOut目标/输入/路由显示，普通账户字段编辑不能启用未实现执行按钮；Router切换清除旧回执，旧查询不得覆盖新结果，发送晚响应仍可按原签名恢复；Orca执行能力保持false。|

独立参考由integration/tools/acceptance/orca_exact_out_reference.py生成，verify_orca_exact_out.py核对原始请求/响应、完整raw bank、来源摘要及逐项原程序复执行。最终同源码有限验收：源码`f9078a8c64d63e5a0e6728f8865fb993170cadbf8a4c8aff19ff0576e6087a22`，28项检查、101次Rust测试执行、91项真实Chrome联调全部通过。600个原始native manager场景含431成功/169拒绝，五个模型双向完整Pool/Oracle/奖励/tick模型后状态一致；8条四步连续序列、三个自适应模型的双池四份额独立枚举网格最小输入一致。原始记录包含153次数组状态更新、189次奖励状态更新、186次Oracle状态更新，成功/失败来源及逐次原始输入、输出与前后账户均留存。

600条实际服务流程记录分层核对：584次对应HTTP目标报价含431成功/153拒绝，另16条在状态准入层拒绝，没有生成HTTP报价。每个成功目标还核对预算减一拒绝、原报价/当前QuoteView/数学计划绑定；三个双池最佳输入与旧视图失效后的SNAPSHOT_REORGED经过实际API验证。纯报价不读取用户动态执行账户，不生成unsigned交易或授予SBF/钱包验证资格。普通/-OO各10个资金/目标/后状态/Oracle/奖励/序列/网格篡改反例通过；旧Orca ExactIn、旧协议ExactOut、计划验证及27个Orca解析测试通过。

Chrome验证五模型双向ExactOut并保持执行按钮禁用；编辑普通账户参数不能绕过能力门禁。画面复核发现切换Router仍残留旧回执，已按Router清除显示并保留当前浏览器会话的签名历史；增加查询版本，旧查询晚到错误不得覆盖返回原地址后恢复的当前回执。用户确认发送时先保存签名，实际loopback提交响应晚于Router切换时仍能恢复；保存/查询/切换均不重发。回执显示币对和原报价ID，发送期间禁止新签名。原有72项、五模型15项及4个回执隔离/恢复检查共91项通过，原有钱包/SBF流程继续正常。311合同/12工具副本/164公共来源、公共协议、严格Clippy、前端/钱包单测与普通/-OO各17项文档检查通过。

初始计划模块直接引用未声明数学依赖，改为独立固定profile准入谓词；参考刷新显式限定Orca家族，修正来源控制测试假设，未放宽生产校验。首次28项/101次/87项通过记录保留，视觉复核补回执测试后重新冻结并完成最终验收；旧失败/前一轮记录不充作当前源码验证。源码前后及当前摘要、上游完整清单、辅助程序/运行时/银行、日志、600条服务记录、原始复执行、桌面/移动截图和harness二进制摘要均固定并复核。复验make -C integration orca-exact-out-current-check；恢复生成数据用orca-exact-out-reference，生成物/下载依赖/二进制不提交。

运行时协议目录61个模型中21个支持ExactOut模式、10个具备执行准备；其余模式和51个执行模板仍是全量任务缺项。本批是原始native manager数学与显式vault投影，SPL CPI、SBF及钱包执行按模板继续实现；Orca的相关执行标签保持false。未向公网发送。all_requested_features_complete=false、production_ready=false。


<a id="plan-2026-11-14-rayclmm-exact-out"></a>

## Raydium CLMM六模型精确输出（2026-11-14，阶段72）

继续六阶段前两项。固定6d7a0f82的静态、奖励、Token选费、Bitmap、Custom Seed和动态费经典SPL模型接入原生输出指定分支，保留既有账户解析与完整状态。Token选费是协议从Token0/Token1收费的位置选择，和Token2022转账费分别处理，不扩大Token行为范围。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F72.1|输出指定量与收费位置|WP12,WP15|PARTIAL_LOCAL|按输入/输出选费分别计算价格驱动量、Q64输出舍入和净目标封顶；覆盖零费率但有原始舍入余量费用、u64/u128/U256/U512边界。|
|F72.2|完整后状态|WP05,WP12,WP15|PARTIAL_LOCAL|逐步LP/协议/fund分账、Observation、奖励与动态费用变量、Bitmap与Seed完整状态对照；保留旧ExactIn回归。|
|F72.3|路由与最佳输入|WP09,WP12,WP14,WP15|PARTIAL_LOCAL|单池单路目标、连续池消耗、六模型双向12组原程序双池四份额网格最小输入；重复池必须按前次后状态重算。|
|F72.4|解析/API/QuoteView|WP05,WP14,WP16|PARTIAL_LOCAL|原生序列化前后12个账户检查点反解六类完整Pool；实际API目标/预算/视图/Orca与Raydium factory独立绑定，能力模型/9更新。|
|F72.5|前端与复现|WP16,WP25|PARTIAL_LOCAL|六模型双向目标报价及能力门禁；新增18项开发预检，连同既有钱包/回执流程109项Chrome通过；固定参考输入/响应可重放。|

原始参考入口integration/tools/acceptance/rayclmm_exact_out_reference.py，verify_rayclmm_exact_out.py逐项复执行原始swap_internal/compute_swap，并核对显式vault投影、完整模型、Clock连续性和网格。最终同源码有限验收：源码`7dc0d07568c45f6b93f0c79824053a40c7a497fc1346c8ae282383159164e960`，27项检查、304次Rust测试执行、109项真实Chrome联调通过。1266参考场景含947成功/319拒绝，280输出步进向量含204成功及10个零费率输出余量费用场景；264个输出端收费场景、196次动态变量更新、44次奖励更新和350次数组状态更新经过原程序对照。6条四步序列按前次状态推进Clock，六模型双向12组四份额双池网格最小输入一致，缓存提示量不作为真实输入依据。

实际服务流程1266条分层核对：947次HTTP成功、307次HTTP拒绝、12次状态准入拒绝；另核对预算减一、单池范围、内部账户拒绝、完整计划/program factory、QuoteView绑定和旧视图撤销后的SNAPSHOT_REORGED。原生序列化的12个前后账户检查点反解六模型完整Pool一致，旧Raydium CLMM/Orca输入数学、旧ExactOut/API/计划和解析回归通过。普通/-OO各12个目标/费用/动态/奖励/Bitmap/Clock/单步/网格/来源篡改反例通过。前端六模型双向目标报价和执行门禁新增18项，连同既有91项钱包/SBF/回执联调共109项通过；Orca和Raydium factory分别绑定，未实现模板不借用其他池能力。

参考新Seed池经原始set_seed_index/PDA规则构造并补齐默认完整Bitmap扩展，避免改变原有准入条件；连续状态深拷贝避免下一次Clock污染上一条记录。初始参考构造器扩展遗漏、状态对象别名和检查点测试相对路径问题均已修正，生产校验未放宽，旧失败日志保留不计通过。上游固定程序源码未修改，自有参考只增加输出模式和受控输入构造/序列化入口，原始typed状态和显式vault投影不冒充SPL CPI/SBF或钱包执行。

源码前后与当前摘要、上游/Parity完整清单、辅助程序/运行时/银行、1266份流程记录、原始复执行/序列化、桌面/移动截图和harness二进制摘要已冻结并复核。313合同/12工具副本/164公共来源、公共协议、严格Clippy、前端/钱包单测与普通/-OO各17项文档检查通过。复验make -C integration rayclmm-exact-out-current-check；数据恢复入口rayclmm-exact-out-reference，生成数据/下载依赖/二进制不提交。实际目录61模型中27支持ExactOut（原21）、10具备执行准备；其余数学和51个执行模板继续推进，六模型执行标签保持false。未向公网发送。all_requested_features_complete=false、production_ready=false。


<a id="plan-2026-11-14-standard-v3-exact-out"></a>

## 标准V3精确输出与回调路由（2026-11-14，阶段73）

继续六阶段前两项与设计A0/A1的EVM基础范围。标准Uniswap V3和明确无LM的Pancake V3接入独立ExactOut接口，复用已有输出驱动的Q96内核并执行独立原始Pool合约核对。Pancake正常解析继续要求LM指针为零，LM和Slipstream保留已有独立上下文模型，不扩充未知行为。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F73.1|标准V3输出目标|WP12,WP15|PARTIAL_LOCAL|Q96输出指定、逐word费用舍入/跨tick/空流动性、完整目标与价格/步数/整数范围，原始负数amountSpecified执行对照。|
|F73.2|真实回调与投影|WP03,WP05,WP15|PARTIAL_LOCAL|官方固定Pool/Factory、真实create/initialize/mint与ERC20回调付款；核对实际扣付/到账及定价投影，额外原始费账/Oracle等观察单独保留。|
|F73.3|简单路径与网格|WP09,WP12,WP14,WP15|PARTIAL_LOCAL|同factory简单多跳、原始递归回调4条双向两跳、双费率池4份额网格；重复池/跨factory/混合模板拒绝。|
|F73.4|API与协议视图|WP05,WP14,WP16|PARTIAL_LOCAL|独立ExactOut金额与计划/视图绑定、预算减一和旧视图撤销；模型/10新增两类报价模式，不自动开放执行模板。|
|F73.5|前端联调/复现|WP16,WP25|PARTIAL_LOCAL|EVM链及双向币种/目标/原始输入显示、保留执行门禁，已有Solana钱包/回执流程继续回归；固定参考输入可重放。|

最终同源码有限验收：源码`478879aabb25b7c22f2e9f25736d223c38f94f3fc9e2c071221116cb4a544d98`，24项检查、45次Rust测试执行、115项真实Chrome联调通过。96个实际原Pool场景含80成功/16拒绝，80个最大输入减一控制在真实交易中回滚所有已观察原状态与Token余额；2条四步连续兑换、4条实际递归回调双向两跳、2组独立费率池四份额网格通过。实际HTTP 96个对应目标报价、预算限制、协议模式/QuoteView/数学计划/旧视图失效检查及双池最小输入与原始回调付款一致；旧V3价格内核、旧ExactOut/API/计划及Go V3/Pancake解析回归通过。普通/-OO各12个原始Token扣付/目标/费率/code/factory/价格后状态/序列/路径/网格/回滚/来源篡改反例通过。

模型定价投影只保留下一次报价所需price/tick/liquidity/proven bitmap/gross-net；原程序LP费账/Oracle等完整已观察状态和真实回调付款分别保留，不宣称基础模型包含全部合约账本。官方Uniswap core 1.0.0发布artifact与同锁定树中的1.0.1 artifact字节一致，仍明确选择1.0.0包和integrity；Pancake使用98684794固定未改源码和已固定0.7.6编译器。真实factory创建、initialize/mint及回调付款，没有Pool字节码修补或存储注入。受控调用器约束完整目标/maxInput，不作为生产执行器。参考EVM显式31337，重放在隔离目录再次执行，完整回执/原始读数/状态/余额/已加载依赖摘要确定一致；不提供主网部署或API unsigned/钱包广播资格。

前端新增两模型双向EVM报价/模式/执行门禁6项，与既有109项合计115项通过。314合同/12工具副本/164公共来源、公共协议、严格Clippy、前端/钱包单测和普通/-OO各17项文档检查通过。初始Pancake monorepo导入别名、实际参考chainId与声明身份一致性已修正，生产校验未放宽，失败日志保留。源码前后及当前摘要、官方artifact/上游清单、编译器/运行时/已加载模块、96份HTTP流程、原始复执行/实际Token余额与交易回执、桌面/移动截图和harness二进制摘要已冻结复核。复验make -C integration v3-exact-out-current-check，恢复入口v3-exact-out-reference，生成物/外部包/二进制不提交。

运行时目录61个模型中29支持ExactOut（原27）、10具备执行准备。Pancake正常解析继续拒绝非零LM，LM与Slipstream上下文模型另列；本批两个基础V3执行模板和其他数学/版本、51个执行模板继续推进。未向公网发送。all_requested_features_complete=false、production_ready=false。


<a id="plan-2026-11-14-weighted-exact-out"></a>

## Balancer不可变Weighted精确输出（2026-11-14，阶段74）

接续上一会话在阶段73后留下的未完成改动，按设计§12/13/14/15/23与区块解析、兑换量计算优先级完成 `balancer-v2-weighted2-equal`、`balancer-v2-weighted2-static`、`balancer-v2-weighted-multi` 的输出目标链路。复用现有Go固定块解析状态、Rust物理池副本/图及API，不新增服务或独立可变的币对状态。当前状态：`LOCAL_SCOPE_VERIFIED`（原回调、路由、API与前端的有限本地范围）。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F74.1|不可变Weighted GIVEN_OUT|WP12,WP25|PARTIAL_LOCAL|输出缩放、向上取整base/exponent/pow、输入反缩放后加费；输出30%上限、uint256中间值与uint112现金限制。304原回调场景（222成功/82拒绝）及12条序列48步数学对照已通过。|
|F74.2|多资产物理状态与路由|WP12,WP14|PARTIAL_LOCAL|3–20币完整余额向量、显式from_token和所有token索引；保留单池单路线范围，重复池/内部资金/歧义token边拒绝；原子提交后状态。|
|F74.3|最佳输入与顺序分配|WP14,WP15|PARTIAL_LOCAL|原回调双池四份额网格，逐次重算同一物理池；在已有有界搜索内选择最小输入，预算减一拒绝，不声明任意连续拆分全局最优。|
|F74.4|HTTP与数学计划|WP15,WP16,WP25|PARTIAL_LOCAL|/v1/quote-exact-out、完整目标/maxSellAmount、显式token路径计划、协议视图和失效撤销；模型版本/11与报价能力保持一致。|
|F74.5|前端与可复验入口|WP16,WP25|PARTIAL_LOCAL|三模型双向报价、非首币对、真实Chrome和执行按钮门禁；make -C integration weighted-exact-out-current-check。|

独立参考锁定Balancerv2源码 `e281ff40583c39bf1d46c322334b2b13936e0e33`，逐文件SHA256核对后以原WeightedPool构造器和onSwap执行GIVEN_OUT，BalanceAllocation更新现金；测试Vault facade不包含ERC20转账、完整Vault或真实部署证明，不能据此升级unsigned/独立VM/钱包执行资格。原模型的Token、部署、固定块准入条件继续适用。Managed、Composable、BPT、RateProvider和Stable没有借用此内核开放ExactOut。

实际能力目录为61模型、32支持ExactOut（原29）、10具备执行准备；本批三个Weighted执行模板及其余51个模板仍未实现。全量六阶段和26工作包完整验收继续未完成，all_requested_features_complete=false、production_ready=false。本批最优只覆盖指定离散网格的原始输入，净链费用和任意连续拆分的全局最优没有据此验收。


最终同源码复验通过：`make -C integration weighted-exact-out-current-check` 执行17项检查全部成功，33次Rust测试执行（含旧Weighted输入、V2/V3目标报价与路径回归）、12项真实Chrome双向金额/门禁检查、11项前端单测、普通与-OO各17项进度文档检查通过。Go固定块Weighted/多资产、配置、状态和wire回归覆盖4个包；315份合同/12工具副本/164公共来源、公共协议生成物及严格Clippy（核心、计划和前端测试服务）通过。304个原回调场景为222成功/82拒绝，12条连续序列48步、2组双池四份额网格和2组同池三次顺序分配均与原回调一致。HTTP对应304目标场景、222个成功场景的预算减一拒绝、数学计划/协议视图及旧视图撤销通过；证据保留旧快照并标记snapshot_valid=false，旧构建返回SNAPSHOT_REORGED。

报告位于最终验证记录（`integration/artifacts/local/weighted-exact-out-check/report.json`，已清理），代码与配置指纹 `43a2ed5278d7ca80ceaa4f4a47b89e3a02a70784d9b7225e62b6d83160d9e0ec`，参考语料SHA256 `061b67d3c2b3e43e875ca94ce72cda8b83d0b8ae46ba2838aa50774dfd573af9`。源码指纹覆盖未忽略的代码/配置/构建及校验工具，Markdown进度回填单独复核。原参考源码逐文件校验，294份实际加载的Node依赖摘要随生成记录保留；生成语料、日志、测试二进制和截图位于忽略目录。已查看多币非首币对页面，目标、实际消耗、单物理池和能力版本/11显示正确；下载/签名/广播门禁保持。最终测试进程退出，未提交公网交易。本批不升级完整链/DEX、独立生产执行或26工作包完整验收状态。


<a id="plan-2026-11-15-stable-exact-out"></a>

## Balancer标准Stable2精确输出（2026-11-15，阶段75）

按原六阶段顺序继续补已有明确解析状态的独立数学，不将单批完成当全目标完成。标准 `balancer-v2-stable2` 复用Go固定块读取与原StableState，不新增核心服务、独立图或数据协议。状态：`LOCAL_SCOPE_VERIFIED`（注明的原数学包装、API与前端本地范围）。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F75.1|StableMath独立GIVEN_OUT|WP12,WP25|PARTIAL_LOCAL|原invariant与输入余额Newton求解，补一scaled单位后反缩放向上、再加raw费用；uint256中间值及uint112现金限制，不套Weighted的30%上限。|
|F75.2|路由与顺序分配|WP14,WP15|PARTIAL_LOCAL|既有完整双币状态单池路线、双池四份额最小输入、同池逐次重算，内部Vault资金/多跳拒绝与失败原子性。|
|F75.3|API/计划/视图|WP15,WP16|PARTIAL_LOCAL|输出目标/maxSellAmount、数学计划与协议子视图绑定、旧报价证据保留和构建撤销，版本/12只升级明确数学能力。|
|F75.4|前端与回归|WP16,WP25|PARTIAL_LOCAL|真实Chrome双向金额和执行门禁、旧Weighted/V2/V3数学路由/API、固定块解析、Clippy与文档普通/-OO复验。|

参考使用锁定 `e281ff40583c39bf1d46c322334b2b13936e0e33` 原始StableMath求解与FixedPoint，以及原BalanceAllocation现金更新；harness为费用/缩放数学包装，不是完整StablePool/Vault部署和ERC20成交证明。Managed、Composable、BPT、rate等模型不借此开放。实际执行准备/unsigned/独立执行/钱包资格保持关闭，净链费用和任意连续拆分全局最优未验收。复验入口 `make -C integration stable-exact-out-current-check`。

实际目录61模型中33支持ExactOut、10具备执行准备；51个执行模板及其余数学/链组合按原设计继续。all_requested_features_complete=false、production_ready=false，全量六阶段继续进行中。


阶段75最终同源码有限验收：`make -C integration stable-exact-out-current-check` 的20项检查全部通过，48次Rust测试执行、4项真实Chrome双向金额/门禁、11项前端单测、8类篡改反例普通/-OO各通过，普通/-OO各17项进度文档检查通过。原数学参考720场景（396成功/324拒绝），45条序列180步、3组双池四份额网格、3组同池三次有序目标完整对照；同一冻结源码下回归旧Weighted/V2/V3数学、路由、分配与API。HTTP保存720份真实请求/响应和396个成功场景的预算减一控制，核对计划/协议视图与撤销后的snapshot_valid=false及SNAPSHOT_REORGED。4包Go固定块/配置/状态/wire、316份合同/12工具副本/164公共来源、公共协议及核心/计划/前端测试服务严格Clippy均通过。

代码/配置/校验工具指纹 `a665aa4a392dcc887d34f549f5a2af1be71a9732b6dfdc1d06d20b1909564485`，原参考语料SHA256 `9dd3f4a208488d9f233d64619b3b6785da300d89cce872e1b4f430ef4bed74f6`；748份日志/捕获/截图/语料/测试服务二进制摘要保留在最终验证记录（`integration/artifacts/local/stable-exact-out-check/report.json`，已清理）。Markdown进度回填不计入代码指纹，回填后单独执行文档校验。294份已加载Node依赖、源文件和语料副本均核对。原始StablePool._onSwapGivenOut与BaseGeneralPool._swapGivenOut的同revision调用顺序另作源码核对，与本包装一致，仍不宣称已执行完整StablePool/Vault/ERC20交易。已查看前端实际输出/输入/单池和禁用执行能力，测试进程退出，未向公网发交易。

后续主线继续：零hook Uniswap V4/Pancake Infinity CL 原始正数amountSpecified探针已在644场景中取得424完整输出及4条连续序列12步的结果，但尚未正式接入路由/API，不据此开放能力。原会话六阶段目标保持active；其他数学/链组合和51个执行模板继续未完成。all_requested_features_complete=false、production_ready=false。


<a id="plan-2026-11-15-singleton-exact-out"></a>

## 零hook Singleton CL精确输出（2026-11-15，阶段76）

阶段75完整本地验收后继续原六阶段目标，不结束整体验收。只覆盖已明确静态零hook的 `uniswap-v4` 与 `pancake-infinity-cl`。新增内部输出驱动核心，复用位图/tick/liquidity与方向协议费快照，保持物理池id为manager/poolId。当前状态：`LOCAL_SCOPE_VERIFIED`（静态零hook核心定价投影、路由/API与前端的有限本地范围）。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F76.1|正数目标输出数学|WP12,WP25|PARTIAL_LOCAL|独立目标输出步进、反向sqrt价格、逐word费用/空流动性/跨tick、signed BalanceDelta边界与完整目标拒绝；644原始核心场景和4序列数学预检通过，正式固定参考继续复验。|
|F76.2|单物理池路由与输入分配|WP14,WP15|PARTIAL_LOCAL|manager身份与单池路线、两费率池网格、同池连续目标、内部资金/缺位图与失败原子性已接入开发代码，待验证。|
|F76.3|计划/API/协议视图|WP15,WP16|PARTIAL_LOCAL|显式单池目标/输入预算、manager/poolId计划身份、QuoteView绑定和重组撤销已接入开发代码，未知/advanced模型继续拒绝，待验证。|
|F76.4|前端和统一验收|WP16,WP25|PARTIAL_LOCAL|双模型双向真实Chrome、旧Stable2/Weighted/V2/V3回归、原始拒绝和控制输入检查；待完成。|

原始参考锁定既有Uniswap V4/Pancake Infinity CL源码清单，直接执行未改的Pool/CLPool swap，原ExactIn为负数amountSpecified，输出目标改为原生正数。harness显式seed状态并运输实际BalanceDelta、价格/tick/liquidity，`remainingOut`表示未完成目标。正式文件位于 `integration/tools/reference/singleton-exact-out`，原参考/语料不改写。初始内部预检644场景中424完整输出及220部分目标/原始拒绝，4个真实同调用连续序列12步与独立重新seed一致；这不证明Manager/Vault/ERC20结算、完整LP费账、hook或交易执行。新能力仍待路由/API/前端与完整最终验证，不能因原型通过记完成。

实际能力目录为61模型、35个ExactOut（原33）、10个执行准备；双模型本批限定范围已验，六阶段目标active，all_requested_features_complete=false、production_ready=false。


阶段76最终同源码有限验收：统一入口 `make -C integration singleton-exact-out-current-check` 的20项检查全部通过，51次Rust测试执行与8项真实Chrome检查通过，含旧Stable2/Weighted/V2/V3数学、路由、分配/API回归。644个正数原核心场景为424完整输出、220拒绝（172部分目标/整数范围、48原始回滚）；4条原调用内混合方向序列12步、4条同池同方向序列12步、4组容量网格与独立seed重放一致。每组网格5份额中3项可行，浅池整单或过大份额原始部分成交保留为不可行控制。HTTP保存644个实际场景与424次预算减一控制，均为预期200/404 NO_ROUTE/400 EXACT_OUT_MAX_INPUT，旧视图证据失效及构建SNAPSHOT_REORGED通过。

10类篡改反例普通/-OO、进度文档各17项普通/-OO、11前端单测、Go固定块/配置/状态/wire、317合同/12工具副本/164公共来源及公共生成物和严格Clippy通过。代码/配置/验证工具指纹 `9714c5dca9466d94b09eb5155e90dd4745f263e7beff51ca7e496148ed210e41`，原核心语料SHA256 `60fae9ca232b4a2a9af76d9f07b9e8f66f36a8ff8d352cb2d6690ce3204f1835`；673份冻结日志、捕获、语料、截图与测试服务二进制摘要及302份已加载Node依赖核对在最终验证记录（`integration/artifacts/local/singleton-exact-out-check/report.json`，已清理）。源码指纹不包含Markdown，进度回填后单独复验文档。已查看前端manager/poolId、原始输入输出及未实现执行门禁。

模型只更新定价core，原始protocolAmount和swapFee保留在原参考；不将原Pool/CLPool的seed执行等同Manager unlock/资金结算、ERC20转账、完整LP费账、native/hooks/advanced或公开unsigned模板。以上仍按原设计继续开发；51个执行模板与其他协议/链/费用组合尚未完成。测试进程退出，无公网交易。all_requested_features_complete=false、production_ready=false；原会话六阶段目标保持active。


<a id="plan-2026-11-15-infinity-bin-exact-out"></a>

## 静态Infinity Bin精确输出（2026-11-15，阶段77）

阶段76已完成限定本地验收，继续原会话链/DEX解析与兑换量计算优先主线。`pancake-infinity-bin` 保持已有静态零hook完整树/固定块状态，输出接口直接按原getAmountsIn分bin反解输入，不反复调用ExactIn搜索。当前状态：`LOCAL_SCOPE_VERIFIED`（静态完整树数学/物理状态投影、路由/API与前端有限本地范围）。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F77.1|完整树目标输出|WP12,WP25|PARTIAL_LOCAL|目标min当前bin输出现金，Q128价格转换和fee向上取整，protocol从fee按比例向下；uint128局部/整数delta范围/流动性与步数检查；初步原20基础场景通过。|
|F77.2|burn脱离树active尘埃|WP12,WP25|PARTIAL_LOCAL|真实原burn后active不在树仍先处理尘埃；后续只遍历真实树成员，不制造虚假成员；48原burn输出场景初步通过。|
|F77.3|物理状态/路由/计划|WP14,WP15|PARTIAL_LOCAL|复用输入路径的共享后状态投影，gross app cash、net bin余额及协议费累计各自更新；已补同manager币种账本到兄弟物理池同步，复用ExactIn同一实现；单池路线与manager/poolId、双费率池及同池顺序分配，本批最终回归已通过。|
|F77.4|HTTP/前端/统一验收|WP16,WP25|PARTIAL_LOCAL|报价目标与预算、协议视图撤销、真实Chrome双向及能力门禁、旧协议回归与源码/参考完整性已通过本批复验。|

原参考锁定3c5ef1be原BinPool/BinHelper/TreeMath与原burn；新包装仅把swap amountSpecified改为正数目标并保留实际delta/后bin/树/手续费。App/Vault和全局protocol计数是明确的定价投影，不宣称完整Manager/Vault/ERC20/native结算；动态/hook/其他版本/执行模板仍未完成。生成物忽略，不修改既有原语料。复验入口 `make -C integration infinity-bin-exact-out-current-check`。

阶段77目录61模型、36个ExactOut（原35）、10个执行准备，新模型本批限定范围通过；51个执行模板与其他协议/链/净费用组合继续。all_requested_features_complete=false、production_ready=false，六阶段目标active。


阶段77最终同源码有限验收：统一入口 `make -C integration infinity-bin-exact-out-current-check` 的20项检查全部通过，72次Rust测试执行、4项真实Chrome、11前端单测、10类普通/-OO篡改反例及普通/-OO各17项进度文档检查通过。264个实际原BinPool场景219成功/45回滚，48个场景先执行原burn形成active脱树尘埃；18连续序列72步、2同方向顺序序列6步、2双费率池四份额网格通过。81个单步实际跨bin、82个成功单步含非零protocol fee；完整raw bin/tree/份额与delta保持一致，app和协议累计为明示投影。HTTP保存264份请求/响应及219预算减一控制；输入为1时预算减至0返回INVALID_REQUEST，其余不足预算返回EXACT_OUT_MAX_INPUT，原始失败均为404 NO_ROUTE。

新增共享manager币种账本同步回归：ExactIn与ExactOut复用同一传播函数，同manager相关app现金/protocol费用同步，兄弟池自身bins/active不改写，下一笔仍通过完整依赖验证；旧共享树/币种记账及Singleton/Stable2/Weighted/V2/V3数学、路由/分配/API一起回归。Go固定块解析/配置/状态/wire、319合同/12工具副本/164公共来源、公共生成物及核心/计划/前端测试服务严格Clippy通过。初期测试迁移字段名、未绑定模型bank和零预算控制的失败已修正，保留日志但不计为通过；生产bank/币种依赖校验未放宽。

源码指纹 `f3c22ffaccaf64bde18a61a8d77bcc1ffc40a8e824e7ffed31e3e03392381c81`，语料SHA256 `c24f144b0ae4934b6dc2cffb4543038708fa607366564a38a6dbef8c8b533d2d`；292份冻结产物摘要、302份已加载运行时模块、所有锁定原源码/语料副本、实际HTTP流程已核对，详见最终验证记录（`integration/artifacts/local/infinity-bin-exact-out-check/report.json`，已清理）。已查看Chrome双向金额及manager/poolId/禁用执行门禁。Markdown回填不计入代码指纹，回填后单独验证。原BinPool seed/burn/swap不升级完整Manager/Vault/ERC20资金结算，未知hook/动态/native与执行模板仍未实现。测试进程退出，无公网交易。原会话六阶段目标active；51执行模板和其余数学/链/净费用组合继续未完成，all_requested_features_complete=false、production_ready=false。


<a id="plan-2026-11-15-managed-snapshot-exact-out"></a>

阶段77报告中的37模式目录为随后Managed Snapshot开发代码加入后的回归计数；报告仅证明其标明的Infinity Bin与旧协议回归范围，Managed目标的独立验收另见阶段78，不计为本阶段交付。

## Managed Snapshot精确输出（2026-11-15，阶段78）

继续原会话六阶段主线的明确解析/数学范围，为 `balancer-v2-managed-snapshot/no-bpt-fees-1` 接入独立目标输出，不重归一化原始压缩权重。复用同一WeightedMath输出算术与30%输出上限，Managed的2–38币/正权重准入与静态2–20币/总和1e18/每权重≥1%分开。状态：`LOCAL_SCOPE_VERIFIED`（原回调、完整定价状态、路由/API和前端有限本地范围）。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F78.1|原始权重/费率目标数学|WP12,WP25|PARTIAL_LOCAL|真实ManagedPool schedule后读取9状态，180目标（108成功/72拒绝）；保留uint256/uint112、30%输出及缩放后加费顺序，零输出/开关/非零BPT费用拒绝。|
|F78.2|完整物理余额与币对|WP14,WP15|PARTIAL_LOCAL|完整3/8/38币物理向量、显式from_token；单池路线和同池有序分配，3双池网格/3顺序分配预检通过，非法索引/内部资金/多跳门禁继续验证。|
|F78.3|HTTP计划和快照|WP15,WP16|PARTIAL_LOCAL|同次实际blockHash/时间的权重/fee与报价快照一致，预算减一、计划token流/协议子视图和旧view撤销，API捕获本批最终联验通过。|
|F78.4|前端与可复验|WP16,WP25|PARTIAL_LOCAL|非首币对双向Chrome和执行门禁、旧Managed ExactIn/静态Weighted等回归、严格Clippy、普通/-OO反例及文档检查本批限定联验通过。|

原参考锁定e281ff40，执行未修改ManagedPool构造、公开渐变权重/费用更新、实际GIVEN_OUT回调和原BalanceAllocation。3/8/38币与三个时间范围读出的权重总和略低于1e18，不得归一化。原30%输出限制保持，非零protocol/management swap fee须BPT会计另验，不借此启用。Go既有同blockHash解析已要求swaps enabled与两项费用为0；不新增服务或wire字段。

Vault facade、合成模型地址/现金和显式unlimited contract size仅为原回调参考，不提供生产部署、完整Vault/ERC20资金转移、AUM/BPT执行或新的unsigned/钱包资格。复验入口 `make -C integration managed-snapshot-exact-out-current-check`；生成物忽略，实际进度只在本文件。

实际能力目录61模型中37个ExactOut（原36）、10个执行准备；阶段78本批有限范围已验，51个执行模板与其余数学/链/净费用范围继续。原会话目标active，all_requested_features_complete=false、production_ready=false。


阶段78最终同源码有限验收：`make -C integration managed-snapshot-exact-out-current-check` 共20项检查、41次Rust测试执行、4项真实Chrome和11前端单测通过。180个原ManagedPool GIVEN_OUT目标含108成功/72拒绝，9个实际权重/费用/区块快照（3/8/38币，三个时间范围），9条连续序列36步、3条同池顺序分配9步、3组双池四份额网格及3关闭swap控制通过。实际未归一化权重、向上舍入、30%输出边界和uint112最终现金均保持；静态Weighted仍要求严格总和和最小权重，旧Managed ExactIn与静态Weighted差分、V2/V3路由/API同时回归。

同一最终源码下保存180份实际HTTP请求/响应、108成功场景的预算减一、数学计划/协议视图绑定与旧视图撤销；失败严格为404 NO_ROUTE，预算不足为400 EXACT_OUT_MAX_INPUT，零预算按请求约束拒绝。10类篡改反例普通/-OO、普通/-OO各17项进度文档、4包Go固定块/配置/状态/wire、319合同/12工具副本/164公共来源、公共协议和核心/计划/前端测试服务严格Clippy通过。Chrome使用原参考完整录制的0↔2非首币对，报价正确且unsigned/签名/广播门禁保留。

代码/配置/验证工具指纹 `6e05bacf315c4c42a87af31192836fb5556f38882ac5234b3778df1fcb2f3dba`，语料SHA256 `7c2cd94ba41e715bdd539255c7f5c333db80b1705918a213e3b7999d3828a002`；207份日志/捕获/语料/截图/测试服务二进制和294份已加载Node模块摘要核对一致，详见最终验证记录（`integration/artifacts/local/managed-snapshot-exact-out-check/report.json`，已清理）。验收每次使用独立run目录并在中断时清理所属进程组；旧重叠日志因混写已作废，当前日志无NUL混写且HTTP捕获数量为强制检查。快照时间使用实际读取区块时间，不能用setTime请求值替代。Markdown进度回填不计入代码指纹，回填后单独复验。

此参考仅为原回调+测试Vault facade，显式unlimited code size与合成模型身份不升级生产部署、完整Vault/ERC20、非零BPT费用、AUM/其他Managed版本或执行模板。51个执行模板及其余数学/链/净费用范围继续未完成；原会话六阶段目标active，all_requested_features_complete=false、production_ready=false。测试服务退出，无公网交易。


<a id="plan-2026-11-15-composable-exact-out"></a>

## Composable普通币对精确输出（2026-11-15，阶段79）

阶段78注明范围已验后继续原六阶段主线，为 `balancer-v2-composable-regular/no-rates-1` 补普通underlying目标输出。BPT仍保留在完整注册向量和pool身份中，仅在原invariant/输入余额求解时移除并映射索引。状态：`LOCAL_SCOPE_VERIFIED`（原数学包装、普通币物理投影、路由/API与前端有限本地范围）。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F79.1|原StableMath目标输出|WP12,WP25|PARTIAL_LOCAL|e91a2b64原_calcInGivenOut、scaled补一/反缩放/费用向上，uint256/uint112边界；288目标和72序列参考，普通币输出不套Weighted30%限制。|
|F79.2|完整状态与BPT隔离|WP14,WP15|PARTIAL_LOCAL|BPT首/中/末索引、2/3/5 underlying和完整现金向量；Graph已有排除BPT，新增数学/边/计划/HTTP显式拒绝，BPT现金不改写，本批最终验证通过。|
|F79.3|路由/分配与API快照|WP14,WP15,WP16|PARTIAL_LOCAL|单池underlying路线、3双池网格/3同池顺序分配、预算与子QuoteView/计划、旧视图撤销，定向验证进行中。|
|F79.4|前端与统一回归|WP16,WP25|PARTIAL_LOCAL|普通币双向Chrome与执行门禁、旧Composable ExactIn/Managed/Weighted和V2/V3回归、严格Clippy/普通与优化反例/文档及真实HTTP捕获本批限定联验通过。|

参考只执行原始StableMath和FixedPoint，费用/缩放与显式uint112现金检查为测试包装；不加载未列入清单的其他源码，不宣称实际ComposableStablePool/Vault/ERC20交易。固定无rate-provider profile不扩展缓存/rate变化、BPT供应或待结协议费；BPT兑换仍需完整独立状态与执行证明。复验入口 `make -C integration composable-exact-out-current-check`，生成数据忽略。

实际目录61模型中38个ExactOut（原37）、10个执行准备；阶段79本批注明范围已验，51个执行模板和其他数学/链/净费用范围继续。原会话六阶段目标active，all_requested_features_complete=false、production_ready=false。


阶段79最终同源码有限验收：统一入口 `make -C integration composable-exact-out-current-check` 的20项检查全部通过，52次Rust测试执行、6项真实Chrome与11前端单测通过；288个原StableMath目标216成功/72拒绝、72条连续序列216步、3条同池顺序分配9步和3组双池四份额网格完整对照。2/3/5underlying加BPT，BPT首/中/末位置、不同小数/amp与40%输出均覆盖，BPT余额和其他注册顺序保持。原参考没有完整Pool/Vault/Token资金执行，显式现金边界为包装检查，不加载未列入原清单的BalanceAllocation。

HTTP保留288份真实请求/响应及216预算减一控制，成功目标/计划token流/协议视图与旧view撤销一致。额外36次BPT买卖HTTP控制为404 NO_ROUTE，图无BPT边，直接数学/注入路径/含BPT计划全部拒绝；Chrome普通币双向金额、BPT不在可选币种和执行门禁通过。10类篡改反例普通/-OO、普通/-OO各17项文档、4包Go固定块/配置/状态/wire、320合同/12工具副本/164公共来源及协议生成物、核心/计划/前端测试服务严格Clippy通过；旧Composable ExactIn、Managed/Weighted及V2/V3路由/API回归通过。

代码/配置/验证工具指纹 `0e7fc88d29920ca8972110f5dcab21351772fffab60be7fde25cf9b95a6672a1`，语料SHA256 `95bc1f5103a8921de4fc64714b422337f2730627f0ab5283fcfe3434e527a5f7`，315份冻结日志/捕获/语料/截图/测试服务二进制和294份已加载Node模块摘要核对，详见最终验证记录（`integration/artifacts/local/composable-exact-out-check/report.json`，已清理）。每次独立run目录、HTTP捕获数量/状态强制检查，Markdown回填后单独复验。已查看最终Chrome的BPT排除、普通币路线/金额与能力版本/16，测试服务退出，无公网交易。

目标输出只开放no-rates普通币模型；BPT供应/铸造/待结费用/缓存/rate与完整ComposableStablePool/Vault/ERC20资金执行仍需独立实现，执行准备/unsigned/钱包仍false。原会话六阶段目标继续active，51个执行模板和其余数学/链/净费用范围未完成，all_requested_features_complete=false、production_ready=false。后续执行链路已有Raydium经典SPL的WSOL补足/临时退款/解包原SBF探针，正式账户准备、独立模拟和钱包/回执验收继续推进。


<a id="plan-2026-11-15-raydium-funding"></a>

## Raydium ATA与WSOL资金准备（2026-11-15，阶段80）

阶段79注明范围已验后，继续已明确数学的CP-Swap经典SPL执行链路。复用classic_funding的显式ATA/WSOL/租金组件，Raydium原状态/代码/Clock/币种/费用校验保留，不新增服务或签名私钥。状态：`LOCAL_SCOPE_VERIFIED`，能力版本/17与新资金选项已通过本批固定范围的统一联验。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F80.1|经典ATA与资金指令|WP14,WP16|LOCAL_SCOPE_VERIFIED|显式缺失收款ATA、WSOL源ATA创建/补足、独立临时输入余款退款和输出解包；普通无选项wire继续单指令，禁关已有账户、未显式授权创建/包装与临时地址占用均有拒绝回归。|
|F80.2|完整同次捕获与独立执行|WP05,WP15,WP25|LOCAL_SCOPE_VERIFIED|额外System/ATA/可空ATA ProgramData/Rent同次读取，ATA原代码hash/System Native身份校验；10场景实际原SBF/RPC/独立模拟通过，断言独立模拟沿同捕获执行且不refetch。|
|F80.3|真实租金/资金与回滚|WP15,WP25|LOCAL_SCOPE_VERIFIED|10场景20次原始unsigned wire执行及限制反例通过；完整Pool/Vault/原Oracle、独立后账户摘要、实际费用/租金/预存补差核对，失败除网络费外全账户回滚。|
|F80.4|钱包/回执与前端|WP16,WP25|LOCAL_SCOPE_VERIFIED|10场景服务签名/单次loopback发送/真实费用租金和重启回执、4失败发送不重发通过；155项真实Chrome（40新增Ray资金）及旧资金/捕获/回执回归通过，浏览器仅传公开场景元数据。|

原生程序仍固定既有CP-Swap源码/bytecode，经典Token程序与原Pool账本/Oracle保持。新增Runtime的System/ATA/Rent账户来自已锁定本地原运行时，RPC fixture明确返回其真实字节而非让独立VM静默补默认值；这不构成历史银行共识证明。ATA SHA256=6804554e...与既有生产校验常量一致，System Native账户身份按原运行时匹配，ProgramData缺失/不可执行/代码变化按既定边界拒绝。

独立原参考按实际下载wire执行，逐项校验签名槽/权限/指令角色/资金顺序、捕获hash、Pool完整字节/原Oracle/原Vault、用户扣付/到账与原SBF后账户摘要。最大输入减一或最小输出加一在原程序失败，全账户创建/包装/兑换回滚，只扣实际网络费。钱包提交仍需要用户签名及独立确认，默认生产配置关闭；测试仅loopback原SBF，没有公网交易。

最终统一验收：21项检查、26次Rust测试执行、155项真实Chrome（40项新增Raydium资金场景），10份实际服务准备/10份重启回执及20次原始交易验证通过。14项证据篡改控制分别在普通和`-OO`模式通过；严格Clippy、公共协议、契约副本、旧Pump资金与捕获/回执回归通过。源码指纹`d0c8975b03aa392bd14afb4ca96fc5a6b275d4af71ee5f06577e506553e6de1f`，398份产物摘要绑定独立目录`integration/artifacts/local/raydium-funding-check/runs/ee0783ded2f441b58ae9385c4901f30d`；最终报告（`integration/artifacts/local/raydium-funding-check/report.json`，已清理）。浏览器场景元数据移除完整原账户/程序和after字节，服务持有的同次捕获与原生证据保持完整。

实际ExactOut能力目录仍38/61、执行准备10/61；Raydium在原有准备能力内增加account_creation/native_funding，原生SOL直接交易与Token2022/其他Raydium版本不借此开放。51个执行模板与其他数学/链/净费用组合继续；原会话六阶段目标active，all_requested_features_complete=false、production_ready=false。

<a id="plan-2026-11-15-orca-execution"></a>

## Orca经典SPL执行链路（2026-11-15，阶段81）

沿既有Orca精确数学和阶段80已验资金组件，补齐经典静态/奖励版的单池执行准备、独立模拟、钱包和回执。状态：`LOCAL_SCOPE_VERIFIED`；三服务与公共协议职责不变。能力目录/18新增两项注明范围的执行准备能力，本批通过本地原程序验收；生产或历史bank资格仍未开放。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F81.1|原始指令和全状态原参考|WP12,WP14,WP15|LOCAL_SCOPE_VERIFIED|32场景64次实际服务unsigned wire原Whirlpool/Token SBF执行通过；完整Pool、所有tick数组、双Vault全部字节与保留SOL捐赠、空Oracle/只读账户及失败全回滚核对。静态奖励清除/动态转换为显式控制。额外8次静态/动态双向跨tick原SBF探针单列于报告产物，未并入32服务场景。|
|F81.2|账户准备和unsigned交易|WP05,WP14,WP16|LOCAL_SCOPE_VERIFIED|完整Pool/tick编码与PDA/代码/Clock/config/mint/vault逐项校验；1/2/3方向数组窗口重复最后实际账户填槽、原程序去重；ComputeBudget原账户和1400000单位指令纳入捕获/交易，容量不足运行时明确拒绝。|
|F81.3|资金组合与独立捕获|WP05,WP15,WP16|LOCAL_SCOPE_VERIFIED|32实际服务准备、独立SBF同捕获不refetch通过；固定/动态、静态/奖励、双向ExactIn/Out、跨tick、ATA/WSOL补足/临时退款解包及预存租金均验。程序/config/mint/array/Oracle/epoch/Vault背书/资金选项和临时地址污染按边界拒绝。|
|F81.4|钱包/回执与前端|WP16,WP25|LOCAL_SCOPE_VERIFIED|32实际单次发送/真实费用租金/重启回执和6发送失败不重发通过；20新场景81项真实Chrome，连同旧流程共236项Chrome。独立模拟后只一次账户读取，发送前另一次实时预检，刷新回执不再取账户或重发。原数学专用银行不能借用他池账户。|
|F81.5|严格回归与进度|WP25|LOCAL_SCOPE_VERIFIED|25统一检查/88次Rust测试执行通过，含原数学/路由/Parser、旧Pump/Raydium资金、账户捕获/回执、严格API与核心Clippy、构建和前端单元；16证据控制普通/-OO及17文档控制普通/-OO通过，冻结源/银行/运行时和全部产物摘要。|

最终统一验收：25检查、88次Rust执行、236项真实Chrome（81新增Orca）通过，32份实际准备/32份重启回执及64次原始交易验证完成；16项证据篡改控制普通/-OO通过。源码指纹`355880d339d0785cd80d5348800b85551957e48e635fa481ac6ef702b25e1137`，863份产物摘要绑定独立目录`integration/artifacts/local/orca-execution-check/runs/1e2bc953f8c146cb9b47525971e565ce`；最终报告（`integration/artifacts/local/orca-execution-check/report.json`，已清理）。原始程序、运行时和银行输入摘要全部复核，浏览器最终截图已检查。

原始指令探针（`integration/artifacts/local/orca-execution-probe.json`，已清理）仅证明其注明的本地原程序范围，不能代替完整API/钱包/前端验收。adaptive动态费/奖励组合、Token2022与其他程序版本按独立适配范围处理，未知扩展继续明确拒绝。原会话整体目标保持active，all_requested_features_complete=false、production_ready=false。


后续执行范围的独立探针：三个既有Orca自适应模型的实际原SBF双向ExactIn/Out共12场景通过，逐字节核对Pool、初始化Oracle、Vault与所有数组；规范化slot/ProgramData/Token身份探针（`integration/artifacts/local/orca-adaptive-execution-normalized-probe.json`，已清理）与重新计算PDA的WSOL受控池探针（`integration/artifacts/local/orca-adaptive-execution-wsol-probe.json`，已清理）分别保留原生证据。它们采用显式受控状态和既有实际程序字节，不代表公开池观察或历史bank共识；自适应执行适配、API、资金组合、钱包和Chrome仍待独立实现验收，本批能力标签不因此扩展。


后续自适应执行银行草稿已扩展至54场景：三模型各18场景，包含普通双币、WSOL双币、双向ExactIn/Out、显式ATA/WSOL资金、临时退款/输出解包及预存租金。银行生成草稿证据（`integration/artifacts/local/orca-adaptive-execution-bank-draft.json`，已清理）与54次成功原SBF交易探针（`integration/artifacts/local/orca-adaptive-bank-sbf-probe.json`，已清理）已保留，最大wire为903字节，Pool/初始化Oracle/Vault/所有tick数组的实际后字节与新鲜原生参考相符。这些交易由独立探针构建，尚不是Router API产出的交易；适配草稿只在独立目录检查，当前统一验收源码和能力范围不变。


自适应草稿的54场景／108次原交易控制（`integration/artifacts/local/orca-adaptive-bank-sbf-controls.json`，已清理）进一步通过：ExactOut最大输入减一、ExactIn最小输出加一在原程序失败，所有已捕获及交易使用账户除payer实际5000网络费外恢复原值；成功时实际payer变化逐项核对包装、退租、未用输入退款、解包收款及预存租金补差。此控制仍只证明独立构建的本地交易，不代替Router产出wire、同次捕获、钱包和前端的后续验收。


自适应独立模拟草稿新增54场景实际worker入口验证（`integration/artifacts/local/orca-adaptive-sbf-worker-probe.json`，已清理）：通过与正式worker相同的有界stdin、unsigned message和RPC捕获结构调用原SBF，核对实际输入／输出、payer变化、wire摘要和源银行不变；三个模型将初始化Oracle改为只读时均拒绝。完整254字节Oracle所有字段及16/16/128预留区的篡改、缺失、owner/可执行标志、绑定和长度控制已在普通/-OO模式下通过。该worker仍只在草稿目录运行，当前Router生产源码和能力标签保持阶段81冻结范围。


自适应组件草稿的54份真实resolver准备与同捕获worker结果、72项Oracle拒绝控制（`integration/artifacts/local/orca-adaptive-component-draft-verification.json`，已清理）已完成：每场景通过真实Rust resolver读一次有界RPC账户、产生unsigned交易，再由草稿worker复用捕获执行原SBF；实际金额、资金/租金、消息hash和Oracle写权限均通过。初始化Oracle全部字段/预留区篡改及owner/长度/可执行标志/缺失均阻止交易产出。输入源码、草稿二进制和54份产物摘要已单独绑定；这些验证不含正式QuoteView缓存、HTTP API、钱包回执或Chrome准入，下一阶段仍需完成集成。

当前能力目录：61模型中38项ExactOut、12项执行准备；本批新增Orca静态/奖励2项，剩余49个执行模板及其他数学/链/净费用组合继续开发。所有26工作包仍未获完整设计验收，all_requested_features_complete=false、production_ready=false，原会话六阶段目标保持active。


<a id="plan-2026-11-15-orca-adaptive-execution"></a>

## Orca自适应SPL执行链路（2026-11-15，阶段82）

本批于2026-11-15启动，2026-11-16继续预检与正式验收；日期不代表历史bank或当前主网状态。

阶段81的25门禁/88Rust/236Chrome正式完成后，继续三个已具备精确数学的自适应经典SPL模型：adaptive、adaptive-rewards、adaptive-dynamic。当前状态：`LOCAL_SCOPE_VERIFIED`；能力目录/19的三个自适应准备/资金/模拟模板已通过本批完整本地验收，生产资格与历史bank共识声明保持关闭。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F82.1|初始化Oracle与真实程序绑定|WP05,WP14,WP15|LOCAL_SCOPE_VERIFIED|完整254字节Oracle/Whirlpool绑定/16、16、128预留区逐字节验证，原legacy swap初始化Oracle可写；原程序和Token代码/部署slot、PDA/Clock及经典资产身份通过；错误/缺失/readonly Oracle明确拒绝。|
|F82.2|资金与完整独立SBF|WP14,WP15,WP16|LOCAL_SCOPE_VERIFIED|54份实际API准备和同捕获独立SBF通过，普通币对/WSOL、双向ExactIn/Out、固定/混合动态数组、ATA/临时退款/解包/预存租金均验；108次实际下载wire原SBF全Pool/Oracle/数组/Vault与全回滚核对通过。|
|F82.3|钱包和回执|WP16,WP25|LOCAL_SCOPE_VERIFIED|54单次签名发送/真实费用租金与用户收付款/重启恢复回执、12失败发送不重发通过；WSOL背书拒绝使用实际WSOL银行验证，普通SPL的lamports不误当Token背书。|
|F82.4|前端与门禁|WP16,WP25|LOCAL_SCOPE_VERIFIED|30场景121项新Chrome及原静态Orca81/旧流程155，共357项真实Chrome通过；28统一门禁/95次Rust执行、原数学/Parser/严格lint/构建、19证据控制及6Oracle worker控制普通/-OO和17文档控制普通/-OO通过。|

阶段81最终报告（`integration/artifacts/local/orca-execution-check/report.json`，已清理）保留了上一批冻结源码、运行时及863产物摘要，两份实际浏览器二进制已存入独立运行目录以供后续重建后的复查。阶段82的银行归档为独立`orca-adaptive-execution-bank.json`，原静态/奖励银行和数学语料不覆盖；受控slot/资产替换/PDA重算与真实程序代码边界均明确记录。当前数学ExactOut仍38/61，准备代码新增3模型到15/61但本批资格待完成；剩余46模板与其他数学/链/净费用范围继续，all_requested_features_complete=false、production_ready=false，原六阶段目标保持active。


阶段82当前正式预检已产出42份API准备和11份重启回执；首批8份实际下载wire的16次外部原SBF成功/限制反例通过。6项初始化Oracle worker控制和16项证据控制均在普通/-OO通过，严格Clippy与自适应浏览器harness构建通过；54场景API组和30场景Chrome预检继续进行。状态拒绝测试中将WSOL背书用于普通SPL的断言已修正到实际WSOL银行，修正用例需待当前测试组结束后重编译复验，未计作通过。


2026-11-16预检更新：30场景121项真实Chrome已通过，54份正式API准备均完成；54份重启回执仍在运行。当前整组测试保留了修正前的普通SPL/WSOL背书断言失败，修正源码已就绪，需在进程终止后单独重编译复验。完整实际wire原SBF重放进行中。证据控制扩至19项并在普通/-OO通过，新增重复ProgramData账户的有界存储复用检查；owner/lamports不同不合并、原始snapshot hash保持不变。


后续Raydium CLMM执行的16场景／32次原交易探针（`integration/artifacts/local/rayclmm-execution-initial-evidence.json`，已清理）于2026-11-16通过，覆盖保留来源的奖励版、显式清除奖励的静态版及Token0/Token1协议收费分支，双向ExactIn/ExactOut与输入/输出限制失败均验。Pool/Observation/全部已声明数组、双Vault全部Token字节和WSOL原有SOL余额变化、用户实际收付与全账户失败回滚和5000网络费相符。既有来源缺少Token2022 ProgramData，探针显式附加锁定本地运行时的Token2022完整闭包，Memo按保留原字节显式载入，不把VM隐式缓存当来源；不代表Token2022资产扩展或历史bank共识。Raydium CLMM的Router执行准备、同次捕获、钱包和前端尚未接入，本批Orca冻结源码与能力标签不变。


Raydium CLMM后续受控银行草稿已扩展为56场景：奖励/静态/两种协议收费方向各14场景，覆盖双向ExactIn/Out、显式ATA/WSOL、临时退款/输出解包和预存租金。其112次独立原SBF交易控制（`integration/artifacts/local/rayclmm-bank-sbf-controls.json`，已清理）通过，Pool/Observation/声明数组/双Vault完整字节及原有SOL余额、用户实际收款、payer费用/租金与失败全账户回滚均核对。该探针自行构建交易，尚不是Router API产出的wire；下一批仍需完整账户验证、同次捕获、钱包与前端接入，当前Orca能力范围未扩大。


Raydium CLMM准备与独立模拟组件草稿验证（`integration/artifacts/local/rayclmm-component-draft-verification.json`，已清理）已通过56场景：真实Rust resolver仅一次读取完整有界RPC、核对Pool/Observation/数组/配置/PDA/代码/Clock与资产后生成unsigned wire，外部原SBF重放112次成功/限制失败并验证实际资金租金和全回滚；同次捕获再交草稿worker独立模拟56次，源银行不变。56项resolver拒绝控制与8组worker普通/-OO控制通过，经济字段和未知扩展被拒绝，原程序允许的历史opaque填充及无挂单的order cohort保留。附属Memo与Token2022程序按显式字节加载并核对，未开放Token2022资产。草稿输入、二进制和56份产物摘要独立绑定；正式Router/QuoteView、钱包/回执及Chrome仍待下一批实现。


阶段82统一冻结验收当前已通过54份正式准备、54份恢复回执、108次实际wire原SBF重放、19项证据控制和6项Oracle worker控制的普通/-OO门禁，旧Orca准备/钱包回归也已通过；其余旧资金/捕获/解析/前端和最终摘要门禁仍进行中，因此阶段状态不升级。当前源码及全部68份运行时/银行/原生参考输入摘要持续一致。

阶段82最终验证完成于2026-11-16：28门禁、95次Rust执行、357项真实Chrome、54份实际API准备/54份恢复回执及108次原交易全部通过。源码指纹`dc54e6df7c6f30fe1d32a36ccb5840481fe6be64fff0f801915142b7caa838eb`，1458份产物摘要与运行时/银行/三个原生参考程序输入绑定独立目录`integration/artifacts/local/orca-adaptive-execution-check/runs/14207d2b93c547a38cca48d90baf92c0`；最终报告（`integration/artifacts/local/orca-adaptive-execution-check/report.json`，已清理）。19项证据控制含大型ProgramData的无损内存复用：完整账户内容相同才共享，owner/lamports变化与原始快照hash均独立保留；6项Oracle全部字段/预留区拒绝控制在普通/-OO均通过。失败预检日志与修正回归保留，最终截图已检查。

当前注明范围能力为61模型中38项ExactOut、15项执行准备；剩余46执行模板及其他数学/链/净费用范围继续。26工作包完整设计验收仍为0，all_requested_features_complete=false、production_ready=false，原会话六阶段目标保持active。


<a id="plan-2026-11-16-rayclmm-execution"></a>

## Raydium CLMM经典SPL执行（2026-11-16，阶段83）

阶段82完成后继续固定6d7a0f82经典SPL的静态、奖励、Token-fee模型。状态：`LOCAL_SCOPE_VERIFIED`。正式执行准备、同次捕获SBF、代码准入和能力目录/20通过本批完整本地验收；bitmap/custom-seed/dynamic及Token2022资产扩展按独立模型拒绝。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F83.1|完整账户与原程序|WP05,WP14,WP15|LOCAL_SCOPE_VERIFIED|完整Pool1544/Observation4483/数组10240、config/PDA/Clock/程序/mint/vault和源WSOL捐赠逐项核对通过；原opaque历史填充与无挂单order cohort保留，未支持订单/动态/保留扩展拒绝，实际原程序全后状态及回滚通过。|
|F83.2|unsigned与显式资金|WP14,WP16|LOCAL_SCOPE_VERIFIED|原swap_v2、显式ComputeBudget、实际方向数组和单签名通过；显式ATA/WSOL补足/临时退款解包/预存租金与真实payer变化匹配。来源Memo和锁定Token2022完整代码闭包明确加载校验，经典资产仍仅Tokenkeg。|
|F83.3|API/独立模拟/钱包回执|WP15,WP16,WP25|LOCAL_SCOPE_VERIFIED|56实际API准备/同捕获独立SBF不refetch、56签名发送/真实费用租金/重启回执和16失败发送不重发通过；112实际下载wire原SBF检查完整Pool/Observation/全部数组/Vault/用户收付款与失败全账户回滚。|
|F83.4|前端和统一验收|WP16,WP25|LOCAL_SCOPE_VERIFIED|31统一冻结门禁/135次Rust执行/486项真实Chrome（32新场景129检查）通过，含旧报价零动态账户读取、Parser/原数学/严格Clippy、23证据/8worker控制普通/-OO及17文档控制普通/-OO；源码/运行时与全部产物摘要绑定。|

首次预检保留失败日志：未设置锁定Python运行时、Cargo未用rustup入口，以及新故障注入尚未接入/Token2022 ProgramData地址误写，均未计作通过；生产程序校验未因此放宽。当前18/61准备为注明范围已验收能力，仍不等于生产或完整协议资格；38/61 ExactOut不变，剩余43执行模板及其他数学/链/净费用范围仍未完成，all_requested_features_complete=false、production_ready=false，原会话整体目标active。


阶段83预检更新：修正后的56份准备/同捕获SBF独立用例通过，56份钱包单次发送及重启回执用例通过，完整状态/资金选项/16失败发送不重发/运行时容量拒绝均通过。一次整组预检在第36场景正确拒绝过期报价，浏览器早期复验记录准备状态变化和第43场景独立模拟超时，均未计作成功且未放宽生产有效期/超时。后续单独复验32场景129项真实Chrome全部通过，明确检查实际准备和独立模拟HTTP响应。31项统一冻结验收已启动，要求56份当前API准备/56回执、112次实际wire原程序和四组共486项Chrome；门禁未全部完成前本批仍IN_PROGRESS。


后续Raydium三个高级模型的独立组件草稿证据（`integration/artifacts/local/rayclmm-advanced-component-draft-verification.json`，已清理）已扩展至162场景：Bitmap、自定义seed、动态费各覆盖三种收费模式、普通币对和明确重算PDA的WSOL币对、双向ExactIn/Out、ATA/补足/临时退款解包和预存租金。324次独立原SBF成功/限制失败验证通过，完整Pool/Observation/数组/扩展/Vault及资金租金与失败全回滚相符；162次真实Rust准备生成wire并沿同捕获交草稿worker模拟通过。新增完整1832字节扩展/seed及34字节动态费状态校验，9组worker普通/-OO控制通过。源码草稿、独立二进制和证据摘要已绑定；当前生产代码仍只开放阶段83冻结范围，后续正式API/钱包回执/Chrome尚待接入。


2026-11-16中断后核查：上一轮验收父进程已不存在，测试日志显示5通过/2失败（`SBF_WORKER_IO`），其遗留RUNNING报告已按实际证据改为FAILED，未猜测未保留的进程退出码。独立复验56场景准备/同捕获模拟通过。原临时目录参考程序缺失，按仓库锁定离线构建脚本恢复后，新二进制摘要与旧摘要分别保留；完整1546个原请求重放（`integration/artifacts/local/rayclmm-restored-reference-replay.json`，已清理）与225份上游源文件检查通过，1266报价/280数值结果全部与原记录一致，不据此声称二进制字节相同。当前新冻结验收重新绑定实际参考和银行输入，仍待所有门禁完成。验收中断的SIGINT/SIGTERM报告与子进程组清理2项控制分别在普通/-OO通过；交易负控独立构建函数新增4项回归后，证据控制为23项普通/-OO通过。

阶段83最终统一结果：31门禁、135次Rust执行、486项真实Chrome、56实际准备/56恢复回执/112实际wire原SBF全部通过。源码指纹`db6e4b21429a193ce6749d0e8ab990454037951d2baaa2273521bd03b989723f`，1761份产物摘要与运行时/银行/实际原参考绑定独立目录`integration/artifacts/local/rayclmm-execution-check/runs/2a6bb3e7814b43ce804534b5456ae362`；最终报告（`integration/artifacts/local/rayclmm-execution-check/report.json`，已清理）。最终完成截图和全部摘要已检查。失败预检均保留：fixture故障注入遗漏/地址误写已修正，过期报价和模拟超时均未被计为通过，也未放宽生产有效期或超时；新冻结运行独立证明完整结果。

原始目标保持完整：38/61 ExactOut、18/61注明范围执行准备，仍有43执行模板和其他链/数学/净费用组合待完成；26工作包完整设计验收仍为0，all_requested_features_complete=false、production_ready=false，六阶段目标active。


<a id="plan-2026-11-16-rayclmm-advanced-execution"></a>

## Raydium CLMM高级经典SPL执行（2026-11-16，阶段84）

在阶段83的31门禁/135Rust/486Chrome全部通过并保留1761产物及62输入摘要后，继续已有精确数学的Bitmap、自定义seed和动态费三个经典SPL模型。当前状态：`LOCAL_SCOPE_VERIFIED`；能力代码/21的三个新增经典SPL执行模板已通过整批本地验收，生产与历史bank资格仍关闭。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F84.1|扩展/seed/动态费身份|WP05,WP14,WP15|LOCAL_SCOPE_VERIFIED|完整1832字节Bitmap扩展/池绑定/112+112 words与PDA、非零seed的u16LE Pool PDA及bump、动态费34字节全部字段和1130后保留区校验；附属Token2022/Memo闭包沿原边界，未知扩展继续拒绝。|
|F84.2|原交易与资金组合|WP14,WP15,WP16|LOCAL_SCOPE_VERIFIED|162受控银行覆盖三个模型各三收费模式、普通币对/WSOL、双向ExactIn/Out及资金/预存租金；324次正式API wire原SBF重放通过。|
|F84.3|API与钱包回执|WP16,WP25|LOCAL_SCOPE_VERIFIED|162正式API准备/同捕获SBF、162单次发送及重启回执，发送失败不重发与状态污染拒绝通过。|
|F84.4|前端/原参考/统一门禁|WP12,WP16,WP25|LOCAL_SCOPE_VERIFIED|45高级场景181项Chrome与既有经典SPL/Orca/Pump回归，共667项Chrome；33项最终门禁及306次Rust通过。|

当前受控银行引用已恢复并完整重放1546原请求的当前源码参考，生成时重建所有WSOL Pool/Vault/Observation/array/extension PDA；未声称这些是公开历史池。程序与Token经典资产限制不变，订单/Token2022资产和未声明组合继续拒绝。当前21/61准备为代码接入范围，其中新增3项资格待完成；38/61 ExactOut不变，剩余40执行模板与其他链/数学/净费用范围未完成，all_requested_features_complete=false、production_ready=false，原会话六阶段目标保持active。


阶段84正式预检当前已生成82份API准备，完整状态污染拒绝、资金选项显式授权及运行时容量检查通过，162场景的准备/SBF和钱包回执组仍运行中。首批8份实际API wire的16次外部原SBF成功/限制失败重放通过，覆盖普通资产、WSOL、ATA、退款与预存租金；完整Bitmap扩展及Pool/Observation/数组/Vault/费用租金/失败全回滚核对相符。严格API Clippy、23证据控制普通/-OO、9Oracle/扩展worker控制普通/-OO及17项进度文档门禁通过。45场景181Chrome测试程序已构建但尚未运行，32项最终冻结驱动已准备但尚未启动，不将这些草稿/局部结果视为完整阶段验收。


阶段84实际准备审计：162份API准备／独立证据承诺、unsigned wire摘要、扩展账户位于原指令剩余账户首位且可写、三模型各三种收费模式均已核对；审计记录（`integration/artifacts/local/rayclmm-advanced-current-preparation-audit.json`，已清理）明确记录核对时的部分回执数量，不作162份钱包完成声明。实际API原wire矩阵（`integration/artifacts/local/rayclmm-advanced-original-model-matrix.json`，已清理）以18场景36次原交易覆盖三模型/三费模式和各类资金操作通过，是完整324次最终重放前的限定子集。当前长测试只剩162场景钱包回执循环，暂无失败。


阶段84完整实际wire预检：162份正式API下载交易已完成324次原SBF成功／限制失败重放，完整Pool、Observation、全部声明数组和Bitmap扩展、双Vault/原SOL余额、用户实际收付、payer费用/租金及失败全账户回滚均通过。原程序完整记录（`integration/artifacts/local/rayclmm-advanced-original-full-preflight.json`，已清理）保留真实捕获批次、交易/准备/独立证据摘要。162份钱包回执预检和45场景浏览器仍在运行，阶段84不借此提前升级完成状态。


阶段84正式预检已全部齐备：162份API准备/独立SBF与162份实际单次发送/重启恢复回执通过，324次实际wire原程序成功/限制失败、23项证据控制/9组worker控制普通/-OO、严格Clippy及45场景181项Chrome全部通过。预检截图已检查，独立模拟/真实回执的输出与手续费、租金相符。32项最终冻结验收已从新目录启动，重新要求162当前准备/162回执/324原交易及五组共667项Chrome；源码和65份运行时/参考输入锁定，未全部通过前仍保持本批IN_PROGRESS。


后续EVM执行链路的原Router02离线签名／费用探针（`integration/artifacts/local/evm-router02-wallet-probe.json`，已清理）通过8次EIP1559原交易：双向ExactIn/ExactOut各含成功和限制失败，输入/输出均大于u64范围，未借用Solana金额宽度。原公开Factory/Pair/Router02/ERC20字节与pins校验、chain ID/nonce/目标/calldata/签名恢复/零ETH交易值绑定通过；每笔仅一次原始交易发送，回执恢复无重发，实际gasUsed×effectiveGasPrice与payer费用一致，成功时用户收付/双Vault和Swap日志核对，限制失败时全部合约余额与Pool状态回滚但扣实际网络费。只在本地受控VM与测试钱包运行，未提交公网交易，也尚未接入正式EVM准备、QuoteView绑定、独立生产验证或前端；该证据不扩大本批Orca/Raydium能力标签。


后续EVM组件的独立准备草稿验证（`integration/artifacts/local/evm-preparation-component-draft-verification.json`，已清理）已通过6组Rust控制及3组ethers编码／离线签名互验。组件重建现有单池ExactIn的完整BoundEvmExecution承诺，保留uint256金额，核对chain ID/pending nonce/源快照/caller/token/spender及费用预算；余额／授权不足不生成unsigned wire，也不自动插入approve。EIP1559九字段unsigned编码与独立ethers逐字节一致，原Router02 ABI核对调用参数并恢复指定测试账户签名，额外覆盖零nonce/priority与高于u128的费用字段。这些使用组件调用方提供的观察，不含实际RPC捕获、共识／部署验证、ExactOut准备、独立REVM执行、QuoteView/API缓存、正式钱包／回执或前端集成；未据此开放EVM能力或缩小原始目标。

阶段84首次冻结运行的API组以6通过/1失败结束：第132个钱包场景的独立模拟超时，失败前131份回执成功，失败场景未发送。该失败报告保留，生产超时和有效期未调整。下一轮以完整相同场景集串行运行，并仅在进程生命周期内禁止系统空闲休眠，未加入自动重试或削减测试；执行环境单独写入新运行目录，当前阶段84仍IN_PROGRESS。

阶段84最终冻结验收已完成：33项检查全部成功，306次Rust测试执行、162份正式API准备、162份单次发送及重启恢复回执、324次实际wire原SBF交易、五组共667项Chrome（其中高级模型181项）。2026-11-16复核源码指纹及全部65份运行时/参考输入与最终报告完全一致；最终运行目录为`166590019da046ed95e007aa4cdd8186`，保留3917项产物摘要。此前独立模拟超时的失败报告继续保留；新完整运行未减少场景或放宽生产超时。21/61执行准备、38/61 ExactOut为当前限定范围，剩余40种执行模板、其他数学及真实部署/历史bank/生产验证继续待完成。

<a id="plan-2026-11-16-incremental-graph"></a>

## 候选图增量索引与COW（2026-11-16，阶段85）

对照设计§28.14第2项发现：数值更新已复用整个Graph，但任一池拓扑或准入变化都会全图重建。本批引入按物理池维护的方向边索引和Arc共享邻接边，增删池、币种或准入变化只复制受影响的token邻接边；完整银行依赖与共享Vault准入仍逐批核对。池顺序变化保持与完整构图相同的候选顺序与预算语义，旧QuoteView的图内容保持不变。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F85.1|物理池增量边索引与不可变共享|WP09,WP12|LOCAL_SCOPE_VERIFIED|状态更新、增删池、币种/多资产变化、空图与恢复、重复池/跨链/坏状态拒绝。|
|F85.2|候选顺序与全量构图对照|WP12|LOCAL_SCOPE_VERIFIED|128次连续变更及紧展开预算；任意变更后与完整构图的边、准入和顺序一致，未触及邻接边共享内存。|
|F85.3|状态副本原子发布与回归|WP09,WP12|LOCAL_SCOPE_VERIFIED|旧句柄、图版本、准入撤销、共享Vault、重组、路由/分配/API及严格Clippy；复验命令`make -C integration incremental-graph-current-check`。|

本批范围为普通提交的候选图索引和邻接边COW；阶段85验收时基线安装/恢复与scope重锚仍使用完整构图，已由阶段86接入同一增量索引。池状态向量、顶层索引映射仍需随完整银行处理，不声称O(变更池数)整批状态发布。beam多标签、通用共享DAG、严格收益界、慢句柄内存压力及生产容量仍未完成；生产与全部功能完成标志保持false。

阶段85最终验证：8项门禁/249次Rust测试通过，源码和341份测试输入同批绑定。新增9项路由回归和1项副本回归覆盖局部复制、128次连续变更、256池中510个未触及邻接桶共享、旧句柄和紧预算候选顺序；现有默认路由/副本/分配器及指定API回归通过（需要额外认证实VM向量的ignored专项仍按原条件保留，不计本轮通过数），四crate全目标严格Clippy、契约及17项普通/优化进度文档检查通过。此前全目标Clippy发现的重复dead_code测试属性已修复，失败日志保留。本批不改变协议能力目录、生产独立验证或全功能完成标志。

<a id="latest-code-gap-audit-2026-11-16"></a>

### 本次设计与最新代码复核说明（2026-11-16）

本轮核对发现三类文档漂移：阶段84的最终冻结报告已通过但当前状态仍写IN_PROGRESS；WP04/WP05/WP20/WP24及R02–R04摘要仍把已经接入normal服务的provider/WS、staging、工厂目录、raw/DLQ、Yellowstone/WAL和scope控制列作未实现。协议矩阵还把阶段84三个已通过执行验收的模型列作NOT_IMPLEMENTED，现已按限定范围改为PARTIAL_LOCAL。现按已有详细复核与实际调用链修正摘要，不扩大真实公开来源、长期保留、完整共识/银行或生产资格声明。执行准备仍为21/61，ExactOut为38/61；这些是限定协议能力覆盖，不能替代原26工作包的完整验收。候选图全量重建是本次新确认并开发的代码差距，详见阶段85。

### 本次最新代码的后续功能清单（2026-11-16）

以下为直接代码核对后的后续实施顺序；已经接线但待真实来源/生产验收的工作继续按R01–R04详细表管理，不重复视作从零开发。

|顺序|设计要求 / 工作包|最新代码差距|下一步与完成门槛|
|---|---|---|---|
|1|§28.14.2 / WP09,WP12|普通提交及阶段86的基线/恢复/scope重锚已接增量图复用|局部与完整构图、128变更/256池共享和统一回归通过；完整池状态COW与规模化压力继续待补|
|2|§28.11 / WP09,WP16|阶段86已接三类缓存的闲时回收和后台任务关闭|23副本/7缓存与HTTP/11API回归通过；活跃慢句柄及总内存压力/容量后续验证|
|3|§14/28.16–28.18 / WP14,WP15,WP16|`preparation::selected`保留Solana路径；阶段87–100已补双模式简单路径、原RPC准备/缓存/worker与界面；实时完整银行及上层钱包链路仍缺|双模式简单路径原VM/准备组件/HTTP/界面已逐轮验收；可信完整当前状态、链规则/L2费用和一次发送回执继续；未验收的草稿不计完成|
|4|§12/28.12–28.13 / WP10,WP11,WP20|能力目录61模型中38个ExactOut、21个执行准备；其余数学、执行和Token行为范围分别缺失|按真实版本逐项补状态/原数学/完整后状态/正常服务/独立执行，目标模式与执行资格独立统计|
|5|§9/18/28.10 / WP08,WP24|`checkpoint::Write/Read`只有本地原子目录及摘要；owner bootstrap不提供对象存储的不可变发布/下载和peer资格|补对象存储manifest先后/半上传/错误摘要/预算与无源恢复，以及真实保留过期和故障域验收|
|6|§15/18/20 / WP16,WP17,WP25|API单凭据/共享限流；生产TLS/SASL/ACL、多区切换及业务SLO仍有缺口|独立租户身份/额度、可信来源隔离和正式设施安全配置；按故障矩阵验收，不把单机PG/Kafka当多区HA|
|7|§16/28.21/28.23 / WP19,WP21|有限Job和显式常驻worker已接入，ClickHouse事实/版本/shadow发布/查询尚缺|继续补持久DLQ/raw重放、迟到/修订/decoder回补、分析存储与发布原子性|
|8|§28.3/28.24–28.25 / WP01,WP22,WP23|本地make入口已有；CI平台执行、镜像/SBOM/扫描、RFQ多求解器与真实灾备放量仍缺|先接可复现CI与安全检查；RFQ/收益界和区域RPO/RTO按各自完整门槛交付|

下一轮缓存回收预检（2026-11-16）：隔离临时crate的5项单元测试全部通过，验证三个缓存的到期边界、撤权后QuoteView/账户捕获实际释放、在途句柄保留、空闲维护和服务任务取消。补丁尚未接入主目录，不计入阶段85或正式功能完成数；主目录源码冻结不变，待本批验收/提交后接入正式服务并复验。

<a id="plan-2026-11-17-baseline-cache-reclamation"></a>

## 基线图复用与闲时缓存回收（2026-11-17，阶段86）

本批补阶段85留下的基线安装路径，并处理无新请求时缓存持续持有旧QuoteView/账户捕获的问题。基线、owner checkpoint和scope扩容接入Graph::update；新QuoteView使用独立的有效性标志，恢复或新generation仍撤销旧句柄，未变的图/邻接桶可以共享。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F86.1|基线/恢复/scope扩容的增量图复用|WP09,WP12|LOCAL_SCOPE_VERIFIED|本工作区23项副本测试exit0；新增2项generation扩容/数值重锚对照完整构图，旧视图撤权、未变邻接桶及恢复图的Arc共享全部通过。|
|F86.2|闲时缓存回收与实际内存释放|WP09,WP16|LOCAL_SCOPE_VERIFIED|ExactIn/ExactOut/准备缓存按1秒周期清理过期或撤权条目，跳过积压tick；条目移除后在锁外析构，Weak/Drop检查实际释放与在途请求保留。|
|F86.3|正常HTTP服务与后台任务关停|WP09,WP16|LOCAL_SCOPE_VERIFIED|真实loopback HTTP服务无新报价时清理缓存；优雅关闭后不再回收，调用方取消时自动中止后台任务；回执继续由耐久journal管理。|
|F86.4|正式工作区统一门禁|WP09,WP12,WP16|LOCAL_SCOPE_VERIFIED|副本全包、7项缓存生命周期、普通API/ExactOut/8场景钱包回执、两crate全目标严格Clippy、契约和普通/优化文档检查；源码、全部固定测试输入及固定SBF/Python运行时摘要统一绑定。|

临时组合副本7项缓存/HTTP生命周期与11项副本预检均exit0；其后本批已通过正式工作区的完整门禁。复验入口：`make -C integration cache-reclamation-current-check`。阶段85已完成同源码整批验收；本批在独立Git工作区完成并复用摘要固定的既有测试输入。

本批覆盖图数据共享和缓存持有者回收。完整Pool状态向量/顶层索引COW、活跃慢请求的内存压力、对象存储恢复、beam/共享DAG/收益界及生产容量仍按原工作包推进；原26工作包和全部功能验收门槛保持不变。

阶段86首次整批验收保留FAILED记录：副本23项及缓存/真实HTTP生命周期7项通过，钱包回执首个ready场景的独立SBF worker在15秒门限内未输出，请求只写入65536/4002351字节，尚未调用发送。固定解释器/同SDK启动探针分别约0.14秒与1.4秒，未复现；完整8场景钱包回执在无其他本任务构建/VM并行的独立重放中exit0（126.36秒）。未修改功能代码、输入或生产超时；下一次整批按原范围串行复验，保留失败与诊断，不据单项重放提前升级本批状态。

阶段86第二次整批保留FAILED记录：41项Rust及全目标严格Clippy通过，公共契约检查发现隔离工作区漏挂Go/Rust独立向量和部分规范来源；这是本次工作区准备遗漏。现已补齐，独立运行公共契约检查325个向量/12个fixture/164份公共来源全部通过；验收器输入摘要清单扩展覆盖三个服务、公共来源及参考Solidity的全部参与文件，Rust功能代码没有变化。下一轮仍完整运行全部门禁，不以独立检查替代整批结论。

阶段86第三次整批保留FAILED记录：41项Rust、严格Clippy及公共契约均通过，文档引用检查发现隔离工作区缺少编译布局、原Managed源码和CPMM构建记录三份既有引用。现按全部文档链接补齐，17项普通及17项优化文档检查单独通过；摘要覆盖相关固定参考文件，公共契约/文档前置检查改为先执行，原8项门禁范围保持完整，Rust功能代码未改。

阶段86最终验收：8项门禁/41次Rust测试，899份固定测试输入与46份运行时输入同批绑定。全部23项副本、7项缓存/真实HTTP生命周期、11项API测试（其中钱包用例保留8个完整场景）、两crate全目标严格Clippy、公共契约及17项普通/优化进度文档检查通过。源码与全部JSON/固定Python-SBF输入运行前后完全一致；首次SBF超时失败、启动探针和独立8场景重放记录保留，串行整批未增加生产超时或缩减用例。本批没有改变协议能力目录或生产/全部功能完成标志。

<a id="plan-2026-11-17-evm-preparation-component"></a>

## EVM计划绑定与准备组件（2026-11-17，阶段87）

按§21.1保持核心服务默认不持有密钥、不签名、不广播。本批先将已有隔离草稿接入正式执行计划/准备crate，并补完整ExactOut绑定与无损数量接口，为下一步真实RPC捕获、链规则、独立VM与正常API接入提供基础。观察值由组件调用方提供，不属于独立链上事实证明；协议能力目录保持21/61执行准备与38/61 ExactOut。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F87.1|完整V2 ExactOut绑定与calldata|WP14|LOCAL_SCOPE_VERIFIED|完整scope/adapter集合/同factory、源码身份和目标路径重算，最多8hop简单路径，caller/recipient/物理池别名、预算、calldata及绑定篡改拒绝。|
|F87.2|资金/费用与EIP-1559准备|WP14,WP15|LOCAL_SCOPE_VERIFIED|保留uint256金额和费用，chain/nonce/快照/调用方/token/spender绑定；资金或授权不足不生成wire，不自动插入approve。|
|F87.3|数量无损与nonce边界|WP14,WP16|LOCAL_SCOPE_VERIFIED|chainId/nonce/gas使用规范十进制字符串，RPC视图使用hex数量；拒绝耗尽nonce，保留最后可用u64值与宽费用。|
|F87.4|原ABI/RLP与统一门禁|WP14,WP15,WP16|LOCAL_SCOPE_VERIFIED|13命令exit0、62次Rust、95份原ABI/独立RLP与离线签名对照、4普通/4优化篡改拒绝、严格Clippy及普通/优化文档全部通过；固定输入899份、参考185份摘要一致并保留。|

本批尚无真实RPC捕获、当前来源/部署资格、链特定hardfork/L2费用、正式EVM模拟准备缓存或钱包链路；组件返回的ready仅表示给定观察下的内部准备条件满足。原26工作包、VERIFIED及生产准入继续按原完整标准验收，不以组件结果替代端到端功能。

阶段87正式Rust预检exit0：7项目标绑定/准备与10项资金/数量控制，共17项测试全部通过；其中原Router02语料90个目标（含至少30多hop和9个宽金额目标）逐项绑定并核对原所需输入。nonce耗尽负例、最后可用u64值、宽费用、无损十进制JSON和规范RPC hex均通过。独立ABI/RLP、完整计划/旧模板回归及严格门禁仍待完成；没有将组件结果计作正式RPC/API/独立生产执行资格。

阶段87独立预检：4份ExactIn与90份ExactOut导出结果通过原Router02 ABI、独立ethers RLP逐字节与受控离线签名恢复，含宽nonce；仅本地离线编码，不接RPC或广播。随后补零基础费/零小费观察下的合法零费准备，以及无access-list调用的EIP-2930基础intrinsic gas下界；该下界不替代当前hardfork的calldata floor或L2费用规则。新统一门禁要求95份导出结果、原90目标全集、篡改拒绝控制及完整代码/固定输入/独立Node依赖摘要闭合后才能升级本批状态。复验入口：`make -C integration evm-preparation-current-check`。

阶段87首轮统一门禁保留FAILED：19项Rust通过，独立编码检查的案例全集比对将编号0误用为JavaScript假值而替换成文件名；这是校验器编号处理错误，已改为空值判断。修正后同批95个案例全部通过，含原90目标与5份ExactIn/费用边界，4项普通/4项优化篡改拒绝控制通过；Rust功能代码未因该校验器错误改动。后续仍完整运行统一门禁，当前不提前升级状态。

阶段87第二轮统一门禁保留FAILED：19组件/31计划/2能力/10API共62次Rust、95份独立对照及普通/优化篡改控制全部通过；最后全目标Clippy指出新增测试的单元素目标切片多余clone。现改为借用切片，保持-D warnings，Rust产品逻辑/用例范围/输入不变，重新严格检查并完整复验。

阶段87最终限定验收：13项门禁/62次Rust/95份独立ABI-RLP签名对照，899份固定输入/185份参考依赖绑定。全部13命令exit0，19项组件、31项旧计划、2项能力目录和10项API共62次Rust测试通过；5份ExactIn/费用边界与90个原目标（含多hop、宽金额、宽nonce）通过原ABI/独立RLP/离线签名，4项普通/4项优化篡改拒绝通过。源码、固定输入和独立参考依赖运行前后摘要一致，参考文件按摘要保留；首次案例0校验器失败原样留存。没有进行RPC或广播；本批不授予正式EVM执行、VERIFIED或生产资格，原26工作包和未完端到端要求继续推进。

<a id="plan-2026-11-17-evm-rpc-preparation"></a>

## EVM固定块RPC准备与API（2026-11-17，阶段88）

本批把阶段87的调用方观察值替换为模块自行采集的单端点固定块RPC观察。operator显式配置genesis、Router/Factory和各Token代码摘要与预算；所有余额/代码/getter/storage读取使用blockHash+requireCanonical，不退回按number读取。普通块头按number读取以确认其canonical映射；这不构成独立共识或状态根证明。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F88.1|同块身份/完整状态/资金采集|WP04,WP14,WP15|LOCAL_SCOPE_VERIFIED|实际chainId/genesis、Router/Factory/Pair/Token代码、完整getReserves/raw balances/cumulatives/unlocked与双向factory pair核对；未知hash负对照覆盖balance/nonce/code/storage/call；固定nonce与pending一致，读取后再次核对pending、canonical头和chainId。|
|F88.2|边界、预算与观察承诺|WP04,WP14,WP16|LOCAL_SCOPE_VERIFIED|总调用次数/总wire/总期限、禁止重定向与重试、严格JSON重复键/数量/ABI，资金或授权不足无unsigned wire；调用参数/结果摘要进入RPC准备与API的QuoteView承诺，错误不输出URL或provider消息。|
|F88.3|显式配置/API/前端|WP14,WP16,WP25|LOCAL_SCOPE_VERIFIED|api.evm_preparation按chain配置一个RPC端点，认证限流/共享并发；POST /v1/quotes/{id}/prepare-evm支持既有单池ExactIn与简单路径ExactOut，前后QuoteView复核；前端展示EVM费用上限与下载，金额仍字符串。|
|F88.4|原状态回放/故障与统一门禁|WP04,WP14,WP15,WP16,WP25|LOCAL_SCOPE_VERIFIED|原固定V2代码/getter经loopback RPC测试；RPC故障/重组/nonce/余额/授权、配置与认证、重复JSON和QuoteView通过；15项门禁/51次Rust/12项Node/9项Chrome与899固定/46运行时输入统一验证通过。|

预检首次失败包括：超时测试的单请求/总期限竞争（已由统一总期限覆盖）、鉴权用例漏设token（默认无认证的既有语义保持）、非法请求框架默认纯文本（现返回显式JSON错误）、页面label样式覆盖hidden（补标准hidden样式）。故障及旧日志保留；最终本批须同源码整体验证，不以预检替代。

本批结果是HASH_PINNED_RPC_OBSERVATION：nonce与代码/状态读取在同次采集中绑定，不能因此称为独立执行、完整当前费用、真实历史共识或生产准入。原始观察与component承诺均随结果提供，源码能力目录仍为21/61执行模板与38/61 ExactOut；quote-options额外明确按运行配置声明evmPreparation，不借用Solana准备或钱包接口。独立EVM状态DB/实际余额差、hardfork/L2费用、当前源资格与上层钱包签名广播仍继续开发，核心服务默认不持钥/不签名/不广播。

阶段88首次统一运行主动停止并保留CANCELLED记录：前9门禁通过后，承诺结构复核发现把RPC字段追加到组件对象会使原组件摘要无法直接按原严格结构重算。实际浏览器导出文件已留反例；现把原组件完整嵌入RPC封装，API保留component与rpcPreparation两个可独立重算层，并补严格反序列化/原组件摘要/RPC摘要回归。总期限在网络后同步JSON/摘要工作完成时再次检查，避免同步阶段越过预算后发布。随后完整复验。

阶段88方法覆盖复核补齐固定块eth_getTransactionCount的未知hash负对照及只忽略该方法selector的故障用例；API在最后编码/摘要后再次核对QuoteView与期限。前次运行保留CANCELLED，严格检查范围未缩减，重新冻结全部源码并完整复验。

阶段88最终限定验收：15项门禁/51次Rust/12项Node/9项Chrome，899固定输入/46运行时输入同批绑定。15命令全部exit0；原组件和旧API/Solana准备回归保留，三crate全目标严格Clippy通过；9项Chrome覆盖双模式准备/金额与QuoteView绑定、交易下载、费用变更撤销及独立/钱包边界。最终截图已检查，实际浏览器导出中的原组件/RPC/API三个摘要均独立重算一致；源码及固定/运行时输入运行前后完全一致，原失败/取消记录保留。本批loopback的Pair代码/getter来自留存原状态，资金/授权及故障为受控RPC观察，不冒充公开RPC或独立执行；当前链hardfork/L2费用、可信状态与原VM闭环、上层钱包及生产仍继续开发。

<a id="plan-2026-11-17-evm-dynamic-fees"></a>

## EVM独立仿真动态费用（2026-11-17，阶段89）

对照§14.2–14.5/28.17发现阶段88的EIP-1559准备与旧独立仿真的单一gas price未接齐。本批为TransactionContext增加可选priority fee，存在时gas_price表示max fee；REVM实际收到两项参数并执行cap、tip、base fee及最大预付资金校验。旧JSON不增加空字段，原承诺保持可重算。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F89.1|EIP-1559完整费用上下文和最大预付资金|WP15|LOCAL_SCOPE_VERIFIED|cap/tip错误先拒绝；有效价格按base+可用tip上限，最大承诺资金不足由真实REVM拒绝。|
|F89.2|同次原生币余额、base burn与tip|WP15|LOCAL_SCOPE_VERIFIED|调用者/coinbase实际pre/post写入仿真承诺，涵盖revert、零费及caller=coinbase。|
|F89.3|原Router代码与旧承诺回归|WP15|LOCAL_SCOPE_VERIFIED|16项既有原Router服务场景对照legacy相同有效价，核对完整后状态摘要、token余额差和native收费；统一门禁通过。|

本批只覆盖显式给定H/H+1银行的Shanghai执行费用。真实当前状态捕获、chain hardfork、L2 data fee、ExactOut独立VM和钱包发送仍未完成，全部功能与生产完成标记保持false。

阶段89首次门禁已保留FAILED记录：format/contracts通过，进度契约发现本批任务计划缺少历史批次的显式链接。已补齐第89批链接，功能实现未放宽，重新执行全部门禁。

阶段89构建环境复核：保留两次CANCELLED记录。macOS在现有巨型target/deps目录的宏动态库装载和测试执行文件校验中长时间等待；同字节文件复制到私有小目录即可执行。验收器增加Cargo native runner，逐次SHA256核对原文件/副本/复制后的原文件，再原样传参运行并保留退出码；未签名、未更改系统安全设置、未放宽功能门禁。宏动态库临时副本也逐一校验原字节。

阶段89本机runner首次配置被Cargo因target环境变量含小写而忽略，本次保留CANCELLED记录；已改为全大写并为每组Rust测试增加实际runner执行标记检查，防止只配置未实际使用。

阶段89用户中断（2026-11-17）：117次Rust测试和format/contracts/进度契约均已通过，全目标严格Clippy尚未完成即中断；最终运行记录为CANCELLED/USER_INTERRUPTED，不计作整批验收通过。本轮修改尚未commit或push。临时目录已随会话中断清除，相应失效的编译缓存链接已移除，缺失的过程宏编译产物由后续Cargo构建重建；源代码与测试输入保留。

阶段89恢复后的最终限定验收：恢复前核对源码、899份固定输入及7份已通过门禁日志摘要全部一致，保留中断报告和Clippy未完成日志后只续跑缺失的全目标严格Clippy；独立持久化构建目录的Clippy在1分41秒内exit0。最终8项门禁、117次Rust测试（独立仿真86、API上下文8、准备/RPC23）通过，16个原Router银行的动态费用与legacy有效价后状态对照通过。源码及固定输入前后完全一致，最终运行目录`5ffe6c2a49634fc795ae6b03a788954e`；各次失败/取消记录保留。当前执行准备21/61、ExactOut38/61，全部功能与生产完成标记保持false。

<a id="plan-2026-11-17-evm-prepared-execution"></a>

## EVM准备组件与同银行独立执行（2026-11-17，阶段90）

对照§14.1–14.5/28.16–28.17，本批将阶段87/88准备组件与阶段89独立费用执行绑定。`prepared_evm::simulate_prepared`重新生成完整组件并比较其全部字段；不是只核对输入自报hash。交易nonce、chain、gas/cap/tip、caller/target/calldata/value、签名者、RPC参数和unsigned wire必须一致，所有资格标记保持原始受限值。实际执行使用该组件的EIP-1559上下文，原wire字节SHA256、原组件hash及独立receipt各自保留，再由外层commitment绑定。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F90.1|准备组件与实际交易严格绑定|WP14,WP15|LOCAL_SCOPE_VERIFIED|16原Router服务银行的完整准备和REVM兑换量对照；25种修改后重算hash的组件仍在实际执行前拒绝，原组件保持不变。|
|F90.2|同银行资金核对和getter上下文|WP15|LOCAL_SCOPE_VERIFIED|同一次Runner、StrictDB和共享预算读取native/nonce/token及真实allowance；准备观察值不一致在swap前拒绝。所有只读getter传递相同cap/tip，GASPRICE反例检查有效价而非cap。|
|F90.3|预算/取消/统一门禁|WP14,WP15|LOCAL_SCOPE_VERIFIED|准备JSON先流式计数再hash，限额与取消/到期先拒绝；10项观察上下文控制、原仿真/准备/API及严格Clippy统一验证通过。|

本批是显式提供H/H+1银行的内部ExactIn组件。新增allowance probe计入原聚合read/byte/gas/instruction预算；未添加另一个可绕过预算的VM。来源继续为PROVIDED_LOCAL_STATE/verified=false，不升级普通EVM API或静态协议能力。RPC到完整可信执行状态DB、ExactOut独立VM、当前链hardfork/L2费用、钱包签名广播和生产部署仍待完成；其中本地实现差距与需要operator提供的正式来源/基础设施分别记录，不将所有待实现项标为外部阻塞。

阶段90最终限定验收：8项门禁全部exit0，121次Rust（仿真90/API上下文8/准备RPC23）通过；16原Router银行的组件/原wire/余额差核对、25次重算hash篡改拒绝、10项上下文控制和实际GASPRICE反例通过。全目标严格Clippy通过，源码和899份固定输入前后完全一致；运行目录`95a24af96f7847ee8190cc0213f3cf74`。当前组件不生成签名或广播、不开放普通API VERIFIED，执行准备21/61与ExactOut38/61限定资格不变。

<a id="plan-2026-11-17-evm-dynamic-fee-api"></a>

## EVM实际HTTP/worker动态费用（2026-11-17，阶段91）

本批代码复核发现LocalSimulation的diagnostic_response仍把gas_price直接视为实际价格；动态上下文中该字段是max fee，会高报执行费用。现根据相同WorkerRequest的execution base fee和cap/tip核对REVM返回的effective price、execution fee、base burn和priority fee；任何缺失或不一致均拒绝，不从cap推定实际费用。原交易上下文保留max fee和priority，新增executionBaseFee承诺，旧legacy价格语义不变。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F91.1|API实际价格与费用核对|WP15,WP16|LOCAL_SCOPE_VERIFIED|三种cap/tip组合、四种费用字段篡改、缺失native费用反例定向通过。|
|F91.2|实际HTTP到同二进制worker|WP15,WP16|LOCAL_SCOPE_VERIFIED|新loopback实例加载原Router银行，正式/v1/quote和/simulate路径实际派生worker，检查收款/native/coinbase、真实费用、拒绝HTTP覆盖费用与认证；统一验证通过。|
|F91.3|统一门禁及输入冻结|WP15,WP16|LOCAL_SCOPE_VERIFIED|复用8项Rust/契约/严格Clippy门禁，增加harness构建与实际HTTP检查；未签名未广播，生产标记保持false。|

首次HTTP预检保留失败：测试运行目录传相对路径，bundle经配置根再次解析导致SIMULATION_STATE_UNREADABLE；验收器已在启动前规范化绝对输出目录，产品路径解析保持原语义。不是功能通过记录，重新执行完整HTTP与门禁。

阶段91第二次HTTP预检保留失败：测试误用复数报价路径得到404；已按正常路由改为/v1/quote并保留非JSON错误的有界诊断，不改产品接口。

阶段91最终限定验收：10项门禁全部exit0，122次Rust（仿真90/API上下文9/准备RPC23）、实际HTTP及同二进制worker通过，源码与899份固定输入前后完全一致；运行目录`d79033cae53e4731952d69c74ae01629`。实际原Router使用104566 gas，max fee 2000000000 wei、有效价226958868 wei，执行费23732180991288 wei；阻止误报差额185399819008712 wei。核对原账户/coinbase净变化、收款与报价、原cap/tip承诺、认证和拒绝HTTP参数覆盖，VERIFIED/链完整费用/生产资格保持关闭。

<a id="plan-2026-11-17-evm-exact-out-execution"></a>

## EVM原Router单池ExactOut独立执行（2026-11-17，阶段92）

本批补独立VM此前只有ExactIn的代码缺口。原ExactIn与新增ExactOut共用同一StrictDB、原getter检查、聚合执行预算、复制后状态、真实token/native余额差及取消机制；原ExactIn入口和receipt承诺结构保留，不用改变旧绑定来模拟目标模式。ExactOut调用自身完整绑定校验后，仅准入一条一物理池路径，实际执行原swapTokensForExactTokens。多池路径继续明确拒绝，不以只检查首池放行。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F92.1|完整绑定/原Router单池ExactOut|WP14,WP15|LOCAL_SCOPE_VERIFIED|16原代码银行的目标收款、实际输入及原Router返回数组与计划核对；复用原完整V2 getter/后状态观察。|
|F92.2|未用预算/退款和revert费用|WP14,WP15|LOCAL_SCOPE_VERIFIED|新增可选unused_input_budget并进入receipt，max_sell_amount=actual_input+unused；Router只pull实际输入，refund=0。授权/余额/期限revert保留完整未用预算与真实gas/nonce扣费。|
|F92.3|旧语义/缺状态/统一门禁|WP15|LOCAL_SCOPE_VERIFIED|交易/金额篡改及缺失token/storage/错误环境拒绝；保留全部ExactIn/prepare/API回归及全目标严格Clippy；统一验证通过。|

首次编译预检保留失败：测试调用BigUint的is_zero但没有引入其trait；改为Amount原生零值比较，未新增依赖或改变功能。原16银行执行预检已通过；这些是留存银行上的原VM调用，不是原链ExactOut矿工回执，来源继续PROVIDED_LOCAL_STATE。静态执行模板与生产资格不因本批升级，ExactOut API/钱包、链规则与真实来源继续待补。

阶段92最终限定验收：8项门禁全部exit0，126次Rust（仿真94/API上下文9/准备RPC23）通过，16个原Router银行ExactOut实测输入/收款、三个revert的完整预算与费用/nonce、原有效多池绑定进入VM前拒绝均通过。旧ExactIn原24交易完整后状态、准备组件/动态费/API上下文继续回归，源码及899固定输入前后完全一致；全目标严格Clippy通过，运行目录`7e61cb2f8d99423181b8b6f2641aecac`。原组件与HTTP准备资格仍独立统计，多池与普通ExactOut HTTP VM未开放，生产与全部功能完成标记仍false。

<a id="plan-2026-11-17-design-delivery-audit"></a>

## 完整交付范围与外部验收边界审计（2026-11-17，阶段93）

本次连续完成并逐轮提交/推送阶段89–92：动态费用原生币执行、准备组件与同银行绑定、实际HTTP/worker费用、单池ExactOut原VM。此后按原设计范围复核，不把这些限定能力写成全系统完成。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F93.1|配置与生产验收外部输入证据|WP01,WP23,WP24|LOCAL_SCOPE_VERIFIED|固定三服务生产配置SHA256，仅投影端点数量/身份占位/控制与安全模式；不读取或复制URL凭据/DSN内容。|
|F93.2|当前二进制正负CLI与本机拓扑|WP01,WP23|LOCAL_SCOPE_VERIFIED|从当前源码构建router；同二进制config-test正对照及config-prod检查，检查最近3份本地Kafka拓扑，不能用多个临时单broker当多区。|
|F93.3|完整设计与本地差距保留|WP23,WP24|LOCAL_SCOPE_VERIFIED|按现有26工作包与协议覆盖表核对；外部输入和本地实现分别列出，完整功能/生产标记保持false。|

本地代码剩余：RPC完整执行状态DB、EVM多池/ExactOut准备HTTP和钱包链路、其他hardfork/L2费用；剩余协议数学/执行及Token行为；完整池COW、共享DAG/收益界、对象存储不可变恢复；生产TLS/SASL/ACL、租户额度、持续analytics、CI安全产物及高级求解。这些仍是实现任务，不因缺少外部环境而改称已经完成或全部外部阻塞。

外部验收输入核对范围限于本仓库明确配置的生产验收环境，不推断用户在其他主机或账户是否持有基础设施。设计§7/14.2/18/20要求的真实当前链/独立来源和多区故障验收，须有明确选定的链/genesis、可用来源及真实部署/故障域/访问与演练范围；这些事实不能从本机合成银行或单broker进程产生。当前配置缺失情况和当前源码CLI结果以本批实际报告为准。

阶段93首次控制预检保留失败：importlib载入审计模块时缺少acceptance模块搜索路径，已将同目录工具路径显式加入；当前router从最新源码已构建成功，正式审计仍待全部控制通过。

阶段93实际审计完成：5项检查全部exit0，6项普通/6项优化审计控制、17项进度契约、公共契约和最新router构建通过；当前同二进制读取config-test exit0/valid=true，读取config-prod exit1/error=CONFIG_CONSTRAINT_VIOLATION。三服务生产文件均为0 RPC来源/0 Kafka端点；两个EVM生产链身份仍为全零genesis。最近3份本机开发Kafka配置均为broker/controller合进程、单controller、事务副本因子1/min ISR1/loopback，报告只证明本地配置事实、不声称已检查远端服务是否存在。报告运行目录`d86990d2910f4ba59900934054bab409`，源码及三生产文件摘要前后一致。

B93.1 正式链来源与执行资格验收（§7/9/14.2/18）：三个生产配置没有RPC来源，EVM genesis占位，当前二进制生产配置拒绝，因此当前链/独立来源/可信状态与真实历史Bank验收被阻塞。解除需要operator明确选定的真实链/genesis、可访问且有来源/故障域说明的RPC或流服务，以及执行/历史状态验收所需的可信状态权限。

B93.2 独立故障域与部署验收（§18/20/28.25）：三个生产配置无Kafka端点，现有样本是同机单broker，当前没有配置可执行多区故障、远端独立存储恢复、业务SLO与RPO/RTO验收的正式部署。解除需要指定真实部署/区域/可独立故障的资源、broker/数据库/独立归档访问，并给出可实施的负载和故障演练范围。

本次依用户约定，在确认上述真实外部验收输入缺失处停止；没有伪造链身份、故障域或生产凭据，也没有把缺环境的项目记为通过。阶段89–92的局部功能交付与阶段93的审计均已独立留证；原26工作包完整设计验收仍为0，61模型的全范围尚未完成，all_requested_features_complete=false、production_ready=false。本地代码差距继续保留在F93前的独立清单与各工作包“待完成范围”，不是“只有外部配置未填”的完成声明。

阶段93最终文档契约复核发现计划章节内追加不同列数的阻塞表会被统一任务表读取器拒绝；现按普通段落保存两项外部输入证据，保留失败日志，不修改既有严格表格解析规则。最终文档契约重新验证。

阶段93最终最新批次复核发现原历史45–48行散落在新增批次之后，导致current_batch误指旧LFJ批次。已仅按原序号重排历史表，全部原行内容保留；最新批次现可按实际第93轮解析，历史scope仍由原显式anchor定位，重新运行普通/优化契约。

阶段93最新批次顺序修正后完整审计再验：5项检查再次全部exit0，普通/优化各6控制、17进度契约、公共契约和当前二进制正负CLI不变。最终报告目录`927d2ca228224549adefc3f4a52a8433`，current_batch正确为第93轮；原目录作为第一次成功审计保留。stop_condition=CONFIRMED_CONFIGURED_EXTERNAL_ACCEPTANCE_BLOCKER，完整功能与生产标记仍false。

<a id="plan-2026-11-17-evm-prepared-exact-out"></a>

## EVM ExactOut准备与独立执行（2026-11-17，阶段94）

用户要求继续完成本地剩余实现，阶段93的外部验收输入缺口继续保留，不用它替代本地实现。本批补齐ExactOut完整准备组件到独立VM的连接；ExactIn和ExactOut重用同一有界组件验证，ExactOut必须重新生成并比较内层交易和外层目标/最大输入/未用请求预算，两层hash均随独立结果保留。实际执行仍使用准备组件的nonce、gas、cap和tip，同次StrictDB检查native/token/allowance，保留取消和总预算。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F94.1|ExactOut完整准备承诺与实际交易|WP14,WP15|LOCAL_SCOPE_VERIFIED|16原Router银行比较两层承诺、原unsigned字节、实际输入/收款和未用编码额度。|
|F94.2|篡改/资金/预算与旧入口回归|WP15|LOCAL_SCOPE_VERIFIED|16项重算内外hash或资金变化、4项限额/取消；既有ExactIn保持可选字段省略及旧承诺；统一验证通过。|

首次定向测试保留失败：错误地把请求input_budget余量与滑点收紧后的max_sell_amount余量当成同一值。已按现有语义分开核对：prepare.unused_quoted_input_budget=allocation.input_budget-required_input；simulation.unused_input_budget=max_sell_amount-actual_input。二者均非实际退款，产品额度语义不变；未放宽任何资金或承诺验证。后续普通HTTP/worker集成继续，不升级当前链来源或生产资格。

阶段94最终限定验收：8门禁全部exit0，129次Rust测试（仿真97/API9/准备RPC23），16原银行、16篡改或资金变化及4预算/取消控制通过；完整旧ExactIn、组件/链路与原24交易后状态回归保留，全目标严格Clippy通过。源码和899固定输入前后相同，最终运行目录`2162ce7389954ead9815d50d367a10a9`；后续HTTP集成继续，不把本轮内部组件声明为整个EVM功能完成。

<a id="plan-2026-11-17-evm-exact-out-api"></a>

## EVM ExactOut正常HTTP独立模拟（2026-11-17，阶段95）

本批取消revm-fixture配置下普通ExactOut模拟入口此前的无条件拒绝。请求仍只接受空body或空对象；模式/金额/费用/状态由已缓存计划和启动时固定bundle决定。ExactIn与ExactOut重用同一有界worker管理器、共享模拟并发、硬期限和取消回收；worker envelope必须恰好指定一种bound，旧ExactIn JSON保留bound字段形式，不增加空模式字段。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F95.1|普通ExactOut接口到实际worker|WP15,WP16|LOCAL_SCOPE_VERIFIED|原/v1/quote-exact-out与/v1/quotes/id/simulate实际启动同二进制worker，原ExactOut selector、输入消耗/目标收款、native费用及未用编码预算核对。|
|F95.2|独立缓存证据和时效撤权|WP16|LOCAL_SCOPE_VERIFIED|ExactOut新增独立simulation_evidence，取证接口保留原quote/plan；发布前和写缓存时复核过期及QuoteView，签名和VERIFIED仍关闭。|
|F95.3|模式/取消回收/旧入口与统一门禁|WP15,WP16|LOCAL_SCOPE_VERIFIED|双模式或无模式worker请求拒绝；ExactOut HTTP取消回收实际子进程/共享许可，缓存锁不跨I/O；实际ExactIn HTTP及旧API回归，统一验证通过。|

首次编译预检发现自动替换命中了相邻build入口的同形缓存代码，已把模式分派限定到simulate_transaction并恢复原build拒绝逻辑，失败日志保留。首次实际ExactOut HTTP已通过，真实原Router gas104574、有效价226958868wei、执行费23733996662232wei；此为预检不替代最终冻结门禁。来源继续PROVIDED_LOCAL_STATE，单池/Shanghai限制不变，多池及RPC完整银行后续继续。

阶段95定向测试发现新单元测试配置未填写revm-fixture必需的instructions/memory/concurrency，三个测试按原配置约束拒绝；已补齐显式预算，保留失败日志，第一整体验证主动取消后重跑。实际HTTP预检此前已通过，不视为替代完整回归。

阶段95最终限定验收：12门禁全部exit0，151次Rust（仿真97/上下文12/准备RPC23/旧API19），普通双模式HTTP分别实际运行同二进制worker并验证原selector、输入/收款/native费/未用编码预算及证据保存。ExactOut取消实际子进程并归还许可，发布过期或撤权结果拒绝；全目标严格Clippy通过。源码及899固定输入前后一致，运行目录`c2b8b152ba5b489a81be1453622a110d`。旧无backend拒绝、原build关闭和所有旧API回归保留，普通ExactOut仍为PROVIDED_LOCAL_STATE，当前链来源、完整费用和钱包生命周期继续独立待补。

<a id="plan-2026-11-17-evm-path-execution"></a>

## EVM多池ExactOut原交易执行（2026-11-17，阶段96）

本批补原简单路径ExactOut VM单池限制。绑定层已有唯一token/物理池、factory/源码和最多8hop边界，执行层现在对所有touched pools检查源银行、Pair/Factory代码、getter/raw余额/cumulative/锁，实际swap后逐池读取完整后状态。所有探针和swap共享原read/byte/gas/instruction/取消预算，不仅验证首池。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F96.1|真实原交易与完整声明银行参考|WP14,WP15|LOCAL_SCOPE_VERIFIED|固定原发布Factory/Pair/Router/Token，实际部署/approve/mint后执行1/2/3hop双向ExactIn/Out共12交易，保留每个声明账户/slot的完整前后值、nonce/code/native与原receipt gas。|
|F96.2|多池ExactOut绑定/准备/独立VM|WP14,WP15|LOCAL_SCOPE_VERIFIED|6原目标路径实际calldata、消耗/收款/gas对照，完整准备同银行执行后状态摘要相同；原12交易完整声明overlay与矿工后状态逐字节核对。|
|F96.3|逐hop拒绝/聚合预算/统一验收|WP15|LOCAL_SCOPE_VERIFIED|3hop逐池缺账户/错误代码/缺slot/错源银行共12拒绝；read/byte/instruction少1聚合预算均拒绝，旧单池/准备/API及严格Clippy统一验证通过。|

生成器只使用已保留原发布源码/字节码及本机Ganache Shanghai，不访问公网；97份原参考输入摘要和新增完整银行进入验收。测试采用真实原交易后状态作为权威，不将数学输出写成执行证据。旧“多池必须拒绝”测试改为“有效多池计划不能用不相关银行执行”，源身份要求保持。ExactIn多池计划仍待补绑定，当前链规则、可信公开银行、完整费用与生产资格不因此升级。

阶段96首次统一验收保留FAILED：仿真97/API12/准备RPC23/新路径3共135次Rust和普通/优化参考控制均通过，最后严格Clippy发现新测试未使用json导入。已移除多余导入，保持-D warnings，重新完整复验，不用此前通过项拼成最终通过结论。

阶段96最终限定验收：11门禁全部exit0，135次Rust（旧仿真97/API12/准备RPC23/路径3）、普通/优化各3参考控制通过；12实际原Router矿工交易的完整声明账户code/native/nonce/storage与独立overlay一致，6原ExactOut简单路径的准备和VM余额/gas/后状态一致。源码及901固定输入、97参考输入前后相同，全目标严格Clippy通过，最终运行目录`3e8d51ebdcf445b5a2ce4b54d2e709d2`。真实参考覆盖1–3hop双向，不将本地Shanghai差分扩成当前链/完整费用/生产资格；ExactIn多池和正常API路径继续下一轮。

<a id="plan-2026-11-17-evm-input-paths"></a>

## EVM ExactIn多池与双模式正常HTTP（2026-11-17，阶段97）

本批补ExactIn此前一hop绑定限制，要求单路径1–8hop、token/物理池不重复、端点/每步方向一致；多hop必须全部使用完整V2版本及相同Factory，逐池匹配原固定源码和snapshot源银行。按路径顺序重新计算完整分配，全部touched池写入绑定，原calldata path数组保留。旧单hoplegacy语义保留，split/DAG、循环和混合旧版本不自动放行。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F97.1|ExactIn完整多hop绑定/准备/VM|WP14,WP15|LOCAL_SCOPE_VERIFIED|6原矿工输入路径的完整calldata/gas/实际收付与准备后状态对照，9重算变形覆盖循环/重复池/错方向/后续池身份和源银行。|
|F97.2|普通双模式HTTP与界面准备|WP16,WP25|LOCAL_SCOPE_VERIFIED|正常API实际选3hop后派生原worker，双向ExactIn/Out核对实际收付/native费；界面允许同样的单路径多hop准备，重复池/未准入池仍拒绝。|
|F97.3|参考闭包/旧计划/严格门禁|WP14,WP15,WP16|LOCAL_SCOPE_VERIFIED|原12矿工完整银行、新路径、旧计划/准备/API及Node模型回归；901固定输入/97原参考文件与全目标严格Clippy统一验证通过。|

预检6原输入路径和9变形通过，旧bound_v2 11项回归通过。实际3hop HTTP预检原ExactIn gas327333、ExactOut gas327383，实际effective价42761746wei；来源仍PROVIDED_LOCAL_STATE，预检不替代最终冻结门禁。当前RPC/真实hardfork/L2费用和上层钱包继续单列，设计范围剩余本地功能继续推进。

阶段97最终限定验收：17门禁全部exit0，168次Rust（旧仿真97/API12/准备RPC23/路径5/计划31）及Node模型通过；6原ExactIn路径、9重算变形、原12交易声明完整后状态和实际双模式3hop HTTP/worker通过，计划及仿真/API全目标严格Clippy通过。源码与901固定输入、97参考文件前后一致，最终运行目录`822951e5a4424a1489509247c978d852`。已补齐正常报价到多hop独立执行的两种目标模式，不把提供银行/Shanghai资格升级为当前链/完整费用/生产。后续继续补RPC准备与实际银行的集成。

<a id="plan-2026-11-17-evm-source-fees"></a>

## EVM来源块与执行块费用上下文（2026-11-17，阶段98）

继续接RPC准备时发现实际语义缺口：FundingObservation记录来源H的base fee，prepared执行此前却要求它与H+1执行块base fee相同。现StateAnchor增加可选base_fee_per_gas（十进制字符串），有明确来源费用时只核对资金观察的源H值；实际交易仍严格使用ExecutionEnvironment的H+1 base fee、同一cap/tip和真实REVM费用。没有源费用字段的旧bundle保留此前受限的同费率语义，不隐式把不相同来源放行。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F98.1|来源H与H+1双费用承诺|WP15|LOCAL_SCOPE_VERIFIED|16原header的H费用均不同于实际H+1；双目标模式准备按来源核对，而实际fee/tip/burn用执行块值。|
|F98.2|错误/缺失源费率与cap拒绝|WP15|LOCAL_SCOPE_VERIFIED|去掉来源字段、换来源数值均在执行前拒绝；来源prepare可通过但H+1超cap仍拒绝，改变声明来源后fixture/外层hash变化。|
|F98.3|旧bundle兼容与统一验证|WP15|LOCAL_SCOPE_VERIFIED|可选字段省略保留旧JSON；uint256及总体fixture预算检查继续，旧原交易/准备/API与全目标Clippy统一验证通过。|

新字段是给定状态的来源声明，不构成header/stateRoot/共识证明；实际RPC到完整银行捕获后续继续。不得用修改base fee把真实H+1环境强行改成H，也不得因本批通过就声称当前链完整费用。

阶段98最终限定验收：8门禁全部exit0，134次Rust（仿真99/API12/准备RPC23），16原银行的双模式源H与H+1费率独立核对、缺失/错误源费用及下一块超cap控制通过；旧无字段bundle和原交易回归继续通过，全目标严格Clippy通过。源码及901固定输入前后相同，运行目录`3aa4be9e7ead47a292e1954690e599ac`。下一步继续把RPC准备缓存与原worker完整连通，不以本批替代真实来源或生产资格。

<a id="plan-2026-11-17-evm-prepared-api"></a>

## EVM缓存RPC准备到独立worker（2026-11-17，阶段99）

本批把原RPC prepare-evm输出与其后独立执行连成正常服务路径。新增POST /v1/quotes/{id}/preparations/{hash}/simulate-evm，只接受空body/空对象；交易、资金观察和费用只取服务端当前缓存，不接受HTTP覆盖。worker原二进制扩展可选prepared上下文，nonce/gas/cap/tip与明确transaction_context必须一致，按目标模式重算原组件并执行。父进程核对原组件hash、ExactOut外hash与返回外commitment，结果附原API/RPC准备hash及preparedExecution，原SimulationOutcome保持可独立重算的嵌套结构。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F99.1|原缓存准备到实际双模式worker|WP14,WP15,WP16|LOCAL_SCOPE_VERIFIED|真实hash固定RPC prepare→缓存→原同二进制worker，原unsigned SHA/组件/实际收付/native费与外层准备hash全链核对；来源H与执行H+1费用分开。|
|F99.2|替换/时效/撤权与有界载荷|WP15,WP16|LOCAL_SCOPE_VERIFIED|每quote只留最新准备，64KiB单组件/32MiB总体序列化载荷额度；旧准备晚返回在写证据锁内拒绝，quote GC回收额度，在途句柄保留自己的额度。|
|F99.3|错误来源/协议/旧入口/统一验证|WP15,WP16|LOCAL_SCOPE_VERIFIED|受控RPC native余额+1时原组件可自洽但独立银行拒绝；响应只允许plain/prepared/error一种并重算外commitment，原无准备worker/取消/cache/API回归及统一Clippy通过。|

预检真实ExactIn/ExactOut准备与worker已通过，更新gas_limit后旧preparationHash不可执行；错误资金观察被SIMULATION_OBSERVATION_MISMATCH拒绝。harness的bank模式直接读取保留原账户/slot，不把FF余额占位用于独立验证。首次构建发现测试harness的RPC句柄未显式读取，已在服务结束时abort，不降低严格lint。实际来源是受控loopback原银行，非公网RPC/共识证明；准备上下文只缓存小组件及资金/费用，不把整份RPC观察body留入准备缓存，不承诺同等于32MiB实际总堆内存。

阶段99最终限定验收：15门禁全部exit0，167次Rust（仿真99/API14/准备RPC23/旧API19/cache7/路径5），实际ExactIn/ExactOut从原RPC准备到原worker成功及错误资金观察拒绝通过；参数变更后的旧hash、迟到证据写入、序列化载荷额度和GC/在途保留控制均通过，全目标严格Clippy通过。源码/901固定输入/97参考文件前后一致，运行目录`3ca6dc7a1e294ca48db1cdb252e62dbd`。本批正常服务已连通缓存准备独立执行，界面与上层钱包、实时完整RPC银行/真实链规则继续独立推进。

<a id="plan-2026-11-17-evm-prepared-frontend"></a>

## EVM原准备独立执行界面（2026-11-17，阶段100）

本批补正常界面之前强制关闭EVM独立按钮的缺口。quote-options单独声明evmIndependentLocalSimulation，只有原RPC准备配置存在且启动时银行的chain/genesis/完整源块身份与当前QuoteView匹配时为true，不借用Solana静态或独立SBF能力。原ExactIn响应现在显式包含kind=EXACT_IN，前端兼容旧响应缺省模式。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F100.1|运行时银行能力与双模式界面|WP16,WP25|LOCAL_SCOPE_VERIFIED|有RPC准备但无匹配银行时保持关闭；有匹配银行时按真实quote/preparation发送simulate-evm，不开放EVM钱包/VERIFIED。|
|F100.2|浏览器原wire与经济上下文复核|WP15,WP25|LOCAL_SCOPE_VERIFIED|WebCrypto重新计算原unsigned字节SHA256，核对API/RPC/组件/ExactOut外hash、quote/plan/view及实际输入/收款；改变wire摘要或金额/权限边界的响应拒绝。|
|F100.3|真实Chrome/旧边界/统一验收|WP15,WP16,WP25|LOCAL_SCOPE_VERIFIED|实际原RPC准备与worker双模式、摘要篡改、费用变化撤销旧wire/证据、桌面/移动布局共9Chrome预检通过，13Node通过；旧无银行浏览器边界及完整统一门禁通过。|

首次真实Chrome预检已通过并检查桌面/移动截图。浏览器日志保留真实Chrome版本、harness字节hash和两种证据及截图摘要；预检不替代冻结门禁。报价仍为INDICATIVE，执行证据为给定银行的本地独立执行，不表示公开历史银行/当前共识/生产完整费用；签名、广播、真实银行捕获及其他未完设计项继续单列。

阶段100最终限定验收：16门禁全部exit0，168次Rust（仿真99/API15/准备RPC23/旧API19/cache7/路径5）、13Node、实际新9/旧9共18Chrome检查通过，全目标严格Clippy通过。真实双模式原RPC准备→缓存wire→worker→浏览器WebCrypto核对一致，错误unsigned摘要、费率变更旧证据撤销及旧无银行禁用边界均通过；桌面/移动截图已检查。源码/901固定输入/97参考文件前后相同，最终运行目录`350a4f4fc0c0459e8a4bd7e053d0ef0b`。来源/当前链/完整费用/钱包与其他工作包仍未完成，继续按设计差距推进，不将第100轮号视为全系统完成。

<a id="plan-2026-11-17-analytics-worker"></a>

## 显式常驻分析worker（2026-11-17，阶段101）

本批补“只有有限RunJournalJob”的剩余实现，新增同一个Go服务的明确配置role=analytics/mode=journal-continuous；原mode=journal仍为有限Job。不增加第四种服务，不打开online KV/lease或producer。每轮复用原完整只读日志和耐久checkpoint，完成后才等待poll_interval再串行开始下一轮，不积压ticker或并行周期。取消可打断等待及Reader的ctx，semantic/transport/retention/topic错误仍由原Job耐久撤销发布并停止，不自动跳过坏事实。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F101.1|正常服务常驻调度及明确隔离|WP19,WP21|LOCAL_SCOPE_VERIFIED|mode显式、Go analytics-only、Kafka只读且control disabled；窗口1..86400000ms/poll1..60000ms预校验，取消不留排队周期。|
|F101.2|实际服务跨轮/重启/故障事实|WP19,WP21|LOCAL_SCOPE_VERIFIED|独立本机Kafka中，当前Go程序跨周期消费新事实、SIGTERM干净退出、重启不重复计数，writer继续可提交；复杂交换使worker停止/游标保留/发布撤销，重启不跳过坏事实，不产生online KV文件。|
|F101.3|race/双语言配置及统一验收|WP01,WP19,WP21|LOCAL_SCOPE_VERIFIED|Go race调度/配置/原分析日志回归、两个Rust配置副本与完整格式/严格lint；正式源码/输入冻结验收通过。|

预检保留失败及修复：首次Rust配置测试发现旧Kafka约束要求解析器必须有PG，与Go analytics禁用PG约束冲突；现仅对明确Go只读analytics（有限/常驻）豁免，其他服务规则不变。分析窗口字段补入Rust Chain的可选字段以保持旧JSON省略语义；两个Solana测试构造仅补显式0。首次实际服务测试重写配置时试图序列化未使用adapter的空Amount，被原INVALID_AMOUNT拒绝；测试改为保留原配置JSON字段存在性后精确修改角色/参数，未把未知量默认成0。实际Kafka第二轮预检已通过，原记录保留。工具首轮Go命令在仓库根目录执行被模块边界拒绝，正式命令固定Go模块cwd，不创建根go.mod。

该常驻模式仍按原单轮max_tail_records/max_payload/timeout限额消费，历史积压超限明确停机并保留游标，不伪造完整历史。ClickHouse事实/迟到修订/shadow发布/查询与大规模保留继续待补；本机独立测试Kafka不等于真实多区验收。

阶段101首次统一门禁保留FAILED：格式通过，契约拒绝Rust测试直接引用另一服务Go配置文件。已改为读取各自本地配置并构造相同最小只读角色，不放宽跨服务依赖检查；验收器也为基础设施启动添加覆盖取消/启动异常的finally清理，重新完整运行。

阶段101第二次统一门禁保留FAILED：Go race96项及router配置40项通过；parser配置测试日志出现与实际parser源码不符的router测试名称，确认两个独立Rust workspace共用target目录复用了同名测试产物。现为parser使用独立持久化target，逐检查记录目标目录，避免把错误二进制运行算作该服务通过；原配置约束不放宽，重新完整复验。

阶段101最终限定验收：13门禁全部exit0，97次Go测试（含96定向race及当前服务真实Kafka集成1）、81次Rust配置测试通过，两个配置副本全目标严格Clippy、Go vet、格式/契约与进度检查通过。实际当前Go服务跨周期读取新事实、SIGTERM正常退出/重启幂等，复杂事实持久拒绝/重启不跳过、原writer继续提交和无online KV文件均通过；测试Kafka/PG由本轮创建并停止。源码与901固定输入前后相同，实际服务二进制及checkpoint/log均保留摘要，最终目录`f08d324113e5402c9c396380afc41bd7`。当前仅完成明确有界只读常驻调度，ClickHouse/持续事实保留、shadow发布/查询及其他设计范围仍待完成。

<a id="plan-2026-11-17-public-bootstrap-checkpoints"></a>

## 公开Protobuf checkpoint（2026-11-17，阶段102）

补齐设计§10.5公开跨语言快照基础：沿用公共OwnerBootstrap协议，把同次reducer Bootstrap的完整状态和committed next_offset一起承诺。旧本地JSON checkpoint保持兼容；新格式不含计算图、不推断owner资格、不创建latest指针，未取得真实cursor时不发布。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F102.1|Go不可变对象与摘要manifest|WP08,WP24|LOCAL_SCOPE_VERIFIED|先fsync payload再hard-link manifest；重复字节幂等、冲突/符号链接拒绝，发布前后owner/代际守卫。|
|F102.2|双Rust公开格式与当前Go互通|WP08,WP09|LOCAL_SCOPE_VERIFIED|先核验pin和长度再读payload；canonical JSON/protobuf、全宽i64/u64/U256、引用/游标一致；同轮Go3组原字节导出供两个独立Rust workspace读取。|
|F102.3|正常管理入口与冻结门禁|WP08,WP24|LOCAL_SCOPE_VERIFIED|授权checkpoint接口返回旧manifests及public_checkpoints；开发模式不编造cursor，race/并发/半上传/撤权/严格lint与源码和901固定输入冻结验收通过。|

该轮仅公开本地格式和管理入口；远端对象存储、canonical目录声明、可信来源选择、正常router恢复和独立故障域演练尚未完成。摘要只证明给定对象完整性，不赋予owner授权/当前性或Kafka topic生命周期证明。

阶段102最终限定验收：13门禁全部exit0，74次Go（checkpoint/journal/reducer73及正常服务checkpoint1）race测试、两个独立Rust transport workspace共42次测试（各含当前Go导出3组互通）通过，Go vet、当前Go服务二进制构建与双Rust全目标严格Clippy通过。源树和901份固定输入及当轮7个Go导出文件前后摘要一致，最终目录`72a3131d77f94d2aac61a29e7ac8de78`。manifest先pin后payload I/O、发布后撤权无资格返回、半发布续传/符号链接/并发幂等均通过；远端存储和正常恢复继续实现。

<a id="plan-2026-11-17-remote-checkpoint-objects"></a>

## 远端checkpoint对象读写（2026-11-17，阶段103）

沿用阶段102公开格式，补齐远端HTTP对象读写及正常Go管理入口。配置journal.checkpoint_store显式base_url和开发明文开关，凭据复用source_auth_tokens的精确base URL键；两个Rust配置副本解析及校验同样字段。仅HTTPS或显式127.0.0.1明文开发地址，禁止URL凭据/query/fragment/转义与相对路径、自动代理/跳转/解压；凭据不进入公开配置hash。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F103.1|不可变远端条件发布与精确回读|WP08,WP24|LOCAL_SCOPE_VERIFIED|上传前校验本地pin/完整状态，GET已存在对象比字节，缺失时If-None-Match:* PUT并GET回读；先payload后manifest，409/412仅精确字节成功，损坏/丢写/半上传/撤权不返回资格。|
|F103.2|受限下载与固定凭据范围|WP08,WP09|LOCAL_SCOPE_VERIFIED|URL仅配置base+内容SHA派生；manifest先pin/长度校验才请求payload，精确size/摘要/引用/游标，整轮timeout/ctx、声明/流式大小限额、状态/跳转/编码拒绝。|
|F103.3|正常checkpoint入口及跨语言配置|WP08,WP24|LOCAL_SCOPE_VERIFIED|授权管理入口返回remote_checkpoints；独立HTTP对象API成功/503经正常handler，Go race/配置hash与双Rust配置/transport回归及严格lint统一冻结验收通过。|

对象API合同要求实现原子条件PUT及read-after-write；当前不内置S3签名、供应商SDK或存储服务，也不把HTTP成功解释为独立故障域耐久/WORM。独立HTTP测试对象API实际读写但驻内存；真实云存储部署与目录资格、canonical声明和router自动远端恢复仍另列。首次定向配置测试误把未用adapter空Amount重新Marshal，受INVALID_AMOUNT拒绝；测试改用原配置文档字段存在性，原数值约束未放宽。

阶段103最终限定验收：13门禁全部exit0，142次Go race测试及两个独立Rust workspace共125次配置/transport测试通过；Go vet/正常服务程序构建、双Rust全目标严格Clippy、格式/契约和进度校验通过。源树/901固定输入与当轮Go跨语言导出前后相同，最终目录`7548bc48538a4691b01045ee2983d014`。真实HTTP对象API和正常授权handler已覆盖成功及故障边界；不把该限定实现写为云存储已部署、独立备份或正常router远端恢复完成。

<a id="plan-2026-11-17-current-remote-checkpoint-recovery"></a>

## 当前远端checkpoint冷恢复（2026-11-17，阶段104）

补正常router读取阶段103远端对象。远端完整回读后在本地对象目录写不可变/fsync的配置store专属完成标记；Go授权GET /internal/chains/{chain}/checkpoint-catalog只按当前Bootstrap的内容SHA派生精确对象并核对三个本地文件，不扫描、不拿较大高度旧文件冒充当前。返回前再次核对完整reference和cursor。半上传/标记损坏/其他store/旧seq或代际均无目录资格，客户端重启同目录可读完成标记。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F104.1|当前owner目录与耐久资格标记|WP08,WP24|LOCAL_SCOPE_VERIFIED|精确状态/cursor匹配、owner bootstrap前后守卫、已确认上传的不可变完成标记、重启可用及旧状态/上传一半拒绝。|
|F104.2|Rust远端下载与来源选择|WP08,WP09|LOCAL_SCOPE_VERIFIED|独立owner/store凭据范围、64KiB元数据/精确payload限额、pin/固定URL/owner-assignment-chain-topic核对、整轮timeout、下载后同目录再核对；远端失败可转当前owner原bootstrap，不直接激活下载结果。|
|F104.3|正常router冷内存恢复与实际报价|WP08,WP09,WP24|LOCAL_SCOPE_VERIFIED|限定实际Kafka单分区与正常sync_sources/HTTP handler，读取当前远端checkpoint并沿用原Reader保留窗/基线年龄/Replica资格，故障源暂停；同轮Go3组原wire实际HTTP下载、双Rust配置/transport与严格lint冻结验收通过。|

本轮目录是当前owner在线资格化入口，要求精确当前状态已发布；没有该对象时正常代码可回退owner bootstrap。尚无canonical CheckpointAvailable公告、离线peer目录、自动周期发布、旧快照下载期间有限尾窗追平和真实独立存储故障域，不能据此写WP08/24整体完成。首次定向service测试误用相同块号BLOCK_COMMIT，原NON_CONTIGUOUS_BLOCK正确拒绝，现生成连续下一块。首次Rust预检仅测试构造误用Producer/Amount符号，修正为公开KafkaProducer/checked接口。首次实际Kafka预检已经恢复出EVM视图，测试目录次数却混入未配置对象的Solana探测（3而非2）；实际验收限定明确EVM链，未放松目录二次核对。

阶段104首次统一门禁保留FAILED：Go143次、双Rust125次配置/transport和API35次/当轮Go wire下载1次通过；严格Clippy拒绝新测试`&[broker.clone()]`的多余clone，已改`std::slice::from_ref`，不添加lint豁免。该失败发生在真实Kafka步骤前，不计为该步骤运行通过；完整重跑冻结门禁。

阶段104最终限定验收：18门禁全部exit0，143次Go race及162次Rust测试（双配置/transport125、API35、当轮Go原wire真实HTTP下载1、明确独立Kafka冷恢复1），两个workspace配置/transport及router API全目标严格Clippy、Go vet/两个当前程序构建、格式/契约与进度检查通过。实际冷内存router请求顺序为owner目录→配置对象manifest→payload→owner目录复核，Kafka游标partition0/offset3，正常授权POST /v1/quote得到1000→996；没有读取owner原bootstrap，损坏恢复时不可用。源树/901固定输入及当轮Go导出文件前后相同，运行目录`66d5d98f9afb4b99b9d50a7ba1a53ce8`，实际API证明摘要`f225338e839f8471060482a19574a580e71270962488c2b0859de69db3cb0f43`。限定当前owner在线目录/单机Kafka/测试HTTP对象API；自动发布、旧快照追平、canonical公告及真实故障域仍继续。

<a id="plan-2026-11-18-checkpoint-worker"></a>

## 周期checkpoint发布（2026-11-18，阶段105）

在同一个正常Go解析服务内增加明确的周期发布，不新增服务，不使用分析角色或改变owner状态。journal.checkpoint_store.publish_interval_ms默认0手动，启用时100..3600000ms，只允许Go online。每周期完成后才启动等待timer；与手动入口共用一个可取消通道，整次发布共用request_timeout_ms预算，不持actor锁做网络I/O，保留原JSON返回兼容及完整Protobuf/远端回读/guard。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F105.1|显式周期发布和正常入口复用|WP08,WP24|LOCAL_SCOPE_VERIFIED|正常Start接worker，0保持手动；失败不产生资格，下一周期可同字节续传，更新当前代际自动发布；未取得真实cursor不编造远端对象，不改变owner或committed cursor。|
|F105.2|取消、关闭及共享发布通道|WP08,WP09|LOCAL_SCOPE_VERIFIED|慢周期不并发/排队ticks，排队手动请求ctx可取消；Close取消手动和后台上传并等退出后关store；父ctx取消后台后手动仍可独立操作，关闭后不返回catalog。|
|F105.3|配置/状态/指标与回归|WP01,WP08,WP24|LOCAL_SCOPE_VERIFIED|两Rust配置识别并拒绝在Rust角色误启周期；授权status及cycles/failures指标、checkpoint.uploaded/failed事件；正常HTTP WriteTimeout覆盖发布总预算，race/Go vet/双Rust严格lint和真实Kafka恢复回归冻结验收通过。|

周期worker只归档已由owner提交的状态，不发canonical事务、不持lease/producer，不把上传失败改成成功，也不以周期次数当上传成功次数。状态接口区分completed_cycles/failed_cycles；错误日志为固定错误码，不包含token或存储URL。实际独立云存储、Rust owner自动发布、canonical CheckpointAvailable、旧快照追平/分片保留仍在范围差距中。

阶段105最终限定验收：18门禁全部exit0，148次Go race（含正常服务5项与原checkpoint/config/reducer/journal/app回归）及164次Rust测试通过，Go vet/当前Go程序与router构建、双Rust配置/transport及API全目标严格Clippy通过。同轮Go原wire下载和明确独立Kafka冷内存恢复/正常报价/损坏暂停均回归通过；源树/901固定输入和Go导出文件前后相同，最终目录`1c2824b26c74477cb0494f0d92997742`。周期发布的限定运行证明为单链，覆盖无cursor不造位置、失败恢复/新代发布、共享gate取消、Close中断手动与后台、父ctx取消不污染手动及不改owner状态/cursor；多链公平性/单链失败隔离接续实现，不把当前“整批首错返回”写成多链已完整验收。

<a id="plan-2026-11-18-checkpoint-fairness"></a>

## 多链checkpoint公平调度（2026-11-18，阶段106）

补阶段105明确未验收的多链故障隔离。后台每次按配置顺序选择一条链，在执行前推进轮转位置；即使该链上传失败/无游标或超时，下一周期仍处理后续链。每条链只占用一次request_timeout_ms预算，所有上传仍共享单一可取消发布通道，不把坏链失败变成成功。手动POST仍按原整批语义，首个错误明确503，不改变后台轮转位置。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F106.1|串行公平轮转及错误范围隔离|WP08,WP24|LOCAL_SCOPE_VERIFIED|每轮一链、先推进cursor再执行，失败不重置起点；同一gate/ctx/Close路径复用，事件携带chain_key。|
|F106.2|正常服务双链故障/更新/恢复|WP08,WP09,WP24|LOCAL_SCOPE_VERIFIED|两个受控EVM链fixture和同一实际HTTP对象API，一链持续503时另一链可发布及新generation更新，故障链无目录资格，修复后自动恢复，网络并发不超过1。|
|F106.3|冻结验收与原生命周期回归|WP01,WP08|LOCAL_SCOPE_VERIFIED|race/checkpoint/config/reducer/journal/app、正常服务新旧发布/取消/关闭/手动批次、Go vet/当前程序构建/契约与进度门禁通过；未修改Rust或协议格式，阶段105真实Kafka证明不冒充本轮重跑。|

publish_interval_ms现在表示全服务相邻链轮次之间的等待；N条链走完整轮至少N个interval加各自上传耗时，避免隐含每条链都有同等独占频率。该轮证明是正常Go服务中的两个受控链状态及HTTP对象API，不表示实际两条公共链/独立云存储故障域验收。

阶段106最终限定验收：7门禁全部exit0，149次Go race（核心143及正常服务新旧6）通过；Go vet、当前正常程序构建、格式/契约与进度校验通过。源树及901固定输入前后同批绑定，运行目录`6e4d86b185e4491aaca5a62f9355f840`。双受控链+HTTP对象API已验证持续坏链不饿死健康链、健康新generation继续、手动503不重置调度、故障链恢复和网络并发上限1；沿用原关闭/取消回归。该轮仅Go调度与测试变动，Rust/原wire/真实Kafka先前证明明确引用阶段105，不重复计为本轮运行。

<a id="plan-2026-11-18-checkpoint-tail-replay"></a>

## 旧checkpoint有限尾窗追平（2026-11-18，阶段107）

补周期发布落后于当前区块时的正常恢复。成功回读对象/写完成标记后更新store专属candidate提示文件；该文件可变但不赋资格，reader必须核对pin、不可变manifest/payload/完成标记并由当前owner重新资格化。新授权GET /internal/chains/{chain}/checkpoint-recovery-catalog返回已上传checkpoint及同一次reducer锁内读取的head_reference/head_cursor；只允许同owner/assignment/writer/canonical/generation/schema/scope/adapter，seq/块/cursor不得倒退或超限。原exact-current目录仍保持原语义。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F107.1|owner旧对象/当前头控制面资格|WP08,WP24|LOCAL_SCOPE_VERIFIED|candidate只作索引，完整本地不可变对象/标记复核、无损i64头游标/完整ref、半上传/损坏/不同代/错误源/尾窗超限拒绝，head读取不复制全部池。|
|F107.2|Rust有限尾窗私有重放与最终激活|WP08,WP09|LOCAL_SCOPE_VERIFIED|正常restore先试owner recovery目录；下载旧Protobuf后用独立Replica及实际Reader只读连续同代BLOCK_COMMIT，帧数/整轮deadline/保留窗/引用/cursor精确校验；中间图不进入公共replica，完整边界且二次目录一致才激活。|
|F107.3|双语言/正常服务与实际Kafka|WP01,WP08,WP09,WP24|LOCAL_SCOPE_VERIFIED|Go管理入口旧对象绑定新head、BootstrapHead与原ready一致；Rust旧对象+实际2块、目录撤权/半事务边界/预算拒绝、正常HTTP报价与原冷恢复回归，源码和固定输入冻结验收通过。|

有界追平只接受同代普通连续块；reorg/范围撤销/新baseline/未知kind明确转当前源，不声称跨代历史恢复。Reader新增完整atomic unit边界查询，游标/序列checked_add和高水位倒退拒绝，不把消费组offset当状态。阻塞任务只持私有状态且有共同deadline，外层取消后不可能发布公共视图；底层Kafka单次请求受剩余时间限制。candidate更新失败仍返回发布失败；文件提示即使留下旧值或被篡改也不能越过对象/owner资格检查。

阶段107实际Kafka预检保留FAILED：旧checkpoint+2块追平、私有不泄漏/目录撤权/半事务边界/低预算拒绝及正常sync最终head均已执行，随后手写HTTP请求误用snake_case被公开接口INVALID_REQUEST_AMOUNT_MUST_BE_STRING拒绝；测试改为规定camelCase，未放宽业务反序列化。正在进行的旧冻结批次已显式取消为CANCELLED（非通过），修正后重新完整门禁。

阶段107最终限定验收：19门禁全部exit0，152次Go race、166次Rust（双配置/transport127、API36、Go原wire HTTP下载1、实际Kafka exact-current冷恢复1及旧checkpoint有限重放1）通过；双Rust相关全目标严格Clippy、Go vet/两个当前程序构建、格式/契约与进度检查通过。真实Kafka旧offset3/seq1经两个完整BLOCK_COMMIT私有重放到offset11/seq3，正常HTTP报价成功，目录撤权/目标落半事务/低尾窗预算拒绝且公共视图不提前可用；最终仅激活精确head。源树/901固定输入及Go导出文件前后相同，运行目录`339d7dd3e96645ccbdf9e023e18bd2a5`。实际API/重放证明保存于同批actual-api目录，摘要见report；跨代/reorg、真实保留过期/丢盘、多区故障域与canonical离线目录仍待补。

<a id="plan-2026-11-18-rust-owner-checkpoints"></a>

## Rust owner公开checkpoint（2026-11-18，阶段108）

补Rust解析owner缺少公开文件格式发布的差距。沿用阶段102公共OwnerBootstrap/manifest版本2，双Rust kafka-transport新增checkpoint::local::encode/write/read，先校验完整快照/cursor再编码，文件fsync/目录fsync/硬链接不可替换，既有精确字节幂等、冲突/符号链接拒绝，来源guard在编码前/后、manifest前/后运行。配置root及祖先为operator管理，新对象目录私有，不供不可信本地进程修改。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F108.1|双Rust公开编码/本地不可变文件|WP08,WP09|LOCAL_SCOPE_VERIFIED|相同公共payload/规范manifest、size/pin先读、半发布续传/并发/冲突/namespace及对象符号链接拒绝；当轮两workspace独立导出原文件字节供Go。|
|F108.2|正常Rust owner管理入口和关闭|WP08,WP24|LOCAL_SCOPE_VERIFIED|POST /internal/checkpoints认证、checked_current/租约与完整committed bootstrap同源，开发模式无cursor返回空数组；单一有界gate、取消flag/owned gate跨blocking写任务，Close发送stop后等待gate排空，网络/actor不跨文件写锁。|
|F108.3|双向互通/正常HTTP和冻结门禁|WP01,WP08,WP09|LOCAL_SCOPE_VERIFIED|Go→双Rust原向量回归，双Rust→Go全宽offset/u64/2^200独立读取；当前Rust parser正常loopback HTTP成功/认证/重试/关闭/门限及原service ready/scope/race回归、严格lint/当前程序构建通过。|

本轮管理入口返回public_checkpoints；manifests/remote_checkpoints为空数组，不伪装旧JSON或远端对象已存在。本轮Rust owner仅本地公开发布，远端条件上传/目录和周期接续；codec可处理由契约验证支持的快照，但正常服务测试使用受控fixture来源，不将其写成公开真实Solana bank验收。首次rustfmt预检因模块声明置于inner doc之前被拒，现将模块置于文档后，不改变格式检查要求。

阶段108最终限定验收：15门禁全部exit0，20次Go（checkpoint18及读取两个Rust workspace当前导出各1）和60次Rust（两个独立transport各26、正常parser service相关8）通过；双Rust全目标相关严格Clippy、Go vet、当前Rust parser正常程序构建、格式/契约及进度校验通过。源树/901固定输入和当轮21个导出文件前后相同，运行目录`ea2d71c8364c4e4f807588634f8089e9`。实际Rust loopback POST认证/committed宽cursor绑定/重复发布/关闭及有界排队取消通过，正常HTTP manifest/payload原字节保存在actual-parser证明内；原ready/scope取消与最后authority竞争回归通过。本轮独立parser target首次编译固定RocksDB原生依赖（test与Clippy各自构建产物），期间不替换跨workspace测试程序或提前记通过。Rust owner远端上传/目录与周期继续接线。

<a id="plan-2026-11-18-rust-remote-checkpoints"></a>

## Rust owner远端checkpoint（2026-11-18，阶段109）

在阶段108正常管理入口接同一个配置HTTP对象API。先本地不可变发布及原字节复核，再GET已有对象精确比较/条件PUT/完整回读，payload先于manifest；各阶段checked_current检查owner/lease/代际，全部确认后写store base专属不可变remote证书并再次guard。当前GET目录按当前完整bootstrap内容寻址，只在manifest/payload/证书都精确匹配时返回，不用其他代际/其他store的标记。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F109.1|正常Rust远端条件发布及耐久证书|WP08,WP24|LOCAL_SCOPE_VERIFIED|固定base路径/精确token、禁止代理/跳转/编码、If-None-Match:* PUT+精确回读、半上传续传/冲突不覆盖，source guard后certify固定安全文件名/64KiB上限/原pin一致。|
|F109.2|当前目录与取消/关闭|WP08,WP09,WP24|LOCAL_SCOPE_VERIFIED|授权catalog只匹配当前reference/cursor和同store完整标记，源变化拒绝；handler select stop watch可立即取消远端HTTP，blocking本地任务仍持owned gate直到退出，Close等待排空后关租约。|
|F109.3|实际双HTTP服务与独立Go解码|WP01,WP08,WP09|LOCAL_SCOPE_VERIFIED|正常Rust owner HTTP与独立对象API半上传/重试/来源切代/伪成功/回读损坏/跳转/编码/关闭中断；Go读取该次原manifest/payload/宽cursor；双Rust原codec/证书/服务竞争与strict lint/当前程序构建冻结验收通过。|

仅使用已配置的条件PUT/read-after-write HTTP对象API，不内置S3签名或宣称独立云存储耐久/WORM。Rust周期配置仍拒绝启用，当前目录仅精确当前state，旧对象recovery目录/有限尾窗在Rust owner后续接入。首次严格Clippy预检拒绝测试handler复杂tuple类型，已提取ObjectState命名类型，未加allow或放宽门禁。

阶段109首次统一门禁保留FAILED：格式/契约/进度通过，初始Go回归误提前设置“当前Rust实际HTTP产物”环境变量，导致产物尚未生成时读取失败；已仅在正常Rust服务导出后执行该独立读取检查，初始Go回归保持该明确可选测试skip，不伪造旧产物。定向实际Rust远端导出→Go解码已独立通过，正式批次重新完整运行。

阶段109最终限定验收：16门禁全部exit0，21次Go（原checkpoint18、读两个Rust当前向量各1、读取实际Rust远端发布原Solana fixture wire1）及66次Rust（双transport各27、正常parser service12）通过；双Rust相关全目标严格Clippy、Go vet、当前parser程序构建、格式/契约/进度检查通过。源树/901固定输入及21个独立向量文件前后相同，最终目录`3af2bf8bda064a86a4e0171da0f95ae4`。两个真实loopback HTTP端口已验证half-upload无资格/原字节续传/条件幂等、当前目录宽offset绑定、不改owner cursor、source代际变更阻断manifest、伪成功/损坏/跳转/编码拒绝，以及Close在500ms内中断挂起对象请求并排空gate；两个实际HTTP原字节证明摘要见同批report。Rust周期发布、旧对象recovery目录/追平、独立云存储和公共真实bank资格仍待补，不改变生产关闭状态。

<a id="plan-2026-11-18-rust-periodic-checkpoints"></a>

## Rust周期checkpoint与配置摘要（2026-11-18，阶段110）

补阶段109明确保留的Rust周期生命周期。publish_interval_ms仍默认0，100..3600000现在允许Go/Rust online owner，router与analytics不允许；两份Rust配置和Go共享规则同步。Rust每轮选择一条链并先推进轮转位置，经同一有界发布gate、normal source guard和条件对象上传，完成后才sleep，不排队tick；错误记录固定码/chain_key，后续链继续。Close先广播stop，取消网络/加入worker，再排空本地gate及关闭owner资源。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F110.1|Rust显式周期和多链公平性|WP08,WP09,WP24|LOCAL_SCOPE_VERIFIED|默认手动无上传、双链持续失败隔离/健康链新代际/坏链恢复、手动503不重置轮转、owner state/cursor不变；active/cycles/failures状态和指标明确。|
|F110.2|正常serve关闭先取消后drain|WP08,WP24|LOCAL_SCOPE_VERIFIED|实际serve路径在signal后先发送stop，使待完成HTTP上传不阻塞graceful drain；worker join及本地owned gate排空后关租约，tick在stop后不继续工作，关闭末尾重申started=false。|
|F110.3|共享配置默认值与公有摘要互通|WP01,WP08|LOCAL_SCOPE_VERIFIED|修正Rust原始字段缺省与Go typed API/journal默认值造成public_hash分歧；统一编译后typed容器再去凭据，经济hash语义不变。Go生成5组原配置，双Rust独立load核对省略/显式默认、周期参数和token不入hash，正常API/原发布严格回归冻结验收通过。|

public_hash规范化属于编译器语义修正：省略API quote默认值/optional checkpoint默认值的旧Rust配置在新程序加载时可能得到新的公有hash，与Go一致；仍须正常重启加载新ConfigSnapshot，不在运行中替换配置或承认旧hash可复用。旧Go/Rust测试selector为历史报告定位保留，新增明确双owner许可测试；阶段105/108/109当时Rust周期禁用的历史结果不改写。

正常周期证明使用两个受控Solana fixture链与实际HTTP对象API，不表示公共bank或独立云存储。completed_cycles包含无cursor/无对象轮次，不等于成功上传数；仅完整remote回读/cert后记录uploaded事件。Rust旧对象恢复目录、存储配额/保留及真实故障域继续按差距补齐。

阶段110最终限定验收：21门禁全部exit0，92次Go race（checkpoint18/config65/原worker6及3次独立Rust数据读取）和194次Rust（两个独立config/transport71+72、parser服务15、router API36）通过；双Rust相关全目标严格Clippy、Go vet、两个当前正常程序构建、格式/契约与进度检查通过。源码/901固定输入及27个同轮独立配置/数据向量文件前后相同，运行目录`29b21c5065534cb2a27fee751dcb2f24`。实际双受控链周期/持续坏链/健康新代际/恢复/不改owner cursor/默认手动与正常serve shutdown停止待完成HTTP并加入任务通过；Go5组配置经两个Rust各自load公有/经济hash一致，省略默认值和凭据变化边界通过。实际normal Rust HTTP及双链周期证明摘要保存在同批report；本轮不把默认忽略的外部Kafka API测试写成实际Kafka运行，有限Kafka证明仍引用阶段107。Rust旧对象recovery目录、存储配额/保留与真实独立故障域继续。

<a id="plan-2026-11-18-rust-owner-recovery"></a>

## Rust恢复目录与独立router追平（2026-11-18，阶段111）

补Rust旧对象恢复目录及真实跨owner/router进程接线。成功条件上传/原字节回读/不可变cert后，原子更新store专属candidate提示；目录按提示读取精确原对象和cert，并对当前已检查owner的锁内BootstrapHead重新资格化，返回前再核对reference/cursor。共享RemotePublication/RecoveryCatalog/recovery_target移至两份kafka-transport，owner/router不各写不同校验。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F111.1|Rust旧对象目录及共享v1/v2约束|WP08,WP09,WP24|LOCAL_SCOPE_VERIFIED|候选只提示、完整对象/cert/URL/pin/head检查、过期权限/损坏/不同writer拒绝。v1同代规则保留，v2仅在相同owner/assignment/writer/canonical/schema/scope/adapter内允许generation单调前进。|
|F111.2|完整baseline前进与私有重放|WP08,WP09|LOCAL_SCOPE_VERIFIED|Rust正常Store只支持完整baseline和revoke，未伪造BLOCK_COMMIT：v2私有Replica允许真实连续ONLINE_BASELINE_COMMIT经过install_baseline推进generation，其余BLOCK仍按原apply验证，有限帧/时间/完整目标/目录复核后才激活。reorg/writer/scope改变仍转当前源。|
|F111.3|独立双进程正常HTTP与真实Kafka|WP01,WP08,WP09,WP24|LOCAL_SCOPE_VERIFIED|实际正常Rust owner HTTP与对象API保持在线，真实Kafka发布旧baseline后两次完整baseline；独立router程序下载/私有追平/normal sync及实际HTTP报价，缺owner凭据拒绝，Go独立读取原Rust目录/数据；原v1 EVM实际Kafka回归及同源门禁通过。|

首次预检正确拒绝测试误用的BLOCK_COMMIT（PREPARED_KIND_UNSUPPORTED），据真实入口调整为两次完整ONLINE_BASELINE_COMMIT，不放开Store未知kind。为了支持设计中的最新完整基线，不再把所有generation变化视为永久外部阻塞；只对目录v2开放经过当前owner资格化的完整baseline单调升代，不允许跨writer/canonical/scope/schema/adapter。版本化控制面与原manifest v2/Protobuf数据不变。

实际跨进程预检已通过：不同独立workspace当前程序，正常owner/对象HTTP和真实Kafka尾窗，router只在精确head后发布并通过HTTP报价；受控source/测试producer用于构造fixture，不表示真实Solana bank/PG控制面或独立云存储故障域。最终冻结结果见本节末尾。

阶段111并行严格lint预检拒绝新certify调用中的多余`&guard`，已按建议传可复制closure值，不添加allow。知晓该失败后显式取消尚未到lint步骤的第一冻结批次（CANCELLED），修正后完整重跑；独立跨进程Kafka预检已实际通过，但仍不替代最终冻结门禁。

阶段111最终限定验收：26门禁全部exit0，93次Go race及204次Rust通过；双Rust config/transport/parser及router API相关全目标严格Clippy、Go vet、两个当前正常程序构建、格式/契约/进度与共享配置摘要通过。实际独立Rust owner与router测试程序保持双正常HTTP服务在线，以真实Kafka旧offset3/generation3/seq3和两次完整baseline，私有追平到offset11/generation5/seq5；正常router HTTP报价1000→996，未提前发布私有状态，精确当前目录对旧对象仍拒绝，缺owner凭据不能ready。两项原v1 exact-current与两块BLOCK_COMMIT Kafka回归也实际执行，非默认ignored冒充通过。源树/901固定输入及27份同轮向量前后相同，最终目录`272eb5e7940b447ba41a859bf02ca1f6`，独立router证明SHA256 `3adcba3abc420e4f025a549c081ba61768f4403168043c588756990cc356da5f`。受控Solana来源/测试producer与单broker本机环境不代表真实bank/PG lease/独立云存储，多区/保留/容量与canonical离线目录继续补齐。

<a id="plan-2026-11-18-checkpoint-local-budgets"></a>

## 公开checkpoint本地容量门禁（2026-11-18，阶段112）

补持续周期归档无本地公开对象上限的差距，配置checkpoint_store.local_budget显式max_bytes/max_files/max_entries，省略保持兼容的不启用；非空要求bytes1..1TiB、files1..100000、entries≥files且≤1000000。两个owner在现有共享发布gate内预先计算payload/manifest/remote证书/candidate的精确逻辑大小，流式枚举public-v2命名空间、统计所有普通文件（含未知/遗留staging），拒绝符号链接/非普通文件/深度>4/扫描超限；新对象会超限则先失败，既有精确重试不重复计费，不删除旧数据。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F112.1|双语言public-v2配额及有界扫描|WP08,WP24|LOCAL_SCOPE_VERIFIED|逻辑大小/文件数/目录条目、待写metadata和候选更新规划，existing retry/新generation/未知文件/过深与链接拒绝、取消guard；Go导出实际对象和精确Usage供两个Rust独立核对。|
|F112.2|正常owner在I/O前准入与Go周期去重|WP08,WP09,WP24|LOCAL_SCOPE_VERIFIED|配置在Start前固定，正常手动/周期超限先失败且不请求远端、不写public对象、原候选仍在；status返回配置budget。Go后台只写公共Protobuf，不再每周期复制旧JSON，手动JSON接口保持原兼容。|
|F112.3|配置/hash互通与恢复回归|WP01,WP08,WP09|LOCAL_SCOPE_VERIFIED|Go3组budget配置经双Rust读取及public/economic hash一致、当轮Usage/原对象精确一致；Go race/双Rust相关测试与严格lint、正常HTTP/独立Kafka恢复、当前程序构建冻结门禁通过。|

配额只覆盖按链public-v2普通文件的逻辑大小/数量，不能等同实际磁盘块/底层KV/旧手动JSON/远端对象总容量。命名空间要求单一owner进程写入，actor/gate串行保护不承诺多进程共享目录配额；原子写临时文件与元数据的瞬时文件系统分配不由该逻辑quota预留。当前仅拒绝新增，不自动删恢复对象；省略预算不宣称容量有界，自动保留/远端配额/独立磁盘水位仍待实现。

阶段112最终限定验收：26门禁全部exit0，97次Go race（checkpoint20/config66/正常worker7及4次独立Rust原数据读取）和217次Rust通过；两workspace config/transport及正常parser/router API相关全目标严格Clippy、Go vet/两个当前程序构建、格式/契约/进度通过。源树/901固定输入与36个同轮配置/对象/预算向量文件前后相同，运行目录`d3ddebee2fe64f578920310e3bbdbc40`。Go/Rust正常owner显式极小budget均在public文件/remote请求前拒绝；已有合格对象保持，重复发布不重复计费，未知文件计入、scan/深度/符号链接拒绝通过；Go实际4文件/5entry及精确字节投影由两个Rust各自核对，3份budget配置public/economic hash一致。独立正常Rust owner/router真实Kafka两baseline恢复与原v1 EVM两块/精确当前恢复均实际回归通过；配额关闭/全盘/远端/多进程/自动保留的边界继续单列，未写为生产容量已完成。

<a id="plan-2026-11-18-checkpoint-retention-plan"></a>

## 有界checkpoint只读保留计划（2026-11-18，阶段113）

在执行任何回收前建立可核验保留计划。Go显式配置local_retention，认证GET /internal/chains/{chain}/checkpoint-retention-plan与发布共用gate/ctx/关闭取消，读取同次完整Bootstrap并在返回前核对精确reference/cursor。计划不写文件、不删除对象、不发送远端请求；applied固定false。后续执行器必须重新检查目录和owner，不把持有planId当删除授权。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F113.1|完整证据和保留规则的只读计划|WP08,WP24|LOCAL_SCOPE_VERIFIED|完整manifest/payload解码及同store上传证书，所有store候选引用/当前内容SHA及最近keep_latest备份保护；最小链状态年龄+本地manifest年龄，未来owner身份保护，最旧eligible按max_remove有限选择。未知/不完整对象只metadata并保留，损坏/链接拒绝计划。|
|F113.2|显式资源上限与证据绑定|WP08,WP09|LOCAL_SCOPE_VERIFIED|keep_latest≥2，最小年龄/最多选择64/对象4096/条目100000/总读取256MiB各自显式上限；原文件字节及metadata摘要、覆盖标识、policy/owner/cursor/观察时刻共同绑定planId，取消逐条检查；只读重复预览命名空间摘要相同。|
|F113.3|正常入口与跨语言配置回归|WP01,WP08|LOCAL_SCOPE_VERIFIED|正常Go认证HTTP预览/取消/过期owner/关闭拒绝且无远端请求；6对象/2store候选/最新备份和不变原数据；Go2组retention policy由双Rust读取/校验hash，Rust实际owner启用仍拒绝，完整冻结回归通过。|

local_retention字段keep_latest2..1000、min_age_ms1..31536000000、max_remove1..64、max_objects≥keep_latest且≤4096、max_entries≥max_objects且≤100000、max_read_bytes1..256MiB；省略不启用。策略目前仅Go online已实现预览，两Rust配置必须识别同字段用于公共compiler，但拒绝在Rust owner/router/analytics误启，避免接受无效果设置。未来身份对象不占用当前域最近备份数量；候选保护可超过keep_latest。

目录摘要的覆盖口径明确为FILE_METADATA_AND_VERIFIED_KNOWN_OBJECT_BYTES：合格对象完整解码/原字节及store证书验证；unknown/incomplete/other-owner对象仅metadata、不会被选。planId不是全文件系统内容证明或外部删除授权。首次预检发现CURRENT_STATE原因被候选原因覆盖以及scan测试预算未实际耗尽，已分别保留更强当前保护原因/收紧有效扫描预算；另一Rust配置测试字符串赋值编译错误已修正为拥有的String，未改编译/lint门禁。实际删除、崩溃可恢复回收、自动触发与Rust保留计划继续接续。

阶段113最终限定验收：26门禁全部exit0，103次Go race（checkpoint23/config68/正常worker与preview8及4次独立Rust数据读取）和221次Rust通过；双Rust相关全目标严格Clippy、Go vet/两个当前正常程序构建、格式/契约与进度检查通过。源码/901固定输入及39份同轮配置/对象向量前后相同，运行目录`d5484add6a0c4ff1995641baaf8f4af2`。6个完整归档对象/2个store候选验证保护当前、全部候选和最近备份，仅选择最旧2个合格对象；同观察时刻重复preview的planId/namespace摘要一致，所有原文件仍可完整解码。正常Go HTTP preview认证/只读/取消/过期owner/关闭拒绝且不访问远端通过；2份策略配置由双Rust独立读取并核对hash。独立正常Rust owner/router实际Kafka两baseline及原v1精确当前/2块追平均实际回归通过。只读预览applied=false，未知对象覆盖明确为metadata，实际回收/自动触发/崩溃恢复与Rust预览尚未完成，不以该轮号宣称保留功能或全设计完成。

<a id="plan-2026-11-18-rust-retention-plan"></a>

## Rust只读保留计划与整计划互通（2026-11-18，阶段114）

补阶段113的Rust同等预览，不执行删除。双Rust kafka-transport新增retention::plan，使用同一显式policy和Go一致的当前/候选/最近备份/age/未来身份规则、原对象解码/证书、资源上限与证据格式。共享配置允许两个online parser启用local_retention，router/analytics继续拒绝；阶段113当时仅Go可启的历史结果保留，当前许可由新增测试明确。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F114.1|双Rust有界只读选择器和完整证据|WP08,WP09|LOCAL_SCOPE_VERIFIED|完整payload/cert与所有候选保护、metadata覆盖/未知不选、最旧有限选择、namespace/planId/policy/ref/cursor/观察时间绑定，utf8和可表达ns时钟拒绝边界与Go对齐。|
|F114.2|正常Rust预览HTTP/来源/关闭|WP08,WP24|LOCAL_SCOPE_VERIFIED|认证GET与发布同一owned gate，阻塞读取持gate直到退出，stop/取消flag/逐entry source-head守卫；checked_current前后/最终cursor核对，过期owner/关闭拒绝，不发送对象API请求、不写文件。|
|F114.3|同目录全计划独立互通与冻结门禁|WP01,WP08,WP09|LOCAL_SCOPE_VERIFIED|Go保留当轮6对象/2store候选原目录，两个Rust各自读取；逐字段JSON、内容覆盖/原因/选择/bytes/读数/目录sha/planId全部对照，而非只比几个字段；配置hash、两normal HTTP/取消及实际Kafka回归门禁通过。|

Go在导出向量时保留原对象目录供Rust只读分析，原观察时刻固定，生成后/消费后的完整文件摘要均验证。只读metadata使用mtime纳秒，Go/Rust都拒绝无法无损表达为i64 ns的文件时钟与非法UTF8路径，避免在异常布局上生成不同的命名空间证据。普通平台文件原子性与单writer前提不扩展为多进程分布式文件锁；计划始终applied=false，下一步回收执行与中断恢复仍需单独实现。

阶段114首次冻结门禁保留FAILED：格式/契约/进度通过，Go新增非法UTF8路径控制在APFS创建文件时即被EILSEQ拒绝，未进入planner。测试现只针对该明确平台前提标SKIP（文件系统已失败封闭），其他文件创建错误仍FAIL；可创建的损坏/证书/候选/符号链接控制不跳过。该主机不声称已直接运行“观察既有非法UTF8路径”的分支；完整重跑冻结验收。

阶段114最终限定验收：26门禁全部exit0，104次Go race与227次Rust（两个独立config/transport83+84、normal parser20、router API36及4次实际Kafka）通过；相关双workspace全目标严格Clippy、Go vet/两个当前正常程序构建、格式/契约/进度均通过。源码/901固定输入及61份同轮配置/原对象向量前后相同，运行目录`1ffa11df6e5d4aaa9c457d928bc64b22`。两个Rust直接读取当轮Go保留原目录，整个计划JSON含每对象保护/选择/覆盖、资源读数、namespace_sha256、plan_id逐字段相同；两normal HTTP认证/只读/无远端访问/过期owner与关闭/等待者取消均通过。该主机APFS在文件创建时即拒绝非法UTF8，特定前提子用例明确SKIP，未冒称planner已观察到不可创建路径；其余损坏和符号链接控制正常执行。独立正常owner/router实际Kafka恢复、原v1冷恢复/两块追平均显式回归通过；所有计划applied=false，实际回收/崩溃恢复/自动触发及全设计其余差距继续。当前逐项表已把阶段102–113中已由后续轮完成的旧“待接入”条目指向对应完成轮，历史限定结论未改写。

<a id="plan-2026-11-18-checkpoint-reclaim"></a>

## 显式Go checkpoint回收与中断恢复（2026-11-18，阶段115）

对照设计§10.5/17.7/18补阶段113–114明确未做的执行。新reclaim_enabled默认false，只有Go online可显式true且plan_ttl_ms1000..600000、API token24..8192字节才允许。原GET预览始终不改文件，开启执行后仍需认证POST apply提交plan_id/observed_at_ms，原policy/source/cursor/目录重新规划相同且未过期才继续；请求仅固定小JSON、拒绝重复/未知/null/trailing/非法ID，不接受路径或候选列表。Rust compiler识别参数但拒绝Rust服务启用执行，避免无效开关。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F115.1|显式选择/原远端备份/来源资格|WP08,WP24|LOCAL_SCOPE_VERIFIED|新操作重新核对完整原计划，复制完整文件清单/长度/哈希并重读配置store原manifest/payload；当前/全部候选/最近备份与资源预算保持；公共执行入口必须有authority guard及单writer发布gate。|
|F115.2|有界journal、隔离和中断恢复|WP08,WP09,WP24|LOCAL_SCOPE_VERIFIED|先耐久自校验state.json，再rooted rename到retirements/<op>/<sha>并fsync两父目录，QUARANTINED记录后逐文件验证/释放；fresh owner可从rename/半删除/目录已删/完成回执前恢复，pending不重选且重新检查候选/保留/远端/文件。|
|F115.3|正常HTTP/幂等/确认状态与容量|WP08,WP24|LOCAL_SCOPE_VERIFIED|认证apply/resume/status、原预览配置不启删除、未知/保护引用/损坏/链接/未来源/配置store变化/取消拒绝；未确认完成不写成功，已提交但未送达回执可幂等确认；单个1MiB/4096文件有界控制journal及累积计数，pending阻断后续checkpoint发布避免隔离空间被当成空闲。|
|F115.4|真实崩溃/独立恢复和统一验收|WP01,WP08,WP09|LOCAL_SCOPE_VERIFIED|九个阶段中断控制、四个实际子进程os.Exit(77)不执行defer并由新writer/generation恢复、已删本地副本从原远端protobuf恢复、正常双HTTP执行/恢复与线上owner state/cursor不变；四组新配置/hash经双Rust读取及完整冻结门禁通过。|

控制ledger只保留一个active/最后complete操作与累计释放计数，不宣称完整永久控制审计；旧操作重试不会变成新删除，最后回执可幂等确认，其他陈旧计划按重新验证拒绝。根目录单一owner写入前提保持，不提供跨进程共享文件锁。外部store不会收到DELETE，只做固定base GET原字节核验；读回成功不等于云端WORM/跨故障域耐久。回收仅针对已证明旧本地cache，至少最近2份和所有当前/candidate始终保护。

启用时执行按现有max_read_bytes/max_entries全轮计费并限定journal1MiB、最多4096叶文件，不接受任意文件名或递归RemoveAll。只有严格生成的空工作目录/未提交临时journal可清理，未知非空内容保留。隔离中的字节并未释放，Go publisher无论当前开关是否关闭都先检查pending/corrupt ledger并暂停本链归档；在线reducer/journal/报价状态不因此修改。存储地址切换不能接续旧pending，也不跟随旧记录发送凭据；完成旧操作后可在新地址产生新plan并保留累计计数。

初始预检发现新配置测试缺strings导入及静态URL校验须用可寻址pointer，已修正后重跑；远端不可用控制改为保持原store身份而拒绝实际GET，避免“换store后没有eligible对象”误冒充故障拒绝。执行中完成数据释放但未耐久写complete时结果为COMPLETION_UNCONFIRMED/applied=false；mutation成功返回receipt_confirmed=true，状态GET只报告观察到的journal（不额外fsync），重试complete会再次fsync后确认。不把进程异常后的内存意图写成成功。

本轮只实现显式Go执行/恢复，不自动触发，也不打开Rust执行或生产准入；真实多区/链/对象锁、永久控制审计与其他设计差距仍待完成。

阶段115最终限定验收：27门禁全部exit0，117次Go race及231次Rust通过；三服务当前程序构建、Go vet/双Rust相关全目标严格Clippy、格式/契约/进度及同目录完整预览互通通过。源码/901固定输入及66份同轮独立配置/对象/计划向量前后相同，运行目录`506ae68c898f48a0aa5caad6671e9618`。九个阶段中断控制和四次子进程os.Exit(77)均实际执行，fresh writer/generation重新打开journal后只完成原集合、保留最新/candidate、不重复累计；normal Go+对象API真实HTTP执行2对象/幂等重试，并经source guard停止第二操作后HTTP resume累计4对象，线上owner state/cursor不变，被删本地副本仍从原远端protobuf恢复。未知文件/links/坏journal/远端失败/新候选/更严格保留/资源预算/错误store/取消均拒绝额外释放，pending阻断新归档；新store不能接续旧pending，完成后可切换且计数不丢。最后complete尚未耐久时COMPLETION_UNCONFIRMED/applied=false；已提交但未送达的回执可重新fsync确认，status GET仅观察原record。实际Kafka双进程owner-router与原v1冷恢复/两块追平均显式回归通过，正常preview仍只读。APFS非法UTF8创建控制仍按明确平台前提SKIP，单独crash-child测试只由父测试带参数启动并由四份真实exit证据验证，未将空环境skip写为运行通过。

该轮ctx在步骤和网络之间检查；native文件fsync须完成或报错后才释放发布gate，不声称可强制打断卡住的内核I/O，真实磁盘故障/多区演练仍待验收。控制journal只保留active/最后完整回执和累计计数，非永久审计；private-root单writer与固定HTTP对象合同不等于WORM/生产耐久。Rust回收执行、自动触发、全盘/远端配额/永久控制审计及全设计其他差距继续，不将该轮标作整个系统完成。

<a id="plan-2026-11-18-checkpoint-auto-reclaim"></a>

## Go周期checkpoint自动配额维护（2026-11-18，阶段116）

本轮补齐Go周期归档遇到本地公开cache配额超限时的显式自动维护。auto_reclaim默认false，只有reclaim_enabled=true、local_budget、正publish_interval_ms和原强认证均有效的Go online owner允许启用；两个Rust compiler识别相同配置并核对去凭据hash，Rust执行继续拒绝。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F116.1|配置/有界自动入口与空计划|WP08,WP24,WP25|LOCAL_SCOPE_VERIFIED|auto_reclaim独立opt-in；必须有实时authority guard，空eligible不写journal或工作目录；配置/hash由Go和两Rust独立读取。|
|F116.2|正常周期配额维护与pending优先|WP08,WP09,WP24|LOCAL_SCOPE_VERIFIED|只在publicOnly周期worker启用，先恢复冻结pending；只对准确容量超限选择一次回收并重新准入一次，不循环；恢复旧操作和选择新操作分属不同周期。|
|F116.3|来源/备份/关闭及手动语义|WP08,WP24|LOCAL_SCOPE_VERIFIED|正常worker保留当前/candidate/最近备份与owner state/cursor；远端不可读不释放，关闭取消网络；普通手动发布不隐式回收；计数/日志/状态提供可观测性。|
|F116.4|统一冻结回归与收尾差距审核|WP01,WP08,WP09|LOCAL_SCOPE_VERIFIED|保留阶段115真实崩溃/双HTTP/实际Kafka恢复及双Rust只读计划回归，增加真实周期配额及两周期边界证据；27门禁/127次Go/233次Rust及最新差距复核通过。|

单周期至多一次维护调用，pending未完成则暂停本链新归档，完成pending但仍超配额会等下一周期，避免同轮恢复后又新选删除。只有已知CHECKPOINT_LOCAL_BUDGET_EXCEEDED触发自动回收，未知目录/扫描/损坏错误仍直接拒绝。显式策略仍只覆盖本地public-v2逻辑预算，保留至少两份最新备份；不是全盘预算、远端删除、生产耐久或永久审计。按用户最新要求，本轮提交push后停止实现，另记录最新设计差距。

首次冻结运行`cf397b7f996c48a1b7924c9c461b7eea`在router API原有两项取消测试出现8秒内子进程未启动（34通过/2失败），该FAILED报告与日志保留。定向单项及固定测试并发2的完整原API测试随后通过；新runner仅显式限定该门禁的harness并发并写入report，不改变产品逻辑、测试断言/内部并发或deadline。重新执行整套冻结门禁，以最终报告为准。 第二次运行`b8322a5d5ade4466a0033749a314772c`在进度文档修改与解析重叠期间因缺失历史anchor观察中止，当前完整文件中的原anchor存在；固定全部文档后重新运行，不把这次中止计作通过。

阶段116最终限定验收：27门禁全部exit0，127次Go race与233次Rust通过，运行目录`16b33d8cacdb4fd699a2546f029521b8`。源码/901固定输入及70份同轮独立配置/对象/计划向量冻结前后相同；三服务当前构建、Go vet、双Rust相关全目标严格Clippy、契约/格式/进度检查通过。两份新增实际证据确认正常周期仅回收一个批次并发布当前状态、最新两份seed备份可解码且owner state/cursor不变、HTTP请求并发1；pending恢复完成后仍超配额的周期明确失败，下一周期才选择第二操作，累计2→4对象。远端失败/恢复、关闭取消、无eligible不写journal/不忙循环及手动不隐式清理通过；原九阶段中断/四次真实进程退出、正常双HTTP回收与实际Kafka独立owner/router追平/报价全部回归通过。API门禁以显式harness并发2运行，内部断言和deadline未修改；首轮失败和文档解析中止记录保留。未重新授予历史协议报告当前源码全系统资格，生产VERIFIED与整设计验收保持未完成。

<a id="design-gap-review-2026-11-18"></a>

## 最新代码与设计差距复核（2026-11-18，第116轮收尾后停止）

复核基线为XONE_SYSTEM_DESIGN.md v2.3及本轮提交代码；逐项对照§28原26工作包，并直接检查正常服务入口、能力准入、显式拒绝分支、状态结构及本轮冻结验收。原26工作包的完整ACCEPTED仍为0，表示整包退出条件未全部满足，不表示已经实现的模块为零。本轮验收只刷新checkpoint相关范围，既有协议原数学/原VM报告保留原冻结边界，不把历史通过重写成当前全系统重验。用户要求本轮结束后停止；此处记录差距，不启动117轮。

以下是仍未闭合的功能与验收项。代码缺项可以继续本地实现；生产来源/基础设施相关事项还需要相应真实环境和证据，两类不能相互替代。

|编号 / 优先级|设计 / 工作包|当前代码事实|仍未完成 / 完成判据|性质与源码证据|
|---|---|---|---|---|
|G01 高|§12.10/13/28.15/28.17；WP13,WP15,WP16,WP18|ExactIn净输出及ExactOut净输入仍为null，响应明确CHAIN_FEE_MODEL_NOT_IMPLEMENTED；route_activation_cost只有配置成本|各链实际Gas、原生币换算、L1数据费/L2差异与完整净收益排序/费用披露|代码缺项；[ExactIn API](dex-router/crates/quote-api/src/lib.rs)、[ExactOut API](dex-router/crates/quote-api/src/exact_out.rs)|
|G02 最高，开放可信交易前|§14/28.17/28.18；WP03,WP04,WP15,WP16,WP20|正常VERIFIED请求直接INDEPENDENT_SIMULATION_UNAVAILABLE；EVM本地后端标PROVIDED_LOCAL_STATE并固定Shanghai；SBF已有独立执行但同样不授予生产资格|可信锚点/完整版本化bank、链规则与费用、独立节点差分、重组撤权/预算回收和实际收付款闭环；达到这些条件后才可开放生产VERIFIED|代码和真实来源双缺项；[API准入](dex-router/crates/quote-api/src/lib.rs)、[EVM authority与hardfork](dex-router/crates/simulation/src/evm_fixture.rs)、[Solana后端](dex-router/crates/quote-api/src/solana_simulation.rs)|
|G03 高|§14/28.16；WP14,WP15,WP20|通用build_transaction仍返回EXECUTION_ADAPTER_UNSUPPORTED；prepare-evm只准入v2-complete；Solana准备限制单路线/单池，已有签名、一次广播和耐久回执|更多协议calldata/账户指令、通用多hop/拆单/共享节点有序DAG、授权/wrap/费用/ExactOut退款组合、受审计执行器及EVM钱包完整流程|代码缺项；[通用入口](dex-router/crates/quote-api/src/lib.rs)、[EVM准备](dex-router/crates/execution-preparation/src/evm_rpc.rs)、[Solana限制](dex-router/crates/quote-api/src/preparation.rs)、[已有钱包链路](dex-router/crates/quote-api/src/transaction_lifecycle.rs)|
|G04 高|§12.2/12.11/12.14/28.13；WP05,WP10,WP11,WP20|已有许多独立profile及固定原源码/原程序差分；V4/Infinity部分动态/native快照、Curve多个子模型、Managed与Solana高级子集已有局部实现|Curve其余Plain/Meta underlying/Lending/外部rate、Balancer剩余BPT/rate-cache/V3组合；V4/Infinity hook回调/自定义会计及manager/Vault完整结算；Bin动态/hook组合；LM新代际/未知模块；Solana订单/奖励领取/扩展交叉和新增版本逐项独立建模/执行|版本相关代码缺项；[注册profile](dex-router/crates/domain/src/protocols.rs)、[能力目录](dex-router/crates/protocol-math/src/capabilities.rs)、上方协议覆盖表；不得把协议总称当作完整支持|
|G05 高|§11/12.13/28.12；WP10,WP11,WP13,WP14,WP20|通用域对非standard Token拒绝；部分命名Solana profile有Token2022支持，不能泛化到全部扩展|真实收款转账税、rebase、暂停/黑名单/代理升级与Token2022多扩展交叉能力分级，报价/执行/后余额一致；不支持者继续明确拒绝|代码及真实执行验收；[Token准入](dex-router/crates/domain/src/lib.rs)、[Solana账户准入](chain-parser-rust/crates/solana-ingest/src/rpc.rs)|
|G06 高|§12/14/28.13/28.16；WP11,WP13,WP14,WP16|ExactOut已按多个profile独立求输入；大量模型限定SINGLE_POOL，V2/V3限定简单路径/同factory，存在显式家族/周期拒绝|未准入profile及跨家族/复杂共享池/多hop拆单ExactOut，执行最大输入/剩余退款/费用全链一致与独立执行证明|代码缺项；[ExactOut路由边界](dex-router/crates/routing/src/exact_out.rs)、[ExactOut执行](dex-router/crates/execution-plan/src/exact_out.rs)|
|G07 中|§13/28.14/28.15/28.24；WP12,WP13,WP22|邻接桶已有增量/COW；候选仍为VecDeque有界搜索，分配已有有界有序网格及顺序共享池重算；网格穷举标记明确限定范围|beam/多标签、通用共享DAG联合求解、适用条件下的可证明上界/剪枝、完整小图oracle与网格外收益gap/尾部测量|代码和规模化验证；[路由](dex-router/crates/routing/src/lib.rs)、[优化器](dex-router/crates/optimizer/src/search.rs)|
|G08 中，规模化前|§18/28.11；WP09,WP12,WP17|QuoteView持有Vec<Pool>，更新会复制完整pool集合；graph/protocol handle已有部分共享，缓存回收也已实现|Pool状态及顶层索引完整增量COW、活跃慢请求固定旧版本的总内存预算/取消与压力恢复、大型图多副本容量|代码和压测；[QuoteView及apply](dex-router/crates/state-replica/src/lib.rs)、[graph更新](dex-router/crates/routing/src/lib.rs)|
|G09 中|§10.5/17.7/18/23.9/28.10/28.26；WP08,WP09,WP24|Go/Rust公开不可变checkpoint、远端HTTP上传/回读/目录、有限Kafka恢复、配额/只读plan及Go显式/自动回收已完成限定实现；Rust开启执行被CHECKPOINT_RECLAIM_OWNER拒绝|Rust回收/中断恢复/自动维护；canonical CheckpointAvailable及离线peer/catalog发现；多对象分片、全盘/WAL/远端容量、跨writer/reorg/scope矩阵、永久控制审计；真实对象锁/独立故障域与停机1h/1d/30d/保留过期恢复|代码和外部验收；[Rust配置拒绝](chain-parser-rust/crates/config/src/lib.rs)、[公共journal](common/protoc/proto/dex/v1/state.proto)、[当前远端恢复](dex-router/crates/quote-api/src/remote_checkpoint.rs)、[本轮worker](chain-parser-go/internal/service/checkpoint_worker.go)|
|G10 最高|§7/8/18/28.5–28.9/28.22/28.26；WP02,WP03,WP04,WP05,WP07,WP18,WP20,WP24|正常provider预算/WS补扫、并行staging、有序提交、工厂动态目录、raw/DLQ与恢复已接；已有公开历史receipt/body/account proof和实际VM样本；Solana tonic/WAL/屏障也已接|目标链/协议全目录的真实部署版本/升级审核与历史多块重组回放；持续公开provider能力/独立性矩阵；可信头/最终性资格；真实同源Yellowstone连续流、fork/补洞、版本化bank与动态scope完整联验及长期资源测量|主要是完整资格和真实来源，部分版本代码仍需补；[RPC锚定并非共识checkpoint](chain-parser-go/internal/chain/evm/chain_receipts_body.go)、[实际账户证明](chain-parser-go/internal/chain/evm/chain_receipts_state_proof.go)、[normal stream](chain-parser-rust/crates/solana-ingest/src/normal_stream.rs)、[已有采集DLQ](chain-parser-go/internal/ingest/dlq.go)|
|G11 上线前|§7.5–7.8/28.8；WP02,WP06,WP07,WP25|事务块/幂等/只读consumer及重组消息已实现；配置只接受显式PLAINTEXT；公共事实目前以swap/整块及reverted_blocks为主|Kafka TLS/SASL/ACL和secret运维；生产ISR/事务未知结果/保留损坏/consumer rebalance/topic重建迁移专项；剩余事实族及原始证据/跨版本schema破坏检测|代码和真实集群验收；[Go配置](chain-parser-go/internal/config/config.go)、[Rust配置](dex-router/crates/config/src/lib.rs)、[公共事实](common/protoc/proto/dex/v1/state.proto)|
|G12 开放API前|§15/28.18；WP16,WP17|已用共享Bearer、全局rate与并发信号量，single-flight/deadline和部分仿真内存预算已存在|多租户身份/独立额度与审计、任意calldata仿真和内部流量独立预算、完整业务成功率/超时取消/热点及容量SLO|代码和压测；[认证/限流/API状态](dex-router/crates/quote-api/src/lib.rs)|
|G13 低，不阻塞报价|§16/28.21/28.23；WP19,WP21|正常analytics持续只读worker、持久cursor/失败停止、K线局部回撤重算已实现；输出仍为本地状态文件|分析专属持久DLQ/raw重放、更多协议事实/插件隔离、ClickHouse事实与revision/dataset表、shadow/发布指针、FINAL/merge前查询、分析API/查询额度/watermark、迟到及decoder升级回补|代码缺项；[持续worker](chain-parser-go/internal/analysis/worker.go)、[当前输出](chain-parser-go/internal/analysis/job.go)、[K线](chain-parser-go/internal/analysis/candles.go)|
|G14 低，设计高级阶段|§13/28.24；WP22|当前优化来自内部pool/path/grid，尚无外部来源竞争链路|RFQ/聚合器、多个求解器提案、来源有效期/调用者/同块约束、统一仿真净成本评分和含失败样本收益分布；具体候选链/DEX是否上线须另定矩阵|代码缺项；[当前求解器](dex-router/crates/optimizer/src/search.rs)、[设计WP22](XONE_SYSTEM_DESIGN.md#2824-wp22高级优化与外部来源竞争)|
|G15 上线前，与主链路同步|§20/28.3/28.4/28.27/28.28；WP01,WP02,WP25,WP26|严格文件配置/跨语言hash、构建契约、本地门禁、JSON有界异步日志/drop指标与部分PG控制审计已有实现|镜像/SBOM/许可证/secret扫描及真实CI执行；完整配置/schema演进/坏输入矩阵、secret版本；SDK日志映射、轮转/spool/采样聚合/OTel/集中检索、所有关键控制事件耐久审计及日志故障/性能验收；配置中心仅为以后bundle物化扩展口|代码和部署验收；[本地CI入口](integration/Makefile)、[Go日志](chain-parser-go/internal/observability/logger.go)、[Rust日志](dex-router/crates/observability/src/lib.rs)、[日志runbook](integration/runbooks/log-investigation.md)|
|G16 上线前|§18/23/28.19/28.20/28.25；WP17,WP18,WP23,WP24|已有独立进程/独立KV与loopback PG/Kafka接管恢复、多链指纹和限定链规则路径；这些验证不能代表真实多区部署|第二/第三条真实EVM链端到端数据/图/执行ready与独立费用/SLO；跨区调度/PDB/灰度、PG切换/Kafka不足ISR、磁盘/节点/AZ丢失、区域RPO/RTO、生产shadow/adapter回退/逐tenant放量|主要外部环境与上线验收，部署/控制实现仍需补；[现有开发compose](integration/compose/compose.yaml)、[设计退出标准](XONE_SYSTEM_DESIGN.md)|

26工作包覆盖关系：WP01→G15；WP02→G10/G11/G15；WP03→G02/G10；WP04→G02/G10；WP05→G04/G10；WP06→G11；WP07→G10/G11/G16；WP08→G09；WP09→G08/G09；WP10→G04/G05；WP11→G04/G05/G06；WP12→G07/G08；WP13→G01/G05/G06/G07；WP14→G03/G05/G06；WP15→G01/G02/G03；WP16→G01/G02/G06/G12；WP17→G08/G12/G16；WP18→G01/G10/G16；WP19→G13；WP20→G02/G03/G04/G05/G10；WP21→G13；WP22→G07/G14；WP23→G16；WP24→G09/G10/G16；WP25→G11/G15；WP26→G15。上方原工作包和R01–R21列表保留更细分版本记录，本表作为本次复核的最新总览。

本次同时修正旧列表中已过时的“对象上传下载尚未实现”“持续分析调度尚未实现”“WP18完全未开始”和固定21/61、38/61统计措辞。原v2 profile不能自动继承新增v2-complete能力；也不能把Solana已有钱包链路说成未开发。设计§4明确候选链/DEX矩阵是建议目标，未确定的未来版本不据此承诺已纳入强制上线范围。

真实外部依赖包括可持续访问且有清晰权威/版本证据的链与provider/Yellowstone数据、目标部署二进制及可信bank，以及具备TLS/ACL/ISR/独立存储和多区条件的基础设施与演练权限。本轮按用户要求停止，未将这些事项标记为已确认不可解决的阻塞；G01/G03/G07/G09/G12/G13等仍有明确本地代码工作，不能把全部未完成项归因为外部阻塞。

<a id="remaining-execution-order-2026-11-18"></a>

## 剩余功能优先级、实现建议与退出标准（2026-11-18，恢复持续开发）

用户已要求恢复持续开发，并将剩余工作按以下顺序逐项完成，每轮验证、记录、commit和push。第116轮暂停是历史状态，本节为当前执行规则。数据解析与精确数学优先，先收敛一个明确的chain/protocol/version端到端闭环，再扩版本；原26工作包和G01–G16范围不缩减。单项仅在列明退出条件和对应当前源码证据均满足后关闭；局部实现记LOCAL_SCOPE_VERIFIED，整包ACCEPTED仍以设计§28为准。

|顺序|差距 / 工作包|实现建议|退出标准 / 后续边界|当前状态|
|---|---|---|---|---|
|P0-1|G10；WP02,WP03,WP04,WP05,WP07,WP18,WP20,WP24|首个闭环选现有v2-complete；复核真实部署/升级、header/body/receipt/state证明与普通服务资格、完整raw回放；Solana真实同源流和bank单独闭合|当前可信锚点、完整性/重组撤权、错误来源拒绝及恢复均有可重复证据；RPC一致性不升级为共识资格。阶段251验证两个真实 Ethereum 连续签名窗口。阶段254重放两个真实 Solana slot 的来源快照。阶段255重放七条 EVM 主网启动快照。阶段256在 BNB Smart Chain 和 Polygon 重建真实连续 journal。独立故障域、Solana 同源 bank 及其余来源仍待完成。外部来源不足逐项记录，继续其余可本地实现工作|IN_PROGRESS|
|P0-2|G04/G05基础；WP05,WP10,WP11,WP20|补选定版本的整数边界、连续交换及完整后状态，原合约/原程序独立差分；先补已接profile缺口|解析→数学→独立执行收付款/后状态一致；未支持Token/布局明确拒绝，限定版本不自动扩展|PARTIAL_LOCAL|
|P0-3|G01；WP13,WP15,WP16,WP18|建立链费上下文及完整性标签，分开Gas/原生币换算/L1数据费/L2差异，接入净收益评分|net输入/输出与真实费用有明确来源/时效/边界，金额守恒；不能用route_activation_cost或调用者猜测替代真实费用；阶段123–145补共享EVM原生费/源余额守恒/最大资金、无可信费用排序回退和Solana消息费用来源/资金分类/提交前上下文、耐久凭证、已执行原生同单位经济净额、失败网络费/回滚核验及成功/失败费用前端闭环及Legacy实际原生测量/净扣款及OP Regolith历史L1数据费内核/同body费用输入来源重放、现代packed版本来源及完整零value费用账户/最大资金/退款内核与签名wire绑定的历史OP严格执行；阶段140完成签名BOUND完整Router/实际Token/后状态及费用闭环，阶段141完成受限worker签名OP协议，阶段142补独立源费用核验组件，阶段143接限定历史正常build/签名simulate，阶段144补KZG/BLS后端与Isthmus输入/Gas规则，阶段145接限定OP Mainnet历史Jovian用户VM/operator/四预编译上限及32笔正常HTTP，阶段146完成9452原Karst预编译组件/Osaka费用与限制差分，阶段147补CLZ/Gas cap与468完整原引擎签名组件差分，阶段148贯通有界Karst本地正常API/实际worker/父费用源审计、四完整原Router与36正常OP HTTP；阶段149补服务端配置预言机签名授权、源块/币种/历史时效绑定和父源审计后的单笔原子单位换算/费后金额；阶段150完成逐完整候选费用评分组件及有符号目标/完整覆盖检查/统一毛额回退，阶段151接通显式启用的单路线V2正常报价、逐候选实际worker/父费用核验/授权换算和固定计划deadline；阶段152补批准Ethereum Chainlink USD双feed固定部署/原ABI/时效和签名来源绑定、全宽约分/换算与有界采集/待签署CLI；阶段153补选中候选费用的正常报价同源披露；阶段154补进程内授权价格book签名版本/原子刷新、单报价价格快照/缓存及旧报价执行绑定；阶段155–156补离线初始化与显式启用的正常Router重启耐久高水位；默认模式仍无跨重启保证。其他执行/成本模型、可信bank、持续live价格刷新/授权发布及其他链来源、完整全候选净评分及报价总费仍未完成|PARTIAL_LOCAL|
|P0-4|G02；WP03,WP04,WP15,WP16,WP20|绑定可信bank、实际链规则、原始交易及费用，独立后余额核验；保留取消/超时/重组撤权和资源预算|按具体链/版本满足独立执行和来源资格后才开放VERIFIED；提供本地状态的成功仍只授予本地资格|PARTIAL_LOCAL|
|P1-1|G03限定交易；WP14,WP15,WP20|先补单池/简单路径的calldata、授权、wrap、recipient、期限/退款、签名发送和耐久回执|报价→准备→独立仿真→用户签名→一次发送→回执收付款一致；先复用已有Solana链路，EVM独立补齐|PARTIAL_LOCAL|
|P1-2|G06现有模型；WP11,WP13,WP14,WP16|已准入ExactOut先贯通最大输入、实际消耗、退款/费用和独立执行|数学输入目标与执行一致，拒绝不能保证的路径；复杂跨家族/共享拆单留P3-1|PARTIAL_LOCAL|
|P1-3并行|G11/G15；WP01,WP02,WP06,WP07,WP25,WP26|随主线建设Kafka TLS/SASL/ACL、secret版本、schema兼容、镜像/SBOM/扫描及耐久控制审计|错误配置/凭据在写入前拒绝，真实CI和安全/审计故障检查通过；生产上线硬门槛|PARTIAL_LOCAL|
|P1-4|G12；WP16,WP17|租户身份/独立额度、内部与任意仿真隔离、热点/超时/取消和业务成功率|租户与工作负载互不耗尽额度，资源有界且取消可回收，开放API前完成|PARTIAL_LOCAL|
|P2-1|G09；WP08,WP09,WP24|Rust回收/中断恢复/自动维护、离线目录与peer发现、多对象分片、全盘/WAL/远端配额|保留/长停机1h/1d/30d与跨writer/reorg/scope恢复、独立存储故障/永久审计均有证据；Go限定自动回收已在116完成|PARTIAL_LOCAL|
|P2-2|G08；WP09,WP12,WP17|Pool及顶层索引增量COW、旧版本活跃慢请求总内存预算与取消|大池/多副本/慢句柄/重组压力下容量受限并能恢复，扩大scope前完成|PARTIAL_LOCAL|
|P2-3上线门槛|G16及G09/G11生产验收；WP06,WP08,WP17,WP18,WP23,WP24|逐链独立费用/ready/SLO，PG/Kafka故障、磁盘/节点/AZ丢失、区域恢复、灰度/adapter回退与逐tenant放量|第二/第三真实链端到端及真实独立故障域RPO/RTO达标；单机loopback不充当多区验收|PARTIAL_LOCAL|
|P3-1|G03/G04/G05/G06扩展；WP05,WP10,WP11,WP13,WP14,WP20|按明确上线矩阵逐版本扩Curve/Balancer/V4/Infinity/LM/Solana组合、特殊Token、多hop/共享节点/拆单DAG|每个版本分别有完整状态/解析/原数学/独立执行/恢复证据；未知hook和扩展不隐式准入|PARTIAL_LOCAL|
|P3-2|G07；WP12,WP13,WP22|beam/多标签、共享DAG联合求解、适用条件上界、小图oracle和网格外gap|正确性/费用/执行先稳定；候选及网格外最优性不夸大，完整收益和尾部报告|PARTIAL_LOCAL|
|P4-1|G13；WP19,WP21|分析专属DLQ/raw重放、ClickHouse事实/revision/dataset、shadow发布、查询/水位和迟到更正|分析故障隔离于canonical/报价，重复/重组/升级回补和发布查询语义通过|PARTIAL_LOCAL|
|P4-2|G14；WP22|RFQ/聚合器/多求解器、有效期和调用者约束、统一独立仿真净成本评分|数据/执行/成本口径稳定后接入，含失败和输样本收益分布，外部来源不绕过资格|NOT_STARTED|

逐轮执行规则：先选择最前的可推进缺项，记录设计条款、代码缺口和本轮有限退出条件；实现正常入口并加入能击中真实风险的验证，冻结源码与输入，更新同一进度文档及必要技术说明；全部必需门禁通过后commit/push并检查远端同步，随后进入下一项。外部证据不足保留具体URL/能力/错误/日期和可复验输入，不把未尝试事项标作阻塞；一个事项外部受限时继续其他本地可推进项。禁止以测试条数、一个本地样本或仅有配置开关判定全功能完成。

首个实现切片：检查v2-complete普通native来源的状态承诺和执行准备的信任边界，优先补足可验证的来源/状态绑定；已有native reader和MPT核验复用，避免重复建设。数学/费用/VERIFIED各有独立退出标准，不能因源码重编译或本地VM成功就跳过生产来源资格。

推送目标事实：当前仅配置`origin = git@github.com:mysear/xone.git`，无GitLab远端。恢复后每轮继续推送既有origin，明确不称为GitLab推送；GitLab目标需提供准确仓库地址后配置。该远端差异不阻断本地实现/验证，也不改变功能完成判据。

<a id="plan-2026-11-18-v2-native-state-proof"></a>

## v2-complete native Pair/Factory状态承诺（2026-11-18，阶段117，P0-1）

按执行顺序先补v2-complete的实际来源绑定缺口。现有native header/body/receipt和独立EIP-1186核验已存在，但普通ReadState调用的eth_call值不能仅靠receipt root认证；本轮将固定原Pair/Factory布局绑定到同一native execution-state root。仅在原known-evm-native-commitments-v2模式启用，legacy路径不获得额外资格。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F117.1|正常ReadState证明接线与代码身份|WP03,WP04,WP05|LOCAL_SCOPE_VERIFIED|复用原共享native session和ContractAccountProof、请求原blockHash/requireCanonical；SHA256核对固定原runtime且Keccak与MPT account codeHash相同。|
|F117.2|完整Pair slot与Factory双向映射|WP05,WP10|LOCAL_SCOPE_VERIFIED|按原源码slot5/6/7身份、slot8双112bit reserves+uint32 time、slot9/a累计价与slotc锁，以及Factory slot2嵌套mapping两方向逐项核对，读取sealed logical StorageValue，不信任可编辑输出map。|
|F117.3|错误/取消/资源边界|WP04,WP05|LOCAL_SCOPE_VERIFIED|getter和已承诺slot矛盾、错误runtime/账户/节点、缺proof拒绝发布；同chain/hash/number/time绑定、超预算和取消失败后session不可重用，不产生额外proof请求。|
|F117.4|独立原bank/正常路径与冻结回归|WP01,WP03,WP05,WP10|LOCAL_SCOPE_VERIFIED|使用服务本地固定原合约runtime和captured storage构造真实MPT/自哈希合成header，直接调用正常Adapter.ReadState并导出范围证据；固定原bank/native/normal service/契约/vet/构建回归通过。|

本轮只证明Pair/Factory承诺与getter一致。原Token balanceOf的代码/layout/执行证明尚未接入，header仍为RPC锚定承诺而非独立共识checkpoint；测试自哈希header和MPT明确为合成构造，原runtime/storage来源为既有固定原合约bank，不能称公开历史bank。完整bank/可信执行来源/生产VERIFIED及其余P0退出标准仍未完成，继续推进。

阶段117首轮冻结`e89a20b360fb49fcbe43e1461861ff27`在Go全EVM包默认600秒整包alarm中止，未出现断言失败，新增Pair/Factory成功及16种错误/预算/取消子场景均实际通过。原包还包含大量固定原合约bank逐项回归，后续runner显式给整包20分钟收集时限；单次RPC与取消/预算测试deadline、断言保持原值。该FAILED报告保留，重跑全部门禁后才确认本轮。

阶段117最终限定验收：8门禁全部exit0，488次Go race测试通过（EVM全包185、普通native服务6、domain/config/reducer297），17项进度检查、格式/契约/vet及当前Go二进制构建通过；运行目录`c9b96344b2a14534a3452931d5ad467c`。源码和901份固定输入冻结前后相同；正常Adapter.ReadState导出实际两份证明请求、Pool bank与返回block hash一致的证据，16类getter/承诺slot/代码/账户/缺proof/预算/取消控制均实际通过。首轮默认10分钟整包超时记录保留，显式20分钟整包门禁未修改单次RPC或取消测试deadline。Token原balanceOf状态证明/可信共识头/生产VERIFIED仍未完成，下一轮继续P0-1的Token只读余额证明。

<a id="plan-2026-11-18-v2-native-balance-proof"></a>

## v2-complete native Token余额证明（2026-11-18，阶段118，P0-1）

紧接117继续证明RawBalance0/1。不能把通用ERC20余额假定放在slot1，也不能用Pair/receipt证明替代Token代码/状态。先读取Token account proof和同block原runtime并核对Keccak，再在全新内存overlay执行固定零caller/origin/value、balanceOf(pool)的确定性只读子集；每次未证明SLOAD在执行前停止，原共享native session获取该逻辑slot的MPT证明后从头重放，已证明空root可直接导出零。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F118.1|Token代码/存储证明与正常路径|WP03,WP04,WP05,WP10|LOCAL_SCOPE_VERIFIED|复用原root/块/来源/总预算；代码Keccak绑定实际account，使用sealed StorageValue而非输出投影；两Token原返回余额与执行结果相同才发布。|
|F118.2|确定性只读内核和有限重放|WP04,WP05,WP10|LOCAL_SCOPE_VERIFIED|固定Berlin稳定指令子集，拒绝外部CALL/DELEGATECALL、外部账户/区块/费用/GAS及写入；每pass100000Gas、最多8已证明slot/9pass、code24576byte及36byte输入/32byte输出；逐opcode检查ctx。|
|F118.3|反例/空root与资源控制|WP03,WP04,WP05|LOCAL_SCOPE_VERIFIED|未证明零不能改变分支，证明后正确选择分支；getter/已承诺余额/代码/缺proof/不支持view拒绝；9slot超限停止，loop/巨大内存/写/外部调用/短返回/revert及取消有界失败；已证明空root不额外请求slot。|
|F118.4|依赖/原bank/native回归冻结|WP01,WP03,WP05,WP10|LOCAL_SCOPE_VERIFIED|沿用固定go-ethereum v1.16.7，只补其VM/state三个传递依赖与现有uint256直接使用，不升级原模块；定向原v2全部bank/native/合约storage/普通服务、模块校验/vet/构建与源码/输入冻结通过。|

该内核不是完整链上交易模拟器。Berlin稳定子集仅用于确定性view结果，100kGas是资源上限而非真实费用；不支持PUSH0/代理/外部调用/环境依赖的view明确失败。只证明固定零caller的balanceOf返回值，不证明税费/rebase/Token转账行为、未来交易、独立共识头或生产VERIFIED。原v2-complete完整swap数学状态与实际balance输入的绑定得到增强，P0-1真实部署/可信head/长期恢复和P0-4生产资格仍继续。

阶段118首轮冻结`edb70847dd8248dc8c1e79330a18576f`已通过v2/native/contract-storage定向race及正常native服务，但原go mod verify将已有本地replace的xone/common/protoc/golang v0.0.0当作需要ziphash的模块而中止；FAILED报告保留。新增模块缓存门禁只在临时modfile移除已验证的唯一local protobuf要求/replace，比较移除前后全部外部模块Path/Version/Sum/Main身份一致后运行真正go mod verify；本地protobuf仍由冻结源码和契约门禁覆盖。预检241个外部模块缓存校验通过，不修改产品go.mod/go.sum，也不跳过外部依赖校验；全部门禁重新冻结运行。

阶段118最终限定验收：8门禁全部exit0，77次Go race（全部v2/native/contract-storage相关71、普通native服务6），241个外部模块缓存校验、17项进度检查、格式/契约/vet和当前Go二进制构建通过；运行目录`aaa23bc8ff8643ee8e3223012bbc2671`。源码和901份固定输入冻结前后相同；正常原bank六次proof并独立重放两Token余额，已证明空root只需五次proof；未证明零分支、getter/承诺余额/代码/缺proof、未知环境view/第九槽以及13类指令/loop/内存/返回/写反例通过。测试仍使用明确合成header与原runtime/storage，不授予真实链共识/Token转账行为资格；原117全EVM185项报告保留原冻结，118只声明本轮相关路径。模块替换首轮FAILED记录保留；另以当前同字节只读内核对256个opcode边界做隔离预检，未发现非私有panic，不替代正式门禁。下一轮继续P0-1，修正EVM confirmation_tag任意历史高度/earliest/pending/空值仍能通过配置校验的实际缺口（原二进制check-config已确认）。

<a id="plan-2026-11-18-evm-confirmation-policy"></a>

## EVM当前头确认策略与披露（2026-11-18，阶段119，P0-1）

原117当前二进制实际check-config对latest/safe/finalized以及0x1/earliest/pending/LATEST/空值均exit0，说明当前头策略可以被误配成历史游标/未确认pending而绕过HEAD_FIRST语义。不是要自研共识节点（设计§3明确复用节点）；本轮限制配置到明确定义的当前provider标签，来源资格/实际链规则仍独立验证。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F119.1|Go/双Rust当前EVM标签语义|WP04,WP24,WP25|LOCAL_SCOPE_VERIFIED|EVM仅latest/safe/finalized；数字高度、earliest、pending、大小写/空白/空值/其他链commitment/未知拒绝CONFIG_CHAIN_CONFIRMATION，不回显输入；Solana已有每profile规则保持。|
|F119.2|同轮编译器配置/hash与CLI|WP01,WP24,WP25|LOCAL_SCOPE_VERIFIED|Go生成3有效/12无效完整配置，双Rust独立Load并核对全部公共/经济hash与拒绝码；当前Go CLI逐一check-config，输出与预期一致且不创建state。|
|F119.3|正常服务资源前拒绝/无降级|WP04,WP05,WP24|LOCAL_SCOPE_VERIFIED|普通New对历史/pending/空/未知策略在打开Pebble前拒绝；合法safe但provider不支持时Start拒绝且无ready、不请求latest/finalized作为替代。|
|F119.4|来源元数据/统一冻结回归|WP01,WP04,WP25|LOCAL_SCOPE_VERIFIED|普通HTTP source-evidence披露confirmation_tag与CONFIGURED_PROVIDER_TAG，保持consensus_verified=false；Go配置/服务、双Rust全配置及严格Clippy、模块/构建/CLI/契约/源码输入冻结通过。|

配置允许一个标签不代表所有链/节点支持该标签，更不把多RPC一致称为finalized；provider能力不足明确失败，不能静默退回latest。保留默认latest的indicative语义，只有实际链规则、来源与独立执行全部合格时才可授予其他资格。生产VERIFIED/真实源矩阵/独立故障域仍未完成。

阶段119首次冻结`73f37462c4bf4c659c27cb9b9200e4f9`已通过Go完整配置/普通服务和router完整配置/同轮互通，但parser独立配置回归发现旧用例由Solana切换EVM时保留confirmed，导致正确拒绝。现对13个测试文件的15处EVM转换显式设置latest，生产fixture/协议断言不修改，也不放宽确认策略；该FAILED记录保留，重新冻结全部门禁。预检还修正Rust测试对非Debug ConfigSnapshot使用unwrap_err及构造rpc模式测试时遗漏RPC URL的测试设置，不影响实现语义。

阶段119最终限定验收：19门禁全部exit0，81次Go race与97次Rust通过；两独立Rust完整配置测试/严格Clippy、同轮3有效/12无效配置的公共/经济hash与拒绝码互通、16份导出文件前后不变、实际Go CLI15次check-config均符合预期且未创建state目录；正常构造前拒绝、safe不支持无fallback/无ready及HTTP明确配置策略通过。三服务当前构建、Go vet/模块/格式/契约/17项进度检查通过，源码及901固定输入冻结不变，运行目录`abacdaa483194a4d94eac27915902410`。首次旧测试继承Solana confirmed的失败记录保留；13文件15处测试转换改为明确EVM latest，协议反例断言和原固定fixture未变。并已只读观测公开节点当前finalized/部署runtime与MPT能力：原Pair/Factory SHA与两来源所返字节一致；一个来源对目标proof报告窗口不足，另一个返回proof。该观测只用于继续实际正常服务闭环，不等于独立故障域或共识最终性。下一轮继续P0-1的真实当前V2状态/普通owner与router闭环资格，不阻断其他可本地推进工作。

<a id="plan-2026-11-18-current-v2-public-source"></a>

## 真实当前V2 normal owner/router与来源拒绝（2026-11-18，阶段120，P0-1）

对照设计§7与§28.5/28.6/28.11/28.26：测试不能只依赖实时节点成功，也必须正常路径/故障/重放恢复/可观测性。第117–119轮已补来源状态证明、Token view和当前头策略，本轮用实际主网原DAI/WETH Pair及Factory验证正常Go owner+Rust router并保留原始响应，继续关闭history也能先报价；不新增服务架构、不改公共设计或广播交易。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F120.1|真实当前来源/普通进程与整数oracle|WP03,WP04,WP05,WP10,WP24|LOCAL_SCOPE_VERIFIED|只读固定两个公开provider、known genesis/固定原Pair与Factory runtime；当前finalized provider标签、普通Go native完整读取/原raw耐久归档，独立Rust通过HTTP开发日志恢复；双方向ExactIn/Out与Python整数算术核对，历史gap明确且0fact/0attempt不影响ready。|
|F120.2|可认证原始响应与封闭离线重放|WP03,WP04,WP05|LOCAL_SCOPE_VERIFIED|验证Go envelope原Payload hash、Ref/owner ID/data hash/长度、provider ID及只读方法；原始byte和JSON顺序变体全部保留，语义冲突拒绝；每provider独立loopback服务，未知请求/公网写方法拒绝，源码/固定输入/本轮raw文件冻结。|
|F120.3|双完整副本/冷恢复与owner升代|WP09,WP24|LOCAL_SCOPE_VERIFIED|两独立router无本地KV各持同完整scope，停一副本后健康副本继续报价、冷重启新副本恢复；owner同KV重新HEAD_FIRST验证当前源后writer/generation/seq递增且canonical epoch不误增。|
|F120.4|来源故障拒绝与真实错误语义|WP04,WP05,WP24,WP26|LOCAL_SCOPE_VERIFIED|改Pair getter/Token getter/Pair account proof、proof不可用及header篡改五控制实际触发；普通owner启动拒绝且router返回503/CHAIN_NOT_READY。native session独立rejected标记使失败终止不冒充额度耗尽，refill仍不可复用；真实预算耗尽仍显示预算错误。|
|F120.5|统一冻结回归与边界披露|WP01,WP03,WP04,WP09|LOCAL_SCOPE_VERIFIED|Go相关race/原native storage预算/普通服务、Python守卫、router API/Clippy、模块/vet/当前构建、进度/契约与actual live+offline多进程验收通过；报告绑定二进制/hash/首scope与首quote时间。|

数据和资格范围：实际公开原始响应→已有normal decoder和MPT/view→普通HTTP开发日志→normal INDICATIVE报价。该条链路不证明provider独立故障域、独立共识或原交易执行收款；不把Python整数oracle当原合约余额执行；不声称Kafka/PG生产接管、多区、1h/1d/30d演练、全市场覆盖、链费或VERIFIED已完成。实时源失败保留真实原因，离线重放的成功不能冒充当前实网成功；其余可本地实现项继续按优先级。

阶段120首次冻结`998504bf469a45a883f901e5b778cd0e`通过Go证明与普通native服务等门禁，但router API既有http_drop取消测试在8秒子进程启动前等待失败（35通过/1失败）。该测试会在启动前计算整个大型Rust测试镜像SHA；runner并发2仍会产生本机启动预算争用。保留FAILED记录，并将该门禁harness显式串行及写入report；产品工作进程/取消/回收逻辑和内部断言、单次deadline不修改。单独串行完整API回归确认后重新冻结全部门禁。

阶段120最终限定验收：13门禁全部exit0，81次Go race及36次Rust API、6项Python证据守卫、17项进度检查、严格Clippy/vet/模块/构建通过；运行目录`9519d88ec46d43af87d1a8f8990e37b3`。实际公开Ethereum provider当前finalized块26438995（`0xf70886a3ca08bbf7297b0df0e0b8418420bd6ee39fb0db9b81b52cd638bbd1c9`）由normal owner验证并发布，history disabled且gap/0fact/0attempt明确；首qualified snapshot 23868ms、首INDICATIVE quote 23873ms（本轮单次墙钟观测，非SLO）。32份原raw共9250774bytes的封装/内容/owner/provider身份校验、原字节保留及双方向ExactIn/Out四独立整数oracle通过。离线双router均无KV，停一后健康副本继续、冷重建恢复；owner同KV重新HEAD_FIRST后writer/generation/seq从1升2，canonical epoch仍1。五来源变更均实际命中、owner退出且router503；原始错误不含虚构预算码，真实预算/取消和不可复用标志回归通过。源码/901固定输入前后一致，首轮router取消startup超时记录保留，正式API门禁显式串行，不改deadline/断言。仅HTTP开发日志、一个固定真实部署的当前state与INDICATIVE数学/恢复验收；共识/独立VM交易后余额/链费/全市场/PG-Kafka/长停机/多区仍缺。下一轮将已验证public raw变为可离线复现的固定夹具，干净环境不依赖本机临时目录；随后按P0数学/费用/独立执行推进。

<a id="plan-2026-11-18-current-v2-fixed-fixture"></a>

## 真实V2固定bank独立离线夹具（2026-11-18，阶段121，P0-1）

设计§28.5要求离线重放不依赖实时RPC，§28.1要求固定块且各服务测试独立。本轮固定阶段120已验证的实际Ethereum块26438995、32份原始raw及准确runtime/proof/receipt/header；保留整个原Envelope字节而不是重写响应/期望。仓库既有.gitignore明确原始数据不进Git，因此不绕过规则：跟踪不可变source-metadata和恢复/验证工具，原gzip/测试数据在本地dataset目录，另拷到Go自己的fixtures。新机器需提供精确bundle或原capture，用显式命令离线恢复；缺数据保持明确失败，不宣称仅git clone即含全部测试数据。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F121.1|原始响应固定与恢复工具|WP01,WP03|LOCAL_SCOPE_VERIFIED|2.39MB压缩包绑定12.34MB解码长度/hash、32原始成员/9.25MB raw全清单、实际block/预期完整pool及acquisition原report/source/binary摘要；restore只接受原pin bundle/capture，无网络，不改变源metadata或静默刷新数据。|
|F121.2|有界解包/成员hash与契约|WP03,WP04|LOCAL_SCOPE_VERIFIED|只读固定gzip，不extract tar；最大32MB解码、1024成员/128MB原raw，重复key/重复成员/逃逸/链接/冲突输出拒绝；全部验证后才发布成员。源metadata→integration fixture→Go本地副本逐byte契约，全部4新输入进入冻结。|
|F121.3|Go独立真实bank正常路径|WP03,WP04,WP05|LOCAL_SCOPE_VERIFIED|Go仅读服务本地fixture，验证压缩/成员/Envelope/owner/provider/内层响应，然后两独立provider httptest重放普通ReadState；重复完整读取与原期望Pool完全一致，原header/证明节点篡改均实际拒绝。|
|F121.4|离线normal进程恢复与冻结|WP01,WP03,WP09,WP24|LOCAL_SCOPE_VERIFIED|同一正式runner显式--fixture，live_public_source_observed=false，复用双方向ExactIn/Out oracle、两无KV router冷重建/owner升代、五来源故障；Go/普通服务/Rust API/Clippy/构建/守卫/模块/契约/进度与输入冻结通过。|

原数据不进Git是既有仓库政策，不将数据藏到tools目录或force-add绕过。新机器离线命令：`python3 integration/tools/acceptance/restore_current_v2_fixture.py --bundle <准确responses.json.gz>`；同pin原capture也可--capture恢复。构建/证明本身只用当前仓库实现和服务本地数据，不依赖其他服务源码。原始Pin误差/缺数据必须失败，真实RPC不可用不能改写成本轮新实网通过。P0-1全来源矩阵/长期恢复与P0-2/3/4数学/链费/独立执行仍继续。

阶段121最终限定验收：13门禁全部exit0，83次Go race、36次Rust API、9项Python守卫、17项进度、Clippy/vet/模块/当前构建及契约通过，运行目录`30bfb64076d4489e928b0f7002c8cc59`。实际固定块26438995的32原raw、2,387,964byte gzip/12,337,328byte解码及9,250,774byte原Envelope与全部嵌套hash保持；metadata/source/Go本地副本逐byte匹配。源码和905份固定输入前后相同（新增4份source/Go数据），两次独立Go normal读取与完整Pool相同，原proof/header篡改实际拒绝；双router无KV冷恢复/owner升代/四方向金额oracle和五来源控制均通过。live_public_source_observed=false，明确仅历史公开字节离线资格，不授予新实网/共识/交易后余额/费用资格。最初Go fixture预检使用10秒总读取时限导致真实大块witness校验超时，改为与normal live相同60秒后全部通过；未改产品预算/断言。可跟踪恢复工具/metadata已准备，原本地数据按既有gitignore保留而不force-add。下一轮推进选定V2的P0-2真实bank原执行差分；旧H补取额外proof已实际遇到historical state unavailable，保留失败响应，单独取得新当前bank后原WETH deposit/transfer与Pair.swap的本地原VM收款原型成功，尚未计作正式执行资格。

<a id="plan-2026-11-18-public-v2-original-execution"></a>

## 真实V2原字节码收款与完整后状态差分（2026-11-18，阶段122，P0-2）

原Pair支付指定quoted output再比较到账相同，本身不足以证明整数精度，低报也可能成交。本轮以新固定actual Ethereum bank26439090的原WETH/DAI/Pair代码及MPT状态为基础，测试独立原字节码执行actual input，输出q成功且q+1必须因原UniswapV2: K回退；随后将实际完整后状态交给独立Rust数学核对。实现只在测试/验收边界，online parser不计算用户路由或执行交易。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F122.1|原bank/额外证明与严谨来源范围|WP03,WP10|LOCAL_SCOPE_VERIFIED|33份原raw/metadata及额外2个eth_getProof结果全部pin，header/hash/账户code/storage root/nonce/balance核对；额外结果保留原Result投影而非全HTTP envelope，明确full_http_envelope_retained=false；绝不把旧H缺proof与新H混用。|
|F122.2|本地原VM收款/精度与完整后状态|WP03,WP10,WP15|LOCAL_SCOPE_VERIFIED|固定geth1.16.7，显式synthetic EOA account/native funding，原WETH deposit→transfer Pair→原Pair swap三call；actual DAI余额差及slot8/9/a/c、原Token raw余额构造after Pool，不用被测数学填答案；q+1必须原K回退。|
|F122.3|证明/取消/资源失败封闭|WP03,WP15|LOCAL_SCOPE_VERIFIED|账户/代码/余额/nonce和每个SLOAD/SSTORE初值均需证明，未知账户/槽拒绝，外部未知调用/CREATE/DELEGATECALL/SELFDESTRUCT/BLOCKHASH拒绝；固定每call2Mgas、总1Mopcode/最多64槽4账户，ctx逐opcode检查，缺proof/账户/10opcode/取消控制。|
|F122.4|独立Rust差分与统一冻结|WP01,WP03,WP10|LOCAL_SCOPE_VERIFIED|Go当轮导出actualdelta/fullafter结果，独立Rustquote_exact_in比较output/consumed/remaining及所有Pool字段；原v2单元/连续原bank语料、Go真实bank/native/普通服务、Clippy/vet/模块/构建/输入冻结通过。|

本轮执行是同H的本地原合约call序列，使用人为建立的EOA及原生币余额，随后通过原WETH公开deposit产生输入，而不是伪造Token storage；不是已签名原Router完整交易，也不声称链上Gas/费用/未来区块或真实用户资金资格。timestamp/部分环境显式固定，源root只为provider/RPC锚定承诺；真实账户代码与storageproof核验不等于独立共识。旧固定H补取额外proof确实返回historical state unavailable，后续再次读取还出现HTTP403，均留在原探测产物；新H另行采集并在窗口内取得两额外proof，来源范围单独固定。

阶段122首次冻结`4322ceb5ed014899b5f4481ab3e16aa8`已通过Go actual原执行/Rust完整after差分及原v2回归，但完整数学all-target严格Clippy发现既有balancer_composable测试私有包含源码而未覆盖GIVEN_OUT，报两项dead_code。保留FAILED报告；补测试实际调用ExactOut核对双尺度余额/保留BPT/其他币不变及BPT/非法索引/0或全额目标拒绝，不放宽警告或库实现。该测试纳入正式回归后全部重新冻结。

阶段122第二轮冻结`3c81d09bcd394d58b832bbfacabf9796`通过Go原执行/实际bank与Rust差分/原v2，但普通native服务的既有polygon-synthetic发现用例在10秒fixture deadline返回context deadline exceeded（其余source用例通过）。该FAILED记录保留；不修改产品或测试deadline，先对同一测试无改动定向复现，再重跑全套门禁，不能把一次超时静默计作通过。

阶段122第三次冻结`3b7a24837f3b4d9f84caa677bec03f29`中普通native原测试通过，但Clippy继续发现Composable独立原数学差分文件也重复私有包含同一实现，仍有未使用GIVEN_OUT。两处旧私有源码包含统一改为直接调用正式公开crate单一内核（均仅使用public API，不改差分fixture/断言），新增ExactOut边界测试保留，原差分也纳入本轮命令。完整Clippy预检后重新冻结，不将该次FAILED升级为通过。

阶段122最终限定验收：14门禁全部exit0，16次Go race、16次Rust（包括当轮Go原执行结果的独立Rust完整afterstate比较、原v2连续bank及Composable原差分与新增ExactOut边界）、11项证据守卫、17项进度、完整数学all-target严格Clippy、Go vet/模块及当前Go/router构建通过；运行目录`fa5846888a8641eba36e9699adc2ccf3`。固定实际H26439090原MPT/code+显式合成EOA/native funding，本地原WETH deposit/transfer/Pair.swap三call测DAI收款增量17974138206626645829，q+1原合约因UniswapV2: K回退；储备/raw/cumulative/time/lock及原bank身份与Rust完整nextPool一致。缺槽/账户、10opcode、取消均拒绝，不用默认零或被测数学生成VM期望；源码与911固定输入及当轮导出前后一致。额外两proof为实际Result投影且fullHTTP未保留的限制显式记录；保留旧H proof不可用/403、Clippy重复私有模块和一次native fixture deadline的失败日志，正式门禁全部实际通过，不放宽产品预算/断言。该差分是一个真实bank下限定原call序列的P0-2证据，不授予完整签名Router交易、真实EOA资金、链费/生产VERIFIED或所有Token行为资格。下一轮按P0-3开始Legacy/EIP1559原生费用核算与覆盖范围，缺失L2/授权/可靠价格时继续明确不可用。

<a id="plan-2026-11-18-evm-native-cost-model"></a>

## EVM原生费用模型与覆盖披露（2026-11-18，阶段123，P0-3）

对照设计§13.1和§15.3，费用不能用max fee cap当实际支出，已从输出扣除的费用不能重复扣；缺可靠换算不伪造net。原Local EVM响应已会计算部分费用，本轮统一为可复用checked uint256核算并补完整范围字段，不宣称该局部内核完成所有链规则或FAST净收益评分。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F123.1|Legacy/EIP1559共享native cost与reserve|WP13,WP14,WP15|LOCAL_SCOPE_VERIFIED|有效价Legacy为声明gasPrice，EIP1559为base+min(priority,max-base)；gasUsed在21000..已绑定gasLimit、base<=max、priority<=max、全部乘加与reserve检查uint256；准备阶段复用同一reserve，保留最大预算与实际支出区别。|
|F123.2|实际API费用上下文和缺项披露|WP15,WP16|LOCAL_SCOPE_VERIFIED|diagnostic根据execution H/H+1 base fee而非准备source H的观测重算，核对worker的price/fee/burn/tip；拒绝超gasLimit或Legacy注入dynamic测量；costs包含model/chain/binding、已知执行小计、reserve/headroom及burn/tip子项，未知data/approval/net保持null，明确不从报价重复扣。|
|F123.3|独立整数向量/坏receipt控制|WP03,WP13,WP15|LOCAL_SCOPE_VERIFIED|Python独立整数56cases（48有效/8拒绝）含cap裁剪/零费/Legacy/大于128位金额/uint256溢出，Rust当轮读取全分量比较；nativecost API与无效Gas/base/legacy混入控制，不用Rust被测模型生成期望。|
|F123.4|原Router实际HTTP/完整回归冻结|WP01,WP03,WP14,WP15,WP16|LOCAL_SCOPE_VERIFIED|实际HTTP→owned原Router worker，ExactIn/Out两目标，actual native caller/coinbase delta/有效价/新costs字段和quote绑定核对；原16bank H/H+1费用区分/准备/VM回归、Clippy/构建/契约/输入冻结通过。|

未知链特定data/其他native费、额外授权/资金操作、可靠Token换算仍明确列missingComponents；nativeKnownSubtotal仅含已观测EVM execution。base burn与priority是execution的分解，不额外加到小计；reserveHeadroom是预留预算差额，不冒充Token refund。返回authority=PROVIDED_LOCAL_STATE、chainFeeCompleteness=LOCAL_EVM_ONLY，totalNativeEstimate/net仍null。FAST quote仍缺完整cost来源/估值/净排序，因此CHAIN_FEE_MODEL_NOT_IMPLEMENTED总链路缺口并未关闭，P0-3继续；生产VERIFIED和P0-4亦未开放。

阶段123最终限定验收：14门禁全部exit0，142次Rust、56份独立Python整数向量（48有效8拒绝）、3项Python守卫、普通/优化各17项进度检查、准备/仿真/API全相关all-target严格Clippy及当前router/HTTP worker构建通过，运行目录`09c3655bd378448abdbe28a442997b7a`。实际HTTP原Router ExactIn/Out各一次owned worker，actualnative caller debit/coinbase credit、有效单价而非maxcap、费用子项和新costs/binding字段均一致；source H与execution H+1、原16银行及准备/VM/取消/证据绑定回归通过。costs gasNative/nativeKnownSubtotal为唯一execution费用，burn/tip只是其拆分，reserve/headroom不假作实际fee/refund；L2/data/额外授权资金/可靠Token换算缺失保持null，total/net与生产VERIFIED未开放。源码/911固定输入和独立向量hash前后不变，实际HTTP报告摘要已绑定主report。该轮无全设计或P0-3整项完成声明；下一轮继续将EIP1559原生余额守恒与caller/coinbase重合的费用证据核验完整接入，随后补范围化成本来源/估值与净排序。

<a id="plan-2026-11-18-evm-native-balance-audit"></a>

## EVM费用原生余额守恒与源绑定（2026-11-18，阶段124，P0-3）

阶段123核对fee算术仍不足以认证worker所报账户余额；同时改变before/after可能保持差额而伪造绝对值。本轮将原worker输入中的caller/coinbase初始余额保存在parent commitment中，EIP1559测量先逐项绑定，再核对费用守恒。适用边界为既有zero-value bound ERC20交易；其他原生币资金流需要单独完整会计，不默认放行。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F124.1|原bank native初值绑定|WP15,WP16|LOCAL_SCOPE_VERIFIED|由已序列化原worker request提取caller/coinbase余额；明确exists=false可得零，账户未声明则None；测量before必须精确相同，拒绝单字段变化及同时平移before/after。|
|F124.2|分离/重合账户守恒|WP13,WP15|LOCAL_SCOPE_VERIFIED|分离时payer扣execution fee、coinbase增tip；相同账户时before/after对应字段一致且净扣base burn，不把自付tip重复扣；uint256/资金不足和scope外value流拒绝。|
|F124.3|费用单位/不可用语义与反例|WP15,WP16|LOCAL_SCOPE_VERIFIED|costs.nativeUnit=EVM_NATIVE_ATOMS、executionCoinbase与balanceReconciliation披露；Legacy无原测量标UNAVAILABLE；缺原余额、无效地址/非零value、改任一字段/成对平移/零base或alias反例验证。|
|F124.4|实际HTTP四场景及冻结回归|WP01,WP03,WP15,WP16|LOCAL_SCOPE_VERIFIED|原Router ExactIn/Out×payer/coinbase分离或重合四owned worker，明确alias为合成H+1环境且重新派生环境标识，不混称原header；原16bank/源H与H+1费用/准备/取消/整数向量及严格Clippy/构建/源码输入冻结通过。|

该核验只认证指定本地执行上下文的原生费用守恒；不会因MATCHED开放生产VERIFIED或伪称完整链费。缺L2/data/额外授权资金/可靠价格时仍为LOCAL_EVM_ONLY/缺项null。HTTP alias模式仅验收helper显式counterfactualCoinbaseAlias=true，原source H不变，H+1环境为明确合成；不广播交易，不声称本轮新公链挖矿/验证人资格。

阶段124最终限定验收：14门禁全部exit0，144次Rust、56份独立Python整数向量/3守卫、普通与优化各17进度检查、准备/仿真/API相关全目标严格Clippy及当前router/worker构建通过；运行目录`34880799bcb640ac9acabd26260acf73`。四次actual HTTP owned worker覆盖ExactIn/Out×分离/alias，原输入native before、实际payer debit/beneficiary credit、原base/tip/cap/fee、costs chain/binding/unit与source H/H+1核对通过。单字段或成对平移before/after、未声明原余额、native value流及零base/alias反例拒绝；Legacy未报告native余额明确UNAVAILABLE，不默认零；旧费用与16原bank/准备/取消回归保持。源码/911固定输入/当轮56向量与实际四HTTP报告摘要前后相同。该轮只关闭指定local执行上下文的原生费守恒缺口，不授予完整链费、可信Token估值、FAST净排序或生产VERIFIED。下一轮继续P0-3的最大预留资金准入与付款人净扣款/自付priority语义，避免将费用总账gross charge与实际账户净成本混为一项，再接范围化价格/成本来源。

<a id="plan-2026-11-18-evm-native-payer-cost"></a>

## EVM最大资金准入与付款人净扣款（2026-11-18，阶段125，P0-3）

按设计§13.1/14/15.3继续补费用来源和资金约束：实际fee小于最大reserve不代表原资金可准入；付款人等于coinbase时自付tip返回也不能绕过交易开始前的最大资金要求。该范围仍限已绑定zero-value ERC20本地交易。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F125.1|最大资金准入|WP13,WP15|LOCAL_SCOPE_VERIFIED|checked原payer余额必须覆盖gasLimit×maxFee；低实际fee/alias零净扣款/Legacy均不绕过，缺before拒绝；最终冻结门禁通过。|
|F125.2|付款人净费用|WP13,WP15,WP16|LOCAL_SCOPE_VERIFIED|保留nativeKnownSubtotal为GROSS_EXECUTION_CHARGE，独立披露payerNetNativeDebit与priorityReturnedToPayerNative，要求net+returned=gross；Legacy无后余额不虚构net。|
|F125.3|实际HTTP与冻结回归|WP01,WP03,WP15,WP16|LOCAL_SCOPE_VERIFIED|ExactIn/Out×分离/alias四owned worker核对原余额/最大准入/net守恒；独立56整数向量、原16bank、准备/VM/取消/严格Clippy和构建通过。|

本轮已完成限定范围最终门禁；完整链费、额外资金/授权操作、可靠Token换算与FAST净排序继续列为P0-3剩余，不开放生产VERIFIED。后续接范围化成本来源及估值，不能用调用者猜测价格或route_activation_cost冒充可信成本。

阶段125最终限定验收：14门禁全部exit0、146次Rust、56独立Python整数向量/3守卫、普通与优化各17进度检查、准备/仿真/API相关全目标严格Clippy及当前router/HTTP worker构建通过；运行目录`9f3c0023beb347d1a1220993c565dfe6`。四次actual HTTP原Router覆盖ExactIn/Out×分离/alias，原before覆盖bound最大reserve、实际net+自付tip返回=gross fee、Legacy净成本null及资金不足/零base-alias反例通过。源码、911固定输入、当轮独立向量及四HTTP报告摘要均冻结复核一致；不开放完整链费/可信估值/FAST净排序/生产VERIFIED。下一轮按设计§13.1修复无可信成本时配置route_activation_cost仍改变ExactIn排序的差距，采用明确的毛输出/最小原输入回退及约束披露，再接真实成本来源。

<a id="plan-2026-11-18-cost-unavailable-ranking"></a>

## 无可信成本时的路由排序回退（2026-11-18，阶段126，P0-3）

设计§13.1要求无法可靠换算Gas时返回明确不可用语义并采用可解释约束，不伪造net。复核发现ExactIn虽net为null，仍以无来源的route_activation_cost改变候选分配；本轮移除该线上排序扣项。优化器显式cost研究接口保留，正常API只在无可信来源时走raw amount回退。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F126.1|无估值ExactIn回退|WP13,WP16|LOCAL_SCOPE_VERIFIED|正常报价不再将route_activation_cost当费用；保留已配置值为ignored，实际proposal cost为零；独立整数三池oracle验证成本100会使旧690/三路降至620/两路，修复后毛输出690。|
|F126.2|双模式成本与目标披露|WP13,WP16,WP18|LOCAL_SCOPE_VERIFIED|quality.costRanking明确ExactIn最大gross/ExactOut最小required输入及候选+网格范围；未知Gas/data/授权/总费用为null，net保持null；原ExactOut独立交易grid最终回归通过。|
|F126.3|约束/预算/实际进程回归|WP01,WP13,WP15,WP16|LOCAL_SCOPE_VERIFIED|maxSplits=1仍一条路线，工作量耗尽不伪称穷尽；四actual HTTP worker、原16bank/费用/资金准入、optimizer oracle/拆单/API/严格Clippy及构建冻结通过。|

该回退关闭未经估值的费用参与线上排序的缺口；完整链费源、可靠价格及真实净收益评分仍为P0-3剩余。未将零排序扣项称为零Gas或免费交易；真实执行后的已测原生费用继续单独保留。

阶段126最终限定验收：16门禁/176次Rust、56独立费用向量、3 Python守卫及普通/优化各17进度检查通过，运行目录`7bb9f90c7f9b4060a63edd1b7ebeafd0`。独立三池整数oracle确认690/三路与未经估值成本100导致620/两路的差异，正常API对0/100及超128位配置均保留毛输出690，maxSplits=1与工作量耗尽语义正确；原ExactOut grid最小输入不受配置成本影响。四actual HTTP覆盖ExactIn/Out×分离/alias，quote回退披露和原执行费用/最大资金/余额守恒兼容，原16bank、17 optimizer及13 API回归/严格Clippy和构建通过。源码/911输入、当轮向量及四HTTP报告摘要复核一致。之前批次`fb73dff077034c7a8494bcb156f32d0e`在第三HTTP场景发生15s supervisor超时，日志保留；本次相同源码与时限完整重跑通过，未确定超时根因，未将其计入通过批次；更早主动取消的测试整理批次亦保留。该轮只关闭无可靠估值时配置费用改变线上排序的缺口，真实成本来源、Token估值和净收益评分仍未完成；下一轮推进Solana准备消息fee来源绑定与费用/租金/包装本金分类。

<a id="plan-2026-11-18-solana-native-cost"></a>

## Solana准备消息费用来源与原生资金分类（2026-11-18，阶段127，P0-3）

按设计§13.1/14/15.3补第二链的费用来源，复用正常准备流程既有getFeeForMessage，不将资金预留当费用。prepare_solana_captured统一绑定费用RPC实际查询的message与已返回unsigned wire、账户快照和payer原余额；正常API在新增costs后重新计算preparationHash，独立SBF执行与钱包仍绑定完整准备结果。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F127.1|费用源与消息/快照绑定|WP13,WP14,WP16|LOCAL_SCOPE_VERIFIED|同请求捕获一次fee RPC的实际message/slot/响应hash；限制单签名legacy原wire完全相同，fee与结果字段一致；slot可高于账户bank，明确MIN_CONTEXT_SLOT_NOT_SAME_BANK，不冒称同银行。|
|F127.2|原生资金分类及守卫|WP13,WP14,WP15|LOCAL_SCOPE_VERIFIED|checked u64校验funding=network fee+rent deposit+swap/wrap principal，payer与原账户余额一致并覆盖总需求；退款租金单列预期/留存，不预先抵资金准入；原生SOL输入不当网络fee。|
|F127.3|实际HTTP与原程序回归|WP01,WP14,WP15,WP16|LOCAL_SCOPE_VERIFIED|14门禁/29次Rust，20正式HTTP/SBF场景及独立Python审计通过；含租金预存/临时WSOL退款/解包/高slot费用，原资金/捕获/钱包、严格Clippy和正常构建通过。|

costs.nativeKnownSubtotal仅含RPC估计的message network fee；nativeFundingRequired、rentDepositNative、expectedRefundableRentNative、expectedRetainedRentDepositNative、swapOrWrapPrincipalNative分列。sourceAuthority=PREPARATION_RPC_OBSERVATION，historicalBankVerified=false；MESSAGE_BOUND_RPC_ESTIMATE不冒称最终链上费用。可靠Token估值/净值及生产VERIFIED继续不可用，完整总估值保持null；未准备就绪时费用明确UNAVAILABLE。


阶段127最终限定验收：14门禁全部exit0，29次Rust、20个当轮HTTP准备与独立原SBF执行/整数审计、普通及优化各3项证据守卫和17项进度检查通过；准备/API全目标严格Clippy与正常router/frontend harness构建通过，运行目录`96f8695a3d0d429d886a976ba59e2d56`。源码/911固定输入/45 pinned运行输入前后一致。10家族及9个额外Raydium资金情景、费用slot领先账户场景全部绑定原wire和message；feeResponseHash是canonical解码RPC Result投影摘要，未保留的原始HTTP envelope不在该摘要声明内。网络fee与资金预留、可退/留存押金、本金分离并重新seal准备hash；UNAVAILABLE/net null及生产资格边界保持。原总控进程在会话恢复时已消失，9个已通过门禁的日志摘要/冻结输入保留并核对；资金cargo在确认仍存活后由macOS NOTE_EXITSTATUS监测到正常exit0，再续跑其余门禁，恢复信息写入同一report，未凭RUNNING文件或测试尾行假定成功。完整P0-3与原26工作包均未宣称完成；下一轮补签名提交前fee context、账户/仿真/发送slot连续约束及耐久费用凭证。

<a id="plan-2026-11-18-solana-fee-preflight"></a>

## Solana提交前费用上下文与耐久凭证（2026-11-18，阶段128，P0-3）

设计§13.1/14要求提交前费用/消息保持一致。本轮修复preflight漏验fee RPC context、忽略准备时更新fee slot、发送仍使用旧账户slot的问题。预检起点为准备账户/费用slot最大值，当前费用响应必须有有效且不低于起点的整数slot；金额改变立即拒绝。账户读取以当前fee slot为minContextSlot，签名仿真以账户slot为最低点，sendTransaction沿用最后已核验的仿真slot。minContextSlot只是RPC时序约束，不证明同bank/fork/共识。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F128.1|签名费用与slot连续约束|WP13,WP14,WP15|LOCAL_SCOPE_VERIFIED|继承准备fee slot；当前费用context缺失/字符串/陈旧、费用变化、账户/仿真回退都应在发送意图前拒绝，实际前进slot逐段及意图前拒绝已验证。|
|F128.2|耐久费用凭证与幂等兼容|WP14,WP15,WP16|LOCAL_SCOPE_VERIFIED|凭证绑定实际签名message/transaction摘要、准备/链身份、fee Result投影/确认级别及各slot；仅新增字段时进入v1哈希，旧记录字节兼容；更新不准替换凭证，同交易重复预检返回首次耐久凭证。|
|F128.3|签名/SBF/资金回归与冻结构建|WP01,WP14,WP15,WP16|LOCAL_SCOPE_VERIFIED|旧v1手造日志读取/更新、新凭证重启与篡改拒绝、八种签名生命周期和并发一次发送、Pump/Raydium资金钱包/原SOL交易通过；11门禁全部通过。|

费用凭证sourceAuthority=SUBMISSION_RPC_OBSERVATION，sameBankClaimed/historicalBankVerified/independentProductionVerification均false。费用Result摘要为canonical解码投影；预检观察不冒称原始HTTP归档或共识银行。完整估值/净评分/全部链规则和生产VERIFIED仍未完成。

阶段128首轮冻结`89f687a2576f4622ae6ffaa2ffe18948`保留FAILED报告：42项API库测试通过（另5项原有忽略）；实际钱包已通过准备fee领先、六种费用/slot拒绝与并发一次发送，随后重启查询触发新测试桩对getSignatureStatuses错误复用账户RPC参数断言。修正只针对该测试桩分支，产品时序校验、原deadline/断言不放宽；完整冻结回归待重跑。

阶段128第二轮冻结`2a0489e893394fb5b6c5b82b54bba134`保留FAILED报告：两个既有EVM取消测试在8秒内未出现owned PID；单项独立复现失败，macOS采样定位启动前spawn_blocking仍在executable_sha256中对大型Rust debug镜像做software SHA-256。取消探针实际启动/bin/sh，现仅将两测试的worker_path/hash绑定为该实际小型可执行文件，保留每次完整身份校验、8秒启动/取消/reap门槛；真实Rust worker/协议/身份路径不改，非跳过哈希或提高deadline。将完整重跑本轮门禁。

阶段128最终限定验收：11门禁全部exit0，46次Rust（API库42项实际执行、5项原有忽略；实际钱包生命周期1项内部八种场景，WSOL资金钱包2项及原SOL签名回执1项）、普通/优化各17进度检查、准备/API全目标严格Clippy和正常router/frontend harness构建通过。运行目录`a63ef262b6334a50858e5ca3154dabff`；源码/911固定输入/45 pinned运行输入前后一致。准备fee slot领先账户1，预检fee/账户/signed simulation随后分别前进至+2/+3/+4，实际send使用+4；六种陈旧/缺失/错误类型/费用改变及后续账户/仿真回退均不写发送intent。同一签名并发双提交仅一次send，重复与重启保留首次完整费用凭证；独立手造旧v1日志正常读取/更新，新凭证篡改或删除update拒绝。保留测试桩分支错误及两项EVM取消探针启动前hash超时FAILED报告；独立复现/采样后，仅修正测试RPC分支及取消探针实际小型/bin/sh的完整身份校验，原8秒启动/取消/reap断言与真实Rust worker/产品时效/金额守卫均保留。凭证哈希绑定实际signed message、transaction及canonical fee Result投影，RPC观测不声称同bank/历史来源/生产VERIFIED。完整P0-3净估值、通用费用后排序与其余设计功能继续；下一轮补限定原生SOL/明确解包WSOL的lamport同单位估值，经济净额与租金资本流分别核验。

<a id="plan-2026-11-18-solana-native-economics"></a>

## Solana原生同单位费用后净额（2026-11-18，阶段129，P0-3）

按设计§12.5/13.1/14/15.3，从已执行的具体准备交易开始补真实费用后估值。原生SOL和明确临时WSOL退款/解包的结算以lamport同单位映射，无外部池价格；只在受支持的实际结算上下文提供单侧net输入或输出，不把普通SPL币猜测换算。独立SBF证据与RPC签名回执共用核算，金额采用实际消耗/收款，不用ExactOut最大资金或请求目标替代。原报价及优化排序保持其既有未知成本口径，未接通用net排名。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F129.1|同单位整数经济净额|WP13,WP15|LOCAL_SCOPE_VERIFIED|native/明确WSOL结算按实际输入+network fee、实际输出-network fee；原始u64与signed native delta规范，负净输出显式不可用，精确零保留；租金押金作为资本流不计network费用。|
|F129.2|正常SBF/回执接线与来源披露|WP14,WP15,WP16|LOCAL_SCOPE_VERIFIED|执行后核算并重新seal证据；回执沿用实际fee/rent而非准备猜测。分别标明PINNED_LOCAL_SBF_EXECUTION、RPC_CAPTURE_SBF_EXECUTION、RPC_TRANSACTION_RECEIPT，生产/历史bank及quote排名资格不升级。|
|F129.3|原程序/签名回执与独立核验|WP01,WP14,WP15,WP16|LOCAL_SCOPE_VERIFIED|4项正常库单测与11个native SOL/显式WSOL/普通Token场景通过，已验证原SBF、原签名一次发送/重启与净额相同；覆盖实际超额收款、最大输入差异、租金及fee超过输出。正常入口和16项冻结门禁通过。|

该范围只产生所选已执行准备交易的限定费用后金额，非可信链bank或全候选Gas-aware路由；普通Token价格、完整多链费用/链规则、通用净评分与其余原26工作包仍需继续。

阶段129最终限定验收：16门禁全部exit0、50次Rust（API库46项实际执行及5项原有忽略、11场景集成一项、20家族准备回归一项、八场景钱包生命周期一项及原生SOL签名回执一项），普通/优化各17进度检查和4项独立审计守卫、准备/API全目标严格Clippy、正常router/frontend harness构建通过；运行目录`d40750b2eca24fafb243f2e8d336b797`。11次正常HTTP独立原SBF与11次实际签名本地原程序执行的回执，逐一验证净额/押金/原付款人余额，重复提交只发送一次、重启后最终回执一致；另外20种原准备/SBF家族回归及其独立审计通过。独立Python读取保留的实际签名wire、meta原生/Token前后余额、实际fee/账户创建押金，不借用被测net字段计算期望；准备和证据hash覆盖新增费用字段，源码/911固定输入/45运行输入前后一致。ExactOut请求1000而实际收10971，费用后5971；原生输入1000000加fee得1005000，2039280租金另列，原生现金包含押金；fee大于输出保持null、精确零保留。原型与正式门禁分开，首次单独进度检查在尚未创建正式报告前发现链接不存在，正式运行建立真实RUNNING报告后两模式全通过，未创建虚假通过占位。该轮仅赋予明确已执行native/临时WSOL的lamport同单位核算；RPC回执为受控本地原程序执行观察，非主网交易或共识银行，普通Token可信估值、全候选净排名/完整链规则/生产VERIFIED及原26工作包仍未全部完成。

<a id="plan-2026-11-18-native-economics-view"></a>

## Solana费用后经济净额前端闭环（2026-11-18，阶段130，P0-3）

设计§13.1/14/15.3要求费用的来源、金额和单位明确。阶段129后正常API已返回限定已执行交易的native同单位净额，但前端只显示毛输入/输出和单独fee，用户尚不能看到净额与不可用原因。本轮在当前SBF证据和实际回执分别显示费用后输入或收款、网络费与留存资本押金；旧回执无costs字段保持原有可读性，零保留，费用大于输出明确不可用。展示前绑定chain/preparation/实际transaction摘要、来源与单位，拒绝不同交易/错误资格/非规范金额。所有金额继续字符串/BigInt处理，不丢失大整数精度。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F130.1|SBF/回执经济净额显示与身份守卫|WP13,WP15,WP16|LOCAL_SCOPE_VERIFIED|SBF/实际回执来源与当前chain/preparation/transaction绑定；金额字符串及BigInt、zero/null、普通Token不可用和旧回执兼容通过，签名区/回执区分离后重载默认EVM仍显示原Solana回执。|
|F130.2|真实浏览器/钱包/重启与冻结回归|WP01,WP14,WP15,WP16|LOCAL_SCOPE_VERIFIED|五类正常HTTP/原SBF/签名回执及重载实际可见，8门禁、18前端单测、16浏览器检查和桌面/手机截图复核通过；源码/911固定输入/45运行输入前后一致。|

该轮只是已有已执行交易经济净额的客户端闭环，不添加新的市场价格、不将受控RPC回执提升为主网/共识来源。通用可信Token估值、全候选净收益排序、链特定费用/生产VERIFIED和其余设计功能继续未完成。

阶段130首批自动检查结果（后续人工截图复核发现未通过显示验收）：8门禁全部exit0，正常浏览器16项检查，五类正常Router HTTP/独立原SBF/实际签名本地原程序回执及重启展示通过。普通Token不显示伪造net，原SOL实际超额收款显示5971，原生买入显示费用后输入5003并单列2039280租金，WSOL退款5004与解包负净输出不可用均符合服务端观测；刷新页面不重复发送。前端整数/来源/交易绑定及旧回执、zero/null兼容守卫通过；普通/优化进度各17项、契约、格式与正常harness构建通过。运行目录`b9c4f6d5476841eb95a7a298f82f34c0`，源码/911固定输入/45运行输入前后一致，五份原始正常API观测和画面摘要绑定报告。独立Python分别复核五份SBF/回执的整数成本和来源。该轮UI仅展示绑定的已执行交易经济净额，未将FAST报价、通用价格来源、全候选净排序、生产VERIFIED或其余工作包提升为完成；后续继续P0-3和剩余优先级。

阶段130截图复核拒绝首批`b9c4f6d5476841eb95a7a298f82f34c0`：自动16项检查只核对回执文本，重载默认EVM链后整个wallet-panel隐藏，导致已恢复Solana回执及新净额在画面不可见；保留自动报告原文和FAILED_MANUAL_VISUAL_REVIEW报告/截图。修复将耐久回执面板与链特定签名面板分离，加入真正可见性断言，完整门禁待重跑；不将DOM中隐藏文本作为用户可见闭环通过。

阶段130修复后最终限定验收：运行目录`07d79503063f4f5799b27f8239e3b393`，8门禁全部exit0、18前端单测、16正常浏览器检查。普通Token、原SOL实际超额收款5971、原生输入5003并单列2039280租金、WSOL退款5004和解包负净额不可用，五类SBF/实际签名本地原程序回执分别展示正确；重载后回执和刷新按钮真实可见，发送次数不增加。新增正常执行前/重载后截图及手机截图均绑定报告；人工复核确认默认EVM控件不会再隐藏恢复的Solana回执，手机负净额原因可见且无横向溢出。独立Python复核五份SBF/回执整数核算，普通/优化进度各17项、格式/契约/正常harness构建通过，源码/911固定输入/45运行输入前后一致，harness实际二进制前后摘要相同。保留首次自动通过但被人工截图拒绝的原报告及FAILED_MANUAL_VISUAL_REVIEW结果，没有把隐藏DOM文本当作最终通过。该轮只完成限定已执行经济净额的前端闭环，完整可信估值/全候选净路由、失败交易费用核算、可信bank/链规则、生产VERIFIED和其余工作包继续；下一轮按P0-3补失败签名交易的实际网络费用与回滚收付核验。

<a id="plan-2026-11-18-solana-failure-fees"></a>

## Solana失败签名费用与回滚核验（2026-11-18，阶段131，P0-3）

设计§13.1/14.3/14.5/15.3及WP13–16要求费用有实际来源、守恒及独立失败语义。原正常回执在签名状态err非空时直接标FAILED，未读取getTransaction，遗漏失败交易仍扣网络费。本轮为已支持的recent-blockhash InstructionError失败增加独立failureFee观察；同一原签名wire、签名/transaction摘要、slot和status/meta错误绑定，meta原生付款人差额只扣实际fee，其他native余额及全部已报告Token整数/身份/精度保持。已知原有源/目标Token账户仍要求元数据，重复/越界/缺失/错误类型均拒绝；没有将RPC元数据覆盖等同完整原账户状态或共识bank证明。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F131.1|失败网络费/回滚观察与耐久结果|WP13,WP14,WP15,WP16|LOCAL_SCOPE_VERIFIED|原签名绑定、整数/身份回滚、独立failureFee与旧v1兼容，已通过冻结门禁。|
|F131.2|实际失败钱包/故障/重启与独立审计|WP01,WP14,WP15,WP16|LOCAL_SCOPE_VERIFIED|三类原程序失败，18故障、processed/confirmed/消失/最终恢复、独立Python和成功回归冻结验证通过。|

processed或meta缺失保持RECEIPT_PENDING；confirmed已核对费用也不提前终态，允许最终确认或签名消失撤销费用观察；finalized且完整核对才FAILED。矛盾/篡改标SETTLEMENT_MISMATCH，保留ONCHAIN_TRANSACTION_FAILED及具体feeVerificationError。新字段默认省略，旧v1 entry/identity hash不变，结果受完整entry hash保护。未知失败类别和nonce未扩展；失败收付款/net不虚构为成功。受控本地原程序执行不升级生产VERIFIED，完整链费/可信Token估值/净排序及其余设计工作继续。

三类原型均通过，首次独立审计明确因原型仅含17故障而拒绝，保留日志；正式集合增加第18项错误类别及confirmed签名消失撤销检查后再完整验证，不放宽检查。

阶段131冻结记录：首个冻结批次`7894472440314150a1b87d19a113f61d`在格式通过后主动取消并保留CANCELLED报告，原因是复核发现新Make入口用了未定义PYTHON变量；停止总控并确认exit130后改为现有python3入口，同时补签名消失时清空旧confirmationStatus/slot和InstructionError索引范围守卫。随后重新冻结全部非Markdown源码，不借用已取消批次作为验收。

阶段131当轮实际交易与独立审计已通过（最终门禁仍进行中）：Token2022单instruction在0处Custom17回退；Raydium临时WSOL七instruction在5处回退；原生SOL创建ATA两instruction在1处回退。三者原付款人分别10000000000→9999995000、10000000000→9999995000、1000000000000→999999995000；实际网络费均5000、留存租金0。16/16/19个native账户和5/2/1个Token账户观察匹配，原签名重启结果一致。18种缺失/篡改/矛盾故障及各自可恢复分支通过，独立Python普通/优化模式均核对实际wire与原余额，并各拒绝30次审计篡改。八场景原生命周期、11成功经济净额回归和严格Clippy/构建继续，尚不提前记整轮通过。

阶段131最终限定验收：运行目录`0b9e9c453c3048c59dba606fa44dbfb2`，13门禁全部exit0、52次Rust（API库49实际通过、5原有忽略，实际失败3场景集成1项、原钱包8场景集成1项、成功经济净额11场景集成1项）。3次实际签名原SBF失败、18种API元数据故障及恢复、普通/优化各30独立审计篡改、普通/优化各17进度检查、准备/API全目标严格Clippy和正常router/harness构建通过。三种失败均仅实际扣费5000、资金操作与账户创建回滚、留存租金0；processed/confirmed保持可重查，签名消失清除fee/旧确认slot，重新确认和最终失败及重启结果一致，重复提交仅一次send。原成功净额与租金分类回归通过。源码/911固定输入/45 pinned运行输入前后一致，主报告绑定实际交易审计摘要和各门禁日志hash；旧v1无新字段的identity/entry hash兼容，新failureFee完整entry篡改拒绝。首个Make变量问题批次CANCELLED与17故障原型审计拒绝记录保留，无放宽断言或产品预算。该轮不升级RPC观察为历史bank/生产VERIFIED，也未实现通用Token可信估值/全候选净排序/未知失败类别及nonce；全部设计与原26工作包仍未完成。下一轮补失败费用与待核验状态的真实页面展示和刷新/重载闭环，再继续P0-3剩余费用估值及原优先级。

<a id="plan-2026-11-18-failed-fee-view"></a>

## 失败交易费用前端闭环（2026-11-18，阶段132，P0-3）

设计§13.1/14.3/15.3要求实际费用来源及失败语义明确。阶段131后正常API的failureFee已核对原失败签名和余额，但页面只显示ONCHAIN_TRANSACTION_FAILED，尚不能看出失败仍扣网络费。本轮在独立耐久回执区展示RPC来源的失败网络费、原付款人SOL差额、已报告回滚一致及零留存押金；待元数据/矛盾/旧回执明确不可用，不猜零。展示前绑定chain/preparation/transaction/signature/slot、错误与确认状态，并用BigInt复核原生金额守恒；来源/生产资格/混合成功收付/非规范金额不一致不展示费用金额。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F132.1|失败费用显示及身份/金额守卫|WP13,WP15,WP16|LOCAL_SCOPE_VERIFIED|22前端单测通过；u64/zero/旧回执/待确认/消失及混合结果守卫，冻结验证通过。|
|F132.2|真实浏览器失败/成功对照与重载|WP01,WP14,WP15,WP16|LOCAL_SCOPE_VERIFIED|普通Token、原SOL创建ATA和临时WSOL三失败及原SOL成功对照，真实签名/重载可见/一次发送/手机，冻结验证及截图复核通过。|

正常HTTP/独立原SBF预演成功后，受控本地钱包签名，send时才注入冻结账户使真实原程序回退；不伪造报价或回执。测试控制扩展只作用明确本地harness，可同时设置受控wallet banks，正常服务入口不增加测试模式。成功/失败金额不混用，不将RPC观察升级为历史bank或生产VERIFIED；通用价格来源/净路由和其余设计优先级继续。

阶段132最终限定验收：运行目录`69616c6ba1e947a7b83b1c0924f4056d`，9门禁全部exit0，22前端单测与13正常浏览器检查通过。普通Token、原SOL创建ATA、临时WSOL三次真实签名原程序回退，页面显示RPC网络费5000、payer差额-5000、已报告余额回滚和留存租金0；成功对照仍显示实际10971/费用后5971。四类正常HTTP/独立原SBF/实际签名回执、重载默认EVM后回执与刷新按钮真实可见且发送次数不增，手机无横向溢出。人工复核正常成功执行、原SOL失败重载和手机原图，确认没有将隐藏DOM文本当显示通过；原图与API结果摘要均绑定报告。独立Python检查失败费用整数守恒/交易绑定/资格及成功净额，普通/优化各17进度、harness严格Clippy与正常构建通过，源码/911固定输入/45运行pins及harness二进制前后一致。旧/未核验/矛盾回执不猜零，跨交易/错误来源/混合成功收付/u64精度守卫通过。该轮仅完成已执行失败费用的页面闭环，完整多链费用、可信Token估值/全候选净评分、可信bank/链规则及其余原26工作包继续；按既定P0-3推进剩余实现。

<a id="plan-2026-11-18-evm-legacy-native-fees"></a>

## EVM Legacy原生费用来源与净扣款（2026-11-18，阶段133，P0-3）

设计§13.1/14.2/14.3/15.3要求实际费用来源、Gas口径与付款人余额守恒明确。阶段123–125已实现Legacy算术和最大资金准入，但正常worker只为EIP1559返回native_fees，Legacy的payerNetNativeDebit因此未知。本轮将同一原始执行前/后image的caller/coinbase余额测量扩展到Legacy，以实际gasPrice而非EIP1559 priority cap计算；共用base burn/priority分解、最大reserve、源before绑定和caller=coinbase净扣burn语义。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F133.1|Legacy原生测量与API源绑定|WP13,WP14,WP15,WP16|LOCAL_SCOPE_VERIFIED|三price×两账户关系×成功/回退/HALT原型通过；源绑定/整体平移/伪造fee拒绝与16原合约bank通过。|
|F133.2|正常HTTP/独立向量及原执行回归|WP01,WP14,WP15,WP16|LOCAL_SCOPE_VERIFIED|Legacy和EIP1559各ExactIn/Out×分离/alias四HTTP、16原Router bank及56独立整数向量，14门禁冻结验证通过。|

旧Legacy缺native_fees仍明确UNAVAILABLE/net debit null，不从公式伪造余额观测；已有EIP1559缺测量继续拒绝。新Legacy测量不允许通过整体平移before/after绕过源绑定，预留上限与实际fee分列，alias也必须具备最大预留资金。仅限定零value/提供本地Shanghai执行的原生费用，不授予完整链规则、L2/data费用、可信Token换算、全候选净路由或生产VERIFIED。

阶段133最终限定验收：运行目录`7ce6512b07a248c093603bdd7b0ede46`，14门禁全部exit0、148次Rust，16原Router银行的Legacy/等价EIP1559原生测量与afterstate对照通过，18种price×alias×成功/回退/HALT控制组合及精确零/低于base fee边界通过；回退和HALT没有伪造Token成交。正常HTTP原worker共8次（Legacy和EIP1559各ExactIn/Out×分离/alias），实际gasPrice/有效动态价、burn/tip/gross charge与源before/实际after一致。56份独立Python整数向量（48有效8拒绝）、3审计守卫、普通/优化各17进度、准备回归、仿真/API全目标严格Clippy和正常router/harness构建通过。源before单独或整体平移、错误费用分解/原生value、缺源及最大资金不足均拒绝；Legacy旧无native_fees结果仍未知，EIP1559缺测量继续拒绝。Legacy分离账户ExactIn实际gas104566、gasPrice2000000000、gross费209132000000000；alias时自付priority返回，仅净扣已观测base burn，最大reserve准入保持。源码/911固定输入/56向量前后一致，两组四HTTP报告及各simulation/日志摘要复核一致。本轮未以Legacy测量补齐声称L2/data fee、额外资金授权、可信Token价格/净排序或当前真实链规则资格；原26工作包和设计全范围仍未完成，继续按P0-3剩余费用来源/估值/净评分与后续优先级推进。


<a id="plan-2026-11-18-op-regolith-data-fee"></a>

## OP Regolith历史L1数据费内核（2026-11-18，阶段134，P0-3）

依设计§5.1/13.1/14.2/15.3，推进独立于EVM gasUsed的L1数据费。本轮为后续来源接线提供共享Rust算术入口，限定OP/Mainnet与Base已明确Regolith启动至Ecotone激活前；Ecotone首块特例和其后所有版本均拒绝套用。参数从完整260字节Bedrock L1 attributes calldata提取，包含L1 block/时间/原baseFee/overhead/scalar，绑定attributes与完整编码交易字节SHA256。算术入口不验证交易RLP/签名、deposit身份、genesis、header/body承诺或来源时效；这些仍由调用方单独认证，参数hash不授予可信来源资格。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F134.1|独立Regolith参数与原始交易字节费用内核|WP13,WP15|LOCAL_SCOPE_VERIFIED|原op-geth函数、三笔原signed user wire及72宽整数向量；验证记录（`integration/artifacts/local/op-l1-data-cost-check/report.json`，已清理）。|
|F134.2|ABI/激活/类型/字节预算与最终溢出守卫|WP13,WP15,WP16|LOCAL_SCOPE_VERIFIED|Rust有限范围与独立参考普通/优化守卫；验证记录（`integration/artifacts/local/op-l1-data-cost-check/report.json`，已清理）。|
|F134.3|冻结输入与原正常worker回归|WP01,WP15,WP16,WP18|LOCAL_SCOPE_VERIFIED|准备/API/原bank及Legacy/EIP1559正常HTTP回归；验证记录（`integration/artifacts/local/op-l1-data-cost-check/report.json`，已清理）。|

Regolith计完整已编码用户交易的零/非零字节，不使用仅calldata/unsigned preparation代替，不重复加pre-Regolith的68字节；4/16 gas权重、overhead与scalar按原整数顺序相乘后向下除1e6，最终fee检查uint256。中间乘积及gas加overhead保留宽整数，避免先截断/溢出或将显式零判为未知。输入字节预算由调用方传入；空输入、deposit/未来类型、不支持链/激活范围、错误ABI/u64高位/L1时间晚于execution均拒绝。该函数不是完整交易解码器，synthetic byte vectors仅作算术探针。

独立参考从既有native语义固定的op-geth `79e9d244748a1201fe686ca8a882b5fe7e173348`取原`newL1CostFuncBedrockHelper`和`l1CostHelper`，完整文件SHA256固定，选中函数原文不改动，由小Go包装仅提供类型/常量和I/O执行。留存OP历史body的原L1 deposit和三笔原signed user wire，结果分别对照留存RPC回执的l1Fee/l1GasUsed；另72组零/高位/舍入/最终溢出与宽中间值，不能称72笔可执行交易。历史RPC费用字段不属于receipt trie承诺，原source/body/root验证和链源资格没有被此算术对照替代。

验证工具：[validate_op_l1_data_cost.py](integration/tools/acceptance/validate_op_l1_data_cost.py)，报告：op-l1-data-cost-check/report.json（`integration/artifacts/local/op-l1-data-cost-check/report.json`，已清理）。本轮尚未把该历史核算接入正常报价的完整链费、付款人总余额核验或候选排名；正常Legacy/EIP1559 worker作为回归，不意味着OP执行适配器已完成。后续继续P0-3的同块属性/签名交易来源绑定和正常链费接线，再补独立版本的Ecotone/Fjord/后续模型、可靠Token估值及净评分；其余优先级均保留。

阶段134首次冻结验收保留FAILED：算术预检4项及75原参考输入通过，正式进度门禁发现新增plan锚点未登记批次历史。已按原文档约束补历史行及任务表，未放宽校验；重启完整冻结验收，首批报告保留。

阶段134最终限定验收：16门禁全部exit0、147次Rust（仿真100/API20/准备23/新内核4），普通/优化各17进度检查与各4参考守卫、准备/仿真/API全目标严格Clippy和正常二进制构建通过。最终运行目录`a168d508bf86428ba24a4cd4195adc83`，源码摘要`ee8b977c5db0b98904310dabf1920afe3ef5ae5e9fc519876ac97270a773e6b3`及911固定输入、45 runtime pins、5参考输入前后一致。75原Go计算包括三笔原signed公链交易（逐笔和留存RPC l1Fee/l1GasUsed一致）及72算术字节探针，8个最终uint256溢出拒绝；显式零及宽中间积保留。原正常Legacy和EIP1559各四HTTP/worker（ExactIn/Out×分离/alias）均回归通过，未把这些Ethereum给定银行当作OP执行证明。保留首批文档门禁FAILED报告；本轮只是历史费用内核限定完成，下一轮复用普通native核验补同一body内L1属性/deposit身份/完整签名user wire来源投影及重放，不将尚未完成的正常链费/现行OP模型、净估值/评分/生产资格提前关闭。


<a id="plan-2026-11-18-op-l1-source-inputs"></a>

## OP Regolith同body费用输入来源与重放（2026-11-18，阶段135，P0-3）

沿设计§5.1/7.3/13.1/14.2/15.3，从历史费用内核推进其来源。普通native `ReadQualifiedReceipts`及`VerifyNativeReceiptEvidence`在同一header/body/receipt验证之后，自动生成仅适用于OP/Base Regolith至Ecotone前的`op_regolith_fee_inputs`。其余链/时间域该字段缺省，不借用历史模型为当前OP计费。Go只提取事实与验证签名身份，不计算L1费用或Token换算。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F135.1|普通native来源与完整签名user wire|WP03,WP05,WP13,WP15|LOCAL_SCOPE_VERIFIED|同header/body承诺、首deposit身份/完整属性ABI、原wire/txID/链ID与London签名恢复；验证记录（`integration/artifacts/local/op-l1-source-inputs-check/report.json`，已清理）。|
|F135.2|历史记录/重放与旧归档兼容|WP05,WP15,WP18|LOCAL_SCOPE_VERIFIED|原证据重算投影、12篡改拒绝、旧缺省不借零费用、header RawMessage压缩前后摘要稳定；验证记录（`integration/artifacts/local/op-l1-source-inputs-check/report.json`，已清理）。|
|F135.3|正常服务/跨语言费用与冻结回归|WP01,WP13,WP15,WP16,WP18|LOCAL_SCOPE_VERIFIED|原OP四tx body/三user wire经普通读取与回调归档后交Rust内核，并对原op-geth；旧native/准备/API/正常构建回归；验证记录（`integration/artifacts/local/op-l1-source-inputs-check/report.json`，已清理）。|

首项deposit必须为原System depositor到L1Block、零mint/value、非system flag，完整260字节`setL1BlockValues`属性，且deposit sourceHash必须按原L1-info domain 1由属性L1 hash/sequence精确导出；L1 number/time/sequence为u64，L1时间不晚于目标执行块。全部属性/用户字节来自同一已核验body；user只接受该历史范围的legacy/type1/type2，解码链ID、原txID和真实签名恢复，deposit单列不作为用户数据费对象。输入投影含block/hash/time/body root、native语义、compact header与attributes摘要、user index/hash/raw/sender，无费用金额和共识标签；receipt的l1Fee等非trie字段不进入此来源投影。解析/保留空间在原递减证明预算内预扣，不新增RPC调用，正常完成前仍按既有最终锚点检查。

历史记录增加可选投影，重放每次从完整原证据重新验证后比较，改链、属性、用户wire/sender/index、body root、deposit或authority均拒绝；旧v1缺省仍可重放既有事实，缺省不是零费用或新费用资格。JSON RawMessage归档会压缩空白，header来源摘要基于compact bytes，避免一次正常保存重载就产生假矛盾。来源仍是RPC锚定header/body承诺，非独立共识或已执行完整OP VM，不能将原L1属性视作独立L1最终性证明。

正常读取测试的完整JSON视图由既有原signed wire生成，其再编码须匹配已留存body root/txID，未伪造新的公网响应。三笔user输入经Go普通完成回调及历史记录，交Rust共享内核与固定原op-geth及留存RPC费用观察对照；该跨语言路径是限定集成验证，不表示当前报价API已消费完整OP链费。测试工具：[validate_op_l1_source_inputs.py](integration/tools/acceptance/validate_op_l1_source_inputs.py)，报告：op-l1-source-inputs-check/report.json（`integration/artifacts/local/op-l1-source-inputs-check/report.json`，已清理）。后续仍需版本化现行OP费用及来源、router正常链费接线/完整付款人净扣款、可靠估值与净评分；原26工作包完整验收和VERIFIED继续关闭。

阶段135首批冻结验收保留CANCELLED（运行目录`8ba4d917c9d44404bddcff539c9d1e0d`）：文档/原仿真/API/准备回归通过，Go EVM运行时补充复核发现应进一步校验L1-info deposit sourceHash。主动停止总控并确认取消/子进程回收后，增加原domain 1与属性L1 hash/sequence的一致性，固定原Optimism v1.9.5 source方法及25独立向量，扩大语义控制后重新冻结；未改预算或放宽断言。

阶段135最终限定验收：18门禁全部exit0，148次Rust（原仿真100/API20/准备23/费用集成5）和209次Go（EVM全量race203/普通服务race6）通过，Go vet/build、Rust准备/仿真/API全目标严格Clippy和正常router构建通过。最终运行目录`1cef4e16300544e2ba15abf505b5559e`，源码摘要`8983bd8fe7bf189ac57970adc7c14fd2f42763583e6080e293b4cebec0b7e8c7`及911固定输入、45 runtime pins、8参考输入前后一致。三笔原signed user wire经普通读取/历史回调与重放，交Rust对照原op-geth；25原sourceHash向量、12投影篡改、6来源故障、2正常发布前拒绝、15语义/预算控制与归档空白稳定性通过。普通/优化各17进度检查、各4参考守卫通过；首批主动取消报告保留，未放宽预算。该轮只完成历史费用输入来源与跨语言核算，未赋予独立L1/共识、现行OP执行或正常报价完整链费资格。后续按P0-3补独立版本的Ecotone/Fjord压缩与operator费用、正常来源/完整净扣款，再推进可信Token估值及净评分；原26工作包和设计全范围仍未完成。

<a id="plan-2026-11-18-op-packed-cost"></a>

## OP packed费用与FastLZ独立核算（2026-11-18，阶段136，P0-3）

依设计§5.1/7.3/13.1/14.2/15.3，在独立Rust模块`op_packed_cost`实现三种严格长度/selector的packed属性解码（Ecotone 164、Isthmus 176、Jovian 178字节），共享Ecotone/Fjord数据费与Isthmus/Jovian operator整数费用。模型由调用方显式选择，模块不凭selector推断硬分叉；Ecotone和Fjord同selector，Jovian首块可能仍用旧属性，实际链/激活/首块规则与正常报价接线继续单独实现。旧Bedrock/Regolith入口保持原历史范围。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F136.1|packed参数/FastLZ与两种DA公式|WP13,WP15|LOCAL_SCOPE_VERIFIED|原op-geth函数原文执行605输入，宽中间值/最终uint256及长度/显式零控制；验证记录（`integration/artifacts/local/op-packed-cost-check/report.json`，已清理）。|
|F136.2|operator单位/舍入/常量与缺省语义|WP13,WP15,WP16|LOCAL_SCOPE_VERIFIED|Isthmus除1e6与Jovian乘100独立规则，gas/scalar/constant宽度和旧格式缺参数不猜零；验证记录（`integration/artifacts/local/op-packed-cost-check/report.json`，已清理）。|
|F136.3|当前公链body普通读取/重放与独立核算|WP03,WP15,WP18|LOCAL_SCOPE_VERIFIED|50交易/receipt header承诺、49原signed user、4来源反例及新旧费用/仿真/API回归；验证记录（`integration/artifacts/local/op-packed-cost-check/report.json`，已清理）。|

Fjord对完整编码user wire计算FastLZ压缩长度，估算大小`max(100e6,836500*FastLZBytes−42585600)`，费用保持原整数顺序除1e12，l1GasUsed为估算大小乘16再除1e6；Ecotone按零/非零字节4/16计Gas，乘原base/blob fee和scalar组合后除16e6。不是仅calldata或预签名长度估计，不加旧68字节。单wire最多16MiB且遵守调用方更低预算，拒绝deposit/blob/未知类型；Type4还需带operator时代属性，完整交易/签名和激活资格由调用方另验。显式零有效，缺operator属性保持未知。operator两规则保持独立，不从DA字段借零；fee-charged gas及refund语义由执行适配器认证，DA footprint scalar只保留，不误当作附加DA费用。

当前原始输入来自官方`https://mainnet.optimism.io`于本日采集的finalized块159697405、hash`0xe400a05f016ba3a9115fe13874fb5790e3c69c7fa395e2ebe64d7ba07d944f51`，50交易/50receipt，读后高度锚点一致。公开raw/bulk方法403后通过完整交易JSON和逐笔receipt完成有界采集；此前PublicNode TLS EOF、drpc403及成功官方请求原响应均保留。采集目录`integration/.runtime/design-review/op-current-captures/e7cc59e6c16c4560ad1229b0264993f6`最初标CAPTURED_NOT_VERIFIED；本轮新增测试以固定完整原字节送普通native ReadComplete与历史回调重放，重算header/body/receipt根，不从采集状态推定可信。user恢复链ID10真实签名，gas取已承诺cumulative差分，deposit身份及L1-info sourceHash独立校验。额外RPC l1Fee/l1GasUsed只作非trie观察对照，不授予共识或独立L1最终性。

独立参考复用固定op-geth原文件SHA256，执行未改动的FastLZ/Ecotone/Fjord/operator函数、原估算方法及常量。605输入包含49原signed用户和556算术字节/参数探针（含103最终溢出）；后者不冒充可执行交易。正式验收前导出原来源以生成参考，冻结期间再次正常导出且必须字节摘要一致。预检先遇负对照误读normal已保存raw而非JSON数组的测试panic，修正测试取实际raw后四反例通过；未改变产品预算或放宽验证。验证工具：[validate_op_packed_cost.py](integration/tools/acceptance/validate_op_packed_cost.py)，报告：op-packed-cost-check/report.json（`integration/artifacts/local/op-packed-cost-check/report.json`，已清理）。本轮不授予完整现行OP执行资格，未接完整payer净扣款/报价链费/可信Token估值与候选净排序；这些与后续优先级继续。

阶段136最终限定验收：运行目录`b2a16308638f42c6a92b37b42256619d`，18门禁全部exit0、150次Rust（仿真100/API20/准备23/新packed4/旧Regolith3）及6次Go（新当前来源1/旧历史来源5）通过，普通/优化各17进度检查与各4参考守卫、Go race/vet、准备/仿真/API全目标严格Clippy及正常router构建通过。605份固定原Go参考含49原signed用户与556算术探针、103最终溢出；原用户费用和l1GasUsed与留存非trie RPC观察一致。当前50交易块经普通ReadComplete/历史回调重放核验后导出，正式期间再次导出字节一致；四来源篡改拒绝，旧Regolith范围与归档回归通过。源码摘要`dd5ebfbd0e44e2633e2f20a33baf2779667d3056493f16c59bc3c70d60db0851`及911固定输入、45运行pins、9参考输入前后一致，所有日志与导出/向量报告摘要核对。仍未把显式算术入口等同自动激活/完整现行OP执行或正常报价总费；后续补真实升级首块及后继的版本选择、普通来源投影/重放与链费上下文，再继续完整净扣款/可信估值/净评分和其余优先级。

<a id="plan-2026-11-18-op-packed-source-inputs"></a>

## OP现代费用版本来源与升级首块重放（2026-11-18，阶段137，P0-3）

依设计§5.1/7.3/13.1/14.2/15.3，普通native读取和原证据重放在同header/body/receipts承诺之后派生可选`OPPackedFeeInputs`。仅已准入OP/Base的固定Ecotone/Fjord/Isthmus/Jovian schedule，复核native语义ID、首L1-info deposit身份/原属性ABI/sourceHash/L1时间，user原wire/hash/index与链ID/实际签名恢复，并从承诺cumulativeGasUsed差分提取gas。Go仅归档来源事实与版本标签，费用金额继续由Rust计算；RPC额外l1Fee等不进入生产来源投影。旧Regolith字段保留历史范围，新字段省略不借零。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F137.1|正常来源现代版本/完整签名交易及Gas投影|WP05,WP13,WP15|LOCAL_SCOPE_VERIFIED|当日50交易和8实际升级区块，deposit身份/ABI/sourceHash、签名与累计Gas；验证记录（`integration/artifacts/local/op-packed-source-inputs-check/report.json`，已清理）。|
|F137.2|升级首块/归档重算与缺目标创建交易|WP03,WP05,WP15,WP18|LOCAL_SCOPE_VERIFIED|旧ABI首块deposits-only/正常后继、60投影篡改、旧归档及15语义预算反例、缺省to创建deposit和普通call目标删除拒绝；验证记录（`integration/artifacts/local/op-packed-source-inputs-check/report.json`，已清理）。|
|F137.3|原参考/正常来源到Rust联验与回归|WP01,WP03,WP13,WP15,WP18|LOCAL_SCOPE_VERIFIED|128升级后继及49当日原signed用户对固定原函数，Go全EVM/正常服务race、旧Regolith/仿真/API/准备回归；验证记录（`integration/artifacts/local/op-packed-source-inputs-check/report.json`，已清理）。|

四升级首块Ecotone/Fjord/Isthmus/Jovian与各下一个块的原完整JSON/逐笔receipt及读后锚点已采集：首Ecotone仍260字节Bedrock、首Isthmus仍164字节Ecotone、首Jovian仍176字节Isthmus，Fjord继续Ecotone格式但费用公式改变。四首块只有deposit/upgrade，投影明确upgrade且user为空；若有user则拒绝，未在缺参数/升级状态尚未建立时猜零。实际8块共158交易、128用户，后继格式/版本分别独立核对。采集目录`integration/.runtime/design-review/op-upgrade-captures/c391bf7c033944b5af1269643ec9479d`，manifest摘要`994b73a40295ba29885e0bb77741547d7b382912b86568c6f18e78d79bcf9514`；初次TLS超时失败目录`320460c8d552448cb5846d466ad8321e`保留，有界重试只补缺块完成。采集状态仍CAPTURED_NOT_VERIFIED，只有本轮正常Go核验授予RPC承诺范围。

预检发现原normal JSON body编码要求所有deposit存在to，真实升级创建合约deposit会省略to，四首块被拒绝。对照固定原op-geth `transaction_marshalling.go`（SHA256 `2bd82ecba3cd08c3edf444e8436d54480bca062241a1030d8c6112aa5053a3b6`）的nil目标解码，允许创建交易缺省/显式null均编码空目标；交易哈希/body root仍必须匹配，删掉实际call目标拒绝，未合成缺失响应或重算新根掩盖问题。测试进一步拒绝四首块user和普通后继unsigned user，旧normal路径完整预算保持。

历史归档保存可选新投影；重放从原header/body/receipt重新派生，60次规则/格式/属性/Gas/sender/wire/index/root/deposit/authority/activation/upgrade/header/time篡改拒绝。旧记录缺字段兼容原事实；原字段不是可信价格、共识/L1最终性证明或完整OP VM。128后继原signed users交Rust显式内核，对固定原Go函数和留存非trie费用观察一致；Ecotone未激活operator保持NotActive，缺参数不调用operator估算。当前49原signed用户沿同生产投影继续回归。工具：[validate_op_packed_source_inputs.py](integration/tools/acceptance/validate_op_packed_source_inputs.py)，报告：op-packed-source-inputs-check/report.json（`integration/artifacts/local/op-packed-source-inputs-check/report.json`，已清理）。完整正常报价链费/付款人净扣款、可信估值/候选净排序、独立链执行和生产VERIFIED仍待继续，原26工作包未关闭。

阶段137最终限定验收：运行目录`5b702f530c124f618ee95f6ba8369435`，19门禁全部exit0、153次Rust及214次Go（208全EVM race与6正常service race）通过。八真实升级块158交易/128后继原signed用户、四首块deposit-only及当日49用户经普通native/原证据重放交Rust对固定原op-geth；60投影篡改、15语义预算/取消、四首块user及后继unsigned、创建deposit缺省/null目标一致与普通call删除目标拒绝全部通过。旧Regolith来源、仿真/API/准备、Go vet/build、普通/优化各参考守卫、全目标严格Clippy及正常router构建通过。源码`a00a3481d1a020f68084629e85f0f84828c8bb2cfdf2e151cf08c1bb2dac3cde`、911固定输入、45运行pins与148参考输入前后一致，五份来源/向量产物及全部日志摘要独立核对。仅完成固定版本的来源与跨语言费用算术，不授予共识/L1最终性、完整当前OP执行或报价总费资格；下一轮继续P0-3的完整付款人链费/最大资金与原客户端状态转换对照，然后推进可信估值、净评分及其余优先级。

<a id="plan-2026-11-18-op-chain-cost"></a>

## OP完整费用账户与原状态转换核验（2026-11-18，阶段138，P0-3）

按设计§5.1/7.3/13.1/14.2/15.3继续补完整链费的付款人核算。Rust新增`op_chain_cost`，复用已独立核验的Regolith/packed原属性和完整signed wire内核，显式选择Ecotone/Fjord/Isthmus/Jovian正常user规则并严格绑定ABI；不支持的首块、原生value转账和缺费参数保持关闭。费用包含execution、L1 data和operator，分别返回gross实际扣款、effective价预付、fee cap最大资金、execution/operator退款，宽中间值和uint256最终总和均检查。OP base fee记入base fee vault，不能套Ethereum burn；coinbase/三个vault按地址聚合后核对源before/独立after，付款人自收priority减入净扣，不能凭小净额绕过最大资金。该组件不认证签名/来源或交易中任意native流，调用方须先绑定原交易和零value fee-only执行范围。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F138.1|完整OP费用、预付/最大资金及退款|WP13,WP15|LOCAL_SCOPE_VERIFIED|五规则、Legacy/type1/type2和宽金额，最终总和溢出/规则/ABI/native value/预算拒绝；验证记录（`integration/artifacts/local/op-chain-cost-check/report.json`，已清理）。|
|F138.2|费用账户聚合和付款人原生净扣款|WP13,WP15,WP16|LOCAL_SCOPE_VERIFIED|分离/payer/base/L1/operator coinbase五别名，源before与独立after精确地址覆盖、资金门限、缺失/额外/篡改和burn替换拒绝；验证记录（`integration/artifacts/local/op-chain-cost-check/report.json`，已清理）。|
|F138.3|完整原客户端状态转换和初始状态一致性|WP01,WP13,WP15,WP18|LOCAL_SCOPE_VERIFIED|固定原op-geth完整源码无改动执行core.ApplyMessage；320本地签名场景/955实际状态转换、交易前committed/current storage一致，正常Ethereum worker/API回归；验证记录（`integration/artifacts/local/op-chain-cost-check/report.json`，已清理）。|

独立参考固定原op-geth commit `79e9d244748a1201fe686ca8a882b5fe7e173348`，codeload归档SHA256 `baee7f36d27a3074e0e601a32618183ce88e4f8f17a86bd23c1b78fef08aa492`，逐文件对照归档核验完整解包源码，所有原Go文件及go.mod/go.sum在统一验收前后固定摘要。只新增外部场景/I/O wrapper，不抽取重写状态转换；通过原`TransactionToMessage`恢复真实本地签名者，nonce与transaction检查均启用；原`NewL1CostFunc/NewOperatorCostFunc`从初始化本地L1Block storage取参数，原VM执行成功、SSTORE清除退款、状态写入后REVERT及INVALID耗尽Gas。五费用规则×三交易类型×四结果×五coinbase地址，另20个零费/精确base/未clip/wide场景，共320；每例富余额/精确最大资金均实际执行，315例不足1拒绝且费用账户/nonce未变，共955状态转换。该bank是明确构造的本地费用隔离场景，不是历史公链bank或真实协议成交。

原wrapper预检首遇旧GasPool API编译拒绝，修正为固定客户端的NewGasPool；手工Rust检查曾因PATH使用旧rustc而拒绝，现明确使用1.90工具链。进一步独立REVM对照发现原wrapper在设置初始storage后直接执行，GetCommittedState仍返回旧零值，构成交易内dirty write而非已完成的交易前状态，导致退款场景失真。已在安装完整本地初始状态后调用原Finalise(false)，并硬检查GetCommittedState与GetState一致、refund计数为零；不会改原客户端函数。此前基于该不一致参考提出的计费Gas下界修改和API/整数正例全部撤回，共享native函数仍按原有准入行为，未把错误参考结果推进产品。工具：[validate_op_chain_cost.py](integration/tools/acceptance/validate_op_chain_cost.py)，报告：op-chain-cost-check/report.json（`integration/artifacts/local/op-chain-cost-check/report.json`，已清理）。正常报价仍披露LOCAL_EVM_ONLY/未知L1项；该轮不升级完整当前OP执行/历史bank、可信Token估值/全候选净排序或生产VERIFIED，后续继续P0-3的正常来源链费接线与其余优先级。

阶段138前两次统一验收记录保留：首轮`03255dea7f5540f9b7f4251b7f3721e7`为FAILED，仿真100次Rust通过、API旧下界测试19通过/1失败；第二轮`d1115457ebfe4f1e8ea80e4e172a8399`已有局部门禁和Legacy HTTP通过，但进一步原Go/独立REVM对照揭示初始storage未完成的参考错误，主动SIGTERM取消，总控exit130、子进程确认回收后才修改源码。先前“57个真实退款场景证明可低于21000”结论撤回；该值来自不一致的原始slot上下文，不能作为合法已提交初始状态依据。撤回相关共享函数/API改动，修正外部场景初始化并加硬断言，再完整对照两个独立内核及重新冻结验收。旧报告/原始参考和REVM差异记录保留，不升级本轮状态。

修正初始状态后的预检：320原Go签名场景/955状态转换和Rust三项测试通过；独立REVM19.7明确OP handler对同一committed初始状态执行256个Regolith/Ecotone/Fjord/Isthmus场景，逐Gas、费用账户、付款人nonce、成功/回退及目标storage全部一致，Jovian64场景显式不支持且只由原op-geth核验。原Go当前最低计费Gas21056，没有低于21000的已提交场景。跨内核参考单独固定Cargo.lock/来源/二进制摘要并加入本轮统一门禁，不接入产品OP执行入口；错误参考及其派生修改没有提交。

阶段138最终限定验收：运行目录`9a33d71e7fe4404290e5823f5f38393d`，24门禁全部exit0、151次Rust（仿真100/API20/准备23/OP费用3/native费用5）通过；原op-geth320本地签名场景955状态转换、独立REVM256历史版本场景、普通/优化各原参考4守卫及native3守卫、56独立整数向量与8正常HTTP原worker（Legacy4/EIP1559 4）回归通过。完整初始状态Finalise后逐Gas、费用账户、nonce及目标storage与两客户端一致；Jovian64仅原Go范围，独立REVM未准入。gross/预付/最大资金、execution/operator退款、coinbase/vault别名及4000余源before/执行after/账户覆盖/burn篡改拒绝通过，资金不足1的315次原状态转换不变nonce/费用账户。源码`bdbe16e54bad32b774a63efc6b2eae66ccb7e8cc08303839af9ffa94a3f59148`、911固定输入、45运行pins与1419参考输入前后一致，五份参考/重放/跨内核/HTTP产物及全部日志摘要复核。前两次FAILED/CANCELLED、未完成初始storage的错误参考与差异记录均保留，错误低Gas结论和未提交的共享费用/API修改已撤回；取消的第二轮还发现脱离原进程组的本地HTTP harness，已按已知运行路径终止并确认退出，清理记录留存。本轮仅完成零value fee-only费用核算组件与独立参考，不认证任意native流/签名来源，也未接入完整当前OP VM或正常报价总链费、可信Token估值/候选净排序。下一轮继续P0-3的完整签名wire与OP执行上下文绑定和正常来源接线，随后按既定优先级推进。


<a id="plan-2026-11-18-op-signed-execution"></a>

## OP签名交易绑定与严格历史执行（2026-11-18，阶段139，P0-3）

设计§5.1/7.3/13.1/14.2/15.3及WP13/15/16/18要求实际链规则、完整费用和同笔交易绑定。阶段138仅有费用算术与外部参考，产品VM仍只有Shanghai；该轮新增Rust内部历史OP执行入口，先关闭“VM参数与L1计费wire可独立替换”缺口，不把本地历史执行当成当前链来源/正常报价资格。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F139.1|完整签名user envelope认证和私有VM环境|WP13,WP15|LOCAL_SCOPE_VERIFIED|EIP155 Legacy/type1/type2严格RLP、链/交易hash/caller、lowS/parity及字段宽度/访问列表总预算，签名wire与实际执行字段不可分离；验证记录（`integration/artifacts/local/op-execution-check/report.json`，已清理）|
|F139.2|七个历史OP规则和严格提供状态执行|WP15,WP18|LOCAL_SCOPE_VERIFIED|OP/Base固定Regolith/Canyon/Ecotone/Fjord/Granite/Holocene/Isthmus，完整同银行L1属性和费用slot匹配，未知账户/code/storage拒绝；不开放Jovian及之后；验证记录（`integration/artifacts/local/op-execution-check/report.json`，已清理）|
|F139.3|实际费用账户、最大资金和资源守卫|WP13,WP15,WP16|LOCAL_SCOPE_VERIFIED|执行前checked最大资金、执行后原生账户守恒与付款人净扣款、实际Gas/nonce/storage；读/字节/指令/内存/取消/期限fatal，无可用失败delta；验证记录（`integration/artifacts/local/op-execution-check/report.json`，已清理）|
|F139.4|完整原客户端与归档公链签名差分|WP01,WP15,WP18,WP25|LOCAL_SCOPE_VERIFIED|448本地原signed场景1337原转换、896产品执行与441资金不足、180归档公链用户认证；冻结全门禁与原Shanghai HTTP回归通过后才收口；验证记录（`integration/artifacts/local/op-execution-check/report.json`，已清理）|

实现边界：`simulation::op_transaction::AuthenticatedUser`字段私有，完整wire由认证入口构造的TxEnv保存，caller/nonce/to/value/data/gas/费用/访问列表均从同一原签名派生。完整wire最多16MiB并受更低调用方预算限制，访问列表最多4096个entry+slot；拒绝未保护legacy、deposit/blob/type4、非规范RLP、签名malleability及nonce/gas/优先费非法值。签名有效只证明该caller授权该wire，不证明来源/共识/时效。

`execute_op_user`仅零value外部Call，实际REVM使用固定历史OP handler，既有Shanghai路径显式保持mainnet handler且拒绝OP字段。复用同一StrictDB和inspector预算；L1Block账户与费用storage必须显式提供并与属性一致，最大资金在执行前共用checked费用组件检查，执行后审计实际费用角色余额，其他地址原生余额变化仍拒绝。原生本金、任意native流、blob/授权/deposit和升级首块不扩展。源block/state身份及历史bank认证仍由来源owner负责，正常quote API/worker接线继续未完成。

参考只使用完整固定原op-geth源码和新独立wrapper，初始化先Finalise(false)并硬查初始committed/current一致及refund0。七规则×三交易类型×四执行结果×五coinbase别名加宽值/零费等共448场景，OP/Base交错并含非空access list；富余额/精确最大资金和不足1合计1337原转换。预检发现测试Base Regolith时间早于其创世，准入正确拒绝后修正输入；初始测试AccountInfo默认内联空code亦被严格image拒绝，现按显式独立code安装。不放宽产品状态/时间边界。

工具：[validate_op_execution.py](integration/tools/acceptance/validate_op_execution.py)，报告：op-execution-check/report.json（`integration/artifacts/local/op-execution-check/report.json`，已清理）。归档公链180笔仅重新认证原阶段137普通来源输出的完整签名/hash/sender，不重新授予历史VM、当前Jovian/Karst或共识资格。统一验收完成后记录冻结摘要、实际通过门禁和提交结果；原26工作包完整验收仍为0，VERIFIED继续关闭，后续按P0-3补正常来源/执行接线、可信估值和净排序及其余优先级。

阶段139首次统一验收`eca269c0e9454e169bf1ee17279a90c0`因新增任务锚点遗漏历史批次入口被进度文档检查拒绝，FAILED记录保留；该次仅format/contracts通过，尚未进入Rust执行门禁。总控退出并确认无子进程后补历史行与任务表标准列名，再重新完整验收，不据预检升级状态。

阶段139最终限定验收：运行目录`9d39fff694d14521bb8a7103c302283d`，20门禁全部exit0、160次Rust（原仿真100/API20/准备23/新认证执行9/OP费用3/native费用5）通过。七历史规则OP/Base交错448本地原signed场景、1337原op-geth转换；896次产品严格执行逐Gas/费用账户/nonce/完整目标slot与原客户端一致，441次最大资金不足1在执行前拒绝，180笔归档普通来源公链用户签名/hash/sender重新认证通过。规范RLP/lowS/parity/链/调用者/字段宽度/截断/逐字节篡改/访问列表总预算、未知状态与属性slot冲突、原生本金/内部转移、读/字节/输入/Gas/指令/内存/取消/期限全部拒绝；运行中取消回收，原Shanghai主路径仍100回归/20API及8真实HTTP原worker通过。普通/优化各4原参考守卫与各17进度检查、全目标严格Clippy和正常router构建通过。源码`583909bc0aba36cda52e863c173af7f0c96ad3343ea42481eba49d2f7c93d0a3`、911固定输入、45运行pins、1429参考输入、五产物和全部日志摘要复核一致，八HTTP simulation原文件摘要独立核对，未残留验收worker。首次因历史入口遗漏的FAILED报告保留，未改验证规则。本轮只完成签名wire绑定的有限历史OP内部VM；当前Jovian/Karst、可信历史bank、正常报价/worker接线、完整Token估值与候选净排序继续未完成。下一轮优先将已认证OP交易接入现有BOUND执行计划及实际Router收付款路径，再推进正常入口与其余P0-3；原26完整工作包验收仍为0，生产VERIFIED保持关闭。


<a id="plan-2026-11-18-op-bound-router"></a>

## OP签名BOUND完整Router执行与费用（2026-11-18，阶段140，P0-3）

依设计§5.1/7.3/13.1/14.2/15.3和WP13/14/15/16/18，把阶段139私有认证完整signed user对象接入既有BOUND计划重验、固定Router/Factory/Pair代码身份、真实Token收付款观察及完整Pair后状态。ExactIn与ExactOut分别提供内部入口，nonce/Gas/费用从同一签名派生，caller/to/value/calldata必须与已重验计划一致；没有接受一份费用wire并执行另一份VM参数。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F140.1|认证签名与BOUND ExactIn/Out执行绑定|WP13,WP14,WP15|LOCAL_SCOPE_VERIFIED|完整wire/属性和全部观察累计预算，实际执行与计划字段匹配；冻结验收通过。|
|F140.2|同OP规则观察、实际Token/后状态及全部费用|WP13,WP15,WP18|LOCAL_SCOPE_VERIFIED|七历史规则、完整原Router/Pair/Token、费用vault与payer before/after、失败费用和零收付；冻结验收通过。|
|F140.3|完整原op-geth差分与旧正常入口回归|WP01,WP15,WP16,WP18|LOCAL_SCOPE_VERIFIED|448原signed Router场景、全部声明账户/code/nonce/storage和正常Shanghai HTTP；冻结验收通过。|

只读观察使用同一OP fork，保留实际caller.balance/GASPRICE/nonce，不预付或计入L1/operator费，丢弃每次状态；只有主swap提交一次费用/nonce。Ecotone及以后OP无blob，excessBlobGas固定0、BLOBBASEFEE为1，符合固定原客户端`consensus/misc/eip4844`规则；不引入调用者猜测header。OP结果单列`op_fees`，不把base fee vault解释为Ethereum burn；实际gross/最大资金/退款与payer净扣款从已审计共享费用组件返回并纳入receipt hash。旧Shanghai wire省略该字段，正常API明确拒绝未绑定OP envelope/属性来源的OP费用回执，当前worker/API仍未开放OP执行。

参考沿用完整固定原op-geth源码，wrapper只装载原完整合约bank与签名wire并调用原`core.ApplyMessage`，安装后先Finalise(false)，硬查所有声明storage的committed/current一致及refund0。七规则×16原完整V2银行×两交易类型×两模式共448本地构造场景；修改历史时间和Pair时间slot均显式同步，不能称真实历史公链bank。原EVM分别在bank副本执行balanceOf静态观察，原/产品逐Gas、输入/收款、费用和全部声明账户/code/nonce/storage对照；BOUND post-state digest与直接严格OP执行delta一致，防止把其他交易后状态冒充本计划。

开发预检曾在Ecotone后首次观察遇缺blob环境拒绝，按固定OP无blob规则补齐；新失败用例初用WETH allowance slot4，但本银行实际Uniswap ERC20 slot2，已按实际代码改正。一次手工过滤命中仅用于导出输入的ignored测试且缺导出路径，明确为测试调用错误；正式执行会跳过导出测试，所有原输入在冻结前生成。未放宽预算或原客户端，原26工作包完整验收仍0，生产VERIFIED继续关闭。

工具：[validate_op_bound.py](integration/tools/acceptance/validate_op_bound.py)，报告：op-bound-check/report.json（`integration/artifacts/local/op-bound-check/report.json`，已清理）。最终冻结限定验收已通过，摘要与实际结果见下；后续按P0-3接正常OP worker/API与来源、可信估值/净评分，再按原优先级推进其余设计范围，没有确认阻断全部后续工作的外部阻塞。

阶段140首次冻结批次`0ce573a313cc487db99c1bbd3b3e6275`在源码复核发现独立wrapper的Isthmus前序Granite/Holocene开关缺失后主动CANCELLED；总控及已知子进程组全部退出、无残留harness后才补齐参考配置。首次已通过format/contracts/普通和优化进度检查，尚未完成统一执行验收，原取消报告保留；补齐后重建448原参考并重新完整冻结，不沿用首次通过资格。

阶段140第二次冻结`e4ad379ecfd54be886ef4bf5b5accc4c`为FAILED：101项仿真和21项API实际测试全通过，但新required-test声明使用未加`local_simulation::tests::`前缀的短名称，被总控完整测试名核验拒绝。确认总控/子进程及本地worker无残留后仅修正门禁名称，保留失败报告并重新完整验收；不删除必需测试或降低通过数量。

阶段140最终限定验收：运行目录`5216827b73a840f1a06151595fd45c6f`，18门禁全部exit0、157次Rust（原仿真101/API21/准备23/BOUND3/签名执行9）通过。七历史规则×16原完整V2银行×Legacy/type2×ExactIn/Out共448原op-geth完整Router状态转换，448次BOUND和448次直接严格OP执行逐Gas、实际输入/收款、全部声明账户原生余额/code/nonce/storage及delta摘要一致；14次精确最大资金及14次少1预拒绝、原Router allowance不足回退/实际费用/零收付、14类字段/状态/预算拒绝及运行中取消/期限通过。只读观察保持实际caller.balance/GASPRICE/nonce并验证OP BLOBBASEFEE1，不重复收取网络费。原签名执行896次与441资金拒绝、180归档公链签名、8真实Shanghai HTTP worker继续通过；本轮未重新执行公链历史VM。

普通/优化各4原参考守卫与各17进度检查、仿真/API全目标严格Clippy、正常router/harness构建通过；源码`b1ebdb15c493b11ddfeaf321df68e147643d4b4a3f65f79eea0d5f41cd822b6e`、911固定输入、45运行pins及清单、1473参考输入、4产物、18日志与8 HTTP simulation原文件摘要独立核对一致，无残留验收worker。前两次CANCELLED/FAILED报告保留，没有降低测试数或修改产品预算。该轮仅完成签名历史OP BOUND完整本地Router执行；正常worker/API、可信历史bank、当前Jovian及后续VM、完整Token估值/候选净评分和其余原26工作包继续未完成，生产VERIFIED保持关闭。下一轮按P0-3补受限worker的签名OP协议与实际子进程执行，再接正常API及来源；只推送已配置的origin，实际目标为GitHub，并非GitLab。


<a id="plan-2026-11-18-op-signed-worker"></a>

## OP签名受限worker协议与实际子进程（2026-11-18，阶段141，P0-3）

依设计§5.1/14.2/14.4/15.3和WP14/15/16/18，在既有`dex-router --evm-simulation-worker`私有stdin/stdout协议上可选增加`op_user`，含完整signed wire、原交易hash和L1属性，全部严格小写hex/长度预算，worker内重新认证签名/chain/caller。所有nonce/Gas/最大费/priority均与认证环境规范hash比较，拒绝旧上下文替代签名；fork只从明确fixture标签选择，实际时间/属性slot和费用范围仍由严格执行器核验。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F141.1|现有worker OP签名可选协议与上下文认证|WP14,WP15,WP18|LOCAL_SCOPE_VERIFIED|输入/输出上限和期限复用，无第四服务，未签名旧wire兼容；冻结验证通过。|
|F141.2|实际router子进程与完整回执逐字段一致|WP15,WP16,WP18|LOCAL_SCOPE_VERIFIED|七规则×两方向×Legacy/type2×ExactIn/Out共56次真实worker，与库内完整receipt相同；冻结验证通过。|
|F141.3|畸形/模式/签名/字段与正常入口回归|WP01,WP15,WP16|LOCAL_SCOPE_VERIFIED|17种实际子进程拒绝，不能输出可用成功证据；旧Shanghai HTTP与前轮原状态回归统一冻结通过。|

OP和prepared模式暂不混用，ExactIn/Out严格唯一；worker先检查expected genesis/protocol/timeout，签名字段认证通过后才选择BOUND执行，不根据调用者传入fee JSON授予权威。stdout仍只有有界response，actual binary SHA256随响应并在父进程核验；旧无`op_user`序列化省略字段，原正常HTTP生产调用显式传None，正常API尚未开放OP来源、费用和签名请求。

测试从已构建router复制字节一致的实际二进制执行，签名不会被发送到链；56成功子进程比较整个结果，17反例覆盖nonce/Gas/fee/priority/hash/wire截断/尾随/属性/不支持fork/genesis/未知字段/缺OP/双模式/预算/过长属性/chain/caller。依然是本地构造历史bank，不提供当前Jovian及后续或共识/生产VERIFIED资格；原26工作包完整验收仍0。工具：[validate_op_worker.py](integration/tools/acceptance/validate_op_worker.py)，报告：op-worker-check/report.json（`integration/artifacts/local/op-worker-check/report.json`，已清理）。该轮完成后继续正常API/来源接线、可信估值/候选净评分与其余原优先级。

阶段141最终限定验收：运行目录`cd7284662bdd4ed2870c02c17c7bc95c`，19门禁全部exit0、159次Rust（仿真101/API21/准备23/BOUND3/实际worker2/签名执行9）通过。56次实际`dex-router --evm-simulation-worker`覆盖七历史规则、两方向、Legacy/type2和ExactIn/Out，整个SimulationOutcome与库内执行完全一致，worker自行报告的实际二进制SHA256逐次相同；17种签名/context/模式/预算替换拒绝且无可用成功outcome。旧OP原Router448参考/448 BOUND与448直接执行、14精确最大资金和14不足1、896原签名产品执行及441资金拒绝和180归档用户签名继续通过；8次真实Shanghai HTTP（Legacy/动态价各四）保持旧wire和费用披露。

普通/优化各4参考守卫及各17进度、仿真/API全目标严格Clippy、正常router/harness构建通过；源码`0a5e7d4a66d78abee2e989987fe3c1185c88951d27a8103309a8d2f15d68f89b`、911固定输入、45运行pins及清单、1475参考输入、4产物、19日志及8 HTTP simulation原文件摘要独立核对一致。实际router二进制`b5df0a971de850caaf63ed9d3cea4a8e7ae7bef57ecbfedd8a0f978ed5fb70b8`也在整个冻结区间保持一致，无残留worker/harness。原阶段140验收工具明确跳过后来追加的实际worker测试，阶段141由独立必需门禁执行二者，不要求旧入口提供新的测试配置。生产VERIFIED和原26完整工作包验收仍不升级，当前OP VM/可信历史bank及正常OP API继续未完成。

下一轮实现建议（保持P0-3）：正常`/build`目前仍返回EXECUTION_ADAPTER_UNSUPPORTED，故不能只为`/simulate`添加signed wire字段；需先提供明确本地历史范围的BOUND待签名交易及source/fee上下文，再接签名模拟请求。父进程必须独立重验wire/交易hash/chain/caller、同一bundle属性和费用账户源before，按共享checked组件核对L1/operator/最大资金/实际净扣，不能直接采信worker费用JSON。ExactIn/Out均复用现有quote过期/view/epoch/计划再检查及进程资源回收；旧无OP字段和未签名Shanghai入口继续兼容。随后推进当前链规则、可信Token估值/净评分及剩余优先级；源bank、共识和生产资格不足继续明确披露，不将本地历史接口当成当前公链交易准备。


<a id="plan-2026-11-18-op-fee-measurements"></a>

## OP费用回执的独立源核验（2026-11-18，阶段142，P0-3）

按设计§14.2、§14.4及§15.3继续正常OP接口的必要前置工作。将上一轮建议拆为可独立复核的提交：先提供保留签名请求/源银行的费用核验组件，后续再开放限定历史本地`/build`和签名`/simulate`闭环。正常OP API当前仍未接线，不以只增加signed wire字段代替完整待签名交易。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F142.1|保留签名及严格源银行的完整费用回执核验|WP13,WP15,WP16|LOCAL_SCOPE_VERIFIED|896参考矩阵，独立核验非VM证明；统一冻结验收通过。|
|F142.2|费用/source替换拒绝与遥测/预算控制|WP15,WP18|LOCAL_SCOPE_VERIFIED|至少650反例，精确读/字节边界、取消与期限；统一冻结验收通过。|
|F142.3|原Router/签名执行与正常Shanghai HTTP回归|WP01,WP15,WP16|LOCAL_SCOPE_VERIFIED|原参考重放及原Router/签名执行、8正常HTTP回归通过，完整范围仍不升级。|

`audit_op_fee_measurements`不执行VM。调用者保留独立认证的交易、block和源image，以原签名wire/属性/有效Gas重算execution、base vault、priority、L1 data、operator、预付/最大资金与退款；重新从有界StrictDB核验L1Block费率slot、所有费用角色原账户、完整最大资金和nonce。只接受精确的费用账户集合与源before，按地址合并角色并核验实际after、gross/self-credit/net。未知字段、非规范地址/金额、交易hash/type替换、任一费用子项或源余额漂移均拒绝；不根据worker给出的费用JSON生成可信金额。

新组件返回独立读遥测，失败保留已消耗读数，预先取消/期限在读取前拒绝，最后再次检查控制；上层接入时必须与worker等已有消耗合并预算。报告借用字段并限制最多五个费用账户，不克隆任意嵌套worker JSON。它证明费用测量与保留输入的一致性，不证明报告的Gas或afterstate由VM产生；实际VM来源仍需既有固定worker身份与完整BOUND执行证据，不提升源银行真实性、当前Jovian及后续VM或生产VERIFIED。

验证以阶段139固定原op-geth完整签名/状态转换参考及阶段141完整Router参考为基准，896组七历史规则/成功、退款、回退、HALT/费用角色重合/实际与精确最大资金，逐项独立源核验；另覆盖费用和payer字段增删改、账户前后及平移、源账户/slot缺失、nonce/资金/属性/时间/签名替换、精确与不足读字节预算、取消/期限。首次预检因参考相对路径、跨链替换探针未先选同链和新增反例计数不足被拒绝，已修正探针来源并补齐逐费用角色缺失及源余额篡改覆盖；未降低产品预算或已声明覆盖数量。

工具：[validate_op_fee_measurements.py](integration/tools/acceptance/validate_op_fee_measurements.py)，报告：op-fee-measurements-check/report.json（`integration/artifacts/local/op-fee-measurements-check/report.json`，已清理）。统一冻结验收已通过，结果见下；后续按P0-3完成限定历史正常build/签名simulate及父进程接线，再推进当前链规则、可信Token估值/净评分与其余原优先级。原26完整工作包验收仍为0，无阻断全部工作的外部阻塞；只推送实际配置的GitHub origin。

阶段142首批冻结`8a2a83c35bdc4b1b9cf4019ee2ec6127`为FAILED：format/contracts通过，进度检查发现新增任务锚点漏写既有规范要求的任务表。确认总控/子进程及harness已退出后补齐Markdown任务表和批次顺序，保留失败报告；不改源码、不放宽验证规则，重新全量冻结验收。

阶段142最终限定验收：运行目录`e7b46c8919bd487c97254bdc5d91b37a`，17门禁全部exit0、160次Rust（仿真101/API21/准备23/BOUND3/签名与新增源核验12）通过。896次实际与精确最大资金源费用核验覆盖七规则、全部原成功/退款/回退/HALT及角色重合；至少650回执/源/签名反例和精确/不足读字节预算、取消/期限守卫通过，失败遥测保留。原448 Router转换重放、448 BOUND与448直接执行、原896签名执行/441资金拒绝及180归档签名继续通过；8正常Shanghai HTTP worker（Legacy/动态价各四）不变。

普通/优化各4参考守卫及各17进度、仿真/API全目标严格Clippy、正常router/harness构建通过；源码`1c0d41d6022041cba990f0488eb7d6965aa7f4be8877720bc46c907213ae6f9c`、911固定输入、46运行输入（45依赖及清单）、1474参考输入、3产物、17日志与8 HTTP原simulation摘要独立复核一致，无残留worker/harness。首批任务表缺失FAILED报告保留。本轮完成可复用的费用源一致性组件，未以费用数学核验充当VM证明；正常OP build/签名simulate接线、当前规则/可信bank、可信Token估值/全候选净评分及其余原工作包继续，生产VERIFIED与原26完整验收仍不升级。下一轮继续按P0-3把该组件接入正常API，实际推送目标为GitHub origin。


<a id="plan-2026-11-18-op-normal-api"></a>

## 正常历史OP构建与签名模拟（2026-11-18，阶段143，P0-3）

依设计§5.1/14.2/14.4/15.3，在原三服务和正常API/同二进制worker内继续接阶段142核验组件。配置`revm-fixture`的本地bundle可选固定`op_attributes_calldata`，旧无字段省略序列化保持兼容；启动验证明确历史规则与属性格式，不从请求接受属性或费用JSON。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F143.1|正常build构造完整本地历史OP待签名交易|WP14,WP16|LOCAL_SCOPE_VERIFIED|ExactIn/Out重新BOUND、完整RPC hex数量/nonce/Gas/费用上下文、源H/H+1与build hash，冻结19门禁及161次Rust通过。|
|F143.2|正常签名simulate、父进程回执及费用源核验|WP13,WP15,WP16,WP18|LOCAL_SCOPE_VERIFIED|完整wire认证与报价/源/receipt重新绑定，独立重算全链费和合并读字节预算，冻结19门禁及161次Rust通过。|
|F143.3|正常HTTP真实签名与原客户端执行对照|WP01,WP15,WP16,WP18|LOCAL_SCOPE_VERIFIED|七规则×Legacy/type2×ExactIn/Out的28原HTTP签名/执行及168反例，旧8正常Shanghai回归，冻结19门禁及161次Rust通过。|

正常`POST /v1/quotes/{id}/build`仅在已配置该历史OP源时返回可供外部钱包签署的`rpcTransaction`，完整from/to/value/data/chainId/nonce/Gas及Legacy或type2费用。source anchor、独立execution environment、bundle/binding/plan/view/build hash均明确；build前后及缓存读取锁内重新验证过期/视图/计划。签名未知前不猜L1字节费和完整最大资金，`ready=false`、`AWAITING_SIGNED_ENVELOPE`和阻断原因明确，仅用于`LOCAL_HISTORICAL_OP_SIGNED_SIMULATION`。该模板不承诺当前公链可发送，不广播。

正常`/simulate`可选body形如`{"signedOpTransaction":{"transactionHash":"0x…","signedWire":"0x…"}}`。所有VM nonce/Gas/费用/chain从完整签名派生，caller/to/value/data必须与当前缓存报价重绑的BOUND一致；拒绝旧计划签名、hash/截断、未知属性/费用覆盖、未签名OP及把OP签名交给未配置源的入口。原未签名Shanghai空body/{}及prepared路径保持原范围，OP和prepared禁止混用。无效签名/绑定先于昂贵可执行文件SHA校验拒绝；任何有效worker启动仍完整核验原二进制身份，不跳过摘要。

父进程保留独立签名、BOUND、源fixture/上下文/limits，重新核验receipt hash、源摘要、nonce与OP版本；对同一源image核验费用slot、before、最大资金、全部费用账户after及payer净扣。父进程额外读取使用worker已消费后的剩余额度，成功公开aggregate usage，原信号取消/期限、permit所有权、进程回收及发布时过期/view再检查保留。费用响应以baseFeeToVault拆分execution/L1/operator，gross、self-credit/net、最大资金、预付和退款独立；额外审批资金操作、可靠Token估值和报价经济净额仍null，不声称完整报价经济成本或生产VERIFIED。费用/receipt一致性不是VM数学证明，真实执行来源仍为固定身份的实际worker。

预检父进程28场景/252次重新seal的回执篡改通过。首个HTTP预演触发原15秒期限，完整保留原进程日志；停止并确认无残留后单独复现，实际ExactIn/Out分别12.659/12.920秒，均在原期限内完成，原客户端分别核对新HTTP calldata的Gas/实际Token/费用。采样确认主要阻塞线程在debug可执行文件SHA256，首次超时与并发Clippy同现，但未据此宣称唯一因果；产品期限与覆盖均未降低。第二预演的ExactOut比较脚本误读省略的None `bound`键而失败，保留原API结果并用独立原转换恢复比较通过，随后修正脚本按可选字段读取；统一冻结仍从头运行，不沿用预演通过资格。

工具：[validate_op_api.py](integration/tools/acceptance/validate_op_api.py)、[op_api_http.py](integration/tools/acceptance/op_api_http.py)，报告：op-api-check/report.json（`integration/artifacts/local/op-api-check/report.json`，已清理）。本地固定公知key1仅存在验收钱包wrapper，实际签署正常build当时返回的calldata，正常服务不持有私钥。独立原完整op-geth在装载初态后Finalise(false)，逐slot确认committed/current一致及refund0，再执行同一完整signed wire；不改原客户端。当前Jovian及后续VM、可信历史bank/共识、通用可信估值/净评分及剩余工作包继续；原26完整验收与生产资格不升级，无阻断全部工作的外部阻塞。验证结束后本轮commit并推送实际GitHub origin，再按优先级继续。

阶段143最终限定验收：运行目录`bc3c76cc99814c6bbda10ebedfc825f2`，19门禁全部exit0、161次Rust（仿真101/API21/准备23/BOUND及父进程回执4/签名执行及费用源12）通过。七历史规则×Legacy/type2×ExactIn/Out共28笔正常build实际签名/HTTP执行与原完整op-geth逐Gas、实际Token和费用账户一致；168错误签名/旧计划/未知覆盖拒绝及28次build未授权拒绝通过，28个父进程回执场景及252次重新seal篡改拒绝、旧8笔正常Shanghai实际worker全部通过。全目标严格Clippy、正常router/harness构建、普通/优化各17进度及各4原参考守卫通过。源码`08d04e6024c144788e6c80c1d370bdcad4e8e3c3e86e93ba276e8f213080fd29`、911固定输入、46运行输入、1488参考输入、4产物、19日志和28 OP场景224原JSON文件及8旧HTTP原simulation摘要独立复核一致，无残留worker/harness。

下一轮优先修复已发现的规则完整性缺口（P0-3/P0-4、§14.2）：当前固定REVM构建未启用BLS/KZG特性，Isthmus BLS输入上限还需独立适配；本轮28个原Router场景不调用这些预编译，不能据此外推完整fork支持。先启用对应加密后端并按固定原客户端逐输入/Gas/错误/升级前后分派差分，补Isthmus三项输入上限，再继续Jovian operator fee单位/四预编译上限与当前可信bank、可信估值/全候选净评分。原26工作包完整验收仍0、生产VERIFIED关闭；仅推送实际配置的GitHub origin，不冒称GitLab，无阻断全部可开发工作的真实外部阻塞。

<a id="plan-2026-11-18-op-precompiles"></a>

## OP预编译加密后端与Isthmus限制（2026-11-18，阶段144，P0-3/P0-4）

设计映射：§14.2每链真实执行规则、§14.4资源边界、§28.17独立VM差分，WP03/WP15/WP16/WP18。阶段143原Router场景不调用BLS/KZG，不能据此认定该fork所有执行规则已完整支持。本轮修复固定构建的缺失加密后端及OP专用输入上限，维持旧历史时间区间；Jovian及后续VM与可信bank不在本轮准入。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F144.1|启用KZG/BLS及OP预编译分派|WP03,WP15|LOCAL_SCOPE_VERIFIED|固定REVM19.7开启已有锁版本c-kzg/blst；Ecotone含KZG、Fjord含P256、Isthmus含七BLS，升级前保持普通账户语义；24门禁统一冻结验收通过。|
|F144.2|Isthmus限制与Gas检查优先级|WP15,WP18|LOCAL_SCOPE_VERIFIED|按固定原op-geth的三BLS上限和BN Granite上限，先RequiredGas再输入校验；共享原加密实现，成功Gas/返回字节逐项对照；24门禁统一冻结验收通过。|
|F144.3|原语料与完整签名交易差分|WP03,WP15,WP16,WP18|LOCAL_SCOPE_VERIFIED|3558直接输入、110实际签名/完整原状态转换；覆盖七规则分派、输入/Gas/上限，复跑原BOUND/正常OP与Shanghai HTTP；24门禁统一冻结验收通过。|

实际适配同时用于swap执行和只读观察，source/输入/指令/Gas/取消/期限与worker回收预算保持。BLS上限513760/488448/235008字节分别绑定G1MSM/G2MSM/pairing；BN上限112687字节。上限先于原生加密调用检查，Gas少于原RequiredGas时先报OutOfGas。共享BLS的折扣表直接读取锁定依赖，不另造密码学运算或更改原客户端。KZG明确启用实际后端，避免只靠构建成功判断能力。原18地址注册表、成功/错误/低Gas与cap边界均纳入差分。

原始语料来自完整归档已验证的op-geth core/vm/testdata，原数据继续留本地。新wrapper直接执行未修改的原预编译和core.ApplyMessage，并复用既有wrapper的配置/初态辅助函数；只重命名其入口，原客户端不修改。完整交易在初态Finalise(false)后执行，实际费用账户、nonce、Gas及返回字节由两引擎对照。首个原wrapper预演因Isthmus低Gas交易未满足EIP7623 calldata floor被原客户端拒绝，失败输入/日志保留；第二预演按原core.FloorDataGas构造合法总Gas，直接调用和完整交易各自比较，不假定floor产生的额外可用Gas与直接调用相等，未修改产品规则或放宽预算。

工具：[validate_op_precompiles.py](integration/tools/acceptance/validate_op_precompiles.py)、[op_precompile_vectors.py](integration/tools/acceptance/op_precompile_vectors.py)，报告：op-precompiles-check/report.json（`integration/artifacts/local/op-precompiles-check/report.json`，已清理）。原26完整工作包验收仍0、生产VERIFIED关闭；本轮成功仅证明列明预编译范围，继续当前VM/可信bank、可靠Token估值和全候选净评分。每轮验证通过后commit并推送实际GitHub origin，再按既定优先级继续。

阶段144文档预检发现新任务表列名未使用既有解析器约定，已改回“任务/内容/工作包/状态/验证”，不修改进度校验器；当时聚合报告尚未创建，报告链接检查随正式总控写入RUNNING报告后执行，未伪造通过记录。

阶段144严格Clippy预检拒绝捕获非Send/Sync原handler的Arc闭包；已改为按固定fork直接构造原注册表并施加上限，闭包仅捕获Copy规则枚举，不添加lint豁免。3558原语料（含OutOfGas优先级）、110签名转换及普通/优化各3验收守卫预检通过，最终资格仍以同一源码的统一冻结验收为准。

阶段144首轮冻结`360c6062766e460491ca1a9159e8a0fa`在前19门禁通过后，第15笔正常OP HTTP触发15秒supervisor期限，报告FAILED及全部原HTTP/日志保留；未计为通过。总控已退出，独立确认无worker/harness残留后才修改源码。使用失败组保留的source WorkerRequest单独执行成功（该请求不是正常HTTP新build后的签名），耗时8.292秒，完整111099512字节可执行文件SHA与Python独立摘要一致；macOS原生采样455主线程样本中452位于executable_sha256，451位于SHA压缩路径，数据保留在`integration/.runtime/design-review/op-precompile-hash-profile`。因此仅对固定sha2依赖的dev/test编译开启opt-level=3；算法、每次完整文件读取、父/子独立身份核验及原15秒期限不变，不引入摘要缓存或跳过核验。重建后先隔离复验，再从头冻结全部门禁，首轮通过部分不沿用最终资格。

优化后同一保留source worker请求首次单独执行2.438秒，完整outcome与优化前一致，二进制摘要继续与Python独立计算一致；原失败Fjord/type2组重新由正常build签名，ExactIn/Out实际HTTP分别1.281/1.250秒、12错误请求拒绝，费用/Gas/实际Token继续与原完整客户端一致。预演只用于定位与修复验证，随后重新冻结全套3558输入、110转换和28正常HTTP，不以两个快速请求替代完整验收。

阶段144最终限定验收：运行目录`63246ced12134db28a894faa253b2c6d`，24门禁全部exit0、165次Rust通过；3558原预编译输入逐Gas/返回字节/成功与OutOfGas/规则分派一致，110完整原签名状态转换逐Gas、nonce、返回字节及全部声明费用账户一致。28笔正常OP HTTP（最长1.874秒）、168错误请求及28次build未授权拒绝通过，原8笔正常Shanghai HTTP实际worker回归通过；原BOUND/父进程回执与费用源审计、普通/优化各3新语料守卫及各4原BOUND守卫、全目标严格Clippy、正常router/harness构建通过。源码`2232a53da97a995cd28bc183e1019921db30a96914a12ff94e4e177e5a33873b`、911固定输入、46运行输入、1521参考输入、5产物、24日志、28 OP场景224原JSON与8旧HTTP simulation摘要独立复核一致，无残留worker/harness。首次FAILED报告及原始输入/采样保留并作为本次引用固定，不放宽15秒期限，不减少28笔覆盖，不跳过完整二进制身份核验。

下一轮继续P0-3/P0-4：依据原客户端内置registry明确OP Mainnet的Jovian历史区间，再验证operator费单位及四预编译输入上限，贯通BOUND/worker/父进程/正常API。固定registry提交`ac4e48516794322e20abfe4f4c175e379b9a5d19`记录OP的Karst激活为2026-07-08 16:00:01 UTC（1783526401），原op-geth自该时间起拒绝构建/导入区块，因此历史Jovian不等于当前链资格，当前Karst及后续仍需适配有效客户端与规则；不能默认Base沿用未核实的后续时间表。原26完整工作包验收仍为0、生产VERIFIED关闭，可信bank/共识、可靠Token估值/全候选净评分与其余优先级继续；实际推送目标仍为GitHub origin。

<a id="plan-2026-11-18-op-jovian"></a>

## OP Mainnet历史Jovian用户执行（2026-11-18，阶段145，P0-3/P0-4）

按设计§14.2、§14.4、§28.17继续补每链实际规则，保持现有三服务与受限worker。固定原registry的OP Mainnet Jovian激活1764691201、Karst激活1783526401；本轮仅普通零value用户Call、时间严格大于前者且小于后者，不接受Jovian升级首块、Karst及后续或未核实的Base后续时间表。历史单笔VM不能提供当前链/区块共识和可信bank资格。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F145.1|Jovian历史准入与operator实际执行单位|WP03,WP13,WP15|LOCAL_SCOPE_VERIFIED|共享checked费用核验真实源slot/最大资金；固定Isthmus底层仅私有缓存适配scalar倍率，64原签名场景/128产品执行/63资金拒绝统一冻结通过。|
|F145.2|Jovian四预编译上限与原加密差分|WP03,WP15,WP18|LOCAL_SCOPE_VERIFIED|BN/G1MSM/G2MSM/pairing上限81984/288960/278784/156672；7056原输入与160签名转换统一冻结通过。|
|F145.3|正常BOUND/worker/父进程与HTTP接线|WP13,WP15,WP16,WP18|LOCAL_SCOPE_VERIFIED|原28历史场景加4 Jovian build实际签名/仿真，父进程独立源费用核验和旧8 Shanghai回归统一冻结通过。|

原REVM19.7底层仍为Isthmus，不能仅改fork标签。本轮按原op-geth逐处核对Jovian用户执行差异：operator改为gas×raw scalar×100，四项预编译缩小输入上限；原source slot8保持不变，前置以共享checked Jovian数学校验完整最大资金，验证成功后仅内部L1BlockInfo scalar乘100000000，使固定引擎的除1000000公式得到正确扣费/退款/费用账户入账。观察模式仍不扣费、不改nonce并丢弃状态。严格178字节属性/selector、source slot绑定及全部资源预算保留。

新独立原参考覆盖64 Jovian签名场景（Legacy/type1/type2、成功/退款/回退/HALT和所有费用角色重合、零/宽整数、实际/精确最大资金及不足1）。完整原预编译语料保留阶段144的3558条，再增加3498条Jovian输入与50原签名转换，总7056/160；原Gas优先级、cap边界、全部费用账户/nonce/返回值与实际引擎比较。正常API扩展到32笔，全部由当时build返回的calldata实际签名，旧28场景覆盖不缩减；额外4笔覆盖Legacy/type2与ExactIn/Out。所有输入/回执/报告仍本地保留，不广播交易。

工具：[validate_op_jovian.py](integration/tools/acceptance/validate_op_jovian.py)、[op_jovian_vectors.py](integration/tools/acceptance/op_jovian_vectors.py)。最终统一冻结已通过，原始记录与摘要如下；预检不替代最终通过资格。当前Karst/后续客户端、可信bank/共识、可靠Token估值/全候选净评分及其余既定优先级继续，原26工作包完整验收仍0、生产VERIFIED关闭。没有阻断全部工作的真实外部阻塞；本轮通过后commit并推送实际GitHub origin，再继续下一项。

阶段145首轮冻结`c090fb6d029646c581ebbd1ff2bf3345`在format门禁停止，source为`aaa6b6437b1997f93d214a67291e9e37a160b6f9dee7d8a35cdee9f994f0d3c6`。之前格式化未使用workspace全部目标，导致新增源码仍有rustfmt差异；失败报告保留。确认总控和worker/harness均退出后执行固定Rust1.90的fmt --all，再重新完整冻结；没有放宽格式或其他验收规则。

阶段145第二次准备原客户端时，Go签名wrapper链接因宿主磁盘No space left on device失败，尚未进入新的统一门禁，不覆盖第一份FAILED报告。原输入、生成源码和build.log保留在`integration/artifacts/local/op-jovian-check/original-runs/75d5d18b2b434bdeb7af47992ae03eef`；确认全部验证/worker/harness退出后，仅清理`integration/.runtime/design-review-target/debug/incremental`可重建增量编译缓存，保留实际二进制、所有原始语料与验收报告，不将可本地解决的容量问题标作外部阻塞。

阶段145最终限定验收：运行目录`e7d9a64d4136492baef07f5e94e1237b`，25门禁全部exit0、168次Rust通过。64 Jovian原签名场景覆盖191原转换，128产品实际/精确最大资金执行及63不足1拒绝、128独立费用源审计通过；7056原预编译输入及160完整签名转换逐Gas/返回/nonce/全部声明费用账户一致，原七规则覆盖保留。32笔正常OP HTTP实际签名并与原完整客户端比较，其中新增4 Jovian；192错误请求及32次build未授权拒绝、旧8笔正常Shanghai HTTP实际worker通过，最长OP请求1.631秒，15秒期限不变。原448 Router/896产品BOUND、28父进程回执/252重新seal篡改、旧签名执行与180归档签名、普通/优化各4新守卫及各4旧守卫、全目标严格Clippy、正常router/harness构建均通过。

源码`15957df77de04f0d601d48322a3ce5f5247e263d731a9c66e91dffd362d49294`、911固定输入、46运行输入、1525参考输入、6产物、25日志及32 OP场景256原JSON与8旧HTTP simulation摘要独立复核一致，无残留worker/harness；原始FAILED报告与磁盘准备失败原日志保留。仅清理可再生成的Rust增量缓存，所有固定语料/报告/实际二进制保留，释放约9.7GB后重跑完整冻结。最终Markdown后的普通/优化进度检查另行执行，不修改非Markdown源码。

下一轮仍按P0-3/P0-4补当前Karst用户执行规则和独立原客户端差分：已核对官方当前实现含Osaka基线，差距包括MODEXP上限/Gas、CLZ、交易Gas cap、P256 Gas及BN输入上限，不能将Jovian重命名为当前规则。先固定实际原实现/registry和可重复独立参考，逐条加入真实执行与正常API；可信bank/共识、可靠Token估值/全候选净评分及其余优先级继续。原26完整工作包验收仍0、生产VERIFIED关闭，单项历史本地通过不等于所有设计功能完成。推送目标为实际GitHub origin，无GitLab远端，不将本地可处理的问题归为阻断全部工作的外部阻塞。

<a id="plan-2026-11-19-karst-precompiles"></a>

## Karst预编译组件与固定现代原参考（2026-11-19，阶段146，P0-3/P0-4）

继续设计§14.2每链实际执行规则、§14.4资源边界和§28.17独立差分。原op-geth已在Karst设kill switch，不能绕过客户端区块拒绝来宣称现行OP执行。本轮固定官方Optimism原提交`60360273aada79572fe2212497de578ad2fa249b`，完整归档SHA256 `c4fa3360916881d233fc7f933f129452043a25bbee5227e4d8e3325a8f272120`；registry固定`0ca0a93ca0aebb05c90c4dae3f1c66927f8db0af`，归档SHA256 `172c5572379683ccc0f326e7d1a804923e7977f1d0e0f084a8461574d4a508b0`。完整原文件逐一认证，实际原spec.rs/precompiles.rs不修改；在隔离参考wrapper中直接编译它们与固定REVM43.0.3，不把组件执行说成完整客户端状态转换。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F146.1|Osaka MODEXP尺寸/价格及P256新Gas|WP03,WP15,WP18|LOCAL_SCOPE_VERIFIED|三个长度字段均限1024，最低500与新复杂度/迭代规则；原模块算术复用，零输出也按实际exponent计价；P256改6900，原9452组件差分通过。|
|F146.2|Karst BN上限与现代输入/Gas顺序|WP03,WP15,WP18|LOCAL_SCOPE_VERIFIED|BN改57600；现代原OP组件cap先于Gas，旧历史op-geth顺序保持；KZG/BLS等完整表回归通过。|
|F146.3|固定原组件/依赖及历史正常链路回归|WP03,WP13,WP15,WP16,WP18|LOCAL_SCOPE_VERIFIED|Rust1.99仅用于隔离原参考，产品锁版本和1.90不变；依赖archive/全部源操作数认证，原Jovian及七历史规则/32 OP/8旧HTTP统一冻结通过。|

先完成这些可独立验证的实际预编译组件；`OpFork::Karst`只用于构造组件表，`at`仍拒绝全部Karst时间，不能让只补部分Osaka规则的VM进入正常build/simulate。CLZ、交易Gas cap、完整签名/状态转换与当前正常API留下一轮；deposit/升级首块、区块DA/共识与可信bank仍未获得资格。固定registry记录Karst为1783526401、未来Lagoon为1799683201（2027-01-11 16:00:01 UTC）；当前日期2026-11-19处于二者之间，不据未来版本推断当前规则。

MODEXP先检查原最低Gas，再解码右补零的三长度并执行EIP-7823上限；最长仅分配3072输入字节，复用固定原模幂运算。原引擎base=modulus=0提前返回最低费用的路径需要显式传入新实际价格，不能忽略非零exponent造成的Gas。P256保留原签名/点验证，成功或无效输入的价格都更新；Gas不足先拒绝。BN与BLS现代组件先执行原OP输入cap检查，之后保留原密码学/Gas，旧Jovian和之前原op-geth顺序不改。

新增9452组件输入，其中1248 MODEXP、6249 P256；包含原语料、0–96字节补齐、三个尺寸1024/1025/uint256最大值、短/长exponent、零输出价格、Python大整数pow独立算术、P256原/新Gas阈值和BN上限/Gas顺序。原modern输出与产品逐成功/Gas/字节/OutOfGas对照。新参考只是两个原模块的真实直接执行，不能替代完整原客户端VM；组件通过之后才继续补其余用户规则及完整执行。

工具：[validate_karst_precompiles.py](integration/tools/acceptance/validate_karst_precompiles.py)、[karst_precompile_vectors.py](integration/tools/acceptance/karst_precompile_vectors.py)。预检原wrapper及日志保留，最终统一冻结结果随后追加。原26工作包完整验收仍0、生产VERIFIED关闭；当前bank/共识、可靠Token估值/全候选净评分和其余优先级继续。每轮通过后commit并推送实际GitHub origin；当前无GitLab远端，不冒称GitLab推送。

阶段146第一原组件预演已成功构建原模块，但依赖认证发现本机预载Cargo缓存没有.cargo-checksum.json，未产生通过报告。保留`prototype-1`的原源码、Cargo.lock、metadata和build.log；改为从锁文件checksum认证原.crate，再逐个对照archive内原文件与实际编译目录并拒绝额外操作数，比依赖可选缓存侧文件更直接，不跳过依赖源身份验证。参考工具链和依赖隔离，未更新产品工具链或产品锁文件。

阶段146第二预演通过全部锁依赖archive/源文件认证，但运行副本因copyfile丢失可执行权限而被宿主拒绝，未计通过；`prototype-2`输入/构建及原二进制副本保留。改用copy2保留权限后，`prototype-3`的9452原组件结果与产品逐输入/Gas/输出/OutOfGas完全一致，普通4验收守卫及严格Clippy预检通过。预演不替代最终同源码统一冻结资格；当前Karst时间继续拒绝。

阶段146最终限定验收：运行目录`97bbe25ecfa34c0bb7913abed8502762`，30门禁全部exit0、171次Rust通过。9452原Karst组件（1248 MODEXP、6249 P256及其余BN/KZG/BLS）、普通/优化各4参考守卫、原组件副本完整重放通过；64 Jovian原签名场景/128产品执行/63不足1拒绝/128独立源费用审计、7056历史预编译输入/160原签名转换、32正常OP HTTP/192错误请求/32未授权build拒绝及8旧Shanghai HTTP回归全部通过。最长OP HTTP为1.390863秒，原15秒产品期限保持。

冻结源码`208d04f8ae3560a0948cd36aac4967c2af4177de3db154b64bd9ff2c226f5e6e`、911固定输入、46运行pins、12437参考操作数（含逐archive认证的原依赖和工具链）、7产物及30日志摘要独立复核一致；256 OP原JSON及8旧simulation原文件摘要重新核验，无残留worker/harness。两次无通过资格的预演均保留，未缩减语料或放宽预算。该轮只完成实际预编译组件，Karst完整签名VM/正常当前API、可信bank/共识和可靠估值/全候选净评分仍未完成；原26工作包完整验收仍0、生产VERIFIED关闭。下一轮继续既定P0-3/P0-4，补CLZ和EIP-7825交易Gas cap及完整原引擎差分后再准入Karst；推送实际配置的GitHub origin。

<a id="plan-2026-11-19-karst-execution"></a>

## Karst用户执行组件与完整原引擎（2026-11-19，阶段147，P0-3/P0-4）

继续设计§14.2实际链规则/原生节点差分、§14.4资源约束和§28.17独立交易仿真。本轮补CLZ opcode 0x1e（Gas 5）及普通用户EIP-7825交易Gas cap 2^24，使用同一固定官方Optimism提交60360273aada79572fe2212497de578ad2fa249b的完整op-revm包和固定REVM43.0.3直接执行；不再只执行两个预编译组件。完整原文件与path依赖仍逐archive认证，Rust1.99隔离参考，产品保持1.90和原Cargo.lock。阶段146两个组件范围不被改写为完整VM证据。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F147.1|CLZ、Gas cap和统一控制预算|WP03,WP15,WP18|LOCAL_SCOPE_VERIFIED|CLZ先于inspector注册，Gas cap在源读前拒绝；真实签名边界、257数值/栈不足/循环及控制守卫冻结通过。|
|F147.2|固定完整原OP VM与签名后状态|WP03,WP13,WP15,WP18|LOCAL_SCOPE_VERIFIED|468明确本地场景，完整signed wire恢复、401执行/67拒绝、费用alias/退款/回滚及全部声明后状态冻结通过。|
|F147.3|历史正常API及组件回归|WP13,WP15,WP16,WP18|LOCAL_SCOPE_VERIFIED|9452预编译、Jovian/历史签名及32正常OP/8旧HTTP同源码统一冻结通过；Karst时间准入保持关闭。|

CLZ通过有界安全栈接口实现；注册在所有指令的inspector包装之前，不能插入新opcode而绕过步数/取消/期限。交易cap仅约束实际普通交易，内部只读probe继续由既有预算限定；deposit/升级首块仍不在用户Call范围。Jovian operator私有缓存倍率适配同步扩为Karst组件，但真实L1Block slot8和独立checked资金验证不改。统一门禁中仅测试模块绕过schedule、直接进入同一私有执行内核，实际签名认证、严格源bank、费用、资源限制全运行；公开execute_op_user/正常API仍拒绝Karst。

原参考468场景由191资金/别名/退款/失败、257个CLZ结果、栈不足与循环、18笔真正重新签名的Legacy/type2 Gas边界组成。真实现代原BlockEnv只接受u64 basefee；来源矩阵中超宽basefee明确改为u64最大值并记录base_fee_remapped_to_u64，保留原signed wire的宽费用字段，不静默截断或宣称超宽原环境已验证。参考全额费用账户守恒由Python独立整数核验，成功、拒绝及完整code/nonce/storage/source before均受守卫；现代原参考不授予可信bank/区块导入/派生/共识资格。

预检完整原引擎先对4笔既有Jovian Router交易交叉核验，Gas及完整声明后状态与原op-geth一致。首次完整468预演因原handler正常清空交易L1费用缓存，wrapper直接unwrap已清空缓存而停止，无通过报告；保留prototype-1原输入/源码/日志，改为调用原calculate_tx_l1_cost重新计算，不改原handler或费用规则。prototype-2原468结果与两项产品执行/控制预检通过；测试代码一次字段误写为getter的编译拒绝保留日志，改为原公开字段。最终使用新完整守卫/元数据重建原输入，预演不替代统一冻结。

工具：[validate_karst_execution.py](integration/tools/acceptance/validate_karst_execution.py)、[karst_execution_vectors.py](integration/tools/acceptance/karst_execution_vectors.py)。本轮限定退出条件是实际Karst执行组件与完整原引擎差分及历史链路回归；下一轮才验证schedule准入、BOUND/worker/父审计和正常Karst build/签名simulate。原26完整工作包验收仍0、生产VERIFIED关闭，可信bank/共识、可靠Token估值/全候选净评分和其余优先级继续；每轮验证后commit/push到实际GitHub origin。

阶段147参考守卫首次曾对旧prototype-2运行，新守卫正确拒绝其缺少完整原提交/版本元数据，保留该失败输出；prototype-3已从头生成含显式basefee转换标记和原提交/版本的完整输入，不修改旧参考报告伪造通过。正式冻结将使用最终格式后的新运行副本。

阶段147最终限定验收：运行目录`88c7544791e8468ca649f935e55da913`，35门禁全部exit0、173次Rust通过。固定完整原OpEvm468签名场景、401产品执行/67拒绝逐Gas/输出/完整声明code/nonce/storage与费用账户一致；257 CLZ结果、18笔真实签名Gas边界、source最大资金/退款/别名/失败回滚、八档指令预算与取消/期限守卫通过。普通/优化各4完整原VM守卫、完整原副本重放、9452预编译组件及原Jovian/历史矩阵、32正常OP HTTP/192错误请求/32未授权build拒绝与8旧Shanghai HTTP回归全部通过；最长OP HTTP 1.391009秒，原15秒期限保持。

冻结源码`e9db1b64e90cf5d14619c16a3dde7b3752c92f260fc20cf1447568fcd54cd6aa`、911固定输入、46运行pins、13671参考操作数、8产物和35日志摘要独立复核一致，256 OP原JSON和8旧simulation原文件摘要再次核验，无残留worker/harness。实际metadata确认完整原op-revm20.0.0和op-alloy-consensus2.0.0来自上述固定monorepo路径，依赖REVM43.0.3/alloy-consensus2.5.0；不与早期发布的同名op-revm20.0.0（另一提交/另一REVM版本）混淆。仅3个资金子场景显式将原超宽basefee改为u64最大值，均在原输入记录，未宣称超宽环境获现代资格。

该轮补齐实际执行组件而未开放Karst schedule；全部原始预演和失败记录保留。下一轮按P0-3/P0-4验证OP Mainnet Karst具体时间准入，贯通BOUND/实际worker/父进程源费用审计和正常build/签名simulate，并用完整原引擎执行同一笔正常入口签名；可信bank/共识、可靠Token估值/全候选净评分和其余优先级继续。原26完整工作包验收仍0、生产VERIFIED关闭；验证后提交并推送实际GitHub origin，再继续开发。


<a id="plan-2026-11-19-karst-api"></a>

## Karst正常本地签名API与完整原Router（2026-11-19，阶段148，P0-3/P0-4）

继续设计§14.2实际链规则/原生节点差分、§14.4有界资源和§28.17独立交易仿真。在阶段146/147实际组件与完整原引擎基础上，开放OP Mainnet普通用户Call的严格区间1783526401 < timestamp < 1799683201；升级首块、未来Lagoon与未认证Base后续规则继续拒绝。完整原接口的basefee/block gas limit为u64，交易费字段为u128；共享准入在源读取前拒绝超宽和超2^24交易Gas，父进程独立源费用核验复用相同边界，不静默截断。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F148.1|Karst公开执行、schedule与源读前宽度/Gas准入|WP03,WP15,WP18|LOCAL_SCOPE_VERIFIED|401公开执行/401独立父费用源审计、67拒绝及控制、宽度/schedule边界统一冻结通过。|
|F148.2|真实Router收付款与完整声明后状态原引擎差分|WP03,WP13,WP15,WP18|LOCAL_SCOPE_VERIFIED|固定完整原OpEvm执行Legacy/type2与ExactIn/Out四例，普通/优化各4参考守卫、原副本重放与产品BOUND完整状态冻结通过。|
|F148.3|正常build签名/worker/父费用审计与历史回归|WP13,WP15,WP16,WP18|LOCAL_SCOPE_VERIFIED|36正常OP HTTP及8旧HTTP、216错误请求/36未授权build拒绝、全部历史和现代原参考统一冻结通过；本地资格不授予生产VERIFIED。|

原参考继续固定官方monorepo提交60360273aada79572fe2212497de578ad2fa249b的完整op-revm及逐archive认证的依赖。实际签名交易始终经正常OpEvm.transact；只读balanceOf观察使用原OpHandler.run_system_call与独立丢弃副本，硬检查原生余额不变、storage不变/无自毁、32字节输出和200000 Gas预算，保留真实caller/GASPRICE/source nonce。不得改原handler以制造参考相等。

第一预演使用原可选disable_fee_charge观察时，硬断言发现OP reward_beneficiary仍改变费用vault，因此停止且保留prototype-1，不计通过；改为原官方system call路径。第二预演真实四笔原Router已执行，但审计错误假设type2固定3gwei；改为解码原signed RLP按cap/basefee/tip计算有效价，保留prototype-2和原始输出。prototype-3通过参考审计。公开执行/父审计四项预检通过；一次BOUND预检误传Karst执行语料作为Jovian源，正确拒绝，改用原指定Jovian参考重跑。HTTP新增原引擎参数被旧CLI长度校验拒绝的问题也已修正，最终资格以完整同源码冻结为准。

工具：[validate_karst_api.py](integration/tools/acceptance/validate_karst_api.py)、[karst_api_vectors.py](integration/tools/acceptance/karst_api_vectors.py)。当前仅对指定本地bank历史源签名模拟进行资格验证，ready=false、verified=false；可信bank/区块派生/共识、可靠Token估值与全候选净评分及其余既定优先级仍未完成。原26完整工作包验收仍0，生产VERIFIED关闭。每轮验证后commit/push实际GitHub origin；未配置GitLab远端，不冒称GitLab。

阶段148首轮冻结e786f994528847f79a794dd462469045因复查发现三条旧专项验收入口会误选新Karst ignored测试而主动中止，保留CANCELLED报告。已完成的门禁均通过；确认总控、子进程及独立harness全部退出后，修正阶段140–142旧脚本的测试范围，新Karst仍由专用必需门禁覆盖，没有跳过本轮能力验证。随后重新完整冻结，不采用被中止运行作为通过资格。

阶段148最终限定验收：运行ecb91103ff6b4430ab10292f747e93b2的39门禁全部exit0、176次Rust通过。完整原引擎468签名场景/401公开执行/401独立父费用源审计/67拒绝、四原Router真实Token与全部声明code/nonce/storage后状态、9452现代组件、64 Jovian/7056历史预编译及160原签名转换全部通过。36正常OP build→实际签名→worker模拟与原引擎相同wire一致，216错误请求及36未授权build拒绝，8旧Shanghai HTTP回归通过；原15秒HTTP预算保持。严格Clippy与正常router/harness重构建、普通/优化进度文档和参考守卫均通过。

下一轮继续P0-3的可靠Token换算与费用后金额/净评分：已有原生费和实际签名回执不代表存在可靠市场估值；应将价格来源授权、具体链/块/币种、时间/过期、整数精度与费用完整性显式绑定，缺少可信覆盖继续原币披露和毛额回退。随后仍需完成全候选费用评分，不能将单笔已执行交易经济核算冒充完整报价优化。可信bank/区块派生/共识和其余既定优先级继续，原26完整工作包验收仍0、生产VERIFIED关闭；完成当前复核后commit/push实际GitHub origin再继续。

独立复核：冻结源码905676054d9d57b614e0e491aa7091904bb71cb8a8bf794724d04961749bf584、911固定输入、46运行pins、13677参考操作数、9产物和39日志摘要一致；288份OP原JSON及8份旧simulation摘要再次匹配。最长OP HTTP为1.657561秒，所有总控/worker/原runner/harness均已退出。首轮CANCELLED与三次参考预演/失败证据保留，最终资格只对应第二次完整冻结。


<a id="plan-2026-11-19-native-valuation"></a>

## 配置预言机授权与已执行交易费用换算（2026-11-19，阶段149，P0-3）

严格对应设计§13.1、§14.2/14.4、§15.3与WP02、WP13、WP15、WP16、WP18。服务端配置批准价格来源、公钥及至少两签阈值，价格必须绑定链/genesis/源块及parent/hash/时间、币种原子单位、policy hash和有效期；HTTP请求不能设置权威价格或签名公钥。签名仅证明配置批准的签名者，不证明市场价格真实、来源独立或bank共识。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F149.1|三服务配置互读及公开/经济身份绑定、服务端签名授权|WP02,WP13,WP16,WP18|LOCAL_SCOPE_VERIFIED|Go全配置包77项、两Rust四配置/hash互读、独立Solders签名42整数参考和授权/轮换/未知签名反例统一冻结通过。|
|F149.2|源块/币种/历史时间绑定的完整宽度整数换算与费后金额|WP13,WP15,WP16,WP18|LOCAL_SCOPE_VERIFIED|四内核测试和实际HTTP统一冻结通过：完整宽度乘法后一次向上舍入、输出不足/输入加费溢出null、零费用/角色重合不重复扣。|
|F149.3|父源费用审计后的正常worker与实际签名Karst价格核算|WP13,WP15,WP16,WP18|LOCAL_SCOPE_VERIFIED|51门禁含全部原39门禁、13 EVM/六启动拒绝/13客户端覆盖拒绝、四Karst实际签名/24错误请求及36 OP/8旧HTTP通过；生产资格不升级。|

配置`simulation.native_valuation`默认缺省，旧身份hash不变；启用时仅接受dex-router的revm-fixture、配置中已有EVM链/genesis和canonical Token。每policy最多16不同公钥，阈值至少2，最多128 policy，age/validity各不超过一小时；book最多1MiB，未知字段、未知/重复/坏签名、重复币种和源块替换启动拒绝。配置、公钥轮换与时间策略进入公开和经济hash，两Rust公共实现同步，Go四配置与hash交叉读取。

正常API在原worker实际执行及父进程独立核验原生费用后，使用payerNetNativeDebit（含OP执行/L1/operator费用及角色重合后的实际净扣）换算：ExactIn按实际输出减费用，ExactOut按实际输入加费用，分别提供两侧可用性。仅在`costs.transactionEconomics`披露本笔已执行交易；`quoteRankingApplied=false`、`additionalOperationsIncluded=false`，顶层报价net/完整总费仍null，旧毛额回退保持。失败执行不伪造成功费后收付。时间按源/执行块核验，明确不是当前wall-clock价格；缺价、未来/过期、换算溢出和输出不足保持可解释的不可用结果。

验收采用固定公知测试key与合成比率，标签`PINNED_TEST_ORACLE_NOT_LIVE_MARKET`；独立Python整数与固定Solders Ed25519对照Rust ring。预检失败均保留：最初HTTP源准备错误读取不存在fixture层已改为state_anchor；首份Go导出遗漏旧API超时/并发必填项，Rust正确拒绝，补全生成器后互读通过；首轮stale HTTP未把mode传入生成器而得到fresh价格，修复工具传参后13实际执行/六启动拒绝通过，没有放宽产品断言。四Karst正常签名执行及严格Clippy预检通过，最终资格仅由同源码完整冻结授予。

工具：[validate_native_valuation.py](integration/tools/acceptance/validate_native_valuation.py)、[native_valuation_vectors.py](integration/tools/acceptance/native_valuation_vectors.py)、[native_valuation_http.py](integration/tools/acceptance/native_valuation_http.py)、[native_valuation_op_http.py](integration/tools/acceptance/native_valuation_op_http.py)。本轮尚未实现live market adapter、全部候选净收益评分或额外授权/资金交易总成本；可信bank/区块派生/共识及其余既定优先级继续。原26完整工作包验收仍0、生产VERIFIED关闭，不把配置签名机制或单笔费用换算记作设计全部完成。每轮完整验证后commit/push实际GitHub origin；当前没有GitLab远端。

阶段149补充预检：Parser专项测试误把其默认Solana首链当作EVM策略输入，产品正确返回NATIVE_VALUATION_POLICY，原FAILED日志保留。该测试改为加载既有Router EVM配置，与本轮共享配置互读的真实服务角色一致；不改产品链类型校验。另补实际输入uint256最大值加费用的独立边界断言，溢出保持null而不回绕。

配置与签名消息、原子单位和历史时钟说明见[技术参考](docs/TECHNICAL_REFERENCE.md#配置预言机授权与交易费用换算阶段149)；正文中的公钥/地址为占位符，固定验收私钥仅存在测试生成器。

阶段149首轮统一冻结44091d3f13134c2ca5ff5c69dc4e7a0c在contracts门禁失败，保留FAILED报告（格式通过，其余未执行）。原因是修正Parser测试时引入跨项目配置路径，违反仓库服务隔离契约。确认所有进程退出后改为从Parser自身fixture构造合法EVM链、owner和确认策略；未放宽契约检查或产品链类型校验。随后完整重跑。

阶段149最终限定验收：完整第二轮运行`ac277e133c344c61bbabc80c95f06632`的51门禁全部exit0、184次Rust与77次Go通过。13带价格EVM实际worker覆盖Legacy/type2、分离/重合费用角色及ExactIn/Out，另含过期/未来/缺价/输出不足/换算溢出；13客户端价格覆盖和六伪造book启动拒绝通过。四带价格Karst正常build→实际签名→worker→父费用源审计与完整原引擎同wire一致，24错误请求拒绝。原36 OP签名HTTP/216拒绝/36未授权build及8旧Shanghai HTTP全部通过，共61笔正常HTTP执行；四完整原Router后状态、401公开Karst执行/401父费用源审计/67拒绝、9452现代组件及全部历史原参考保持通过。两workspace共享crate、仿真/API严格Clippy和正常router/harness重构建通过。首轮FAILED保留，最终通过仅对应第二轮冻结。

下一轮保持P0-3、§13.1/13.3/13.5：先给现有ExactIn/ExactOut有界优化器加入逐完整候选费用评分入口，成本必须来自具体候选的核验/估计与授权单位换算，不能把毛额最优一笔的费用外推其他路径；覆盖不足时在相同已评估集合统一毛额回退，保留预算/取消/原子余数与共享池顺序。非加性的逐候选费用禁止套用原加性独立池DP。随后接正常报价的执行/成本模型与可靠真实价格来源，未绑定签名的OP候选不得伪造完整L1费；单笔transactionEconomics继续不冒充全候选净评分。原26完整工作包验收仍0，生产VERIFIED关闭；可信bank/共识及其余既定优先级继续。

阶段149独立复核：冻结源码`c68237973c1450ffc4171e1937ff89fc930994ffdfd03fc4b93a9c3bf50b25e8`、913固定输入、46运行pins、13683参考输入、12产物与51日志摘要一致；484份嵌套原始文件摘要及8旧HTTP simulation重新核对，含36 OP/13带价格EVM/4带价格Karst的实际输入与输出。最长原OP HTTP为1.360416秒，15秒预算未放宽。独立复核记录（`integration/artifacts/local/native-valuation-check/runs/ac277e133c344c61bbabc80c95f06632/independent-audit.json`，已清理）。


<a id="plan-2026-11-19-candidate-costs"></a>

## 逐完整候选费用评分内核（2026-11-19，阶段150，P0-3）

阶段149已提交并推送`a2042130555b920020aa52f136c05dda21837bf0`，远端HEAD核对一致。本轮继续设计§13.1/13.3/13.5与WP13、WP15、WP16、WP18，先补优化器逐完整候选评分组件；正常API成本模型、真实价格来源和可信bank分别接续，不改变服务职责或用固定route_activation_cost充当链费用。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F150.1|ExactIn每个完整分配方案成本回调与非加性顺序网格|WP13,WP15,WP18|LOCAL_SCOPE_VERIFIED|独立小整数CPMM完整有序网格/原子余数/共享物理池、费用改变最优方案及负数不截零统一冻结通过；任意候选成本入口禁用加性DP。|
|F150.2|ExactOut独立费用目标与完整成本覆盖/负数/宽整数|WP13,WP15,WP18|LOCAL_SCOPE_VERIFIED|原Router39计划/38可行记录加明确合成费用独立求最小；原始输入目标/预算守恒、257位评分、晚到缺成本统一回退及双模式取消/模型错误统一冻结通过。|
|F150.3|新评分与旧DP/共享状态/正常API回退及完整价格/VM回归|WP13,WP15,WP16,WP18|LOCAL_SCOPE_VERIFIED|54门禁、208次Rust/77次Go、原51门禁及新增22优化器/2正常API回退、优化器全目标严格Clippy通过；normal_quote_cost_model_connected=false。|

`xone_optimizer::optimize_with_costs`与`exact_out::optimize_with_costs`在实际数学重放出的完整allocation上调用成本模型，单位分别是buy/sell Token原子单位；服务调用者负责绑定具体源/请求/方案、核验执行/费用完整性和可靠价格，回调本身不授予价格、bank或共识资格。只在完整分配上评分，部分seed不伪造完整交易费。任一已评估可行候选返回None时，整个相同已评估集合统一使用原毛输出/最小原输入；不是把缺失成本当零，也不忽略可能更优的未定价候选。回调报错/超宽费用直接失败，前后均检查已有deadline/取消控制，次数预算耗尽保留已评估最好方案并披露截断。

经济比较使用BigInt完整宽度：ExactIn实际总输出减完整候选费用，可为负，不截成零；ExactOut所需输入加成本可达257位，比较值不回绕。结果`ranking.signed_objective_token_units`与`selected_cost_token_units`单列，并记录完整候选评估/已定价/缺失次数、实际objective、是否应用和有限网格scope。allocation仍表示交换金额，内部旧net字段不充当新费用目标；正常公开报价未接模型时仍使用阶段126毛额回退，顶层net与总费用未知。

任意逐方案成本不满足原独立池DP的可加性假设，本入口明确关闭该快捷路径，复用有界完整有序枚举/大拆单种子，保留同一物理池执行顺序、实际状态重放及原子单位余数。旧无回调入口保留原DP及行为，原八路独立DP/八路共享种子和V3原SDK序列等门禁回归。测试费用是明确合成的算法输入，不冒充真实Gas或价格；ExactOut直接使用既有完整原Router记录，不从ExactIn倒数构造目标。

工具：[validate_candidate_costs.py](integration/tools/acceptance/validate_candidate_costs.py)。正常API仍未把所有候选绑定执行/成本模型，未签名OP候选仍不能声明已知完整L1费，真实市场适配/可信bank与其余优先级继续。原26完整工作包验收仍0，生产VERIFIED关闭。完整冻结期间不改非Markdown源码，成功后独立复核、commit并推送实际GitHub origin，再继续正常报价成本接线。

阶段150下一轮实现建议（P0-3、仍按原设计）：正常EVM报价的逐候选成本接线先限定已有可绑定的单路线V2路径与revm-fixture，显式配置启用和每请求最大执行计划数；现有BOUND不支持任意多路拆单calldata，不能把数学评分组件直接记作拆单完整执行成本。每个候选复用现有实际worker/父源费用核验和阶段149授权价格，绑定同view/source、caller/recipient、固定deadline、plan/binding哈希；复用独立仿真额度、总期限与取消后的进程回收。默认关闭以保持现有无模型回退；任何缺价/不支持/执行不匹配均不授予完整费用排名。未签名OP不能准确得到最终压缩wire费用，先明确拒绝该启用组合或保留未签名成本未知，不构造假签名估计冒充真实费用。成本评分结果和原API报价缓存/重组/过期检查必须同一版本，不能计算完再更换deadline导致实际calldata与成本脱绑定。随后补真实多候选原合约收付/Gas对照、真实价格来源适配、拆单执行绑定及其余优先级。


阶段150最终限定验收：运行`b53bf6ef3d3e4ddb8a2e1d8a2f17bede`的54门禁全部exit0、208次Rust及77次Go通过，无FAILED/CANCELLED冻结。新增五项费用评分、原7网格/8拆单/2 ExactOut共22项优化器与2正常API回退通过；独立整数完整网格/共享池状态、原Router39计划/38可行记录的合成费用比较、负数不截零、257位输入加费、成本缺失统一回退和双模式取消/错误均通过。优化器全目标严格Clippy通过。原51门禁全部保留：13带价格EVM/四带价格Karst、36 OP签名及8旧Shanghai，共61正常HTTP；全部授权/覆盖拒绝、现代/历史原参考、四完整原Router和父费用源审计通过。仿真/API/公共crate严格Clippy及正常router/harness重构建通过。

该轮只完成可接服务执行/成本模型的算法组件；回调不认证价格或bank，测试费用明确合成。`normal_quote_cost_model_connected=false`与`all_candidate_net_cost_ranking_supported=false`保留，正常API继续毛额回退，不能把组件通过记作完整设计完成。下一轮按上面的实现建议接限定正常报价、逐候选实际执行/父费用核验与来源/时间/单位绑定，然后继续可靠真实价格来源、拆单执行成本及其余既定优先级；原26完整工作包验收仍0、生产VERIFIED关闭。

阶段150独立复核：冻结源码`b5985eeb2589f9a824e25ae25afc23247e83c6d458f49ebeca58dabbbac3d389`、913固定输入、46运行pins、13683参考输入、12产物、54门禁日志摘要一致，484份嵌套原始文件及8旧HTTP simulation摘要重新核验；正常OP最长HTTP为1.361364秒，原15秒预算保持。独立复核记录（`integration/artifacts/local/candidate-cost-check/runs/b53bf6ef3d3e4ddb8a2e1d8a2f17bede/independent-audit.json`，已清理）。


<a id="plan-2026-11-19-normal-candidate-costs"></a>

## 正常报价本地逐候选执行费用接线（2026-11-19，阶段151，P0-3）

阶段150已提交并推送`512870a82dfe9a02b4eca71e9f8814c3cfc06376`，本地/远端HEAD一致。本轮按设计§13.1/13.3/13.5/14.2/14.4/15.3继续，把完整候选成本组件接入正常ExactIn/ExactOut报价。仍限既有三服务和同二进制worker，未改变生产VERIFIED准入，原26工作包完整验收仍0。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F151.1|可选有界配置、三语言互读与费用策略身份|WP02,WP13,WP18|LOCAL_SCOPE_VERIFIED|candidate_cost_max_plans默认0省略；限1..16、revm-fixture、授权价格/max_splits=1；六Go/Rust配置身份与拒绝、59门禁冻结通过。|
|F151.2|正常ExactIn/Out逐候选真实执行与固定计划绑定|WP13,WP15,WP16,WP18|LOCAL_SCOPE_VERIFIED|同view/source/caller/recipient及固定deadline/plan/binding，实际worker/父审计/授权价格；未知统一回退、错误/取消不吞为零；59门禁冻结通过。|
|F151.3|正常HTTP与原合约逐calldata/Gas/收付对照及完整历史回归|WP03,WP13,WP15,WP16,WP18|LOCAL_SCOPE_VERIFIED|16报价/30原候选执行、4未签名OP回退/后续签名执行和旧61 HTTP；59门禁/215次Rust/78次Go、独立摘要复核通过。|

可选`simulation.candidate_cost_max_plans`进入public/economic配置身份，默认0保持旧序列化及毛额回退；非Router/无授权价格/非本地VM或多路拆单启用配置拒绝。正常报价创建一次Context/deadline，优化器每完整allocation生成独立plan及BOUND，随后实际worker执行并由父进程核验费用，授权价格按源块/执行时间换算。最终选中计划使用相同deadline，不在评分后续期。每候选成本都绑定本次实际输入/输出/费用，禁止把毛额最优交易的费用外推其他路线。

本轮仅能计算`LOCAL_EVM_TRANSACTION_EXECUTION_FEES_ONLY`目标，不包含额外授权/资金交易和未知链专用费用。`quality.costRanking`披露局部目标、完整候选计数、仿真预算、逐候选实际证据与最终选中plan；`completeQuoteCostRanking=false`、顶层完整net/总费null和INDICATIVE保留。已授权换算可用但净目标为负或加费达257位时继续宽整数比较；缺价/过期/换算溢出/执行不匹配或超仿真计划数时，相同已评估集合统一毛额回退。worker协议/身份/繁忙/超时等错误直接失败，不计免费成本。未签名OP不启动候选worker，明确`OP_SIGNED_ENVELOPE_REQUIRED`，不构造假签名估算压缩wire费。

异步执行桥接复用外层总期限/取消，10毫秒检查取消并drop现有执行future，由既有owned job持有独立permit直到子进程回收；不会在一笔候选结束时误取消后续候选。发布前重新检查view/过期，缓存和内部evidence保存同一费用评分与最终plan。单笔额外simulate可核对评分时绑定/calldata/outcome不变。

预检以原未修改Router02/Factory/Pair/Token构造直达与两跳竞争，同一银行下两跳毛额较优且Gas较高；固定测试签名者与明确合成价格让实际费用改变最优路线。16正常报价覆盖ExactIn/Out、Legacy/type2、额度1、过期/未来/缺价/负目标/换算溢出；30实际候选分别由独立Ganache执行精确HTTP calldata，逐Gas/Token收付/分离付款人净扣一致。独立重放保留两侧header，显式effective gasPrice相同；不宣称Ganache重放baseFee/区块hash与产品执行环境相同，原Router零value且不读BASEFEE。完整费用账户、alias及历史/现代OP继续由已有独立原参考覆盖。

预检报告`integration/artifacts/local/candidate-normal-cost-precheck-20261119-a/report.json`通过，报价1.378..3.058秒（原15秒预算不变）；最终资格必须来自随后同一源码完整冻结，预检不替代正式门禁。工具：[validate_normal_candidate_costs.py](integration/tools/acceptance/validate_normal_candidate_costs.py)、[candidate_cost_http.py](integration/tools/acceptance/candidate_cost_http.py)、[原竞争路径生成器](integration/tools/ci/candidate-cost-execution-reference.cjs)、[原calldata重放](integration/tools/ci/candidate-cost-original-replay.cjs)。原输入和生成产物本地保留，不强制提交ignored数据。

真实市场价格来源适配、拆单执行绑定/完整成本、可信bank/区块派生/共识和其余既定优先级仍未完成。仅有GitHub origin而无GitLab远端，本轮通过后commit/push实际origin；不冒称GitLab。没有阻断全部可开发工作的真实外部阻塞，继续按优先级推进。


阶段151首份限定验收（行尾空格修正前源码）：冻结运行`e5c5ebeb7c174000b7199fcad9039f9b`的59门禁全部exit0、215次Rust与78次Go通过，无FAILED/CANCELLED冻结。16正常报价覆盖ExactIn/Out、Legacy/type2、费用改变路线选择、全部负目标、过期/未来/缺价/换算溢出与额度1；30候选实际worker及30独立Ganache原Router同calldata逐Gas/Token收付/分离付款人净扣一致。缓存重用/内部evidence/最终额外simulate保持同deadline/plan/binding/calldata/outcome，客户端成本覆盖拒绝；三个绑定/宽整数/取消桥接测试、两侧新增配置限制及六Go/Rust身份互读通过。

额外四Karst正常报价在模型启用时明确未签名费用未知、零候选worker；当时正常build后的实际签名/worker/原完整引擎执行和24错误请求拒绝继续通过。旧13带价格EVM/4带价格Karst、36 OP签名HTTP与8 Shanghai HTTP全部保留，共原61正常HTTP不减；原448 BOUND/父费用源核验、现代9452组件及全部历史Gas/签名/费用/原生余额回归保持。优化器、公共crate、仿真/API全目标严格Clippy及正常router/harness构建通过。

独立复核：源码`033d71fac1be05af23e8240a69c7170cd0b59047d2b58909165f7ddc58051592`、913固定输入、46运行pins、13827参考输入、14产物和59日志摘要一致，694份嵌套原始文件及8旧HTTP simulation摘要重新核验；16正常成本报价最长2.894542秒，旧OP最长1.516594秒，原15秒预算不变。独立复核记录（`integration/artifacts/local/normal-candidate-cost-check/runs/e5c5ebeb7c174000b7199fcad9039f9b/independent-audit.json`，已清理）。确认无总控、worker或harness残留。验收期间仅清理可重建Go编译缓存以回收磁盘，未删除原参考、测试二进制或报告，冻结源码/输入摘要未变。

本轮`normal_quote_cost_model_connected=true`仅指上述可选本地单路线V2目标；`complete_quote_cost_ranking_supported=false`、`all_candidate_net_cost_ranking_supported=false`与`live_market_price_adapter_supported=false`保留。证据有单worker输出1MiB及每请求最多16计划的局部上界；租户/缓存总字节配额仍属P1-4/P2-2后续范围，不据此宣称生产内存容量通过。原26完整工作包验收仍0、生产VERIFIED关闭。

下一轮继续P0-3：补可靠真实市场价格来源适配，固定具体feed/版本、币种原子精度、同源块和时效，保留原始返回及授权边界，不能把固定测试比率或多数签名本身当成市场真实性。原价格授权和本轮执行接线复用；缺乏可信来源继续披露原币费用/统一毛额回退。随后接拆单完整执行绑定/成本和其他既定优先级；正常costs与局部评分披露也须同源一致。完整来源/生产资格不足逐项记录，继续其余可本地开发功能；本轮最终文档检查后commit/push实际GitHub origin，再进入下一项，不因本地范围通过停止目标。


阶段151提交前补查：第一份冻结运行与独立复核已通过，但`git diff --cached --check`发现新验收脚本第32行存在一个行尾空格（此前工作区diff不覆盖未跟踪新文件）。确认总控/worker/harness全部退出后仅移除该空格；不改功能、断言或预算。第一份LOCAL_SCOPE_VERIFIED报告及其`033d71…`源码摘要保留，仅证明修正前字节。当前任务暂回IN_PROGRESS，重新运行完整59门禁，最终提交资格以第二份冻结及独立复核为准，不能用行为未变替代精确源码一致性。

阶段151最终提交资格：移除单个行尾空格后的完整冻结运行`6f0c325abe344e6c87713c5789fefef0`再次59门禁全部exit0、215次Rust/78次Go通过；本轮16报价/30候选worker/30原Router重放、额外4 OP未知费用回退与原61正常HTTP完整保留。源码`12173ec4f6ca6b61e0f433eeffc7b4990ad2fdfbc3cb9c1c1efe2ea3a629e307`、913固定输入、46运行pins、13827参考输入、14产物和59日志摘要均独立重算一致，694份嵌套原始文件及8旧HTTP simulation摘要通过。本次独立复核（`integration/artifacts/local/normal-candidate-cost-check/runs/6f0c325abe344e6c87713c5789fefef0/independent-audit.json`，已清理）。最长候选报价3.017708秒、原OP HTTP 1.439458秒，原15秒预算保持。第一份报告仅保留其原字节资格；本轮状态恢复LOCAL_SCOPE_VERIFIED，完整设计/生产资格边界不变。总控、worker和harness均已退出，提交前同时检查暂存区与工作区空白，最终文档普通/优化检查后commit/push实际origin，再进入下一项。


<a id="plan-2026-11-19-approved-chainlink-feeds"></a>

## 批准Chainlink feed的真实来源适配（2026-11-19，阶段152，P0-3）

阶段151已提交并推送`eb008fb462b0e80974e3bb3ba0496bb407c4b75d`，本地/远端HEAD一致，工作区干净。本轮仍按§13.1/14.2/15.3的可靠价格与时间要求，补已有签名比率缺少具体市场来源适配的问题。先限Ethereum主网的服务端批准AggregatorV3 USD双feed，不能自动推及L2 sequencer/其他链。官方接口固定到chainlink-evm `cfc3cc2381a2c83ba0e3242ed740609a800f5fff` 的AggregatorV3Interface.sol（SHA256 `87fd1685e015c2001673b442c212f07847a64afa2f3b22bbbcb7f211ed293e88`）。公开RPC已实际成功返回固定块feed，不记录为外部阻塞。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F152.1|可选批准feed策略与跨语言配置身份|WP02,WP13,WP18|LOCAL_SCOPE_VERIFIED|代理/聚合器与代码、USD描述、精度、答案上下界/时效进入策略身份；旧策略省略新字段保持兼容。|
|F152.2|原始ABI证据、整数比率推导及签名授权接线|WP13,WP15,WP16|LOCAL_SCOPE_VERIFIED|同源块/币种，拒绝负数/零/脏宽度/轮次/未来或过期/升级不匹配；全宽约分后uint256，换算一次ceil；签名覆盖完整来源。|
|F152.3|有界只读固定块采集和待签署输出|WP03,WP13,WP18|LOCAL_SCOPE_VERIFIED|EIP1898 blockHash/requireCanonical、链/创世身份与原响应留存；不持有生产签名私钥，输出待批准/签名观察值。|
|F152.4|真实公共返回、独立算术/反例与既有链路回归|WP02,WP13,WP15,WP16,WP18|LOCAL_SCOPE_VERIFIED|公开返回仅证明捕获的RPC/具体feed范围；本地签名者明确测试用途，不能据此授予bank共识/来源独立性/生产资格。|

本轮局部退出标准是批准来源适配器、严格授权与源时效/精度边界及正常成本链路回归，不是完整P0-3关闭。生产配置仍须真实操作方审核feed身份/币种/上下界和更新策略；公开RPC一致性不等于共识证明。未完成的可信bank、拆单完整执行/成本、额外授权/资金操作费用和其余既定优先级继续，原26完整工作包验收仍0、生产VERIFIED关闭。最终冻结期间禁止修改非Markdown源码，记录真实门禁和摘要后commit/push实际GitHub origin，继续下一项。

阶段152预检：四项源绑定/独立算术/时效/拒绝测试通过，48合成精度/宽整数案例与一份真实公共feed观察交叉验证；Go与Router的8有效/14无效配置互读通过。Parser首份预检因测试加载了本服务配置却指定dex-router而拒绝，已修正测试服务名，不改生产验证规则；随后重跑。采集器普通/优化各6项控制通过，绝对总期限拥有并回收子进程，HTTP429原返回保留且不回显provider异常文本。CLI对49组候选及真实原始RPC的完整离线重放预检通过，历史捕获时间显式保留。

公开来源预检事实：首份finalized采集在实际读取时其ETH轮次已超过批准的一小时age，正确拒绝，`integration/artifacts/local/approved-market-feed-precheck-20261119/publicnode/report.json`保留FAILED，未放宽age。随后显式当前块`0x92c236c39c86b66fa8176aeadaeb94b994f76f10412ab7aaf6006250b5f903cc`的捕获通过（block 26444392），明确不声明最终性，原始code/ABI与同高度canonical复查保留于`explicit-publicnode`。另一Cloudflare尝试读取失败记录保留，单来源成功不证明来源独立性；不能把公共RPC全部记录为阻塞。全部市场捕获使用公开测试签名者，仅授权测试语料，不是生产批准。

本轮继续保留`live_market_price_adapter_supported=false`作为完整持续live book链路未完成的边界，新增`approved_ethereum_chainlink_usd_pair_adapter_supported`单独标识本轮有限功能。已有book在服务生命周期内不可变；持续更新/授权发布和同可信bank联验仍待后续实现。48向量与公开原始响应由当前源码冻结验收后才取得本轮最终资格，预检不替代正式门禁。


阶段152最终限定验收：完整冻结运行`b880a8f71f784227bfdcdba4d4a678e3`的68门禁全部exit0、223次Rust与80次Go通过，无FAILED/CANCELLED冻结。Go与两独立Rust workspace的8有效/14无效市场策略与public/economic hash一致；默认省略的新字段保持原六身份向量与42固定签名/整数案例通过。48独立Fraction向量包含37可表示/11最终比率溢出、148费用换算，四源/ABI/精度/时间/升级/重新签名篡改守卫通过；普通/优化采集器各6项资源/错误测试通过。

当前产品CLI对48合成向量与1真实公开观察逐一重验，49结果与独立观察/签名消息一致；17份原始RPC返回以固定请求和原捕获时间完整离线重放，不进行新公共读取，也不更新旧轮次时间。固定官方AggregatorV3/Proxy/EACProxy源码摘要一致，公开测试策略固定到实际proxy/aggregator代码；实际市场数据、合成算术样本和公开测试签名者的权限边界分别保留。当前CLI构建摘要为`3c37d58654faada3712dc93d2ddbaa2af72a78606bb438869c87de693a84dfc9`。

原59门禁完整保留：16正常候选报价/30实际worker/30原Router重放、4未签名OP回退与后续签名执行、原61正常HTTP及全部现代/历史原引擎、父费用审计、严格Clippy和正常构建均通过。最长本轮候选报价2.837371秒、原OP HTTP 1.462419秒，原15秒预算不变。源码`bd7d177ef6ef76170002cd9b60fa19750f75330f6860a04580931d607c12220b`、913固定输入、46运行pins、13856参考输入、16产物和68日志均独立重算一致；840份原始产物、8旧HTTP simulation、49 CLI观察及当前校验器二进制摘要再次核验。独立复核（`integration/artifacts/local/approved-market-feed-check/runs/b880a8f71f784227bfdcdba4d4a678e3/independent-audit.json`，已清理）。总控/worker/harness/采集子进程已退出。

该轮完成批准Ethereum feed的有限适配/授权与采集工具，未完成持续live book刷新、可信bank联验、L2 sequencer/其他链来源、拆单完整执行及报价总成本。`approved_ethereum_chainlink_usd_pair_adapter_supported=true`，完整live/完整净评分/生产边界继续false，原26完整工作包验收仍0。最终文档普通/优化检查后commit/push实际GitHub origin，再进入下一项；GitLab远端仍未配置。

下一轮按P0-3、§13.1/15.3，补正常报价成本与候选评分的同源披露：当前`quality.costRanking.selectedExecution`已有逐计划审计的Gas/原币费用/授权换算，但正常报价`costs`仍一律写全部未知。仅对真正选中且成功、来源/计划/绑定一致的执行披露已知局部交易费用与单笔换算，选中计划无证据则保持未知，不借用其他候选；继续保留额外操作/未知链费和完整net为null，不把局部Gas当完整报价成本。然后继续拆单执行/完整费用及既定优先级，不因本轮限定通过停止目标。


<a id="plan-2026-11-19-selected-candidate-cost-disclosure"></a>

## 选中候选费用的正常报价同源披露（2026-11-19，阶段153，P0-3）

阶段152已提交并推送`fd22b0b8aa9565650bb063c1fbe48fd1f3ddcdcb`，本地/远端HEAD一致，工作区干净。按§13.1/15.3/15.5继续修正常报价费用展示与实际候选评分脱节：已选中且成功执行的计划具有审计Gas/原币/单笔授权换算，但顶层costs仍无条件全部未知。只复用相同计划/绑定/金额/来源证据，不新增伪造估值或替其他候选推算费用。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F153.1|选中计划费用发布与证据一致性|WP13,WP15,WP16|LOCAL_SCOPE_VERIFIED|selectedPlanHash、候选证据映射、binding/caller/recipient/原输入输出与已用费用相同才发布；失败/无证据保持未知。|
|F153.2|ExactIn/Out正常入口、缓存和内部evidence同源|WP13,WP16,WP18|LOCAL_SCOPE_VERIFIED|正常costs披露已知局部交易Gas/原币费用/单笔换算及来源；完整总费和完整net继续null；不借用其他候选。|
|F153.3|正常HTTP/原Router及全部来源/历史回归|WP13,WP15,WP16,WP18|LOCAL_SCOPE_VERIFIED|既有16报价/30原执行逐字段比对costs与选中执行，覆盖毛额回退/缺价/负目标/溢出/预算；全部68门禁保留。|

本轮仍限已有显式启用的本地V2单路线费用目标，权威仍为PROVIDED_LOCAL_STATE及配置价格授权；公开costs不是完整报价总费用，不能因此开放生产VERIFIED。未知链费/额外操作、可信bank、持续live价格、拆单完整执行和其余优先级继续。完整冻结期间禁止修改非Markdown源码；通过后独立复核、记录并commit/push实际GitHub origin，继续下一项。

阶段153冻结前预检：候选模块六项单测、Python语法检查通过；实际HTTP ExactIn两候选费用排名/选中披露及ExactOut单计划预算/统一毛额回退通过，原15秒预算未改。预检产物保留于`integration/artifacts/local/selected-candidate-cost-precheck-20261119`。下一步运行完整68门禁并独立重算摘要。

阶段153最终限定验收：完整冻结运行`9a4eba40e65b41e495863fea60834a1e`的68门禁全部exit0、226次Rust与80次Go通过，无FAILED/CANCELLED冻结。六项候选控制/费用发布测试覆盖15种计划/绑定/金额/评分错配、无证据/执行失败、缺价、负目标与宽费用。16个正常报价/30次实际worker及30次原Router重放通过；16报价公开costs逐字段与选中执行一致，单计划预算场景的毛额选中计划也有执行证据；另4未签名OP报价没有执行证据，费用保持未知。缓存、内部evidence及最终simulate同计划/绑定/calldata/outcome不变。

原批准feed的8有效/14无效三语言配置、48合成/1公共捕获、49产品CLI结果及17原RPC返回离线重放通过；原61正常HTTP、全部现代/历史原引擎与父费用源审计、严格Clippy及正常构建完整保留。冻结源码`3fdc38ea81de6fdff1511dc9c59ae68202bdc68d507f9cd9914bbfe38def0bf6`；独立摘要复核通过：913固定输入、46运行pins、13856参考输入、16产物与68日志一致，840原始文件和8旧HTTP simulation再次核对；16同源费用披露及4未签名OP未知报价、49市场CLI与当前二进制摘要均一致。最长候选报价2.897446秒、原OP HTTP 1.458427秒，原15秒预算不变。独立复核（`integration/artifacts/local/selected-candidate-cost-check/runs/9a4eba40e65b41e495863fea60834a1e/independent-audit.json`，已清理）。总控/worker/harness均已退出。

本轮仅完成已有显式启用的本地V2单路线正常报价选中费用披露，完整报价总费/完整net与生产VERIFIED继续关闭；其他执行/成本模型、可信bank、持续live价格刷新/授权发布及其余优先级未完成，原26完整工作包验收仍0。下一轮按P0-3和§13.1/15.5/21.3/26.3补授权价格book的运行期版本发布和报价快照/缓存绑定：版本必须由配置签署方签名覆盖，拒绝进程内回退、同版本改写与币种集合降级，每次候选比较固定同一book，旧报价执行不得悄悄套新价格。固定本地bank不获得live/共识资格，跨重启耐久发布与自动采集/签署需分别闭合。每轮继续验证/文档/commit/push现有GitHub origin，GitLab仍未配置。

阶段153独立复核计数修正：首次审计脚本错误预期16正常报价中有2份无选中执行，实际两份预算受限报价的毛额选中计划均为已执行候选。原始报价逐一核对后将计数改为16份同源费用披露，并独立核对另4份未签名OP正常报价的selectedExecution=null及Gas/原币费未知；没有修改产品/冻结门禁或减少语料。首次审计错误记录保留于正式run目录。


<a id="plan-2026-11-19-price-book-refresh"></a>

## 授权价格book版本刷新与报价快照（2026-11-19，阶段154，P0-3）

阶段153已提交并推送`904ef9533558d6c2dbcc82ca5e1f5ce67f9004df`，远端HEAD一致、工作区干净。对应§13.1/15.5/21.3/26.3，当前服务只在启动读取价格book，替换文件不生效，且候选/缓存尚无运行期价格版本绑定。本轮补固定配置路径的显式授权刷新、签名覆盖的发布版本和不可变报价快照，保持现有链身份/锚点与父费用审计边界。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F154.1|签名发布版本与原子替换|WP13,WP15,WP16|LOCAL_SCOPE_VERIFIED|旧观察省略版本保持原hash；新book全部观察版本一致且由原quorum签名，变化必须递增且币种集合相同；错误发布保持原版本。|
|F154.2|单报价价格快照、缓存及旧报价执行检查|WP13,WP15,WP16,WP18|LOCAL_SCOPE_VERIFIED|每次候选比较固定同一不可变book，身份包括bookHash；刷新后不命中旧报价，旧quoteId不得悄悄换价。|
|F154.3|有界认证刷新与正常HTTP/全部来源回归|WP13,WP15,WP16,WP18|LOCAL_SCOPE_VERIFIED|只读固定配置文件，不从请求接收价格/路径；签名/版本/缺币/过期错误拒绝；普通与优化文档及完整68门禁保留。|

退出范围仅为进程内版本发布和固定本地bank下的价格刷新，重启防回退的耐久高水位、自动市场采集/独立签署发布、可信bank持续推进与生产资格分别保留。旧版零发布版本允许启动，运行期变化须使用签名的正递增版本。全设计/26工作包未完成；本轮通过后文档/commit/push既有GitHub origin并继续下一项。

阶段154预检：独立Solders生成10份签名发布book，两项发布状态测试与五项授权/单位换算测试通过。首次/第二次HTTP预检在build检查发现验收脚本误用GET（真实接口为POST），保留目录`price-publication-precheck-20261119`与`price-publication-precheck2-20261119`，未修改产品路由；脚本改为POST并记录拒绝原响应后，第三次完整预检通过6报价/12候选执行/12原Router重放、11错误发布拒绝、4并发发布旧报价明确失效。该次尚未包含新增price_current/公开来源版本字段的最终复测。严格Clippy要求测试模块放文件末尾，已按要求调整，未关闭lint。

阶段154补充预检：第四次正常HTTP已验证price_current与公开sourceEvidence价格版本，6报价/12候选执行/12原Router重放、11拒绝和4并发发布通过；独立CLI6案例覆盖零/正/u64最大版本及负数/超宽/字符串拒绝，保留原捕获时间且不读取公共RPC。Unix价格文件采用已有锁定libc0.2.191的O_NONBLOCK后核对普通文件，避免FIFO在open阶段等待writer；Cargo.lock仅增加quote-api对既有版本的直接依赖，无新版本解析。新增缺文件/目录/FIFO拒绝的最后预检后，冻结全部72门禁。

阶段154首轮完整冻结`a90ca43fd7d64f7aace87e75abcefd5e`在contracts门禁失败（format已通过）：新增`native-valuation-publications.json`仅生成Router本地副本，遗漏integration契约源副本。确认总控/worker/harness均退出后，生成器与冻结入口补齐integration和Router同字节双副本；未改契约检查或产品权限。FAILED报告保留，重新完整冻结72门禁。最终Unix预检已通过14种错误发布拒绝（含目录/FIFO/缺文件）、6报价/12原执行及4并发发布；独立输入数因双副本增加2份。

阶段154最终限定验收：完整第二轮运行`1efadba05545403b8a65d6c3e6a14f5f`的72门禁全部exit0、229次Rust与80次Go通过。首轮contracts FAILED保留，最终通过只对应第二轮冻结。新发布状态/签名版本测试与独立Solders10份book核对通过；旧零版本省略编码保持原签名/hash。6个正常ExactIn/Out报价、12候选实际worker与12原Router重放通过，11种内容/授权错误加缺文件/目录/FIFO共14错误发布拒绝，原已授权快照保持不变；4次并发发布均成功，旧版本报价均以409明确失效，未混用价格。新版本改变了实际费用排序，旧quoteId的build/simulate均拒绝换价，原内部evidence保持且price_current=false。

新增市场CLI6案例覆盖零/正/u64最大发布版本及负数/超宽/字符串拒绝，签名消息由独立Python摘要核对，未签名观察不获得授权，原公开捕获时间不改且不进行新公共RPC。原68门禁完整保留，包括16候选正常报价/30原执行、4未签名OP回退、原61正常HTTP、48合成/1真实公开捕获、49CLI/17原RPC、全部现代/历史原引擎与严格Clippy/正常构建。源码`2b4d870c1325cf0018907ec3aad06d2945943163d8da4d6a7fb02703a63c4141`，独立复核通过：915固定输入、46运行pins、13859参考输入、18产物与72日志一致，902原始文件和8旧HTTP simulation再次核对；12发布观察的签名独立验证、6报价/12原重放、14拒绝/4并发及6 CLI消息一致。最长普通候选报价4.465740秒、原OP HTTP 1.937883秒，15秒预算未改。当前市场CLI摘要`3cffe3ffc271f25b8c4fff030ec3ebdea96f9604fab5147819d26cb8df9f619b`。独立复核（`integration/artifacts/local/price-publication-check/runs/1efadba05545403b8a65d6c3e6a14f5f/independent-audit.json`，已清理）。

本轮只完成当前进程、固定本地bank下的显式授权发布，`price_publication_restart_rollback_protected=false`和`continuous_live_market_book_refresh_supported=false`；完整费用/net、可信bank/共识与生产VERIFIED保持关闭，原26完整工作包验收仍0。下一轮继续P0-3和§15.5/21.3/26.3，补显式配置的耐久发布journal与重启防回退：固定策略/链身份、已有签名book和记录hash链、独占文件锁、写入fsync先于确认、尾部半记录恢复/完整损坏拒绝、丢journal不自动重新初始化，覆盖旧book重启、并发writer、崩溃/IO故障与恢复；持久化提交超时必须区分未知结果并关闭旧价格使用，不能承诺无法实现的“超时必无持久效果”。持续自动市场采集/独立签署/真实bank推进及其余优先级分别继续。通过最终文档检查后commit/push实际GitHub origin，GitLab目标仍未配置。


<a id="plan-2026-11-19-price-publication-journal"></a>

## 耐久价格发布journal与离线初始化（2026-11-19，阶段155，P0-3）

阶段154已提交并推送`22b850952ce591f7c88abd69f7ea50adc2b0c9ba`，远端HEAD一致、工作区干净。按§15.5/21.3/26.3继续补跨重启的发布高水位。本轮先实现Router所属的耐久journal组件及显式离线初始化/校验/推进工具；下一轮再以默认省略的跨语言配置接入正常刷新/启动，使普通路径也具备该保证。组件通过不能提前改为normalRouterConnected=true。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F155.1|签名book准入及耐久高水位格式|WP13,WP15,WP16|LOCAL_SCOPE_VERIFIED|正发布版本、同源锚点/配置quorum、实际时间、固定策略/币种集合、序号与hash链；三条记录逐项独立复算。|
|F155.2|独占journal、fsync确认与恢复|WP15,WP16,WP18|LOCAL_SCOPE_VERIFIED|显式create-new初始化，缺文件不自动重建；排他锁、半尾恢复、完整损坏拒绝、提交不确定关闭writer及进程退出重开通过。|
|F155.3|离线CLI与独立文件/崩溃/原路径回归|WP13,WP15,WP16,WP18|LOCAL_SCOPE_VERIFIED|34个CLI案例（9成功、25拒绝）、6项组件测试与原72门禁通过；不签名不发送交易。|

耐久性以本地持久卷不整体回滚为前提，不提供恶意文件系统回滚/独立共识证明；真实丢盘/跨区灾备与生产资格仍按既定上线门槛验收。必须区分fsync前后未知提交结果，不将超时或IO错误解释成确定未发生。正常API目前仍使用阶段154进程内模式，只有完成后续显式配置与接线后才可声明其具备跨重启防回退。

恢复开发后先核对清理边界：旧Rust构建目录已不存在；确认没有运行中的测试或打开的句柄后，移除五个遗留的同字节测试二进制暂存副本（约108 MiB）。保留冻结报告、签名输入、原始数据与固定运行依赖；原参考清单中缺失的ethers和ganache两个package.json从仓库锁文件恢复，SHA-256均与阶段154一致。后续各轮测试前先检查遗留进程及可重建临时文件，不清理仍在用的构建目录或作为验证依据的文件。

阶段155预检：此前6项Rust测试与34项离线CLI案例通过。复核发现首次初始化成功写入后CLI借用check回执，导致changed错误地为false；已修正并加强独立回放，以实际文件末条核对响应的序号、entry_hash、高水位与操作是否写入。恢复截断前复核文件身份及原始长度，初始化同步父目录后再复核路径。以上修正须以当前源码重测，不引用先前的通过作为冻结结果。

阶段155最终限定验收：运行`5255594d26fa40fcb3e700601e15eb15`的75门禁全部exit 0，235次Rust及80次Go通过。源码摘要`1496572e58364a0aee0886adaf0695b895f1e77b7766521946be6f5648d8dee1`。独立复核915个固定输入、46个运行pins、13859个参考输入、19份报告、75份日志、1017份原始产物及三条日志的策略/授权book/前摘要链；CLI SHA-256 `4c5927d7876a46c6af63ad29a52a1a2dfa543f3748d619e18c0c72284b907e15`。34个CLI案例中9个成功回执与落盘记录一致，25个拒绝核对错误码。16个普通候选报价/30次原Router重放、4个未签名OP未知费用及进程内发布回归保留。总工作包完整验收仍为0，生产VERIFIED保持关闭。

下一轮按P0-3：在Go及两个Rust配置副本中加入显式、默认省略的journal路径；正常Router启动仅打开已有日志并核对授权book与耐久高水位一致。正常reload先验证、后同步落盘、最后发布价格快照；提交期间或写入结果未知时不得服务旧价格。验证启动缺文件/旧book/锁冲突、运行期刷新、重启恢复、旧报价及已有进程内模式。其后继续真实市场采集/授权、拆单执行与完整费用等已列优先级，阶段155不授予普通API跨重启保证。

<a id="plan-2026-11-19-durable-price-router"></a>

## 正常Router耐久价格发布与重启（2026-11-19，阶段156，P0-3）

阶段155已推送至GitHub origin `293201935dfd5f7bd2d1d37257da1d6a99952c3c`；未配置GitLab远端。本轮对应§15.5/21.3/26.3，仅在显式启用持久日志的正常本地bank服务提供跨重启保护，未改变默认回退或生产VERIFIED。测试前核实无打开句柄，清理旧HTTP harness暂存副本及三份旧worker临时状态，释放约99 MiB；保留阶段155的报告、冻结输入及共享依赖。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F156.1|跨语言配置与启动高水位|WP15,WP16,WP18|LOCAL_SCOPE_VERIFIED|Go 导出默认/显式配置，两份 Rust 分别核对公开和经济摘要；非法路径拒绝。只打开预初始化日志，重启不自动推进。|
|F156.2|持久提交期间撤销价格快照|WP13,WP15,WP16|LOCAL_SCOPE_VERIFIED|文件同步不持快照锁；提交期间拒绝取价。确认丢失后内存未恢复旧价，重开读回已提交版本 2。|
|F156.3|正常HTTP、重启与原门禁|WP13,WP15,WP16,WP18|LOCAL_SCOPE_VERIFIED|4 次成功报价、5 个启动拒绝、3 份发布回执，旧报价 409、日志替换后 503、恢复原日志后重新报价。原 75 项门禁也全部通过。|

限定工具：`python3 integration/tools/acceptance/validate_price_publication_journal.py --router`。实际回放调用正常Api及其worker，不向公网提交交易。报价为本地固定bank的INDICATIVE结果；持续live来源/签署、可信bank、拆单执行、完整费用与净排序均不能因本轮通过而关闭。

首次全量运行 `6c9f5b21afea49c49d2c2e188a408c60` 失败，报告保留。旧配置门禁运行整个 `native_valuation` 测试目标，提前选中了依赖尚未导出的 `XONE_PRICE_JOURNAL_CONFIGS` 的新增用例。已将两份 Rust 的新增用例隔离到 `durable_price_journal` 测试目标，在 Go 导出之后运行；保留原断言和旧门禁。

最终运行 `4ab92751086041e49f974d0a2d599007`：80 项门禁全部通过，239 次 Rust、82 次 Go；源码 SHA-256 `d2cca58ec04921d2a9613588a7d6500db1c40bf9fd72b190d0fd57958835cf78`。独立复核（`integration/artifacts/local/durable-price-router-check/runs/4ab92751086041e49f974d0a2d599007/independent-audit.json`，已清理）核对了 21 份报告、80 份日志、1055 份原始产物，以及报价版本和费用来源、发布回执和前一条记录的摘要。默认进程内模式的回执仍为 `restartRollbackProtected=false`。

下一项仍为 P0-3：持续采集批准来源，保留原始块、采集时间与有效期，组装待独立授权的完整价格簿；验证过期、不同块或签名不足的输入不能发布。随后补拆单完整执行、授权等辅助交易费用与全候选净额。以上不等于可信 bank 或生产资格；原 26 个工作包完整验收仍为 0。

交付状态：本轮在本地分支提交。当前唯一配置的推送目标是 GitHub `origin`，不是要求的 GitLab；缺少 GitLab 项目克隆地址，本轮未向其他托管平台推送。

<a id="market-book-signing-request"></a>

## 整簿同块采集与独立签署输入（2026-11-19，阶段157，P0-3）

针对 §15.5 的同一快照及来源有效期要求、§21.3 的可靠计价来源要求，本轮把单币种采集接成一个可独立核验的完整请求。验证范围仅限配置批准的 Ethereum 双 feed，使用本地模拟 RPC 与实际 `xone-market-price`，没有进行本轮实时公网或生产权限验证。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F157.1|完整配置、统一锚点与签名版本|WP13,WP15,WP16|LOCAL_SCOPE_VERIFIED|两币种 34 次只读 RPC，仅一次选择 finalized 块。独立复算两份策略摘要及带版本签名消息。|
|F157.2|失败隔离与交接有效期|WP15,WP16|LOCAL_SCOPE_VERIFIED|第二币种换块、ABI 错误、最早有效期到达、时钟回退均无完整请求；重复币种、其他链和非法版本在 RPC 前拒绝。|

定向验收：`python3 -B integration/tools/acceptance/tests/test_market_book_capture.py -v`（4 项）；原采集器测试 6 项通过。已将新套件接在市场验证器构建之后；本轮没有重跑阶段156全量冻结，不复用其通过数。测试前确认无活动进程和打开句柄，清理上一轮遗留的一份临时 Rust checkpoint 测试目录。

下一项：把独立签署回执与当前请求、策略、有效期及耐久版本核对后组装可发布价格簿，验证缺签/错签/过期不能发布；再接有界持续采集及真实 bank 推进。拆单执行、辅助交易费用、完整净额和其他既定优先级未关闭。阶段156本地提交 `d1703d9`；截至本轮仍未配置 GitLab，不能将本地提交记为已推送。

<a id="market-book-offline-assembly"></a>

## 独立签署回执与离线候选簿（2026-11-19，阶段158，P0-3）

交付目标已改为 GitHub，阶段156 `d1703d9` 和阶段157 `b65e648` 已分别推送至 `origin` 同一分支。上两轮的 GitLab 阻塞是历史状态，不再阻碍本轮。测试前检查了活动进程与系统临时目录，未发现可清理的上轮 xone 临时运行目录；保留共享构建缓存及验收证据。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F158.1|逐币种重算消息并组装独立签署回执|WP13,WP15,WP16|LOCAL_SCOPE_VERIFIED|两币种各两名测试签署者，乱序回执生成确定的候选簿；缺签、跨币种错签、重复签署者、错批次与过期均无输出。|
|F158.2|耐久日志离线预检，不改在线价格文件|WP15,WP16|LOCAL_SCOPE_VERIFIED|实际 CLI 预检有效簿后日志字节不变。输出不覆盖；显式 advance 后才推进，重复版本不再可提交。原 6 项日志组件回归通过。|

定向回放入口：`python3 -B integration/tools/acceptance/tests/test_market_book_assembly.py -v`，使用回环 RPC、测试签名和实际 Rust 二进制。已加入综合验收入口；不把其他阶段的全量统计或生产授权计入本轮。下一项是运行中 Router 的预检、发布及结果回读，避免离线工具争用在线日志。其后继续有界持续采集、真实 bank 推进、拆单执行、辅助交易费用和完整净额；整体设计未完成。

<a id="market-book-online-publication"></a>

## 在线条件发布与结果回读（2026-11-19，阶段159，P0-3）

阶段158 `9546761` 已推送 GitHub。按 §15.5/21.3/26.3，本轮用持锁 Router 核对待发布快照。测试前检查系统临时目录和活动进程，未发现遗留 xone 临时运行目录；共享构建缓存和历史报告保留。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F159.1|不打开日志的签署候选生成|WP13,WP15|LOCAL_SCOPE_VERIFIED|日志文件不在原处时仍能核验并生成相同候选，明确不声称已核对版本或已发布。|
|F159.2|在线预检、双摘要条件与结果回读|WP15,WP16|LOCAL_SCOPE_VERIFIED|实际 HTTP 未认证 401，候选变化、错误高水位与过期回执重放均 409；预检不改日志，状态回读与重启确认一致。|

定向 HTTP 结果（`integration/artifacts/local/online-price-control-159-1795059640/report.json`，已清理）保留原 4 次报价和 5 个启动拒绝。该范围仅是固定本地 bank、显式耐久模式的在线发布。安装候选文件、收集独立回执和服务调度仍需编排；持续采集、可信 bank 推进、拆单、辅助交易费用、完整净额及其他优先级保持未完成。

本轮完整回归（`integration/artifacts/local/durable-price-router-check/runs/c7390d09c36d4869af00c102a12af6ce/report.json`，已清理）：82 项检查全部退出 0，239 次 Rust、82 次 Go 测试执行。逐项复核 82 份日志及 21 份产物报告摘要，源码摘要 `bd0e15d88270cfee3f9a9a9c49fa1e11154982595e94e4c4efeb3d391452d801`。下一项 P0-3：将已核验候选的原子安装、双摘要发布和结果不确定时的状态回读接入操作工具；不会因网络超时盲目覆盖旧文件或重发。

<a id="market-book-operator"></a>

## 本机候选安装与结果核对（2026-11-19，阶段160，P0-3）

阶段159 `2d5f2dc` 已推送 GitHub。本轮在同一配置、显式耐久日志和固定 bank 的前提下，把批准候选的安装、条件提交、状态回读接入本机操作命令。测试前核对系统临时目录及测试进程，无遗留 xone 临时运行目录；保留共享 Rust 构建缓存和冻结证据。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F160.1|验证候选、留存旧文件并原子安装|WP13,WP15,WP16|LOCAL_SCOPE_VERIFIED|核对候选摘要、签名、时效、配置币种及已生效的旧簿；失败回放中旧文件备份字节一致、已安装但未解决的候选不被覆盖。|
|F160.2|只提交一次并支持进程外只读核对|WP15,WP16|LOCAL_SCOPE_VERIFIED|请求未送达返回结果未知；提交后丢失回执能通过当前日志头确认；相同版本不重发。独立 CLI 能回读，日志后续推进则拒绝冒认旧结果。|

定向 HTTP 报告（`integration/artifacts/local/online-price-operator-160-final-1795061399/report.json`，已清理）及操作顺序记录（`integration/artifacts/local/online-price-operator-160-final-1795061399/operator-run/results.json`，已清理）：保留 4 次正常报价和 5 个启动拒绝，工具分支独立日志依次为 1、2、3、4。回放使用实际 Router、实际 Rust 验证器、固定测试时间，在传输边界注入未送达与丢回执；不等于真实断电或生产环境认证。阶段158组装测试亦通过，本轮未重跑前轮 82 项全量验收。

下一项 P0-3：将同块采集、等待外部阈值签署、组装和本机发布接入有界的轮次调度；过期重新采集，未决发布先核对，不自动生成签署权。可信 bank 推进、拆单执行、辅助交易费与完整净额仍未完成。

<a id="market-round-scheduler"></a>

## 有界价格轮次调度（2026-11-19，阶段161，P0-3）

阶段160 `5124dd4` 已推送 GitHub。按 §15.5/21.3/26.3 绑定每轮来源配置、请求字节、正版本和 Router 日志基准，先持久记录发布意图再移交操作工具。测试前检查活动进程和系统临时目录，没有遗留 xone 临时运行目录；保留历史证据与共享编译缓存。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F161.1|分步采集、等待签名和过期换版|WP13,WP15,WP16|LOCAL_SCOPE_VERIFIED|两币种通过回环 RPC 实际采集；等待不发交易，过期换更高版本，旧回执不能授权新请求。|
|F161.2|未决发布防重放与受限恢复|WP15,WP16|LOCAL_SCOPE_VERIFIED|用实际 Rust CLI 推进日志后注入中断，恢复仅查询一次状态即确认；请求未送达时连续两次恢复仅查询状态、不启动新轮次。|

`python3 -B integration/tools/acceptance/tests/test_market_book_schedule.py -v` 三项通过，已接入综合验收。调度测试使用固定时钟、测试签署者和由真实日志 CLI 支持的测试 Router 接口；另行运行的真实 Router HTTP 回归（`integration/artifacts/local/round-schedule-161-operator-1795061864/report.json`，已清理）保留 4 次报价、5 个启动拒绝和操作工具断连回读。本轮未重跑 82 项全量回归，不宣称生产签署或可信共识已就绪。

下一项 P0-3：让采集以 Router 已确认的 bank 身份选块，并在发布前复核锚点和来源有效期，避免 finalized 采集落在另一块；可信 bank 推进另行验证。随后继续拆单执行、辅助交易费和完整净额，整体设计未完成。

<a id="market-router-bank-anchor"></a>

## 按 Router 区块锚点采集与复核（2026-11-19，阶段162，P0-3）

阶段161 `ea6505d` 已推送 GitHub。测试前检查了系统临时目录及活动进程，未发现遗留 xone 临时运行目录；共享构建和历史验收证据保留。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F162.1|按服务端完整锚点选块|WP13,WP15,WP16|LOCAL_SCOPE_VERIFIED|对两币种均调用按哈希取块，不查询 finalized；显式块冲突、旧服务缺少锚点在 RPC 前拒绝；同块哈希但父哈希不同不产出请求。|
|F162.2|签后重验与持久状态回读|WP15,WP16|LOCAL_SCOPE_VERIFIED|等待期间发生锚点变化则旧请求作废，不安装文件或发布。真实 Router 状态、预检、重启后的锚点均与固定 bank 一致。|

定向测试：5 项调度测试、4 项原独立采集测试、Rust 格式检查及真实 Router HTTP 回归（`integration/artifacts/local/bank-anchor-162-1795062290/report.json`，已清理）通过。HTTP 回归保留 4 次报价、5 个启动拒绝及断连确认流程。本轮未重跑全量门禁；仅补齐固定 bank 的选块一致性，不证明动态 bank 推进、来源共识、生产签署或完整工作包通过。

下一项 P0-3 按 §13 的候选成本覆盖要求，核对拆单候选的执行绑定、顺序状态与费用覆盖，从可执行的完整计划验证入手；无法核验全部腿及辅助交易时保留原币披露和毛额回退。可信 bank 推进和其他既定优先级继续。

<a id="ordered-v2-split"></a>

## 共享池 V2 整笔拆单执行（2026-11-19，阶段163，P0-3）

阶段162 `f4bf4a1` 已核实与远端分支一致。审查发现两种 Exact 模式的 V2 绑定和正常报价配置都只支持单路线；单个 Router02 swap 也不能表示多个路径。新增本地标准 ERC20 批量合约，固定下游 Router02，仅从调用者拉取各腿实际用量。原单路线准入规则不变。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F163.1|按序执行整笔 ExactIn/Out|WP13,WP15|LOCAL_SCOPE_VERIFIED|两腿共享末端池，调换腿序后各自与同一初始状态的原 Router02 顺序参考一致。逐笔核对付款、收款、执行器余款及残留授权、四池储备和五次 Swap 日志顺序。|
|F163.2|聚合限额及单笔原币费用|WP15,WP16|LOCAL_SCOPE_VERIFIED|最低收款提高 1 或最高付款降低 1 均使整笔失败，未保留日志或池及代币变化；宽松 ExactOut 预算不多扣。成功回执直接给整笔 Gas，预先授权另计。|

本地差分回放（`integration/artifacts/local/ordered-v2-split-163.json`，已清理）：4 笔成功、4 笔回滚。测试前检查了活动进程及系统临时目录，没有遗留 xone 临时目录；保留共享构建和历史报告。本轮仅定向验收；综合入口已加入此回放，未重跑全量。合约未部署到公共网络、未经生产审计，结果不代表正常报价已经支持拆单净额。

下一项 P0-3：从 Plan/ExactOutPlan 的有序分配重建批量 calldata，在 worker 独立核对批量合约、Router02、工厂、各池和 bank；把同一计划下的回执接回父进程及正常报价。授权与辅助交易费用、可信 bank 和其他优先项继续开放。

<a id="ordered-v2-binding"></a>

## 按完整拆单计划生成批量交易（2026-11-19，阶段164，P0-3）

阶段163 `6835c16` 已核对远端分支。按 §13.5/14.1 将批量执行器与下游 Router02 分别绑定，在完整快照上按腿序重算 V2 分配。绑定包含计划、两个执行目标的身份、每腿有序池及完整池状态摘要，并可用独立提供的执行器配置重建。这里验证的是交易构造，不证明 bank 或代码实际可信。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F164.1|ExactIn/Out 整笔调用绑定|WP13,WP15|LOCAL_SCOPE_VERIFIED|两种腿序分别从四池状态重建成交金额、整笔 calldata 和 Swap 日志池序；ExactOut 使用用户预算与滑点的较紧上限。|
|F164.2|拒绝陈旧分配及来源替换|WP13,WP15|LOCAL_SCOPE_VERIFIED|调换腿序后重新计算计划哈希仍不能复用旧金额；ExactOut 重排会越过旧预算；篡改中间金额、池序、交易数据、执行器身份、状态摘要或区块均拒绝。|

本地原合约回放报告（`integration/artifacts/local/ordered-v2-split-164-final.json`，已清理）记录 4 笔成功、4 笔整笔回滚和生成自链上读取的本地向量；测试不依赖未提交的数据文件。执行计划包 34 项、原 V2 路径仿真 5 项通过。测试前检查临时目录、活动进程；保留共享构建缓存和历史证据，清理本轮多余的可再生向量副本。未重跑综合验收，也未开放拆单正常报价。

下一项 P0-3：为本地拆单回放补齐 worker 所需的执行前账户与存储，再让 worker 在同一 bank 独立核对批量合约指向的 Router02、工厂、池、余额与授权并执行单笔交易。之后接父进程费用核验和正常报价；辅助费用和可信 bank 仍未完成。

<a id="ordered-v2-worker"></a>

## 共享池拆单独立仿真（2026-11-19，阶段165，P0-3）

阶段164 `db68297` 已核对 GitHub 分支。本轮从同一初始区块读取 15 个账户和 132 项存储，每笔交易均在其直接子块执行。回放保留执行前后的代码、原币余额、nonce 和声明的存储槽。费用回放使用显式 EIP-1559 上限。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F165.1|对照原始成交和失败回执|WP13,WP15|LOCAL_SCOPE_VERIFIED|严格 REVM 重放 ExactIn/Out 两种腿序的 4 笔成功及 4 笔回滚，逐笔核对状态、Gas、完整事件序列、全部已采集账户和存储值。|
|F165.2|独立子进程执行整笔绑定|WP15,WP16|LOCAL_SCOPE_VERIFIED|真实 worker 重新绑定计划，核对执行器代码、router()、工厂和各物理池，读取卖方对批量合约的授权。4 笔成功的输入、输出、Gas、有效单价和整笔原币费用与出块器一致。代码摘要不符、Router 指向错误、缺少授权存储均拒绝。|

本地回放报告（`integration/artifacts/local/ordered-v2-bank-165-caps.json`，已清理）。路径仿真 6 项、拆单绑定与 worker 4 项通过。测试前检查了系统临时目录及活动进程，共享构建缓存和往期证据保留。工作包仍为部分完成。本轮验收范围为固定本地 Shanghai bank。

下一项 P0-3 是在报价服务载入独立配置的批量合约身份，将同一计划的结果交给父进程费用核验，再对正常报价覆盖多候选竞争与统一回退。

阶段165 结构回归修正：综合验收在仓库契约检查处发现 Router 根目录的 `contracts` 为保留路径。执行源码移至 [Router EVM 目录](dex-router/evm/OrderedV2Split.sol)。修正后 3 个服务的契约检查通过（328 份契约、12 份基础夹具、171 份公开来源夹具），并重跑 4 笔成功与 4 笔回滚（`integration/artifacts/local/ordered-v2-source-layout-165.json`，已清理）。源码 SHA-256 与阶段163一致。

<a id="ordered-v2-quote"></a>

## 共享池拆单正常报价（2026-11-19，阶段166，P0-3）

按 §13.5 和 §14，报价服务可在本地仿真配置中指定独立的批量合约。启动时校验代码摘要、目标白名单及下游 Router02 身份。每个拆单候选提交一笔有序交易。父进程用自己的计划、执行前状态摘要、费用参数、预算和仿真器版本复核回执，再核对付款余额并换算费用。仅在配置了候选费用模型时启用，最多比较两条路线的分配。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F166.1|共享池计划按实测费用比较|WP13,WP15|LOCAL_SCOPE_VERIFIED|ExactIn 和 ExactOut 各比较两条单路线及两种拆单顺序，独立计算目标值后均选中两腿计划。|
|F166.2|费用证据不全时统一回退|WP13,WP16|LOCAL_SCOPE_VERIFIED|两种方向分别覆盖执行预算不足、价格过期及缺批量执行器。选中计划缺回执时不披露其他候选费用。|

同一出块夹具增加调用者对 Router02 的授权槽，使单路线和批量合约均能读取实际授权。两套 Rust 配置测试、Go 配置测试、父进程回执替换测试及 正常 HTTP 的 8 个场景（`integration/artifacts/local/ordered-split-166/http/report.json`，已清理）通过。阶段165冻结验收（`integration/artifacts/local/durable-price-router-check/runs/7f9298b6d4774e92a0a6058d0b85912c/report.json`，已清理）在已运行的 82 个门禁中通过 81 个，签名 OP 的 HTTP 测试读到了尚未写完的启动文件。本轮改为同目录原子发布启动文件，使用原始输入重跑签名 OP 的 HTTP 门禁（`integration/artifacts/local/ordered-split-166/op/report.json`，已清理），全部 36 个执行场景通过。进度文档在普通和优化 Python 下各通过 17 项检查。本轮未重新运行整套综合验收。

下一项 P0-3 按 §13 处理额外授权和资金交易。先区分已具备授权、需要授权、证据不足三种情况，再按同一区块顺序验证每笔交易的费用和付款余额。可信动态 bank、生产验证及其余工作包仍开放。当前仓库的已配置远端为 GitHub，GitLab 交付需要项目地址。

<a id="caller-approval-cost"></a>

## 调用者授权费用判定（2026-11-19，阶段167，P0-3）

§13.1、§14.1 和 §15 要求报价披露授权成本。父进程复核拆单回执中的 `allowance(owner,spender)` 调用，核对卖出代币、付款人、实际批量执行器以及 32 字节返回值。足额授权的候选披露 `approvalNative="0"`。缺少读数的候选披露 `UNKNOWN` 并保留空费用。ExactOut 按同一状态的报价所需输入判断。RPC 准备接口返回读取来源、实际授权额和所需额度，授权不足时继续阻止生成兑换交易。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F167.1|按候选执行目标判定额外授权费用|WP13,WP15|LOCAL_SCOPE_VERIFIED|两种报价方向各核对所有已计价候选。错误 spender、重复读数、缺少读数和返回长度异常均拒绝。|
|F167.2|准备接口披露授权缺口|WP15,WP16|LOCAL_SCOPE_VERIFIED|4 项接口测试通过，覆盖授权不足、资金不足、重组、ExactOut 所需输入及准备摘要。|

正常 HTTP 8 个场景（`integration/artifacts/local/ordered-approval-167/http/report.json`，已清理）与父进程定向单测通过。测试前确认上一轮隔离编译目录已经清理，核对并移除了重复的出块夹具。下一项是用连续状态核对新增授权交易及随后兑换的实际费用，随后处理补充资金和全候选净额。

<a id="approval-sequence"></a>

## 连续授权与兑换回放（2026-11-19，阶段168，P0-3）

按 §13.1 和 §14.2，从同一零授权区块分别执行 ExactIn 和 ExactOut。调用者先授权该拆单所需的准确额度，再在直接子块执行兑换。REVM 从初始账户和存储执行第一笔，将得到的状态交给第二笔。每笔均与原合约的出块记录核对 calldata、nonce、事件、Gas、付款人原币余额、账户和已采集的存储。

|任务|报价方向|授权 Gas|兑换 Gas|付款人两笔原币支出合计（wei）|
|---|---|---:|---:|---:|
|F168.1|ExactIn|46074|476599|65968458803684|
|F168.2|ExactOut|46110|497444|68597279195014|

7 项 V2 路径执行测试全部通过，其中包括新增的两笔连续状态回放和原有的 8 笔拆单成功及回滚比对。本地出块记录（`integration/artifacts/local/ordered-approval-168/bank.json`，已清理）

下一项是将已验证的顺序执行引入受限 worker，绑定授权额度、执行目标、费用上限、两笔 nonce 和环境来源，再由报价服务按候选核对总费用。补充资金交易、可信动态 bank 和其他优先项继续开放。

<a id="approval-worker"></a>

## 两笔交易的 worker 核验（2026-11-19，阶段169，P0-3）

§13.1 和 §14 要求把授权操作纳入有序计划。worker 从零授权开始，对照完整计划验证代币、实际 spender、精确额度和同一付款人。第一笔执行成功后，worker 复核代币返回值、更新后的 allowance 和原币费用，并将状态交给第二笔兑换。两笔使用连续 nonce 和相邻区块环境，探针及交易共享 Gas、指令、读取及输入预算。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F169.1|从完整拆单计划重建调用者授权|WP13,WP15|LOCAL_SCOPE_VERIFIED|两种方向生成的 calldata 与出块交易相同。已有足额授权时省略额外交易，非零但不足的授权需要代币替换策略。|
|F169.2|子进程逐笔执行并核对费用|WP15|LOCAL_SCOPE_VERIFIED|两笔费用、实际输入输出、授权后余额与下一笔起始余额相符。错误代码摘要、父区块和 nonce 均拒绝。|

6 项绑定和独立 worker 测试通过，使用两种方向的原始本地区块记录（`integration/artifacts/local/ordered-approval-168/bank.json`，已清理）。报价父进程接收授权序列的费用、完整净额、补充资金交易和可信动态 bank 仍待完成。下一步在父进程核对两笔付款、区块、费用与授权来源，并将合计应用到相应候选。

阶段168的冻结提交 `af504ab` 完成了 88 项命令检查，全部通过。最终汇总读取出块器报告时发现缺少 `passed` 字段，因此本次综合验收结果为失败。综合报告（`integration/artifacts/local/durable-price-router-check/runs/58da093613bd4ce88c227843eaf53521/report.json`，已清理）

出块器现于所有回放断言及冻结绑定摘要校验通过后写入成功标记。定向重新出块的完整 JSON 除新增标记外与原始输入逐项相同。修正后出块报告（`integration/artifacts/local/ordered-approval-168/verified-bank.json`，已清理）

<a id="approval-quote"></a>

## 正常报价计入调用者授权费（2026-11-19，阶段170，P0-3）

§13.1 的原币成本与 §14 的有序计划在 ExactIn 和 ExactOut 的正常报价入口接通。服务端配置指定付款人、卖币、拆单执行器、代币代码摘要和两笔交易环境。父进程从计划重建授权目标和额度，校验逐笔 Gas、付款人及区块受益人的余额，并按付款人实际净支出折算费用。直接路线仍按首个区块估价。拆单的 `gasNative` 指兑换费，`approvalNative` 指授权费，两笔合计记录在 `nativeKnownSubtotal`。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F170.1|在同一报价的候选中比较直接兑换及先授权后拆单|WP13,WP15,WP16|LOCAL_SCOPE_VERIFIED|两种方向按各候选实测 Gas 和签名价格独立核算，两笔费用仅归入对应计划。|
|F170.2|按各候选成交区块检查签名价格有效期|WP13,WP16|LOCAL_SCOPE_VERIFIED|价格在授权后、兑换时到期，两种方向均回退为毛额排序。|

12 个本地 HTTP 场景覆盖有价、预算不足、价格过期、缺执行器、顺序授权和兑换时到期。后续 P0-3 优先处理非零不足授权的替换策略、补充资金交易及其费用，再验收完整候选净额。完整工作包验收仍为 0/26。HTTP 验证报告（`integration/artifacts/local/approval-quote-170/report.json`，已清理）

<a id="reset-approval-replay"></a>

## 非零不足授权的三笔交易回放（2026-11-19，阶段171，P0-3）

为验证 §13.1 费用模型与 §14 有序执行，出块器在独立分支给调用者保留一个最小单位的 allowance。ExactIn 和 ExactOut 分别执行清零、准确额度授权和兑换。每一步保存区块、交易、日志与账户状态，REVM 从上一笔执行得到的状态继续运行。回放逐笔核对 nonce、Gas、日志、原币余额和采集的存储槽。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F171.1|采集非零不足授权时的三笔连续交易|WP13,WP15|LOCAL_SCOPE_VERIFIED|两个方向各有三个连续区块。原两笔序列及此前报告字段逐项相同。|
|F171.2|对照独立出块逐笔执行、累计核算付款人支出|WP15|LOCAL_SCOPE_VERIFIED|V2 路径执行套件 8 项全部通过。|

下一项是将三笔序列接入执行计划、受限 worker 及报价排序。阶段170 的报价库回归为 68 项通过、5 项按测试环境要求跳过。三笔序列出块报告（`integration/artifacts/local/approval-reset-171/bank.json`，已清理）

<a id="reset-approval-plan"></a>

## 拆单授权替换计划（2026-11-19，阶段172，P0-3）

执行计划按 §13.1、§14.1–14.3 读取已绑定的付款人、代币、spender 和所需输入。在选择 `ResetThenExact` 时，非零不足额度生成清零及精确授权两笔操作，零授权生成一笔，足额授权生成零笔。原有单笔执行入口继续使用 `ExactOnly`。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|F172.1|按明确策略构建有序授权计划|WP13,WP15|LOCAL_SCOPE_VERIFIED|两种方向的授权数据、付款方、目标和交易原币金额均与出块记录逐笔相符。篡改执行目标会被拒绝。|

绑定和独立 worker 回归共 7 项通过。下一项是为清零、授权、兑换绑定三个交易环境及 nonce，并让报价服务核对三笔费用。原始本地区块（`integration/artifacts/local/approval-reset-171/bank.json`，已清理）

冻结提交 `48e2489` 的综合回归已通过，覆盖 88 项命令检查、248 次 Rust 测试。阶段170–172 的验证结果分别记录在各阶段条目中。冻结提交验收报告（`integration/artifacts/local/durable-price-router-check/runs/a2ecb98ae8f74422b94a0e59e102abd4/report.json`，已清理）

<a id="reset-approval-worker"></a>

## 拆单 worker 执行三笔交易（2026-11-19，阶段173，P0-3）

worker 在同一仿真预算内，按三个连续区块执行清零、准确额度授权和拆单兑换，将每一笔的结果状态交给下一笔。每次授权后重新读取 allowance，回执记录付款人和受益人的余额与费用。两种方向都与独立出块记录逐笔比对。缺少清零环境、错误 nonce 和不匹配的父区块均触发拒绝。

8 项执行计划和 worker 回归通过。下一项是正常报价端独立核验三个区块的付款连续性，并以合计费用进行候选比较。三笔序列原始区块（`integration/artifacts/local/approval-reset-171/bank.json`，已清理）

<a id="reset-approval-quote"></a>

## 正常报价计入清零费用（2026-11-19，阶段174，P0-3）

对明确配置的非零不足授权模型，报价服务从同一份初始区块状态读取付款人和各区块受益人的余额，按顺序核对三笔交易的目标、额度、nonce、费用和余额。返回的费用明细列出清零、授权和兑换，`approvalNative` 合计前两笔，候选排序使用三笔付款人净支出。若价格在兑换区块到期，整组候选使用毛额排序。

16 个正常 HTTP 场景通过。报价库单元测试 68 项通过，5 项按环境要求跳过。报价与仿真库通过 Clippy 严格检查。P0-3 下一项为资金操作的来源、金额与费用核算。完整工作包验收仍为 0/26。HTTP 报告（`integration/artifacts/local/reset-quote-174/report.json`，已清理）

<a id="sequence-native-funding"></a>

## 付款人起始原币需求（2026-11-19，阶段175，P0-3）

在本地 Shanghai 成交候选中，服务端按执行顺序取“已发生的付款人净扣款加当前 Gas 预留额”的最大值，得到指定零原币转账交易序列的最低起始余额。该值与付款人的初始账户余额核对。`nativeFunding` 同时披露计划绑定、余额来源金额和最终净扣款。对有完整原币余额证据的单笔兑换、授权后兑换和清零后授权兑换均适用。

16 个 HTTP 场景通过。原币费用定向测试 5 项通过，1 项需要单独参考输入。报价本地仿真定向测试 22 项通过，1 项为可选探针。后续资金操作仍需验证原币转账或包装的本金及费用。HTTP 验证报告（`integration/artifacts/local/sequence-funding-175/report.json`，已清理）

<a id="wrapped-input-replay"></a>

## 包装本金与兑换费用回放（2026-11-19，阶段176，P0-3）

出块器部署固定版本的 WETH9、Router02 和拆单合约，把调用者已有的 WETH 保留为一个最小单位。ExactIn 和 ExactOut 均按成交所需数量补足差额，先执行 `deposit()`，再从上一步的状态执行拆单。核对付款人原币余额减少额等于包装本金与两笔网络费之和，并逐笔对照 Gas、日志、nonce、代码及全部采集的账户和存储槽。

9 项 V2 路径执行测试通过。下一项为包装交易的计划绑定、准入资金核算、worker 核验及正常报价接线。独立出块记录（`integration/artifacts/local/weth-wrap-176/bank.json`，已清理）

<a id="wrapped-input-worker"></a>

## WETH 包装计划和 worker 执行（2026-11-19，阶段177，P0-3）

worker 核对 Router02 的 WETH 地址和包装合约运行代码摘要，再读取付款人的 WETH 余额。执行计划依据 ExactIn 卖出量或 ExactOut 实际所需输入计算包装本金。worker 核对 WETH 余额增量、合约收到的原币、付款人支出与区块受益人的收入，再执行兑换。

9 项绑定和 worker 测试通过。两种方向生成的包装交易和两笔 Gas 与独立出块记录相符，错误包装地址、nonce 和父区块均被拒绝。正常报价端的费用核验与排序是下一项。本地区块记录（`integration/artifacts/local/weth-wrap-176/bank.json`，已清理）

<a id="wrapped-quote-funding"></a>

## 正常报价中的 WETH 本金与费用（2026-11-19，阶段178，P0-3）

依据 §13.1、§14.2–14.3，正常 ExactIn 和 ExactOut 报价现在对单路线与拆单执行同一套资金核算。报价服务从源区块 WETH9 存储读取付款人的已有余额，按计划实际输入补足差额。服务分别核对包装本金、两笔交易的网络费、付款人及区块受益人的原币余额。最低起始余额包含包装本金与执行两笔交易所需的峰值预留。排序只折算付款人的网络费，价格须覆盖包装与兑换两个区块。

新增 4 个本地 HTTP 场景，分别核对两种报价方向在有效价格和到期价格下的 4 个候选。有效价格下按折算费用选择最优候选，到期时采用毛额。既有 16 个 HTTP 场景全部通过。报价库测试 68 项通过、5 项按环境忽略，Clippy 严格检查通过。剩余 P0-3 范围包括包装与授权都不足时的组合操作，以及不同链费上下文的完整候选净额。工作包总体验收仍为 0/26。包装报价验收报告（`integration/artifacts/local/wrapped-quote-178/report.json`，已清理）

阶段175提交 `fcf7cb3` 的综合验收另有 89 项检查通过。阶段175综合报告（`integration/artifacts/local/durable-price-router-check/runs/05fdf1fcf60b45379dc3924503c606a2/report.json`，已清理）

<a id="wrapped-approval-replay"></a>

## 包装与授权组合回放（2026-11-19，阶段179，P0-3）

付款人初始持有一个最小单位 WETH。按 §13.1、§14.2–14.3，在授权充足、零授权和非零不足授权三种源状态下分别执行 ExactIn 和 ExactOut。包装按成交所需输入补足差额，零授权时增加精确授权，非零不足时先清零再精确授权。REVM 连续执行同一序列，逐笔对照 Gas、日志及全部采集账户和存储槽。每步付款人累计原币支出等于一次包装本金与截至该步的网络费之和。

9 项 V2 路径测试通过。其中 6 组包装序列覆盖 18 笔交易。下一项为 worker 和正常报价端的组合操作绑定及候选费用核算。本地独立出块记录（`integration/artifacts/local/wrap-approval-179/bank.json`，已清理）

<a id="wrapped-approval-worker"></a>

## 包装与授权的 worker 核验（2026-11-19，阶段180，P0-3）

拆单 worker 在同一仿真预算中执行包装、可选清零、精确授权和兑换。回执摘要同时覆盖包装及授权模型。每笔交易使用前一笔的状态，校验父区块、nonce 和执行环境，核对授权额度、付款人及受益人的余额。独立出块对照覆盖两种报价方向，遗漏中间区块或替换 nonce 等情形均被拒绝。

9 项计划绑定与 worker 测试通过。报价服务的组合费用披露和候选排序待接入。独立出块记录（`integration/artifacts/local/wrap-approval-179/bank.json`，已清理）

<a id="combined-funding-quote"></a>

## 组合资金操作的报价核算（2026-11-19，阶段181，P0-3）

在固定 Shanghai 状态上，ExactIn 与 ExactOut 均可在正常报价时比较四个候选。单路线使用已有 Router 授权，拆单按其额度执行包装、可选清零、精确授权和兑换。服务从源区块核对包装本金，再将实际付款人、包装合约和受益人的余额传给后续费用校验。报价披露每步网络费，以及本金加上执行各步所需的最高资金预留。

12 个包装 HTTP 场景通过，涵盖额度充足、零授权、非零不足及成交区块价格到期。四个候选逐一核对资金需求与成交时估值，价格到期时验证统一毛额回退。报价库 68 项通过、5 项按环境忽略，Clippy 严格检查通过。后续仍需依据正常单路线的实际授权余额选择资金操作，并补齐不同链的费用范围。组合报价验收报告（`integration/artifacts/local/combined-funding-181/report.json`，已清理）

<a id="weth-spender-allowance"></a>

## 按支出合约核对 WETH 授权（2026-11-19，阶段182，P0-3）

WETH 包装报价从固定版本 WETH9 的源存储读取 `allowance[caller][spender]`。单路线核对 Router02，拆单核对批量执行合约。报价进程将其与 worker 实际调用 `allowance` 的结果核对，按当前候选所需输入披露授权是否充足及额外授权费。

12 个 HTTP 场景逐个核对两种方向的四个候选。报价库 68 项测试通过、5 项按环境忽略，Clippy 严格检查通过。下一项为单路线额度不足时执行精确授权及可选清零。验收报告（`integration/artifacts/local/spender-allowance-182/report.json`，已清理）

<a id="router-wrap-approval"></a>

## 单路线包装与授权费用（2026-11-19，阶段183，P0-3）

在固定 Shanghai 区块，报价服务根据 Router02 与批量执行器各自的 WETH9 额度，为每个候选选择资金操作。额度为零时授权实际所需输入，额度非零且不足时按配置先清零，再精确授权。单路线和拆单各自绑定 spender，父进程核对每笔收支、预留资金和成交时价格。

新增 Router02 额度不足的本地独立出块记录。20 个正常报价 HTTP 场景全部通过，覆盖 ExactIn、ExactOut、不同 spender 的额度差异和价格到期。报价库 68 项通过、5 项按环境忽略，Clippy 严格检查通过。独立出块记录（`integration/artifacts/local/router-approval-183/bank.json`，已清理） · HTTP 验收报告（`integration/artifacts/local/router-approval-183/http/report.json`，已清理）

<a id="quote-funding-ui"></a>

## 报价页的资金需求（2026-11-19，阶段184，P0-3）

报价页按选中计划和执行回执的绑定摘要核对资金信息。ExactIn 和 ExactOut 均以 wei 展示付款人网络费、包装本金、预计原币支出和最低起始余额，并标明本次排序是否使用已折算的网络费。金额以整数原子单位校验。

24 项前端测试通过。另用 20 个正常报价响应核对展示值，并在 Chromium 中通过实际 Router 请求核对两种报价方向的页面金额。浏览器验收报告（`integration/artifacts/local/quote-funding-ui-184/run-nkFhvg/report.json`，已清理）

<a id="ethereum-withdrawals-integrity"></a>

## Ethereum 主网提款列表完整性（2026-11-19，阶段185，P0-1）

复核设计 §7.3、§28.5、§28.7 时，发现普通 Ethereum 完整区块读取和离线证据验证都接受只修改提款列表的响应。两条路径现按主网 Shanghai 激活条件检查列表与根的存在性，从每项完整字段按原顺序重建提款树，校验区块头中的根。空列表与遗漏列表分别处理。读取使用既有完整区块响应。生产来源资格仍需独立可信的共识锚点。

固定主网块 26355865 含 101 笔交易及 16 条提款。Go `-race -count=1` 专项通过 52 项（含子测试），覆盖实际 RPC 读取、列表遗漏、对调、空条目、分叉边界、缺少取值为零的字段及 17 条输入。`source-evidence -verify` 对仓库中的原始证据回放成功，相关 Go 包 `go vet` 通过。专项测试记录（`integration/artifacts/local/ethereum-body-185/test.jsonl`，已清理） · 离线回放报告（`integration/artifacts/local/ethereum-body-185/replay.json`，已清理）

阶段183综合验收已结束，92 个门禁通过，Rust 测试执行 253 次，Go 测试执行 82 次，最终状态为 `LOCAL_SCOPE_VERIFIED`。该报告对应提交 `cf1eee9`。阶段184前端验收见上一节。阶段183最终报告（`integration/artifacts/local/durable-price-router-check/runs/e8c1d92b128e4f0598750fbeb0481b90/report.json`，已清理）

<a id="ethereum-offline-transaction-body"></a>

## Ethereum 主网离线交易正文复核（2026-11-19，阶段186，P0-1）

按 §7.3 和 §28.5，离线证据中的区块交易列表现在与已由交易根验证的原始签名交易逐笔核对。完整对象须符合签名交易内容、交易编号、块号、块哈希及位置。含 `from` 的记录还从签名按当时的主网规则恢复发送人。哈希列表按相同顺序核对。历史仅保存区块头和原始交易的证据继续核对交易根。

Go 证据包 `-race -count=1` 共通过 48 项测试（含子测试），`go vet` 通过。普通 `source-evidence -verify` 回放块 26355865 的 101 笔及块 26360354 的 248 笔交易成功。额外输入 `prestate-evidence.json` 含两个空代码账户，部署证据 CLI 以 `PROVENANCE_DEPLOYMENT_CODE_HASH` 拒绝。P0-1 的独立共识锚点、重组撤权与跨链恢复仍在跟踪。GitLab 项目克隆地址待提供。专项测试、回放与拒绝记录（`integration/artifacts/local/mainnet-transaction-body-186/`，已清理）

<a id="beacon-checkpoint-execution"></a>

## 信标检查点与执行区块（2026-11-19，阶段187，P0-1）

设计 §7.1、§7.3、§28.5 要求核对来源并披露确认依据。`source-evidence --verify` 新增配对参数 `--beacon-root` 和 `--beacon-bootstrap`。工具重算 beacon header 根，核对执行载荷及同步委员会的两条 SSZ 分支，随后核对执行块哈希并运行原部署证明验证。报告保留显式检查点、slot、fork、委员会根与执行哈希。当前历史根取自公开主网信标 API，其信任来源和新鲜度仍待独立验证，`consensus_verified` 保持 false。

实际 Fulu slot 15595392 对应主网块 26355865，101 笔交易与部署证据通过。12 个 CLI 场景覆盖正常响应、替换根、错误证据、分叉、时间、字段、载荷及委员会分支。Go 证据包 48 项 race 测试和 `go vet` 通过。仓库的模块校验器通过 242 个外部模块校验，并识别本地 protobuf 模块。直接调用 `go mod verify` 对该本地模块报告缺少 ziphash。历史回放与拒绝记录（`integration/artifacts/local/beacon-checkpoint-187/report.json`，已清理） · 检查点绑定报告（`integration/artifacts/local/beacon-checkpoint-187/verified.json`，已清理）

<a id="beacon-committee-finality"></a>

## 同周期同步委员会最终性验证（2026-11-19，阶段188，P0-1）

按设计 §7.1、§7.3、§28.5，`source-evidence --verify` 支持读取标准 `finality_update`。工具从显式信标根验证当前委员会，核对签名 slot、主网分叉域、参与位图与三分之二门槛，验证 BLS 聚合签名。再从 attested header 的状态根核对最终化分支，将 finalized header 的执行载荷与部署证据核对。报告单列检查点、最终化 slot、签名 slot 与人数。当前支持同一分叉和委员会周期，跨期交接、运维可信根配置及在线 owner 接入仍开放。

本轮采集的 Fulu slot 15687904 对应块 26448094，100 笔交易。508 名签署者的聚合签名和 Pair 账户证明通过。主查节点首次返回 RPC 错误 4444，交换已配置的两个只读来源后取得同一最终化块的完整证据。13 个最终性 CLI 场景、阶段187的 12 个场景、48 项 Go race 测试与 `go vet` 通过。最终性回放与输入摘要（`integration/artifacts/local/beacon-finality-188/report.json`，已清理） · 部署验证报告（`integration/artifacts/local/beacon-finality-188/verified.json`，已清理）

<a id="beacon-committee-transitions"></a>

## 同步委员会跨期交接（2026-11-19，阶段189，P0-1）

按 §7.1、§7.3、§28.5，离线部署验证支持标准 `light_client/updates` 响应中的连续交接。每项验证所在周期、同分叉的最终化和 attested 载荷、最终化分支、下一委员会分支及当前委员会超多数签名，然后以已证明的下一委员会校验下一项。输入上限为 16 个周期和 2 MiB。报告按序记录委员会根及签名人数。

实际 Fulu 周期 1913 和 1914 的连续交接后，周期 1915 的签名和最终化执行块 26448094 均通过。10 项交接回放覆盖跳过、重复、提前结束、委员会或分支替换、签名与门槛。原同周期 13 项回归、Go 证据包 48 项 race 测试与 vet 通过。跨分叉、可信检查点的发布与更新、在线 owner 验证和重组撤权仍待完成。交接验收（`integration/artifacts/local/beacon-transition-189/report.json`，已清理）

<a id="beacon-selected-capture"></a>

## 从最终性证据选取执行块（2026-11-19，阶段190，P0-1）

§7.1、§7.3 和 §28.5 要求明确每次读取的链和区块。`source-evidence` 捕获模式现可先验证显式信标根、委员会更新和最终性签名，再从已验证载荷取执行高度。首个完整区块响应须匹配该高度及哈希，随后才读取合约代码与账户证明。见证来源的同高复核和最后一次主查复核仍参与采集。

普通 CLI 从周期 1913 的指定根，经两次交接和 510 名参与者的最终性签名，选定主网块 26448285，完成 151 笔交易及原 Pair 的部署证明验证。采集报告与离线重放逐字节相同。较早块 26448094 的 RPC 采集尝试返回 -32000，本轮改用新取得的已签名更新。错链和冲突高度在首次 RPC 前失败，同高异块在第三次 RPC 后失败。53 项 Go race 测试、23 项历史 CLI 场景和 vet 通过。采集报告（`integration/artifacts/local/beacon-capture-190/live.json`，已清理） · 离线报告（`integration/artifacts/local/beacon-capture-190/offline.json`，已清理）

阶段189提交 `c1870c8` 的完整来源、EVM 与服务包 race 回归已结束，三个包分别通过 48、2914 和 230 项测试（含子测试）。完整回归记录（`integration/artifacts/local/beacon-transition-189/owner-evm-regression.jsonl`，已清理）

<a id="beacon-owner-admission"></a>

## 普通 owner 的信标最终性校验（2026-11-19，阶段191，P0-1）

按 §7.1–7.3、§7.9 和 §28.26，Go owner 的 Ethereum 主网 native profile 可配置检查点根、证明文件和检查点年龄上限。每次同步读取一份完整证明，验证签名、委员会和最终化载荷，按载荷高度读取执行层。发布时用同一份证明检查快照哈希与时效。短缺口在提交前从已验证的头向后检查父块。后退或同高冲突进入现有撤权路径。

主网 DAI/WETH 池在块 26448349 首次发布。替换证明后基线接口返回 503，恢复原证明后同块重新发布，日志序号从 1 增至 3。首次测试脚本使用了错误的 HTTP 高度字段，修正为 `number_or_slot` 后重新运行了完整场景。验收使用公开信标来源提供的检查点。生产运维的独立可信根来源和轮换流程仍待验收。普通进程验收记录（`integration/artifacts/local/beacon-owner-191/live-2/report.json`，已清理）

本轮配置、来源证明和 owner 服务的完整 Go race 回归分别通过 309、58 和 230 项（含子测试），三个包的 `go vet` 通过。回归记录（`integration/artifacts/local/beacon-owner-191/go.jsonl`，已清理）

<a id="beacon-owner-refresh"></a>

## 普通 owner 的信标证明刷新（2026-11-19，阶段192，P0-1）

Go 命令 `chain-parser --config <owner 配置> --refresh-beacon-proof <HTTPS 信标 API>` 从发布配置取得可信根、目标文件与时效预算，读取 bootstrap、finality_update 及所需周期的 updates。验证签名、最终化分支与时效后，持文件锁复核已安装证明，拒绝高度或 slot 后退及同高冲突。写入使用同目录临时文件、文件同步、重命名和目录同步。

实际主网块 26448381 的证明先后两次通过 CLI 刷新。从两份保留的历史更新重放正常推进、同块重复、回退、无效数据、并发竞争、过期及取消。五个失败场景均逐字节核对了原文件。Go 证据包 64 项 race 测试及 vet 通过。主网刷新报告（`integration/artifacts/local/beacon-refresh-192/refresh.json`，已清理） · 回归记录（`integration/artifacts/local/beacon-refresh-192/go.jsonl`，已清理）

<a id="beacon-owner-ancestry"></a>

## 主网短尾祖先块校验（2026-11-19，阶段193，P0-1）

普通 Go owner 从签名最终化头反向核对短尾时，按精确高度请求 `eth_getBlockByNumber(height, false)`，重算执行头哈希并沿父哈希连接已有游标。该路径沿用整段追块时限、租约截止时间和来源观测。正向读取仍验证完整区块、回执和池状态，发布时逐高度核对反向确认的哈希。

使用主网块 26355865 回放正常响应、父哈希篡改、错误高度、浮动标签、非规范数量、取消和未配置证明七种情况。服务回归使用已验证的信标证明和构造的中间链节，通过正常发布门禁检验接受与拒绝。相关包通过 `go vet`。服务包首跑的 V3 模板重启用例超时，该用例和本轮用例在 `-count=1 -race` 独立复跑通过。后续优先实测同一 owner 在相邻最终化头之间逐块发布。定向回归（`integration/artifacts/local/beacon-ancestor-193/focused.log`，已清理） · 全包首跑（`integration/artifacts/local/beacon-ancestor-193/tests.jsonl`，已清理）

<a id="beacon-owner-advance"></a>

## 相邻最终化头连续发布验收（2026-11-19，阶段194，P0-1）

`verify_beacon_owner.py` 新增 `--refresh-endpoint`，从首个已验证快照开始，调用正常 CLI 刷新证明，核对每条 `BLOCK_COMMIT` 的高度、父哈希、日志序号、代际、作用域和终点基线。`--next-proof` 可安装另一份签名证明重放。验证器通过连续发布、父哈希冲突、代际变化、重新建立基线和终点基线冲突测试。

旧主网起点 26448349 的公开状态请求失败。改用新鲜证明后，owner 在 26448795 发布 DAI/WETH 基线，CLI 获取最终化块 26448827。日志在首个连续增量前记录 `RECEIPT_PROOF_BUDGET` 并撤权，验收器报 `OWNER_JOURNAL_DISCONTINUITY`。单独读取块 26448796 的交易体使用 5,232,897 字节预算，核对回执和见证后剩余 14,811,711 字节。下一步减少已验证交易体的重复解码与获取，再运行同一发布验收。阶段193的 service 包 231 项 race 用例复跑通过。EVM 全包首跑在 10 分钟总时限终止，定向回归已通过。运行日志（`integration/artifacts/local/beacon-advance-194/run-2/owner.log`，已清理） · 预算诊断（`integration/artifacts/local/beacon-advance-194/budget.log`，已清理） · service 包复跑（`integration/artifacts/local/beacon-ancestor-193/service-retry.jsonl`，已清理）

<a id="beacon-witness-budget"></a>

## 主网回执见证预算（2026-11-19，阶段195，P0-1）

每个 Ethereum 主网见证节点先提供完整交易体，回执读取使用本节点刚取得的原始交易重算交易根。完成回执验证后再向该节点读取完整区块核对锚点。两个节点分别回读两次完整交易体，任一节点缺失回执即拒绝。相关 41 项 race 测试和 `go vet` 通过。

在同一 64 MiB 配置下，对块 26448796 完成回执见证后可用预算由 14,811,711 升至 20,877,746 字节，对块 26448827 则由 33,123,702 升至 37,321,993 字节。普通 owner 从新鲜基线 26448987 向 26449019 推进，依序发布 26448988 和 26448989。随后日志记录 `RECEIPT_WITNESS_INVALID` 并撤权，完整的 32 块短尾验收仍为 `PARTIAL_LOCAL`。下一轮优先在块 26448990 区分来源读取、字节预算与回执内容校验的错误原因，再复验整个区间。

主网读取预算（`integration/artifacts/local/beacon-witness-195/budget.log`，已清理） · 定向回归（`integration/artifacts/local/beacon-witness-195/regression.log`，已清理） · owner 运行日志（`integration/artifacts/local/beacon-witness-195/live-2/owner.log`，已清理）

<a id="beacon-witness-diagnostics"></a>

## 原生回执见证错误定位（2026-11-19，阶段196，P0-1）

普通 owner 的 `chain_sync_failed` 日志可定位见证交易体读取、回执核验、最终交易体回读及结算复核的失败，记录健康见证集合中的序号、块高和原因分类。双节点测试覆盖第二个见证缺失回执，并检查预算、限流、截止时间、取消和无效内容的分类。相关包通过 vet。

块 26448990 的双节点回读核验了 275 笔交易及回执。回执核验后剩余 34,625,608 字节，该次复测成功。此前线上失败的具体原因尚待观测。新鲜签名块 26449307 的 owner 启动返回 `RECEIPT_PROOF_BUDGET`，可用此块度量各步骤消耗。阶段195的 EVM 全包延长测试期限后通过，共 2,923 项通过、21 项跳过。完整短尾验收和可信检查点发布仍为待办。

块 26448990 回读（`integration/artifacts/local/beacon-witness-196/diagnostic.log`，已清理） · 错误分类回归（`integration/artifacts/local/beacon-witness-196/controls.log`，已清理） · 启动记录（`integration/artifacts/local/beacon-witness-196/live/owner.log`，已清理） · EVM 全包结果（`integration/artifacts/local/beacon-witness-195/full.jsonl`，已清理）

<a id="native-body-encoding-budget"></a>

## 原生交易原文与验证预算（2026-11-19，阶段197，P0-1）

取得完整原生区块后，解析器保留已检查位置和类型的签名交易原文，按交易顺序验证哈希及交易根，并把同一份原文交给后续回执核验。主网块 26449307 共 407 笔交易，配置预算为 64 MiB。区块交易体预算由 6,652,717 降至 4,233,601 字节。回执和最终锚点复核后，剩余预算由 5,838,429 增至 20,353,125 字节。Arbitrum 固定输入的精确开销降至 72,791 字节，少给一字节时拒绝发布。56 项 race 回归及 vet 通过。

同一旧块的 owner 状态读取返回 `RPC_ERROR_-32000`。用新鲜证明启动时，从 26449402 连续发布至 26449404，目标为 26449434。后续源请求返回 HTTP 408，owner 撤权。该窗口的连续发布验收仍为 `PARTIAL_LOCAL`。下一步把现有 RPC 有界重试策略接入普通在线配置并复验瞬时超时。

改动前（`integration/artifacts/local/beacon-body-197/before.log`，已清理） · 改动后（`integration/artifacts/local/beacon-body-197/after.log`，已清理） · 预算回归（`integration/artifacts/local/beacon-body-197/regression-fixed.jsonl`，已清理） · 普通 owner 日志（`integration/artifacts/local/beacon-body-197/live-2/owner.log`，已清理）

<a id="owner-rpc-retry"></a>

## 普通读取的 RPC 重试（2026-11-19，阶段198，P0-1）

普通 Go owner 和历史读取可从 `chains[].rpc_retry` 加载总尝试次数和带抖动的退避区间。每个已绑定见证节点的读取继承配置。主网验收使用 3 次总尝试，基础退避 150 毫秒，上限 1500 毫秒。正常服务构造后的测试验证了单节点 HTTP 408 后成功、429 用尽次数、400 立即终止和缺省单节点行为。配置、RPC 和服务包共 21 项定向 race 回归及 vet 通过。

主网 owner 以 26449658 为基线。证明刷新至 26449722 后发生 `RPC_ERROR_-32000`，owner 撤权，对该区间的连续发布验收仍为 `PARTIAL_LOCAL`。

回归结果（`integration/artifacts/local/beacon-retry-198/go.jsonl`，已清理） · owner 日志（`integration/artifacts/local/beacon-retry-198/live/owner.log`，已清理）

<a id="v2-verified-account-batch"></a>

## V2 池账户证明共用执行锚点（2026-11-19，阶段199，P0-1）

同一池的首次账户读取重新验证该区块的回执证据与业务范围，复制执行状态根。该池内后续账户和存储键的证明共享执行锚点与总预算。每次 `eth_getProof` 仍逐项核对地址、存储路径和 Merkle 根。单独调用的账户读取各自验证回执证据。批次测试检查了两次独立证明读取、第二次读取的预算下降、篡改证据时独立调用拒绝及已拒绝会话的后续处理。39 项 race 回归及 vet 通过。

上一轮首个增量 26449659 的独立复查中，两端点返回同一区块哈希。dRPC 对账户证明返回历史状态不可用，PublicNode 返回该块超过证明窗口，错误码分别为 `-32000`、`4444`。新鲜主网窗口以 26449753 为基线，目标为 26449785。owner 连续发布至 26449761，共 8 个增量，随后源请求返回 `RPC_ERROR_-32000` 并撤权。完整连续发布待具有对应历史状态的输入复验。

来源复查（`integration/artifacts/local/beacon-account-199/historical-availability.json`，已清理） · 批次回归（`integration/artifacts/local/beacon-account-199/batch.log`，已清理） · 池验证回归（`integration/artifacts/local/beacon-account-199/tests.jsonl`，已清理） · 主网运行日志（`integration/artifacts/local/beacon-account-199/live/owner.log`，已清理）

<a id="beacon-owner-archive"></a>

## 跨最终化头发布的归档核验（2026-11-19，阶段200，P0-1）

连续发布验收启用已有的普通 owner 双块并行读取和有序提交通道。每次验收归档限制为 4 GiB、10,000 条、单条负载 128 MiB。报告逐条保存已核对的连续提交和签名目标。归档审计验证文件 ID、封装与内容哈希，将节点响应按端点核对，将派生块与提交引用核对。每个已发布高度检查两端点的区块体及同哈希下 Pair、Factory、两种 Token 的证明响应。MPT 与签名仍由普通 owner 验证。

主网起始块 26449944，签名目标 26449976，最后连续提交 26449962。583 条记录共 108,580,577 字节，18 条派生块与发布引用相符。owner 在并行读取高度 26449964 时报告 `RPC_HTTP_429`，验收观察到日志中断。dRPC 原始响应对 `eth_getCode`、`eth_chainId`、`eth_getBlockByNumber` 报告公共端点限流（码 15）。已发布 19 个高度的归档覆盖检查全部通过，32 块发布仍待完整验收。5 项信标验收工具测试、11 项来源证据测试和 Go 服务相关 race 回归通过。

主网报告（`integration/artifacts/local/beacon-archive-200/live/report.json`，已清理） · 普通 owner 日志（`integration/artifacts/local/beacon-archive-200/live/owner.log`，已清理）

<a id="pool-proof-endpoint"></a>

## 池证明读取期间的节点身份（2026-11-20，阶段201，P0-1）

DAI/WETH 每块需要对不同账户和存储键多次请求 `eth_getProof`。同一池的同步读取首次核验节点的链 ID 与创世块，随后使用相同端点时复用该身份。每次读取仍选取健康节点、绑定单个端点并消耗当前区块的剩余预算。切换节点时重新核验链 ID 和创世块。独立账户读取和新池批次各自建立身份。两次连续的独立 MPT 查询在测试中各成功一次，合计请求一次链 ID 和一次创世块。23 项专项 race 测试（含子测试）与 EVM、RPC 的 vet 通过。

主网基线 26450264，目标 26450296，连续发布至 26450270。256 条归档共 45,063,808 字节，已发布 7 个高度的覆盖检查通过。26450271 的并行读取报告 `RPC_ERROR_-32000`。dRPC 对该窗口内两个哈希的 Pair 存储证明返回历史状态不可用。PublicNode 对完全相同的两次 `eth_getProof` 请求均返回错误码 4444，指明超过证明窗口。完整跨头发布仍需要能提供这些历史状态证明的节点。

专项回归（`integration/artifacts/local/beacon-endpoint-201/focused.log`，已清理） · 主网报告（`integration/artifacts/local/beacon-endpoint-201/live/report.json`，已清理） · 第二端点复查（`integration/artifacts/local/beacon-endpoint-201/secondary-proof-check.json`，已清理）

<a id="beacon-owner-execution-config"></a>

## 连续发布验收的归档节点配置（2026-11-20，阶段202，P0-1）

使用 `verify_beacon_owner.py --execution-config <owner配置文件>` 可从现有 JSON 配置读取 `evm:1` 的两个执行节点。工具核对主网创世哈希和两个不同的 HTTPS 地址，再沿用签名检查点、原生区块验证、逐块发布和停止后的归档核对流程。可将归档节点列在 `rpc_urls` 首位以承担账户证明读取。结果目录权限为 0700，生成的 owner 配置为 0600。公开报告中自定义节点使用来源 ID，错误只取数字码，完整响应保存在本地归档。

6 项工具测试通过。以阶段 201 的 owner 配置核对其 256 条归档与 7 个发布高度，结果与当次的归档报告一致。尚需实际可用的主网归档节点配置完成 32 块连续发布。GitLab 交付仍需项目克隆地址，当前仓库唯一远端为此前授权的 GitHub origin。

<a id="owner-window-replay"></a>

## 从主网节点响应重建已发布区块（2026-11-20，阶段203，P0-1）

在 `chain-parser-go` 下构建 `./cmd/replay-beacon-window`，运行 `integration/tools/acceptance/replay_beacon_window.py <二进制> <已停止的owner验收目录> <可信检查点根>`。回放工具按端点、方法和请求参数索引已封装的 RPC 响应，从签名目标沿执行头回溯祖先。普通 Go 解码器逐块读取交易、回执、账户和存储证明，从中重算池状态与兑换事实。每个增量的块引用、代际、发布序号和派生负载 SHA-256 均须与原记录相符。基线只比对块引用并记录重算结果。缺少读取或成功响应互相矛盾会使回放失败。历史重算只在本地运行，错误响应另行计数。

26449944–26449962 和 26450264–26450270 两段合计 26 个高度通过，其中 24 个增量的派生哈希全部相符。两个窗口分别执行 1,022 次与 398 次本地 RPC 读取。原归档分别有 3 条限流响应、2 条历史状态错误。完整 32 块在线发布仍依赖可提供相应证明的主网节点。两项新增回归涵盖响应歧义、未记录请求、派生内容篡改和代际改变，Go 工具构建与 vet 通过。

18 个增量重算报告（`integration/artifacts/local/beacon-replay-203/window-200.json`，已清理） · 6 个增量重算报告（`integration/artifacts/local/beacon-replay-203/window-201.json`，已清理）

<a id="beacon-fork-handover"></a>

## 同步委员会跨分叉交接（2026-11-20，阶段204，P0-1）

从 Electra 的显式主网检查点验证周期 1606 获取的下一委员会，再验证该委员会对 Fulu 执行块 23942645 的最终性签名。验证器按 attested header 的时隙选择状态证明索引，按各执行载荷所属时隙校验头部分支，签名域沿用签名时隙前一时隙的分叉版本。Fulu 更新提供的下一委员会分支也经实际数据核对。两份固定输入随仓库提供，回归可离线运行。

主网块 23942645 的哈希为 `0x4417a28d346e978a8e77128d3596a355c20d87ca18ebd126276933895cace473`。错误的更新版本、缺少委员会交接和下一委员会分支篡改均被拒绝。另一个历史 Deneb 样本通过了检查点根与执行载荷校验，在当前委员会的 Merkle 分支校验时拒绝。Deneb 至 Electra 的完整正例仍需能通过该校验的起始证据。来源验证包 66 项 race 回归（含子测试）及 vet 通过。

[固定输入说明](integration/test/source/beacon-fork-handover/README.md) · [专项测试](chain-parser-go/internal/provenance/beacon_fork_handover_test.go) · 包回归（`integration/artifacts/local/beacon-fork-204/go.log`，已清理）

<a id="beacon-checkpoint-rotation"></a>

## 主网信标检查点轮换（2026-11-20，阶段205，P0-1）

普通 Go CLI 使用 `--config <旧配置> --prepare-beacon-rotation <后继配置> --beacon-bootstrap <后继根的bootstrap响应>` 准备新版本的证明。工具持有旧版本的刷新锁，从旧配置指定的可信根验证最新签名，并要求后继配置指定的根恰为该签名确认的最终化信标块。随后用后继委员会重新验证同一最终性证明，复核两份配置的时效限制，向新路径独占发布。下一次启动可直接读取该证明。历史输入随仓库提供。

执行块 26450712、信标时隙 15690528 的实测包含 511 名签署者。旧版本从原检查点经两轮交接验证，新版本从最终化根直接验证。CLI 输出的最终化结果逐字节相同。75 项 race 回归及 vet 通过，覆盖配置根错误、bootstrap 不匹配、各自时效到期、取消、刷新锁竞争及目标文件已存在。首个生产可信根的独立获取、部署版本实际切换和跨区恢复仍待验收。

主网轮换结果（`integration/artifacts/local/beacon-rotation-205/rotation.json`，已清理） · [固定输入回归](chain-parser-go/internal/provenance/beacon_rotation_test.go)

<a id="beacon-rotation-switch"></a>

## 普通 owner 使用后继检查点启动（2026-11-20，阶段206，P0-1）

`verify_beacon_rotation.py <普通owner二进制> <验收配置模板> <新输出目录> <信标API>` 使用独立的本地状态目录刷新旧证明，获取已签名最终化根的 bootstrap 并生成后继配置。旧 owner 发布基线后退出，新 owner 使用同一存储和后继证明启动。验收读取新一代的 `ONLINE_BASELINE_COMMIT`，核对链、区块、代际、写入者版本、日志序号以及全部池状态。

两次主网读取均返回区块 26451448 的 DAI/WETH 池状态。签署者 511 名，代际、写入者版本和日志序号均从 1 递增至 2。7 项信标工具回归通过。独立初始根的运维供应和生产多节点切换仍待验收。

主网切换报告（`integration/artifacts/local/beacon-switch-206/report.json`，已清理） · [运行工具](integration/tools/acceptance/verify_beacon_rotation.py)

<a id="account-proof-rpc-routing"></a>

## 普通 owner 的账户证明来源（2026-11-20，阶段207，P0-1）

按设计 §7.1，在 Go 链配置中设置 `proof_rpc_url`，可让专用节点承担 `eth_getProof`。该节点先报告链 ID 和创世块。每个账户及存储证明依然按请求的规范块哈希和执行状态根验证。节点沿用当前读取的超时、字节上限、重试和响应归档。省略配置时，普通 owner 沿用现有节点。离线回放为各来源创建对应的本地服务，两个区块体见证仍单独核对。

Ethereum 主网块 26452499 的归档包含 dRPC 和 PublicNode 的区块体及回执，以及 Alchemy 的 6 个账户证明响应。普通 owner 核验 Pair、Factory、DAI 和 WETH 后发布基线，归档审计缺口为 0。用 Alchemy 承担整块读取时，`eth_getBlockReceipts` 返回 HTTP 401。错误链 ID、错误创世块、损坏的 MPT 节点及错误来源的归档回归均通过。Go 全包测试、`go vet ./...`、证明路由和配置/RPC 专项 race 测试通过。完整 32 块窗口和重放单独验收。当前已授权推送目标为 GitHub `origin`，GitLab 项目地址待提供。

主网基线归档审计（`integration/artifacts/local/beacon-proof-routing-207/first-audit.json`，已清理） · 整块读取失败报告（`integration/artifacts/local/beacon-archive-providers-207/live/report.json`，已清理） · Go 全包回归（`integration/artifacts/local/beacon-proof-routing-207/go-all.log`，已清理）

<a id="beacon-complete-window"></a>

## Ethereum 主网连续发布 32 块及归档重放（2026-11-20，阶段208，P0-1）

阶段 207 的首次运行发布了 3 个增量。块 26452503 含 300 笔交易，两个见证均返回完整回执。旧程序从封存响应稳定复现 `RECEIPT_PROOF_BUDGET`。按设计 §7.1，Ethereum 见证在回执核对后再次读取同一高度的完整块体，若响应字节与该见证刚验证的块体完全一致，复用已验证的交易结果。响应内容有变化时重新解码并验证。每次读取继续计入同一预算。原失败块经同一批归档响应在 64 MiB 上限内通过。专项 race 测试覆盖重读次数、等价响应和损坏的状态根。Go 全包测试及 vet 通过。

普通 owner 在新窗口从块 26453011 建立基线，到签名最终化块 26453043 共发布 32 个连续增量。逐块核对父哈希、代际、日志序号及终点基线。三个 RPC 来源共归档 909 条记录，覆盖缺口和 RPC 错误均为 0。离线重算 33 个高度，每个增量的池状态和兑换事实哈希均与原始归档匹配。P0-1 仍需完成其余链与协议矩阵、来源资格和恢复验收。GitLab 推送待项目地址。

在线发布报告（`integration/artifacts/local/beacon-proof-window-208/live/report.json`，已清理） · 离线重算（`integration/artifacts/local/beacon-proof-window-208/replay.json`，已清理） · 原失败块重算（`integration/artifacts/local/beacon-proof-routing-207/replay208-26452503.json`，已清理） · Go 回归（`integration/artifacts/local/beacon-proof-window-208/go-all.log`，已清理）

<a id="op-mainnet-withdrawals"></a>

## OP Mainnet 提款字段与 Pair 证明（2026-11-20，阶段209，P0-1）

按设计 §7.1、§7.3 和 §28，普通 Go 解析器按 OP Mainnet 的 Canyon 和 Isthmus 激活时间核验提款列表与头部字段。Canyon 要求空列表和空提款树根。Isthmus 要求空列表和空请求哈希，提款根按消息传递合约存储根解释。固定主网块 159769967 的回归重建了 66 笔交易、完整回执根及 USDC/WETH Pair 的账户和七项存储证明。真实块保留原头部哈希但更换提款列表时，解码器拒绝。Go 全包测试、专项 race 和 vet 通过。

普通 owner 起初在批量回执方法收到 HTTP 403。改用 PublicNode 与 Tenderly 读取区块体和回执后，OP 主网公共节点对 Factory 的历史账户证明返回 `-32603`。改用 Tenderly 读取账户证明后，普通 owner 在 Token 余额验证报告 `STATE_VIEW_OPCODE_UNSUPPORTED`，启动未发布基线。下一项是从该次响应定位具体 Token、执行路径及所需账户证明。L2 共识来源、消息传递合约存储根的 MPT 核验以及完整池状态仍待验收。

[固定主网输入及复验说明](integration/test/source/op-mainnet-withdrawals/README.md) · 两来源 owner 日志（`integration/artifacts/local/optimism-v2-209/witness.log`，已清理） · 使用 Tenderly 证明的 owner 日志（`integration/artifacts/local/optimism-v2-209/tenderly/owner.log`，已清理） · Go 全包回归（`integration/artifacts/local/optimism-v2-209/go-final.log`，已清理）

<a id="op-usdc-delegate"></a>

## OP Mainnet 固定部署的 USDC 代理余额（2026-11-20，阶段210，P0-1）

普通 Go owner 在块 159770518 核对代理运行时代码、实现地址槽、实现账户及运行时代码，再用代理账户已证明的存储值执行 `balanceOf`。只允许代理向该实现成功委托一次，最多读取 8 个不同的存储槽，单次执行 gas 上限为 100000。固定部署升级后须重新核验代码才可使用。

同块的旧 owner 返回 `STATE_VIEW_OPCODE_UNSUPPORTED`。新 owner 发布的 Pair 余额为 250054139 个 USDC 最小单位和 144299348285428149 wei WETH。固定输入支持离线重算 USDC 余额。逐个移除已读槽、换错目标、嵌套或重复委托，以及子调用回滚后代理仍返回一个字的用例均已覆盖。

`go test -count=1 ./...`、`go vet ./...` 以及只读执行器和 OP 固定输入的专项 race 测试均通过。

P0-1 下一项是从执行头状态根核验 `L2ToL1MessagePasser` 账户证明，并在普通读取与归档重放中比对 Isthmus `withdrawalsRoot`。OP 的 L1 派生与最终性、重组撤权和恢复仍待验收。GitLab 推送等待项目克隆地址，当前 `origin` 指向 GitHub。

[固定输入与复验命令](integration/test/source/op-mainnet-withdrawals/README.md) · 普通 owner 基线（`integration/artifacts/local/optimism-v2-209/delegate/baseline.json`，已清理） · Go 验证日志（`integration/artifacts/local/op-usdc-210/`，已清理）

<a id="op-message-passer"></a>

## OP Mainnet 普通合约状态读取验证 Isthmus 提款根（2026-11-20，阶段211，P0-1）

每批普通合约状态读取从重验的执行头取得 Isthmus `withdrawalsRoot`，通过已核验身份的证明节点，在同块 `stateRoot` 下验证 `L2ToL1MessagePasser` 的账户 MPT。节点返回的账户必须存在，证明所得存储根必须与头部值相等。验证复用当前批次的超时、块哈希选择器和字节预算。读取纯回执时只核验头部字段。

离线固定块 159769967 通过了普通账户读取。替换消息传递合约的账户证明会终止读取会话，错误的头部存储根也被拒绝。在线普通 owner 发布块 159771104 的 USDC/WETH 基线。随后用同一二进制和新的状态目录重放归档，两处本地 RPC 分别服务 45 和 26 次请求，重算的区块及完整池状态均与基线相同。Go 全包测试、vet、专项 race 和两项既有 Python 重放回归通过。

P0-1 下一项是把 OP 的 L2 来源与可信 L1 最终化证据绑定，再验证重组后的撤权与恢复。原 26 个工作包完整验收仍为 0。GitLab 交付需要项目克隆地址。

主网基线（`integration/artifacts/local/optimism-v2-209/message-passer/baseline.json`，已清理） · 归档重放报告（`integration/artifacts/local/op-passer-211/replay.json`，已清理） · [固定输入说明](integration/test/source/op-mainnet-withdrawals/README.md)

<a id="op-signed-l1-origin"></a>

## 从 OP 块追溯签名确认的以太坊来源（2026-11-20，阶段212，P0-1）

`verify-op-origin` 从调用方指定的主网信标检查点验证同步委员会交接与最终性签名，按父哈希向前校验最多 128 个区块。它从 OP 区块重新编码交易并验证交易根，检查首笔 L1 信息存款的地址、分叉格式与来源哈希，再将其中的高度、哈希、时间戳与对应的以太坊头部比对。在线费用读取与该命令共用打包属性解码。

固定输入中 OP 块 144644465 引用了以太坊块 23942644，序号为 0。签名确认的执行块 23942645 的父哈希与该来源一致。离线 CLI 可重算。缺少来源头、重复头和顺序调换均拒绝。Go 全包测试、vet 及 OP 来源和费用解码专项 race 回归通过。该项覆盖历史 L1 来源资格，普通 owner 的来源策略、从 L1 批次重建 L2 以及重组撤权和恢复仍在 P0-1 范围内。

[固定输入与重算命令](integration/test/source/op-mainnet-withdrawals/README.md) · CLI 输出（`integration/artifacts/local/op-l1-origin-212/result.json`，已清理） · [专项回归](chain-parser-go/internal/chain/evm/op_l1_origin_test.go)

<a id="op-owner-l1-origin"></a>

## 普通 owner 在 OP Mainnet 发布前核验 L1 来源（2026-11-20，阶段213，P0-1）

OP Mainnet 的 `finalized` 原生承诺配置可设置 `op_l1_origin`，字段为 `checkpoint_root`、`proof_file`、`max_checkpoint_age_ms` 和 `l1_rpc_url`。Go owner 从发布配置读取检查点，从证明文件重验以太坊最终性签名，向指定 L1 节点核对链 ID 和创世块。每次池状态读取从已验证的 OP 交易体解出来源，逐个核验签名执行头至来源块的高度、哈希及父链接，核对来源时间。读取受本次超时、剩余字节、头部数与 128 层回溯上限约束。状态读取结束时按 `max_baseline_age_ms` 再检查签名头时效。

普通 owner 发布的 OP 块 159771706 引用了以太坊块 26453350。511 名签署者确认了以太坊执行块 26453360。本次归档用三个本地端点重放，共读取 84 次，重建的区块和 USDC/WETH 池状态与在线基线一致。用重复头替换中间父块后，owner 启动返回 `OP_ORIGIN_ANCESTRY_LINK`。Go 全包测试、专项 race 回归与 vet 通过。阶段 208 的已留存主网窗口含 8 笔发往 OP 批次收件地址的 blob 交易，可用于下一轮核验。P0-1 仍需证明 L1 中对应批次的包含和数据可用性、重建 L2，以及验证重组后的撤权与恢复。

主网基线（`integration/artifacts/local/op-signed-owner-213/baseline.json`，已清理） · 重放报告（`integration/artifacts/local/op-signed-owner-213/replay.json`，已清理） · 拒绝记录（`integration/artifacts/local/op-signed-owner-213/bad.log`，已清理） · [时效与父链接回归](chain-parser-go/internal/chain/evm/op_l1_online_test.go)

<a id="op-batch-blob"></a>

## 从已最终化的以太坊块核验 OP 批次提交（2026-11-20，阶段214，P0-1）

`verify-op-batch` 使用调用方指定的主网检查点验证最终性签名，再从以太坊区块重算交易根。它恢复指定交易的发送者，核对预期地址、链 ID 和 OP Mainnet 收件地址，按全块交易顺序定位 blob。从实际 blob 字节计算 KZG 承诺并逐一比对交易中签名的版本哈希。

主网块 26453043 含 309 笔交易。第 193 号交易（从 0 起算）引用了信标时隙 15692864 的第 3 至 7 号 blob。五份完整数据均已复核。最初测试的 sidecar 响应中，五项 `kzg_proof` 都使用无穷远点编码，逐项验证在第一项即拒绝。实际成功路径使用同一时隙的 blobs 接口，直接从原始数据重算承诺。错误发送者、缺少末尾 blob、错位起点及改单字节均有拒绝回归。Go 全包测试、专项 race 回归与 vet 通过。发送者在该高度的动态授权、通道解码、重建 L2 块和 owner 撤权恢复仍待验收。

[固定主网输入与 CLI 命令](integration/test/source/op-mainnet-batches/README.md) · 五个版本哈希（`integration/artifacts/local/op-batch-214/result.json`，已清理） · [专项回归](chain-parser-go/internal/chain/evm/op_batch_blobs_test.go)

<a id="op-channel-frames"></a>

## OP Mainnet 五帧通道重组（2026-11-20，阶段215，P0-1）

`verify-op-batch -channel` 从已通过 KZG 核验的 blob 解包版本、字段和填充，读取每个帧的通道 ID、序号、长度与结束位。该模式支持单笔提交中的完整有序通道，并对压缩和解压字节数设置上限。按 RLP 边界逐项读取批次类型。

块 26453043 的五个帧构成通道 `912c873f4ac241003e7e83ea8738dc67`。614073 字节的带版本压缩数据解压后为 1202848 字节，包含一个 span 批次。独立 Brotli 命令得到相同的解压长度和 SHA-256。缺帧、乱序、重复帧、结束后追加帧、错误字段高位、截断和非零填充均已覆盖。Go 全包测试、专项 race 回归和 vet 通过。P0-1 继续核验 span 批次内的区块及签名交易、跨提交的通道与发送者授权。

[固定主网输入与复验命令](integration/test/source/op-mainnet-batches/README.md) · 通道报告（`integration/artifacts/local/op-channel-215/result.json`，已清理）


<a id="op-span-contents"></a>

## OP Mainnet 批次与 21 个 L2 块的逐字节核对（2026-11-20，阶段216，P0-1）

`verify-op-batch -span-blocks` 接受有序的完整 OP 块。工具重算每块头部哈希和交易根，核对高度、时间、父链接和首笔存款的 L1 来源变化。它从用户交易重建 span 的区块前缀、创建位、签名、接收地址、交易数据、nonce、gas 与 legacy 保护位，并与已核验的完整通道逐字节比对。

以太坊块 26453043 的单个 span 匹配 OP 块 159769779 至 159769799，共 1082 笔用户交易，其中 1050 笔为类型 2，32 笔为 legacy。改动最终来源哈希、签名字节或交易数据，以及缺块、跳块和倒序均被拒绝。普通 CLI、Go 全包测试、专项 race 和 vet 通过。P0-1 仍需对全部来源做 L1 祖先核验，补齐对应高度的发送者授权和跨提交通道，验证执行派生及普通 owner 的重组撤权和恢复。工作包完整验收保持 0/26。GitLab 项目克隆地址待提供。

[主网输入与复验命令](integration/test/source/op-mainnet-batches/README.md)


<a id="op-span-origin-range"></a>

## OP 批次内全部 L1 来源的祖先核验（2026-11-20，阶段217，P0-1）

`verify-op-batch -span-blocks … -origin-headers …` 从签名最终化的以太坊提交块向父块回溯一次。工具逐个重算头部哈希，在批次引用的每个来源高度核对哈希和时间。阶段 208 的主网归档提供了从块 26453043 至 26453032 的全部 12 个头部。21 个 OP 块引用的 4 个来源均通过核验。更换中间来源的哈希会使核验失败。普通 CLI、Go 全包测试、专项 race 和 vet 均通过。

下一项核验主网提交高度实际生效的 batch sender，随后覆盖跨提交通道、执行派生与普通 owner 的重组恢复。现有 CLI 仍由调用方给出预期发送者。该来源窗口的验收范围与工作包完整验收分别记录。

[主网归档与离线复验](integration/test/source/op-mainnet-batches/README.md)


<a id="op-batch-sender-authority"></a>

## 从提交高度的 SystemConfig 核验发送者（2026-11-20，阶段218，P0-1）

`verify-op-batch -sender-proof` 从签名确认的以太坊执行头核验提交块及其状态根。通过账户和存储 MPT 证明检查 OP Mainnet 的 SystemConfig 代理、实现地址、两份运行时代码哈希与 `batcherHash` 槽，再将该地址与提交交易签名恢复出的地址匹配。配置合约版本变化需要单独核验其存储布局。

固定样本中，以太坊最终化块 26453871 的父块 26453870 包含一笔五 blob 提交。普通 CLI 从状态证明取得 `0x6887246668a3b87f54deb3b94ba47a6f63f32985` 并核验五份承诺。原样本 26453043 的 `eth_getProof` 返回错误 4444，超出节点历史证明窗口。新样本使用同一提交块高度的交易、状态证明和数据。回归覆盖策略地址不符、缺子块头、错误实现账户和被更改的证明值。Go 全包测试、专项 race 和 vet 均通过。阶段 208 主网窗口的八笔提交分别使用不同的通道 ID，每笔的帧编号均从 0 开始。跨提交通道需要另取样本。

[主网固定输入与普通 CLI 复验](integration/test/source/op-mainnet-batches/README.md)


<a id="op-ordered-channel-submissions"></a>

## 多笔提交的 OP 通道核验入口（2026-11-20，阶段219，P0-1）

`verify-op-batch -submissions` 在同一签名最终化头下逐笔核验提交块、授权账户及 blob 承诺，按区块高度和块内 blob 序号核对提交顺序。帧累积到同一通道，检查 Granite 的 50 块时限。单通道完整后可继续核验 span 交易。

普通 CLI 从块 26453870 的提交读出五帧，通道 `c7a81691135d79ebbb32d983e3f23954` 解压后为 1395451 字节。重复提交返回 `OP_CHANNEL_SUBMISSION_ORDER`。分组回归遍历了五帧的四个拆分位置，并核验最后允许高度和超时拒绝。Go 全包测试、专项 race 和 vet 通过。真实跨提交样本、期间全部有效收件交易及通道重置仍待核验，状态为 `PARTIAL_LOCAL`。

[普通入口和输入说明](integration/test/source/op-mainnet-batches/README.md)


<a id="op-channel-interval"></a>

## OP 通道的提交间交易覆盖（2026-11-20，阶段220，P0-1）

多提交核验入口现在要求两笔提交之间每个 L1 高度的完整区块，核对父链接并重算交易根。它也会扫描首块在前一笔提交之后、末块在后一笔提交之前的所有交易。同块提交仅扫描两个交易位置之间的区间。遇到发往收件地址的交易时核对发送者，若发现遗漏的授权提交则拒绝该通道。

阶段 208 归档中，以太坊块 26453040 和 26453043 各含一笔 OP 提交。块 26453041、26453042 及两个边界块的相应交易范围均通过扫描。缺块、错序、候选交易缺少授权证明及漏报发送者交易均有拒绝回归。Go 全包测试、专项 race 和 vet 均通过。两笔提交使用不同通道 ID，真实跨提交正例仍待采集。

[输入与复验说明](integration/test/source/op-mainnet-batches/README.md)


<a id="op-owner-span-membership"></a>

## OP owner 读取池状态前核对批次（2026-11-20，阶段221，P0-1）

启用 `op_l1_origin.batch_proof_file` 后，Go owner 用同一签名最终化头核验批次提交、发送者及完整通道，从 L2 块重建 span。读取池状态前，它核对当前块在批次中的高度和哈希，并从 L1 RPC 核验各个来源的哈希和时间。证明文件消耗本次原生读取的剩余预算。

固定输入中，以太坊块 26453870 的提交涵盖 OP 块 159774755 至 159774778，共 1099 笔用户交易。owner 核验函数接受位于批次中间的块 159774767，拒绝同一高度的错误哈希和超出剩余预算的文件。Go 全包测试、专项 race 和 vet 通过。后续验收包括批次文件持续供应、普通进程发布、执行派生、重组撤权与恢复。

[固定输入与复验命令](integration/test/source/op-mainnet-batches/README.md)


<a id="op-owner-span-replay"></a>

## 普通 OP owner 核验批次后发布基线（2026-11-20，阶段222，P0-1）

验收端点将 OP `finalized` 读取固定在主网块 159774767，提供该高度的区块、回执和池状态证明。普通 `chain-parser` 进程核验以太坊提交、批次交易和 L1 来源，随后通过基线接口返回该块及 USDC/WETH 池。归档重放工具把两个 L2 来源、证明来源及 L1 来源映射到本机端点，用独立状态目录重新启动同一进程。正确证据重建的块和池与采集结果一致。将批次观测替换为空数组后，进程在发布基线前退出，错误为 `OP_SPAN_BLOCKS_REQUIRED`。两次重放共读取 123 次归档 RPC。

[重放工具](integration/tools/acceptance/replay_op_span_owner.py) · 重放报告（`integration/artifacts/local/op-owner-run-222/replay.json`，已清理）


<a id="op-owner-span-refresh"></a>

## 从已核验的提交更新 owner 批次文件（2026-11-20，阶段223，P0-1）

`chain-parser --config <release> --refresh-op-span <submissions>` 核验 L1 提交和通道后，从 span 前缀取得 L2 起始高度和块数，向发布配置的 OP RPC 读取完整区块。所有交易逐字节匹配且来源通过 L1 祖先核验后，命令在文件锁内检查旧水位、重新验证签名头和时效，原子替换批次文件。

普通 CLI 从归档节点读取块 159774755 至 159774778，核验 1099 笔用户交易和 5 个 L1 来源。输出的 24 个块与阶段 222 owner 的输入一致。截断前缀、超限块数、旧水位回退、重叠哈希冲突及发布前撤销均有回归。Go 全包测试、专项 race 和 vet 通过。

[运行说明](integration/test/source/op-mainnet-batches/README.md) · 更新报告（`integration/artifacts/local/op-span-refresh-223/report.json`，已清理）


<a id="op-owner-live-collection"></a>

## 从签名头自动采集 OP 提交（2026-11-20，阶段224，P0-1）

普通 owner 配置可以刷新 OP 所用的以太坊信标签名证明。`--refresh-latest-op-span` 从已最终化的执行块向父块查找，逐块核对交易根和父哈希，使用提交高度的 SystemConfig 证明选择授权发送者，然后从对应信标时隙取得 blob。命令按有界并发读取 L2 块并核验整个 span。当前自动发现覆盖单笔交易内完整的通道。

主网运行从以太坊块 26454223 找到提交块 26454214，得到五帧通道 `9f631924b60aac4e1087c7d35d07b80b`，验证 OP 块 159776772 至 159776841、4354 笔用户交易和 13 个 L1 来源。前两次采集分别遇到 RPC 错误和读取超时。采用有界并发读取后，在正常 CLI 的时限内发布了观测文件。固定输入回归覆盖搜索窗口耗尽、字节上限、错误父块和独立授权证明请求。Go 全包测试、专项 race 和 vet 通过。

[复验命令](integration/test/source/op-mainnet-batches/README.md) · 主网采集报告（`integration/artifacts/local/op-live-refresh-224/result.json`，已清理）


<a id="owner-fork-revocation"></a>

## 旧基线撤权与同高分叉恢复（2026-11-20，阶段225，P0-1）

对照 §7.9，在 Go owner 已发布基线后损坏动态发现队列，复现了重组处理返回清理错误、旧基线仍可读取的问题。现在先关闭本地基线读取并向 owner journal 发布 `ONLINE_SCOPE_REVOKED`，再清理目录和隔离失败的原始响应。孤块标记与发现队列清理分别尝试，错误返回调用方。

通过普通 `Tick` 和两个本地 RPC 端点复验同高换块。替代池状态暂缺时，baseline HTTP 返回 503，`Current` 和 `Bootstrap` 均不可用，日志只有撤权记录，`canonical_epoch` 保持原值。替代状态恢复可读并通过两个端点确认后，owner 发布新基线，代际和 `canonical_epoch` 各递增一次，HTTP 恢复 200。

`go test ./...`、重组路径的 `go test -race` 和 `go vet ./...` 均通过。

下一项仍属 P0-1，使用 OP 批次文件和归档重放验证证明缺失、重新取得证明及连续更新。跨提交自动发现与真实主网正例、L2 执行派生以及其余链和协议的来源资格仍待验收。GitLab 交付等待该项目的克隆地址，当前仓库只配置了 GitHub `origin`。


<a id="op-owner-idle-recovery"></a>

## OP 同高轮询的证明撤权与恢复（2026-11-20，阶段226，P0-1）

重放阶段222的主网归档时，旧 owner 发布 OP 块 159774767 后，即使把批次证明原子替换为空，30 秒内基线仍然可读。此前普通 `Tick` 看到同高同哈希就返回。现在这条路径复用本轮已核验的 OP 区块体，在读取预算和超时内重新检查 L1 签名与时效、批次成员及 L1 来源。

同一进程在下一次轮询返回 `OP_SPAN_BLOCKS_REQUIRED`，基线和 bootstrap 均返回 503。换回原证明后重新发布同块同池基线，代际从 1 升至 2，规范链版本为 1。journal 依次记录序号 2 的 `ONLINE_SCOPE_REVOKED` 和序号 3 的 `ONLINE_BASELINE_COMMIT`。四个按高度只取哈希的确认响应由各自来源归档的完整块投影得到。两次独立重放均通过，Go 全包测试、OP 与重组专项 race 和 vet 通过。

后续优先验证相邻签名周期内的连续刷新，再完成跨提交发现、主网跨提交正例和 L2 执行派生。GitHub 已配置为授权远端，GitLab 交付仍需项目克隆地址。

[重放工具](integration/tools/acceptance/replay_op_span_owner.py) · [重放报告](integration/artifacts/local/op-owner-recovery-226/report.json)


<a id="op-refresh-consecutive"></a>

## 相邻最终化周期刷新 OP 批次文件（2026-11-20，阶段227，P0-1）

之前，信标更新可能在批次发布者完成最终复核后、替换文件前提交。现在批次发布者在最终复核、原子替换及目录同步期间持有信标文件共享锁。信标写入者持有独占锁期间，批次发布返回 `OP_SPAN_BEACON_REFRESH_BUSY` 并保留旧文件。两种锁竞争顺序、失败后的旧文件保留和成功后的解锁均有回归。Go 全包测试、专项 race 和 vet 通过。

普通 CLI 在同一目录刷新到相邻的以太坊最终化时隙 15694240 和 15694272。第一次从提交块 26454407 验证 OP 块 159777910 至 159778005 共 4081 笔用户交易。第二次从提交块 26454447 验证 OP 块 159778224 至 159778249 共 1046 笔用户交易。两次采集之间还有 218 个 OP 块。后续需要按 L1 顺序发现这段区间的提交、组装跨提交通道并验证连续 owner 发布。L2 执行派生和其余链的来源资格仍待验收。

[两次主网刷新报告](integration/artifacts/local/op-refresh-227/window.json)


<a id="op-forward-window"></a>

## 按 L1 交易顺序补齐两个最终化周期之间的 OP 批次（2026-11-20，阶段228，P0-1）

对照 §7.1、§7.3 和 §7.9，CLI 现在可从指定 L1 高度与交易序号向前扫描。它从签名最终化头核验父链，再按顺序检查每个完整块的交易根、发送者权限和 blob 承诺，遇到完整通道时核验 L2 批次并原子发布。目标已有批次时，发布者在锁内要求新批次首块接续旧批次末块的高度和完整父哈希。

从以太坊块 26454408 开始，四笔提交分别覆盖 98、67、29、24 个 OP 块。共 218 块、8140 笔用户交易。首块 159778006 接续阶段227的末块 159778005，第五笔提交接续到阶段227已验证的块 159778249，末块哈希相同。采集遇到 RPC 错误后重试成功，使用收集的 RPC 响应和信标 blob 接口重跑，五份报告逐字节一致。Go 全包测试、定向 race 和 vet 通过。

此次发布的是批次观测文件。普通 owner 在整个区间逐块提交仍需验收。这五个通道各由一笔提交组成，跨多笔提交的通道仍需主网正例。P0-1 还需补齐 L2 执行派生和其他链的来源资格。

[采集报告](integration/artifacts/local/op-gap-228/verified.json) · [复验说明](integration/test/source/op-mainnet-batches/README.md)

<a id="op-owner-window"></a>

## 普通 owner 跨批次发布与同源离线复验（2026-11-20，阶段229，P0-1）

在 OP 块 159778223 提交后替换批次文件，旧头的同高轮询可能读到下一批次并返回 `OP_SPAN_OWNER_BLOCK_MISMATCH`，触发错误撤权。现在仅在同高轮询遇到该错误时，在原截止时间内再取一次头。高度或哈希变化交给原追块或重组流程，同一头仍返回成员核验错误。

使用 HTTP 开发日志和 Pebble，普通 Go owner 从块 159778222 跨两个 span 提交至 159778226。以同一来源缓存严格离线复验，四个代理共命中 419 次，上游读取为零。两次的基线引用及四条增量的引用、载荷哈希逐条一致。空批次仍导致撤权，恢复后代际升至 2。Go 全包测试、EVM/service 定向 race 与 vet 通过。历史窗口年龄预算为 86400000 毫秒，仅用于这次归档验收。

218 块长窗口与严格离线复验另行记录。P0-1 后续需取得同一通道跨多笔 L1 提交的主网样本，核验 L2 执行派生，并完成其余链和协议的来源与恢复验收。原工作包完整验收 0/26。GitLab 推送等待项目克隆地址。

[窗口复验说明](integration/test/source/op-mainnet-batches/README.md) · 归档边界复验（`integration/artifacts/local/op-owner-window-229/offline-boundary/report.json`，已清理） · 撤权与恢复（`integration/artifacts/local/op-owner-window-229/withdrawal.json`，已清理）

<a id="op-origin-base-fee"></a>

## 从签名 L1 祖先区块头核对 OP 存款基础费用（2026-11-20，阶段230，P0-1）

此前首笔 L1 属性存款的来源检查只与祖先链核对高度、哈希和时间。现在从 Bedrock ABI 或激活后的紧凑格式读取完整的 256 位 L1 基础费用。每个 epoch 的声明值须与同一已核验以太坊头的 `baseFeePerGas` 一致。同一 span 内引用同一 epoch 的所有块须保持该值一致。在线轮询、普通读取和批次 CLI 共用核验路径。

OP 块 144644465 的存款与以太坊块 23942644 均给出 48265514 wei。修改中间 epoch 的费用会得到 `OP_ORIGIN_L1_BASE_FEE_MISMATCH`。阶段229的跨批次边界在新二进制下用纯缓存重放，基线和四条增量与前轮逐条相同。Go 全包测试、专项 race 和 vet 通过。其余存款、blob 费参数、L2 执行后状态和主网跨提交样本仍待验证。

纯缓存 owner 报告（`integration/artifacts/local/op-owner-window-229/basefee-boundary/report.json`，已清理）

<a id="op-portal-deposits"></a>

## 由已最终化的以太坊回执重建 OP 用户存款（2026-11-20，阶段231，P0-1）

归档 CLI 现在接受 L1 origin 的完整回执，从签名最终化头核对祖先链后重算回执根。对 Jovian 激活后的 epoch 首块，按以太坊规范日志序号处理 OP Mainnet Portal 的 `TransactionDeposited` 事件，重建来源哈希、发送方、接收方或创建标志、铸币量、转账额、gas 和 calldata。在 L2 已核验的交易体中逐字节核对顺序和内容，同时检查多出的存款。

以太坊块 26454406、26454411、26454425、26454436 共 1029 条回执，各重建一笔存款，分别与 OP 块 159778047、159778075、159778160、159778224 的存款匹配。仓库包含第四个样本和签名证明，覆盖回执倒序、缺失、存款错位和多余存款。普通 owner 的自动回执读取、其余升级规则与执行后状态仍待验收。

[四块核验报告](integration/artifacts/local/op-origin-deposits-231/report.json) · [复验命令](integration/test/source/op-mainnet-batches/README.md)

<a id="op-owner-portal"></a>

## 普通 OP owner 在发布前核验用户存款（2026-11-20，阶段232，P0-1）

普通读取和同高轮询现在从本次签名祖先核验中保留所选 L1 origin 头。对 Jovian 激活后的 epoch 首块，日志 bloom 可以排除 Portal 事件时，L2 块只允许属性存款。可能有事件时，通过同一个受预算和截止时间约束的 L1 reader 读取完整回执、验证回执根并核对存款序列。epoch 内后续块也会拒绝多出的存款。

在 159778222 至 159778226 的窗口，L1 代理新增一次按区块哈希读取完整回执的请求。纯缓存复验得到相同的基线和四条增量。最初的工具运行只复验了旧路径，修正调用点后已用新二进制复查来源请求。Go 全包测试、OP 专项 race 和 vet 通过。

[普通读取](integration/artifacts/local/op-origin-deposits-231/owner-wired/report.json) · [离线复验](integration/artifacts/local/op-origin-deposits-231/owner-wired-offline/report.json)

<a id="op-owner-full-218"></a>

## OP 普通 owner 连续发布 218 个历史高度（2026-11-20，阶段233，P0-1）

阶段229的 owner 从 159778006 建立基线，经过四个相邻 span，顺序发布至 159778223。验收逐块检查高度、父哈希、journal 序号、epoch、代际、scope 及终点基线。再次启动独立 owner，以同源归档缓存完成整个窗口。两次的基线以及 217 条提交引用和载荷哈希全部一致。离线运行的四个来源均为零上游读取。

历史窗口使用开发 HTTP 日志、Pebble 和 24 小时年龄预算。阶段232的新 owner 已通过含实际用户存款的边界窗口，完整长窗口单独跟踪。P0-1 仍需跨多笔 L1 提交的真实通道、L2 执行后状态及其他链的来源和恢复验收。GitLab 远端待提供。

[脱敏的逐块记录与四个来源的读数](integration/artifacts/local/op-owner-window-229/verified.json.gz)

<a id="op-owner-full-deposits"></a>

## 带 Portal 存款核验的 owner 完成 218 块复验（2026-11-20，阶段234，P0-1）

用阶段232的普通 owner 重跑 OP 块 159778006 至 159778223。沿用阶段233的来源缓存，本次只有 L1 代理发生上游读取，共 13 次，四个代理的 RPC 错误均为零。区间包含阶段231已逐笔重建的三笔用户存款。再次启动独立 owner，在纯缓存模式下完成四个 span。与采集及阶段233的结果比较，基线、217 条提交引用和载荷哈希全部一致。

[本次来源读数和校验值](integration/artifacts/local/op-origin-deposits-231/full-window.json) · [逐块记录](integration/artifacts/local/op-owner-window-229/verified.json.gz)

<a id="op-origin-boundaries"></a>

## OP 批次交界处核验 L1 来源和序号（2026-11-20，阶段235，P0-1）

此前批次内部核验来源切换，连续发布时在两个批次交界处只比较 L2 高度与父哈希。现在发布者在文件锁内重新核验旧末块的交易根和属性存款，对新首块应用与批次内部相同的 epoch、序号、时间、哈希和基础费用规则。重复序号回归覆盖拒绝写入和原文件保留。

普通 CLI 从以太坊块 26454408 顺序重放五个批次，产生的观测文件与阶段229归档逐字节相同。四个边界中，同一 epoch 的序号分别由 4 增至 5、由 0 增至 1，另外两处切换 epoch 后均从 0 开始。执行 RPC 命中原来源缓存，blob 来自信标 API。Go 全包测试、发布与来源专项 race、vet 通过。

[边界记录](integration/artifacts/local/op-boundaries-235/boundaries.json)

<a id="op-origin-blob-fee"></a>

## 由已签名 L1 祖先头重算 OP 的 blob 基础费用（2026-11-20，阶段236，P0-1）

普通 owner 和归档 CLI 读取紧凑属性存款中的完整 256 位费用，使用已核验 L1 头的时间、excessBlobGas 和仓库固定的 geth 主网参数重算。同一 epoch 内的声明须一致。计算前检查分叉和超大输入。以太坊块 26454436 派生的值为 8298648 wei，声明增加 1 wei 时来源核验拒绝。

普通 owner 以纯缓存跨四个 span 发布 OP 块 159778006 至 159778223，基线和全部 217 条增量与阶段233逐条相同。四个代理的上游读取均为零。Go 全包测试、OP 专项 race 和 vet 通过。测试前清理了闲置的阶段235二进制，释放 57381976 字节。

[窗口记录](integration/artifacts/local/op-blob-fee-236/window.json)

<a id="op-origin-payload-attributes"></a>

## 核对 OP 执行载荷的随机数与父信标块根（2026-11-20，阶段237，P0-1）

普通 owner、归档 CLI 和连续批次发布者从已验证的 L2 头读取 prevRandao 和激活后的父信标块根，按 L1 epoch 与签名祖先链的对应头比较。回归测试分别改变中间 epoch 的这两个值，均被拒绝。两次纯缓存运行从 159778222 跨批次提交到 159778226，基线和四条增量逐条一致。第一次运行结束时日志出现一次 `RPC_RECORDING_FAILED`，第二次复验只记录退出时取消。Go 全包测试、专项 race 和 vet 通过。

两个已配置 OP 来源对 159778224 的 `debug_executionWitness` 均返回 -32601。`debug_traceBlockByNumber` 分别返回 -32601 和 HTTP 429。对父高度的账户证明请求分别返回 HTTP 403 和 -32603。以这两个来源重算该块完整执行后状态仍缺少见证。测试前清理闲置构建产物 57386664 字节。

[边界复验](integration/artifacts/local/op-execution-witness-237/owner-second/report.json) · [见证探测](integration/artifacts/local/op-execution-witness-237/probes.json) · [替代来源探测](integration/artifacts/local/op-execution-witness-237/alternative-probes.json)

<a id="op-fixed-payload-attributes"></a>

## OP 属性存款 gas 与区块收款地址的固定派生规则（2026-11-20，阶段238，P0-1）

上一轮停止于阶段237，代码与 GitHub origin 已同步。本轮对照固定 Optimism 源码 `60360273aada79572fe2212497de578ad2fa249b` 的 `L1InfoDeposit` 和 `PreparePayloadAttributes`，发现首笔属性存款接受任意 gas 上限、L2 头接受任意收款地址。现在普通 owner、归档 CLI、批次核验和连续发布共用的来源路径要求属性存款 gas 为 1000000、区块收款地址为 SequencerFeeVault `0x4200000000000000000000000000000000000011`。gas 规则也覆盖共用解析器的 Base packed 费用输入。

反例修改主网样本中的属性存款 gas，再重算交易哈希、交易根和区块哈希；另修改收款地址并重算区块哈希。它们均通过一般头部与交易体承诺核验，随后由 OP 派生规则分别以 `OP_PACKED_ATTRIBUTES_GAS`、`OP_ORIGIN_FEE_RECIPIENT` 拒绝。覆盖 gas 为零、相差一、旧 system gas 及 uint64 上限。原始两份签名来源样本保持通过。四个升级点的八份原始区块归档也通过 packed 来源及升级测试的 race 复验。

普通 owner 使用阶段229的同源缓存，跨四个 span 从 159778006 发布至 159778223。基线及全部 217 条增量与阶段233逐条一致，四个来源均无上游读取或 RPC 错误。Go 全包测试、vet、OP 专项 race 通过。初次本地 RPC 测试因沙箱禁止监听端口退出，取得运行权限后重新通过。历史验收仍使用 HTTP 开发日志、Pebble 和 24 小时时效预算，不增加生产或完整执行资格。

下一项修复自动采集遇到废弃通道后的恢复。完整执行后状态仍缺少阶段237所探测节点的见证，跨多笔 L1 提交的真实通道仍需采集；这些外部证据限制不关闭其他可推进项。GitLab 地址仍未提供，每轮推送既有 GitHub origin。

[窗口记录](integration/artifacts/local/op-fixed-attributes-238/window.json)

<a id="op-forward-channel-recovery"></a>

## OP 自动扫描在交易边界丢弃废弃通道后继续（2026-11-20，阶段239，P0-1）

对照固定 Optimism `channel_assembler.go` 的 Holocene 规则，原先前向采集器遇到未完成通道后的新 frame zero 或 50 块超时就直接退出。现在对 Holocene L1 origin，下一笔提交从 frame zero 开始时丢弃旧通道，没有活动通道时跳过仅含非首帧的提交，超过含首尾的 50 块窗口后继续扫描。旧提交、区间块与授权引用一并清空，输出大小预算重新计算；累计读取字节和最多 128 个授权候选的限制不重置。SystemConfig 代理和实现证明按高度计入读取预算，发送者或帧过滤前即记账。Holocene 之前保留原严格行为。

采集与最终核验仍分开：入选的完整通道继续验证每笔提交的签名来源、KZG、发送者权限和提交间全部授权交易。本轮恢复范围只到交易边界，一笔交易中混合废弃帧与新通道、活动通道的非法续帧仍明确拒绝。回归以主网原始五份 blob 人工拆分提交位置，覆盖替换、孤立帧、超时边界、旧证据清理、累计字节及候选上限、取消和早期分叉规则；这些合成位置不是主网多提交采集正例。

普通 CLI 从 L1 游标 26454408:0 依次发现并发布五个通道，覆盖 OP 159778006 至 159778249，共 244 个高度。执行 RPC 严格使用阶段228原来源缓存，blob 使用原公开信标 API。五份观测及五份报告均与阶段235逐字节一致。现有发现清单包含 70 个候选：60 个可读首帧均为 frame zero、10 个 HTTP 失败；八笔完整留存的提交均各自包含完整通道，真实跨多提交正例仍待采集。

本轮首个全包回归中，新增字节预算反例错误地按夹具文件空白计算大小，导致未在 blob 请求前耗尽。按测试服务器实际压缩后的 JSON 字节修正，保留提前拒绝断言。修正后 Go 全包测试、通道专项 race、vet 和差异检查全部通过，验证结果与来源边界记录在报告中。阶段238的普通 owner 218 块结果保持为最新长窗口验收；本轮未声称完整执行后状态、生产准入或任一原工作包全部完成，原26工作包完整验收仍为0。

后续可本地推进项包括对照固定 op-geth 共识代码补齐 OP 原生头的固定字段规则。完整执行重算仍需要阶段237来源未提供的见证；真实跨提交通道、其他链与协议来源、恢复和完整设计验收继续保留。推送目标仍为 GitHub origin，GitLab 克隆地址待提供。

[验证记录](integration/artifacts/local/op-forward-recovery-239/verification.json) · [采集与复验说明](integration/test/source/op-mainnet-batches/README.md)

<a id="go-standalone-fixtures"></a>

## 修复独立Go回归与进一步保留必要文件（2026-11-21，阶段240，P0-1）

阶段239后的全范围审计发现5处Go测试读取兄弟integration目录。本轮将信标分叉/轮换及OP区块、批次、存款、提款核验所需的20份压缩原始样本放入Go自己的 `test/fixtures/source/testdata`，保持原始gzip字节不变。两个读取helper改为模块内路径，提款/消息传递测试共用已存在的读取helper；契约检查显式登记这20项并逐字节比较副本，不放宽跨项目依赖规则，也不复制无关的运行目录。原始与服务副本内容相同，Git可复用blob；约6.2MiB的工作区副本属于独立测试所需文件。

最初全包检查补出信标轮换还需 `fulu-rotation-bootstrap.json.gz`，纳入20份集合后Go全包测试、OP专项race和vet通过。`contracts.py` 现在通过：328份contract、12份普通fixture、191份公开来源文件。阶段审计时的“契约失败”保留为历史事实，此项已由本轮修复；不能将这一个门禁通过等同于WP01全包或生产验收完成。

进一步清理49处冗余工具链下载包、下载分片、sysroot/vendor副本和编译中间目录，共11608911872字节，约10.81GiB。两处正式参考程序在原位置保留且SHA256不变，正常Solders 0.29.0/LiteSVM初始化在清理前后均通过。源码、固定程序、必要bank、编译器及来源清单保留；显式离线源码重编译若需要已清理的工具链缓存，须按固定版本重新准备。没有生成新的测试报告/二进制留在项目中。

后续每轮执行：先选最前的可推进设计缺项，实现正常入口及必要反例，运行所需门禁，更新本文件、commit/push，然后清理本轮临时目录、构建中间物和测试结果；保留实际运行及必要回归输入。范围完整验收仍为0/26，完整功能目标继续有效，不因局部通过而停止。

<a id="op-native-header-rules"></a>

## OP Mainnet原生头的固定字段规则（2026-11-21，阶段241，P0-1）

核对固定op-geth `79e9d244748a1201fe686ca8a882b5fe7e173348` 的beacon共识代码及主网Bedrock高度规则：从105235063（含迁移genesis）起，正常原生头核验在重算哈希后要求difficulty为零、nonce为零、uncle hash为空列表哈希、gasLimit不超过2^63-1、gasUsed不超过gasLimit。只用于evm:10；原始genesis与Bedrock之前的头保留既有路径。Jovian会使用blobGasUsed承载DA footprint，本轮未将其错误固定为零，也没有改语义描述符或冒称完整共识/执行证明。

两个真实完整区块分别核对原始正例，再独立修改五个字段并重新计算区块哈希、同步交易位置。一般头哈希仍自洽，新OP规则分别拒绝；另覆盖高度0、激活前、迁移genesis、激活后以及最大合法Gas边界。Go全包测试、OP专项race、vet和契约检查通过。阶段240的独立模块验证也已完成：无integration兄弟目录的临时模块分别通过历史信标/OP回归与额外轮换样本测试，临时副本均自动删除，20份gzip总计6488333字节。

本轮测试没有产生需要保留的运行目录、二进制或报告；必要的源码与样本留在模块内，其余临时文件已清理。下一项推进两解析服务的链级启动隔离，先避免一条失败链阻止另一条合格链启动，再处理慢链调度和其余设计缺项。原26工作包完整验收仍为0，整体目标继续。

<a id="go-chain-startup-isolation"></a>

## Go解析器按链启动与失败后安全恢复（2026-11-21，阶段242，P0-1）

原先Service.Start遇到任何链的身份、状态或会话错误就返回，其他合格链无法进入正常HTTP服务。现在逐链尝试并隔离失败范围；至少一链初始化成功或存在真实租约standby时继续启动。共享配置、存储与PG初始化仍在链循环前失败退出，所有活动链都失败则返回首个错误；不将全部失败改成成功。未完成链保持不可读，Tick重新经过既有身份核验和状态门禁后恢复，不给其他链升代。全局取消返回取消错误，并撤销本次已发布的范围及关闭对应writer。

复核同时发现openSession获取租约后，owner/prepared等校验错误的部分出口没有释放租约。统一失败清理现在关闭producer、暂停旧状态，并以独立一秒上下文释放租约，保留冲突的prepared字节。实PG/Kafka测试分别制造JOURNAL_OWNER_MISMATCH与PREPARED_JOURNAL_CONFLICT：旧基线不可读，peer立即取得原本还有120秒的租约，prepared哈希不变。测试使用自行启动的回环PG/Kafka、独立topic，完成后停止服务并删除全部临时数据。初次环境启动受macOS Unix socket路径长度影响，改用短的/tmp路径后通过；没有将环境准备交给用户。

9组正常本地RPC场景覆盖错链、节点503、池状态不可用、两种配置顺序、全部失败和全局取消。恢复期间继续错误的身份不能读池状态；修复后首次发布自身基线，健康链的完整引用保持不变。Go全包、专项race、vet、contracts及实PG/Kafka门禁通过。本轮没有留下测试结果或二进制；准备后因优先级暂不实施的租户工作树和分支已移除。仍需按链并发调度以隔离慢链延迟，本轮只关闭失败对其他链启动的阻断，未关闭全部WP18/WP24条款。

<a id="rust-chain-startup-isolation"></a>

## Rust解析器逐链启动、失败重试与writer交接（2026-11-21，阶段243，P0-1）

Rust普通启动在共享配置/存储/PG初始化之后逐链尝试。单条RPC或Yellowstone恢复失败时撤销可读范围、暂停store、关闭writer，记录pending后继续其他链；至少一链初始化成功或真实LEASE_NOT_OWNED standby时启动服务，全部失败仍返回首个错误。Tick重试pending链，成功后清除标记；保留状态的首次重试按启动语义提升自身writer/generation，健康链引用不变。正常流任务只在初始化恢复完成后启动，不使用RPC后备，也不将异步任务存在当作完整bank，未取得所需来源证明仍返回503。

复审修复两类关闭/交接边界：流任务在列表锁内重新检查stop并注册，Close先取出句柄、释放列表锁再join，避免恢复actor与任务退出互相等待；读取和提交之间检查关闭状态，关闭后的启动不返回成功。Writer::open取得租约后任一初始化Result错误均以独立有界PG操作释放；open成功后而尚未安装writer时，本地prepared/ledger恢复或store锁错误也由统一helper关闭。此helper同时用于普通baseline、流撤销恢复及scope writer恢复，冲突证据不被删除。

服务crate完整tests通过；复审修复后49项lib通过、2项需显式实环境驱动的测试保持ignored，其中新增租约测试已单独启用并通过。正常Yellowstone含实PG/Kafka的18项全部通过：发布、撤销、重启和fencing继续成立。新测试在取得writer后制造SCOPE_STORE_CORRUPT，确认坏字节不变、失败writer未安装、健康链HTTP200且版本不变，peer在30秒测试窗口内立即取得原本120秒TTL的租约。环境为自行启动的回环PG13/Kafka3.9.1，退出后停止全部服务并删除临时数据。fmt、全目标全feature Clippy的-D warnings及契约门禁通过。

本轮额外移除10个闲置的可重取编译器/SDK/sysroot/crate缓存及2个临时symlink，释放7879778304字节；正常Solders/LiteSVM和两份正式参考.so在清理前后均能加载且SHA256不变。保留固定源代码、锁文件、版本/来源清单、bank与实际运行时；显式源码重编译所需缓存须按固定版本重新准备。本轮Rust临时target在检查完成后删除，释放13132001280字节，没有保留测试报告或二进制。每轮清理要求继续执行。

阶段242–243关闭的是失败链对其他链启动的阻断和对应交接漏洞，当前仍使用串行恢复actor，慢链延迟隔离未完成。下一步沿P0-1推进Ethereum首个v2-complete真实签名来源、正常owner/Router与恢复闭环，已自行找到可提供finalized证明的公开源并取得512签署者的当前BLS最终性；不因此授予生产或VERIFIED资格。原26工作包完整验收仍为0，继续开发剩余条款。

<a id="beacon-bounded-tail-prefetch"></a>

## 签名Ethereum短尾祖先的四路有界预取（2026-11-21，阶段244，P0-1）

真实普通owner已通过Ethereum 26460850的签名最终性、完整正文/回执及六份账户/存储证明，随后向26460882推进。旧admitTail在同一60秒预算内串行读取31个中间祖先，实际仅完成24个；超时后正确撤销范围，连续提交验收失败。此问题不通过增加超时或放宽journal断言解决。

现在最多4个worker读取独立核验的原生头，结果按请求高度暂存；等全部读取完成，再从签名head按父链顺序检查每个高度、chain和parent/hash，连接到完整base后检查取消及proof时效，最后一次替换本轮资格表。任一读取错误取消其他worker并等待其退出；链冲突、取消或过期不留下部分祖先资格。现有总60秒期限、配置MaxTail、writer lease边界和raw捕获继续生效，不增加后台悬挂读取。

新的独立模块测试用已跟踪electra-fulu压缩夹具验证真实BLS根，再构造31个合成连续祖先，覆盖实际/最大并发4、乱序完成、middle/base/错高度/错链、读取错误/取消等待慢worker、最终时效复查及0/1/2块边界。原短尾发布门禁改为共用该固定夹具，不再依赖已经清理的阶段192运行目录或环境变量，因此普通测试也会检查篡改后的发布拒绝。Go全包与vet通过，新祖先及发布门禁race通过；contracts继续通过328份contract、12份普通fixture、191份公开来源文件。

本轮没有把合成父链测试算作真实32高度成功窗口。直连的签名双Router尝试因公网大响应超时被拒绝；信标API短暂返回参与者不足的更新也被正确拒绝。已自行准备另一网络路径的有界只读转发环境，压缩传输与源部署验收继续推进，不交由用户配置，也不授予生产、完整共识或VERIFIED资格。下一项仍是P0-1的首个v2-complete签名来源与正常owner/双Router/恢复闭环。

追加清理21处未跟踪、无打开文件的旧编译工具、下载包、Go缓存、不再读取的旧parser运行状态及未接入原型的生成JSON，释放2837950464字节；其中vendor的948个crate、42521个文件均对照锁文件及逐文件hash确认没有唯一修改。保留固定源码/锁/哈希清单、正式bank、实际回放所读的case结果与归档二进制。新的Router临时构建target在编译完成后删除，释放2114486272字节，仅当前验收仍需的可执行文件临时放在私有/tmp目录，待该项结束清除。本轮不保留新的测试结果报告到项目。原26工作包完整验收仍为0，整体目标继续。

<a id="recent-signed-v2-bank"></a>

## 当前签名V2 bank的有界采集、正常报价及持久状态恢复（2026-11-21，阶段245，P0-1）

实测当前公网路径的完整正文/回执读取时间超过普通readState贯穿的60秒总预算。保持原始完整正文复核、receipt覆盖、MPT和时效门禁，本轮将有限原始响应采集与正常处理分开。SignedRPCCapture只接受显式HTTPS上游和只读JSON-RPC，最多4个fetch worker、64个pending、10000个key、512MiB总解码/磁盘预算及16MiB单响应；MISS异步采集并立即返回pending，不能授予范围可读。原始响应不重新序列化，按原provider/method/params绑定、metadata/gzip/原文hash校验，私有权限原子落盘/fsync。freeze后禁止新采集，缺失请求拒绝；关闭取消仅自己的curl进程组。空连接/半header回归同时覆盖HTTP解析前的2秒超时与有界关闭。

prime_current_bank以临时普通Go owner逐次发现缺失输入，每轮失败的config/log/state全部清除；只有普通HTTP baseline为200、子进程仍活且block完全匹配固定proof，才返回primed。采集器没有snapshot注入或ready文件。重试由新enqueued/pending驱动，永久失败key立即停止；总900秒、最多96次。近期签名录制模式明确标记RETAINED_RECENT_SIGNED_RPC_BYTES，owner及Router均保留1小时新鲜度；旧未签离线模式的10年容限没有用于本模式。HTTPS直连仍单独标为LIVE_EXPLICIT_SIGNED_RPC。CLI支持显式proof/root/source及精确预期启动错误，不把任意启动失败算作签名负控。

Ethereum主网块26464230、hash为0xac965af6cf1f4725fb1bd664aa973dd67ad670cb0616aa8bdc42f3289f45da4d，由原固定检查点跨委员会验证，509名签署者。固定proof的SHA256为c3d881f6849f84bac285b9f8badbddc0d3d1f9180d02425d2e23706279b19134。正文/回执原源为MEV Blocker与Publicnode，证明端点为0xRPC；这些URL选择和录制不证明独立故障域。采集过程中预采12条及新增40条；清除其中6份未用于本bank的旧高度响应后，保留46份必要原始输入，解码1664990字节、磁盘298648字节，其中6份eth_getProof。普通owner完成全部原门禁后，四个方向的ExactIn/ExactOut报价与独立整数公式和完整snapshotRef一致；首个报价2440ms仅表示冻结输入后的处理时间，采集预热约75.565秒、26次有限尝试另行记录，不能合并成直连启动SLO。

两个正常Router完成单实例退出与冷重启，均不建本地状态KV、不调用外部RPC；Go owner保留原本地状态重启，同一block的writer_epoch/online_generation/journal_seq均由1变为2，canonical_epoch不变，再次四向报价通过。签名改动一位后精确返回PROVENANCE_BEACON_UPDATE_SIGNATURE，HTTP quote503。冻结后的正常报价、拒绝和owner恢复均无上游fetch。source disclosure保留consensus_verified=false，验收production_ready=false；输出INDICATIVE，不授予VERIFIED或完整共识资格。

刷新工具保持900秒有限等待，仅对精确网络失败和参与率不足重试12秒；拒绝时确认proof逐字节未变，成功时校验CLI与实际proof的执行height/hash一致。其他错误、owner退出及超限立即失败，原600秒journal连续提交断言保持。新增/受影响的18项签名模式、16项采集器、15项普通owner预热、9项刷新控制测试通过，既有连续性/重放、17项进度文档、6项审计控制与统一契约门禁均通过。本轮未修改Go/Rust生产校验和各自预算。

私有/tmp仅保留下一项连续窗口仍需的固定输入、采集cache及当前验收可执行文件；失败attempt目录、无用SSH转发原型/manifest和旧测试输出已清除，不在项目内保留新的结果报告。后续先完成同P0-1的真实连续高度及raw重放，不跳到低优先级租户/分析等功能。原26工作包完整验收仍为0，整个设计目标继续。

<a id="signed-window-input-planning"></a>

## 签名窗口输入的有界规划、传输重试及录制重放入口（2026-11-22，阶段246，P0-1）

SignedRPCCapture新增经原文件完整校验的请求清单，返回原provider/method/params和响应hash，不返回可用来伪造新状态的模板响应。prefill_signed_range最多规划128个连续高度、8个pending，先读取实际头并检查父链及签名目标，再按已知方法的确切block selector取得真实正文、回执、code、call、storage和proof；不会替换call data、地址或storage key中的同形hash。重复原provider按原身份去重；未知或动态请求交由普通owner发现，规划完成本身不授予可读或freeze资格。

普通owner连续性与归档重放工具新增显式--retained-signed：所有正文/证明端点必须为无路径的literal 127.0.0.1有效端口，默认HTTPS模式没有放宽。仍使用1小时baseline新鲜度、60秒普通RPC和600秒连续journal验证，归档重放只消费实际发布前缀的原始输入及staged digest；结果标记RETAINED_RECENT_SIGNED_RPC_BYTES、production_ready=false、consensus_verified=false。

真实采集保留失败事实：26464452至26464484的32高度窗口在0xRPC返回historical state unavailable时失败。对同一证明请求，MEV Blocker实际返回有效证明，因此后续将它同时用于主正文与证明，正文第二源仍为Publicnode；这只有两个原URL，不能声称第三独立来源或独立物理故障域。26464612至26464644窗口的初次采集及原期限内最多两次重连均因传输失败退出，未进入连续发布验收。仅诊断的继续采集观察到curl(56) HTTP504；诊断输入不计作窗口通过。

针对传输失败，curl_fetch最多尝试三次，只重试传输层CAPTURE_FETCH_FAILED；共用原120秒、16MiB解码输出及64KiB stderr预算，失败片段消耗预算但不会拼入后续响应。JSON-RPC语义错误、取消、存储和预算错误不重试。总期限同时检查单调钟和实际时间，防止主机挂起或时钟回拨延长原预算；退出回收仅本操作的子进程。

新采集轮的普通owner取得26464708、hash 0xd1913ee3526ce8ca3b98a68a40cd685a0e86b5ee195ec30faea66698cd7cff26的完整baseline，固定proof SHA256为e5e7da3d1a76609bbf04cd2acff149594b730c4cf5b6df2cadabd3510f2d1ea3、495名签署者，13次有限输入发现后通过。等待期间实际时间跨过约6.7小时，后续有效签名head已为26466718，差2010块；128块上限正确拒绝，旧基线没有作为新鲜连续窗口发布。本轮不新增32高度、双Router窗口或完整raw重放成功声明，阶段245单bank结果保持原范围。

采集器27项、窗口规划16项、信标相关22项、归档重放2项、当前V2模式18项及普通预热15项，共100项通过；包括真实本机子进程的失败片段隔离、三次上限、累计字节、取消、共同期限和wall前跳/回拨。另17项进度文档、6项审计控制及328/12/191份契约门禁通过。子agent环境曾禁止一个socket测试bind，root可监听环境的完整27项已复跑通过；没有将该环境错误记成逻辑通过。

已清理两个不再使用的私有响应cache，共1334个文件、16378932字节，并在核对失败事实后删除旧trial及其诊断/结果目录。下一轮只保留41份经原字节/hash复核的必要请求输入、固定proof/config与当前验收可执行文件；旧输入只是请求形状，不能恢复新鲜度资格，项目内不新增测试结果。慢链隔离及Ethereum冗余sender字段核验继续按同P0-1开发，分别验收和提交。原26工作包完整验收仍为0，完整设计目标继续。

<a id="go-per-chain-scheduling"></a>

## Go每链调度、慢源隔离与完整生命周期（2026-11-22，阶段247，P0-1）

普通Start先完成共享资源初始化，再分别启动各链的初始probe与owner actor。每链只等待自己的probe；首条合格链或真实lease standby完成后开放服务，其他链仍由已登记worker管理，不将pending当作ready。全部链失败仍按配置顺序返回首个错误。普通app改用非阻塞ScheduleTick，每链固定一个worker、最多一个待处理wake；显式Tick并发处理可取得actor的链，忙链不累积等待goroutine。WS/HTTP读取间隔仍在链内执行，慢链不会占用健康链后续轮询。

同一链的session、recovery、scope与writer操作保持串行；session/recovery映射仅使用短锁。每链独立续租timer也在自身actor内运行，不另开无锁heartbeat写入。Scope请求只竞争目标链，raw replay仍先在目标actor内冻结ref/writer，解锁读取来源，提交时重锁并核对完整ref、writer及runtime fence。Kafka配置下没有该链session就拒绝reanchor，防止另一链已开放HTTP时尚未持租的链落入本地发布分支；最终门禁同时拒绝closed状态。

Close先取消服务源生命周期，再无全局I/O锁等待已登记操作和worker退出，raw维护关闭时同样能取消普通RPC。退出在仍有权威时以独立一秒上下文尝试peer撤权，随后本地Suspend、失效runtime owner并释放writer。失败Start只取消本次子run，清理可重启句柄而保留持久store/catalog/PG，可用新context重试；旧context不能杀掉新run。成功run取消后重复Start明确返回取消错误，不能借旧成功记录报告可运行。

正常RPC夹具分别阻塞identity、pool及初始provider探测，覆盖两种链顺序：健康链先取得HTTP200并连续前进两高度，慢链仍503；64次重叠唤醒最多一个inflight和一个待办。raw开/关均覆盖Close取消及故意迟到的有效响应，scope/raw目标actor及generation fencing反例通过。首轮全包发现清理测试适配器的nil client和checkpoint取消测试的旧生命周期假设；补充nil保护，并将后台发布器取消与全局源取消分别验证后通过，未放宽全局取消撤权。

自行启动的私有回环PG13/Kafka3.9.1中，A已持有120秒租约但普通finalized RPC阻塞，B仍完成真实baseline及两次BLOCK_COMMIT；无新RPC或ScheduleTick时PG ExpiresMS继续增长，证明独立续租。只读Kafka peer按完整引用连续应用，Close的ONLINE_SCOPE_REVOKED令peer失去可读；A晚响应不产生记录，A/B均立即可由peer取得租约。新门禁3.007秒，原post-Acquire失败释放门禁4.018秒，合并race4.894秒均通过。结束后已停止全部自行启动的服务并删除其数据、topic及临时脚本。

最终Go全包通过（本轮evm152.535秒、service168.450秒），vet通过；普通生命周期/慢链/scope/raw/runtime的race及全包修复专项race通过，契约门禁保持328/12/191。本轮不保留新的测试报告或二进制。此项证明本机受控RPC和真实中间件下的链级隔离；Rust慢链调度、真实多链完整来源与独立故障域仍需推进，原26工作包完整验收仍为0。

<a id="public-mainnet-dev-rpc"></a>

## Ethereum 与 Solana 开发配置直连主网（2026-11-22，阶段248，P0-1）

默认开发配置使用 Ethereum 主网的 MEV Blocker、PublicNode 和 Solana 官方主网 RPC、Pocket Network。解析器核对固定创世哈希，两个池使用各自的主网程序或合约身份。Router 消费两条主网链的 owner 快照。离线夹具使用测试配置。公开 Solana 节点中有端点能读块，但对本轮 11 个账户的完整批量请求返回 HTTP 403；最终选择的两个 Solana 端点均返回完整批次，Raydium 程序代码摘要和部署 slot 463522946 均匹配。

2026-11-22 北京时间 08:41–08:42，`make -C integration public-rpc-test` 通过。Go 在 Ethereum 块 26467038（`0xbc23b083d143c8b2bdc98d0955ab9b02e31012946f017b44b7f1e4e480cfaac9`）发布 DAI/WETH 的 Uniswap V2 完整池状态，Rust 在 Solana slot 472343814（`DwZPitXjkFRi6vS9XPjjy3pubNgxP4fNjPCcpwAp8Bug`）发布 Raydium CPMM 固定版本的池状态。以普通解析器 HTTP 基线为输入，逐项与公开 RPC 的同高度块哈希、父哈希和时间比对。单独使用 Pocket Network 启动 Rust 也在 slot 472343574 发布了相同池的快照。

其余六链通过公开 RPC 核对链 ID、固定创世哈希与 finalized 块。观察高度分别为 Base 60582915、BNB Smart Chain 134984940、Arbitrum One 526374762、OP Mainnet 159854011、Polygon PoS 97735130、Avalanche C-Chain 101024097。本轮覆盖两个池的实时状态读取与六条链的块观察。阶段246的连续签名窗口、Solana 独立 bank 证明、Yellowstone 流及整体生产验收继续按原门槛核对。

三服务联调首次启动时 Ethereum 基线超过开发配置的 20 分钟上限，Go 返回 `BASELINE_TOO_OLD`。节点公布新的 finalized 块后，以相同配置和全新状态目录重启。Router 的 `/v1/quote-options` 同时返回两条链 ready，并列出了 Ethereum 块 26467069 的 DAI/WETH 池和 Solana slot 472344468 的 Raydium 池。

[公网复验入口](integration/tools/acceptance/public_rpc_smoke.py) · [运行说明](integration/README.md)

<a id="ethereum-rpc-sender"></a>

## Ethereum 正文中的发送方地址（2026-11-22，阶段249，P0-1）

交易的签名编码不包含 RPC 单独提供的 `from` 字段。普通 Ethereum 正文读取和原生正文证明读取都在已核对交易身份后，使用该块高度、时间对应的主网签名规则恢复发送方。声明地址与签名不符、格式无效或显式 null 会拒绝整个正文。旧记录可以省略该字段。适用范围为 Ethereum 主网标准交易。

使用主网块 26355865 的原始正文，通过普通适配器验证两种入口。只改 RPC 的发送地址时，交易的签名字节及哈希保持一致，两个入口均返回 `BLOCK_INTEGRITY_TRANSACTION_SENDER`。另以 Frontier/Homestead 的 high-S 和 Cancun 激活前后的 blob 交易核对分叉规则。专项 race 与 Go vet 通过。已有主网开发配置的正式 Go 进程也在块 26467038 读得完整池状态。

[源码](chain-parser-go/internal/chain/evm/transaction_sender.go) · [回归样本与测试](chain-parser-go/internal/chain/evm/transaction_sender_test.go)

<a id="rust-chain-workers"></a>

## Solana 多链启动、续租和关闭（2026-11-22，阶段250，P0-1）

Rust 解析器分别启动每条链的工作任务。第一条完成初始化的链允许服务启动；HTTP 就绪检查仍核对提交引用、数据年龄和写入权限。每条链最多积压一次轮询，慢链上的 RPC、数据恢复和 Kafka 发布不再占用其他链的写入锁。独立计时器为已有 writer 续租，状态和正在写入的 manifest 引用按链保护。关闭服务时，停止接收新工作，取消只读 RPC，等待已开始的写入和 WAL 落盘，然后发送撤权、释放租约。

慢 RPC 回归覆盖了交换两条链的配置顺序、128 次调度通知、另一条链提交新 slot，以及关闭时抑制迟到响应。隔离的 PostgreSQL 13 和 Kafka 3.9.1 实测读取到两条连续基线和一条撤权，HTTP 调用者中途取消仍能完成已接收的提交。另以普通 Rust 服务、受控 Yellowstone 流和同一类真实数据库复验 1、24、720 小时等价 slot 间隔，以及空 owner 状态恢复。真实时间停机和生产故障域演练仍待验收。

全套回归的重复运行发现，Yellowstone 测试在发出 HTTP 服务中止请求后立即重开 RocksDB，前一个 HTTP 连接有时仍持有数据库锁。测试服务已改为等待连接处理完毕再重新打开同一目录。此前失败的关闭、重开和重新订阅用例连续八次通过；PG/Kafka 额外验证了两个链隔离用例和受控流的撤权恢复。

主网复测使用 Go 和 Rust 正常入口读取池状态。Ethereum 的 DAI/WETH 位于区块 26467325（`0x04b70aa8b7b3190160e72c55dc74117a0a1a05c93cab2a7f2f740cda8cf1b53c`），Solana 的 Raydium CPMM 位于 slot 472359176（`86nfds8d66uWT9Gf7ims7qDNv4zZqDtAd1fqCvUiVeuq`）。PublicNode 和 Pocket Network 分别回读了对应区块。另六链完成链身份和 finalized 区块检查。全程未广播交易。

清理三批旧 Orca 和 Raydium 测试输出，共 4055 份 JSON，磁盘占用 2564354048 字节。保留三个默认回归样本，清理后 58 项相关测试通过。另清理 16 份已执行完毕的阶段文档编辑脚本。完整流程可重新采集 RPC 和 API 输出；项目中不新增本轮测试报告。

[链调度源码](chain-parser-rust/crates/solana-ingest/src/service_chain_workers.rs) · [多链回归](chain-parser-rust/crates/solana-ingest/src/service_startup_tests.rs) · [PG 与 Kafka 回归](chain-parser-rust/crates/solana-ingest/src/service_writer_recovery_tests.rs)

<a id="public-signed-ethereum-windows"></a>

## Ethereum 主网连续签名窗口、报价和原始输入重放（2026-11-22，阶段251，P0-1）

`make -C integration public-signed-window-test` 从版本库的主网开发配置和固定信标检查点启动。Go 校验信标 BLS 签名并取得两个递增的最终化头，采集器按完整区块、回执、合约调用和状态证明的原始响应录制输入。共享主机的请求有统一的发送间隔，遇到 HTTP 429 或 503 时遵守 Retry-After。单次请求保留三次尝试、120 秒及原字节上限，采集总时限 1800 秒。冻结输入后再次运行普通 Go 服务，按高度逐条比较父哈希、journal 序号、writer、generation 和 scope，再启动两个 Router，核验双向 EXACT_IN／EXACT_OUT 的原子单位结果、健康副本和冷重启。独立重放器读取 owner 存档，核对签名头、原始正文、回执和对应提交的摘要。

|主网窗口|采集|冻结及独立回放|
|---|---|---|
|26467165 → 26467197|912.855 秒，使用已认证的请求模板；结束于 `0x5bb36c21f072bedce3a3666ed32e90f208431017fc9908cdda4599e1cd89b7db`|32 条连续提交、33 个含起点的区块，冻结后 0 次上游请求|
|26467293 → 26467325|1277.127 秒，从空目录启动，零模板、零旧签名文件。两个正文端点分别完成 chainId 和创世核验，录得 1357 条响应。起点 `0x37d749e8a47466601bbc7680e85f130027ee4223eb796017d0ebcf34d8ea0cab`，终点 `0x04b70aa8b7b3190160e72c55dc74117a0a1a05c93cab2a7f2f740cda8cf1b53c`|32 条连续提交、33 个含起点的区块，冻结后 0 次上游请求|

较早的 26467133 → 26467165 尝试因公开节点返回 HTTP 429 失败。空目录初试也发现缺少两个正文端点的身份响应；现在显式采集并核验两个端点，原有拒绝条件保留。两个通过窗口均使用 MEV Blocker 和 PublicNode 正文，证明端点与第一正文端点重合。供应商故障域独立性仍未证实。本项核验 DAI/WETH 的 Uniswap V2 完整池范围、已签名的执行层引用和对应读集；全链执行后状态、Solana 独立 bank、其他协议和生产容量继续按原设计验收。

常规复验输出起止区块及计数，自动删除临时目录。显式提供 `--output` 时可保留输入以逐条审计。64 项签名采集、规划和验收流程回归通过。

[验收入口](integration/tools/acceptance/signed_window_evidence.py) · [采集和限速](integration/tools/acceptance/signed_rpc_capture.py) · [复验说明](integration/README.md)


<a id="public-mainnet-pool-owners"></a>

## 八条主网的实时池状态与普通解析器验证（2026-11-22，阶段252，P0-1）

北京时间 10:23–10:28，`make -C integration public-rpc-owner-test` 一次运行中八条主网全部通过。使用 Go 和 Rust 正式运行入口、空状态目录和公开 HTTPS RPC。校验链身份、固定创世哈希和 finalized 块或 slot，并按同高度核对解析器发布的块哈希、父哈希和时间。下表记录实际快照，块哈希可按高度在主网上复查。

|主网|区块或 slot|块哈希|池状态|
|---|---:|---|---|
|Ethereum|26467549|`0xe2ed9cee9803425bf4633562803449fa57d42037a4a09106e086f48b34daf9c1`|DAI/WETH Uniswap V2 完整快照|
|Base|60613521|`0xe3e86a149b191254654d45ab81e0c0f8ba703fe5d0202bf6597f533128bc2a1d`|Uniswap V3，256 个初始化 tick|
|BNB Smart Chain|134998582|`0x177d491f5513a2ecb2b760fdb75cb91ea7e1b39110fd0a39fb2c7da9c0ca7c43`|Pancake V2 双边储备|
|Solana|472372615|`2rtvJQFngdPEqVPkAwbnEpGihTkKKnYqC7PbwGuEpPy9`|Raydium CPMM 快照|
|Arbitrum One|526393225|`0x0c6cafae4b8d4909b15f8209200dbaac8c572f81a6792a8a2ec32c2f08913a29`|Uniswap V3，256 个初始化 tick|
|OP Mainnet|159857248|`0xdcee4020e6b8f677bf726a419e9839f25b14783a7a7509b64b6744ca7dbc0663`|Uniswap V3，203 个初始化 tick|
|Polygon PoS|97739323|`0xb86ddba5ca27fc615a7b19e8a4f9305086251664c4fc9400adafe206da938e51`|Uniswap V3，79 个初始化 tick|
|Avalanche C-Chain|101030093|`0xaede38cde646e070c71f151fa41064b4bc8ed2302eb7df9bbf142dc248109def`|Uniswap V3，117 个初始化 tick|

六条新增 EVM 链均固定部署地址、运行时代码 SHA-256、工厂、两种代币和费率。Go 解析器使用对应链的原生区块、交易及回执校验规则。快照发布后通过公开 RPC 在同一高度读取 V2 储备或 V3 的价格、tick、流动性，逐项比较并检查结束块头。另以 Tenderly 和官方端点分别作为 Arbitrum 第一来源，在同一块成功复验。免费端点曾返回 HTTP 403、429、超时或过期的 finalized 头。保留 60 秒请求时限及既有 20 分钟 EVM、2 分钟 Solana 新鲜度要求。来源失配和超限样本均未计入通过次数。

真实读取发现，密集的 Uniswap V3 池在一个 bitmap word 内可有 256 个初始化 tick。Go 增加可选池配置 `v3.tick_read_concurrency`，省略时串行，本轮并发 4。所有请求固定在同一块哈希，沿用同一字节预算、来源检查和原始输入记录。返回顺序固定为 bitmap 顺序，含无效或缺失响应时整池失败。两套 Rust 配置读取同一字段，历史采集在并发回调时串行计入总字节预算。本轮覆盖每条链的一个池，Solana 独立 bank 与同源 Yellowstone、全协议覆盖、端到端生产 SLO 及完整工作包验收仍待完成。

同一 Go 进程也同时发布了 BNB Smart Chain 块 135001670 与 Avalanche 块 101031302 的两个池。复验工具校验池与块引用各自的 chain key，拒绝把另一条链的同地址状态计为通过。再次执行八链严格入口时，Base 和 OP 的 finalized 块超出时效，两条明确返回 `BASELINE_TOO_OLD`，其余六条通过。等公开源提供新 finalized 块后，Base 与 OP 在同一个 Go 进程分别发布块 60623228、159858049，同地址 WETH 的两个池分别读取 256 和 203 个已初始化 tick，并经同高度合约调用复核。

本轮另清理 837 份旧报告、预检日志和一次性源码编辑脚本，共占用 964141056 字节。四份最大的 Balancer 输出均有仓库内的原始输入和生成命令。清理后覆盖 68 项相关回归，其中两个测试类使用项目固定的 Node v25.3.0 复验。613 份属于旧 Kafka 应用和 GC 日志。本轮另起真实 PostgreSQL 与 Kafka，验证测试程序把此类日志放入每实例临时目录，退出后删除，Kafka 安装目录的文件及修改时间保持一致。Go 全包普通测试、vet、race 与两个 Rust 配置包回归通过。全量 Go race 的 EVM 包超过首次运行的十分钟限制，在延长测试超时后通过。

[公开主网复验](integration/tools/acceptance/public_rpc_smoke.py) · [固定部署与公开端点](integration/config/chain-targets.json) · [运行方式](integration/README.md) · [V3 状态读取](chain-parser-go/internal/chain/evm/v3.go)


<a id="public-mainnet-stale-head"></a>

## 公开 RPC 过期头切换与主网池储备复核（2026-11-22，阶段253，P0-1）

Go 普通在线解析器现在先检查移动标签的时间。当首选端点返回哈希有效但已过期的头时，解析器向其他已配置的健康端点请求同一标签，并检查链 ID 和创世哈希。候选端点须按高度确认已见旧头，首选端点须按高度确认候选头，随后仍从首选端点读取完整正文。Avalanche SAE 同时检查待发布执行块。所有探测共用此次证明的字节预算和截止时间。未来时间、哈希冲突、记录失败、取消和预算耗尽都会阻止本次发布。历史数值高度及已签名的固定引用继续按原目标读取。

北京时间 11:09–11:21，正式解析器分别接入七条 EVM 主网及 Solana。Ethereum 在 26467804 发布 DAI/WETH 快照，增加储备复核后再次在 26467836 发布，PublicNode 在同块读回两个原始整数 `3167250035322729866418488` 和 `1896973864188940389507`。Base 60628953、BNB Smart Chain 135004841、Arbitrum One 526401634、OP Mainnet 159858651、Polygon PoS 97741154、Avalanche C-Chain 101033014 均完成同块合约状态复核。Solana 在 slot 472388290 发布 Raydium CPMM 快照。随后将账户来源限定为官方主网 RPC，Pocket 在 slot 472388907 读回同一块哈希 `9k9sXXoAw2AKgagQLSq6xg4LWMKUJEeubPeAR9YBx81T`、父哈希和时间。两侧金库余额分别扣除协议、基金和创建者累计费用后得到原始整数 `125400457955`、`843930427296813`，与池快照一致。Solana 这项跨来源复核覆盖块引用，独立 bank 仍待验收。

另用 BNB Smart Chain 主网演练首选端点滞后。将首选端点的 finalized 读取固定在此前实测的 134998582，其余调用实时转发至原公网来源。备用端点提供块 135004698，首选端点提供该块完整正文。复查哈希 `0x76e5e498ca2f5b60f1096bbfeec6fc26b473073dda5e1aeedb1079c32f71731f` 和 Pancake V2 两侧储备均与解析器一致。最终版本在相同滞后条件下再次通过，选定块 135006263，哈希 `0xa3b2568ed710bff4e120d8894560c1c3a90fca9aef16cf54c55edacfd00ca521`。再将两个端点均固定到真实旧块，解析器发出两次 finalized 请求，返回 `BASELINE_TOO_OLD`，未请求新区块正文或发布池快照。此次滞后由演练注入。实际探测中 OP 两个端点也曾同时落后约 23 分钟，待端点推进后在既有 20 分钟限制内通过。

11:26–11:33 以最终版本再执行八链验收，Ethereum 26467868、BNB Smart Chain 135007199、Solana 472390541、Arbitrum One 526405425、OP Mainnet 159859165、Polygon PoS 97741896、Avalanche C-Chain 101034248 通过。Base 首次触及 60 秒截止时间，整次命令返回非零。保留相同端点、时限及 20 分钟时效要求单链重试，在块 60634671 通过，256 个已初始化 tick，链上价格和流动性与快照一致。

Go 全包与 vet 通过，移动头和时效用例通过 race 检查。全包复跑曾暴露 checkpoint 轮转回归的等待条件只观察健康链发布，故障链尚未轮到即可断言失败。现等待健康链发布和故障计数两个事件，保留目录、恢复和单次并发断言，连续 12 次 race 及全包复验通过。跨服务契约检查覆盖 328 个契约文件、12 个普通 fixture 和 191 个公开来源文件。

历史测试目录中 12 份已按内容摘要固定的程序有 3 组不同字节。核对 SHA-256 后将 9 份重复副本改为硬链接，保留每份旧记录的验证路径，减少重复文件内容 570993248 字节。删除 54 个过期日志及构建文件共 49934971 字节。新的 Solana 三池离线验收为同一轮的各池共享按哈希归档的程序，44 项原有回归通过。主网测试运行时生成的状态、凭据、报告与构建目录按轮清理。

[移动头选择](chain-parser-go/internal/chain/evm/native_head.go) · [回归与故障场景](chain-parser-go/internal/chain/evm/native_head_test.go) · [公网验收](integration/tools/acceptance/public_rpc_smoke.py) · [主网配置与复验](integration/README.md)

<a id="solana-raw-rpc-replay"></a>

## Solana 主网账户 RPC 的逐次归档与离线重放（2026-11-22，阶段254，P0-1）

普通 Rust 解析器在每个来源完成基线解码时，记录有序的原始响应编号、来源会话、端点标识与快照 SHA-256。相同响应可复用同一条原始记录，结束时再次核验块头的调用也出现在顺序中。同一会话中若连续两次的全部调用编号和快照摘要相同，则复用读取编号。索引按读取编号连结前项摘要，纳入原归档空间配额及持久化确认点。只读入口核验确认点、读取顺序、每一条请求的方法与参数、全部账户布局和快照摘要。缺少索引的旧档案保留原始证据，新写入的成功读取可按编号回放。

真实主网验收使用开发配置的 Raydium CPMM 池 `B5oZhRDmdazrJoDSJSL9EQw6Fe9M46aG98bvXH3h1QNi`。最终版本先由 Solana 官方 RPC 在 slot 472399630 读取账户，Pocket 复核块 `2X1FhMoRUG855xVgD5HLFG4L5ExM4RHREeH7rbBhTQj8`。重启后由 Pocket 在 slot 472399677 读取账户，官方 RPC 复核块 `9ExxURJJoYJd7Nf56e2hitCQKjLAumPDaczj8iG79eYh`。来源会话、在线代次、提交序号和 writer epoch 均推进。关闭解析器后按归档重放两次读取，各有四个调用和三份不同响应，包含两次相同的块头检查。两个 slot 的净储备原始整数均为 `125400457955` 和 `843930427296813`。两次重放的池与来源字段分别匹配各自发布的快照。第二个来源复核了创世块和同 slot 块引用；独立账户 bank、Yellowstone 实时流和生产故障域仍待验收。

本轮另用正式解析器复验八条主网，Ethereum 26468060、Base 60644193、BNB Smart Chain 135012109、Polygon PoS 97743294 和 Solana 472400698 通过。八链整次命令返回非零。Arbitrum 与 OP 曾返回超出 20 分钟时效的 finalized 头。使用普通配置重试时，Arbitrum 与 Avalanche 的区块正文复核在 60 秒截止时间内未完成，OP 再次触及时效限制；备用端点还出现过 HTTP 403。13:14–13:17 再用普通开发参数顺序重试这三条链，Arbitrum 超时，OP 的 finalized 头超龄，Avalanche 的第二个见证端点在回执阶段超时。独立通过公开 RPC 在 Arbitrum 526413864、OP 159860584 和 Avalanche 101037122 核对了指定池的代码、合约身份、同块状态及收尾块哈希。这三项合约读取与阶段253的完整解析器结果分别记录。

Rust 工作区 746 项通过，15 项按原有条件跳过。针对归档和分片的最终版本复跑 74 项通过。另起 PostgreSQL 与 Kafka 并建好四个单分区测试主题后，验证了慢链隔离、取消与提交、故障链释放 writer 和健康链继续发布，两项通过。格式、Clippy、11 项公网验收工具回归、17 项进度文档回归及跨服务契约检查通过。普通归档恢复与离线读取的回归覆盖了受损调用顺序、来源和摘要不一致、缺索引的旧库、配额耗尽以及持久化确认点写入失败。

清理 15 份已并入项目的 Rust 草稿、21 份旧预检日志与 2 个可从现有 Go 源码离线重建的原型程序，共 30475992 字节。重新构建两个程序后，用保留的固定输入和原始 OP 客户端重算 448 个历史用例，两个相关测试组共 8 项通过。本轮临时构建、数据库和 Kafka 状态目录合计占用 6682529792 字节，关闭后清除。

[基线解析与重放](chain-parser-rust/crates/solana-ingest/src/rpc.rs) · [归档持久化](chain-parser-rust/crates/solana-ingest/src/source_wal.rs) · [主网验收命令](integration/README.md)

<a id="public-evm-witness-batch"></a>

## EVM 主网原生见证与 V3 批量归档（2026-11-23，阶段255，P0-1）

每个见证端点在既有截止时间和共享字节预算内，验证并复用该端点自己的编号块正文读取回执。正文承诺、回执根、Bor 的业务范围和 Avalanche SAE 的结算窗口仍按链规则检查。读完回执后重新查询链 ID、创世块和正文，SAE 同时重查结算块。回归覆盖两来源各自的调用顺序、总预算耗尽、收尾创世块或结算锚点变更。

Arbitrum 真链响应显示逐个读取 V3 tick 超出解析器的启动截止时间。主网验收配置启用每批最多 16 个同块哈希的 `eth_call`，至多四个并行请求。通用配置保留逐条调用的默认值。批次包含调用者和 EIP-1898 块引用，全部成员按请求编号校验，完整 tick 列表按 bitmap 顺序写入。批次按成员数量计入 RPC 限流并保留原始请求和响应。缺项、重复编号、错误 ABI 长度、归档失败均会拒绝发布。

11 月 22 日使用普通 Go 解析器和现有在线时效配置通过以下采样。每条快照完成原生区块和回执见证，随后在表中第二个端点核对同高块引用、池代码、工厂、代币、价格、tick 和流动性。

|主网|finalized 块|初始化 tick 数|配置的主端点|同块复核端点|
|---|---:|---:|---|---|
|Arbitrum One|526432690|234|`arb1.arbitrum.io/rpc`|`arbitrum.gateway.tenderly.co`|
|OP Mainnet|159863252|203|`optimism-rpc.publicnode.com`|`optimism.gateway.tenderly.co`|
|Avalanche C-Chain|101042607|118|`avalanche-c-chain-rpc.publicnode.com`|`avalanche.drpc.org`|

Arbitrum 块哈希为 `0x8e62515b5b42a97ed8195bb12f5d8e8cb085cc08e26fb029d536df377dedcb94`。保留原始响应后，从空目录以两个本地归档端点重启解析器，两个来源分别回答 48 和 11 次读取，主端点归档含 15 个批次。完整快照的 SHA-256 为 `c163fe969b892f7f5cf052ba117cc15240f856a4248c7ab4c3cc93f2e5302535`，与在线发布一致。历史样本跨过新鲜度窗口后，通过隔离配置复核离线功能，在线验收命令沿用正常时效。

11 月 23 日，`--evm-replay` 在正常时效内完成以下主网的实时采集、同块合约复核和立即离线重放。两个来源均参与重放，所有请求均来自对应端点的归档。

|主网|finalized 块|两个来源的回放请求数|实际回放批次数|完整快照 SHA-256|
|---|---:|---|---:|---|
|Ethereum|26475461|MEV Blocker 49、PublicNode 11|0|`f4a7883df8d51f34235b563597dc423fd0d47d4cb252bddd4c68c9a64943a06a`|
|Base|61091540|官方端点 289、dRPC 11|0|`98f4fb1036f50a3fa108a737cbc349f2bcda032dfb84d051f852482ef32bc387`|
|Arbitrum One|526705329|官方端点 49、Tenderly 11|16|`d1e956ee984f69e5d4fbdd249acadfd331d3eb16e1b3962904a64ae11beabf70`|
|OP Mainnet|159904962|PublicNode 46、Tenderly 11|13|`1592d6efff21b786a6044b28c1c5bb07dcf4014ec9a36a72e964af662079d479`|
|Polygon PoS|97802543|PublicNode 43、dRPC 11|9|`69235ad745dd9f7800dde878ba72b666958313482b9f6400363be99a6c0eeb56`|
|Avalanche C-Chain|101133547|PublicNode 62、dRPC 19|8|`7a06b65c7d16e7ac1b0bc619681b29c14cc4a11b543134c45dc1197d2a2e1afa`|
|BNB Smart Chain|135209736|BNB Chain 31、Defibit 11|0|`494785f6fb4b1ab099a22100f359a388c4881eb3e608914097a35fba94e4c3e3`|

11 月 22 日后续整组复跑未通过，多端点的 HTTP 请求超过 15 秒，Go 在既有截止时间拒绝启动。11 月 23 日再次通过公开 RPC 核对八条主网的链身份、创世块与 finalized 块。OP Mainnet 首次采集遇到 `BASELINE_TOO_OLD`，更新的 finalized 头进入时效窗口后采集和重放通过。Base 官方端点对大批次返回每秒 25 调用的限流错误，dRPC 免费端点最多接受 3 个成员，另一个试用端点 PublicNode 的创世块请求返回 HTTP 403。Base 保留四个并发的逐条读取，并在出现合格的新 finalized 头后采集和重放了 256 个初始化 tick。

Solana 在 11 月 22 日复跑重放命令时遇到 `BASELINE_DEADLINE`，阶段254中两个经正式解析器验证的 slot 见上节。11 月 23 日，Solana 官方端点和 Pocket 分别从 finalized slot 472815374 和 472815378 返回 Raydium CPMM 池的 11 个依赖账户。分别核对了 Clock slot、固定的 Raydium 和 SPL Token 部署 slot 与程序代码摘要，再读取对应 slot 的块引用。五次并行双端点探测中，四次账户快照落在不同 slot，一次 RPC 拒绝方法参数，未取得跨端点的同 slot 账户证据。

实测 PublicNode 的 Yellowstone `GetVersion` 返回 `grpc-status: 7`，需要个人 token。本机有 16 GiB RAM，Agave 的主网节点建议至少 256 GB RAM 和独立的大容量 NVMe。后续主网同源 Bank 验收需要可运行原生 exporter 的节点，现有冻结 Bank 导出器已覆盖本地 Agave Bank 及通知回调测试。 [PublicNode](https://solana.publicnode.com/) · [Agave 节点要求](https://docs.anza.xyz/operations/requirements)

Go 全包、vet、见证及 V3 专项 race、两套 Rust 配置测试、跨服务契约和 17 项进度检查通过。Rust 解析器及 Router 的配置测试分别通过 51 和 50 项，各有 9 项按既有条件跳过。Python 公网与归档相关测试共 33 项通过。阶段255未重复完整构建 Rust 解析器二进制。

两份历史中间测试输入由保留的 OP 原始执行向量及仓库现有导出测试重新生成，分别通过 SHA-256 逐字节核对。已删除旧中间文件共 59551318 字节。新一轮的 16 个 EVM 采集与诊断运行目录、57421304 字节的 Go 测试程序，以及占用 2637852672 字节的本轮本地构建和验证目录均已清理。

[正文与回执见证](chain-parser-go/internal/chain/evm/chain_receipts_witness.go) · [批量读取](chain-parser-go/internal/rpc/eth_call_batch.go) · [公网与离线验收](integration/README.md)

<a id="public-evm-contiguous-journal"></a>

## BNB Smart Chain 与 Polygon 主网连续 journal 及原始响应重建（2026-11-23，阶段256，P0-1）

新增 `make -C integration public-evm-window-test`。正式解析器从空状态启动，读取公开 finalized 头和固定部署。在同一代收集启动快照、连续 `BLOCK_COMMIT`、末快照与两个 RPC 的原始响应。验收配置以 2 秒轮询观测 finalized 短尾，沿用开发配置的时效、证明预算和双端点见证。首末区块同时核对归档中的 finalized 观测、journal 父链和同块合约状态。

离线入口按显式块号读取封存的编号响应，验证正常原生正文、回执和池状态。finalized 原始响应只用于证明采集时观察到的首末锚点。离线使用普通 baseline 状态构造逻辑和空 Pebble reducer，核对首末完整快照、启动记录、每条增量的引用、记录编号和池内容摘要，并按原序列化方式核对含 swap 的 staging 摘要。覆盖链上协议的已解码事实依赖现有适配器，本轮 BNB Smart Chain 与 Polygon 样本的已解码 swap 数均为 0。

|主网|启动块 → 末块|同代连续提交|两个来源的离线请求数|在线窗口 SHA-256|
|---|---|---:|---|---|
|BNB Smart Chain|135213856 → 135213872|16|BNB Chain 359、Defibit 121|`581442bc87e4802d57dcb5c1dcfaf9657025ea46f5dbd80e8f170490ddc23ef7`|
|Polygon PoS|97803728 → 97803734|6|PublicNode 240、dRPC 51|`2ae36e1d67dae57250fc569bafaacbf903b137a53fde234d986d2397c0e0f980`|

两个窗口的在线末块哈希分别为 `0x63950fa485a3816d71f80b5d7db02fd7e46e1b5f00aa66e5c20602548b8f0e19` 和 `0xda5d59aa63a44d674ef272fb7eccb5f51e6dbcbf1515eba922f6d4b0183964aa`。Polygon 离线实际处理 63 个 JSON-RPC 批次。较早一次 BNB Smart Chain 运行遇到 RPC 错误后切换代次，Polygon 也曾出现代次切换，验收拒绝跨代拼接。Polygon 另一次复跑取得初始快照，100 秒内未取得合格连续窗口。首快照同块复核在启动发布后即执行，避免公开 BNB 节点稍后剪枝时返回 `missing trie node`。

Avalanche 初始快照已绑定封存 finalized 响应的 `settledHeight` 和同端点父链。实测高度 101135144 缺少直接绑定该块的结算状态锚点，解析器返回 `EXECUTION_STATE_TARGET_NOT_DIRECTLY_SETTLED` 并撤销当前代。该链跨结算窗口的连续状态仍待验收。

Go 全包、vet、离线命令专项 race、跨服务契约以及 17 项进度文档检查通过。新增验收的 4 项 Python 回归在普通和优化模式均通过，阶段255的 15 项公网及离线回归继续通过。另验证了原始证据与连续窗口相关的 54 项 Python 回归。

采集目录和离线 Pebble 目录随测试结束清理，两个独立工作目录的归档、构建和日志共清理 299228 KiB。已核对本轮无残留采集或重放目录。

[主网连续验收入口](integration/tools/acceptance/public_evm_window.py) · [编号离线重建](chain-parser-go/cmd/replay-beacon-window/provider_window.go) · [原生结算状态门禁](chain-parser-go/internal/chain/evm/chain_receipts_coverage.go)

<a id="bnb-pancake-canonical-swaps"></a>

## BNB Smart Chain 固定 Pancake V2 池的 Swap 事实（2026-11-23，阶段257，P0-1）

在原生 BNB 区块和回执校验范围内，按已配置池的 Pancake V2 Swap ABI 解出交易索引、日志索引与四个无损整数金额。`evm-v2-swap/2` 标明新增覆盖范围。连续提交、重组与修复使用同一事实选择逻辑。Pebble 本地提交将可选 `normalized_facts` 与池快照和 journal 记录原子落盘。Go/Rust 的旧解码器兼容性保留，新 staging 和历史数据集使用不同身份，旧范围的空结果仍按原版本验证。

使用原 BNB Chain 与 Defibit 公开 RPC，通过正式历史读取入口复验 BSC 块 135214575（`0xc71f5d34f6b813a64e832f2378e2a51ffc823f3d166e25d276050592a31ae7d7`）。固定池 `0x16b9a82891338f9ba80e2d6970fdda79d1eb0dae` 有两条事实，同一交易 `0x483623549d63e583dd298961c2b06e29b631d4de8c5951dfca7aa338b756c58f` 的日志索引分别为 268 和 278。序列化原生回执证据后重新验算，事实 SHA-256 为 `92c12e7f82fe3c32cdf5f6dd20e52d2ba9761c8e3a7412ede4a658b488bd253e`，历史数据集为 `730eb48ae34834bd86bd39564d47fb5214ded6be480cec8ee8506b7a5571999c`。PublicNode 返回同一编号块和这两条日志，交易与日志索引和四个金额逐项相符。

连续窗口的首次复跑因代次切换拒绝。带日志的下一次复跑已在 135216197 的实际 HTTP journal 读到两条持久化 Swap。随后 135216200 的 RPC 返回 `-32000`，解析器撤销已发布代并重新建立基线，整段验收拒绝跨代拼接。

[原生事实解析](chain-parser-go/internal/chain/evm/facts.go) · [历史证据核验](chain-parser-go/internal/chain/evm/chain_receipts_history.go) · [公网非空窗口验收](integration/README.md)

<a id="fixed-evm-block-read-retry"></a>

## 固定 EVM 区块读取的有限重试（2026-11-26，阶段258，P0-1）

显式配置 `rpc_retry` 后，按区块号的 `eth_getBlockByNumber` 和按区块哈希的 `eth_getBlockReceipts` 遇到 `-32000` 可再次调用。每次都经过原有尝试次数、期限、归档和原生读取预算。公网 BSC 窗口工具启用最多三次尝试。本地回归验证正式原生解析入口从一次故障恢复，并在持续故障时于第三次结束。另验证移动块标签及其他不可用错误的处理、所有重试响应的计费和归档。

Go RPC 包 race 检查、RPC/EVM/service 定向回归及 Python 3.13 的窗口和离线回放测试通过。公网非空 Swap 窗口另行验收。

<a id="bsc-contiguous-swap-journal-replay"></a>

## BNB Smart Chain 连续 Swap journal 复验（2026-11-26，阶段259，P0-1）

本轮用 BNB Smart Chain 固定 Pancake V2 池完成 18 个同代连续提交，首块为 135843804，末块为 135843822。两个 owner RPC 端点提供连续父块、原生提交和回执，NodeReal 作为固定高度池状态见证端点完成首末状态核对。PublicNode 的区间日志逐条匹配交易、日志索引、区块哈希和四个金额，共 10 条 Swap。历史非空块为 135843805，包含 6 条 Swap。离线空 Pebble 重放完成，归档 195 条记录、55,858,336 字节。

本轮工具新增独立 BSC 状态见证端点，避免把单一 RPC 的 `missing trie node` 当作池状态失败。封存时间解析接受 Go RFC3339 的 1–9 位小数并按纳秒排序，拒绝非 UTC 或超过 9 位小数。复验结果摘要为窗口 SHA-256 `da48f3f229521e1ce11e9cedfd937ff299d56aa5210acda1af3126a6ca50a57f`、Swap SHA-256 `a1297ce6f7252ae9426ab53611296ba745285453c7ef8abcbc40fcc69de52e24`、历史证据 SHA-256 `7b5ec0b90f09a657c1954ec33c3b9726b62b7684921aeda8d9c70017120f81d6`。

<a id="v2-exact-out-explicit-clock"></a>

## V2 ExactOut 显式执行时钟与后状态（2026-11-26，阶段260，P0-2）

将标准 V2 ExactOut 的 Router02 整数输入计算、Pair 完整结算和累计价格更新收敛到同一个 `v2_complete` 内核。新增显式执行时间入口，后续区块执行会更新 `block_timestamp_last`、两个累计价格和原始余额，保留原 `quote_exact_out` 入口使用采集银行时间的兼容行为。删除原 ExactOut 分支中的重复结算逻辑，避免两个内核出现不同的边界检查。

新增回归覆盖后续 7 秒执行、输入输出守恒、uint32 时间和 uint256 累计价格边界。`cargo check -p xone-protocol-math` 与新增定向测试通过。完整测试仍受工作区已有历史 fixture 删除影响，未恢复这些用户已有改动。

[V2 完整数学](dex-router/crates/protocol-math/src/v2_complete.rs) · [ExactOut 入口](dex-router/crates/protocol-math/src/exact_out.rs) · [回归](dex-router/crates/protocol-math/tests/v2_complete.rs)

<a id="curve-plain-multi-exact-out"></a>

## Curve Plain3/Plain4 多资产 ExactOut（2026-11-26，阶段261，P0-2）

为已登记的 `curve-plain3-basic` 与 `curve-plain4-basic` profile 补充 ExactOut。实现使用原 PlainPool 的整数 ExactIn 内核做单调有界逆解，再以最终整数输入重新计算完整余额和 admin fee 后状态。无法由原始舍入规则精确达到的目标明确返回 `CURVE_EXACT_OUT_UNREACHABLE`，不会把近似输出当成 ExactOut 成功。

多资产路由与协议能力目录已接入这两个 profile，返回结果携带实际输入金额和完整物理余额向量。`cargo check -p xone-protocol-math` 和格式检查通过。测试目标仍受仓库已有删除的 `curve-multi-quotes.json` fixture 影响，临时 smoke example 因 dex-router 默认 Kafka 原生构建缺少 macOS C++ 标准库而未保留。

[Curve 多资产数学](dex-router/crates/protocol-math/src/curve_multi.rs) · [多资产路由](dex-router/crates/protocol-math/src/multi_asset.rs) · [ExactOut 分发](dex-router/crates/protocol-math/src/exact_out.rs)

<a id="curve-meta-underlying-base-liquidity"></a>

## Meta underlying 的 Base LP 存取款内核（2026-11-27，阶段262，P0-2）

为 MetaUSD `exchange_underlying` 的后续组合接入补齐固定 3pool 依赖的单币 `add_liquidity` 与 `remove_liquidity_one_coin` 整数内核。实现严格复用 Plain3Basic 的 D、`get_y_D`、不平衡费、admin fee、LP 供应量和内部余额舍入规则。结果返回完整 base pool 后余额、LP 总供应量、逐币 fee 和实际 admin fee，初始单币存款、无 LP 铸造、非法 burn 和余额不足均拒绝。

本轮新增自包含 `curve_meta_underlying_standalone` 回归，覆盖单币存款、单币退出、供应量守恒、admin fee 记账及边界拒绝。`cargo test -p xone-protocol-math --test curve_meta_underlying_standalone` 三项通过。该内核尚未接入在线 Meta 状态，因为当前 Meta 快照只保存 base virtual price，不含可证明的 base pool 三币余额、A、fee 和 LP totalSupply。在线 `exchange_underlying`、base pool 真实读取、组合 ExactOut 与独立执行仍保持拒绝，不能将本轮记为 Meta underlying 完成。

<a id="curve-meta-underlying-composition"></a>

## Meta underlying 三路径组合报价（2026-11-27，阶段263，P0-2）

在显式 base pool 快照上接入 `exchange_underlying` 的三条 ExactIn 组合路径。base→meta 先按 Plain3Basic 单币存款铸造 LP，再执行 Meta LP→meta 兑换。meta→base 先执行 Meta meta→LP 兑换，再按 base pool 单币退出。base→base 直接复用 base pool 的原始交换内核。每条结果同时返回 Meta 内部余额、Meta admin 余额、base 余额、LP 供应量、fee 和 admin fee，避免把 virtual price 当作可替代的 base pool 状态。

新增回归覆盖三条路径和 Meta 状态不变边界，`curve_meta_underlying_standalone` 六项通过。组合 API 仍要求调用方提供同一快照的 base 三币余额、A、fee 和 totalSupply，当前在线 Meta protobuf 未扩展这些字段，因此正常服务与 ExactOut 入口继续拒绝 underlying，真实 base 读取和独立执行仍未完成。

## Meta underlying 组合 ExactOut（2026-11-27，阶段264，P0-2）

为三条 underlying 组合路径增加有界单调 ExactOut 逆解。每个候选输入都完整重放 base LP 铸造或退出、Meta 交换、admin fee 和两侧后状态，最终重新计算并拒绝整数舍入无法精确达到的目标。新增回归覆盖由 ExactIn 生成的组合目标、最小输入守恒及零目标拒绝，`curve_meta_underlying_standalone` 七项通过。

组合 ExactOut 仍属于显式快照数学能力。在线 Meta 快照未包含 base pool 的三币状态，服务入口和生产执行资格保持关闭。

## Meta base pool 快照字段与跨语言 wire（2026-11-27，阶段265，P0-2）

为解除 Meta underlying 对 virtual price-only 快照的限制，新增可选 `CurveMetaBasePoolState`。字段绑定同一块的 base 三币内部余额、admin 余额、固定 rate、`A_precise`、fee、admin fee 和 LP totalSupply。Go EVM 解析器在 `enable_underlying` 配置开启时读取这些字段，旧配置默认省略并保留 direct-only 行为。Go、Router Rust、Parser Rust 的 domain 和 protobuf 转换均保留该可选状态，非法长度、范围、空供应量和零 rate 拒绝。

新增 Go 自包含 wire roundtrip 回归。Go domain/config/evm 包编译通过，Router 与 Parser Rust domain/config 编译通过。完整历史测试仍受仓库既有已删除 fixture 影响，common/protoc 生成检查在当前机器缺少固定 Rust `1.90.0` toolchain 时无法完成，已用本机 Rust 1.94 生成物和 Go 编译检查验证本轮字段。

## Meta underlying base admin 会计传播（2026-11-27，阶段266，P0-2）

补齐 base 单币存取款和 base→base underlying 交换的 admin 余额后状态。`BaseLiquidityState` 现在显式携带三币 admin 余额，存款将逐币 admin fee 累加，退出和交换按源规则更新对应币的 admin 余额，组合结果同时返回 base 内部余额与 admin 余额。原有七项 Meta underlying 回归继续通过，覆盖三路径 ExactIn、组合 ExactOut 和 base admin 状态传播。

本轮只推进显式快照数学，尚未把 `Pool` 路由入口接到 underlying token 索引，真实在线执行仍需 base 余额与 token 转账证据。

## Meta underlying Pool 快照入口（2026-11-27，阶段267，P0-2）

新增 `multi_asset::quote_meta_underlying_exact_in` 与 `quote_meta_underlying_exact_out`，从已验证的 Meta `Pool` 快照提取 Meta 内部/admin 余额和可选 base pool 状态，统一调用三路径组合内核。缺少 `base_pool_state` 或使用非 Meta profile 时返回明确错误，避免将 Meta virtual price 误当作完整 base 状态。

协议数学编译和七项 Meta underlying 自包含回归通过。上层 token 路由和正常 Quote API 已接入并通过固定 protobuf 快照回归，交易准备仍缺在线 token 转账、base LP allowance、真实 receipt 与生产 VERIFIED。

## R03 隔离中间件与恢复迭代（2026-11-27，阶段268，P0-1）

R03 验收基础设施在本机缺少 PostgreSQL 13 或 Kafka 安装时自动切换到隔离 Docker 实例。Kafka 容器使用内部 `localhost:9092` 与宿主机映射端口的双监听器，避免容器内 topic 管理命令获得宿主机地址后无法建立元数据连接。Rust `normal_yellowstone` 构建固定使用仓库锁定 protobuf 工具链和 macOS SDK 头文件。

`python3 integration/tools/acceptance/r03_authority_process.py` 通过，覆盖受控 tonic 流、WAL、manifest、owner 发布、撤销、重启、Fence 和 HEAD_FIRST 恢复。R03 history runtime 7 项测试通过。该批次的受控来源、真实本机 PostgreSQL/Kafka 和公开历史 bank 完整性仍分别记录，真实同源 Yellowstone、连续断线和生产故障域继续按 R01–R04 验收。

## Curve Meta underlying 正常 API 与恢复联验（2026-11-27，阶段269，P0-2）

使用四个固定 protobuf 快照和三笔连续 underlying 交换，routing 的双向共享 base 状态测试以及 `xone-quote-api` 的正常 HTTP ExactIn/ExactOut 入口、Meta/base 后状态差分、快照推进和冷启动恢复均通过。验证命令为 `cargo +1.90.0 test --locked -p xone-routing --test curve_meta_underlying` 与 `cargo +1.90.0 test --locked -p xone-quote-api --test curve_meta_underlying_wire`，各 1 项通过。该范围仍依赖显式 base pool 状态和固定 bank，真实链收款、allowance、独立执行与生产 VERIFIED 继续关闭。
