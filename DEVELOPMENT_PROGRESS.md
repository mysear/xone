# Xone 开发进度与完成情况

更新：2026-11-04。设计基线：[XONE_SYSTEM_DESIGN.md](XONE_SYSTEM_DESIGN.md) v2.3。

本文件只记录项目进度、已完成范围、剩余功能及验证结果。后续进度只更新本文件；测试原始数据、JSON报告和校验规则由集成工具维护，不再嵌入进度文档。

状态：`PARTIAL_LOCAL`为部分实现并完成本地验证，`LOCAL_SCOPE_VERIFIED`为注明范围通过本地验收，`NOT_STARTED`为未开始；只有完整设计验收通过才可记为`ACCEPTED`。目前原26工作包完整验收仍为0，生产准入与VERIFIED报价关闭。

优先级：区块解析和精确兑换量计算优先，分析功能低优先级。三服务各自独立，公共协议位于common。

导航：[本轮开发](#active-work) · [工作包](#work-packages) · [协议覆盖](#protocol-coverage) · [逐项功能](#design-traceability) · [已验证批次](#batch-history) · [剩余功能及优先级](#remaining-priorities)

<a id="active-work"></a>

## 本轮开发

|工作项|设计条款 / 工作包|状态|已完成|剩余 / 验证|
|---|---|---|---|---|
|EVM在线头时间准入与撤销|§7.3/9/23；WP05、WP09、WP25|LOCAL_SCOPE_VERIFIED|在线head与提交前统一新鲜度检查，未来/过期头拒绝、旧引用保留及Tick撤销定向/全量通过|真实进程未来head故障撤销/恢复通过；独立可信头、完整provider矩阵及共识仍未完成|
|旧CLMM验收优化模式拒绝校验|§23/28；WP25|LOCAL_SCOPE_VERIFIED|旧dynamic/seed验收78处显式拒绝保留条件/消息/异常；24项普通/优化定向回归和子进程防回归通过|212项辅助回归普通/优化均通过，独立审查关闭；仅修复验收可靠性，不重新宣称旧协议完整验收|
|Managed V2实际发布版与非零AUM|§7.3/11.3/12.2/12.9/12.11/12.14；WP02–05、WP11–13、WP25|IN_PROGRESS|aum-3完整原getter/来源与派生上下文/实际钱包/独立Pool-Vault暂停/Recovery已完成当前单池限定同源验收；15来源状态、18进程检查、8双副本正向、真实原计划余额重放及双审通过|2/3/5/50当前目录正式固化与普通服务、跨物理池共享Vault overlay尚未接入；原型已独立验证，继续T34.8/T34.9，不能据单池范围结束本批或原26工作包|
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
|Managed非零BPT费用参考与状态契约|原§7.3、§11.3、§12.2/12.11/12.14 / WP02–05、WP11、WP13、WP25|PARTIAL_LOCAL|原回调费用/铸币完整后状态、同收款方合并、独立wire和固定区块依赖组件已实现|旧源码runtime超EIP170，正常配置/parser及公共报价保持拒绝；实际可部署版本服务闭环未完成，见[本批计划](#plan-2026-11-01-managed-bpt-fees)|
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

<a id="work-packages"></a>

## 工作包与设计方案对照

原设计完整要求直接查看对应§28条款，不在这里重复复制设计原文。功能细项在下方逐项映射。

|工作包|功能|方案|状态|已完成范围|待完成范围|
|---|---|---|---|---|---|
|WP01|仓库、构建与质量基线|§28.3|PARTIAL_LOCAL|三项目独立构建、固定Go/Rust工具链、锁文件、Make验证入口、公共契约副本检查；独立性补查：服务测试不读取兄弟项目源码/配置，Go测试本地完整路由设置，集成层逐字节校验共享副本并禁止跨项目测试路径|镜像/SBOM/许可证及secret扫描、真正的CI平台执行尚未验收|
|WP02|链身份、金额和快照契约|§28.4|PARTIAL_LOCAL|Kafka实际record和owner bootstrap已接入Protobuf；uint256恰好32字节大端、块hash原始32字节；RaydiumCLMM dynamic_fee九字段及独立protofield63、signedindex/宽度/PDA/clone、旧profile隔离与独立protoc已验证；本批限定范围整体验收通过；本批新增独立CurveCryptoState field12与原LP账本/定价/来源的跨语言严格契约；本批新增Orca adaptive独立profile及field51完整Oracle/保留字段契约与严格跨语言校验；本批新增独立InfinityBin field30完整tree/share/manager账本与field29树外active锁定dust、跨语言wire/config/clone和来源约束；新增独立Orca adaptive-rewards复用完整Oracle/reward/tick wire，4007byte独立protoc与旧profile隔离通过；新增adaptive-dynamic独立版本、完整mixed layout/bitmap/reward/Oracle与4013byte独立wire，旧fixed-only隔离通过|网络指纹/所有链地址规范化、schema演进破坏检测和更多事实族仍需扩展|
|WP03|历史fixture与独立回放框架|§28.5|PARTIAL_LOCAL|固定合成基线、V2反例、大整数、重复重放、Python独立顺序计算对照；416固定原Pool.swap/CLPool.swap EVM样本、8组原库连续序列与1115主源码hash；动态费原DynamicFeeInfo/SwapState/swap_internal独立native：348案例12连续序列24步、8时间不变/6预算准入探针、2原始bank完整Pool；旧480/968/364/464语料保护，本批限定范围整体验收通过；正常Solana RPC原始响应证据WAL、独立耐久head/损坏封闭与重启新会话验证通过；标准receipt type0–4/legacy post-state的12组EthereumJS独立根向量（含128/130/257索引边界）验证通过；新增固定原Vyper NG完整交换/EMA、独立protobuf及正常服务回归；Managed非零BPT费用原回调参考357案例/30序列120步及常规EVM大小限制失败证据，明确无服务准入；MetaUSD实际clone/initialize/add_liquidity及32直接交换、8原探针、完整Token/raw/admin与正常服务回归；本批新增Crypto原池及原V4 LP的60案例、Orca费率197案例与14连续交易、Infinity原BinPool跨Bin326案例；本批新增固定原program pack+swap/update后的20bank、16成交/4拒绝，26类普通/优化模式故障拒绝；ProgramData为明确合成身份；本批原manager/Vault正常部署getter与5bank/4连续跨Bin交换、原burn16场景48交易（9回退）及原manager降低初始化上限证据通过；新增固定原Orca初始化奖励adaptive参考20bank/16成交4拒绝、12乘法溢出/4global回绕/35outside更新、34普通优化篡改拒绝；新增原dynamic枚举/实际加载器20bank及44篡改反例、100成功2失败×7动态布局内核结果对照|真实历史块来源/许可/原始证据和事件顺序篡改fixture待补|
|WP04|RPC Provider与链能力|§28.6|PARTIAL_LOCAL|EVM chainId+genesis握手、错链隔离、总超时、逐provider有界尝试、响应体上限；Solana genesis校验；RaydiumCLMM动态profile复用同槽15..19账户finalized整批、ProgramData/Token/Clock及读后block验证，实际2bank样本通过；本批限定范围整体验收通过；同一提供方finalized context有界分片、逐片Clock/首末锚点、总响应预算和整批切源已局部验证；本批新增Crypto原模板/immutable及原LP minter、完整定价/raw/LP状态的同blockHash读取；本批新增Orca adaptive独立fee-tier PDA、254-byte Oracle及Pool/ticks同finalized bank读取和重锚；本批固定hash读取Bin原模板/immutableVault/key/pause/token/app依赖与完整root/L1/L2/所有bin/share，预算和getNext双向校验通过；新增adaptive-rewards同finalized bank/Clock完整账户RPC与重锚失败拒绝，旧版本隔离通过；新增完整dynamic/mixed账户同finalized bank读取和重锚、148/10004精确compact边界与畸形拒绝|WS订阅、完整能力矩阵、分提供方速率/历史范围探测及双源head对照待实现|
|WP05|区块读取、解码与完整性组块|§28.7|PARTIAL_LOCAL|EVM固定blockHash读取、token身份、receipt/log完整性、同块读后锚点复核；单链串行提交；RaydiumCLMM DynamicFeeInfo1096..1130九字段实际解析、1130..1544未知padding拒绝、正常RPC动态分派与原完整Pool样本通过；本批限定范围整体验收通过；本轮receipt结构与日志连续性校验完成，Go全check/race及新旧27范围回归通过；多RPC完整receipt/尾部遗漏差异核验与恢复闭环通过；正常Solana RPC原始响应证据WAL、独立耐久head/损坏封闭与重启新会话验证通过；主源/全部witness的consensus receipt根重算、严格可选字段与bloom校验、前后root锚点及共同删尾拒绝完成局部验证；超过100依赖可用最多32片构造同context当前基线，归档/混合协议同样执行屏障；13池CLMM seed状态完整保留，16池134账户进程验证通过；本批新增Crypto完整状态解析及专属版本拒绝边界，旧settled仍拒绝active ramp，独立ramp版本已完成限定在线验收；本批新增initialized Oracle专属profile、Clock/源bank/timestamp绑定和完整账户解析，旧static/rewards拒绝边界不变；本批新增Bin完整树与树外active1000share/dust解析、原getters/storage核对及source重锚，10种进程来源故障撤销/新generation恢复通过；新增classic SPL三fixed数组/初始化Oracle与奖励组合正常解析、13来源故障撤销恢复及完整后状态限定验证通过；新增adaptive-dynamic真实compact账户解析与16来源故障撤销恢复限定验证|并行staging、自动工厂发现/长期raw归档/持久DLQ；新增版本Swap/Liquidity/Transfer分析事实、更多DEX具体版本仍未实现；独立可信区块头、transactionsRoot、原生执行与真实来源独立审核仍待补|
|WP06|Canonical Journal、事务与幂等|§28.8|PARTIAL_LOCAL|Go/Rust公共BEGIN/CHUNK/COMMIT契约、完整屏障/校验/Protobuf二进制/32字节大端金额；真实Kafka事务发布、read_committed消费、稳定transactional.id、分区0/单分区检查、prepared与state/cursor原子应用、未知结果从journal确认；prepared对照确定性wire hash；只读analytics Reader不创建producer或fencing|生产TLS/SASL/ACL、DLQ原始证据、topic重建/迁移全链校验、真实ISR/保留损坏/消费者重平衡专项仍未验收|
|WP07|Reducer、分叉与旧主隔离|§28.9|PARTIAL_LOCAL|真实PG互斥租约/server clock/过期不可续租/owner投影/单调版本下界与关键激活审计；两解析器接入Kafka fencing和prepared恢复；限定V2旧分支精确BlockRef倒序撤销与新分支事实在REORG_COMMIT整体提交并供隔离analytics更正；Rust来源错误广播ONLINE_SCOPE_REVOKED，精确同scope/块/epoch/代seq+1，prepare/Kafka/PG/apply持久化与合格高代基线恢复；receipt根/共识字段异常的旧scope撤销、HEAD_FIRST新代恢复、无伪造BLOCK_COMMIT与后续正常提交经13项进程验证|真实链交叉来源/多块历史fixture、通用事实/BLOCK_REVERT独立记录、Solana完整fork、PG故障切换/区域及独立故障域、对象存储fencing和完整控制审计仍未完成/验收|
|WP08|当前基线、归档与HEAD_FIRST空盘恢复|§28.10|PARTIAL_LOCAL|Go校验checkpoint；Go/Rust owner提供Protobuf bootstrap，router可无KV从当前checkpoint与有界Kafka尾窗恢复；Rust旧KV不自动ready|对象存储不可变归档、公共manifest/checksum下载、peer选择、实时流/WAL缓冲、上传一半/保留过期及长停机演练待完成；现有owner bootstrap不能代替对象存储恢复|
|WP09|Rust版本化状态副本|§28.11|PARTIAL_LOCAL|不可变QuoteView/池图原子发布、数值更新复用拓扑；两独立路由副本读取完整分区、禁自动提交offset、owner当前完整checkpoint+cursor恢复、有限Kafka尾窗；HTTP开发模式所有源失败暂停及同generation恢复；字段24独立state进入双完整路由，错误Solana依赖撤销该链旧视图，EVM链保持可用；新增adaptive-rewards双副本当前bank分发、故障撤销/新generation恢复和router重启通过；新增adaptive-dynamic双副本完整状态、撤销恢复与router重启验证|大池状态COW/慢句柄回收/内存压力、真实保留过期和大型图恢复、生产多实例容量仍需实测|
|WP10|精确V2与基础Token行为|§28.12|PARTIAL_LOCAL|单一V2精确整数内核、先乘后除、方向/费率/大整数、consumed/refund；未知token行为失败；Raydium ceil输入组合费/creator拆分及输出creator费、protocol/fund floor、LP保留费与raw/净储备后状态，u64/u128checked；1604原代码差分；新增独立完整V2版本的raw/储备/累计价/锁及原VM差分完成限定验收|真实链参考合约/独立执行余额差分、税费/rebase等特殊Token未实现或验收|
|WP11|V3、Stable及V4能力|§28.13|PARTIAL_LOCAL|标准Uniswap V3有界证明范围内的独立整数exact-in、逐word手续费舍入、跨tick净流动性、零流动性空段、int256/uint160/uint128边界及乘法溢出fallback；264项固定官方SDK差分（含最低初始化价）与双副本解析报价场景通过；RaydiumCLMM动态profile原spacing分段/时间filterdecay/ceilfee/cap/zeroL/index/逐步费用与完整后Pool，348原native案例及连续状态通过；本批限定范围整体验收通过；Balancer V2双币不可变50/50独立Weighted模型，固定区块及765原onSwap案例/45组180步完整余额与17项双副本进程检查通过；新增不可变双币任意合法归一化权重static版本，原整数LogExpMath/powUp、5355案例/315组1260步/240pow与18项双副本检查通过；五族联合增量已补多币物理状态、限定Managed/Composable、Plain3/4正常读取路由及Meta/NG/Crypto原数学子集、Solana静态奖励和V4/Infinity高级CL/单Bin内核，按各自边界完成本地测试；标准type0 NG2–8币已补齐固定源码模板/immutable读取、完整raw/admin/oracle后状态与原exchange差分及双副本连续报价；非零Managed协议/管理费BPT铸币、完整供应/收款余额与powDown数学已原回调验证，旧源码超限仅参考使用；MetaUSD标准meta/3CRV直接兑换已补齐独立clone版本、base VP只读依赖、完整状态/wire、正常RPC解析/双副本报价及共享池重算；本批新增Crypto完整调价/admin mint/gulp与完整后状态、Orca自适应费率管理器和Infinity跨Bin内核；Infinity静态零hook完整树在线接入已由后续Bin批次完成限定验收；本批Orca完整adaptive内核已接普通报价，完整Oracle/fee/ticks/raw后状态和全额/正输出约束、双副本及来源故障恢复已通过本地验收；本批静态零hook标准ERC20 Bin普通分派、真实tree遍历与树外active、完整储备/fee/app账本及全额正输出通过，完整Vault结算待补；新增Orca adaptive初始化奖励checked乘法溢出归零、wrapping global/outside及普通报价完整后状态与原native差分通过；新增三连续完整dynamic/fixed混合数组的adaptive奖励内核/普通分派及完整原后状态差分|其他Curve模型与特殊Token、Managed非零BPT费用/Composable BPT及rate-cache/V3、Slipstream外部module/reward、V4/Infinity dynamic/hooks/native/Bin与管理器完整执行、实际Token/部署审计仍未完成；五族全部功能仍未完成，详见联合计划明确剩余项；V2未同步raw余额的解析/完整数学后状态及共享路径依赖仍需补齐，不能由独立EVM单hop通过反推数学覆盖完整|
|WP12|金额感知候选图|§28.14|PARTIAL_LOCAL|方向性多重图、物理池ID、金额计算排序、跳数/展开/候选/期限预算、范围内启发式语义；V3与V2混合图、按配置拆单粒度保留仅部分分配可完整消费的路径；RaydiumCLMM动态profile进入已有金额感知图，报价使用动态完整状态；专项回归通过，本批限定范围整体验收通过|大图增量索引、beam多标签、上界证明、全量小图oracle与跨跳shift反例仍需补充|
|WP13|拆单、共享池与整数守恒|§28.15|PARTIAL_LOCAL|单路径/最多两路有界枚举、两种执行顺序、共享池状态模拟、余数逐跳重算、输入守恒、启用成本提案；共享V3物理池顺序更新sqrt/tick/liquidity，最终path/route输入输出都重算；RaydiumCLMM动态共享池连续消费完整volatility/reference/fee后状态，原序列及分配输入守恒通过；本批限定范围整体验收通过；本批新增Crypto共享池拆单连续重算价格/raw/LP账本验证；本批新增四组Orca adaptive两步共享allocation与原序列后状态匹配，后一步复用Oracle/fee/ticks/raw；本批共享manager/currency依赖冲突拒绝及每步app/protocol fees传播，4步跨物理池原序列和实际拆分计划21步原EVM重放通过；新增四组adaptive-rewards共享allocation复用完整奖励/Oracle/tick，实际选中计划36步原native连续重放通过；新增dynamic四组共享allocation和39个实际选中计划原生步骤验证|通用DAG合并/联合求解、税费、实际Gas/L1费换算与系统oracle gap测量尚未完成|
|WP14|执行计划与calldata|§28.16|PARTIAL_LOCAL|caller/recipient/滑点/截止/快照/策略进入plan hash、单路径V2 ABI构造、目标白名单拒绝；Edge绑定protocol，V3/其他协议路径不能误编码为V2 router calldata；新增singleton profile只生成指示性计划，明确拒绝套V2 calldata，专属执行器/settlement未实现；T32新增标准V2单hop完整BOUND信封/执行器身份/精确calldata与本地独立执行绑定，限定本地验收通过|受审计执行器/拆单calldata、allowance/wrap/fee/DAG/ExactOut退款待实现；API不开放未验证交易|
|WP15|独立交易仿真|§28.17|PARTIAL_LOCAL|验证接口明确拒绝无后端的VERIFIED；固定blockHash eth_call仅标RPC_CALL_CHECKED；新增Shanghai单hop V2的严格REVM本地状态DB、独立真实余额差、执行预算与可终止子进程接口，原合约全状态差分、双副本故障恢复、全量及双审通过限定本地验收|可信链状态DB、多链hardfork/L2费用、其他协议/拆单/SBF完整仿真仍缺，VERIFIED保持关闭|
|WP16|报价API、认证、配额及质量语义|§28.18|PARTIAL_LOCAL|FAST指示性ExactIn、字符串金额、鉴权、单凭据配额、并发/期限、single-flight、证据、缓存版本、旧quote build拒绝；新增认证simulate API、缓存前后新鲜度复核、实际余额/费用/worker证据；限定本地验收通过|多租户独立额度、任意仿真隔离、成本完整性、VERIFIED可执行闭环与完整业务成功率SLO待完成|
|WP17|多副本、高可用、监控与发布|§28.19|PARTIAL_LOCAL|进程health/startup/按链状态、SIGTERM、日志/报价低基数指标、三个独立Dockerfile及本地compose|多区主备、PG/Kafka故障/磁盘满/节点或区丢失演练、OTel、灰度/发布控制与生产readiness待完成|
|WP18|第二与第三条EVM链|§28.20|NOT_STARTED|chain_key隔离、两个解析源独立范围；配置可描述多条EVM链；多DEX版本以chainKey+code/factory+adapter配置域区分，协议覆盖矩阵单列每版本解析/数学/执行|第二/第三条真实EVM链部署、硬分叉/Gas差异、真实协议/版本目录与升级审计未验收；合成同链不算真实多链上线|
|WP19|分析事件与worker基础|§28.21|PARTIAL_LOCAL|现有Go解析二进制analytics角色已接只读Kafka有限Job；持久cursor、topic身份、事实去重、毒消息暂停及失败状态checkpoint已实现；不占用在线KV/PG|持续worker调度、持久DLQ/raw回放、ClickHouse重试/发布、跨插件隔离配额与故障验收待完成|
|WP20|Solana数据与执行适配|§28.22|PARTIAL_LOCAL|Rust RocksDB、账户依赖/完成屏障、来源session/write_version隔离、skipped范围验证；RPC整批限定布局CPMM数据解码；固定6d7a0f82动态费独立profile，classicSPL/backedWSOL、fee0/1/2、seed0..65535及mandatoryextension，原解析/分段exact-in和41真实PG-Kafka双router检查通过；本批限定范围整体验收通过；正常Solana RPC原始响应证据WAL、独立耐久head/损坏封闭与重启新会话验证通过；当前RPC分片bootstrap、失败整批丢弃/切源重新采集、全scope/报价快照引用及WAL新会话恢复局部验证完成；经典SPL静态费Orca/Raydium奖励增长及完整后状态已由联合批次局部验证；本批新增固定Orca原FeeRateManager的逐group/时间/波动原native差分，限定adaptive在线解析由本批独立版本完成；其他组合未完成；本批新增经典SPL三fixed数组/未初始化奖励的Orca adaptive账户解析及正常分派；其余组合和完整SBF执行待补；新增独立adaptive-rewards版classic SPL/三fixed数组/完整Oracle/初始化奖励增长与跨tick状态，20原bank及双router/13故障恢复限定验收通过；新增classic SPL/完整Oracle/三完整dynamic或fixed混合数组与初始化奖励的解析/报价/双副本限定验收|RaydiumCLMM limit-order/Token2022、奖励领取与其他程序版本，Orca adaptive的Token2022及奖励领取/其他数组范围、Meteora/Pump等协议，Yellowstone流及流WAL连续性/补洞/fork、CU/账户锁/交易/完整Anchor-SBF-Token执行及真实部署仍未完成；当前RPC已局部实现同context最多32片；EnsurePoolScope、连续流/版本化账户、范围扩容与跨片独立来源仍缺；固定静态/token-fee/bitmap/custom-seed/dynamic五个profile仅局部本地验证|
|WP21|K线、重组更正与历史分析发布|§28.23|PARTIAL_LOCAL|精确有理数OHLC、事件时间排序、撤销最高/最低成交后独立重算、revision、离线原子发布；现有Go二进制analytics journal有限Job只读消费公共事实，无在线KV/PG；HEAD_PROVISIONAL事件时间水位/原子单位和历史缺口；有限预算失败状态持久化|ClickHouse事实/版本表、shadow/发布指针、FINAL及merge前查询、正式分析API/查询配额、迟到/decoder升级回补、真实链与生产故障条款待实现/验收|
|WP22|高级优化与外部来源竞争|§28.24|NOT_STARTED|保留HEURISTIC_WITHIN_SCOPE，不声称全局最优|RFQ/聚合器、多个求解器、有效期/调用者约束、可证明上界及收益分布尚未开发|
|WP23|迁移、灾备与生产放量|§28.25|PARTIAL_LOCAL|本地重启/无KV路由恢复演练入口与报告；明确发布阻断项|真实生产shadow迁移、区域灾备、RPO/RTO、adapter灰度回退及逐租户放量未实施|
|WP24|HEAD_FIRST启动与历史解耦（前置工作）|§28.26|PARTIAL_LOCAL|HEAD_FIRST不以旧KV/cursor/history gap置ready；owner分配代际、PG/Kafka下界恢复、当前完整checkpoint加有界尾窗；未确认分叉持久暂停、可信源确认后HEAD_FIRST恢复；临时源故障恢复不误增canonical epoch；正常Solana RPC原始响应证据WAL、独立耐久head/损坏封闭与重启新会话验证通过；16池134账户当前bootstrap与空内存路由副本/解析器重启恢复通过；不依赖历史补洞，不假定普通RPC提供严格版本化bank|1小时/1天/30天真实停机、实时流缓冲/流WAL溢出、EnsurePoolScope、独立来源能力/资源配额及首个VERIFIED报价时间仍未完成|
|WP25|配置文件体系与配置中心扩展口（P0前置）|§28.27|PARTIAL_LOCAL|三服务严格单文件配置校验、配置/策略hash与secret权限；新增Kafka/PG配置与有界时长校验；raydium-clmm-dynamic profile与mandatoryextension账户范围准入、原动态参数只从链状态读取，旧profile动态污染拒绝；本批限定范围整体验收通过；本批新增MultiAssetState field12 CurveCryptoState及独立protoc/克隆/来源bank验证；本批新增OrcaWhirlpoolState.adaptive field51，保留reserved32–49，独立protoc与16/16/128保留数据往返验证；本批新增InfinityBin完整状态/可选树外active，原PoolState保留字段不变，独立protoc及旧模型兼容回归通过；新增adaptive-rewards独立版本准入、完整reward/Oracle wire及旧profile拒绝边界和两阶段审查通过；新增adaptive-dynamic独立版本准入与旧fixed-only隔离，跨语言4013byte wire与两阶段审查通过|生产TLS/secret修订id、完整公共配置schema/兼容矩阵/坏输入语料及配置中心bundle物化仍待完善|
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
|uniswap-v4|uniswap-v4/core-1 (source bea7f6d1)|EVM|4,12.2,12.11|WP02,WP04,WP05,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|零hook/静态LPfee<1e6/标准非零ERC20；独立manager/rawPoolId、extsload、方向费与完整tick证明已实现；动态fees/hooks/native/自定义会计、manager unlock/settlement及真实执行未完成；真实部署目录/历史块/特殊Token/生产验收未完成|
|curve|每个stable/crypto具体合约版本待固定|EVM|4,12.2,12.11|WP11|NOT_IMPLEMENTED|PARTIAL_LOCAL|NOT_IMPLEMENTED|Plain2/3/4各独立profile已局部实现；标准Meta直接交换及NG已由独立profile接入服务，Crypto完整交换数学及标准settled独立版本的状态/解析/报价已补，经典SPL/fixed-array限定服务验收通过；其他Plain/Meta underlying/Lending、特殊Token、Crypto其他版本与active-ramp在线状态、完整执行仍未完成|
|balancer|V2/V3分别适配，具体pool版本待固定|EVM|4,12.2,12.11|WP11|NOT_IMPLEMENTED|PARTIAL_LOCAL|NOT_IMPLEMENTED|Stable2、Weighted双币/多币及限定Managed/Composable regular另列已实现子集；原BPT数学内核不等于完整callback；非零BPT费用、供应/缓存/rate provider/V3及执行仍未完成|
|aerodrome-v2-volatile|aerodrome-v2-volatile/pool-1|EVM|4,7.3,11.3,12.2,12.11|WP02,WP04,WP05,WP11,WP12,WP13|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|特殊Token/完整执行与部署目录审计；支持fee<=300/10000的标准Pool，factory动态值按同块读取；合成RPC与独立参考已验证，真实链历史块及收款差分仍未完成|
|aerodrome-v2-stable|aerodrome-v2-stable/pool-1|EVM|4,7.3,11.3,12.2,12.11|WP02,WP04,WP05,WP11,WP12,WP13|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|特殊Token/完整执行与部署目录审计；支持fee<=300/10000的标准Pool，factory动态值按同块读取；合成RPC与独立参考已验证，真实链历史块及收款差分仍未完成|
|curve-plain2-basic|curve-plain2-basic/factory-5|EVM|4,7.3,11.3,12.2,12.11|WP02,WP04,WP05,WP11,WP12,WP13|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|仅factory Plain2Basic Vyper0.3.1两币/固定rate_multipliers；其他plain/meta/crypto/NG、动态rate/rebase未支持；合成RPC与独立参考已验证，真实链历史块及收款差分仍未完成|
|balancer-v2-stable2|balancer-v2-stable2/stable-1|EVM|4,7.3,11.3,12.2,12.11|WP02,WP04,WP05,WP11,WP12,WP13|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|仅历史V2非composable两币StablePool，无BPT/rate-provider；Weighted双币等权另列独立profile；composable/V3及执行未支持；合成RPC与独立参考已验证，真实链历史块及收款差分仍未完成|
|aerodrome-slipstream|aerodrome-slipstream/cl-1|EVM|4,12.2,12.11|WP11,WP18|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|6word slot0/10word tick、factory及clone实现身份、两个fee module均为零的金额域；未实现非零外部费用模块、reward/fee-growth忠实重演及完整执行；金额参考目前为原V3 SDK；真实部署目录与Slipstream全合约执行差分未完成|
|pancake-infinity|其他CL/hook及Bin版本|EVM|4,12.2,12.11|WP11|NOT_IMPLEMENTED|NOT_IMPLEMENTED|NOT_IMPLEMENTED|pancake-infinity-cl/cl-1静态零hook标准域另列PARTIAL_LOCAL；其他CL参数/动态swap hook与native完整结算、Bin其他动态/hook/native组合及完整执行未完成；静态零hook完整树独立服务已在对应版本完成有限验收|
|pancake-v3-lm|非零lmPool回调|EVM|4,12.2,12.11|WP11,WP15|NOT_IMPLEMENTED|NOT_IMPLEMENTED|NOT_IMPLEMENTED|外部回调/版本审计及独立执行；协议/合约或program升级后重新固定身份并验证，未知版本拒绝|
|sushiswap-trident|独立pool模型/版本待固定|EVM|4,12.2,12.11|WP11|NOT_IMPLEMENTED|NOT_IMPLEMENTED|NOT_IMPLEMENTED|不与SushiV2互认；协议/合约或program升级后重新固定身份并验证，未知版本拒绝|
|lfj-liquidity-book|离散bin的具体版本待固定|EVM|4,12.2,12.11|WP11,WP18|NOT_IMPLEMENTED|NOT_IMPLEMENTED|NOT_IMPLEMENTED|bin/动态费用专属模型；协议/合约或program升级后重新固定身份并验证，未知版本拒绝|
|raydium-cpmm|archived cpmm-v1 decoder only; nonzero creator extension rejected|Solana|4,12.2,12.11|WP20|PARTIAL_LOCAL|NOT_IMPLEMENTED|NOT_IMPLEMENTED|仅archive旧零扩展布局有限解析，不报价；当前creator源码profile另列独立支持；此旧标签不自动升级；版本身份/Token扩展/执行不从新profile借用|
|raydium-clmm|其他order/reward/Token与程序协议版本|Solana|4,12.2,12.11|WP20|NOT_IMPLEMENTED|NOT_IMPLEMENTED|NOT_IMPLEMENTED|raydium-clmm-static/token-fee/bitmap/custom-seed/dynamic五个固定6d7a0f82经典SPL范围另列局部实现；初始化奖励/限价订单/Token2022/其他程序版本及完整SBF执行未实现|
|orca-whirlpools|其他Orca fee/oracle/token/reward版本|Solana|4,12.2,12.11|WP20|NOT_IMPLEMENTED|NOT_IMPLEMENTED|NOT_IMPLEMENTED|orca-whirlpools-static/rewards与新adaptive固定cc683ec9各自限定范围另列；adaptive初始化奖励/dynamic数组组合、Token2022/扩展及完整SBF执行未实现|
|meteora-dlmm|具体program版本待固定|Solana|4,12.2,12.11|WP20|NOT_IMPLEMENTED|NOT_IMPLEMENTED|NOT_IMPLEMENTED|bins/动态费用及账户完整性；协议/合约或program升级后重新固定身份并验证，未知版本拒绝|
|pump|发射曲线及迁移后市场分别适配|Solana|4,12.2,12.11|WP20|NOT_IMPLEMENTED|NOT_IMPLEMENTED|NOT_IMPLEMENTED|虚拟/真实余额/毕业迁移/动态费；协议/合约或program升级后重新固定身份并验证，未知版本拒绝|
|pancake-infinity-bin|pancake-infinity-bin/3c5ef1be-static-1|EVM|7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP09,WP11,WP12,WP13,WP14,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|固定原BinPoolManager/Vault，静态LP fee<=100000、零hook、标准非零ERC20；完整root/L1/L2/all bins/share、active与树外锁定dust、固定块来源和manager共享账本已接入；限定服务本地验收通过；动态hook/override/native、完整Vault unlock/settlement及真实部署未完成|
|pancake-infinity-cl|pancake-infinity-cl/cl-1 (source 3c5ef1be)|EVM|4,7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP04,WP05,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|零hook/静态LPfee<1e6/标准ERC20；parameters仅tickSpacing<<16，独立6word key/manager getters/pause；每方向protocol fee<=4000；未知参数/hook/dynamic/native/Bin拒绝；Vault约束/管理器unlock/settlement/真实执行及部署审计未完成|
|raydium-cpmm-creator|raydium-cpmm/f4b9b263-spl-1|Solana|4,7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP07,WP09,WP10,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|固定f4b9b263 creator布局/经典SPL域；十账户同finalized上下文、ProgramData及Tokenkeg代码hash、Clock/PDA/WSOL准入；actualcreator输入/输出ceil、protocol/fund计提与raw/账本/net后状态；独立native原程序参考，未执行完整SBF/Token程序；真实部署/source-binary/runtimeactivation、Token2022、流/WAL/历史补洞、交易/CU/账户锁/执行未完成|
|orca-whirlpools-static|orca-whirlpools/cc683ec9-spl-static-1|Solana|4,7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|固定cc683ec9、static tier/空systemoracle/无reward-control、经典SPL和backedWSOL；Q64与fixed/dynamic arrays独立状态/数学；一次finalized账户集/ProgramData-Tokenhash/Clock/PDA，方向最多3数组，完整消耗+feeGrowth/raw/owed后状态；adaptivefees/rewards/Token2022/实际部署source-binary/runtimeactivation/SBF/CPI/用户收款/CU/WAL/流未完成|
|raydium-clmm-static|raydium-clmm/6d7a0f82-spl-static-1|Solana|4,7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|固定6d7a0f82/inputfee/defaultDynamic/legacyseed/no-rewards/no-orders/classicSPL/WSOL，3..7数组/基础1024bit范围；同finalized程序/ProgramData/Tokenhash/Clock/PDA/实际布局；原Q64step及全Pool后状态；高级fee/订单/奖励/扩展bitmap/Token2022/真实部署/sourcebinary/runtimeactivation/流/WAL/完整交易/CU/SBF未完成|
|raydium-clmm-token-fee|raydium-clmm/6d7a0f82-spl-token-fee-1|Solana|4,7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|固定6d7a0f82静态费/classicSPL/basebitmap，显式Token0Only/Token1Only模式，与旧mode0profile隔离；968原native案例/24连续序列192步、21进程检查、两阶段独立审查与冻结完整回归通过；不等于SBF或部署验证；dynamicfee/customseed/reward/order/bitmapextension/Token2022/实际部署/流/WAL/交易/SBF/CPI未完成|
|raydium-clmm-bitmap|raydium-clmm/6d7a0f82-spl-bitmap-1|Solana|4,7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|新profilemandatory1832extension、14正/负512bit分组与跨base范围，静态mode0/1/2/classicSPL；364原native案例/12序列24步、86源索引/4非法、28进程、两阶段审查与冻结完整回归通过；非完整SBF或部署证明；dynamicfee/customseed/rewards/orders/Token2022/实际部署/流/WAL/交易SBF未完成|
|raydium-clmm-custom-seed|raydium-clmm/6d7a0f82-spl-custom-seed-1|Solana|4,7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|固定create_customizable_pool_v2/seed1..65535u16LE、mandatorybitmap、staticfee0/1/2/classicSPL；旧profileseed0隔离；464原native案例/48序列96步、18身份错误探针/9身份向量、32进程、双seed状态隔离、两阶段审查和冻结完整回归通过；非完整SBF或部署证明；dynamicfee/rewards/orders/Token2022/真实部署/流/WAL/交易SBF与生产未完成|
|raydium-clmm-dynamic|raydium-clmm/6d7a0f82-spl-dynamic-1|Solana|4,7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|固定DynamicFeeInfo9字段/原time-reference波动/spacing分段exact-in，mandatorybitmap/classicSPL/mode0..2/seed0..65535限定范围；348原native案例/12序列24步、8时间不变/6admission探针、2原始bank完整Pool及41进程、两阶段审查和冻结完整回归通过；非完整SBF或部署证明；reward/order/Token2022/其他程序版本/真实部署/流/WAL/交易SBF及生产未完成|
|balancer-v2-weighted2-equal|balancer-v2-weighted2/equal-1|EVM|7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|双币不可变50/50、普通非BPT代币及零资产管理器的版本限定模型；固定区块、765原onSwap案例、45组180步及17项PG/Kafka双副本检查通过；配置方标准非BPT声明不等于部署/Token行为证明；其他权重/Managed/Composable/RateProvider/V3未完成|
|balancer-v2-weighted2-static|balancer-v2-weighted2/static-1|EVM|7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|不可变双币、各权重≥1%且总和1e18，保留equal旧版；5355原onSwap案例/315组1260步、240pow向量及18项六池双副本验证通过；旧模型和canonical fixture保持；配置方标准非BPT声明不等于部署证明；多币/Managed/Composable/BPT/RateProvider/V3及执行未完成|
|balancer-v2-weighted-multi|balancer-v2-weighted-multi/static-1|EVM|7.3,11.3,12.2,12.9,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|3至20币的单物理池余额向量、全token路由边和固定块读取已完成限定模型本地验收；原参考和六模型进程通过，Managed/Composable另列任务|
|curve-plain3-basic|curve-plain3-basic/factory-1|EVM|7.3,11.3,12.2,12.9,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|三币Plain固定源码/布局/原exchange差分、完整后余额及正常RPC/路由通过，Meta/Lending/NG/Crypto不能由此替代|
|curve-plain4-basic|curve-plain4-basic/factory-1|EVM|7.3,11.3,12.2,12.9,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|四币Plain固定源码/布局/原exchange差分、多资产路由与完整后余额通过|
|curve-stableswap-ng|curve-stableswap-ng/3332cd44-standard-1|EVM|7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP09,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|固定原Vyper版本2–8币标准type0；完整raw/admin/effective余额、动态费、packed价格/D/EMA及正常服务接入已开发，三组正式进程回归及限定范围验收通过；rebasing/oracle/ERC4626、clones及真实部署/完整执行仍未完成|
|curve-crypto2-full|curve-crypto2-full/d7d04cd9-standard-1|EVM|7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP09,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|固定d7d04cd9原CryptoSwap2及原V4 LP模板；标准ERC20/settled A-gamma，完整price-scale调整/fee/gulp/LP账本已接入固定块解析和普通报价；正常服务进程与独立审查通过；本版本继续拒绝active ramp，活动ramp由独立版本另列；其他Crypto版本/特殊Token、真实部署与完整执行未完成|
|curve-crypto2-ramp|curve-crypto2-ramp/d7d04cd9-ramp-1|EVM|7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP09,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|固定d7d04cd9原CryptoSwap2与原V4 LP；标准ERC20的活动/到期/停止ramp，完整起点/未来端点及时间；原公开管理调用/完整交换、固定块解析与双副本正常服务限定验收通过；其他版本/特殊Token/真实历史及完整执行未完成|
|curve-metausd-direct|curve-metausd-direct/5272fd2b-fixed-base-1|EVM|7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP09,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|固定MetaUSD实现与EIP1167初始化池，标准meta/3CRV直接兑换、base虚拟价格依赖和完整raw/admin状态；针对性与四组正式流程回归、有限范围验收均通过；underlying、可变base同计划及真实部署/完整执行未完成|
|balancer-v2-managed-snapshot|balancer-v2-managed-snapshot/no-bpt-fees-1|EVM|7.3,11.3,12.2,12.9,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|2至38币当前权重快照，必须swaps enabled且协议/管理swap fee为零；非零费用BPT铸造仍未完成；后续非零费用批次发现所用旧参考源码runtime超EIP170，已有合成解析/零费数学结果不构成真实部署版本验收|
|balancer-v2-managed-fees|balancer-v2-managed-fees/e281ff40-bpt-1（参考契约，在线拒绝）|EVM|7.3,11.3,12.2,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP13,WP25|NOT_IMPLEMENTED|PARTIAL_LOCAL|NOT_IMPLEMENTED|非零协议/管理费BPT数学、完整供应量/收款方状态与原回调参考已实现；该固定旧源码runtime超过EIP170，正常配置/parser拒绝、在线报价未开放，需可部署实际版本后继续闭环|
|balancer-v2-managed-aum|balancer-v2/20230411-managed-aum-3|EVM|7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|实际20230411原Factory/Vault正常部署、2–50币原解析/数学与非零AUM参考已接入；双币当前日期样本普通服务首轮通过；派生execution context、实际钱包完整保留、最终冻结/独立计划VM重放与完整服务样本尚未完成；BPT路由、动态权重/费、启用breaker和其他版本仍未实现|
|balancer-v2-composable-regular|balancer-v2-composable-regular/no-rates-1|EVM|7.3,11.3,12.2,12.9,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|仅无rate provider的underlying间swap，完整余额含BPT但路由排除BPT；BPT数学内核有参考，完整待结费用/实际供应量/缓存刷新未完成|
|orca-whirlpools-adaptive|orca-whirlpools/cc683ec9-spl-adaptive-1|Solana|7.3,8.12,11.3,12.2,12.5,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP09,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|完整初始化Oracle/独立fee tier PDA、3fixed arrays、经典SPL和未初始化reward的独立版本；完整source/bank/wire及raw/fee/Oracle/tick后状态已实现；正常进程本地验收通过；initialized rewards/dynamic arrays/Token2022/SBF与真实部署未完成|
|orca-whirlpools-adaptive-rewards|orca-whirlpools/cc683ec9-spl-adaptive-rewards-1|Solana|7.3,8.12,11.3,12.2,12.5,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP09,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|固定cc683ec9 classic SPL/三fixed arrays/完整Oracle与初始化奖励增长、跨tick与完整后状态，正常RPC/双副本限定验收通过；dynamic arrays、Token2022、reward领取/position结算/SBF/CPI及真实历史仍未完成|
|orca-whirlpools-adaptive-dynamic|orca-whirlpools/cc683ec9-spl-adaptive-dynamic-1|Solana|7.3,8.12,11.3,12.2,12.5,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP09,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|classic SPL/完整Oracle/三连续fixed或dynamic混合数组及奖励，正常解析/数学/双副本服务限定验收通过；补充/稀疏数组、Token2022、领取/position/SBF/CPI及真实部署历史仍未完成|
|orca-whirlpools-rewards|orca-whirlpools/cc683ec9-spl-rewards-1|Solana|7.3,11.3,12.2,12.9,12.14|WP02,WP03,WP05,WP11,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|经典SPL/静态费，三奖励槽及tick增长；adaptive/Token2022/奖励领取和组合高级状态未完成|
|raydium-clmm-rewards|raydium-clmm/6d7a0f82-spl-rewards-1|Solana|7.3,11.3,12.2,12.9,12.14|WP02,WP03,WP05,WP11,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|经典SPL/静态输入费/default seed/基础bitmap奖励版；订单/Token2022/动态费和奖励组合未完成|
|uniswap-v4-advanced|uniswap-v4/core-advanced-1|EVM|7.3,11.3,12.2,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP14|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|key fee与live LP fee独立，native currency0/无swap回调hook；任意swap hook、自定义会计与真实settlement未完成|
|pancake-infinity-cl-advanced|pancake-infinity-cl/cl-advanced-1|EVM|7.3,11.3,12.2,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP14|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|动态LP费快照/native/callback位校验与CL后状态；本CL版本不兼容Bin；静态零hookBin完整树/跨Bin服务已在独立版本验收，完整manager/Vault结算未完成|

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

|11.3.managed-v2-published-state|WP02,WP04,WP05,WP11,WP25|PARTIAL_LOCAL|实际发布版独立状态/profile/config、sourceH、JSON/wire/clone与uint32原storage边界已接入；四副本独立C++ 2895字节通过|派生execution_context与canonical隔离、实际Vault钱包余额完整保留正在补齐；最终冻结与双审未完成|[wire初验](integration/artifacts/local/managed-v2-aum-wire-check.log)|
|7.3.managed-v2-fixed-bank|WP03,WP04,WP05,WP11|PARTIAL_LOCAL|固定blockHash/原runtime及links/immutables、真实Factory/Provider/Vault/collector getter与完整供应量/AUM核对；48原snapshot/81投影对照已接入|更多真实链版本/来源证明、实际余额完整保留及最终冻结未完成|[原bank精确再生](integration/artifacts/local/managed-v2-aum-reference-check.log)|
|12.14.managed-v2-original-replay|WP03,WP11,WP13,WP25|PARTIAL_LOCAL|正常EIP170原部署27case/292交易/2285对象，原swap与非零AUM/协议分成/零重复/uint32回退；81类原篡改普通与优化模式拒绝通过|普通服务只覆盖当前日期双币样本；其他未来clock状态仍为本地parser/math参考，不能放宽未来头准入|[普通保护](integration/artifacts/local/managed-v2-aum-guards.log)<br>[优化保护](integration/artifacts/local/managed-v2-aum-guards-optimized.log)|
|12.9.managed-v2-derived-context|WP02,WP11,WP12,WP13|IN_PROGRESS|固定H共享池连续重算通过；独立SPEC发现未来time同clock下一步拒绝及高度回卷后完整context丢失|显式派生执行高度/时间、sourceH不变、canonical/wire拒绝派生状态正在红绿补齐；不能据旧通过宣称完整连续重算|[本批任务](#plan-2026-11-03-managed-v2-aum)|
|23.managed-v2-normal-service|WP02,WP03,WP05,WP11,WP12,WP13,WP25|IN_PROGRESS|正常RPC/PG/Kafka双router首轮6原来源状态/4原swap金额/10来源故障撤销恢复/重启与VERIFIED-build拒绝共15检查通过，live C++ wire一致|后续源码变更使首轮为历史记录；完整钱包和派生上下文收口后需最终冻结全量/独立双审，原生选中计划VM重放未完成|[首轮服务记录](integration/artifacts/local/managed-v2-aum-three-service-smoke.json)|
|12.11.managed-v2-distinct-pause|WP02,WP03,WP04,WP05,WP11,WP13,WP25|IN_PROGRESS|原实际Vault暂停交易后Pool公开AUM collection仍成功，证实单一OR标志不符合原合约；按Pool/Vault各自有效flag和窗口独立建模，新增公开状态版本3|原暂停三case、固定块解析/数学完整后状态与正常服务边界正在补齐，最终同源全检和独立双审尚未完成|[原执行发现](integration/artifacts/local/managed-v2-aum-vault-pause-native-probe.log)|

<a id="batch-history"></a>

## 已验证批次摘要

下列均为历史本地验证范围；原始报告保留在integration/artifacts/local，详细原始输出不复制到本文件。

|序号|批次|结果|验证|
|---|---|---|---|
|1|Kafka/PG提交协议与租约隔离|LOCAL_SCOPE_VERIFIED|[kafka-three-service-smoke.json](integration/artifacts/local/kafka-three-service-smoke.json)|
|2|EVM重组撤销、回滚与原子发布|LOCAL_SCOPE_VERIFIED|[reorg-http-smoke.json](integration/artifacts/local/reorg-http-smoke.json)|
|3|V2成交事实与分析重组更正|LOCAL_SCOPE_VERIFIED|[analytics-journal-7.json](integration/artifacts/local/analytics-journal-7.json)|
|4|Uniswap V3固定块解析与有界精确输入报价|LOCAL_SCOPE_VERIFIED|[v3-sdk-golden-generation.json](integration/artifacts/local/v3-sdk-golden-generation.json)|
|5|Pancake V1/V2/V3与Sushi V2版本适配|LOCAL_SCOPE_VERIFIED|[v3-differential.json](integration/artifacts/local/v3-differential.json)|
|6|Curve/Balancer/Aerodrome限定稳定池解析与计算|LOCAL_SCOPE_VERIFIED|[stable-three-service-smoke.json](integration/artifacts/local/stable-three-service-smoke.json)|
|7|V4/Infinity CL静态无hook解析与计算|LOCAL_SCOPE_VERIFIED|[singleton-progress-consistency.json](integration/artifacts/local/singleton-progress-consistency.json)|
|8|Raydium CP-Swap creator布局解析与计算|LOCAL_SCOPE_VERIFIED|[raydium-progress-consistency.json](integration/artifacts/local/raydium-progress-consistency.json)|
|9|Orca静态费、固定/动态tick数组与Q64计算|LOCAL_SCOPE_VERIFIED|[orca-progress-consistency.json](integration/artifacts/local/orca-progress-consistency.json)|
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

<a id="plan-2026-10-27-raydium-clmm-static"></a>

## Raydium CLMM静态输入费与基础bitmap：任务完成情况

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|Task1|锁定源码和准入|WP03,WP20|LOCAL_SCOPE_VERIFIED|[rayclmm-golden-generation.json](integration/artifacts/local/rayclmm-golden-generation.json)<br>[raydium-clmm-static-quotes.md](docs/TECHNICAL_REFERENCE.md#raydium-clmm-static-quotes)|
|Task2|独立state/config/wire|WP02,WP25|LOCAL_SCOPE_VERIFIED|[rayclmm-differential.json](integration/artifacts/local/rayclmm-differential.json)<br>[rayclmm-wire-generation.log](integration/artifacts/local/rayclmm-wire-generation.log)|
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
|Task2|跨语言状态/config/wire先红后绿|WP02,WP25|LOCAL_SCOPE_VERIFIED|[rayclmm-bitmap-golden-generation.json](integration/artifacts/local/rayclmm-bitmap-golden-generation.json)<br>[rayclmm-bitmap-differential.json](integration/artifacts/local/rayclmm-bitmap-differential.json)<br>[rayclmm-bitmap-wire.log](integration/artifacts/local/rayclmm-bitmap-wire.log)|
|Task3|真实账户与RPC先红后绿|WP04,WP05,WP20|LOCAL_SCOPE_VERIFIED|[rayclmm-bitmap-golden-generation.json](integration/artifacts/local/rayclmm-bitmap-golden-generation.json)<br>[rayclmm-bitmap-differential.json](integration/artifacts/local/rayclmm-bitmap-differential.json)<br>[rayclmm-bitmap-parser-main.log](integration/artifacts/local/rayclmm-bitmap-parser-main.log)|
|Task4|跨bitmap数学与原program参考先红后绿|WP03,WP11,WP12,WP13,WP20|LOCAL_SCOPE_VERIFIED|[rayclmm-bitmap-golden-generation.json](integration/artifacts/local/rayclmm-bitmap-golden-generation.json)<br>[rayclmm-bitmap-differential.json](integration/artifacts/local/rayclmm-bitmap-differential.json)|
|Task5|集成/审查/完整回归/逐项进度|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP20,WP25|LOCAL_SCOPE_VERIFIED|[rayclmm-bitmap-golden-generation.json](integration/artifacts/local/rayclmm-bitmap-golden-generation.json)<br>[rayclmm-bitmap-differential.json](integration/artifacts/local/rayclmm-bitmap-differential.json)<br>[rayclmm-bitmap-three-service-smoke.json](integration/artifacts/local/rayclmm-bitmap-three-service-smoke.json)<br>[rayclmm-bitmap-review.json](integration/artifacts/local/rayclmm-bitmap-review.json)<br>[rayclmm-bitmap-final-source.json](integration/artifacts/local/rayclmm-bitmap-final-source.json)<br>[rayclmm-bitmap-progress-consistency.json](integration/artifacts/local/rayclmm-bitmap-progress-consistency.json)|

<a id="plan-2026-10-29-raydium-clmm-custom-seed"></a>

## Raydium CLMM非零seed独立池身份：任务完成情况

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|Task1|固定原seed语义|WP03,WP20|LOCAL_SCOPE_VERIFIED|[rayclmm-seed-native-source.json](integration/artifacts/local/rayclmm-seed-native-source.json)<br>[raydium-clmm-custom-seed-quotes.md](docs/TECHNICAL_REFERENCE.md#raydium-clmm-custom-seed-quotes)|
|Task2|跨语言state/wire/config先红后绿|WP02,WP25|LOCAL_SCOPE_VERIFIED|[rayclmm-seed-go-green.log](integration/artifacts/local/rayclmm-seed-go-green.log)<br>[rayclmm-seed-wire.log](integration/artifacts/local/rayclmm-seed-wire.log)<br>[rayclmm-seed-differential.json](integration/artifacts/local/rayclmm-seed-differential.json)|
|Task3|实际账户/RPC先红后绿|WP04,WP05,WP20|LOCAL_SCOPE_VERIFIED|[rayclmm-seed-parser-main.log](integration/artifacts/local/rayclmm-seed-parser-main.log)<br>[rayclmm-seed-three-service-smoke.json](integration/artifacts/local/rayclmm-seed-three-service-smoke.json)|
|Task4|原native金额与共享池|WP03,WP11,WP12,WP13,WP20|LOCAL_SCOPE_VERIFIED|[rayclmm-seed-native-source.json](integration/artifacts/local/rayclmm-seed-native-source.json)<br>[rayclmm-seed-differential.json](integration/artifacts/local/rayclmm-seed-differential.json)|
|Task5|进程/审查/完整回归/逐项进度|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP20,WP25|LOCAL_SCOPE_VERIFIED|[rayclmm-seed-three-service-smoke.json](integration/artifacts/local/rayclmm-seed-three-service-smoke.json)<br>[rayclmm-seed-review.json](integration/artifacts/local/rayclmm-seed-review.json)<br>[rayclmm-seed-final-source.json](integration/artifacts/local/rayclmm-seed-final-source.json)<br>[rayclmm-seed-progress-consistency.json](integration/artifacts/local/rayclmm-seed-progress-consistency.json)|

<a id="plan-2026-10-29-raydium-clmm-dynamic"></a>

## Raydium CLMM动态费与分段精确计算：任务完成情况

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|Task1|固定原动态费语义|WP03,WP20|LOCAL_SCOPE_VERIFIED|[rayclmm-dynamic-native-source.json](integration/artifacts/local/rayclmm-dynamic-native-source.json)<br>[raydium-clmm-dynamic-quotes.md](docs/TECHNICAL_REFERENCE.md#raydium-clmm-dynamic-quotes)|
|Task2|跨语言state/wire/config先红后绿|WP02,WP25|LOCAL_SCOPE_VERIFIED|[rayclmm-dynamic-go-green.log](integration/artifacts/local/rayclmm-dynamic-go-green.log)<br>[rayclmm-dynamic-wire.log](integration/artifacts/local/rayclmm-dynamic-wire.log)<br>[rayclmm-dynamic-differential.json](integration/artifacts/local/rayclmm-dynamic-differential.json)|
|Task3|实际账户/RPC先红后绿|WP04,WP05,WP20|LOCAL_SCOPE_VERIFIED|[rayclmm-dynamic-parser-main.log](integration/artifacts/local/rayclmm-dynamic-parser-main.log)<br>[rayclmm-dynamic-three-service-smoke.json](integration/artifacts/local/rayclmm-dynamic-three-service-smoke.json)|
|Task4|独立动态数学与原program参考先红后绿|WP03,WP11,WP12,WP13,WP20|LOCAL_SCOPE_VERIFIED|[rayclmm-dynamic-native-source.json](integration/artifacts/local/rayclmm-dynamic-native-source.json)<br>[rayclmm-dynamic-differential.json](integration/artifacts/local/rayclmm-dynamic-differential.json)|
|Task5|进程/审查/完整回归/逐项进度|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP20,WP25|LOCAL_SCOPE_VERIFIED|[rayclmm-dynamic-three-service-smoke.json](integration/artifacts/local/rayclmm-dynamic-three-service-smoke.json)<br>[rayclmm-dynamic-review.json](integration/artifacts/local/rayclmm-dynamic-review.json)<br>[rayclmm-dynamic-clock-red.log](integration/artifacts/local/rayclmm-dynamic-clock-red.log)<br>[rayclmm-dynamic-clock-green.log](integration/artifacts/local/rayclmm-dynamic-clock-green.log)<br>[rayclmm-dynamic-resume-green.log](integration/artifacts/local/rayclmm-dynamic-resume-green.log)<br>[full-local-acceptance-resume.json](integration/artifacts/local/full-local-acceptance-resume.json)<br>[raydium-lease-recovery-audit.json](integration/artifacts/local/raydium-lease-recovery-audit.json)<br>[rayclmm-dynamic-final-source.json](integration/artifacts/local/rayclmm-dynamic-final-source.json)<br>[rayclmm-dynamic-progress-consistency.json](integration/artifacts/local/rayclmm-dynamic-progress-consistency.json)|

<a id="plan-2026-10-30-balancer-weighted2-equal"></a>

## Balancer V2双币不可变50/50权重池：任务完成情况

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|Task1|固定原源码、构造器/onSwap与Vault余额参考；限定无managed资金、标准非BPT代币|WP03,WP11|LOCAL_SCOPE_VERIFIED|[源码清单](integration/tools/reference/weighted-model/source-metadata.json)<br>[原合约参考](integration/artifacts/local/weighted-golden-generation.json)|
|Task2|独立WeightedState/Config、版本准入和protobuf field27；三服务契约|WP02,WP25|LOCAL_SCOPE_VERIFIED|[三服务及公共协议完整检查](integration/artifacts/local/verification.json)|
|Task3|固定blockHash读取pool/Vault/weights/scales/decimals/tokeninfo；保留receipt根和读后锚定|WP04,WP05,WP20|LOCAL_SCOPE_VERIFIED|[解析与撤销回归](integration/artifacts/local/weighted-review-green.log)<br>[双副本进程](integration/artifacts/local/weighted-three-service-smoke.json)|
|Task4|原始扣费/缩放/舍入/净输入30%上限/uint256中间值与uint112后余额；共享池连续兑换|WP11,WP12,WP13|LOCAL_SCOPE_VERIFIED|[765案例及45组180步差分](integration/artifacts/local/weighted-math-differential.log)<br>[分配重算测试](integration/artifacts/local/weighted-routing-tests.log)|
|Task5|PG/Kafka双副本、完整scope/重组/撤销恢复、独立审查、三服务检查与统一进度|WP01,WP03,WP09,WP14,WP16,WP25|LOCAL_SCOPE_VERIFIED|[17项进程检查](integration/artifacts/local/weighted-three-service-smoke.json)<br>[审查关闭](integration/artifacts/local/weighted-review.json)<br>[本批验收](integration/artifacts/local/weighted-acceptance.json)|

边界：仅指定源码域的双币不可变50/50模型，固定runtime/代币code哈希与`standard-no-bpt`为配置方准入声明；尚无真实部署/immutable编译产物映射或特殊代币行为证明。原合约参考执行构造器与onSwap及余额打包，Vault facade不执行完整Vault/真实ERC20转账。其他权重、Managed/Composable/BPT/RateProvider/V3、Weighted成交事实分析、独立执行/真实收款及VERIFIED均未实现。本批只推进R06有限模型，不改变原26工作包完整验收为0。

验证结果：三服务与common/protoc完整检查通过，含五个实际执行文件的构建指纹；91份契约与8份fixture副本一致。765参考案例、45组180步连续兑换逐步比较输出及双侧余额；正常Go RPC→PG/Kafka→双路由17项检查、基础与原稳定池两组当前回归通过。新增20项验收辅助、全部91项辅助及31项文档映射测试通过。SPEC与独立质量审查问题已修复，保留失败/通过回归；普通模式和`PYTHONOPTIMIZE=2`证据门禁均通过。

<a id="plan-2026-10-30-balancer-weighted2-static"></a>

## Balancer V2双币不可变任意合法权重：任务完成情况

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|Task1|固定同版本原WeightedPool/LogExpMath/FixedPoint参考；新独立版本与旧50/50边界|WP03,WP11|LOCAL_SCOPE_VERIFIED|[固定40原文件](integration/tools/reference/weighted-model/source-metadata.json)<br>[原构造器/onSwap及pow执行参考](integration/artifacts/local/weighted-static-golden-generation.json)|
|Task2|独立static profile与现有Weighted字段复用；最小权重、归一化总和及Go/Rust/wire一致准入|WP02,WP25|LOCAL_SCOPE_VERIFIED|[三服务及公共协议完整检查](integration/artifacts/local/verification.json)<br>[规范adapter-set身份回归](integration/artifacts/local/weighted-static-adapter-hash-green.log)|
|Task3|同blockHash双币权重解析；保留Vault/token/cash/manager/receipt及重锚定屏障|WP04,WP05,WP20|LOCAL_SCOPE_VERIFIED|[双profile固定块准入](integration/artifacts/local/weighted-static-go-contract-parser.log)<br>[六池双副本进程](integration/artifacts/local/weighted-static-three-service-smoke.json)|
|Task4|整数LogExpMath和powUp误差界、权重比向下舍入、完整raw后余额与原onSwap连续差分|WP11,WP12,WP13|LOCAL_SCOPE_VERIFIED|[5355案例及315组1260步差分](integration/artifacts/local/weighted-static-differential.log)<br>[240组原pow向量](integration/artifacts/local/weighted-static-power-differential.log)|
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
|T29.5|§23/28：在线clock与优化模式拒绝校验修复、三服务/common检查、SPEC后质量审查、逐项台账同步|WP01,WP25|LOCAL_SCOPE_VERIFIED|[全量检查](integration/artifacts/local/verification.json)<br>[实际进程](integration/artifacts/local/curve-crypto-ramp-three-service-smoke.json)<br>[独立审查](integration/artifacts/local/curve-crypto-ramp-review.json)<br>[本批验收](integration/artifacts/local/curve-crypto-ramp-acceptance.json)<br>[逐项一致性](integration/artifacts/local/curve-crypto-ramp-progress-consistency.json)|

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
|T30.5|§23/28：三服务/common全量、真实进程/撤销恢复/重启、SPEC后独立质量审查与逐项进度|WP01,WP09,WP14,WP16,WP20,WP25|LOCAL_SCOPE_VERIFIED|[全量验证](integration/artifacts/local/verification.json)<br>[实际进程](integration/artifacts/local/orca-adaptive-rewards-three-service-smoke.json)<br>[独立审查](integration/artifacts/local/orca-adaptive-rewards-review.json)<br>[本批验收](integration/artifacts/local/orca-adaptive-rewards-acceptance.json)<br>[逐项一致性](integration/artifacts/local/orca-adaptive-rewards-progress-consistency.json)|

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
|T32.5|§23/28：三服务回归、实际进程与故障/隔离、两阶段独立审查与逐项进度|WP01,WP14,WP15,WP16,WP25|LOCAL_SCOPE_VERIFIED|[全量](integration/artifacts/local/verification.json)、[两阶段审查](integration/artifacts/local/evm-execution-review.json)、[逐项一致性](integration/artifacts/local/evm-execution-progress-consistency.json)|

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
|T33.5|完整状态绑定到既有独立执行及正常API：不混用来源/时钟，核对实际raw/时间戳/累计价/锁；双副本、来源故障撤销恢复、重启、共享计划原生重放；全量回归、SPEC后质量审查、逐项进度一致性|WP05,WP10,WP11,WP13,WP14,WP15,WP25|LOCAL_SCOPE_VERIFIED|[全量](integration/artifacts/local/verification.json)、[真实进程](integration/artifacts/local/v2-complete-three-service-smoke.json)、[双审](integration/artifacts/local/v2-complete-review.json)、[逐项一致性](integration/artifacts/local/v2-complete-progress-consistency.json)|

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
|T34.1|固化原build-info完整input/output/来源/许可证、两编译job、links/immutables、正常Vault/Factory/Token部署；生成普通swap、非零AUM/协议比例、重复/零/边界收取、多币/精度与账本原bank，禁止facade/storage injection/无限codesize代替正常链路|WP03,WP11,WP25|LOCAL_SCOPE_VERIFIED|27case/292真实交易、2/3/5/50 underlying、2285内容寻址对象/87代码身份，两个原编译job及正常EIP170原部署完整闭包已固化；精确临时再生与81类普通/优化篡改拒绝通过；原future/u32边界bank字节与时间保持不改；[最终同源限定验收](integration/artifacts/local/managed-v2-aum-acceptance.json)通过，真实链身份另列未完成|
|T34.2|独立ManagedV2状态/profile/config/wire及Snapshot/clone/未知字段/presence；精确验证Vault BPT、virtual/actual/pending供应量与source bank，不把derived state当新canonical，不改旧拒绝范围|WP02,WP04,WP11|LOCAL_SCOPE_VERIFIED|公开aum-3独立模型/config、39字段C++3023字节四副本/15自测与Go/Rust契约通过；required真实钱包、独立Pool/Vault暂停及Recovery有效费率、BPT固定注册/feeIDuint8/u32storage/clone/known0/未知值拒绝已验证；所有成功转换始终Some(ctx)、sourceH不变且canonicalSnapshot/wire拒绝派生；[最终同源限定验收](integration/artifacts/local/managed-v2-aum-acceptance.json)通过|
|T34.3|固定blockHash解析实际runtime/templates/依赖、Vault注册与balances、原AUM/cache/provider映射/完整账本及可成交状态；2–50 underlying按原协议容量，现金不足/暂停/禁swap等按真实语义处理|WP03,WP04,WP05,WP11|LOCAL_SCOPE_VERIFIED|固定blockHash、原runtime/templates/links/immutables与完整Factory/Provider/Vault/getter核对；48原snapshot、81薄投影Pool及新暂停/Recovery前后真实状态逐字段解析通过；BPT/钱包/source/AUM反例与旧版本隔离、正常双副本来源故障闭环通过；[最终同源限定验收](integration/artifacts/local/managed-v2-aum-acceptance.json)；当前四池目录正式服务接入仍属T34.8|
|T34.4|Rust精确AUM preview/collection与新版普通Weighted swap、SafeMath/舍入/别名账本及完整后状态、共享物理池连续重算；先红后绿并与原发布完整Vault实际交易差分|WP11,WP12,WP13|LOCAL_SCOPE_VERIFIED|原fee-after-upscale、2–50币数学、AUM floor/floor/ceil及别名账本完整后状态通过；原8swap/17collection（16成功1回退）/81Pool、同池sharedallocation、futureclock连续性/完整height防退及原暂停与Recovery swap/collection矩阵通过；[最终同源限定验收](integration/artifacts/local/managed-v2-aum-acceptance.json)；跨不同物理池Vault账本仍属T34.9|
|T34.5|正常source→PG/Kafka→双router报价/来源故障撤销恢复/重启、独立原生计划重放与完整状态检查；三服务/common全量、CPP wire、普通/优化反例及SPEC后quality审查和逐项进度一致性|WP02,WP03,WP05,WP11,WP12,WP13,WP25|LOCAL_SCOPE_VERIFIED|三服务/common完整check与五二进制、契约208/fixture8、普通/优化49辅助、15独立wire自测通过；source/PG/Kafka双router15银行/8正向/6暂停拒绝/5Recovery来源、10故障恢复/重启等18检查通过，选中4API路径28原签名交易独立重放且真实余额/完整getter/原receipt一致；当前完整输入绑定的SPEC/quality独立双审通过；[最终同源限定验收](integration/artifacts/local/managed-v2-aum-acceptance.json)；仅本节单池范围|
|T34.6|按原计划补完整Vault实际钱包状态：各underlying实际余额及Vault实际BPT与注册cash/managed分离，JSON/wire/clone/来源准入/整数swap和AUM别名收取后状态逐项保留，原getter投影差分与正常服务验证|WP02,WP03,WP05,WP11,WP13|LOCAL_SCOPE_VERIFIED|Vault实际BPT及每underlying实际钱包作为必需Amount进入JSON/wire/tag6及36/clone/原getter投影，registered cash/managed与global wallet区分；global>cash、u256溢出、gross/raw钱包delta、Vault收款别名AUMmint去重及81完整原Pool与实际服务通过；[最终同源限定验收](integration/artifacts/local/managed-v2-aum-acceptance.json)；跨物理池global传播仍属T34.9|
|T34.7|原Pool与Vault暂停语义分别建模与差分：保留各自有效paused、pause/buffer截止；普通swap分别准入，公开AUM collection不因仅Vault暂停误拒绝，未来clock过期视图更新及完整后状态|WP02,WP03,WP04,WP05,WP11,WP13,WP25|LOCAL_SCOPE_VERIFIED|独立原暂停3case真实状态0/1/1、7个source/after/executionSource完整Pool与数学后状态验证通过；pool/Vault各自暂停及期限37–39已分离，collection仅受Pool暂停约束，Vaultbuffer到期后普通双副本恢复报价、有效flag与source/derived时钟一致；[最终同源限定验收](integration/artifacts/local/managed-v2-aum-acceptance.json)通过|
|T34.8|补齐当前日期可服务的2/3/5/50币原Factory/Vault目录：独立正常EIP170真实交易生成，不改旧长时间/uint32边界bank；每次引用完整共同源H的全部池getter/实际钱包，普通source→PG/Kafka双副本多币报价和逐笔完整后状态/原计划余额重放|WP03,WP04,WP05,WP11,WP12,WP13,WP25|IN_PROGRESS|已在/tmp独立正常EIP170实跑2/3/5/50四池原型，4swap与30天4次非零AUM、10组四池共同H/9800原getter及精确再生通过；全部H位于2026-01-01/01-31且未改原future bank；仅原型并已归档到integration/artifacts/local/prototypes/2026-11-04-managed-v2-current-catalog防止/tmp丢失；tmp overlay已实测Go正常parser解析10共同源H×4池=40份完整Pool一致（仅写/tmp，无repo源码改动）；正式generator/认证projection/Go40/Rust8已导入，已把known可变验收输出与不可变manifest成员精确分离并真实再生；当前ROOT新raw b843e859…/13785866字节的三副本精确再生、Go40完整Pool/Rust8完整后状态及8认证边界测试通过；4池普通RPC→PG/Kafka→双router真实进程验收通过：10共同来源H、8原交换双副本正向、4来源故障撤销/新generation恢复、Solana实际报价/重启/VERIFIED-build拒绝；新范围完整冻结验收和双审尚未完成，不能据其宣称T34已完整验收|
|T34.9|不同Managed物理池共享同Vault/underlying的全局钱包overlay：跨池金额路径及拆单按实际执行顺序同步Vault实际余额、暂停依赖和账户别名，保留各自registered cash/managed与BPT供应；原多池实际交易完整后状态及普通服务验证|WP02,WP03,WP11,WP12,WP13,WP14|IN_PROGRESS|共享allocation已接入跨物理Managed池的全局钱包传播；独立/tmp原参考已实跑真实同Vault/同underlying双池正常部署、4次A/B/A/B交替swap及30天CollectionA，7完整双池银行/1064原getter和精确再生通过；原反例证实未选中池注册余额不变但global Vault钱包随交易变化，已归档integration/artifacts/local/prototypes/2026-11-04-managed-v2-shared-vault；临时Rust候选已实现同Vault钱包传播及整path原子提交，34测试（26专项+8原routing回归）通过、原实现同套19专项失败，已归档integration/artifacts/local/prototypes/2026-11-04-managed-v2-shared-overlay；domain共享事实与执行能力隔离、routing钱包传播/整path原子提交已正式接入，30路由定向及Go共享14Pool解析通过；31路由（含大额容量探测红绿）、13 optimizer/replica/HTTP范围回归及9独立多route原Vault计划重放回归已通过；共享2池7银行原raw已按原admin正常mint把交易方资金升至10000token，以真实验证超单池容量的API拆单；4笔原交换输出及AUM金额不变，四份独立消费副本同步，当前raw SHA `1f917c9951269028d24f927e1d415229f200019881bd05991e8a84f0b49c79e2`/2720562字节，Go14完整Pool解析通过；首轮真实服务7源银行/8 API计划原VM通过但合计cash缺1故障未撤销，已补globalactual≥去重的已声明registered cash合计（不含managed），并红绿修复同H角色code/live-provider事实及重复注册冲突顺序稳定，第二轮全部7source/14真实计划与4故障恢复均跑通，但因开发中的输入变化最终封闭为FAILED，未继承为通过；资金版真实API两副本均已选中2物理池：输入5000000000000000000000、独立原VM真实收款7874328412914279650444且全计划/钱包后状态一致；整套来源/故障/重启及新范围独立审查/完整冻结验收仍在进行，不能以原型通过代表跨池产品结算完成|
|T34.10|Recovery状态来源与有效AUM费率：按原getManagementAumFeeParams在recovery中返回0、normal时校验配置常规费率；原enable/普通swap/零collection/disable真实状态、完整后状态、普通服务进入/退出及审查闭环|WP02,WP03,WP04,WP05,WP11,WP13,WP25|LOCAL_SCOPE_VERIFIED|原独立7case分支实际enable/swap/零collection/disable成功并精确再生；recovery getter有效0、normal配置费恢复1%、零mint仍更新时间已完整差分；三domain拒绝recovery非零费/零费非零pending；15源bank服务路径含5recovery来源、2次recovery正向及退出恢复，质量P2独立复核关闭；[最终同源限定验收](integration/artifacts/local/managed-v2-aum-acceptance.json)通过，control只标原来源观察|

执行顺序：主线程先基于原源码确定公共状态与精确AUM算术；原发布参考正常链路可独立固化，固定块parser随后按已声明schema并行；普通swap与显式collection分开测试，不允许用swap时虚构BPT铸币。基础Weighted不变量/幂函数仅在版本差分通过后复用。原getter/代码identity不完整、未知rate/breaker/token能力均失败关闭；动态权重/费用、启用breaker、BPT swap/join/exit、实际主网来源及其他Managed版本仍保留后续设计功能，不因本批边界删除。完成状态仍需实际源、原合约、数学/后状态、服务、故障与双审的同一冻结闭包。

独立SPEC审查补充：固定H普通共享重算通过，不代表未来clock派生连续性通过。已复现未来time swap后同clock再次swap/collect因旧source时钟错误拒绝，以及完整执行高度超过2^32后回退到source高度仍被接受；已增加单独派生执行上下文并完成连续clock红绿回归，不能修改sourceH或取消原storage低32位语义。进一步SPEC复现固定H首次转换仍可伪装canonical，已将所有成功转换改为显式derived，新增Snapshot/wire拒绝回归通过。实际钱包缺口已按T34.6贯通模型/原getter/数学/独立wire，保持registered与global wallet分离；仍需最终冻结验收。

本轮续作核对：三个原bank副本均为7905624字节、SHA256 `1ee7ec5dd981994af6dd42eb75717cf83a5b209857883fa2923885ce7181fb66`；精确再生记录见 `integration/artifacts/local/managed-v2-aum-reference-check.log`。新增getter不改变27案例/292交易的历史经济投影。部分原bank因长时间/uint32边界时间推进，时间晚于当前本地日期；正常服务必须继续按未来头拒绝，不能为测试放开时间准入。服务验收需明确可服务样本和边界拒绝范围。

本轮当前已实现单物理池范围历史最终验收通过（2026-11-04；后续T34.8/T34.9代码变动后不作为新范围通过证据，完整源码/报告已归档[历史闭包](integration/artifacts/local/accepted/2026-11-04-managed-v2-single-pool/MANIFEST.json)）：[同源验收报告](integration/artifacts/local/managed-v2-aum-acceptance.json)绑定三服务/common完整check、五个二进制、208份契约/8份fixture、当前全部原输入、独立SPEC与quality无遗留审查。原27case/292tx/2285对象、旧原稿完整两编译job的精确临时再生、81篡改普通/优化拒绝；独立39字段C++3023字节四副本/15自测；普通/优化各49辅助回归均通过。实际source→PG/Kafka→双router的15原来源状态、8正向报价、6暂停状态拒绝但保持ready、5Recovery有效零费率来源及退出恢复常规费、10来源故障撤销/新generation恢复、无关Solana报价及重启、VERIFIED/build拒绝，共18检查通过。选中4份API单跳计划由独立原Vault在新VM中重放前28真实签名交易，核对真实扣款/收款余额、完整getter/账户、原header/receipt。控制enable/disable只计真实原来源观察，不冒称已实现控制数学内核；独立执行仍是本地参考，不是可信链/生产执行器。

T34尚未完成。T34.8与T34.9仍为IN_PROGRESS，分别保留当前日期四池目录正式接入和跨物理池共享Vault结算，且不减少原设计中的BPT路由/动态权重和费用/启用breaker/其他Managed版本或真实链与生产要求。T33及更早报告自后续源码/输入变更起为历史验收；原26工作包完整验收仍为0，总体目标仍为全部设计功能完成。

<a id="remaining-priorities"></a>

## 剩余功能与开发优先级（代码核对：2026-11-03）

核对口径：以实际服务入口/协议分派/拒绝分支和原§28验收要求为准，不以文件夹存在、测试样本数量或配置标签当功能完成。原26工作包中24个PARTIAL_LOCAL、2个NOT_STARTED（WP18/WP22），完整验收仍为0。51个协议条目中40个有局部解析、42个有局部数学；所有条目的完整执行仍未验收。解析仅归档的raydium-cpmm不等于creator新profile已覆盖全部版本。

“最高/高/中/上线前/低”是剩余事项的安排建议，不改写原§24.1的P0–P8阶段定义。每个协议按固定版本身份→状态/wire→实际解析→精确数学及完整后状态→独立参考→恢复/进程验证逐项完成；数据不足保持失败封闭。

|编号|优先级|设计章节 / 工作包|尚未完成的功能|代码核对与完成门槛|
|---|---|---|---|---|
|R01|最高|§7.1/12.11/12.14；WP02,WP03,WP10,WP11,WP18,WP20|真实部署身份与真实历史块参考；源码、部署二进制、升级slot/合约实现逐版本核对|现有原生/SDK差分多为固定源码+合成RPC；以真实块/账户来源、可重复回放和独立收款差分补证，不把code hash配置当部署审核|
|R02|最高|§7.1–7.3/7.9；WP04,WP05,WP07|EVM持续采集完整性：WS通知/轮询一致、provider能力/限流/双源head、并行staging有序提交、raw归档/持久DLQ、工厂发现|[adapter.go](chain-parser-go/internal/chain/evm/adapter.go)已有结构、多RPC全量receipt及RPC声明root重算局部验证，标准0–4/legacy及共同删尾拒绝通过；独立可信头/transactionsRoot/共识、真实来源独立审核与链专有receipt仍缺；WS/provider矩阵、并行staging、raw归档/DLQ/工厂发现和更多规范事实仍未完成|
|R03|最高|§7/18/28.22；WP04,WP05,WP07,WP20,WP24|Solana Yellowstone接入、流WAL连续恢复、补洞、slot/fork屏障、事务/metadata/账户乱序重建|[rpc.rs](chain-parser-rust/crates/solana-ingest/src/rpc.rs)为finalized当前账户批次；[assembly.rs](chain-parser-rust/crates/solana-ingest/src/assembly.rs)有本地屏障模型，已补正常RPC原始接收证据WAL；同context当前RPC分片已局部实现；尚未形成真实流到WAL/fork的完整链路|
|R04|最高|§7/18/28.10/28.26；WP08,WP09,WP20,WP24|范围化恢复与扩容：EnsurePoolScope、账户依赖分片、当前基线选择和历史补洞隔离|当前RPC已支持同源同finalized context最多32片（每片99依赖+Clock、总响应预算不扩倍），16池134账户/13池CLMM seed及全scope恢复通过；slot推进导致无法收齐时失败封闭。EnsurePoolScope控制接口、连续流/版本化账户与历史补洞隔离仍未完成，不能将bootstrap当独立bank/严格slot证明|
|R05|高|§11/12.2/12.13/12.14；WP02,WP03,WP10,WP11,WP20,WP25|Solana现有协议扩展：Token2022、转账费/账户扩展，Orca adaptive的Token2022/补充稀疏数组及其他高级组合/奖励领取，Raydium限价订单、奖励领取和高级组合|[orca.rs](chain-parser-rust/crates/solana-ingest/src/orca.rs)已支持独立经典SPL静态费奖励版，旧profile仍拒绝adaptive oracle；独立adaptive版本已补完整Oracle账户/普通分派；新增独立adaptive-rewards版的classic SPL/三fixed数组/初始化奖励增长与跨tick完整状态、正常RPC/双副本服务已完成限定验收；第31批独立dynamic版本已完成三连续完整dynamic/fixed混合数组解析与普通报价/全后状态及双副本限定验收；[raydium_clmm.rs](chain-parser-rust/crates/solana-ingest/src/raydium_clmm.rs)已支持独立静态输入费基础bitmap奖励版，仍拒绝订单及未建模组合；每种版本补独立状态/wire、解析、原数学/完整后状态差分|
|R06|高|§12.2/12.11/12.14；WP02,WP03,WP05,WP11|Curve其他Plain变体/Meta underlying/旧Lending及NG特殊Token/外部rate及Crypto其他版本，Balancer Managed非零BPT费用/Composable待结费用与BPT/rate-cache/V3|[stable.go](chain-parser-go/internal/chain/evm/stable.go)与[weighted.go](chain-parser-go/internal/chain/evm/weighted.go)仅命名限定模型，双币及3–20币Weighted、Managed零swap-share快照、Composable无rate-provider常规交换和Plain3/4已本地验证；非零Managed BPT完整数学/状态契约已补原回调参考，但该旧源码runtime超EIP170，正常服务继续拒绝；实际20230411版本已补独立aum-3状态、固定块解析、非零AUM/钱包及原Pause/Recovery差分，完整本批最终验收仍未通过；当前日期2–50目录整合与跨物理池共享Vault全局钱包传播见T34.8/T34.9；Meta直接兑换已补正常服务和只读base依赖；Meta underlying、Crypto其他版本及BPT路由仍未完成；Crypto活动ramp独立版本已完成固定块解析、完整schedule/后状态与正常双副本服务限定验收；Crypto标准settled独立版本已补原LP/gulp/调价完整状态及正常解析/报价，本地进程验收通过；标准NG独立profile已完成固定原源码模板/immutable准入、完整状态/EMA及正常RPC与双副本连续报价；[protocol-math](dex-router/crates/protocol-math/src/lib.rs)不支持的数学失败；按实际代码版本逐个补状态、不变量、精度与费用|
|R07|高|§12.11/12.14；WP03,WP05,WP11|Aerodrome/Slipstream非零外部费用模块、LM/reward与忠实费用账本；PancakeV3 LM|[v3.go](chain-parser-go/internal/chain/evm/v3.go)对未知/非零外部模块有限制；需同块依赖、实际回调语义及原合约执行差分，不能用标准V3参考替代全部Slipstream|
|R08|高|§12.2/12.11/12.14；WP02,WP03,WP05,WP11,WP14|V4/Infinity swap hook、自定义会计/覆盖费、native结算、Infinity Bin动态/hook/native组合及完整manager结算|[singleton.go](chain-parser-go/internal/chain/evm/singleton.go)及独立advanced版本已补动态LP费快照/native currency0/无swap回调权限与CL core后状态；Bin已补固定3c5ef1be原manager/tree完整读取、跨bin数学/后状态与静态零hook普通服务限定验收；完整manager/Vault结算未完成；新能力需manager状态、hook依赖和独立执行，unknown hook仍拒绝VERIFIED|
|R09|高|§12.2/12.11/28.22；WP02,WP03,WP05,WP11,WP20|Meteora DLMM、Pump/迁移曲线、LFJ Liquidity Book、Sushi Trident等新模型|当前协议分发表无这些报价内核；需独立bin/曲线/lot模型、真实可成交限制与原程序参考，不能套用V2/V3；先完成R05-R08现有家族缺口再扩大协议面|
|R10|高，可并行|§12.13/14/28.16/28.17；WP10,WP14,WP15,WP20|独立执行后端与真实收款余额差：EVM REVM/固定状态DB/链规则；Solana版本化账户及SBF/Token CPI|原[simulation::verify](dex-router/crates/simulation/src/lib.rs)保持无合格后端时拒绝；T32新增独立REVM严格显式本地状态DB、Shanghai单hop标准V2完整交易与真实收款余额/全声明后状态差分，以及可终止worker/正常simulate API，已通过限定本地验收；可信链按blockHash取状态、多链规则/费用、其他协议/拆单和SBF仍未完成，是开放VERIFIED的硬阻断|
|R11|高，依赖R10|§14/28.16/28.18/28.22；WP14,WP16,WP20|受审计执行器与可用交易：各协议/拆单DAG calldata、授权/wrap/费用/退款；Solana交易/CU/账户锁/blockhash|[execution-plan](dex-router/crates/execution-plan/src/lib.rs)新增完整单路V2 BOUND信封与精确calldata/上下文/池状态重建校验；正常simulate API通过同二进制隔离worker执行已提供本地状态，限定本地服务验收通过；其他协议/多hop/拆单/授权wrap退款及链费用未完成，公开build/VERIFIED仍拒绝|
|R12|中，交易前完成|§12.10/13/28.14/28.15；WP12,WP13,WP16|实际Gas/L1数据费/原生费换算、净输出评分和费用完整性|quote响应netBuyAmountEstimate为null，CHAIN_FEE_MODEL_NOT_IMPLEMENTED；配置route_activation_cost只是提案成本，不是实测链上费用|
|R13|中|§12.10/13/28.14/28.15；WP12,WP13|图增量索引/COW、beam多标签、三路及以上拆单、共享DAG联合求解、小图穷举及收益gap|[optimizer](dex-router/crates/optimizer/src/lib.rs)显式max_splits<=2；当前为有界启发式；先补独立小图oracle和反例，再提高搜索/分配能力|
|R14|中|§12.10/12.13/13/28.12–28.16；WP10,WP11,WP13,WP14,WP16|ExactOut完整输入目标/退款语义、转账税/rebase/暂停/黑名单/代理升级能力分级|[QuoteRequest](dex-router/crates/quote-api/src/lib.rs)仅EXACT_IN；特殊Token须按真实caller/recipient上下文仿真或拒绝；ExactOut不能对ExactIn结果取倒数|
|R15|中，规模化前|§9/18/28.10/28.11/28.26；WP08,WP09,WP24|S3/对象存储不可变checkpoint、公共manifest校验、peer选择；大池COW/慢句柄回收和恢复容量|当前owner Protobuf bootstrap和有界Kafka tail已实现；未完成对象存储下载/上传故障、1小时/1天/30天停机、WAL溢出和无源恢复专项|
|R16|上线前必做|§7.5–7.8/18/28.8/28.9/28.19/28.25；WP06,WP07,WP17,WP23|Kafka TLS/SASL/ACL、真实ISR/retention/topic重建与事务故障；PG切换、磁盘/节点/可用区故障、灾备RPO/RTO及shadow放量|现有专用单机PG/Kafka及进程接管测试已做；不等于多区/独立故障域验收，生产准入必须单独验证|
|R17|上线前必做|§4/20/28.3/28.4/28.27/28.28；WP01,WP02,WP25,WP26|CI平台/镜像扫描/SBOM/许可证/secret扫描、schema破坏检测、完整配置兼容矩阵、secret版本和日志采集/审计/OTel|本地make check和结构化日志存在；SDK故障映射、轮转/spool、生产安全与性能故障验收仍缺；必要安全项与最高优先级链路同步建设|
|R18|上线前按链推进|§4/28.18/28.20；WP16,WP18,WP25|第二第三条真实EVM链、独立硬分叉/Gas/费用/owner与缓存隔离验证、版本升级目录|WP18仍NOT_STARTED；可描述多个chain_key不等于真实多链上线；每条链单独数据ready、图ready和SLO|
|R19|中，开放API前|§15/28.18/28.19/28.28；WP16,WP17,WP26|多租户额度、内部/任意仿真隔离、取消资源回收、业务成功率SLO/压测和完整可观测性|当前共享认证/限流/single-flight/deadline和基础指标已实现；多租户/费用/执行质量及规模化故障容量未验收|
|R20|低|§16/28.21/28.23；WP19,WP21|持续分析调度、其他协议事实、持久DLQ/raw回放、ClickHouse事实/版本表/shadow发布/FINAL查询、迟到和decoder回补|[RunJournalJob](chain-parser-go/internal/analysis/job.go)已实现只读Kafka有限Job和持久cursor；缺项是完整分析服务与发布，不是Kafka消费从零开发；保持不阻塞报价|
|R21|低|§13/28.24；WP22|RFQ/外部来源竞争、多求解器、适用模型上界和收益分布|WP22仍NOT_STARTED；待本地数学、执行和成本口径稳定后做，不能用收益目标绕过正确性|

R06后续版本核对：已只读取得官方balancer-deployments固定b55dc2b8a46bce8f27aed5a3b03f719f662f860a的20230411-managed-pool-v2完整build-info，按原solc0.7.1/optimizer140及独立库job9999精确重编译；原Factory在正常EIP170限制下创建24561-byte pool，并用24577-byte控制合约验证限制有效。该部署实验仍使用构造facade，未完成原Vault/ERC20初始化与交换闭环，也未取得主网eth_getCode。该实际发布版的常规swap不走旧swap-share BPT铸费路径，需独立版本建模预铸BPT/virtual及actual supply、非零AUM与协议分成；旧e281ff40版本的拒绝不解除，不能用新版本冒充旧费用语义。下一阶段依此补实际发布版本与非零费用能力，仍属R06/WP11未完成项。

本轮按用户要求先补R06中的Curve Crypto，再补R05的Solana高级能力与R08的V4/Infinity扩展，逐个实际版本推进完整状态、解析、精确计算和进程验证。R01真实历史/部署基准、R02数据证据闭环及R03/R04真实流/恢复同步保留为验收依赖，R10独立仿真可并行。R09新增家族排在已有家族补齐之后；R20/R21保持低优先级。任何面向生产的放量都必须满足R16–R19，不能因其排列靠后跳过。

历史核对修正（2026-10-29）：WP19已有只读Kafka分析Job和持久cursor/失败checkpoint；WP08已有Rust owner bootstrap与router无KV恢复，缺的是对象存储/公共manifest/长期恢复；Raydium CLMM动态费、扩展bitmap和非零seed已局部完成，不再列为整体未开发。历史核对证据见[代码与剩余范围审计](integration/artifacts/local/remaining-features-audit.json)；2026-11-01的NG增量、源码与构建绑定见[NG限定范围验收](integration/artifacts/local/curve-ng-acceptance.json)。
