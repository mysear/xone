# Xone 开发进度与完成情况

更新：2026-11-13。设计基线：[XONE_SYSTEM_DESIGN.md](XONE_SYSTEM_DESIGN.md) v2.3。

本文件只记录项目进度、已完成范围、剩余功能及验证结果。后续进度只更新本文件；测试原始数据、JSON报告和校验规则由集成工具维护，不再嵌入进度文档。

状态：`PARTIAL_LOCAL`为部分实现并完成本地验证，`LOCAL_SCOPE_VERIFIED`为注明范围通过本地验收，`NOT_STARTED`为未开始；只有完整设计验收通过才可记为`ACCEPTED`。目前原26工作包完整验收仍为0，生产准入与VERIFIED报价关闭。

优先级：区块解析和精确兑换量计算优先，分析功能低优先级。三服务各自独立，公共协议位于common。

导航：[本轮开发](#active-work) · [工作包](#work-packages) · [协议覆盖](#protocol-coverage) · [逐项功能](#design-traceability) · [已验证批次](#batch-history) · [剩余功能及优先级](#remaining-priorities)

<a id="active-work"></a>

## 本轮开发

|工作项|设计条款 / 工作包|状态|已完成|剩余 / 验证|
|---|---|---|---|---|
|链/DEX解析、路由与兑换量计算收尾|§3/7/8/12/13；[计算收尾计划](#plan-2026-11-08-quote-completion)|IN_PROGRESS|新增最多8路拆单、独立池网格动态规划、共享状态有界搜索、浅池候选与预算报告；Velodrome V2独立版本和216原数学/8序列24步已实现|阶段46–48补齐旧LM、Slipstream及LFJ独立ExactOut注明范围；阶段49–53补齐限定DLMM MM/OnlyY/奖励/订单/Token2022/bitmap、Pump独立ExactOut及PumpSwap/毕业迁移；阶段57–59完成卖出目标及前端账户准备/包装，阶段60已补临时WSOL解包/余款退款，阶段61补限定Token2022收款ATA/withheld联验；策略交叉、其他协议组合/净链费用和第二批链仍有缺口，不宣称全链/全DEX完成|
|R01–R04真实来源、持续采集与范围恢复|§7/8/18/27.3/28；[11项计划](#plan-2026-11-06-r01-r04)|IN_PROGRESS|三服务当前全check及167 Rust定向/174 EVM race、普通23检查/8恢复通过；13 EVM+3 SBF源码精确重建，新Polygon/Arbitrum原VM及normal收款一致|动态目录、同KV恢复、实际PG/Kafka与normal流已接并验；全目录可信历史bank/来源独立性/生产资格仍待闭合，参见[当前核对报告](integration/artifacts/local/r01-r04-current-final-audit.json)|
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
|velodrome-v2-volatile|velodrome-v2-volatile/b3065d8b-1|EVM|3,7.3,11.3,12.2,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP18,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|固定源码标准Token/factory fee≤300/10000；Go固定块解析、原Solidity数学与后余额、共享状态及正常PG/Kafka双副本限定验证；完整Pool.swap/Token行为、OP部署与历史资格未完成|
|velodrome-v2-stable|velodrome-v2-stable/b3065d8b-1|EVM|3,7.3,11.3,12.2,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP18,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|独立_f舍入语义，保留Aerodrome旧_k分支；216原数学/8序列24步及30舍入控制，factory暂停/reorg/恢复通过；不将固定标签当实际OP部署证明|
|lfj-lb-v21|lfj-lb/v2.1.0-fee-oracle-1|EVM|3,7.3,11.3,12.2,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP18,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|独立field31、完整有界bin集/份额、动态费/协议费/Oracle；96原ExactIn/19API38原swap及10负控通过；阶段48新增独立目标输入与显式额外到账，216完整/72拒绝/18连续及181API原Pair/24额外到账/9负控同源码通过；标准Token/同步余额/外部funds限定，真实Avalanche部署资格/公开执行与链费用仍未闭合|
|curve-plain2-basic|curve-plain2-basic/factory-5|EVM|4,7.3,11.3,12.2,12.11|WP02,WP04,WP05,WP11,WP12,WP13|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|仅factory Plain2Basic Vyper0.3.1两币/固定rate_multipliers；其他plain/meta/crypto/NG、动态rate/rebase未支持；合成RPC与独立参考已验证，真实链历史块及收款差分仍未完成|
|balancer-v2-stable2|balancer-v2-stable2/stable-1|EVM|4,7.3,11.3,12.2,12.11|WP02,WP04,WP05,WP11,WP12,WP13|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|仅历史V2非composable两币StablePool，无BPT/rate-provider；Weighted双币等权另列独立profile；composable/V3及执行未支持；合成RPC与独立参考已验证，真实链历史块及收款差分仍未完成|
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
|meteora-dlmm|1532e540独立MM/InputOnly、OnlyY、bitmap、rewards、orders、token2022版本|Solana|4,7.3,8,11.3,12.2,12.10,12.11,12.14,13|WP02,WP03,WP05,WP09,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|阶段49..51历史限定验收保留；阶段52新增扩展bitmap、双奖励、三层订单ExactIn、X侧Token2022转账费/epoch/withheld，408原银行与完整账本复验通过，33检查/56Rust/45实际计划限定验收通过；高级策略交叉/订单ExactOut/任意Token扩展/host/奖励领取/订单管理、可信银行及公开执行仍缺|
|pump|pump-bonding/7a19affaa2c6-sol-1；独立migration版本|Solana|4,7.3,11.3,12.2,12.6,12.10,12.11,12.14|WP02,WP03,WP05,WP09,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|阶段51classic/SOL原银行与正常进程历史验收保留；阶段52独立buy目标与sell最小输入/显式盈余、迁移专用库存和已迁移清零状态、原migrate创建账户交接及重复幂等已验；最终限定验收通过；非SOL/Token2022/返还奖励/动态用户账户/自动发现/公开执行仍缺|
|pump-amm-classic-wsol|pump-amm/6a7ad34596d8-classic-wsol-1|Solana|4,7.3,12.6,12.10,12.11,12.14|WP02,WP03,WP05,WP09,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|独立canonical AMM/真实vault；classic+backedWSOL v1 sell/buy_exact_quote_in/buy目标，27原案例/完整分账及迁移创建的真实池后续swap复验通过；最终同源限定验收通过；v2/Token2022资产/非WSOL/高级费用与共享多池/自动发现/公开构建仍缺|
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
|balancer-v2-managed-aum|balancer-v2/20230411-managed-aum-3|EVM|7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|实际20230411原Factory/Vault正常部署、2–50币完整注册/AUM/实际钱包状态已接入；单池、四池目录及跨物理池共享Vault历史限定验收已完成（T34.8/9），新源码需重新冻结验证；旧版本继续拒绝活动schedule。BPT路由、rates/mixed、启用breaker、可信链/公开执行及其他版本仍未完成|
|balancer-v2-managed-scheduled|balancer-v2/20230411-managed-scheduled-1|EVM|7.3,11.3,12.2,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|独立field40/raw整数clock、2/3币单池及两个三币物理池混合/双活动M1–M4已限定通过；234单池/144多池原状态、实际API两路、原VM真实余额/连续全后状态/新冻结全检与审查一致。ONE±2和固定SWAP/YIELD构造ID按原语义保留，旧aum-3不提升。全部2–50参数/能力组合、BPT/rates/breaker、可信链与public执行仍未完成|
|balancer-v2-composable-regular|balancer-v2-composable-regular/no-rates-1|EVM|7.3,11.3,12.2,12.9,12.14|WP02,WP03,WP04,WP05,WP11,WP12,WP13|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|仅无rate provider的underlying间swap，完整余额含BPT但路由排除BPT；BPT数学内核有参考，完整待结费用/实际供应量/缓存刷新未完成|
|orca-whirlpools-adaptive|orca-whirlpools/cc683ec9-spl-adaptive-1|Solana|7.3,8.12,11.3,12.2,12.5,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP09,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|完整初始化Oracle/独立fee tier PDA、3fixed arrays、经典SPL和未初始化reward的独立版本；完整source/bank/wire及raw/fee/Oracle/tick后状态已实现；正常进程本地验收通过；initialized rewards/dynamic arrays/Token2022/SBF与真实部署未完成|
|orca-whirlpools-adaptive-rewards|orca-whirlpools/cc683ec9-spl-adaptive-rewards-1|Solana|7.3,8.12,11.3,12.2,12.5,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP09,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|固定cc683ec9 classic SPL/三fixed arrays/完整Oracle与初始化奖励增长、跨tick与完整后状态，正常RPC/双副本限定验收通过；dynamic arrays、Token2022、reward领取/position结算/SBF/CPI及真实历史仍未完成|
|orca-whirlpools-adaptive-dynamic|orca-whirlpools/cc683ec9-spl-adaptive-dynamic-1|Solana|7.3,8.12,11.3,12.2,12.5,12.9,12.11,12.14|WP02,WP03,WP04,WP05,WP09,WP11,WP12,WP13,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|classic SPL/完整Oracle/三连续fixed或dynamic混合数组及奖励，正常解析/数学/双副本服务限定验收通过；补充/稀疏数组、Token2022、领取/position/SBF/CPI及真实部署历史仍未完成|
|orca-whirlpools-rewards|orca-whirlpools/cc683ec9-spl-rewards-1|Solana|7.3,11.3,12.2,12.9,12.14|WP02,WP03,WP05,WP11,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|经典SPL/静态费，三奖励槽及tick增长；adaptive/Token2022/奖励领取和组合高级状态未完成|
|raydium-clmm-rewards|raydium-clmm/6d7a0f82-spl-rewards-1|Solana|7.3,11.3,12.2,12.9,12.14|WP02,WP03,WP05,WP11,WP20,WP25|PARTIAL_LOCAL|PARTIAL_LOCAL|NOT_IMPLEMENTED|经典SPL/静态输入费/default seed/基础bitmap奖励版；订单/Token2022/动态费和奖励组合未完成|
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

|11.3.managed-v2-published-state|WP02,WP04,WP05,WP11,WP25|PARTIAL_LOCAL|实际发布版独立状态/profile/config、sourceH、JSON/wire/clone与uint32原storage边界已接入；四副本独立C++ 2895字节通过|派生execution_context与canonical隔离、实际Vault钱包余额完整保留正在补齐；最终冻结与双审未完成|[wire初验](integration/artifacts/local/managed-v2-aum-wire-check.log)|
|7.3.managed-v2-fixed-bank|WP03,WP04,WP05,WP11|PARTIAL_LOCAL|固定blockHash/原runtime及links/immutables、真实Factory/Provider/Vault/collector getter与完整供应量/AUM核对；48原snapshot/81投影对照已接入|更多真实链版本/来源证明、实际余额完整保留及最终冻结未完成|[原bank精确再生](integration/artifacts/local/managed-v2-aum-reference-check.log)|
|12.14.managed-v2-original-replay|WP03,WP11,WP13,WP25|PARTIAL_LOCAL|正常EIP170原部署27case/292交易/2285对象，原swap与非零AUM/协议分成/零重复/uint32回退；81类原篡改普通与优化模式拒绝通过|普通服务只覆盖当前日期双币样本；其他未来clock状态仍为本地parser/math参考，不能放宽未来头准入|[普通保护](integration/artifacts/local/managed-v2-aum-guards.log)<br>[优化保护](integration/artifacts/local/managed-v2-aum-guards-optimized.log)|
|12.9.managed-v2-derived-context|WP02,WP11,WP12,WP13|IN_PROGRESS|固定H共享池连续重算通过；独立SPEC发现未来time同clock下一步拒绝及高度回卷后完整context丢失|显式派生执行高度/时间、sourceH不变、canonical/wire拒绝派生状态正在红绿补齐；不能据旧通过宣称完整连续重算|[本批任务](#plan-2026-11-03-managed-v2-aum)|
|23.managed-v2-normal-service|WP02,WP03,WP05,WP11,WP12,WP13,WP25|IN_PROGRESS|正常RPC/PG/Kafka双router首轮6原来源状态/4原swap金额/10来源故障撤销恢复/重启与VERIFIED-build拒绝共15检查通过，live C++ wire一致|后续源码变更使首轮为历史记录；完整钱包和派生上下文收口后需最终冻结全量/独立双审，原生选中计划VM重放未完成|[首轮服务记录](integration/artifacts/local/managed-v2-aum-three-service-smoke.json)|
|12.11.managed-v2-distinct-pause|WP02,WP03,WP04,WP05,WP11,WP13,WP25|IN_PROGRESS|原实际Vault暂停交易后Pool公开AUM collection仍成功，证实单一OR标志不符合原合约；按Pool/Vault各自有效flag和窗口独立建模，新增公开状态版本3|原暂停三case、固定块解析/数学完整后状态与正常服务边界正在补齐，最终同源全检和独立双审尚未完成|[原执行发现](integration/artifacts/local/managed-v2-aum-vault-pause-native-probe.log)|

|7.3.managed-v2-current-catalog|WP02,WP03,WP04,WP05,WP11,WP25|LOCAL_SCOPE_VERIFIED|[新范围最终同源验收](integration/artifacts/local/managed-v2-multi-acceptance.json)；真实2026-01-01/01-31的2/3/5/50币四池共同H目录、73代码身份/9800原getter、原4swap/4非零AUM，Go40完整Pool与Rust8原后状态通过；普通双副本10bank/14计划通过|新冻结全检/质量/完整门禁已通过；本地参考不提供真实链trust|[原目录](integration/test/contracts/managed-v2-aum-catalog.json)<br>[四池进程](integration/artifacts/local/managed-v2-catalog-three-service-smoke.json)|
|11.3.managed-v2-shared-vault-facts|WP02,WP03,WP05,WP11|LOCAL_SCOPE_VERIFIED|[新范围最终同源验收](integration/artifacts/local/managed-v2-multi-acceptance.json)；同sourceH角色/代码/参数/Provider外部费率事实、全局钱包与known注册cash合计/重复冲突校验，Go及两Rust公共domain统一；knownunsupported与事实冲突区别，完整银行保全|新范围独立质量与完整冻结验收已通过；未知Vault注册不冒称全目录覆盖|[共享原银行](integration/test/contracts/managed-v2-shared-vault.json)<br>[红绿回归](integration/artifacts/local/managed-v2-shared-role-cash-go.log)|
|12.9.managed-v2-shared-wallet-transition|WP02,WP03,WP11,WP12,WP13|LOCAL_SCOPE_VERIFIED|[新范围最终同源验收](integration/artifacts/local/managed-v2-multi-acceptance.json)；原4次A/B/A/B交易两Pool完整后状态、共享global钱包传播/registered账本保留、Somectx与sourceH、整path失败原子回滚、capacity探测与整数optimizer回归通过|普通BPT/join/exit/rate/mixed能力和通用collection传播仍未实现；生产性能另验|[路由原生回归](integration/artifacts/local/managed-v2-shared-funded-scope-regressions.log)|
|13.managed-v2-executable-scope|WP02,WP09,WP11,WP12,WP13,WP16|LOCAL_SCOPE_VERIFIED|[新范围最终同源验收](integration/artifacts/local/managed-v2-multi-acceptance.json)；Graph/API显式排除不支持的相关Vault组并保留完整来源ScopeHash，无关V2及weighted-only可报价；Snapshot/Replica/HTTP13回归通过|新范围全量/独立质量门禁已通过；不将scope限制改写为原设计功能删除|[范围回归](integration/artifacts/local/managed-v2-shared-funded-scope-regressions.log)|
|12.14.managed-v2-real-selected-split|WP03,WP11,WP12,WP13,WP14,WP25|LOCAL_SCOPE_VERIFIED|[新范围最终同源验收](integration/artifacts/local/managed-v2-multi-acceptance.json)；原signed history恢复source后按真实API路线逐步原Vault.swap，两个actor真实选择2物理池、输入5e21/收款7874328412914279650444，与报价一致；7source/16实际计划/4故障恢复进程通过|新冻结全检/质量/完整门禁已通过；仍无trusted live chain/public executor/VERIFIED-build|[共享真实进程](integration/artifacts/local/managed-v2-shared-three-service-smoke.json)|

|11.3.managed-v2-scheduled-state|WP02,WP11,WP25|LOCAL_SCOPE_VERIFIED|独立scheduled profile及field40、纯整数raw decoder与canonical/derived隔离；独立C++四副本wire通过，ONE±2原getter保持未归一化|M1–M4多物理池原组合、更多token/能力矩阵及完整工作包验收未完成|[field40 wire](integration/artifacts/local/managed-v2-scheduled-wire-check.log)|
|7.3.managed-v2-scheduled-source|WP03,WP04,WP05,WP11,WP25|LOCAL_SCOPE_VERIFIED|234原VM状态固定H完整RPC/双Rust域/完整field40 wire及8来源/4构造ID联合篡改拒绝；来源代码/全部immutable/Factory/WETH/必需getter/身份唯一已审查修复|真实链trust、多物理池活动schedule原source及原设计完整验收仍缺|[当前原来源与构造拒绝回归](integration/artifacts/local/managed-v2-scheduled-parser-test.log)|
|12.9.managed-v2-scheduled-transition|WP11,WP12,WP13|LOCAL_SCOPE_VERIFIED|234时钟视图/41检查点、53原swap实际收款及全部后状态、2非零AUM collection后状态精确匹配；未来共享Vault clock定向通过|M1–M4多物理池原VM组合及完整能力共享DAG仍未完成|[当前原数学与完整后状态](integration/artifacts/local/managed-v2-scheduled-math-test.log)|
|12.14.managed-v2-scheduled-original|WP03,WP11,WP13,WP25|LOCAL_SCOPE_VERIFIED|原53swap/234state/41checkpoint的四独立副本与单物理池55来源/127实际API、53不同原签名交易VM全部真实余额/后观察匹配；干净目录仅注册输入精确再生通过|多物理池活动schedule原source/组合执行/拆单及原26工作包全验收仍未完成|[当前干净目录原VM再生](integration/artifacts/local/managed-v2-scheduled-reference-check.log)|

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
|14.wallet-lifecycle|WP05,WP09,WP14,WP15,WP16,WP25|PARTIAL_LOCAL|已有unsigned/独立SBF模板接Wallet Standard、显式发送与实际收付回执/恢复|默认关闭，按链启用；本批仅受控Solana模板，不等于全链全协议生产资格/完整多租户或多区执行验收|[验证记录](integration/artifacts/local/core-wallet-lifecycle-validation-20261113.json)|

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
|35|[Managed V2 BPT有限GIVEN_IN路由与服务](#plan-2026-11-06-managed-v2-bpt-finite)|LOCAL_SCOPE_VERIFIED|[最后同源有限验收](integration/artifacts/local/managed-v2-bpt-local-acceptance.json)|
|36|[Managed V2已启用circuit breaker](#plan-2026-11-06-managed-v2-breaker)|LOCAL_SCOPE_VERIFIED|[限定最终验收](integration/artifacts/local/managed-v2-breaker-local-acceptance.json)；5任务/6功能映射与同freeze完整串行闭合|
|37|[R01–R04真实来源、持续采集与范围恢复](#plan-2026-11-06-r01-r04)|IN_PROGRESS|逐任务核对与实现中；[环境探针](integration/artifacts/local/r01-r04-environment-probe.json)|
|38|[链与DEX解析、路由与兑换量计算收尾](#plan-2026-11-08-quote-completion)|IN_PROGRESS|C01/C02/C03及LFJ有限范围已通过[阶段冻结](integration/artifacts/local/core-quote-q1-local-463896a2b08e.json)；R07等计算缺口继续开发，整体请求未全部完成|
|39|[Slipstream调用方与Oracle费用上下文](#plan-2026-11-08-slipstream-context)|LOCAL_SCOPE_VERIFIED|三服务全check和5项正常进程范围同源冻结通过；13实际API原交换及8负控已通过；Polygon/Arbitrum修复验收输出目录后复验通过，未宣称整体剩余计算完成|
|40|[Pancake V3 LM主目录接入](#plan-2026-11-09-pancake-lm)|LOCAL_SCOPE_VERIFIED|独立field33/完整解析与账本/共享路由、80原完整swap/12序列步、13实际API原swap/9负控、六进程及同源分阶段检查通过；[最终报告](integration/artifacts/local/core-lm-local-q1-20261109.json)|
|41|[实际BSC第四代LM与惰性迁移](#plan-2026-11-09-bsc4-lm)|LOCAL_SCOPE_VERIFIED|独立field34/实际当前与前三代原程序；64原swap/15序列步、正常13API/13原swap/6次实际迁移/13负控、Go全检及双Rust全目标lint/定向通过；[最终报告](integration/artifacts/local/core-bsc4-local-q1-20261109.json)|
|42|[状态驱动的路由准入刷新](#plan-2026-11-09-graph-admission)|LOCAL_SCOPE_VERIFIED|3池准入反例修复，空候选水位推进/撤权与恢复；17副本/12API和相关全target lint通过，正常13API原程序/6迁移回归通过；[最终报告](integration/artifacts/local/core-graph-admission-local-20261109.json)|
|43|[候选范围与不可用池披露](#plan-2026-11-09-candidate-scope)|LOCAL_SCOPE_VERIFIED|修复left6/right4候选披露；52 Router/11解析域回归、相关全target lint和正常13API原程序/6迁移回归通过；[最终报告](integration/artifacts/local/core-candidate-scope-local-20261109.json)|
|44|[ExactOut独立输入目标与原Router基准](#plan-2026-11-09-exact-out)|LOCAL_SCOPE_VERIFIED|独立V2完整状态ExactOut数学/反向路径/有序拆单/API；整单探测遗漏反例先红后绿；90原交易/162 Pair转换、39网格/38可行、45实际API/61原交易/7负控、84 Rust执行及Router全目标lint通过；[当前限定验收](integration/artifacts/local/core-exact-out-local-q1-20261109.json)|
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

|45|[实际BSC第四代LM独立ExactOut](#plan-2026-11-10-bsc4-exact-out)|LOCAL_SCOPE_VERIFIED|独立负amountSpecified与完整LM账本/单池路由/API；48完整/16部分拒绝/4无效/15连续原执行，41实际API/41原swap/31迁移/13负控；97 Rust执行/Router全目标lint与V2正常回归通过；[限定验收](integration/artifacts/local/core-bsc4-exact-out-local-q1-20261110.json)|

|46|[固定98684794旧LM独立ExactOut](#plan-2026-11-10-legacy-lm-exact-out)|LOCAL_SCOPE_VERIFIED|原80完整/20部分拒绝/4无效/12连续，49实际API与原兑换、9负控；20检查/106 Rust及Router全目标lint、BSC4和V2正常回归通过；[限定验收](integration/artifacts/local/core-legacy-lm-exact-out-local-q1-20261110.json)|

|47|[Slipstream调用者费率独立ExactOut](#plan-2026-11-10-slipstream-exact-out)|LOCAL_SCOPE_VERIFIED|90完整/18部分拒绝/12连续原步骤，49实际API与原兑换/9负控、121 Rust执行/24检查及5项正常服务回归通过；候选截断标志红绿修复；[限定验收](integration/artifacts/local/core-slipstream-exact-out-local-q1-20261110.json)|

|48|[LFJ最小输入目标与额外输出](#plan-2026-11-10-lfj-exact-out)|LOCAL_SCOPE_VERIFIED|216完整/72部分拒绝/60额外输出/18连续原步骤，181实际API/原Pair和24额外到账/9负控；136 Rust执行/27检查/7项正常回归同源码通过；[限定验收](integration/artifacts/local/core-lfj-exact-out-local-q1-20261110.json)|

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
|T34.12|实际20230411发布版BPT join/exit swap路由与完整供给/钱包/控制语义：先固化原执行，再独立版本/状态/精确计算/逻辑BPT边/真实API执行逐项接入|WP02,WP03,WP04,WP05,WP11,WP12,WP13,WP14,WP25|LOCAL_SCOPE_VERIFIED|[最终有限同源验收](integration/artifacts/local/managed-v2-bpt-local-acceptance.json)：独立BPT状态/wire、固定H解析、原精确转换、Funds图/分配/HTTP与四场景进程已通过；141source/250API/30拒绝/24fault/4重启，97计划100原交易及逐步完整Pool，实际worker7模拟、24 helper双模式、全检/双审均同freeze。仅标准3underlying/两物理池有限GIVEN_IN范围；enabled breaker/rates/mixed/其他custody/ExactOut/真实链与公开执行不计完成|



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

T34.11.6 M1–M4当前有限范围最终闭环（2026-11-05）：源码冻结 `cb1ea485d9daae8d088d66b8e153963db0597d4ac75ab4d39d53fcb5519a509e`；四原bank仍22,999,112字节/c1a4094d...e79eb5，四独立Pool投影2,912,236字节/7667c768...9eef72。三服务/common所有make check和debug builds、229契约向量/8fixture及服务权限边界八命令均0，源码/五binary与进程输入一致。M1 source四类已报问题独立复审关闭；流程/原VM及新增实际报告mapper/强制Rust调用亦独立限定通过。最终72共同来源/CPP、52实际两路API、12故障撤销恢复/2重启、12独立plan/24新签名swap原ERC20钱包/事件与全部观察通过；52报价明确quote_links关联，不冒称52个fresh VM；每mode一ownedVM，每个不同plan snapshot/revert后完整source恢复。实际大额24步双方完整Pool/SourceH/execCtx本地连续模型已由M4强制运行normally-ignored Rust test1 passed；Python实际报告映射测试额外显式1 passed。普通/优化23辅助项各22通过+1依赖真实报告的默认跳过，该路径已由上述实际执行覆盖。仅434登记文件的干净目录原VM重新生成四份原bank逐字节一致且mtime未改。有限证据汇总见 [当前多池验证记录](integration/artifacts/local/managed-v2-scheduled-shared-local-validation.json)，[冻结构建验证](integration/artifacts/local/managed-v2-scheduled-shared-final-verification.json)，[实际进程与原执行](integration/artifacts/local/managed-v2-scheduled-shared-three-service-smoke.json)。T34.11仅升级LOCAL_SCOPE_VERIFIED（注明版本、2/3币及两三币物理池标准underlying范围）；全部2–50参数、BPT/rate/mixed/breaker组合、生产原子执行、真实链与原26完整WP仍未完成，public VERIFIED/build维持关闭。

T34.11.6跨物理池原执行子计划（与下方原步骤6一一对应；M1–M4已按以下有限2池/3币范围通过本地验证，不是完整设计验收）：

- M1：独立 `managed-v2-scheduled-shared` 原参考沿用固定原Factory/Vault/ERC20与prefix28；创建共享underlying的第二物理池，分别记录活动权重/费率及静态/活动混合profile的共同H getter、slot9/mapping11、角色/全局钱包/各池BPT账；价格期望只取原交易和真实钱包变化。保留当前单池53原case作为独立历史闭包，不把新多池数据混入旧分母。
- M2：两池完整来源投影和Go固定块bank读取、两Rust domain/field40契约、共同Vault的代码/角色/provider/实际钱包一致性；缺任一池依赖时整组失败，其他Vault和Solana不受影响。
- M3：原A/B/A/B及显式collection的共享钱包、各池注册资金/BPT和未来clock全后状态差分；正常API实际选择两条路线/整数拆单后，由原VM按返回顺序执行并核对真实余额，禁止用本地数学生成期望。
- M4：source→PG/Kafka→双router、多池来源故障撤销恢复与重启，重新完整check/普通和优化helpers/独立SPEC与quality，再按新冻结升级该限定范围。该计划不开放BPT/rates/breaker、可信链或public VERIFIED/build，不将已有单池通过称为跨池完成。

T34.11单物理池当前限定范围最终闭环（2026-11-05）：最新冻结源码 `24e16da70c82d50c83b81ee654b392402cba3bb7f5498cd3797f96aabc69a550`；原bank `d7b6b...4501e` / 四Pool投影 `a67c9e...aa81` 与五binary、完整check、38 helper普通/优化、实际进程/原VM和独立审查在同一冻结一致。原始证据汇总见 [当前本地验证记录](integration/artifacts/local/managed-v2-scheduled-local-validation.json)，完整构建报告已另存为 [该范围构建验证](integration/artifacts/local/managed-v2-scheduled-final-verification.json)，实际127报价/53不同原签名交易真实余额及完整原观察见 [进程与原VM报告](integration/artifacts/local/managed-v2-scheduled-three-service-smoke.json)。普通与优化helper各38项，其中37通过、1项干净复制VM按默认开关跳过；该项随后以 `XONE_SCHEDULED_FULL_CHECK=1` 单独实际执行163.5秒通过，确认只复制登记依赖即可精确重放四份原bank且不改输入mtime。各独立审查的全部已报问题已关闭。四条逐项功能仅升级为注明的LOCAL_SCOPE_VERIFIED，T34.11整体继续IN_PROGRESS：原步骤6中的M1–M4跨物理池活动schedule原组合仍缺；BPT/rates/mixed/breaker、可信链和public VERIFIED/build等原设计余项不变，原26完整WP验收仍为0。

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

T34.12新原闭包阶段收口（2026-11-05，未最终验收）：raw `4cbf03...892f8`的byteexact原VM与独立C++wire均通过；四投影当前SHA256 `84307dc10b1a0232c324f9289502a161528e74098df250651832fb2d7d2fdbe4`，来源/槽10正常与合成Recovery边界完整Go差分及race全check通过。最后同一23项BPT helper清单普通286.502秒、优化219.950秒通过。源码审查全部已报告P1/P2（包括strict deadline与slot10）均关闭；生命周期测试小进程替换质量审查通过，无生产预算/语义更改，实际worker冷启动/协议覆盖另由已启动的原evm_execution_smoke执行，不以小进程单测冒称。strict四场景进程现settled14src40API6fault/重启、scheduled83src158API6fault/重启通过，controls正在开始；最后supervisor测试注释/source变化使本轮最终绑定仍只能历史，待统一最终冻结再跑。已将20份完成的红绿日志保全为 [阶段红绿证据](integration/artifacts/local/managed-v2-bpt-red-green-evidence.json)，不当最终同源验收。

T34.12实际worker与四场景进程阶段证据（2026-11-05，未最终验收）：实际 `evm_execution_smoke` 已12checks/7隔离模拟/2来源fault与显式reload/restart通过，真实worker binary/协议/执行结果覆盖明确，runtime与inputs均匹配当前 `managed-v2-bpt-freeze.json`，不以tiny生命周期单测代替；原公开VERIFIED/build继续关闭。strict BPT四场景已经settled14/40、scheduled83/158、controls25/22（12拒绝）、limits19/30（18拒绝）来源/实际API；总141支持bank/250成功报价/30明确拒绝、24来源fault/4router重启完成，正在全实际选中计划原VM重放。由于本轮startup source fingerprint早于最后supervisor测试与注释，已核实其runtime不匹配当前freeze，即使amount/native通过也只能历史FAILED；inputs一致不补救source不一致。最终canonical verify在当前freeze独立运行，早期已失配的canonical拥有进程组已停止并记录 [取消原因](integration/artifacts/local/managed-v2-bpt-superseded-verification.json)，不让旧进程覆盖最终报告。新视角只读质量审查正在进行；最后同冻结BPT全流程仍需fresh run。

T34.12最终质量负例P2补强（2026-11-05，进行中）：新视角质量审查复现smoke原 `code!=200` 会将500内部错误错记业务拒绝。已补 `validate_rejected_case`：响应只能含error，零输入仅400/INVALID_REQUEST；非零原revert/零output仅404/NO_ROUTE或400/NO_EXECUTABLE_ALLOCATION；500/503/504/429/无关INVALID_REQUEST或quoteId均拒绝。正式回归先红后普通6项1.048秒、优化6项1.094秒通过，历史30实际负例审计确为20*NO_ROUTE/6*NO_EXECUTABLE/4*zero INVALID_REQUEST，无500实际伪阳性。最后BPT清单因新增该回归变为24项，已重新启动普通/优化同清单；该改动使当下进程输入绑定仍不可最终接受，待质量复核关闭后固定24项freeze再跑。原production/source fingerprint未因这个tool-only修复改变，canonical全检可继续；inputs绑定必须重新核验，不能绕过。

T34.12最终24项与真实worker复验（2026-11-05，进行中）：最后同一24项helper普通174.623秒、优化296.916秒通过，来源/routing/spec及质量已报问题均关闭。固定24输入后BPT四场景已完成全部141source/250API/30拒绝/24故障/4重启，当前正在100实际选中原签名交易后大报告重校验及完整Pool差分，启动runtime/inputs与freeze相同。canonical全检已Go/Rustparser通过、router workspace测试和doc tests通过，contract-test/最终build仍运行。实际worker复验在最后router重启后模拟返回SIMULATION_TIMEOUT，诊断30秒预算内失败且未发布成功，报告已保留 [失败现场](integration/artifacts/local/managed-v2-bpt-worker-final-input24-timeout-history.json)；不以旧pass替代、不提高预算，待当前重计算结束后在同一binary/输入串行复验。最后目标保持T34.12与原全部设计功能，尚未验收。

T34.12最终串行验证恢复：前次执行句柄已消失且ps确认无验证进程，磁盘仍为未完成的RUNNING，不视为活跃或成功。已重新核对当前source、五binary和13,013项输入与最后freeze逐项完全一致，将中断验证保全并明确标记INTERRUPTED。当前由持久脚本按verify→原reference/wire→同24项普通/优化helpers→实际worker→BPT四场景顺序运行，阶段命令/退出码与日志保存在 [串行验证](integration/artifacts/local/managed-v2-bpt-final-serial-run.json)，避免大额原VM和worker冷启动并发；每阶段前后仍比原freeze，不放宽预算或恢复旧失败报告。当前串行verification的Go/Rust parser完整check均已通过，router完整check进行中；之后还需原reference/wire、同24项helpers、实际worker和BPT四场景在本次freeze重新通过。T34.12继续IN_PROGRESS，原26完整WP仍0。

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

本轮当前已实现单物理池范围历史最终验收通过（2026-11-04；后续T34.8/T34.9代码变动后不作为新范围通过证据，完整源码/报告已归档[历史闭包](integration/artifacts/local/accepted/2026-11-04-managed-v2-single-pool/MANIFEST.json)）：[同源验收报告](integration/artifacts/local/managed-v2-aum-acceptance.json)绑定三服务/common完整check、五个二进制、208份契约/8份fixture、当前全部原输入、独立SPEC与quality无遗留审查。原27case/292tx/2285对象、旧原稿完整两编译job的精确临时再生、81篡改普通/优化拒绝；独立39字段C++3023字节四副本/15自测；普通/优化各49辅助回归均通过。实际source→PG/Kafka→双router的15原来源状态、8正向报价、6暂停状态拒绝但保持ready、5Recovery有效零费率来源及退出恢复常规费、10来源故障撤销/新generation恢复、无关Solana报价及重启、VERIFIED/build拒绝，共18检查通过。选中4份API单跳计划由独立原Vault在新VM中重放前28真实签名交易，核对真实扣款/收款余额、完整getter/账户、原header/receipt。控制enable/disable只计真实原来源观察，不冒称已实现控制数学内核；独立执行仍是本地参考，不是可信链/生产执行器。

T34.8/T34.9新范围最终同源验收通过（2026-11-04，Asia/Shanghai）：[最终报告](integration/artifacts/local/managed-v2-multi-acceptance.json)14阶段全部PASSED，绑定当前源码/五二进制/全部参考输入、独立SPEC与quality及真实进程原VM证据；106辅助回归普通/优化全部通过。四池10银行/14计划/4故障与共享两池7银行/16计划/4故障通过，两个actor各2物理池拆单及实际收款7874328412914279650444一致；逐来源parser必读RPC/代码身份覆盖、API edge token与实际步骤绑定亦校验。原51/214等更早计数以各自历史快照为准；新接受闭包已保全到[历史验收](integration/artifacts/local/accepted/2026-11-04-managed-v2-catalog-shared/MANIFEST.json)。

T34仍未全部完成，T34.8/T34.9仅按上述范围记LOCAL_SCOPE_VERIFIED。T34.11活动权重/费用已按上述标准underlying、2/3币及两个三币物理池有限范围记LOCAL_SCOPE_VERIFIED，BPT路由/启用breaker/混合会计与rates/通用collection传播、其他Managed版本、可信链/生产要求均保留，不因新局部验收减少原设计。T33及更早报告自后续源码/输入变更起为历史验收；原26工作包完整验收仍为0，总体目标仍为全部设计功能完成。

<a id="plan-2026-11-06-managed-v2-bpt-finite"></a>

## Managed V2 BPT有限GIVEN_IN：任务完成情况

设计对应原§7.3、11.3、12.2、12.9–12.14、23及WP02–05、WP09、WP11–14、WP16、WP25。固定20230411正常原Factory/Vault/ERC20与标准18/6/8三underlying、两真实物理池的本地历史范围；不改变系统设计基线或完整工作包门槛。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|B1|固定原版本与完整执行来源：145bank/116尝试，金额来自真实原钱包/事件；constructor/runtime/immutable、raw9/10/11/13及原signed history资格|WP03,WP11,WP25|LOCAL_SCOPE_VERIFIED|[最终有限验收](integration/artifacts/local/managed-v2-bpt-local-acceptance.json)|
|B2|独立BPT profile/common41/Go与两Rust、required/unknown/wire/clone，286 Pool原固定H差分及slot10 recovery有效费率、来源错误整批拒绝|WP02,WP04,WP05,WP11,WP25|LOCAL_SCOPE_VERIFIED|[最终有限验收](integration/artifacts/local/managed-v2-bpt-local-acceptance.json)|
|B3|原单币整数join/exit、先AUM后转预铸BPT，供应/注册/全局钱包/别名/sourceH与derived隔离、完整后Pool|WP11,WP13,WP25|LOCAL_SCOPE_VERIFIED|[最终有限验收](integration/artifacts/local/managed-v2-bpt-local-acceptance.json)|
|B4|唯一物理池逻辑BPT边，sender/recipient贯穿search/optimizer/HTTP；连续/拆单整数重算、known跨V2钱包修复与失败整path原子性|WP11,WP12,WP13,WP14|LOCAL_SCOPE_VERIFIED|[最终有限验收](integration/artifacts/local/managed-v2-bpt-local-acceptance.json)|
|B5|source→PG/Kafka→双router/141bankCPP、250报价/30业务拒绝、24fault/4重启；97实际计划100原交易与逐步完整after；同freeze全检/24双模式helper/真实worker及两阶段独立审查|WP03,WP04,WP05,WP09,WP11,WP12,WP13,WP14,WP16,WP25|LOCAL_SCOPE_VERIFIED|[最终有限验收](integration/artifacts/local/managed-v2-bpt-local-acceptance.json)|

最后闭合结果：串行五阶段命令全部exit0，canonical8完整命令/source_unchanged/five binary通过；原参考与C++wire byteexact、同24项普通215.075秒/优化198.819秒、真实worker12checks/7隔离模拟/2sourcefault/重载重启，随后BPT四场景全部通过。实际250输入逐项link承诺对应97原VM计划100交易（含一项两路四步），原after重验及每步双方完整Pool差分通过；最后独立核对source/binary/input/日志摘要/审查源均一致。原参考SHA256 `4cbf03d19a773ba374d8df87e4e3231798f042f576addeb5532300af0bb892f8`，四投影 `84307dc10b1a0232c324f9289502a161528e74098df250651832fb2d7d2fdbe4`；[串行原记录](integration/artifacts/local/managed-v2-bpt-final-serial-run.json)、[独立审查](integration/artifacts/local/managed-v2-bpt-review.json)、[红绿记录](integration/artifacts/local/managed-v2-bpt-red-green-evidence.json)。

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

本轮方案对应检查：进度解析15项回归普通/优化双模式均通过；26个工作包/198条设计功能/56个协议条目及当前CB1–CB5五任务已逐项读取，breaker六条设计映射的源码路径和实际测试定义均存在，无失配。结构检查保存在[方案对应报告](integration/artifacts/local/managed-v2-breaker-progress-consistency.json)，只证明文档/源码/测试映射，不替代功能验收。本轮Go和Rust parser完整check均已通过（含随后独立contract-test）；dex-router全检中，统一流水线尚未结束。

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

文档回填后最终复验：17项progress普通/优化与20项门禁回归通过，结构摘要仍与freeze及acceptance一致；精确5任务/6设计映射、全部绑定路径/实际测试定义、26工作包/56协议条目、21剩余事项覆盖全部工作包均校验通过。[最终进度一致性报告](integration/artifacts/local/managed-v2-breaker-progress-consistency.json) passed，正式acceptance重新audit仍LOCAL_SCOPE_VERIFIED，git diff --check通过。回填中附加单项报告链接曾被既定evidence规则拦截，已仅修正文档为保留单项原绑定、批次统一引用最终报告，未改冻结工具或放宽校验。

实现约束：slot12依次为lower16、upper16、reference weight64、stored BPT price96、cached lower32、cached upper32；百分比16位字段以左移48解压。初次set时cached ratio来自未压缩的输入bound，cache miss来自压缩后还原bound，因此不能用重新计算的ratio硬校验合法原cache值；同weight用原缓存，不同weight按原方向重新pow，0边界独立关闭。管理输入BPT price=actualSupply×weight/raw balance，原_set先downscale，getter再upscale，6/8 decimals不可按18位币照搬。BPT join/exit改变供应，对未兑换token也须检查双边；所有成功/回退仍保留唯一物理Pool和source H。剩余rate/mixed/其他custody/self-funding/ExactOut/真实链/公开执行与生产完整要求不因本批删除。

<a id="plan-2026-11-06-r01-r04"></a>

## R01–R04：真实来源、持续采集与范围恢复（进行中）

范围严格对应R01–R04及原§7.1–7.3/7.9、§18、§28.5–7/10/20/22/26。既有第36批breaker的限定结论保留其冻结证据；本批任何新代码须重新验证，不能沿用旧指纹。除进度状态外，任务/设计/实现/测试/证据逐项登记在本文件。真实部署审核、共识/来源独立性及Yellowstone来源必须有真实证据；HTTP连通、配置hash、模型单测均不等于完成。

|任务|内容|工作包|状态|验证|
|---|---|---|---|---|
|R01.1|冻结逐chain/protocol/version部署目录，源码/发布二进制/代理实现或ProgramData升级身份与固定历史H逐项匹配|WP02,WP03,WP10,WP11,WP18,WP20|IN_PROGRESS|8链16部署目录保留；13项EVM源码/metadata与3个实际Solana ProgramData均已有精确重建，55/83/404项程序输入与原归档一致；可信历史状态和生产准入资格仍须逐项核验|
|R01.2|固定真实历史块/账户原始证据，独立状态/执行/收款差分及离线可重复回放；来源或版本缺失拒绝|WP02,WP03,WP10,WP11,WP18,WP20|IN_PROGRESS|原Ethereum证据保留；新Polygon H96900773与Arbitrum H522108799完整MPT/原VM→normal Parser/Router收款差分均一致；全目录可信历史bank/费用和生产执行资格尚未闭合|
|R02.1|Provider身份/能力/历史范围/错误分类/限流预算、独立故障域及双源head；WS通知与轮询/重连短缺口一致|WP04,WP05,WP07|IN_PROGRESS|normal provider身份/能力/HTTP分类/共享预算/WS与短缺口接线通过当前Go完整check/race；真实来源故障域和持续限流/历史矩阵不凭URL数推断|
|R02.2|可信头/交易根/receipt根与链规则逐项核对，限定并行Fetch/Decode staging按链有序原子提交|WP04,WP05,WP07|IN_PROGRESS|native V2 normal Adapter/Factory/history/settled-head及Coreth逻辑/物理storage消费已接，174项EVM race全过；承诺核验不替代可信checkpoint/共识证明|
|R02.3|原始数据不可变归档、解码异常持久DLQ/隔离和重放；工厂事件发现的当前状态准入|WP04,WP05,WP07|IN_PROGRESS|normal工厂目录、immutable archive/DLQ修复重放及owner责任已接；普通进程13项/845 raw/6 DLQ通过；证据流式清单和450真实请求计数另有37/43项边界回归，生产保留/吞吐仍有限|
|R03.1|Yellowstone真实订阅传输、账户/交易/meta/slot来源身份和序号，先耐久WAL后解码/消费；重连与容量失败封闭|WP04,WP05,WP07,WP20,WP24|IN_PROGRESS|normal tonic/WAL/manifest/owner已接，当前Rust完整check和真实本机PG/Kafka发布/撤销/重启/Fence通过；既有Agave实际Bank构建证据保留，真实同源流资格继续核验|
|R03.2|乱序账户/交易/metadata重建、版本化账户、slot/fork/skipped屏障、短缺口补洞；窗口不足HEAD_FIRST并登记history gap|WP04,WP05,WP07,WP20,WP24|IN_PROGRESS|版本化manifest、非零tail、fork/skipped/session屏障与HEAD_FIRST已接normal；当前PG/Kafka4恢复场景及定向/全Rust回归通过；真实历史bank账户完成性未自动成立|
|R04.1|认证EnsurePoolScope控制接口，显式预算/去重/版本隔离、依赖分片和失败撤销；完成当前scope才能ready|WP08,WP09,WP20,WP24|IN_PROGRESS|Go32池首scope→33池扩容、Rust subset/认证/actor bridge已接并回归；旧fence/取消/预算及失败不发布部分ready均有测试；全准入目录与真实源资格联验仍待完成|
|R04.2|当前可信baseline/peer选择、先缓冲流/固定锚点/有界尾窗对齐/generation原子发布，空盘及长停机恢复|WP08,WP09,WP20,WP24|IN_PROGRESS|Go/Rust同owner KV恢复账本及prepared/journal/原子READY已接；本机PG/Kafka共8个等效间隔/空KV场景通过；真实peer、retention及跨故障域资格仍有限|
|R04.3|历史补洞独立namespace/权限/队列/配额，不参与ready、不覆盖online；记录首合格scope/报价及恢复容量|WP08,WP09,WP20,WP24|IN_PROGRESS|Go/Rust独立history namespace/dataset/revision及≤15%墙钟调度已接；Rust queue/runtime18项和normal关闭/暂停/不改online通过，真实历史能力及长期容量/留存持续验收未完成|
|R01-R04.A|逐任务独立SPEC→quality、真实与受控证据明确归属、全检/进程/恢复与完备清单audit|WP02,WP03,WP04,WP05,WP07,WP08,WP09,WP10,WP11,WP18,WP20,WP24|IN_PROGRESS|当前core5f99三服务完整check/proto/247契约边界、23普通进程检查及8恢复场景通过；R01工具218项普通/优化通过、3SBF精确匹配；逐任务真实来源/生产资格仍未全闭合|

实施顺序：先补可信证据与可独立验证的采集/恢复基础；EVM采集、Solana流屏障和scope恢复模块按不重叠文件并行实现，主线程负责真实来源与集成。每项先失败测试、最小实现、定向回归，再正常服务和故障/重启、原始来源与源码冻结。外部来源缺少时继续可完成代码，明确记录缺口而不降低门槛。

环境首查（2026-11-06）：Go/Rust prod示例均为loopback，未含Yellowstone配置。只读公共探针中Solana mainnet genesis返回预期值，两个EVM公共探针HTTP失败；只证明可达性，不证明信任/独立故障域。原始结果见[环境探针](integration/artifacts/local/r01-r04-environment-probe.json)。

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

R01源码重建取得新结果：任务隔离Linux虚拟机、官方Linux v1.52工具链与实际SBPF target下，CPMM源码`3d69122790e97cd6aff24e8ba95940793773f58c`重建的749848字节与部署payload逐字节一致，余43976字节全零；CLMM源码`6d7a0f82d2f293c553c69aaf943377367c6c4cc6`重建的1498416字节也逐字节一致，余201744字节全零。原失败候选保留。Orca源码`cc683ec92acb5e7606caa3d8e514d9383e9c7c11`通过Linux v1.44原版rustc1.79/Cargo1.79/SBPF构建，1495896字节与部署payload逐字节一致，余8989819字节全零。仅在隔离vendor视图中为三个未参与编译的time依赖manifest添加edition2024解析opt-in，原程序、锁、Rust代码及ELF未修改。三项共55/83/404个源码/manifest/lock输入均与公开原归档核对无差异，[三份精确重建与退出/填充校验](integration/artifacts/local/r01-three-native-sbf-exact-builds.json)已汇总，任务自建Linux VM已正常关机。[来源与构建绑定](integration/test/source/deployment-catalog/native-sbf-v5/cpmm-exact.json)及[CLMM实际字节比较](integration/artifacts/local/r01-solana-rebuild/clmm-native-vz-sbpf-comparison.json)各自限定所证明的版本，不自动赋予历史bank/生产资格。

R01 getter依赖采集新增负tick取整、最多1027个storage key、同chain/genesis/H/hash/runtime/stateRoot和原VM/MPT闭包门禁；完整留存证明先实际重跑原VM闭包后可0网络复验，不强求重新访问旧H，17项普通/优化测试已通过并接基础入口。Polygon新H96900773补齐531个key并通过140个原VM getter；正常Parser→Router的gross与原程序实际Token收款增量均为9原始单位，[有限差分报告](integration/artifacts/local/r01-public-polygon-refresh-04-normal-retry1/bound-report.json)保留原头时间，未提升为费用后净输出或生产执行。旧H缺失和首次有界观察器到期均原样保留。Arbitrum新H522090937的18账户/1055 storage proof已核对；发现1129个派生cache记录被误算为450次公网请求的预算计数问题，现已分离真实尝试计数并补43项普通/优化回归；新H522108799的18账户/1058 storage proof与原VM全部1114个状态读均通过，1027个getter key/264个getter可0网络重验，正常收款增量224255逐整数一致，见[Arbitrum有限差分](integration/artifacts/local/r01-public-arbitrum-refresh-07-normal/bound-report.json)。旧H失败、1024 key限制和历史窗口拒绝均保留。

R01清单核验改用64KiB分块摘要，显式64MiB单原件/512MiB总量预算；37项普通/优化测试通过，原32MiB JSON解释上限不变，旧41MB verbose trace仍只作为不透明留存。原设计文件、旧冻结来源和所有11个任务范围保留；当前仍不能宣称R01–R04全部完成。[当前最终核对报告](integration/artifacts/local/r01-r04-current-final-audit.json)记录全部实际命令退出/日志摘要、同core源码、16项部署目录与源码重放、两个新固定H数量差分及剩余资格边界：`local_checks_passed=true`，`all_requested_features_complete=false`、`production_ready=false`。本轮13 EVM源码重放与3 SBF实际重建均通过；原2258份冻结EVM输入和原设计文件保持不变。

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

本轮最终验证：core source SHA `bfb314782b68c7062bdec555597404cd3144a1277dd098143a31692347ec31f8`；Go完整check exit0（全部包race、EVM96项完整8分片、fmt/vet/contract/build）。配置15、对应关系/缺口6、进度17项普通及优化均通过。8链配置从摘要固定的真实genesis记录离线生成，实际Go/Rust正常loader全部exit0；此项不证明运行期健康/协议准入。普通PG/Kafka进程EVM13项、Solana当前RPC/WAL10项通过，留存845 raw/7 DLQ，运行前后core SHA一致；它不包含真实Yellowstone。单Ethereum真实留存H26355865的101交易在线Adapter核对及篡改/位置/预算/取消反例通过，正常服务错误撤销/上一完整提交保留/DLQ通过。Rust源码本轮未改，未把配置加载或上一freeze的Rust全检冒充本轮全检。原设计文件SHA保持不变，所有11任务仍IN_PROGRESS。完整范围与逐项缺口见[本轮核对及验证报告](integration/artifacts/local/r01-r04-followup-audit-report.json)。

当前核心功能**没有要求用户提供的私有信息**。开发自行核对的是公开来源的链身份、协议版本、最终性、历史/回放/账户完成能力、限流及故障域；能探测的自动探测，不能证明的如实保留缺口。吞吐/留存由默认预算和压测推进。只有以后用户明确选择其已有的受认证托管端点时，才涉及该可选端点的连接配置；这不是公有数据解析/报价的功能前提。当前动态目录、流发布和恢复事务等缺口仍由开发完成，不能转化为索要用户供应商账号、API Key或私有资料的理由。


所有R01–R04任务继续IN_PROGRESS；[开发与验证报告](integration/artifacts/local/r01-r04-development-report.json)保持all_requested_features_complete=false。本地验收报告只陈述其有限范围，不构成四项全部交付。


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

## 剩余功能与开发优先级（基础核对：2026-11-07；R07接入更新：2026-11-09）

核对口径：以实际服务入口/协议分派/拒绝分支和原§28验收要求为准，不以文件夹存在、测试样本数量或配置标签当功能完成。原26工作包中24个PARTIAL_LOCAL、2个NOT_STARTED（WP18/WP22），完整验收仍为0。截至2026-11-09按当前表重算为61个协议条目，其中51个有局部解析、53个有局部数学；2个有注明范围的局部执行，所有条目的完整执行仍未验收。解析仅归档的raydium-cpmm不等于creator新profile已覆盖全部版本。

“最高/高/中/上线前/低”是剩余事项的安排建议，不改写原§24.1的P0–P8阶段定义。每个协议按固定版本身份→状态/wire→实际解析→精确数学及完整后状态→独立参考→恢复/进程验证逐项完成；数据不足保持失败封闭。

|编号|优先级|设计章节 / 工作包|尚未完成的功能|代码核对与完成门槛|
|---|---|---|---|---|
|R01|最高|§7.1/12.11/12.14；WP02,WP03,WP10,WP11,WP18,WP20|真实部署身份与真实历史块参考；源码、部署二进制、升级slot/合约实现逐版本核对|现有原生/SDK差分多为固定源码+合成RPC；以真实块/账户来源、可重复回放和独立收款差分补证，不把code hash配置当部署审核|
|R02|最高|§7.1–7.3/7.9；WP04,WP05,WP07|EVM持续采集完整性：WS通知/轮询一致、provider能力/限流/双源head、并行staging有序提交、raw归档/持久DLQ、工厂发现|[adapter.go](chain-parser-go/internal/chain/evm/adapter.go)已有结构、多RPC全量receipt及RPC声明root重算局部验证，标准0–4/legacy及共同删尾拒绝通过；本轮已补Ethereum在线头hash/transactionsRoot核对；可信共识/其他链规则、normal provider能力门禁、动态工厂目录、DLQ修复重放/保留和真实独立来源验收仍缺；WS/staging/raw基础已接normal服务|
|R03|最高|§7/18/28.22；WP04,WP05,WP07,WP20,WP24|Solana Yellowstone接入、流WAL连续恢复、补洞、slot/fork屏障、事务/metadata/账户乱序重建|[rpc.rs](chain-parser-rust/crates/solana-ingest/src/rpc.rs)为finalized当前账户批次；[assembly.rs](chain-parser-rust/crates/solana-ingest/src/assembly.rs)有本地屏障模型，已补正常RPC原始接收证据WAL；同context当前RPC分片已局部实现；尚未形成真实流到WAL/fork的完整链路|
|R04|最高|§7/18/28.10/28.26；WP08,WP09,WP20,WP24|范围化恢复与扩容：EnsurePoolScope、账户依赖分片、当前基线选择和历史补洞隔离|当前RPC已支持同源同finalized context最多32片（每片99依赖+Clock、总响应预算不扩倍），16池134账户/13池CLMM seed及全scope恢复通过；slot推进导致无法收齐时失败封闭。认证EnsurePoolScope固定目录控制接口已接；动态scope、连续流/版本化账户、生产恢复后端及持续历史调度仍未完成，不能将bootstrap当独立bank/严格slot证明|
|R05|高|§11/12.2/12.13/12.14；WP02,WP03,WP10,WP11,WP20,WP25|Solana现有协议扩展：Token2022、转账费/账户扩展，Orca adaptive的Token2022/补充稀疏数组及其他高级组合/奖励领取，Raydium限价订单、奖励领取和高级组合|[orca.rs](chain-parser-rust/crates/solana-ingest/src/orca.rs)已支持独立经典SPL静态费奖励版，旧profile仍拒绝adaptive oracle；独立adaptive版本已补完整Oracle账户/普通分派；新增独立adaptive-rewards版的classic SPL/三fixed数组/初始化奖励增长与跨tick完整状态、正常RPC/双副本服务已完成限定验收；第31批独立dynamic版本已完成三连续完整dynamic/fixed混合数组解析与普通报价/全后状态及双副本限定验收；[raydium_clmm.rs](chain-parser-rust/crates/solana-ingest/src/raydium_clmm.rs)已支持独立静态输入费基础bitmap奖励版，仍拒绝订单及未建模组合；每种版本补独立状态/wire、解析、原数学/完整后状态差分|
|R06|高|§12.2/12.11/12.14；WP02,WP03,WP05,WP11|Curve其他Plain变体/Meta underlying/旧Lending及NG特殊Token/外部rate及Crypto其他版本，Balancer Managed非零BPT费用/Composable待结费用与BPT/rate-cache/V3|[stable.go](chain-parser-go/internal/chain/evm/stable.go)与[weighted.go](chain-parser-go/internal/chain/evm/weighted.go)仅命名限定模型，双币及3–20币Weighted、Managed零swap-share快照、Composable无rate-provider常规交换和Plain3/4已本地验证；非零Managed BPT完整数学/状态契约已补原回调参考，但该旧源码runtime超EIP170，正常服务继续拒绝；实际20230411版本已补独立aum-3状态、固定块解析、非零AUM/钱包及原Pause/Recovery差分，当前T34.11活动schedule单池及M1–M4跨物理池有限闭环已同源通过；BPT有限GIVEN_IN已同freeze验收，breaker独立版本CB1–CB5已通过注明范围的同freeze本地验收；rate/mixed/一般参数/完整执行等其余能力未完成；当前日期2–50目录整合与跨物理池共享Vault全局钱包传播见T34.8/T34.9；Meta直接兑换已补正常服务和只读base依赖；Meta underlying、Crypto其他版本及BPT更广参数/会计/执行仍未完成；Crypto活动ramp独立版本已完成固定块解析、完整schedule/后状态与正常双副本服务限定验收；Crypto标准settled独立版本已补原LP/gulp/调价完整状态及正常解析/报价，本地进程验收通过；标准NG独立profile已完成固定原源码模板/immutable准入、完整状态/EMA及正常RPC与双副本连续报价；[protocol-math](dex-router/crates/protocol-math/src/lib.rs)不支持的数学失败；按实际代码版本逐个补状态、不变量、精度与费用|
|R07|高|§12.11/12.14；WP03,WP05,WP11|Aerodrome/Slipstream非零外部费用模块、LM/reward与忠实费用账本；PancakeV3 LM|独立Slipstream field32/f8717faa-module-1已完成注明范围解析/原数学/正常服务及同源冻结；Pancake LM field33/98684794-lm-1已通过注明范围的主目录验收。未知模块、不同LM代际/迁移和特殊Token仍拒绝；实际BSC generation4已由阶段41独立field34通过注明范围验收；其他升级/未知模块和特殊Token仍拒绝，不能用基础V3或旧LM替代|
|R08|高|§12.2/12.11/12.14；WP02,WP03,WP05,WP11,WP14|V4/Infinity swap hook、自定义会计/覆盖费、native结算、Infinity Bin动态/hook/native组合及完整manager结算|[singleton.go](chain-parser-go/internal/chain/evm/singleton.go)及独立advanced版本已补动态LP费快照/native currency0/无swap回调权限与CL core后状态；Bin已补固定3c5ef1be原manager/tree完整读取、跨bin数学/后状态与静态零hook普通服务限定验收；完整manager/Vault结算未完成；新能力需manager状态、hook依赖和独立执行，unknown hook仍拒绝VERIFIED|
|R09|高|§12.2/12.11/28.22；WP02,WP03,WP05,WP11,WP20|Meteora DLMM、Pump/迁移曲线、LFJ Liquidity Book、Sushi Trident等新模型|LFJ v2.1.0独立field31已有限本地验收，真实Avalanche来源/执行资格仍缺；阶段49–53已补限定DLMM MM/OnlyY、奖励、订单ExactIn/ExactOut、X侧Token2022和扩展bitmap，阶段51–52补Pump classic/SOL独立ExactOut、PumpSwap canonical/boost及原毕业迁移交接，阶段57补PumpSwap卖出目标。余项为策略/Token扩展交叉、更多实际程序版本及Sushi Trident等未接入模型；按独立状态/解析/原程序差分逐项推进|
|R10|高，可并行|§12.13/14/28.16/28.17；WP10,WP14,WP15,WP20|独立执行后端与真实收款余额差：EVM REVM/固定状态DB/链规则；Solana版本化账户及SBF/Token CPI|原[simulation::verify](dex-router/crates/simulation/src/lib.rs)仍在无合格后端时拒绝；T32已有限REVM/真实收款差分和正常simulate API。阶段58–60的Meteora prepare使用实际unsigned wire只读RPC模拟，受控RPC以原SBF执行并校验余额/关闭账户，但独立可信生产资格仍缺；多链固定状态/规则/费用、其他协议及拆单等后端继续开发|
|R11|高，依赖R10|§14/28.16/28.18/28.22；WP14,WP16,WP20|受审计执行器与可用交易：各协议/拆单DAG calldata、授权/wrap/费用/退款；Solana交易/CU/账户锁/blockhash|[execution-plan](dex-router/crates/execution-plan/src/lib.rs)已有单路V2 BOUND/calldata和本地正常simulate API；阶段57–59增加Meteora单池unsigned、账户核对、标准收款ATA创建与WSOL差额包装，阶段60已完成注明范围的临时账户解包/余款退款。阶段61已补限定Token2022收款ATA、扩展租金和withheld核对；余项为其他Token扩展账户创建、多协议/多hop/拆单DAG、跨收款人原生结算、授权/完整费用及钱包签名广播确认；公开build/VERIFIED仍拒绝|
|R12|中，交易前完成|§12.10/13/28.14/28.15；WP12,WP13,WP16|实际Gas/L1数据费/原生费换算、净输出评分和费用完整性|quote响应netBuyAmountEstimate为null，CHAIN_FEE_MODEL_NOT_IMPLEMENTED；配置route_activation_cost只是提案成本，不是实测链上费用|
|R13|中|§12.10/13/28.14/28.15；WP12,WP13|图增量索引/COW、beam多标签、共享DAG联合求解、小图穷举及收益gap|新增max_splits=1..8、独立V2/V3/Aero物理池子集DP与独立整数穷举；共享池按执行顺序有界多路分配、显式次数/时间预算和网格覆盖标记；默认4路，保留候选/网格外最优性边界。全量/进程验证见[计算收尾计划](#plan-2026-11-08-quote-completion)；增量图/通用DAG与严格收益界仍未完成|
|R14|中|§12.10/12.13/13/28.12–28.16；WP10,WP11,WP13,WP14,WP16|ExactOut完整输入目标/退款语义、转账税/rebase/暂停/黑名单/代理升级能力分级|独立完整V2、旧LM/实际BSC第四代LM、Slipstream与LFJ限定ExactOut已记录；阶段50/53补DLMM MM/订单输出驱动与阶段52/57补Pump/PumpSwap目标卖出，阶段60已完成注明范围的单池WSOL退款结算。策略交叉、其他协议/版本和Token高级行为仍缺；ExactOut保持独立目标与最小输入语义，不对ExactIn结果取倒数|
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


本批LFJ增量：固定26份上游v2.1.0源码、OpenZeppelin IERC20接口及官方Solidity0.8.10摘要，检查原storageLayout后执行原LBPair.swap/实际本地Token转账。96案例包含48成功/48拒绝；独立field31模型已接入Go正常固定块getter/storage/code读取、共享契约与Router，保留动态费、协议费、全部有界bins/份额和Oracle样本。4份C++编码覆盖显式零bank观察，缺失与null不冒充零。19实际API报价已原样关联38次原运行时代码交换，真实收款增量和全部声明后状态匹配；10个篡改反例均拒绝。早期草稿和失败JS整数检查记录保留；最终比较器采用BigInt无损整数并验证10^18与10^18+1不可混同。该原计划重放是固定时钟、受控银行和标准Token资金注入的本地Q1证据，不代表真实Avalanche历史/共识或生产执行。第一批8链16目录的现有模型与限制见[计算覆盖核对](integration/artifacts/local/core-quote-functional-coverage.json)；原目录与来源资格不改写。


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


以下为2026-11-08隔离开发历史，已由阶段40主目录进度续接。阶段39期间的后续R07实现已在隔离副本`/tmp/xone-pancake-lm-next-20261108`推进：固定Pancake提交`986847948755cba528324d41be19480731c36c2a`，100个有效原银行中80完整成交/全后状态匹配、20部分成交拒绝，另4个有意不一致银行的原回滚与解析拒绝保留；4序列12步通过。新增独立field33、Go完整固定块解析及代码/immutable模板、两Rust域和严格wire/JSON、路由/共享状态及钱包Alias拒绝；100正常银行解析和9坏来源反例、4 C++wire通过。隔离目录13实际API/13原swap的真实收款和全部声明后状态一致，9个实际计划/原程序负控拒绝；当前格式和全检版源码`201705bc08c2fa6baffaec6c93c73a91c03057d25066cbc8369cf9e6533c6c6e`正在三服务全check，先前进程报告注明为格式前`d19767996acf9029d33195a0c3deb8034dd2d0351c6f595aea40e67905ca334d`，不冒充当前版复验。[隔离状态](integration/artifacts/local/core-pancake-lm-staging-state.json)。截至2026-11-08记录：主目录仍保持阶段39冻结的`374ce438e77db25642fe7bac24d52b597328fc16ebbf1556e6878786dcaff012`；Go和Rust解析器全check已观察退出码0，Rust864次执行且0忽略；Router全check和最终同源进程刷新继续运行。须待阶段39完整冻结后按基线逐文件合入LM增量，并以最新源码/二进制重验；此时不将主目录LM接入或整体请求标记完成。


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

前置阶段39已收尾：[阶段39最终报告](integration/artifacts/local/core-stage39-final-20261109.json)验证三服务全check源码一致，复用前先核对五二进制及每个旧日志/报告摘要，再补跑Polygon与Arbitrum实际正常服务回归；5项进程范围全部通过。原错误输出目录被`OWNED_NORMAL_OUTPUT`拒绝，已修复为合法`r01-public-`前缀，5项辅助测试包含目录所有权、二进制改变与日志篡改拒绝；保留旧失败报告。

本批仍限注明代码版本、标准ERC20及完整消费的ExactIn。原程序来源为受控原合约银行与合成固定时钟封套；真实BSC候选已观察到后续LM代际/惰性迁移，不能套用本版本。特殊Token、ExactOut、可信历史/独立来源、链费用后的最优净收益、第二批链及其他协议扩展仍按原剩余清单继续。`all_requested_features_complete=false`、`production_ready=false`和VERIFIED关闭保持。


阶段40复核补修：发现新LM/Slipstream专用分派跳过原通用输出代币校验，手工构造错误`Edge.to`可被误认为成交。已用LM反例实际复现：旧代码返回`999000749`而未拒绝；保留[修复前失败日志](integration/artifacts/local/core-lm-output-direction-red-20261109.log)。现将所有双币内核的方向/输出校验前移到协议分派之前，新增两协议、双方向、同输入币/第三币输出的拒绝与状态不变反例。此前尚未完成的Rust全检和等待中的进程刷新已显式中断保留，不作为当前验收通过；按修复后源码重新检查和冻结。


构建缓存补修：修复版Rust解析器工作区677次执行全部通过后，独立contract目标命中了field33生成前的旧protobuf缓存而编译失败；源码与全工作区使用的wire均含field33，内容摘要未变。原因是隔离合入采用`copy2`保留了较旧mtime，Cargo窄目标将旧编译物误判为新鲜。合入工具已改为保留字节/权限但使用新mtime，并新增旧时间戳回归；本次仅刷新生成Rust文件mtime，补跑`make -C chain-parser-rust contract-test build`，不隐去原全check退出2或冒称原命令整体退出0。


阶段40最终结果：源码`31295d87bb32ae676223c3bb134c5c50a4bfa4b3fe9b153c817a0ac53b85752d`前后一致，五个服务二进制及LM计划投影器/回放脚本按摘要绑定。Go全`make check`退出0；Router全`make check`退出0，1048次执行通过、5个既有专用API/VM入口忽略（本批LM回放独立执行）；Rust解析器原`make check`在677次工作区执行通过后因旧mtime的窄目标wire缓存退出2，修复后`contract-test build`退出0、197次契约执行通过，原失败与分阶段凭证均保留，不记作原全命令退出0。

[最终有限业务报告](integration/artifacts/local/core-lm-local-q1-20261109.json)与[六进程同源回归](integration/artifacts/local/core-lm-final-process-20261109.json)均通过。Slipstream、Velodrome、LFJ、Pancake LM以及Polygon/Arbitrum正常来源回归全部通过。新LM实际normal Go/本机PG与Kafka/双Router产生13条API计划，原Pool/LM/MasterChef执行13次swap的真实Token收款及完整声明后状态一致；9类模型/原程序篡改被拒绝，代码来源经当轮重新编译核验。双副本、固定块读取、坏来源撤销、恢复和重启均已覆盖。原80完整swap、20部分成交拒绝、4不一致银行拒绝、12序列步、4独立C++wire和两协议方向反例已通过。267公共契约副本/10基线fixture/164公开来源fixture保持一致；文档/CI及辅助普通/优化检查随后按最终报告再复验。

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

阶段44首轮限定验收：源码`c2af56fe500d02e69cd9936931724c201daa24aae2208a4eb17326d0ac062ae8`在[首轮14项验证流水线](integration/artifacts/local/core-exact-out-initial-validation-20261109.json)、最终构建与正常进程回放之间前后一致。82次Rust测试执行（10次新ExactOut、72次既有数学/路径/拆单/计划/API回归）、Router完整workspace全部target Clippy -D warnings和格式检查退出0；原始来源反例4项普通/优化共8次通过；进度17项普通/优化共34次及三服务契约复制检查通过。原93份发布artifact/源码/许可证未修改；90原交易/162 Pair转换、2无效目标和39有序网格/38可行已核对。网格原最小输入`440077088837282106916853`与正常HTTP三路结果一致。

当前normal Go/实际本机PG与Kafka/双Router的45实际API计划直接执行为61笔原Router02交易，真实发送方扣款、接收方精确到账、每步及最终全部声明Pair后状态一致；5类模型/公开响应篡改与2类原程序重放篡改共7负控通过，故障撤销/恢复及重启保持另一链可用。[首轮限定验收](integration/artifacts/local/core-exact-out-initial-local-q1-20261109.json)保留不可覆盖进程凭证、API原输入/模型投影/原执行记录与摘要。原getter补采前正常接入失败、测试配置超界、数组比较顺序问题和无效负控值均已保留；早期被中断的预检不计作最终lint通过。

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

阶段49最终结果：源码`51fe374be5d29fb07454ac87f61a61814f7d2a6eee2dbbea8591b0bf3276a7ed`在全部26项统一检查、最终构建与正常进程运行前后保持一致。[当前限定验收](integration/artifacts/local/core-meteora-mm-validation-20261110.json)状态为LOCAL_SCOPE_VERIFIED，绑定不可覆盖的[同源进程凭证](integration/artifacts/local/core-meteora-mm-process-51fe374be5d29fb07454ac87f61a61814f7d2a6eee2dbbea8591b0bf3276a7ed-4624f5a16e047699d3a44405801a58724ed5bcb9e6b1a9078f64580940f6dfb5.json)。108次Rust执行通过，包含解析器新增及既有54次、Meteora数学/路由/共享优化/API 9次及45次LFJ/V2/V3/候选/拆单/API回归。两个Rust工作区的workspace/all-targets编译检查退出0；Parser所改domain/config/ingest库及新测试、Router workspace库/二进制/示例及本批新测试的Clippy -D warnings退出0。未宣称对全部未修改测试目标重复全目标Clippy。Go domain测试/额外vet、公共代码再生核对、281契约数据复制/10 fixture/164公开来源文件，以及17项进度测试普通/优化34次通过。

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

原始复验与篡改检查：[参考复验](integration/artifacts/local/solana-expanded-reference-reexecution-20261111.log)、[参考控制](integration/artifacts/local/solana-expanded-reference-controls-20261111.log)。统一入口`validate_solana_expanded.py`已完成两Rust工作区全目标、Clippy、旧策略回归、5类正常RPC-PG/Kafka/双Router及实际计划回放；当前结果见本节最终凭证。all_requested_features_complete=false、production_ready=false，原26工作包完整设计验收仍0。

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
