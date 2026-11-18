# Xone 技术参考

本文只记录协议规则、账户/ABI布局、费用舍入及服务通信约定。开发完成情况、剩余功能和优先级统一维护在 [DEVELOPMENT_PROGRESS.md](../DEVELOPMENT_PROGRESS.md)。

源码版本与文件指纹见 [固定源码身份清单](../integration/tools/reference/protocol-sources/source-metadata.json)。固定源码身份不等于已完成真实部署或生产验收。

## 目录

- [多DEX与不同版本：具体profile、身份和费用](#multiple-dex-version-profiles)
- [规范化 V2 Swap 事实与隔离分析 Job](#normalized-facts-and-analytics)
- [Orca Whirlpools static SPL：独立Q64与固定/动态tick数组](#orca-whirlpools-static-quotes)
- [Raydium CLMM 静态费 bitmap扩展范围](#raydium-clmm-bitmap-quotes)
- [Raydium CLMM customizable V2 非零seed限定范围](#raydium-clmm-custom-seed-quotes)
- [Raydium CLMM DynamicFeeInfo动态费限定范围](#raydium-clmm-dynamic-quotes)
- [Raydium CLMM 静态输入费与基础 bitmap 范围](#raydium-clmm-static-quotes)
- [Raydium CLMM Token0Only / Token1Only 静态收费范围](#raydium-clmm-token-fee-quotes)
- [Raydium CP-Swap creator-layout：实际账户状态与专属 exact-in](#raydium-cpmm-creator-quotes)
- [三服务通信与正式编码](#service-communication)
- [V4 / Infinity CL：独立 singleton 状态与静态无 hook 报价](#singleton-cl-static-quotes)
- [标准 Uniswap V3：固定块解析与有界 exact-in 报价](#uniswap-v3-bounded-quotes)

<a id="multiple-dex-version-profiles"></a>

## 多DEX与不同版本：具体profile、身份和费用

对应设计§4、§7.3、§11.3、§12.2/12.9/12.11/12.14及WP02/04/05/10/11/12/13/18/25。用户2026-10-26要求优先区块解析及兑换量，并覆盖不同DEX/版本；分析低优先级。

### 当前profile

|配置protocol|adapter_version|状态与费用语义|解析差异|
|---|---|---|---|
|uniswap-v2|uniswap-v2/1|既有V2整数；legacy费率来自配置|可选v2身份；保留既有合成/legacy支持域|
|pancake-v1|pancake-v1/core-1|锁定历史标准pair，2/1000|强制v2 runtime code/factory；不声称该历史版本仍有活跃市场|
|pancake-v2|pancake-v2/core-1|标准pair，25/10000|强制v2 runtime code/factory；不含Infinity|
|sushiswap-v2|sushiswap-v2/core-1|锁定标准core，3/1000|强制v2 runtime code/factory；不含Trident或RouteProcessor执行|
|uniswap-v3|uniswap-v3/core-1|标准集中流动性及百万分fee|slot0 protocol fee为uint8；独立证明tick范围|
|pancake-v3|pancake-v3/core-1|标准集中流动性及链上百万分fee|slot0 protocol fee为uint32，两个16位分量；首批lmPool必须为0|

profile是明确的支持域，不是根据函数名猜测协议。新版本、fee/layout变化、hooks/回调和未知adapter必须单独接入；不把V2/V3当成所有DEX的统一公式。

### 配置与同块读取

单文件环境配置的 `chains[].pools[]` 指定protocol/token和fee。命名V2 profile增加：

```json
{
  "id": "0x0303030303030303030303030303030303030303",
  "protocol": "pancake-v2",
  "token0": "0x0101010101010101010101010101010101010101",
  "token1": "0x0202020202020202020202020202020202020202",
  "fee_numerator": 25,
  "fee_denominator": 10000,
  "program_id": "",
  "accounts": [],
  "v2": {
    "expected_factory": "0x0404040404040404040404040404040404040404",
    "expected_code_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  }
}
```

Go按profile读取：

- V2：code（配置身份时）、factory、token0/1、getReserves。校验uint112 reserves、uint32 timestamp和地址ABI padding。
- V3：继承既有sqrt/tick/liquidity、fee/spacing、连续bitmap和每个初始化tick读取；Uniswap使用8位protocol fee，Pancake使用32位且每16位分量不超过10000。Pancake额外读取lmPool并拒绝非零地址；未知LM回调不能声称已支持。
- 所有依赖固定同一EIP-1898 blockHash且requireCanonical=true，结束重新检查块锚点。完整块检查receipt/log身份；read失败不发布半状态。

配置ID/token/factory使用规范小写20字节EVM地址。命名profile及所有地址形式的EVM Pool（包括无V2身份的legacy标签）在Go/Rust公共域、wire入口和图构造中再次拒绝非规范pool ID，防止同一物理地址大小写别名被重复计为流动性。保留旧Uniswap V2合成ID不代表新的命名profile可接受该旧约定。旧数据若含非规范EVM地址会被拒绝，须由owner按已验证配置发布合格新generation；不自动合并大小写别名后沿用旧计算结果。

协议profile只适用于EVM chain family，不能把Solana池更换为Pancake/Sushi标签进入数学模型。

### 传输、数学和路由

公共 `PoolState.v2` 使用新增字段21，传输factory及code SHA256；`V3State.fee_protocol`使用字段20。旧reserved字段不复用。内容指纹包含新增状态，prepared/Kafka/KV同一提交恢复仍按规范wire比较。Go/Rust都校验字段与protocol/adapter匹配；独立protoc六profile分片样本验证身份与宽protocol fee保留。

命名V2复用已验证的常数乘积整数原语，但profile决定固定费用、uint112状态和uint256中间边界；Pancake V1/V2和Sushi V2不共享一个硬编码费率。Pancake V3在限定标准pool域复用已差分的集中流动性原语；protocol fee影响LP/协议分成，不重复扣入交易者输入fee。非零LM回调在解析端拒绝。

路由Edge携带协议身份，逐跳验证与物理池状态一致。混合DEX版本、多跳和共享池顺序使用准确post-state；每个allocation必须全额消费，不能以部分输入结果冒充整单。新profile当前都拒绝V2诊断执行器的calldata，API VERIFIED/build仍保持原独立执行门槛。


<a id="normalized-facts-and-analytics"></a>

## 规范化 V2 Swap 事实与隔离分析 Job

对应设计 §7.6/7.7/7.9.7、§11、§16.1/16.3/16.4/16.8 和 WP02/05/06/07/21/25。本页描述限定 EVM V2 scope 的实现；ClickHouse、其他协议和真实链验收不包含在本地通过声明中。

### 数据契约

`JournalUnit.normalized_facts` 使用新增字段20，保留已有3–19号reserved区间。内部为版本化 `FactBatch`：

- `schema_version=1`，`decoder_version=uniswap-v2/1`。
- `blocks`：严格按高度/parent连续排列的完整事实块，包括无Swap的空块。每块具有完整BlockRef。
- `swaps`：按稳定log位置排列的配置池Swap；包含token0/token1和四个原始uint256金额，不丢弃flash/双向原始语义。金额使用32字节BE编码，hash使用原始32字节。
- `event_id=chain_key/block_hash/tx_index/log_index`。内容冲突拒绝，不能用txHash合并同交易多条日志。
- `reverted_blocks`：重组旧分支按高度倒序的精确BlockRef列表；必须从旧tip连续到新分支共同祖先的子块。

普通BLOCK_COMMIT携带一个完整事实块。REORG_COMMIT携带旧分支撤销清单及全部新分支事实；与池状态处于同一prepared/Kafka事务单元。REORG_BEGIN与ONLINE_SCOPE_REVOKED只发布资格屏障，不伪造Swap。基线不包含历史成交，分析必须披露覆盖起点和缺口。

Go/Rust均校验版本、序列、事件ID、hash/金额宽度、pool/token scope。独立protoc样本 `integration/test/contracts/normalized-facts.json` 覆盖uint256最大值。prepared恢复按确定性Protobuf内容hash比较，避免私有JSON的nil/空数组差异误判冲突。

### analytics 运行方式

现有 `chain-parser-go` 二进制支持有限Job：

```text
chain-parser-go --config <analytics专用配置文件路径>
```

显式配置要求：

只读消费者从私有checkpoint的精确已应用cursor继续，`read_committed`，单链固定单分区且绑定topic UUID。首次从保留起点0读取；若保留已删除所需日志、序列断裂、缺完整事实或decoder不支持，Job明确失败，不静默跳到当前owner状态。回补/外部历史恢复仍需后续实现。

Job读取当前有限稳定尾窗后退出；可以再次启动同一专用配置续读。它不新增第四种服务二进制或长期常驻API。

### 结果与崩溃语义

结果保存在 `state.path/analytics/<chain_key>.journal.json`，同一个原子文件包含：

1. 插件私有事实状态、已应用引用/cursor、topic UUID、修订、窗口身份、历史缺口。
2. 与该状态匹配的release和state内容hash。

使用方必须读取完整envelope并核验state hash/对应release，不能仅截取旧candles当成最新发布。`LoadJournalState`会验证这些条件。原子文件写入使用临时文件、fsync、rename和目录fsync。

新单元只在候选状态Save成功后安装到内存，失败不推进cursor。成功checkpoint预留有限失败状态空间；预算超限、毒消息或网络失败时，用最后耐久状态发布 `publishable=false`，保留精确消费位置。下一次从该位置重试，禁止跳过毒消息。pending重组在无新日志时仍不能被清除。

当前插件：

`journal.max_payload_bytes` 同时限制一次输入及checkpoint总量；tail记录数和RPC时间也有上界。当前达到保留预算时暂停Job，尚无自动窗口归档/ClickHouse扩展，不将无限增长隐含为生产能力。


<a id="orca-whirlpools-static-quotes"></a>

## Orca Whirlpools static SPL：独立Q64与固定/动态tick数组

### 版本与限定域

profile `orca-whirlpools-static` / adapter `orca-whirlpools/cc683ec9-spl-static-1` 固定上游
orca-so/whirlpools revision `cc683ec92acb5e7606caa3d8e514d9383e9c7c11`。
program为`whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc`，ProgramData PDA为
`CtXfPzz36dH5Ws4UYKZvrQ1Xqzn42ecDW6y8NKuiN8nD`。

只准入经典SPL Token及backed WSOL、静态fee tier（seed==spacing）、未初始化system-owned空oracle、
无初始化奖励/零控制标志/零config feature flags。Reward0 authority扩展允许保留，reward1控制/保留及
reward2扩展必须为零；reward mint/vault/emissions/growth、tick奖励outside必须为零。
初始化oracle/adaptive fee、Token2022/transfer hook/税费、未知控制特性均拒绝。

Program/ProgramData owner、executable、tag、代码SHA256与deployedslot每次账户读取核验，经典Tokenkeg
要求不可升级BPFLoader2 owner及单独代码hash。只确认已配置的账户身份，不证明源码与部署二进制对应、
validator runtime activation、当前链上最新部署已审计或未来不会升级；测试codeproof明确为合成bytes。

### 独立状态和配置

公共新增PoolState字段25 `orca_whirlpool`，独立OrcaWhirlpoolState、OrcaTickArray与OrcaTick。
价格/liquidity/fee-growth为u128 Amount，输入输出与raw/owed为u64，signed net为i128字符串；
不用V3 Q96价格右移代替Orca原始TickMath。Pool.reserve0/1为零，不把CL当CPMM。
保留程序/Token身份、config/池bump/oracle、raw vault/精度、Clock bank、fee tier/费率、
price/tick/active liquidity、protocol owed、global/outside fee growth、reward timestamp及完整数组证明。

配置`orca_whirlpool`字段为ProgramData/双codehash/deployedslot、tick_array_starts和max_steps；原accounts
为[pool,vault0,vault1,config,mint0,mint1]。要求Solana/finalized、唯一规范pubkey、3..7个有界连续数组。
spacing从池读取，实际跨度为88*spacing。每次方向调用按原SparseSwapTickSequenceBuilder最多选择3数组，
窗口中多余数组不授权第四数组遍历；当前price左向越过窗口边界后tick=边界−1，后续右向按原shift规则选数组。

状态验证检查链族、adapter/fee/model混用、u64/u128/i128宽度、静态tier、完整bitmap/tick对应关系、
连续范围和原始price/tick关系。深拷贝包含每个数组与ticks，wire往返不丢signed net或fee growth。

### finalized账户与实际布局

同一次getMultipleAccounts读取6业务账户＋program/ProgramData/Tokenkeg/Clock/oracle＋N数组，
当前3数组为14账户，总预算≤100。仅声明的oracle PDA可以用RPC null虚拟system-owned空账户；其他null拒绝。
Clock.slot须等于context.slot；reward timestamp不得晚于Clock；blockTime供freshness，二者不强制相等。
读取后复核同finalized slot的块hash/parentSlot/parentHash，不将minContextSlot当历史精确账户选择。

- Whirlpool账户653bytes；config108bytes含offset106 feature_flags。fee上限60000ppm，protocol fee上限2500/10000。
- pool PDA由whirlpool/config/mints/fee-tier-seed推导，array PDA由tick_array/pool/start十进制字符串推导，oracle PDA独立推导。
- config/vault为存储关系和SPL authority约束；原初始化的vault是任意signer账户，不能虚构vault/config PDA。
- 固定array9988bytes，88项各113bytes，校验start、pool、bool、gross/net/fee-growth与奖励字段。
- 动态array148..10004bytes：start/pool/128bit bitmap与逐项1或113bytes Borsh编码；要求bitmap与实际initialized完全一致、长度合法、无尾部未知字段。
- mint/vault经典SPL initialized/精度/authority/state/COption/delegate/close字段严格核验；WSOL检查native标记/rent/lamports backing，允许未sync捐赠但不虚增amount。

来源验证失败复用已验证的持久化scope撤销：本地先notready，fenced Kafka撤销传播到两个完整router；
新合格基线高generation恢复，另一EVM链保持可用。未扩大Solana fork/Yellowstone/WAL/生产故障域的完成声明。

### 数学与后状态

原Q64TickMath tick±443636，价格4295048016..79226673515401279992447579055；正tick以原Q96系数逐步floor后>>32，
负tick用原Q64系数。遍历使用Orca数组搜索与B→A首数组shift，空数组不制造V3 word手续费步骤。
精确输入每步checked整数/ceil费用、跨tick流动性更新，input/output u64，中间原u128/u256宽度遵循原源代码。
protocol fees和fee growth按原宽度wrapping，crossed tick outside按新global wrapping-sub；raw转账checked u64，
reward timestamp更新为banktime。原manager允许的partial/零输出在本API拒绝，不能宣称完整消耗。


<a id="raydium-clmm-bitmap-quotes"></a>

## Raydium CLMM 静态费 bitmap扩展范围

原设计§4/7.3/11.3/12.2/12.9/12.11/12.14、WP02/03/04/05/11/12/13/20/25对应。
固定raydium-clmm源码6d7a0f82d2f293c553c69aaf943377367c6c4cc6，不宣称最新部署。
新profile `raydium-clmm-bitmap`，adapter `raydium-clmm/6d7a0f82-spl-bitmap-1`。

### 账户和证明边界

原7业务accounts配置保持，程序/ProgramData/Token/Clock/3..7连续数组之外，新profile固定添加
`[b"pool_tick_array_bitmap_extension",pool]`派生的原程序PDA。一次finalized原子账户batch共15..19账户，
读后block身份复核。Extension实际1832字节，discriminator8/pool32/positive14*8u64/negative14*8u64，
校验owner、非executable、长度、discriminator、存储pool_id及规范PDA；缺失账户不视为空bitmap。

独立RaydiumCLMMState新增optional bitmap_extension={address,positive112word,negative112word}，
公共field61，保留field60 collect_fee_on及reserved40..59。新profile必须有extension；旧static/token-fee
仍必须无extension且只支持基础[-512,511]，不暗中扩大原验证范围。新profile允许静态收费mode0/1/2。
空值不改变旧JSON，旧480/968参考及fixture/TickMath保持字节一致。

逻辑数组索引基础[-512,511]，扩展positive[512,7679]、negative[-7680,-513]。
实际proofarray仍要满足原tick范围、spacing、60tick、3..7连续数组/PDA、bitmap与initializedticks一一对应。
负组低bit映射更小的负index：例如-1024对应negative组0 bit0，-513对应组0 bit511；负整组边界需独立处理。
存储完整224word，不把高位截成uint256，不推断窗口外无流动性。

### 计算与完整后状态

Q64原TickMath、逐stepfee、Token0/Token1收费、growthwrap/checked账本和rawvault语义沿用独立已验证模型。
搜索覆盖基础/扩展和正负512位组边界，先按整个bitmap找到候选，再要求其数组在已证明账户集中。
选中窗口外数组拒绝unproven，缺失extension拒绝，越过原可用tick域终止为insufficientliquidity。
成功必须全额消耗且输出非零，完整Pool后状态用于共享池按实际执行次序重算。


<a id="raydium-clmm-custom-seed-quotes"></a>

## Raydium CLMM customizable V2 非零seed限定范围

对照原设计§4/7.3/11.3/12.2/12.9/12.11/12.14、WP02/03/04/05/11/12/13/20/25。
固定raydium-clmm@6d7a0f82d2f293c553c69aaf943377367c6c4cc6；新profile `raydium-clmm-custom-seed`，
adapter `raydium-clmm/6d7a0f82-spl-custom-seed-1`，非最新部署或源码与链上二进制一致的声明。

### 创建版本、身份和实际布局

原create_customizable_pool_v2要求seed_index>0，将其u16小端编码追加至pool/config/mint0/mint1种子。
原PoolState.seeds在seed为0时使用空seed兼容legacy；非零时使用stored[2bytes]，`key()`按原bump派生。
本profile seed限定1..65535，零值只属于旧profile；状态使用u32以显式拒绝宽度溢出，JSON零值省略保持旧样本。
公共RaydiumCLMMState field62为seed_index，field60/61和reserved40..59保持。

实际pool偏移391..393为u16LE；393..397仍须零padding。不可把seed编码误当配置index或arraystart的BE，
也不能只检查地址字节合法而忽略seed/bump。Go/Rust typedstate、JSON/protobuf入口和RPC解码均绑定原poolPDA。
vault0/vault1、observation、tick arrays和bitmapextension继续派生自该pool，SPL vault authority也必须是该池。
同config和mints但seed不同表示两个独立物理池，不能按mintpair或旧PDA去重或共用状态。

新profile强制完整1832bitmapextension，staticfee0/1/2、defaultDynamic、经典SPL/backedWSOL、无初始化reward/limitorder。
旧static/token-fee/bitmap profiles保持seed0及原extension范围，不借本profile放开身份。配置仍为原7业务账户，
不新增会覆盖账户seed的配置项；seed由实际账户读取并与配置pool身份比对。

### 正常RPC和精确计算

同finalized单batch15..19账户读取所有依赖、ProgramData/Token代码与部署slot、Clock.slot和读后block身份。
程序owner/executable/精确ABI长度/discriminator、严格open_time、原bitmap/60tick/价格/费用宽度与未知状态拒绝保持。
seed0/错LE/旧poolbump或vaultauthority、错误extensionpool、未知padding及缺依赖必须失败封闭。
合法pool的seed不能原地变更；进程下一bank只更新同一seed的费用/流动性/bitmap等可变状态，另一seed池有独立身份。

seed改变身份不改变已证明原Q64精确数学。原输入/输出侧费用ceil、protocol/fund逐stepfloor、growthwrap/checked账本、
跨base/正负bitmap组和真实tickdomain、数组epoch/observation后状态全部保留。成功必须全额消耗且输出非零；
证明不足、预算/价格边界或金额宽度不满足则拒绝，不伪造部分成交。
共享池连续重算比较完整postPool；两独立seed池的图/状态/allocation/输入守恒需分别验证，不互相借用余额。


<a id="raydium-clmm-dynamic-quotes"></a>

## Raydium CLMM DynamicFeeInfo动态费限定范围

对照原§4/7.3/11.3/12.2/12.9/12.11/12.14和WP02/03/04/05/11/12/13/20/25，固定
raydium-clmm@6d7a0f82d2f293c553c69aaf943377367c6c4cc6，不宣称最新部署。
profile `raydium-clmm-dynamic`，adapter `raydium-clmm/6d7a0f82-spl-dynamic-1`。

### 状态和实际账户

RaydiumCLMMState field63 optional dynamic_fee显式保存9字段：filter_period、decay_period、
reduction_factor、dynamic_fee_control、max_volatility_accumulator、signed tick_spacing_index_reference、
volatility_reference、volatility_accumulator、last_update_timestamp。旧profile必须None/default原80bytes，
JSON省略None保持旧480/968/364/464及各fixture/hash可复核。
新profile必须原参数域内Some，mandatory完整bitmapextension，支持fee0/1/2和原seed0/nonzero身份。

实际pool偏移1096..1176为80字节DynamicFeeInfo，前34字节按原packedLE读取、后46字节padding仍零，
1176..1544及原其它reserved继续拒绝未知非零；不因为支持动态费就放开新ABI未知字段。
原参数要求filter>0、decay>filter且均u16、reduction1..9999、control1..99999、
maxVolatilityAccumulator*tickSpacing<=u32MAX。reference/accumulator必须<=max，
referenceindex在原tickspacingindex域，timestamp限定u32范围；允许reference更新时间晚于当前Clock时按原saturating_sub处理。

正常RPC仍一次finalized15..19账户，ProgramData/Token代码hash/部署slot、Clock.slot、pool/PDA/seedLE、
vault/mint/SPL/backedWSOL、strictopenTime、bitmap数组证明及读后block校验保留。
不支持初始化reward/limitorder/Token2022，动态状态不得混入旧staticprofile。

### 原分段计算与完整后状态

初始化按原tick floor/spacing求index，并按Clock与last_update的filter/decay窗口更新reference。
每段更新volatility accumulator并封顶，dynamic component按原整数平方/ceil求费，
base+dynamic总费率按原100000上限截断，不能以静态最大fee替代。
按tick-spacing bounded price细分原initializedtick之间的路径，保留空流动性/达到cap时跳过的原index更新和边界规则。

原输入/输出侧费用、partialdust全输入、protocol/fund逐段floor、global/outside增长与raw账本继续按真实费用币种更新。
最终动态9字段（含reference更新时间/波动值/index）、price/tick/liquidity、arrays/epoch、observation及完整Pool必须匹配原native。
共享池连续报价以实际postPool再计算，不能重用旧fee参数或每条route重置动态状态。
成功必须全额消耗且输出非零；预算/价格/证明/宽度耗尽拒绝，不返回隐式部分成交。


<a id="raydium-clmm-static-quotes"></a>

## Raydium CLMM 静态输入费与基础 bitmap 范围

### 准入与账户解析

正常 Rust RPC 入口一次 finalized getMultipleAccounts 读取7业务账户、4程序/Clock证明和3..7数组，
共14..18账户。配置顺序为 pool、vault0、vault1、ammConfig、mint0、mint1、observation。
程序必须使用规范 ProgramData `HzD2cCXXT3UQNjMMY6kDv9w6gZ9qquSdfoGXrLL3LXx`，
校验 upgradeable loader 的 Program/ProgramData 链接、部署slot、配置代码SHA256，以及经典Token程序代码身份。
Clock.slot 必须等于账户batch context slot，所有账户owner/executable/长度和读后block身份均须满足准入。

实际布局分别为 pool1544、config117、tick168、60tick数组10240、observation4483字节。
config index 和数组 start seeds 使用大端；其余账户字段按原 packed layout 小端解析。
池、vault、observation、数组 PDA 与配置/池/mint 关系校验；typed state在Go/Rust也重新验证可推导PDA。
未初始化reward可保留原创建者authority，但时间、mint/vault、emissions、余额和growth必须默认。
未初始化tick允许原source清除后保留的tickindex，但订单字段、奖励growth与未知padding必须零。

限定 classic Tokenkeg 与有lamport backing的WSOL，拒绝 Token2022、delegate/closeauthority/冻结vault和扩展。
静态输入侧 `fee_on=0`、完全默认 DynamicFeeInfo、legacy `seed_index=[0,0]`、swap未暂停。
不支持初始化奖励、限价订单或未知新增状态。尽管源码open_time字段注释称禁用，实际external_swap/swap_v2
要求 Clock 严格大于 open_time；相等必须拒绝。bank timestamp限定0..u32::MAX，未支持未来时间回绕。

### 独立状态与精确金额

公共 PoolState 字段26为独立 RaydiumCLMMState：Q64.64价格、liquidity、signednet、16个u64基础bitmap、
3..7连续数组、逐tick outsidegrowth、全局growth、protocol/fund/rawvault账本、Clock/epoch及100项observation。
不转换为Orca/V3，不伪造储备。Raydium正tick按原19个截断常量后做u128::MAX除法；最大价格为
`79226673521066979257578248091`，与Orca不同。

exact-in逐步执行原60tick/bitmap遍历、输入费用ceil、protocol与fund分别floor，保留LP增长wrap语义，
费用账本和vault执行checked宽度。穿越更新outsidegrowth、liquidity、tick及数组recent_epoch。
Observation只在最终tick变化时按旧tick更新：15秒间隔、饱和时间差/乘积、i64累计wrap；原epoch保持。
完整后状态用于共享池顺序模拟和allocation重算；成功必须全额消耗，剩余输入或证明/步骤/价格边界耗尽拒绝。
业务max_steps计量实际遍历；native参考使用更严格的“提供的全部initialized ticks+1”预检，不能证明所有更小预算等价。

仅支持基础1024bit（数组索引[-512,511]）。保留证明窗口外的bitmap位，不假定外面无流动性；
报价若需要缺失数组或bitmapextension则拒绝。此范围不包含outputfee、自定义seed、dynamicfee、
limitorders、rewards、Token2022或其他升级版本。


<a id="raydium-clmm-token-fee-quotes"></a>

## Raydium CLMM Token0Only / Token1Only 静态收费范围

对照原设计§4/7.3/11.3/12.2/12.9/12.11/12.14、WP02/03/04/05/11/12/13/20/25。
固定源码 `raydium-io/raydium-clmm@6d7a0f82d2f293c553c69aaf943377367c6c4cc6`，
新profile `raydium-clmm-token-fee`，adapter `raydium-clmm/6d7a0f82-spl-token-fee-1`。
这是固定原程序版本的有限金额范围，不宣称最新部署、源代码与部署二进制等价或生产准入。

### 身份与状态边界

继续使用独立RaydiumCLMMState/Q64/60tick/base1024bitmap/3..7连续数组/100observation，
公共PoolState字段26保持，RaydiumCLMMState新增`collect_fee_on`字段60，reserved40..59保持。
JSON缺省0且0值不输出，旧480案例与旧static profile保持可验证；新profile必须显式为1或2。
0只属于旧`raydium-clmm-static`；3/255/更大值、profile/adapter混淆一律拒绝，不继承原函数未知mode回退。

正常RustRPC读取实际packed pool偏移390；391..397继续只允许legacyseed与零保留区。
同finalized原子14..18账户集、ProgramData/Token程序代码hash与部署slot、Clock.slot、PDA和读后block身份、
pool/config/vault/mint/array/observation实际布局、classicSPL/WSOL backing、no-reward/no-order准入全部保留。
配置profile可切换，但不会以配置fee覆盖链上ammConfig费率。实际external_swap的strictopenTime仍要求Clock>open_time。

### 收费方向与完整后状态

|模式|卖token0买token1|卖token1买token0|费用增长/费用账本|
|---|---|---|---|
|Token0Only(1)|输入侧收费|输出侧收费|token0|
|Token1Only(2)|输出侧收费|输入侧收费|token1|

输入侧沿用已证明原step语义；输出侧让完整输入驱动价格，grossoutput按原ceil计算每step费用，
用户得到netoutput。未到目标价格的partial step按原源码消耗完整remaininginput，避免舍入dust停滞，
不能把dust改计为输出币种费用。protocol/fund分别逐step floor，剩余LP费按实际fee token更新globalgrowth；
crossedoutside按两个币种正确global更新。raw输入vault增加全额输入，输出vault只减少用户实际净输出，
留在vault的输出费用进入相应LP/protocol/fund账本。所有checked/wrapping宽度语义保留。

共享物理池连续报价按完整Pool后状态重算，包含模式、价格、tick、liquidity、arrays/epoch、
global/outside growth、protocol/fund/rawvault和observation。金额计算成功必须完整消耗且输出非零，
证明窗口、价格、步骤预算或资金宽度不满足则拒绝；不返回隐式部分兑换。


<a id="raydium-cpmm-creator-quotes"></a>

## Raydium CP-Swap creator-layout：实际账户状态与专属 exact-in

日期：2026-10-26。依据设计§4、§7.3、§11.3、§12.2/12.9/12.11/12.14，WP02/03/04/05/07/09/10/12/13/20/25。
计划：[统一进度中的实施计划](../DEVELOPMENT_PROGRESS.md#design-traceability)。当前实现域是经典SPL Token的池金额状态与整数报价；
分析保持低优先级，未宣称整个Solana接入或交易执行完成。

### 新旧版本与代码身份

新profile为 `raydium-cpmm-creator`，adapter为 `raydium-cpmm/f4b9b263-spl-1`，对应
raydium-io/raydium-cp-swap源码revision `f4b9b26342afe9122d5ff67cd7a4f19ded4ac0a5`。
旧 `raydium-cpmm/archive-v1` 仍保留限定解析，继续拒绝creator字段且不报价。新模型未重用V2分式费用。

固定program `CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C`；UpgradeableLoader Program账户
(tag2、36bytes)必须指向其loader PDA ProgramData `DMawCQzbgNTmbzaESc7o6pvL1KAeetY8zA7jNpzntHhU`。
ProgramData tag3、45byte metadata后的全部代码SHA256及deployed_slot须匹配显式配置并不晚于账户bank。
每次账户快照重验。经典Tokenkeg程序须executable、不可升级BPFLoader2 owner，并匹配独立配置代码SHA256。

这证明当前返回账户集的配置代码身份，不证明源码与部署产物对应、validator feature/runtime activation、
当前部署最新版本已审计或未来不会升级。部署slot等于bank slot仍只作为账户身份；VERIFIED不开放。
程序/Token测试代码为明确的合成proof bytes，不能作为mainnet部署证明。

### 公共状态与配置

新增 `PoolState.raydium_cpmm` 字段24，独立RaydiumCPMMState保留program/ProgramData/hash/slot、
Token程序hash、authority/bump、config/vault、raw vault、三种累计费用、费率、creator模式/开关、
Clock slot/unix/epoch、open_time/recent_epoch、mint精度。Pool.reserve0/1始终为raw扣protocol/fund/creator
后的净储备，raw/累计额均u64，Go/Rust及Protobuf/JSON往返严格保留完整整数。

全局authority按官方PDA规则独立推导并固定为 `GpMZbSM2GgvTKHJirzeGfMFoaZ8UR2X7F4v8vHTvxFbL` / bump253。
Go/Rust公共域拒绝模型混用、角色地址重复、错误链族、未知mode、无效hash、未来epoch或不一致净储备。
快照bank_slot必须等于SnapshotRef的slot。账户的ProgramData、vault/config PDA和Token关系由解析owner验证。

`PoolConfig.raydium_cpmm`：expected_programdata、expected_program_code_sha256、expected_deployed_slot、
expected_token_program_code_sha256。原accounts顺序仍为[pool,vault0,vault1,config,mint0,mint1]。
新profile必须Solana链和finalized commitment，拒绝确认级别混用、未知扩展及缺失版本配置。
fee来自当前AmmConfig；配置中的fee_numerator不覆盖链上admin更新。

### 账户屏障与失败传播

一次getMultipleAccounts包含六个业务账户＋program/ProgramData/Tokenkeg/Clock，总唯一账户≤100。
不使用minContextSlot假装历史精确选择；取得当前finalized context，要求Clock.slot精确等于context.slot，
用Clock.unix_timestamp判断open_time。getBlock.blockTime供快照freshness使用，两者不是同一时间语义，
不要求时间值相等。finalized块hash/parentSlot/parentHash需合法，读后重新读取同slot块锚点。

### 整数费用、raw/net后状态

- creator在输入侧：对trade+有效creator费整体ceil，然后按creator占比floor拆分。creator disabled时
  有效creator费为0，但仍扣除历史累计creator账本。原输入分支的组合费率为0会因split除零返回None。
- creator在输出侧：先ceil输入trade fee，算原始curve输出，再ceil输出creator fee。模式0两方向输入，
  模式1只token0收费，模式2只token1收费，方向不能互换。
- protocol/fund取trade fee的floor份额。raw输入vault加完整dx，raw输出vault减用户实际输出，
  三种账本checked u64累加，再重新计算净储备。LP保留费用仍在池中，不能使用CurveCalculator
  扣掉所有输入费的临时new_input作为下一笔净储备。recent_epoch更新为Clock.bank_epoch。
- 输入/output/原始vault/账本u64与中间u128边界、正输出、不变量与完整消耗均校验；失败返回错误。

公开create_amm_config入口及update入口要求trade+creator<1e6，protocol+fund≤1e6；内部赋值函数
没有断言不代表公开入口允许绕过。当前数学/状态探测另覆盖独立费率更宽域；超过组合上限的golden
样本明确是算术压力探测，不能证明配置可达性。原helper只有在实际分支的checked运算允许时才成功。
creator share只影响collect已有费用，不额外扣swap费用。


<a id="service-communication"></a>

## 三服务通信与正式编码

对应设计§7、§9.17、§10.4、§11和WP02/06/09/24。中央协议源为 `common/protoc/proto/dex/v1/state.proto`，Go/Rust 生成类型分别位于 `common/protoc/golang`、`common/protoc/rust`，服务直接引用，不自行生成。

### Kafka数据通道

1. Go/Rust解析owner将领域状态转换为Protobuf `OnlineSnapshot`；每个金额使用恰好32字节的无符号大端`Amount.uint256_be`，EVM块hash和Solana块hash均使用原始32字节。
2. `JournalUnit(kind,snapshot)`采用Protobuf二进制编码，按配置切成`JournalFrame` BEGIN/CHUNK/COMMIT。payload SHA256针对真实二进制单元，所有chunk必须完整且身份一致。
3. 每条链固定一个topic的分区0；topic/分区数量检查失败即停止，transactional.id按chain_key稳定，不随实例/owner assignment改变。
4. Kafka事务成功后，解析器原子更新本地state、applied journal_seq、块身份和下一cursor。private prepared/KV可使用各语言私有持久格式，不将其作为跨语言协议。
5. 路由器使用read_committed；完整COMMIT后才应用到池/图同一QuoteView，独立副本均消费完整分区，不将group offset当成状态。

### 基线恢复

`GET /internal/chains/{chain}/bootstrap`要求内部Bearer凭据，返回`application/x-protobuf`的`OwnerBootstrap`。snapshot与cursor来自同一已提交本地batch。计算器验证owner、assignment、schema、scope、新鲜度、topic/partition及cursor范围后恢复，再读取有界尾窗。

同代际的完整owner checkpoint可重建本地计算副本，不分配或伪造上游generation；旧本地view失效后重新构建池图。超出保留/预算时重新取当前checkpoint，不以全历史重放作为ready前置。

### JSON接口与开发profile

用户报价API、状态/健康/指标、`/baseline`诊断接口保持HTTP JSON；JSON金额均为十进制字符串，拒绝number。显式`http-development` profile仍可用JSON基线/日志完成离线联调，不作为正式Kafka编码。

`integration/test/contracts/journal-frames.json`是测试描述文件，其data_hex保存protoc独立产生的二进制fixture；该JSON文件不是运行中的Kafka编码。生成工具为`integration/tools/ci/journal-golden.py`。

### EVM重组与在线资格屏障（§7.9）

所有控制单元仍使用同一 `JournalUnit`/`JournalFrame` Protobuf及完整事务屏障，不增加额外服务或第二条规范顺序流。

1. `ONLINE_SCOPE_REVOKED` 携带旧块/池内容，journal_seq加一；canonical_epoch不变。源读取失败或疑似分叉先撤销旧资格。
2. 不同配置RPC确认新head且定位窗口内共同祖先后，`REORG_BEGIN`再次加seq并推进canonical_epoch；writer_epoch、online_generation、scope保持。
3. Go按parent/hash倒序读取snapshot undo，在有界私有staging中按高度应用新分支；中间状态不提供Current/Bootstrap。
4. `REORG_COMMIT`加seq且精确匹配begin的canonical/writer/generation/scope；完整新状态和图才重新ready。Begin/commit间不能插入可报价BLOCK_COMMIT。
5. 重组read失败/超窗/崩溃时屏障保留。owner以独立来源确认的HEAD_FIRST新generation恢复；普通来源故障若旧anchor仍canonical，不推进canonical_epoch。

路由副本记录成功应用完整单元的ref/cursor；Kafka reader重建从该cursor续读matching commit。严格禁止旧/同代际checkpoint直接解除pending重组屏障。显式HTTP开发profile在所有来源不可用时暂停，并用最后消费引用继续回放；它不提供生产Kafka耐久性保证。


<a id="singleton-cl-static-quotes"></a>

## V4 / Infinity CL：独立 singleton 状态与静态无 hook 报价

日期：2026-10-26。设计对应§4、§7.3、§11.3、§12.2/12.9/12.11/12.14；实施任务见
[统一进度中的实施计划](../DEVELOPMENT_PROGRESS.md#design-traceability)。解析与兑换量优先，分析维持低优先级。

### 版本和准入

|profile / adapter|固定主源码|本地准入|
|---|---|---|
|uniswap-v4 / uniswap-v4/core-1|Uniswap/v4-core，bea7f6d1c11cd5dc4f0d5151ffa37a1c239feee6|零 hooks、静态 LP fee<1e6、非零有序标准 ERC20；protocol fee 每方向≤1000|
|pancake-infinity-cl / pancake-infinity-cl/cl-1|pancakeswap/infinity-core，3c5ef1be1f71442d8bc60cc6fd11f9c677b714df|相同标准币和静态无hook条件；parameters只含tickSpacing<<16，protocol fee 每方向≤4000|

两者 tickSpacing 均支持1..=32767；maxGross 按 floor(MIN_TICK/spacing) 计算，
不能采用旧V3向零截断的下界。动态fee标志、非零hook地址、Infinity hook权限或保留bit、
native currency、Bin及未知版本均拒绝。实际部署地址/代码目录、Token行为、管理器升级
和真实历史块仍需另行审计；运行时配置的code hash不能替代部署审批。

### 独立身份和公共协议

新增 `PoolState.singleton_cl` 字段23，以及 `SingletonCLState` / `ConcentratedState`：
manager/runtime code hash/rawPoolId、LP fee、两个方向的protocol fee、hooks/parameters，
以及Q96 price、tick、active liquidity、连续bitmap和每个初始化tick的gross/signed net。
不创建伪造factory的V3对象，不给既有V3状态换协议标签。

V4 raw PoolId 为5word ABI key的Keccak：currency0/currency1/fee/tickSpacing/hooks；
Infinity 为6word key：currency0/currency1/hooks/manager/fee/parameters。
物理ID为 `manager/rawPoolId`，因此相同V4 key在不同manager中是不同物理池。
Go、Rust和独立ethers参考分别重算key。原池类型禁止携带singleton状态。

配置字段为 `singleton_cl`：manager、expected_manager_code_sha256、pool_id（raw）、
lp_fee、tick_spacing、parameters、tick_word_radius、max_initialized_ticks、max_steps。
`PoolConfig.id` 必须为完整物理ID，fee_numerator必须等于lp_fee，denominator=1e6。
bitmap半径≤16、初始化tick≤4096、步数≤8192；超界/缺依赖不能作为完整成交。

### 同块读取与完整性

- V4：manager固定code hash，识别的EIP1167 clone拒绝；通过 `extsload(bytes32)`
  读取固定版 `_pools` 映射slot6。s=keccak256(abi.encode(rawPoolId,6))；slot0=s，
  liquidity=s+3、ticks=s+4、bitmap=s+5。slot0按160/24/24/24位解码，保留高位须零；
  tick首word低128为gross，高128为signed net。StateLibrary方法不是外部getter。
- Infinity：manager code hash、paused()、poolIdToPoolKey、getSlot0、getLiquidity、
  getPoolBitmapInfo、getPoolTickInfo。核对完整6word key、4word slot0及4word tick布局。
- 所有code/call使用同一EIP1898 blockHash/requireCanonical锚点，外层保留完整receipt
  检查和读后canonical复核。bitmap标记的每个tick必须读取，数量/宽度/净流动性严格校验。

### 兑换量与后状态

每个方向独立取p，使用 `p+l-floor(p*l/1e6)` 组合协议费和LP费，逐word/tick计算和舍入。
输入作为原合约负amountSpecified处理；全额输入允许到2^127（负BalanceDelta最小值），
正输出必须小于2^127。输出、sqrtPrice、tick和active liquidity按实际顺序更新，后续
共享池/反向交易继续使用新状态。100%合成费用、零输出、证明范围不足、价格边界未完全
消费或步数预算耗尽都返回错误，不把部分输出包装成全额成功。


<a id="uniswap-v3-bounded-quotes"></a>

## 标准 Uniswap V3：固定块解析与有界 exact-in 报价

### 支持域与服务职责

- Go解析owner负责EVM块/receipt完整性和V3池依赖读取，按blockHash发布完整状态。
- Rust计算服务使用独立V3整数内核，双方向exact-in、逐word/initialized tick、liquidityNet跨tick、零流动性空段、手续费与Q64.96舍入。
- `protocol=uniswap-v3`、`adapter_version=uniswap-v3/core-1`；不是所有CLMM通用实现。动态fee、Slipstream、Pancake V3、V4 hooks等不能仅改标签进入该适配。
- V3 `reserve0/reserve1`固定为0占位；报价只使用V3专用状态，绝不套V2公式。仅标准token假设的指示性报价，token税/rebase/回调差异不在已验证域。

### 配置和锚点读取

在已有 `chains[].pools[]` 增加显式 `v3` 字段；其他协议不允许携带该字段：

```json
{
  "id": "0x0303030303030303030303030303030303030303",
  "protocol": "uniswap-v3",
  "token0": "0x0101010101010101010101010101010101010101",
  "token1": "0x0202020202020202020202020202020202020202",
  "fee_numerator": 3000,
  "fee_denominator": 1000000,
  "program_id": "",
  "accounts": [],
  "v3": {
    "expected_factory": "0x0404040404040404040404040404040404040404",
    "expected_code_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "tick_word_radius": 1,
    "max_initialized_ticks": 1024,
    "max_steps": 512
  }
}
```

上例地址/hash为合成示意，不是可直接使用的链上目录。实际配置必须固定目标池地址、factory、token身份及**该地址runtime bytecode字节的SHA256**；不同池的immutables可能使代码hash不同。这里不是eth_getCode十六进制文本的hash，也不是Keccak codeHash。业务值仍来自经过字段及约束校验的单个环境配置文件，不从环境变量覆盖。

读取顺序包括：pool runtime code、token0/token1、factory、fee、tickSpacing、slot0、liquidity、连续bitmap words，以及每个置位对应的ticks。全部 `eth_call`/`eth_getCode` 绑定同一个EIP-1898 `{blockHash, requireCanonical:true}`，结束再次核对块锚点。方法不支持固定hash、code/fee/factory不匹配、lock状态、ABI宽度/符号扩展、缺initialized tick均失败封闭。完整块仍验证receipt/log身份；V3 Swap事实分析未实现，不把V3-only范围的空V2事实批次当成分析完整性证据。

预算：word radius为0–16，最多33个连续word；initialized ticks上限1–4096；maxSteps为1–8192；整次区块状态读取受 `journal.request_timeout_ms` 总期限限制，单RPC也有界。范围以当前tick对应word为中心，裁剪协议全局tick边界；不通过无界历史事件回放补齐索引。

### 公共状态与证明范围

`PoolState.v3` 为Protobuf新增字段20，保留原12–19号reserved。包含：

- sqrtPriceX96（BE32容器，校验uint160）、current tick、active liquidity（校验uint128）、tickSpacing。
- factory、runtime code SHA256、maxSteps。
- 连续 `V3BitmapWord(word_pos, bitmap)`。
- 排序的 `V3Tick(tick, liquidity_gross, liquidity_net)`；net在wire为16字节有符号二补码，JSON为规范十进制字符串。

Go/Rust验证每一个置位与tick条目一一对应，无漏项、重复或域外bit；gross受该spacing的maxLiquidityPerTick限制，net必须i128且绝对值不超过gross。校验sqrtPrice/currentTick一致性；允许协议合法MIN_SQRT_RATIO初始化及跨tick后位于边界的tick-1状态。

有限range是当前可证明状态范围，不宣称完整全市场tick覆盖。报价读取到未证明word且仍需计算时返回 `V3_TICK_RANGE_INCOMPLETE`；达到价格极限或maxSteps也不返回虚构全额。API在声明候选范围内找不到合格路线时返回NO_ROUTE/相应分配失败。

### 计算、拆单和预算

内核使用整数Q64.96，tick到价格的协议数学常数、逐步手续费向上取整、输入/输出delta各自协议舍入；保留uint256乘法溢出时sqrt更新的fallback语义。每word边界都计算，不能直接跳过空word而改变手续费舍入。dust结果在数学层可为0，但不会被路由作为有效输出使用；拆单探测遇到dust会继续尝试更大的配置分配金额。

每次成功返回 consumedIn=请求输入、remainingIn=0及真实post-pool state。不能完成全额的单步/路径被拒绝；不将部分成交当成完整报价。

候选搜索使用配置 `split_parts`（仅max_splits>1时启用）探测分配粒度。若全额超范围，保留能完整消费某个分配金额的路径，以允许多个有限容量池共同完成订单。候选保存**实际探测输入与输出**，按有理数输出/输入排序；这是有界启发式，不声称全局最优。金额探测数受max_expansions限制，每次探测检查取消/期限。候选上限截断时保留最佳已证明可完整消费全单的后备路径，避免高平均汇率但容量不足的候选排挤已知可行解。

最终allocation始终按执行顺序重新逐跳计算，更新共享物理V3池的sqrtPrice/tick/liquidity；path和route的输入输出都改为该次真实分配的结果。所有分配之和必须等于用户输入。Edge明确携带protocol，不能拿V3路径生成V2 router calldata。


<a id="evm-receipt-witnesses"></a>

## EVM完整区块的多RPC receipt核验

对应原§7.3。完整区块读取先校验交易列表、每笔receipt归属及全块日志索引，再执行已配置多端点的交叉核验，合格后才解码事实和读取池状态。当前状态baseline读取与完整事实块证明分别处理。

每个端点客户端绑定单一URL，复用原HTTP超时/响应大小限制，不会在见证读取中自动切换到另一个URL。不同配置URL去重，已隔离的错链来源不参与核验。配置至少两个不同URL时，健康来源不足两个返回RECEIPT_WITNESS_SOURCE_REQUIRED；任何已参与的端点不可用返回RECEIPT_WITNESS_UNAVAILABLE，不用多数票覆盖分歧。

按交易索引归位receipt，逐项比较所有建模日志字段；32字节blockHash/parentHash/transactionHash及address/topics/data按十六进制字节语义忽略字母大小写；交易和日志位置quantity仍严格使用标准编码，不忽略日志增删、内容或顺序。每个来源在读receipt前后核对同一块号/hash/parent/timestamp和完整交易列表。全部调用受原整块总期限约束。

该检查能发现已配置来源之间的完整尾部遗漏差异。重复URL不产生第二来源。正常完整块读取现在另做下述receipt根校验；不同URL属于同一供应商、共同伪造区块头/root或未验证的链共识仍不能由此证明正确。该能力不提升报价等级、不新增DEX数学、不替代Solana账户版本或历史slot证明。


完整receipt日志先验证address为20字节、topic为32字节且至多4项、data为偶数字节hex且必须使用小写`0x`前缀。空data=`0x`、零topic空数组合法；缺失字段、非法hex和宽度错误拒绝，之后才做字母大小写归一。未知池日志也执行此结构检查。

整块累计receipt的保留内容与结构/索引开销受既有`journal.max_payload_bytes`对应RPC MaxBytes预算约束，超限返回`RECEIPT_BLOCK_BUDGET`，不再仅依赖逐响应上限。SHA256摘要使用长度前缀逐字段流式写入，不使用整块JSON/通用map副本；每receipt/日志检查取消与整块期限。主源与见证源各自均受此预算，预算不足失败封闭。


<a id="solana-rpc-source-evidence"></a>

## Solana RPC原始响应接收证据

对应原§7.3、§8.9–8.14。正常Rust RPC入口在有界HTTP200响应JSON解码前，将原始body和请求方法/参数写入按chain隔离的`state.path/source-evidence/<chain>/` RocksDB。响应不先做base64/账户布局转换。来源URL仅存SHA256，provider版本标注unattested，记录类型明确是当前JSON-RPC baseline接收证据，不伪造write_version、bank_id或历史slot证明。

每次进程打开产生耐久session计数；每条变化响应保留sequence/receive timestamp、payload SHA256、previous hash和原始body，record与head元数据以sync WriteBatch原子提交。启动逐条核对hash链、sequence、chain、session和累计字节，任何破坏均失败封闭。相同进程内同来源、同方法的最近一次参数及body未变化时复用原证据序号（A→B→A保留三条）；它只证明原响应已收到，不保留每次轮询的全部到达时间。

预算由该服务既有`journal.max_payload_bytes`（单个response/request上限）和`journal.kafka.max_tail_records`（最大保留条数）提供。单record编码上限为`2*max_payload_bytes+2048`，逻辑保留总量上限为该值乘最大条数，带checked运算。达到预算返回SOURCE_WAL_CAPACITY，不删除未归档证据，不自动换provider绕过失败；归档/保留轮转仍需后续实现。RocksDB物理日志/compaction有额外空间，不能将逻辑上限当物理磁盘配额。

持久写由有界RPC调用等待blocking任务完成后才放行至解码器，WAL不与在线cursor混用。debug fixture SnapshotSource不产生真实源证据。内部鉴权接口`/internal/chains/{chain}/source-evidence`只返回计数、session、hash和预算；evidence_only=true、confers_readiness=false，不返回URL凭据或原始账户内容。

本能力是RPC接收证据，不是Yellowstone接入、网络断流连续性、历史账户版本恢复、完整fork/WAL重放上线或生产吞吐证明；不提升INDICATIVE/VERIFIED等级。原始response可由受控离线工具逐序号读取；在线恢复仍需要正常RPC及全部既有程序/Clock/身份/完整性校验。


证据库显式采用RocksDB AbsoluteConsistency；另有包含全部字段checksum、独立fsync/rename/目录fsync的SOURCE_HEAD标记，防止底层DB WAL丢失或截断后接受较旧前缀。DB可能在崩溃时领先最后已确认head，重启可保留并验证该未确认后缀，但不能丢弃已确认的记录；整个证据目录与head同时丢失仍需外部归档/灾备来证明历史丢失。

任一DB/marker写入错误都会将该打开实例置为write_poisoned，后续新写与旧去重命中均失败，必须重新打开并完成验证。异步capture在复制响应和派发blocking任务前取得单个许可，许可随实际任务持有，外层超时不允许积压第二个写盘任务。receive_timestamp在后台排队之前采集；状态接口使用非阻塞锁，慢磁盘返回SOURCE_WAL_BUSY而不阻塞健康服务。

SOURCE_HEAD读取最多2049字节，超过2048即拒绝，避免损坏文件在验证前造成无界分配。可用`make -C integration source-wal-test`重跑存储/RPC/准入专项，最终冻结证据通过`make -C integration source-wal-evidence-check`校验；后者使用显式拒绝条件，不因Python优化模式失效，并同时检查报告生成时间与文件新鲜度。


<a id="evm-receipt-root"></a>

## EVM receipt根与信任边界

对应原§7.3、§28.7。正常`ReadComplete`先检查完整交易列表、receipt/log身份和全块日志序号，再重算consensus receiptsRoot。主源及每个witness均独立校验，缺失/错误根、状态或gas编码、bloom不符、根不符均拒绝，先于池状态读取与BLOCK_COMMIT；同块前后锚点比较包含receiptsRoot。按需读取当前池baseline仍走独立HEAD_FIRST流程：历史receipt损坏可先撤销旧scope，再按既有HEAD_FIRST恢复新代baseline；该恢复不补历史receipt/成交事实。

支持标准Ethereum receipt type 0–4（legacy、access-list、dynamic-fee、blob、set-code），以及legacy的32字节pre-Byzantium post-state。status与post-state不可同时出现（包括显式null/空值）；typed receipt不接受post-state。旧RPC省略type仅解释为legacy；显式空字符串、null或错误JSON类型仍拒绝；typed数据在该解释下必须与真实根匹配。未知链专有type（包括示例0x7e）显式拒绝，需独立适配。累计gas以交易顺序严格递增；logsBloom必须为256字节且与地址/topics重算一致；日志data仍包含在receipt RLP中。

Go实现固定使用go-ethereum v1.16.7的receipt编码与内存StackTrie；依赖由go.mod/go.sum锁定。输入沿用逐响应及整块receipt累计预算，新共识字段计入累计开销。重算只依赖有界内存，仍检查总调用期限；这不是生产吞吐或单次计算硬抢占保证。

独立参考固定为@ethereumjs/trie 6.2.1、@ethereumjs/rlp 5.0.2、ethereum-cryptography 2.2.1，package-lock含完整性hash；`make -C integration receipt-root-reference-refresh`可重建12组service-local vectors，覆盖空块、全部五类receipt、legacy post-state及128/130/257交易索引边界；同时冻结11个合成进程fixture根。普通单元/进程测试只读固定数据，不执行Node或联网；新增fixture必须先显式重建独立参考，未知payload不使用占位root。生成器、参考依赖锁及fixture hash由验收证据绑定。

本能力核对的是RPC声明的区块root，不重算完整区块头hash/transactionsRoot，不独立验证执行与共识，也不证明供应商独立性。多个来源同时遗漏日志且保持原root时可以拒绝；来源同时伪造receipt、root与其他头字段时仍须独立可信头/共识或执行参考。所有报价继续保持INDICATIVE，生产与VERIFIED准入仍关闭。


receipt根验收进程在导入fixture/harness前固定全部相关工具、参考和离线payload hash，结束时再核对不变；报告保留执行时input_hashes。审查记录绑定同一组hash和三服务源码摘要。最终门禁同时对照执行、审查与当前输入，不能在工具改变后仅重写新hash复用旧进程结果。门禁和进程wrapper在fixture/harness导入或验收输入读取前先写本次未通过状态，异常时覆盖为FAILED；只有本次全部验证成功才原子替换为成功报告，各报告携带attempt_id。


<a id="solana-current-account-shards"></a>

## Solana同finalized context的当前账户分片

对应原§8.12、§8.15、§28.22、§28.26。正常RPC当前baseline在依赖并集≤100时保留原单批路径；>100时只允许finalized，同一提供方顺序读取最多32片，每片最多99个非Clock依赖加一个Clock（最多35次RPC调用：genesis、32片、首末两次block锚点）。依赖清单在采集前按协议固定并去重；每片的返回位置/数量必须匹配请求，所有context.slot必须完全相同；不使用minContextSlot、不追赶变化slot、不从不同来源拼接依赖。

每片Clock必须是40字节、Sysvar owner、不可执行且slot等于context；完整Clock账户必须逐片一致。首片之后读取并验证该slot的block hash/parent/parentSlot/time，全部依赖解析通过后再复核锚点；归档CPMM单独或混合scope同样执行全部分片屏障。Oracle null只保留原Orca解码器已明确允许的未初始化形式，其他缺失仍拒绝。任何失败均不返回部分Snapshot；尝试备用提供方时从genesis与第一片重新开始，旧暂存状态不复用。

所有账户响应body（包括重复Clock及JSON envelope）累计占用既有journal.max_payload_bytes/Rpc.max_bytes预算，在网络chunk进入buffer之前按剩余预算拒绝；不是每片各获得一份完整预算。WAL仍在每个有界完整响应JSON/账户解码前sync记录；SOURCE_WAL失败不能切换提供方绕过。现有服务的整个baseline超时覆盖全部分片与提供方，不按分片重置。

输出source为RPC_SAME_FINALIZED_CONTEXT_SHARDS。它只说明同一来源声明的finalized context和局部屏障一致，是当前bootstrap，不能独立把任意账户字节绑定到blockhash，不证明历史版本、启动快照到连续流、无遗漏fork或严格slot执行。quote仍为INDICATIVE，Yellowstone、版本化账户、EnsurePoolScope和生产准入仍未完成。若slot持续推进无法收齐，失败封闭而不伪造共同slot。

多片轮询会按现有WAL最近method/params去重语义记录变化请求，保留容量仍由既有预算约束；未实现归档轮转，到达容量后失败封闭。当前32片上限不代表该范围在真实供应商延迟/slot推进下必然可恢复，也不代表物理内存、磁盘或生产吞吐验收。


分片验收入口为`make -C integration account-shards-regression`：先运行三服务/公共协议check与正常/debug构建，再固定5个可执行文件及服务源码摘要，之后执行普通三进程、现有Solana协议/WAL和新分片共10个受影响进程范围。`account-shards-evidence-check`核对构建绑定、执行时与当前二进制/源码一致，以及输入hash、报告时间、审查关闭和完整案例集合；不把之前EVM全范围报告视为本轮重新执行。

每次恢复及重启后，验收同时核对owner的16个池与bank_slot、两个路由副本的完整池数和池ID集合；每笔报价的整个SnapshotRef及onlineGeneration必须与当前owner一致。scope_hash只表达池成员范围，不能单独证明同一bank。链间隔离使用显式失败条件，不依赖Python assert；旧bank但正确金额/池集合、部分scope、重复池ID、二进制替换、执行后重贴源码标签和优化模式503均有拒绝反例。


执行清单覆盖全部10个回归生产脚本、共享Python辅助/测试、所有integration/test输入及PostgreSQL schema；在批次开始前与检查后生成的源码/5个可执行文件绑定。全部进程结束后将每份报告hash封存到同一执行清单，要求报告生成时间位于批次绑定与封存之间。更改任一生产脚本、报价参考、数据库schema或批后重写报告都需要重新执行，不能通过补写新hash复用旧成功结果。

<a id="balancer-weighted2-static"></a>

## Balancer V2不可变双币权重模型

`balancer-v2-weighted2-equal` / `balancer-v2-weighted2/equal-1`保持仅50/50。
`balancer-v2-weighted2-static` / `balancer-v2-weighted2/static-1`使用同一
`WeightedState`（protobuf field27），接纳两个权重各至少1%、总和恰为1e18的
不可变双币模型，包括50/50。其他配置/适配版本交叉组合失败封闭。
权重、缩放与token顺序来自同一blockHash的原池读取；配置中的标准非BPT策略
是运营方声明，代码哈希固定也不替代源代码、immutable编译产物与部署映射核验。

兑换按原BaseMinimalSwapInfoPool/WeightedMath执行：先缩放余额，在原始输入上
向上取整扣费后缩放净输入；检查净输入不大于缩放输入余额的30%；权重比向下取整，
base向上取整，再应用FixedPoint.powUp。指数ONE/TWO/FOUR保留原特殊分支，
其他指数使用整数LogExpMath和原误差上界。精确输入不施加30%输出限制。
Vault后余额加入gross原始输入、扣除raw原始输出，并限制uint112；中间值限制uint256。
签名对数/指数除法和余数均向零截断，不使用浮点或数学库近似。

一个物理池只能配置一个profile。static包含50/50并不允许同池用两个标签同时注册；
适配版本变更影响adapter-set身份，必须走现有新基线/范围切换流程。应先部署支持新版本
的消费者，再启用static配置；旧消费者会拒绝不认识的新profile。

参考运行固定于e281ff40583c39bf1d46c322334b2b13936e0e33：新静态权重参考另写
`weighted-static-*`文件，保持既有`weighted-*`向量字节不变。参考执行原构造器、onSwap、
FixedPoint/LogExpMath和BalanceAllocation；Vault facade不执行真实ERC20转账、完整Vault
或历史部署核验。报价只允许INDICATIVE，VERIFIED/build保持拒绝。多币、Managed、Composable、
BPT/外部RateProvider及V3需要另外的模型准入与执行参考。功能完成情况见根目录开发进度文档。

<a id="multi-asset-and-advanced-protocols"></a>

## 多资产与高级状态接口

多币池使用`MultiAssetState`（PoolState field28）保存完整、有序的token/balance/scaling
向量，Pool的token0/1及reserve0/1是前两项的兼容字段，必须逐项相等。路由边携带
`from_token`与输出token，所有边指向同一个物理pool_id；执行一步后更新完整余额向量，
共享池分配从上一步状态重算。副本拓扑比较包含完整token向量，后续资产变化会重建图。
同一个Pool只允许一种具体状态，不能混合旧V2/V3/Stable或高级状态。

`balancer-v2-weighted-multi/static-1`接纳3至20币不可变模型。
`balancer-v2-managed-snapshot/no-bpt-fees-1`接纳2至38币当前权重，读取同块
getNormalizedWeights/getSwapFeePercentage/getSwapEnabled及协议/管理swap fee；
后两项必须为零，权重不得重新归一化。`balancer-v2-composable-regular/no-rates-1`
保留完整BPT库存，仅接纳零rate provider的underlying间交换，BPT不生成路由边。
其原StableMath单token BPT内核需要外部提供结清费用后的实际供应量，不能当作完整BPT报价。
以上读取都校验注册类型、代码pin、全token精度/缩放和现金，拒绝非零asset manager/managed资金。

`curve-plain3-basic/factory-1`与`curve-plain4-basic/factory-1`按原Factory Basic布局读取
`2*N+7`起始的rate_multipliers。NG和Crypto有独立整数内核及原exchange参考，
NG必须使用自己的D舍入和offpeg fee，Crypto必须保留gamma/价格与利润状态；它们不是Plain的别名。
Meta underlying、旧Lending存取款及其他实际模型需要各自执行状态。NG标准资产EMA与Crypto2标准settled交换的完整状态分别使用其独立profile；其他旧profile继续保持原有边界。

Orca与Raydium奖励profile使用三槽reward_infos，state tags分别50和64，tick增长tag20。
未初始化槽允许原始零Pubkey哨兵，但一般业务账户仍拒绝零地址。Go/Rust保持同一宽度、
奖励时序和深拷贝语义。经典SPL/static费奖励模型不意味着Token2022/adaptive/订单或奖励领取已支持。

`SingletonAdvancedState`（field29）保存不可变key_fee、live LP fee与起始block hash，
必须与外层SnapshotRef同一hash。高级版本允许native currency0和不会执行swap回调的hook权限，
带before/after swap或return-delta能力的hook继续拒绝。配置和运行时使用同样的身份/能力校验。
Infinity Bin单活跃bin内核独立于CL；缺少相邻bin或manager/tree证明时拒绝跨bin报价。
上述接口不提供真实native结算、任意hook VM或VERIFIED证明。功能完成情况只记录于根开发进度文档。

<a id="curve-ng-standard-state"></a>

## Curve StableSwapNG 标准资产状态与完整交换

独立协议 `curve-stableswap-ng`、adapter `curve-stableswap-ng/3332cd44-standard-1` 固定
curvefi/stableswap-ng revision `3332cd44656ec64b3c048885e1dd71955254b262` 的
`contracts/main/CurveStableSwapNG.vy`，编译器为 Vyper 0.3.10。准入为 2–8 币、
asset type 0、decimals 0–18 的固定精度标准 ERC20；时间和非零 EMA window 限定 uint64。
原数学 API 的其他 rate 探针不代表服务准入。

`MultiAssetState.curve_ng`（字段9）保留 stored/admin/effective 余额、packed price/D、
packed MA times、两个 EMA window、offpeg multiplier，以及源区块 number/hash/timestamp。
每币满足 `stored = effective + admin`；完整快照中的源身份必须与外层 BlockRef 一致。
NG 状态只能出现在本 profile，旧 Plain/其他多资产 profile 拒绝此负载。

Go reader 使用同一 blockHash 读取池/Token code、公有状态和私有 storage，随后复核区块锚点；
正常区块读取仍经 receipt 完整性校验。准入同时检查配置实例 code hash、原编译可执行前缀
21,491 bytes 及 immutable suffix 2,144 bytes。immutable 的币序、类型、rate multiplier、
rebase 标志及外部 oracle/call amount/scale factor 均与标准资产约束一致。
未知模板、clone、rate provider、rebasing 或不一致依赖使来源失败。

报价保留原合约 uint256 检查、整数迭代、动态费用和 admin fee 舍入。oracle upkeep 使用
原 exchange 中扣费前的求解 xp 与 D，随后更新 raw/admin/effective 余额和 packed oracle。
EMA 分别处理 price/D 时间戳；signed exp 包括原 unsafe wrap、算术位移和 SDIV 语义。
同物理池的后续 allocation 使用完整返回状态重新计算；大额溢出允许搜索继续探测较小分配，
每次探测仍执行状态校验和完整计算。

参考语料来自原 Vyper exchange 实际调用，Token mock 执行标准余额转账；额外 helper 仅将
原 `exp/_get_p/get_D` 的可见性改为 external。生成器在加载依赖前撤销旧成功凭证，失败退出
保留失败状态。`curve-ng-state-verify.py --recompile` 复核固定源码、编译产物、完整布局、
部署 runtime/immutable、依赖 hash 和语料；服务 wire 由独立 protoc 编码。

进程回归属于冻结于 2026-01-01 的历史合成 RPC replay；测试专用配置放宽 freshness，
不修改原 oracle 时间。该参考环境允许超大合约体积，其结果不证明公网部署对应、真实链共识、
特殊 Token 行为或可执行交易。INDICATIVE/VERIFIED、build 和独立交易仿真沿用既有能力门槛。
项目进度、数量及验收结果只记录在根 `DEVELOPMENT_PROGRESS.md`。

<a id="managed-bpt-fee-reference"></a>

## Managed普通交换的非零BPT费用：参考模型与准入边界

固定 `balancer-v2-monorepo/e281ff40583c39bf1d46c322334b2b13936e0e33` 的
`ManagedPool._onSwapGivenIn/_payProtocolAndManagementFees`。独立
`protocol-math::balancer_managed_fees::quote_exact_in` 记录完整原始余额、总BPT供应、协议与管理收款方余额。
协议份额最大50%、管理份额最大100%、swap fee按原版本最大80%。同一收款方的两个观察字段描述
同一个账户，后状态一并增加总mint；不同收款方的初始余额之和不得超过总供应。

费用计算使用原回调选中两币的部分不变量，不能替换成所有币的不变量或raw Vault最终余额。
输入先扣原始费用再upscale；用于费用不变量的gross输入由scaled net通过原divUp(1-fee)重建。
输出在downscale前用于费用不变量，因此raw输出为零也可能产生BPT费用mint。
`pow_down`沿用原FixedPoint的ONE/TWO/FOUR分支及LogExpMath误差扣减。
先算总费用份额，再按原向上舍入拆协议费，剩余管理费；供应和余额更新执行uint256检查，
Vault facade使用原BalanceAllocation检查uint112 cash，任何错误返回失败且不暴露局部后状态。

`MultiAssetState.managed_fees`字段10是独立状态契约，保留供给、费用、收款方和block number/hash/time；
跨语言校验、wire/clone与外层来源身份一致。固定区块BPT依赖读取组件检查recovery/delegation bool、
pool与Vault collector一致性及完整balanceOf，收款方相同只读一次余额。

该固定旧源码的可执行runtime超过EIP170上限；包括上游编译器在内的已保留编译实验仍超限，
标准大小限制环境实际创建交易失败。完整原回调参考只在显式`--reference-only`的隔离环境运行，
生成/验收凭证包含`service_admission=false`。正常配置和Go parser返回
`MANAGED_FEE_RUNTIME_UNAVAILABLE`；公共数学分派不开放此profile。独立状态契约或导出数学API
不等于正常服务支持，须另固定可部署真实版本、模板/immutable和完整依赖后再接在线链路。

原初始化仅用于建立真实BPT供应与收款余额；join/exit、AUM收取、BPT路由、实际Vault/Token完整交易
及真实部署未由此参考证明。隔离facade只负责调用原接口和记录原始cash，不替代原Managed手续费逻辑。
后续进度和验收结果仅记录在根`DEVELOPMENT_PROGRESS.md`。

<a id="curve-metausd-direct"></a>

## MetaUSD标准meta/3CRV直接兑换与只读base依赖

独立版本 `curve-metausd-direct/5272fd2b-fixed-base-1` 固定curvefi/curve-factory
revision `5272fd2b1199f185d9cd94822be9262c387dc2c2` 的 `MetaUSD.vy` 与Vyper0.3.1。
实现runtime20060字节，SHA256 `c5351fd332fdf0f04486a838b2bceba15d7a45965e3df61e9a0ccedc674d450d`。
只接受EIP1167标准clone，clone内实现地址、配置code hash和固定原实现runtime必须一致。
原构造器设置fee31337锁定实现，参考通过实际clone/initialize建立池。

币序固定meta token在0、3CRV在1，meta精度0–18、LP精度18；token policy为`standard-base-lp`。
`MultiAssetState.curve_meta`字段11记录implementation与code hash、固定base pool与code hash、
当前base virtual price、effective/raw/admin余额以及来源block number/hash/timestamp。
每币满足raw=effective+admin；快照来源与外层BlockRef一致。配置明确要求base code证明，
旧profile拒绝该字段/状态，Meta拒绝混用其他协议的附加状态。

正常Go reader在同一个blockHash且requireCanonical上下文核验clone、实现、两个Token与base的代码，
读取coins、decimals、私有slot11 rate multiplier、A_precise/fee/admin_fee、base get_virtual_price、
有效balances、Token balanceOf(pool)和admin_balances。完整区块读取保留receipt及读后重锚定。
标准Token政策是操作方准入断言；代码hash和合成RPC不替代真实部署审核或通用Token行为证明。

直接`exchange`采用原Meta整数公式，rates为[meta rate multiplier, base virtual price]。
报价返回完整effective/raw/admin后状态并在共享池后续allocation中重算。原合约可成功产生dust零输出，
服务数学分派将其拒绝为INSUFFICIENT_OUTPUT；没有有效路由时API沿用404/NO_ROUTE。
`exchange_underlying`需要base deposit/withdraw/exchange及相应后状态，不由此直接模型提供。

当前base依赖为只读快照：配置、快照、建图、路径和allocation入口拒绝固定base3pool同时作为
可变物理池。多个Meta池必须对base VP/code、LP code及来源银行完全一致，否则拒绝。
这使本地meta/LP直接兑换的base VP在同一计划内保持有效；需要改变base的联合计划仍未准入。

原参考在合约大小限制开启的隔离EVM中部署原实现、克隆并初始化，通过原add_liquidity提供初始余额，
执行原exchange与实际标准Token余额转移。固定base地址安装supplied-VP mock，LP及基础币也是独立mock；
因此原Meta执行已对照，实际3pool虚拟价格算法、真实3CRV部署和完整真实交易尚未由此证明。
历史合成RPC重放保持2026-01-01原时间，仅测试配置允许较长freshness。VERIFIED/build门槛保持原状。
所有项目进度和验收状态只更新根`DEVELOPMENT_PROGRESS.md`。


<a id="curve-crypto-complete-state"></a>

## Curve Crypto2 完整定价与 LP 账本

`curve-crypto2-full/d7d04cd9-standard-1` 固定 `curvefi/curve-crypto-contract`
revision `d7d04cd9ae038970e40be850df99de8c1ff7241b` 的 two/CurveCryptoSwap2，
以及同 revision 的原 CurveTokenV4。池模板、五个 immutable word 和 LP runtime/minter
都参与准入。配置使用 `multi_asset.expected_lp_token` 与
`multi_asset.expected_lp_token_code_sha256` 绑定 LP；不能将任意 LP 合约的
`mint_relative` 行为当作原版本。

`MultiAssetState.curve_crypto` 使用 protobuf field 12，包含 gamma、动态费参数、
D/price scale/oracle、利润与虚拟价格、时间、调价标志、LP totalSupply/admin receiver
余额、实际 ERC20 raw balances 以及源区块。有效内部余额继续放在完整物理资产向量，
scaling 为 `10^(36-decimals)`；Crypto precision 为 scaling 除以 `1e18`。
所有 RPC 依赖固定在同一个 canonical blockHash，发布前再次核对锚点。

普通服务只接受标准 ERC20 的 settled A/gamma（future time 为 0 或完成标志 1）。
标准代币策略是运营准入断言，不能由一个 code hash 推导出任意代币都没有回调、税费或重基。
数学 API 另可接收完整 ramp origin；缺失在线 schedule 时服务拒绝，而不省略 ramp 变化。
每次交换传递价格调整、admin LP 铸币、donation gulp 与全部余额/供应后状态；
共享池拆分继续使用前一步完整状态。如果管理员LP mint的收款方是另一候选池，
且该池持有此LP资产，快照/图/路径/allocation统一拒绝未建模的跨池raw余额变化；
零admin fee与外部收款方保持可用。旧 Crypto API 保留其调整/claim 边界错误。
公开报价仍为 INDICATIVE，独立交易仿真和 build 的准入规则保持关闭。

<a id="priority-extension-kernel-boundaries"></a>

## Orca adaptive 与 Infinity 多 Bin 的状态边界

Orca 的 `orca_adaptive` 是固定 `cc683ec9` 版本的逐 group 费率管理器。
`begin` 后，每一步先更新 accumulator，再取得 fee 与 bounded target；按 skip 状态选择
推进方式，整笔交换结束再更新 major timestamp 并保留 Oracle 状态。
完整 adaptive swap 使用独立 `orca_adaptive_swap` API 和完整 fixed-array 数学快照；
该 API 的原程序对照不提供账户身份认证、Token2022 或 SBF 交易保证。
逐步 steps 是本地审计信息；独立 native 对照比较兑换量和完整后状态，未提供独立原程序 step trace。
旧 static/rewards profile 继续拒绝 initialized Oracle。独立 adaptive profile 的账户、来源和报价范围见下节。

Infinity 的 `infinity_bin_multi` 使用完整初始化 bin 映射与全局树边界，
保留耗尽节点，并按严格前驱/后继跨越空档；逐 bin 扣费、储备、协议费及 active ID
都进入后状态。缺流动性、宽度越界或步骤预算耗尽时整笔计算失败。
完整映射是调用方声明，不能把局部窗口冒充整棵链上树；manager/tree固定块读取与静态零hook标准代币普通服务已在独立Bin版本接入；
hook return delta、override 和实际 native/unlock/settlement 尚须单独接入和验证。


<a id="orca-adaptive-online"></a>

## Orca adaptive 独立服务版本

`orca-whirlpools/cc683ec9-spl-adaptive-1` 使用 `orca-whirlpools-adaptive`
协议标签，固定此前的原程序源码版本。它采用经典 SPL、三份连续 fixed tick arrays、
三个未初始化 reward slot。初始化奖励、dynamic arrays 和 Token2022 的组合继续拒绝。

Pool PDA 的最后一个 seed 是独立 `fee_tier_index_seed`。自适应池满足 fee tier index
与 tick spacing 不同；static/rewards 仍要求相等。Pool/Oracle/tick-array PDA、池关联、
Program/ProgramData 部署 slot 和代码摘要、经典 Token 程序、mint/vault、Clock 均在
同 finalized 账户批次读取，发布前再核对源 block。Clock timestamp/slot 必须与该
snapshot bank 一致。配置中的 code hash 是部署准入断言，不能替代实际程序审计。

`OrcaWhirlpoolState.adaptive` 使用 protobuf field51，不修改已保留的32–49。
它保存254-byte原Oracle中的pool绑定、trade gate、全部fee constants/variables，
以及16/16/128字节保留数据。周期和阈值按原u16宽度验证；时间不能超过bank时间，
volatile变量/分组必须处于原模型支持域。保留数据参与JSON/protobuf和clone，不丢弃。

普通报价调用已有完整adaptive exact-in内核，传递Oracle、tick/liquidity、全局和
outside fee growth、protocol owed、raw vault余额和reward timestamp的后状态。
内核的部分成交不能直接成为普通服务报价：remaining必须为0且输出为正；数组证明不足、
预算耗尽、vault不足或宽度溢出均返回错误。共享池重算沿用前一步完整后状态。

独立参考通过原程序pack和原swap_manager/update_after_swap生成账户与结果。
ProgramData/Token代码、RPC finalized声明与raw vault名义转账是本地合成来源；
这些测试不等于线上SBF部署、Token CPI或完整交易收款验证。公开报价仍为INDICATIVE。


<a id="infinity-bin-online"></a>

## Infinity Bin固定版本完整树服务

`pancake-infinity-bin/3c5ef1be-static-1` 是独立Bin模型，静态LP费率不超过100000，
零hook，binStep为1–100，标准非零ERC20。物理身份为 `manager/rawPoolId`；
PoolKey按原6word ABI绑定currency0/1、hooks、manager、fee及parameters。

准入固定Solc0.8.26/viaIR/Cancun/原optimizer设置的Manager模板23852字节，
仅归一化编译器声明的5处Vault immutable；Vault运行码9247字节必须匹配固定模板。
配置的部署摘要还需与实际同blockHash代码一致，不将代理或任意mock作为原manager。
`maxBinStep`只限制初始化；降低当前上限不禁用已有池的静态交换。

`PoolState.infinity_bin` 使用field30。原 `pools` mapping slot5 下的State偏移为
slot0=0、reserve mapping=1、share mapping=2、root=4、L1=5、L2=6。
Parser枚举root的全部set bits、全部L1/L2和真实初始化bin，交叉核对getBin和
storage/share、双向getNext。树节点与bin数量分别受预算限制，超预算或缺子节点拒绝。
耗尽但仍初始化的bin保留成员身份与share，不能丢弃其反向兑换能力。

原burn在剩1000份锁定share时会移除树成员，可能留下active ID与非零舍入dust。
`detached_active`（state field29）单独保存这个树外active：必须id匹配active、
share为1000且不重复入树；可接受只剩树外active的空树。报价先处理初始active实际储备，
其后只沿真实树前进；跨走后不将被移除节点重新加入后续遍历。Clone、wire均保留该状态。

Vault `reservesOfApp(manager,currency)` 和 manager `protocolFeesAccrued(currency)`
是共享依赖。Snapshot/图/路径/分配入口拒绝冲突，同一步后将两个货币账本传播到
相同manager的其他物理池；每池的真实树/储备仍独立。app储备按uint256 checked变化，
协议费按原manager unchecked uint256累加。输入输出须全额、正输出且满足vault app储备。

独立参考部署原manager/Vault并执行getters，使用原TreeMath/BinPool独立执行交换及burn。
本地状态通过隔离storage投影构造，Shanghai参考未验证Cancun Vault transient lock及
完整manager结算；metadata代币也不构成ERC20转账证明。普通报价保持INDICATIVE，
未知hook/动态override/native完整执行、VERIFIED/build和生产准入继续关闭。

## 独立 EVM 本地状态执行

`TransactionContext`支持可选的十进制字符串`max_priority_fee_per_gas`。存在时，`gas_price`表示EIP-1559的max fee，二者原样传给REVM；省略时保留既有legacy语义和JSON承诺。cap低于执行块base fee、tip超过cap或最大预付资金不足均拒绝。动态费用结果增加`native_fees`，记录同次银行调用者/coinbase原生币pre/post、实际effective gas price、execution fee、base fee burn和priority fee；这些字段进入receipt承诺。caller等于coinbase时，其净扣款包含返还给自己的tip，不能按净扣款再次收取tip。该结果只覆盖给定Shanghai银行的执行费用，L2 data fee和当前链资格独立验收。

现有 dex-router 可配置 `simulation.backend: revm-fixture`。`state_file` 指向严格
`LocalSimulationBundle`：显式账户/code/storage/blockhash、源块 H 身份、实际 H+1
执行环境、固定 executor 身份和 transaction_context。账户/slot 缺失不会默认为零；
本版本限定 Shanghai、单 hop 原 Router02、标准 ERC20 ExactIn。未知组合继续拒绝。

通过已认证 `POST /v1/quotes/{quoteId}/simulate` 执行已缓存 FAST 报价，body只接受空或
`{}`；caller、recipient、交易和状态不得在此覆盖。服务启动时捕获不可变本地状态，
更换文件须显式重启并重新获得当前报价。同二进制私有 worker 通过stdin/stdout运行，
独立并发、gas/read/byte/instruction/memory预算和父进程硬超时约束，不增加服务目录。
结果保存实际输入/收款余额差、退款、gas、精确交易/执行环境和来源/worker绑定。
`authority=PROVIDED_LOCAL_STATE`、`verified=false`；费用完整性为`LOCAL_EVM_ONLY`。

原参考使用发布的 Uniswap V2 Factory/Pair/Router02/ERC20实际部署及交易，不以报价
数学替代独立执行；来源与包完整性位于`integration/tools/reference/evm-execution/`。
在integration中运行`make evm-execution-reference-check evm-execution-regression`，
随后`make evm-execution-smoke evm-execution-acceptance`。验收需要先通过全服务
`tools/acceptance/verify.py`及当前输入的两阶段审查；开发状态只维护根进度文档。

`xone_simulation::prepared_evm::simulate_prepared`提供显式银行的准备组件执行入口。它根据bound plan、资金观察和费用上下文重算完整组件，任何字段、RPC参数、unsigned wire或资格标记不同均拒绝；资金观察再通过同次REVM银行实际读取native余额/nonce、sell余额和allowance核对，使用原预算。执行结果分别保留原组件hash、原unsigned字节SHA256和原simulation receipt，再计算外层commitment。此入口限定ExactIn和PROVIDED_LOCAL_STATE，执行时间是显式H+1环境；普通HTTP请求的时效和QuoteView仍由API原检查负责，不能据此授权VERIFIED。所有动态费用只读探针同时传递cap和tip，使GASPRICE与实际swap相同。

本地仿真API的`fees.gasPriceWei`在动态交易中表示实际effective price，`fees.evmFeeWei`为原REVM执行费；`transactionContext.gas_price`继续保留max fee。响应的`executionBaseFee`来自送入worker的显式执行环境。父进程核对native receipt的有效价格、base burn和tip分解，缺失或矛盾拒绝，不能把最大预算当实际扣款。`chainFeeCompleteness=LOCAL_EVM_ONLY`不因本地执行成功改变。

`evm_fixture::simulate_bound_exact_out`接受原Router02完整ExactOut绑定及给定H/H+1状态，只准入单条单物理池路径，执行原`swapTokensForExactTokens`。成功匹配同时要求实际输入等于计划required_input、实际收款等于target_output；nonce、gas、native费与完整后状态仍来自相同REVM执行。可选`unused_input_budget`为max_sell_amount减实际消耗，表示留在调用者账户的未用额度；`refund=0`保持未发生退款转账的事实。普通ExactIn结果省略该可选字段，旧承诺结构保持。多池路径、当前链资格和普通ExactOut HTTP独立执行需分别接入并验收。

## 原 Uniswap V2 完整 Swap 状态版本

`uniswap-v2-complete` / `uniswap-v2/core-1.0.1-full-swap-1` 使用原 core 1.0.1
Pair/Factory 固定 runtime，标准 ERC20、3/1000 费率；配置仍使用现有 `v2` 身份字段。
它与旧 reserve-only 版本分别准入。`V2State.swap` 位于 field20，旧 reserved 3–19
保留；旧 profile 拒绝扩展，新 profile 要求完整 raw 余额、储备 LastTimestamp、
累计价格、锁及源 bank。可合法为零的三个时间/高度字段有 protobuf presence；
未知值不能默认成零。源 bank 仍指原 H，派生状态不会生成新 canonical 身份。

普通路由按当前 bank 时钟复用完整后状态；`v2_complete::quote_transition` /
`settle_swap` 接收显式执行时钟，输出原 Pair 的双侧输入/输出与完整 Swap 写集。
捐赠保留到真实 raw 储备，K、SafeMath、uint112 容量和累计价 wrap 按原协议处理。
多步同计划使用同一时钟；独立未来 clock 的参考用例不代表未来 head 获准或保证成交。
Mint/Burn/LP供应、特殊 Token 及其他 V2 fork 均不由此 profile 自动支持。

现有本地独立执行接口可绑定此版本，执行前核对完整 H 状态，并在执行后观察
真实 `getReserves`、两侧 `balanceOf(Pair)`、累计价格与 slot12。storage 观察明确
使用 `type: STORAGE`、`slot` 和零 EVM-call 使用量，并计入 read/byte/取消预算。
这些观察来自实际 overlay。旧 call 观察的 JSON 形状不增加字段。

集成命令：`make v2-complete-reference-check v2-complete-wire v2-complete-regression`；
完整构建验证后运行 `make v2-complete-smoke v2-complete-acceptance`。原参考包括
24 个公开操作案例、4 条序列与 79 份原始 trace；独立 C++ wire fixture 另外包含
明确标记的 schema-only 已知零值例。机器证据在 integration/artifacts，进度仅记录于根文档。

<a id="managed-v2-published-aum"></a>

## 实际发布 Managed V2：完整钱包、AUM 与共享 Vault

`balancer-v2-managed-aum` / `balancer-v2/20230411-managed-aum-3` 对应固定的
`balancer-deployments` 发布任务 `20230411-managed-pool-v2`。原参考保留原 build-info、
两种优化参数的编译 job、库链接、immutable、正常 EIP170 部署、原 Factory/Vault/ERC20
公开操作和完整执行观察。旧 `e281ff40` 费用参考因 runtime 超限而被正常服务拒绝的边界保持不变。
原参考 metadata 的 `aum-1` 是历史生产者版本；公开 `aum-3` 必须携带新增完整钱包和两套暂停字段，
旧公开状态不能通过省略字段升级。

Managed 状态区分每个物理池的注册 cash/managed 与 Vault 全局实际钱包。virtual supply 只减
该池注册的 BPT，actual supply 包含当前有效时钟下未收取的 AUM。普通 underlying→underlying
GIVEN_IN 先 upscale 再扣交换费；它更新原 gross 输入和 raw 输出对应的现金及实际钱包，
不会铸造管理费 BPT 或推进 AUM 收取时间。显式 collection 采用原顺序的年度向下取整、
时间比例向下取整及协议份额向上取整，收款方相同或与 Vault 别名时按实际地址记账。

Pool 和 Vault 各自保留 effective paused、pause window 与 buffer 截止时间。普通交换分别检查
两套限制；Pool 的公开 AUM collection 不会仅因 Vault 暂停而被本地误拒绝。Recovery 中原
`getManagementAumFeeParams` 返回有效零费率，未收取 AUM 为零；恢复正常时重新校验配置中的
常规费率。控制操作的原来源观察与本地 swap/collection 数学转换分开验证。

所有计算转换保留来源 H，另带 `execution_context` 保存完整执行高度和时钟，即使计算恰在
来源 H 的同高同时也标为派生。派生 Pool 可用于继续执行，但不能作为 canonical Snapshot
或通过普通状态 wire 发布。Vault 历史变更高度按原 packed uint32 语义保留，完整执行高度
另存，避免回卷后丢失上下文。required scalar/Amount 缺失、未知字段、超宽整数以及无效
账本不能默认为已知零。

多个 Managed 物理池共享同一 Vault/token 时，公共领域层校验相同来源、Vault 暂停与代码、
角色及 provider 实时参数、token 身份和实际钱包的一致性。钱包至少覆盖去重后已声明注册的
cash 合计；managed 资金不计入 Vault 现金下界，未知注册仍不因此被宣称已完整覆盖。
重复 physical ID 的冲突声明必须拒绝，完全相同的重复声明不重复计算现金。

路由每步把实际钱包变化传播到同 Vault 的相关兄弟池，保留它们自己的注册余额、权重、
费用和 BPT 供应量；整条 path 成功后才提交候选状态。已知 BPT underlying 或混合适配器
组合通过明确原因隔离相关 Vault 组；来源银行保留全部记录，图只发布可执行边，API
`candidateScope` 表示实际可执行候选，`snapshotRef.scope_hash` 仍绑定完整来源范围。
无关 V2 或不同 Vault 的正常路径不应因另一组尚不支持而消失。

本地验证入口：

```sh
make -C integration managed-v2-catalog-reference-check
make -C integration managed-v2-shared-reference-check
make -C integration managed-v2-multi-differential-test
make -C integration managed-v2-multi-quote-test
# 需先完成当前源码的完整 check 并绑定独立 SPEC/quality 审查
make -C integration managed-v2-multi-acceptance
```

当前原目录包含 2/3/5/50 币池的共同 H 读取和原交换/非零 AUM；共享目录包含同 Vault 同 token
双池的 A/B/A/B 真实交易及独立 BPT 账。实际选中 API 计划由新的原 bytecode VM 恢复来源后按
顺序执行，检查真实扣款、收款、原 getters 和交易记录。此证明仍限定本地参考，
`VERIFIED` 和 public build 保持关闭。活动 schedule、breaker、BPT swap/join/exit、rate/mixed
完整能力、可信真实链与生产费用和性能继续按设计另行实现。最新状态与验收绑定以
[开发进度](../DEVELOPMENT_PROGRESS.md#plan-2026-11-03-managed-v2-aum) 为准。


### 活动权重/费率 schedule 的独立版本与来源资格

`balancer-v2-managed-scheduled` / `balancer-v2/20230411-managed-scheduled-1`
在完整 Managed 状态上增加可选 field40。新 profile 必须携带来源 H 的 pool slot9
原 word，以及按有序 underlying 地址在 mapping11 读取的 token words；旧 `aum-3`
仍拒绝 field40 和活动 schedule，不能通过换标签升级旧状态。

pool word 使用四个直接 uint32 时间、两个直接 uint62 费率及最高四个 flags；token word
使用两个直接 uint64 权重和 uint5 decimals difference，其余高位必须为零。整数插值遵循
原合约加减及截断顺序，直接使用被选中的两个权重计算比率。合法 current weights 及管理
重排取得的 computed start 不强求合计 ONE；caller 的目标 end 数组仍严格要求合计 ONE，
不能归一化或增加误差容忍来掩盖单位差异。

`reclock` 保留 sourceH/raw schedule，并按显式、单调的 execution context 重算费用、权重、
AUM 和两套暂停视图。同 Vault 的兄弟池各自重算自己的时钟视图，即使未共享 underlying
也不能留下旧 clock；传播全局钱包时仍保留各池自己的注册资金和 BPT 供应量。派生状态
继续拒绝 canonical Snapshot/wire。

来源资格必须同时绑定原 compiler AST、完整 runtime、库链接、全部 immutable、公开
getters 和原创建参数。身份地址不得重复或遮盖已固定的 prefix 身份；runtime 先校验
canonical hex 再计算摘要，Factory creation pair 和 Vault WETH 必须匹配原固定观察。
原 `ManagedPoolSettings` 构造函数固定 SWAP provider ID 为 0、YIELD 为 2，只有 AUM ID
来自参数；生产 parser 和参考校验器均拒绝 getter 与伪造 immutable 共同改变这些常量。

验证入口（状态和最新证据只记录于开发进度）：

```sh
make -C integration managed-v2-scheduled-projection-check
make -C integration managed-v2-scheduled-wire-check
make -C integration managed-v2-scheduled-helper-test
make -C integration managed-v2-scheduled-differential-test
make -C integration managed-v2-scheduled-smoke
```

原参考仅覆盖注明的双币/三币、活动重排和单物理池 signed swap/collection。
真实 API 选中的原单跳由独立原 VM 重放并检查真实扣款/收款及完整观察；这不开放
public VERIFIED/build，也不提供真实链信任、完整 trie、跨物理池活动 schedule 执行、
BPT/rate/mixed 完整能力或生产性能证明。这些仍按原设计保留。


### 两个物理池的共同 H 与实际分配后状态

`managed-v2-scheduled-shared` 使用独立原参考文件，区分显式 scheduled/static 与
scheduled/scheduled 配置。每个 bank 的两个池、原代码、getters、选定 raw words、
完整全局 underlying 钱包和各自 BPT 账处于同一来源 H；同一实际调用在整个 bank
必须有相同结果。每笔原交易、管理调用及 clock 需要和前后 header、nonce、bank
及检查点一一对应，重复引用某池交易不能替代另一物理池覆盖。

普通 swap 在相同 clock 不得改变自身/兄弟池 weights、fee、packed schedule 等只读
Pool 视图；注册余额仍归各物理池，全局钱包变化同时被双方观察。clock-only 转换
分别重算每池自己的权重、费率、AUM，静态 peer 也推进派生执行上下文。

实际 API 两路计划按返回顺序执行原 Vault.swap。每 mode 一个独立拥有的原 VM，
不同计划恢复精确来源并 snapshot/revert，重复报价只有在 namespace、source、完整
request/routes 一致时明确关联同次执行。执行器同时检查真实扣款/收款、事件和完整
after 观察，恢复后再次核对完整来源。观察投影只映射 getter/raw 值；服务门禁显式
运行 Rust 实际分配向量测试，将每个真实大额步骤的两个完整 Pool 与本地连续状态
逐字段比较。该测试依赖实际 VM 报告，普通 unit 中显式 ignored，服务门禁必须执行。

```sh
make -C integration managed-v2-scheduled-shared-projection-check
make -C integration managed-v2-scheduled-shared-helper-test
make -C integration managed-v2-scheduled-shared-differential-test
make -C integration managed-v2-scheduled-shared-smoke
```

这些仍是固定原版本的有限本地来源/执行证据，保持真实链、完整 trie、费用模型、
BPT/rate/mixed/breaker 更广能力和生产准入边界；当前任务和验证结果见开发进度。

<a id="managed-v2-bpt-routing"></a>

### 发布版 Managed BPT 的完整状态与资金上下文

`balancer-v2-managed-bpt` / `balancer-v2/20230411-managed-bpt-1` 使用原
20230411 ManagedPool 的 BPT swap 语义。它在完整 Managed/AUM/schedule 状态上增加
可选 field41，显式携带 join/exit flag、固定最小锁定 BPT、BPT 注册 lastChangeBlock
以及按地址排序的有限白名单观察。旧 `aum-3` 和 `scheduled-1` 不接受该字段；新版本
不能省略它或 raw schedule。白名单查询结果的 `false` 与“没有观察此地址”分别保存，
不得将未知成员补成允许。Go clone 必须复制成员数组，三个 domain 的 wire 转换须保留
显式 `false` 和 block `0`。

新来源配置使用 `standard-bpt`，并要求
`managed_v2.bpt.allowlist_subjects` 明确列出要观察的地址。列表本身不授予权限：Go
解析器在相同 EIP1898 blockHash 读取原 `isAddressOnAllowlist` 与 AST 固定 mapping13，
核对严格的 bool storage；锁定 BPT 从零地址余额读取，join/exit flag 与原 packed
schedule 对照。固定 slot10 的 AUM word 按原 uint60 年费率、bit60 起 uint32
收取时间解码，bit92 以上必须为零；正常模式核对 getter 年费率，recovery 模式保留
底层非零正常费率而要求有效 getter 为零，收取时间始终与 raw 一致。来源解析及
原参考/实际计划 after 校验共用该布局约束。普通固定 H 来源完成后仍重锚，任一依赖或控制矛盾使整批发布失败。

精确 GIVEN_IN join/exit 先收取 pending AUM，按原 floor/floor/ceil 顺序更新总供应与
manager/collector 钱包，再用 actual supply 计算单币 BPT 数量。费用只作用于原单币
非比例部分，保持原 upscaling、幂方向和舍入顺序。swap 输出 BPT 来自 Vault 预铸
注册现金，不能再次铸币；exit 将 BPT 放回该现金。注册 BPT、Vault 全局 BPT 钱包、
virtual/actual supply、underlying 注册余额、实际钱包和 touched lastChange 均随同一
物理 Pool 转换。所有成功转换保留原 source H，写入派生 execution context；派生对象
不可作为新的 canonical Snapshot/wire。

逻辑图中的 underlying↔BPT 边均引用同一个完整物理 Pool。`Funds(sender,recipient)`
贯穿候选搜索、分配顺序重算及 HTTP 请求：中间腿回到 sender，最后一腿给 recipient；
join allowlist 检查 sender，exit 不以 allowlist 禁止退出。旧缺少 Funds 的内部入口
继续拒绝 BPT swap。处理 Managed 范围前校验资金地址为非零 canonical EVM 地址，
不能让大小写变体绕过别名判断。

同 Vault 的普通 underlying 钱包继续按物理兄弟池传播，注册资金与各自供应保持独立。
外部标准 V2 腿兑换某个已声明 Managed BPT 时，按 sender→pair 输入和 pair→该腿
recipient 输出更新发行池的已观察 BPT 钱包；该 ERC20 转账不修改发行池注册现金或
供应。其他 custody 需要独立会计适配。当前普通 Managed 腿的 Vault 自收付与外部 V2
pair 自收付没有完整结算模型，返回明确 unsupported；不能以 reserve-only 数学伪造
收款或储备。整条路径只在全部步骤成功后提交，任何后续错误保留调用方原状态。

原执行参考、投影和 synthetic wire 分开保存。实际选中 API 计划在原 Vault/Pool/ERC20
字节码中按返回顺序执行，使用真实钱包差、签名交易及 receipt 检查金额与 AUM/BPT
事件。不同计划先恢复精确来源，并在执行后 snapshot/revert 再验证来源。后续投影先
重验实际报告，再将每个真实执行块的双方完整后 Pool 与本地连续模型逐字段比较；
对应 Rust 测试在普通 unit 中 ignored，服务门禁必须显式运行。

```sh
make -C integration managed-v2-bpt-reference-check
make -C integration managed-v2-bpt-projection-check
make -C integration managed-v2-bpt-wire-check
make -C integration managed-v2-bpt-helper-test
make -C integration managed-v2-bpt-differential-test
make -C integration managed-v2-bpt-smoke
```

这些是固定发布版的有限本地来源与顺序执行验证。实际链身份、完整 trie、enabled
circuit breaker、其他 custody/rate/mixed 组合、ExactOut、生产费用及 public 原子执行
仍依原设计分别实施；`INDICATIVE` 不升级为 `VERIFIED`，public build 继续关闭。
最新开发状态、测试结果及尚未完成项只记录于
[开发进度](../DEVELOPMENT_PROGRESS.md#plan-2026-11-03-managed-v2-aum)。

<a id="managed-v2-enabled-circuit-breaker"></a>

### 已启用 circuit breaker 的独立版本

`balancer-v2-managed-breaker` / `balancer-v2/20230411-managed-breaker-1` 在
已发布 BPT/AUM/schedule 状态上增加 optional field42 `circuit_breakers`。新版本要求
schedule、BPT 控制和 circuit 状态同时存在；旧 AUM、scheduled、BPT profile 不接受
field42。每个有序 underlying 保存原 slot12 mapping 的完整 uint256 word 和六项
`getCircuitBreakerState` 观察，包含 reference BPT price、reference weight、上下百分比
及当前上下 BPT price bound。价格 getter 已按 token scaling 恢复为 manager-facing 单位。

原 word 依次编码 lower16、upper16、reference weight64、stored BPT price96、cached
lower32、cached upper32。百分比通过右移48压缩，getter 左移48恢复；upper 可以小于1，
只要满足原入口范围，`upper=1` 最小整数也可压缩成0，从而关闭该方向。lower 的原最小
输入压缩后恢复可能略低于0.1，不能据此误拒合法状态。安装word与启用某方向分别建模，
保存 reference 数据而两侧百分比均为0也仍是完整原状态。

同 reference weight 时必须使用缓存 ratio：`floor(u32 × 10 × ONE / (2^32−1))`。
不同 current weight 时，用压缩后恢复的百分比重新执行原 fixed-point power；lower 用
powUp，upper 用 powDown。原 set 输入使用尚未量化的百分比来生成缓存，因此不能用
恢复后的百分比重算并硬性要求 cached ratio 相同。边界价格先 lower ceil／upper floor，
再按 getter scaling 向下还原，禁止合并两次舍入。

解析配置显式选择 `standard-breaker-bpt`，保留 BPT 的有限白名单观察范围。固定 H 读取
slot12 与六项 getter，动态 ratio 通过同 H 的原 linked CircuitBreakerLib 纯调用独立核对，
库身份仍由完整原 runtime/template/immutable 约束。Domain 验证 raw、presence、范围和
cache-hit view；动态 pow 的完整 getter 校验在 pricing 前使用精确内核再次执行。

普通 TOKEN swap 对 gross input 侧检查 lower，对 output 侧检查 upper。BPT join/exit
先收 AUM 并计算新 actual supply，对全部 underlying 的两个方向检查，包含未参与兑换
的 token。条件使用 downscale 前的内部 scaled `amountCalculated`；低精度币种可能最终
raw output 为0，但原断路器已经应当回退。不能只根据 raw 钱包后状态重构该条件。
所有条件与定价使用同一执行时钟的 actual supply、weights 和 bounds；未来 reclock
保留原 source H，不能拿旧时钟的 breaker 状态检查新报价。

当前价格越界不一定意味着所有金额都不可行：合法 upper<1 可让小额 join 回退、较大
join 恢复到边界内。因此图保留原 Pool，候选按金额计算；失败时整条路径和共享状态
回滚。对于普通 TOKEN 同交易对，其他健康物理池可能提供合法替代路线；原单池回退
不等于全局 NO_ROUTE。完整 scope 的实际选中路线必须在原 VM 重放，而单池错误语义
由明确单池 scope 回归验证。

参考的 cached/dynamic/boundaries/shared 场景由独立顺序清单固定，控制与交换的原
signed transactions、nonce/gas/native balances、bank/clock/checkpoint 双向唯一对应。
固定清单拒绝自洽删尾、孤立覆盖记录及重复 clock。实际 API 计划另绑定独立输入摘要
和逐输入 link，after 状态再次校验 runtime/immutable/raw/control，并以实际执行块比较
双方完整 Pool。原 sequential witness 只定义资金与容量输入，不作为优化器期望输出。

```sh
make -C integration managed-v2-breaker-reference-check
make -C integration managed-v2-breaker-projection-check
make -C integration managed-v2-breaker-wire-check
make -C integration managed-v2-breaker-helper-test
make -C integration managed-v2-breaker-differential-test
make -C integration managed-v2-breaker-smoke
make -C integration managed-v2-breaker-final-check
make -C integration managed-v2-breaker-acceptance
```

该能力的开发／测试／验收状态仅以
[开发进度](../DEVELOPMENT_PROGRESS.md#plan-2026-11-06-managed-v2-breaker) 为准。
rate/mixed、其他 custody 与自收付完整结算、ExactOut、可信实际链来源、完整费用和生产
原子执行仍按原设计分别实现；public VERIFIED/build 不因本地参考通过而开放。

最终入口要求当前 `managed-v2-breaker-freeze.json` 与完整独立审查记录。串行记录绑定源码、五二进制、全部输入和 freeze 文件摘要，并保留五阶段精确命令、日志、产物摘要及执行时间；复审会拒绝旧轮次或被替换的结果。新轮次启动或 audit 失败/中断会将最终验收状态置为未通过。进度读取器也参与输入冻结；本批 CB1–CB5、六条设计映射、工作包及源码／测试绑定另生成规范化结构摘要，freeze、串行记录与最终审计均重新核对。正常状态与备注更新不改变该摘要。失败或运行中报告不能贡献 `verified_*` 计数。此入口仍只验收注明的本地 breaker GIVEN_IN 范围。


<a id="r01-r04-sources-and-recovery"></a>

## 真实来源、持续采集和范围恢复接口

Go `internal/provenance` 检查完整EVM头/交易根和账户、代码、storage MPT承诺。`cmd/source-evidence` 只读采集明确H及另一endpoint同高头，离线验证无需实时RPC；它不将两个endpoint一致称为共识或独立故障域。固定源码重建工具在编译前验证同一部署证据，核对完整源码清单、compiler build与settings、实际runtime；源码重建与独立交易收款是两项证据。

```sh
cd chain-parser-go
go build -trimpath -o bin/source-evidence ./cmd/source-evidence
./bin/source-evidence --verify ../integration/test/source/ethereum-mainnet/uniswap-v2-26355865-evidence.json
```

Go正常EVM适配器可配置`chains[].block_integrity: "ethereum-header-transactions-v1"`，仅允许`evm:1`。`Adapter.Block`通过标准`eth_getBlockByNumber(tag,true)`一次读取完整头及交易，重算头hash、交易hash/chain ID/位置和transactionsRoot；交易数据与保守对象开销共用已有块预算，整个操作受已有RPC期限约束，失败进入原正常服务撤销/耐久DLQ流程。它与后续receiptsRoot校验分别生效；承诺核对的锚点仍来自RPC，响应metadata显式`consensus_verified:false`。该profile不能用于其他链，也不能替代共识/独立故障域证明。

公开目标配置生成器为7条EVM目标默认选择 `known-evm-native-commitments-v2`，并填写显式的 `native_proof` 限界：

```json
{"block_integrity":"known-evm-native-commitments-v2","receipts_mode":"bulk-auto","native_proof":{"max_bytes":16777216,"max_headers":64,"max_window_blocks":16,"max_transactions":4096,"max_lookahead":32}}
```

该profile仅允许Ethereum、OP、Base、BSC、Polygon、Arbitrum和Avalanche对应的固定chain key。头、原始交易体、receipt及后继执行窗口的证明共用显式字节预算；具体链规则由适配器选择。普通Ethereum0–4型receipt校验保持原边界。Avalanche异步执行窗口必须区分观察到的head与已具备后继承诺的执行块；窗口内只有末执行块可引用该stateRoot。Polygon系统receipt与普通交易receipt分别保留证明语义，不能因过滤系统记录而声称全部区块日志完整。原始证明随factory/history归档，历史指定高度不得静默替换成其他已settled高度。配置生成器保留已有显式profile；非V2 profile携带native_proof、V2缺少限界或未知chain key均拒绝。实现测试及部署资格状态仍以开发进度中对应项为准。

方案§3的8链目标与公开开发RPC默认值保存在`integration/config/chain-targets.json`。配置工具自动核对chain ID/完整genesis、拒绝覆盖已有同链genesis pin或将其他网络池目录重新贴标签；8链genesis均固定并绑定留存来源文件。RPC选择优先显式参数、已有同链来源、最后公共开发默认；保留base基础设施/认证及已有同链topic，新增state路径/topic含完整genesis。HEAD_FIRST年龄上限120秒、tail上限128、history无online写权；不可能满足的raw预算组合在写文件前拒绝。实时身份探测由独立worker限制整个请求5秒，超时结束并回收worker，不留慢读线程；限制三种只读请求、响应字节和重定向。输出配置是正常解析器配置格式，`*.identity.json`只保存来源观察。指定`--identity-evidence`可基于摘要固定的留存身份记录离线生成，明确`live_probe_executed:false`，不赋予协议或生产资格。

```sh
python3 integration/tools/configuration/chain_target_config.py --list
mkdir -p integration/.runtime
python3 integration/tools/configuration/chain_target_config.py --target ethereum --base-config chain-parser-go/config/config-test.yaml --identity-evidence integration/test/source/chain-genesis-observations.json --output integration/.runtime/ethereum-config.json
chain-parser-go/bin/chain-parser-go --config integration/.runtime/ethereum-config.json --check-config
```

RPC可挂载同步耐久response recorder；原始有限响应在HTTP状态/envelope/result解码前存储，记录失败不跨provider绕过。正常Go服务通过显式 `journal.raw_evidence` 将raw/DLQ置于自己的state目录，独立配置max_bytes/max_records/max_record_bytes；不能复用Kafka尾窗预算。未配置时保留旧轮询路径且明确不宣称raw capture能力。启用后容量耗尽停止而不丢弃关键证据。并行staging只产生候选，原有owner prepared/journal依次提交完整块；worker不修改lease。WS `newHeads` 仅为HTTP查头提示，短缺口有界对齐、超窗HEAD_FIRST；工厂事件只产生候选，当前版本/状态准入后才可入目录。

两解析器提供认证的 `POST /internal/chains/{chain}/ensure-pool-scope`，请求为 `{"pool_ids":["配置目录中的ID"],"expected_online_generation":1}`。只允许已准入目录，拒绝重复/未知池、代际冲突及过期状态。正常reanchor仍由原owner链路发布，接口不自行分配generation。Rust恢复提交收敛由拥有actor的任务负责，等待方取消不会提前释放仍在提交的actor。

Raydium CPMM、Raydium CLMM、Orca 配置中的 `expected_token_program_code_sha256` 始终固定 classic SPL Token 的完整可执行 payload。旧的 immutable loader 配置保持原格式；当该公有程序使用 Upgradeable Loader 时，同一协议配置增加 `token_program_upgrade: {"programdata":"3gvYRKWyXRR9xKWe1ZjPhLY5ZJRN7KDB4rFZFGoJfFk2","deployed_slot":公开观察到的部署slot}`。部署slot由固定账户证据读取，不能任意填当前slot。ProgramData被加入RPC分片及Yellowstone/native manifest的相同bank依赖；验证Program指针、独立派生PDA、loader、可执行标志、部署slot、元数据和完整 `ProgramData[45..]` 哈希。声明升级模式不能回退到immutable；未声明时也不会自动把新loader当旧版本。`netBuyAmountEstimate` 的链费用后净值不可用与原程序收款Token余额差分分别记录，不能用gross或slippage minimum伪装费用后净输出。

Rust `yellowstone` 以固定上游proto profile读取真实gRPC原始帧，耐久WAL先于解释；`stream` 按原bank_id/slot区分分叉、版本和来源会话。完整provider block的计数仅证明观测集合；它不代替独立版本账户manifest。源会话变化、帧缺口、未知完成语义、dead/skipped、缺依赖或fork均要求reanchor；历史重放不会使online ready。 可信版本化初始账户快照和其独立manifest核对后可继承未写入的父账户，保留原write bank；skipped范围必须显式绑定source/session及父、目标bank，不从null或slot差推定。`source-evidence-rust` 是有限只读捕获工具，不发布在线状态。

双语言scope控制器负责buffer→固定锚点→当前候选选择→依赖分片→尾窗校验。文件store原子持久化恢复数据及任务，其资格为 `RECOVERY_DATA_ONLY`，重开不恢复旧授权/旧ready；需要在线发布时，ProductionStore必须接同一owner DB的prepared/journal事务。正常Go历史写入在同一owner DB的隔离namespace中提交，使用专属权限能力和预算；它不参与在线ready，也不能覆盖online。

```sh
make -C integration r01-r04-foundation-test
make -C integration r01-r04-traceability-check
make -C integration r01-r04-recovery-process-test
make -C integration r03-authority-process-test
```

R01公开来源清单对留存的不透明原文件分块核验摘要：每次读取不超过64KiB，单文件不超过64MiB，清单与工具合计不超过512MiB，并核对读取前后文件身份、长度和元数据；替换、截断、增长、路径重定向或摘要变化均拒绝。清单核验不将文件内容交给解码器，实际读取/解释JSON证据仍受原32MiB上限约束，旧的大体积verbose trace只能作为留存原件参与摘要核验。

Uniswap V3的有限getter证明采集只选择当前bitmap word：负tick先按tick spacing向下取整再定位word，采集slot0、liquidity、bitmap及每个初始化tick的四个原始storage word，最多1027个key。公开ABI返回值仅用于选择候选；必须固定同一chain/genesis/H/hash、部署代码版本和stateRoot，通过原runtime在MPT上的实际getter执行复核全部读取，再交正常Parser/Router做数量差分。候选值、旧H证明缺失或保存的成功标志均不能授权报价。`r01-getter-proof-test`提供离线原VM键集合及拒绝边界回归。

当前完成状态与真实来源/生产事务尚缺项仅以[开发进度](../DEVELOPMENT_PROGRESS.md#plan-2026-11-06-r01-r04)为准。模块测试、接口可调用或单个真实部署样本均不构成R01–R04整体完成。


## 多路分配与Velodrome V2计算范围（2026-11-08）

Router配置`routing.max_splits`支持1–8，示例默认4；`split_parts`仍1–100。普通API使用`routing.max_expansions`分别限制候选试算与分配试算，外层时间/取消预算共同约束。`quality.allocationSearch`返回`evaluations`、`gridParts`、`maxSplits`及`exhaustiveWithinGrid`；`quality.budgetExhausted`合并候选或分配次数耗尽。已找到完整方案时允许返回标明未穷举的INDICATIVE结果；无完整方案且次数耗尽返回BUDGET_EXHAUSTED。原CHAIN_FEE_MODEL_NOT_IMPLEMENTED/VERIFIED准入保持。

分配按累计网格边界取floor，最后累计量精确等于全部输入，原子单位余数随边界分配。最多8个候选、32份网格且物理池互不重叠的限定V2/V3/Aero状态使用子集/累计份额动态规划；费用按激活路数计，使用原数学输出缓存和最终完整重放。共享池/跨物理池会计和其他模型使用有界顺序枚举，大范围先为多路方案保留部分试算预算。网格内穷举不含未入选路径、网格之间原子单位、同一路径多次出现或任意DAG。候选排序新增小额试算，防止浅池整单滑点导致被筛除，同时保留最佳完整整单fallback。

`velodrome-v2-stable`和`velodrome-v2-volatile`使用独立`b3065d8b-1`adapter。Go读取与现有Aero ABI共享`metadata`、factory暂停和`getFee(pool,stable)`；code/clone implementation/factory摘要仍须显式匹配同固定块。原Velodrome源码`_get_y`上升舍入分支检查`_f`，旧Aerodrome版本检查再次缩放的`_k`；保留两个固定数学语义。216原函数EVM案例、8序列24步和30舍入控制为限定Q1证据，不能代替完整Pool.swap、特殊Token、实际部署或可信链状态验收。

## LFJ Liquidity Book v2.1.0（2026-11-08）

`lfj-lb-v21`使用独立`lfj-lb/v2.1.0-fee-oracle-1`适配器和公共`PoolState.lfj` field31。Go固定区块读取明确校验immutable-args clone的全部字节、implementation/factory/token代码摘要、同锚点getter与storage，并读取完整有界bin集合及反向链接、份额、raw reserves/协议费和Oracle当前/下一样本。同步Token余额必须等于raw reserves；未建模的捐赠、未知Token行为和池地址自资助不进入当前模型。配置允许1–4096个bin、1–8192个数学步骤，超预算失败封闭。

状态保留原packed参数，按原v2.1.0更新volatility reference/accumulator、18位精度费用、uint128余额与协议份额，以及Oracle环形样本、uint64累计值的原始溢出语义。与Infinity Bin共享的只是已验证的固定点价格幂基础函数；LFJ的费率、协议份额、储备和Oracle语义独立。ExactIn只返回全额消费或错误，原`getSwapOut`的未成交余量不会被当作完整报价。

`block_number`与`timestamp`使用明确的optional scalar presence，观察到的零保留，缺字段拒绝。Go JSON解码另拒绝缺失、null、重复和未知成员；Rust对应类型采用严格结构。4份独立C++ protobuf向量覆盖完整两种Oracle状态与显式零银行坐标。

正常Go→真实本机PG/Kafka→双Router的19条实际API计划被原样捕获，经独立模型投影后，用原编译参考记录的LBPair运行时代码逐腿重放38次swap，比较真实本地Token收款、参数、raw reserves、协议费、bins/份额和Oracle后状态。10类负控涵盖重算hash后的不守恒/自资助、缺时钟、储备不一致、原代码/池/输入绑定及10^18与10^18+1的无损整数区分。原程序比较器从JSON原始整数词法构造BigInt，禁止IEEE-754舍入掩盖差异。

这些是固定时钟、受控初始银行和标准Token资金注入的本地Q1结果。公开报价仍为INDICATIVE，净链费用未知；当前没有LFJ公开交易构建/生产执行或真实Avalanche来源资格声明。余下多协议和费用工作继续按进度文件核对。

## Slipstream上下文版本接入（2026-11-09，限定本地范围通过）

`aerodrome-slipstream-context`是独立的`f8717faa-module-1`版本，公共field32。
旧`aerodrome-slipstream/cl-1`继续拒绝非零模块；新解码器只有在匹配固定Pool、
Factory及custom/dynamic模块执行体和immutable绑定、固定真实Voter运行时代码、
注册池键及全部必要输入后才发布此版本。公共映射Voter的原编译器执行体与保留代码
匹配，`gauges`/`isAlive`是上下文无关getter；不把任意Voter或未知模块视作可计算。

动态模块必须提供显式有界classic EOA调用方列表，源码匹配不等于任意调用方的
`discounted(tx.origin)`已知。未知调用方费用上下文返回不可用，不默认为零折扣。
normal Parser固定同一个blockHash/requireCanonical收集当前price、liquidity、
staking、fee/reward/gauge ledger、完整有界observations和初始化tick扩展状态，
再由Rust域复算来源费用视图。特殊Token/授权账户/未知module组合仍不在本版本内。

Source重采集的bitmap窗口随当前tick中心变化；内部顺序计算只保留实际已知窗口，
不会假设缺失word为零。原程序参考同时保存两种视图：canonical source bank与
carried execution projection。`derived=true`区分内部修改，不能冒充原始来源。
Readonly factory/module/discount/Voter事实冲突时整个替换失败；新上下文及LFJ范围
还拒绝未建模的其他池/manager/vault钱包自资助，防止跨账户转移漏记。

原程序108案例中90完整、18部分成交；完整上下文数学、4组12步费用变化与全部
声明后状态差分通过，部分成交不当作全额ExactIn。当前normal Go/真实本机PG/Kafka/
双Router已产生13条实际API计划，原运行时13次swap收款及完整后状态一致，另有
8类模型/原程序负控拒绝。阶段39已完成限定范围的完整回归和同源进程验证；后续补修的当前状态以进度文档为准。
所有公开结果保持INDICATIVE，净链费用未知；没有升级真实历史/共识或生产执行资格。


### Pancake V3 LM 固定原程序计算基准

`integration/tools/reference/pancake-v3-lm/source-metadata.json`固定官方提交`986847948755cba528324d41be19480731c36c2a`的Pool、LM及MasterChef源码，保留依赖文件摘要。受控银行使用完整原`PancakeV3Pool.swap`、`PancakeV3LmPool.accumulateReward/crossLmTick`和`MasterChefV3.getLatestPeriodInfo`；原Pool/LM用Solidity0.7.6，MasterChef用已验证摘要的Solidity0.8.10及固定OpenZeppelin4.8.1编译。测试Token及构造LM的developer为明确的受控辅助合约，未替换上述原合约函数。编译器输出与原运行时代码按immutable范围归零后完整比对，包含CBOR元数据。

`make -C integration pancake-v3-lm-reference-refresh`重建原程序样本；`pancake-v3-lm-draft-differential-test`构建独立草稿，重新编译原源码并核对完整输出/后状态和负控。草稿在现有V3逐step曲线上独立更新协议/LP费、LM奖励与liquidity、tick/Oracle及raw余额，覆盖奖励两层floor、uint32结束时间截断、uint256增长回绕和同块连续执行。该基准报告只代表声明银行和输入的原程序差分；正常解析、公共状态/wire及路由使用下节的独立profile，旧Pancake V3的非零LM门禁不变。


LM版本使用独立`pancake-v3-lm-context`/`pancake-v3/98684794-lm-1`及公共field33；旧`pancake-v3/core-1`的非零LM门禁保留。MasterChef输入保留pool pid/alloc与共享总alloc/period，checked乘法及view一致性单独校验；LM的奖励先按原双层floor/uint32结束时间计算，再按原tick交叉更新。代码模板同时校验规范化执行体与每个immutable的实际getter绑定。新JSON校验对旧V3 core也扫描重复键，显式tick/feeProtocol观察值与core一致。受控零LM银行的所有LM tick必须保持未初始化零；有意破坏活动liquidity曲线的银行另作负控，不作为可采纳来源。

`pancake-v3-lm-state.cjs`分别保留模拟携带的已知bitmap窗口和原getters重新观测的canonical窗口；实际API源快照只能绑定到有编号的原source bank，允许明确的合成block hash/number封套，不能把派生执行状态当原来源或补造未知bitmap word。原程序计划重放另外校验窗口/readonly标记不随swap改变、每步完整账本及收款Token增量。本段历史接入限标准ERC20/已知代码及声明范围的ExactIn；后续独立ExactOut模板见下文；净链费、特殊Token、生产来源/执行资格均需自己的验收。


## Pancake V3 LM主目录业务入口（2026-11-09）

独立`pancake-v3-lm-context` / `pancake-v3/98684794-lm-1`已接入主目录。Go按同一blockHash读取Pool、Factory、LM、MasterChef和Token代码/完整状态；每个运行时代码既检查固定模板，又核对immutable实际getter绑定。Rust使用公共`PoolState.pancake_lm` field33重建状态，复算MasterChef奖励视图，明确区分原来源与顺序执行后的`derived`状态。

兑换计算复用已验证V3逐step曲线，按本版本原合约语义分别更新协议费/LP费、奖励双层floor与uint32结束时间、uint256增长回绕、LM与普通tick交叉、Oracle环及raw余额。路由通过明确Funds上下文使用此内核；拆单按实际顺序重放同一物理池的完整账本，内部Pool/Factory/LM/MasterChef钱包alias拒绝。部分成交不生成完整消费报价。

可重复入口为`make -C integration pancake-v3-lm-differential-test`（三语言域/配置/解析/原数学）、`pancake-v3-lm-model-projector`和`pancake-v3-lm-quote-test`（正常Go/真实本机PG与Kafka/双Router，实际API计划重放原运行时代码）。服务测试直接读取已纳入公共契约校验的`integration/test/contracts/pancake-lm-state.json`，不依赖隔离目录中的临时normalized副本。原程序参考文件继续按fixture声明的SHA-256验证。

当前支持范围为固定旧代码版本、标准ERC20的有限ExactIn及单池ExactOut，公开API保持INDICATIVE；真实BSC后来LM代际/惰性迁移不由本版本覆盖。链费用后的netOut、特殊Token、可信链历史与生产交易资格仍需各自实现和验收。详细验证结果及剩余范围只在`DEVELOPMENT_PROGRESS.md`维护。


## BSC第四代LM的独立解析与计算（2026-11-09）

`pancake-v3-bsc-lm-v4` / `pancake-v3/bsc-p400-lm1000000-mc999-v4`采用公共field34。原Pool optimizer400、LM optimizer1000000、MasterChef optimizer999及实际Factory/前三代前置LM源码与代码分别固定，不能将这些实际执行体套入旧98684794 profile。当前Pool/LM/MasterChef完整immutable-normalized runtime重编译匹配；前三代与Factory的执行体均匹配，其中第一代CBOR metadata与当前重编译结果不同，完整字节匹配状态独立保留。第二、三代和Factory完整字节匹配。

状态保留本地原始ledger、initialization、本地lmTicksFlag，以及三代前置的原始tick/实际存在的flag。第一代没有flag getter，保存空flag向量而非猜测false；第二代核对OldLMPool，第三代核对firstLMPool/secondLMPool，第四代核对三个前置immutable。来源按同一blockHash/requireCanonical读取；code模板与每个实际getter绑定一致后才发布。

数学只在内部将有效tick投影到已验证V3曲线与费用/奖励内核。`crossLmTick`选择第三代flag、第二代flag、第一代tick，实际跨越才写入本地tick/flag，继承gross为零也写flag；未访问的本地原始槽和所有前置状态保持不变，initialization在swap中保持观察值。外部来源、公共protobuf和路由始终保留独立第四代身份，不能将投影作为旧field33的canonical来源。共享池按执行顺序携带完整费用/Oracle及迁移状态，内部Factory/LM/MasterChef/前置钱包自资助拒绝。

原程序参考覆盖64swap（48完整、16部分成交）、4不一致银行和5序列15步，normal服务使用实际PG/Kafka与双Router，捕获实际API计划后用原当前Pool/LM/MasterChef/Factory及三代代码执行，核对真实Token收款、完整后状态和实际迁移次数。`make -C integration pancake-bsc4-reference-refresh`重建参考与4份独立C++wire；`pancake-bsc4-differential-test`验证解析/域/配置/数学/共享池；`pancake-bsc4-quote-test`运行正常服务链路。进度与最终范围只记录于`DEVELOPMENT_PROGRESS.md`。

本段ExactIn验收限标准ERC20和完整消费的swap；后续独立ExactOut模板见下文；不由此覆盖LM位置修改/领取，原始/受控来源不升级为可信历史bank/共识，净链费用与公开执行资格仍须单独验收。


## 完整银行驱动的路由准入刷新（2026-11-09）

Replica连续提交除了币种/协议拓扑，还比较完整银行推导的`excluded_pools`。共享Vault分组导致准入变化时重建候选图并推进graph_version；旧graph不原地修改，旧报价句柄撤销。否则只按币种拓扑复用图会保留不再合格的边或恢复后仍缺候选。

完整已提交银行即使没有可报价边，也可以推进last_reference；公开view保持非ready，不能用旧银行继续报价。下一合格连续块可恢复候选。canonical有效性与报价readiness分离，显式suspend/reorg屏障仍需原重新锚定流程，不能通过改变准入集合绕过。普通只改余额且准入/拓扑不变的更新继续复用图。

反例与验证使用明确合法的完整模型银行及真实in-process HTTP handler；正常三服务/PG/Kafka/双Router原程序回归另行验证协议金额和迁移状态未变。具体已验证范围与剩余通用增量/COW/费用/其他链协议项见进度文档。

### 独立 ExactOut 报价路径

`POST /v1/quote-exact-out` 使用独立 `ExactOutRequest`，接收字符串原子金额
`buyAmount` 与 `maxSellAmount`、`kind=EXACT_OUT`、链/Token/caller/recipient、
滑点、`FAST` 和可选 hop 上限，地址使用规范小写 EVM 表示。原 `POST /v1/quote` 的 ExactIn 请求/计划保持原有契约。
ExactOut 能力包括 `uniswap-v2-complete` 的固定原 Router02 模板与
`pancake-v3-bsc-lm-v4`、`pancake-v3-lm-context` 的单池外部回调模板，以及
`aerodrome-slipstream-context` 的单池origin绑定回调模板，以及
`lfj-lb-v21` 的单池最小目标/显式额外到账模板；其他 profile
在 ExactOut 候选中明确排除。计划 engine 为 `xone-router-exact-out/0.1`，缓存 ID
前缀为 `xo-`；请求与计划分别绑定源 view、完整 snapshot、经济策略与到期时间。

每 hop 从目标输出反算原 `getAmountIn` 的 `floor+1`，正向按选择的精确输出进行
Pair 完整结算。单路径要求简单 Token 路径、唯一物理池及同一 Factory；多路线按声明
顺序重算共享物理状态。有限有序输出网格以精确整数边界分配余数，每个候选路径至多
使用一次，在给定候选/网格/路数/预算内最小化原始所需输入。整单可行路径先试算，供
次数预算耗尽时返回已确认方案；取消或超时按原 API 资源门禁处理。

返回 `requiredSellAmount`、`inputBudget`、`unusedInputBudget` 和受用户预算约束的
滑点上限 `maxSellAmount`。`minBuyAmount` 等于目标，`refund=0`：原 Router 只拉取实际
输入，未用预算不是已转入后退款。链费用/输入币成本模型不可用时返回明确原因和空
`netSellAmountEstimate`。报价为 INDICATIVE；独立 build/模拟能力未通过时明确拒绝。
多路原程序验证是按序独立本地交易，不代表已具备公开多路原子执行器。
具体通过范围与尚缺能力只在 `DEVELOPMENT_PROGRESS.md` 记录。

### BSC第四代LM的独立ExactOut模板

`pancake-v3-bsc-lm-v4` 使用负 `amountSpecified` 的输出驱动价格步进：token1输出
从sqrtPrice减去向上舍入的商，token0输出使用有界乘积和向上舍入的倒数；每步input
向上舍入、output封顶为剩余目标，fee独立向上舍入。超过完整tick/step/价格范围时
拒绝整单，不用部分成交充当精确输出。完整fee/reward/Oracle与实际BSC惰性迁移账本
沿用对应原版本的状态模型；ExactIn使用原入口。

目前ExactOut路径只允许一个BSC物理池，使用外部标准资金回调模板；该profile不能
静默混入V2或其他多跳模板。公开响应的 `quality.pathPolicy` 明确单池限制，最大输入
和unused预算沿用独立ExactOut计划，未使用预算不记退款。有限有序分配可按顺序重算
同一物理池状态；这项模型能力不代表公开原子批量执行器已实现。具体通过状态及原程序
验证记录只维护于 `DEVELOPMENT_PROGRESS.md`。

### 固定旧LM版本的独立ExactOut

`pancake-v3-lm-context` 固定98684794版本按对应的完整LM账本进行负输出曲线计算，
复用价格step/fee舍入和完整fee/reward/Oracle结算；不继承第四代的前置LM/迁移字段。
该版本的ExactOut同样只允许一个物理池及标准外部回调资金，公开pathPolicy明确限制；
重新哈希也不能把单池模板升级为多跳或混合协议模板。原field33解析和ExactIn入口保持。
原getter到完整参考状态的投影可在`pancake-lm-exact-out-state.cjs --verify <绝对路径>`
离线逐字节核对，验证模式不修改参考或服务副本。通过范围只记录于开发进度文档。


### Slipstream origin绑定的独立ExactOut

`aerodrome-slipstream-context` 使用输出驱动的sqrtPrice步进，按实际origin的费用模块
计算每步输入及fee，并连续结算Gauge分成、奖励增长、质押流动性、跨tick和Oracle。
`quote_exact_out_for_origin`显式传入origin；不带origin的通用数学入口拒绝该profile。
动态模块的未采集origin、未建模内部资金、混合多跳及超出完整tick/step/价格范围的目标
保持拒绝。原field32固定块解析与ExactIn入口继续使用同一完整结算原语。

当前模板每条路径仅一个物理池。请求caller对应已采集的origin；原程序参考中的付款
账户是回调合约，其Token实际扣款与recipient实际到账独立核对。报价层检查输入预算，
该计算模板尚未提供公开build/独立模拟、链上输入上限保护或原子多路执行器。
原始ABI读取和完整投影可分别用`slipstream-exact-out-getters.cjs`及
`slipstream-exact-out-state.py --verify <绝对路径>`离线复核，验证模式不改写文件。

ExactOut的候选数超过上限时设置`quality.budgetExhausted`，同时保留可行整单fallback。
`quality.candidateSearch`公开实际hop上限、候选上限、展开预算和probeParts；它与
`allocationSearch`的有序网格覆盖分别描述候选和分配阶段。最优性仅覆盖已评估候选/网格，
不扩大为全市场或包含未建模净链费用的最优承诺。通过状态只维护在开发进度文档。


### LFJ v2.1输出目标与真实额外到账

固定`lfj-lb-v21`沿用field31状态与原fee/Oracle语义，使用原`getSwapIn`方向的独立逐bin
求解计算覆盖目标所需的gross输入及fee。实际Pair是输入驱动，整数价格舍入可能产生额外
收款；结算继续按真实输入调用同一个物理Swap原语，保留真实输出及完整bin/储备/协议费/
Oracle后状态，不能将多收到的金额裁剪为目标。

ExactOut请求的`buyAmount`/计划`min_buy_amount`是收款目标，响应`grossBuyAmount`是实际
预计到账，`outputSurplus`为两者差额。只有存在非零额外输出时，路径增加可选
`target_output`，分配增加可选`gross_buy_amount`；无额外输出时不序列化这些字段，原四类
模板的计划与字段保持兼容。`amounts`及路径`amount_out`始终对应真实物理兑换，目标总和
与实际收款总和分别校验；输入相同时，优先选择实际到账更多、随后路数更少的方案。

该版本每条路径仅一个LFJ物理池。独立原程序回放同时核对getSwapIn、input-1不足控制、
EOA付款扣款、recipient实际收款和全部声明后状态；受控参考使用factory sentinel与分开的
预付款交易，不代表已经有原Router或公开原子输入限额执行器。公开build/模拟和链费用
模型保持其各自能力边界。来源投影区分原Oracle索引记录顺序、模拟携带顺序及Go解码器
当前/下一Oracle的canonical顺序，原始storage值按样本ID核对。具体通过状态仅维护于进度文档。


### Meteora MM-only DLMM 的注明实现范围

`meteora-dlmm-mm`使用独立版本`meteora-dlmm/1532e540-mm-1`、固定53e587ee classic Token代码和公共protobuf PoolState字段35。Rust解析器要求同一finalized bank的pair、oracle、明确的bin数组、两mint/vault、Clock、DLMM及classic SPL程序/ProgramData；程序原始代码摘要必须匹配固定参考，所有PDA、Token余额与WSOL backing逐项检查。账户未知版本、奖励、限价订单、host-fee账户、OnlyY费用、Token2022和默认bitmap外的数组不会进入报价。

模型保留pair/arrays/Oracle完整原始字节。ExactIn按原程序整数规则逐bin消费：动态费用、LP增长（流动性先取Q64整数部分）、协议费、active、仅跨active时的last_update_timestamp及Oracle均参与后状态；Oracle达到120秒即滚动，有限初始化记录必须与头部计数相符。非空bin必须具有正确缓存价格；输入未完全成交或数组覆盖/步数不够时拒绝。普通候选与拆单使用相同物理状态顺序重算，跨池可变账户冲突及池内资金账户别名拒绝；Solana地址始终区分大小写。

API继续返回INDICATIVE和限定候选范围，链费用缺失时netBuyAmountEstimate保持null。缓存计划随源范围撤销失效。本段描述ExactIn；ExactOut 的独立接口见下文。公开交易构建与独立生产模拟尚未开放；固定原SBF受控回放不建立可信主网银行资格。恢复输入、生成数据和同源码验证入口分别为integration的meteora-mm-reference-refresh、meteora-mm-differential-test及meteora-mm-current-check。


### Meteora MM-only ExactOut

同一 `meteora-dlmm/1532e540-mm-1` 解析状态支持 `/v1/quote-exact-out` 的受限 Solana 单池计划。沿用字段35，不需要新增链采集。输入、目标和用户硬上限为u64，Solana公钥区分大小写。输出驱动的整数计算按目标逐bin确定输入和动态费；收款人获得恰好目标，bin可能因整数舍入扣除更多，差额保留为vault未分配余量。计算器保留真实bin、费用、vault和Oracle后状态；`grossBuyAmount`等于`buyAmount`，`outputSurplus`为0。计划中的既有`factory`字段对Solana承载固定DLMM Program ID；EVM承诺保持原字段和语义。

共享状态分配按顺序重新计算各路输入并受用户上限约束。`unusedInputBudget`是未使用的预算，`refund`为0。报价仍为INDICATIVE；生产模拟、公开交易构建、净链费用和其他DLMM策略没有在此批开放。参考、差分和统一验收入口分别为 `meteora-exact-out-reference-refresh`、`meteora-exact-out-differential-test` 和 `meteora-exact-out-current-check`。


### DLMM OnlyY 与 Pump classic/SOL

`meteora-dlmm-mm-only-y` / `meteora-dlmm/1532e540-mm-only-y-1` 复用字段35但具有独立版本与策略准入。卖X的费用计入输出Y、卖Y的费用计入输入Y；原程序OnlyY ExactOut的输出舍入差额计入协议Y费用，InputOnly版本保持原行为。奖励、订单、host、Token2022和默认bitmap外的组合仍拒绝。

`pump-bonding-sol-classic` / `pump-bonding/7a19affaa2c6-sol-1` 使用公共字段36，解析固定Pump/Fee/Token程序、同finalized上下文的10个状态账户及Clock/程序数据闭包。当前要求native SOL报价、classic mint、非mayhem/cashback/holder策略、预存volume账户并关闭跟踪；买卖费用和buyback拆分进入完整原始余额后状态。仅sell和buy_exact_sol_in的ExactIn模板开放，caller/recipient必须是已声明同一用户，路由单池。费用network payer与交易用户分离用于原程序差分；API仍INDICATIVE且净费用估计不可用。

完成曲线不产生报价边。complete与零Token储备及虚拟/真实偏移矛盾的源状态拒绝。原生资产节点是`native:solana:sol`，不会冒充WSOL；没有显式wrap模板和实际账户/费用验证时，跨该边界的组合不报价。两个Pump池共享资金账户时，规范源行仍保留但双方候选标记PUMP_SHARED_FUND_ACCOUNT_NOT_MODELED；共享账户观测矛盾则拒绝源状态，避免逐池复制费用余额导致错误拆单后状态。

七个首批EVM chain-key的图/数学复用测试是受控兼容性检查，不验证对应主网部署，也不覆盖Sui/Aptos/TRON等第二批链原生接口。新增原程序参考及验证入口见integration的`meteora-only-y-reference.py`、`pump-bonding-reference.py`、`pump-bonding-decoder.py`、`family_reference.py`和`validate_family_expansion.py`。

统一有限验收入口为 `make -C integration family-expansion-current-check`：依次执行同源码编译/解析/数学/HTTP/组合检查、Pump正常进程及OnlyY正常进程，再由 `family_expansion_acceptance.py` 核对凭证、摘要和各银行覆盖。OnlyY正常进程只覆盖ExactIn；其ExactOut由原SBF/完整数学后状态和HTTP矩阵验证。Pump正常计划经生产Rust投影器重算完整账户后状态，OnlyY正常计划直接回放原SBF且完整模型状态由同源码差分测试单独核对。

### DLMM 扩展、Pump ExactOut 与迁移（阶段52）

高级功能按独立 profile 准入。bitmap 版本保存原1576-byte扩展，验证`bitmap` PDA与池身份，正侧从512、负侧从-513开始反向编号，覆盖default bitmap及extension扫描；仍保留有限数组/步数预算。奖励版本按本次经过的非空bin数分摊每个奖励槽的时间增长，并以reward end截断，原MM/订单共用的48..80字节仅在正确function_type解释。该限定版本最多15个获奖bin；领取奖励不在swap模型中。订单版本的ExactIn依次消费MM、processed和open，各层独立价格取整；LP、订单和协议费用分开，记录filled、total processing、remaining与age，不把订单库存算作LP。订单ExactOut、OnlyY及高级策略交叉组合仍未开放。

Token2022版本只接受X为固定Token2022的TransferFeeConfig/TransferFeeAmount、Y为classic/backedWSOL。Clock epoch参与字段21状态，原mint/vault与withheld完整保留；gross输入先扣转账费，输出按实际recipient到账计量，ExactOut使用独立费用逆函数处理older/newer、0..10000bps及封顶。未知TLV、hook和Y侧Token2022组合拒绝。字段20用于bitmap，保留嵌套reserved 14..19。原有classic/InputOnly/OnlyY profile不因新增能力而放宽。

Pump的独立ExactOut买入调用原buy目标语义，SOL成本是floor+1及两笔单独ceil费用；卖出请求最小SOL到账，反求以周期10000枚举费用余数，避免双ceil导致的非单调净输出。返回最小可行输入与实际到账，`outputSurplus`、route target及plan完整保留，用户input budget未用部分仍不是已转账退款。候选修正有32次预算，无法证明时拒绝。

PumpSwap使用独立program/adapter与12原账户、19依赖闭包，curve与AMM校验共享字段36载体但不共享账户布局或虚拟储备。classic/WSOL v1的sell与buy_exact_quote_in、buy输出目标单独差分，LP费留在vault，协议/creator/buyback进入各自账户。共享费用账户的多池组合仍按既有联合账本缺口排除。

迁移感知曲线严格区别真实可交易量与保留迁移库存，完成后停止生成曲线边。原migrate CPI创建的真实AMM池/vault及再次执行幂等性是迁移参考；`migration_target`仅解析身份，真正报价需要完整目标依赖的已提交源银行。测试不构建公开迁移交易、不自动发现目标、不创建SOL/WSOL隐式边。统一入口为`solana-expanded-reference-refresh`及`solana-expanded-current-check`，最终证据及尚未完成项维护在开发进度中。

阶段52迁移补强：canonical PumpSwap 的已知正虚拟报价余额参与定价及market-cap费用档，但每次sell的实际vault扣款（扣除留存LP费后）必须不超过真实报价余额。严格boost版本保存正值；canonical v1版本也能解析此已知字段，未将virtual视作提款流动性。实际原程序的`InsufficientRealQuoteReserves`失败控制与迁移创建池的正向报价同时验证。共享资金冲突按真实可变账户判断：Curve的SOL收费钱包与AMM的WSOL收费ATA分开，完成曲线不参与候选资金锁冲突；共享账户原始观测一致性仍对全部源行检查。


### DLMM订单独立ExactOut（阶段53，2026-11-11）

`meteora-dlmm-orders`与`meteora-dlmm/1532e540-orders-1`的输入/输出驱动路径共用同一份原始订单状态。ExactOut通过固定SBF逐字节差分后接入数学、单池路由、优化器和计划准入；公共protobuf保持不变。部分bin按总目标反推一次净输入，向上补手续费后重放三层；耗尽bin则按MM、processed、open各层完整输入之和进入全填分支。不能直接照搬SDK对部分bin各层反推之和，部署SBF在层边界的实际语义不同。

若三层重放扣费后所得物理输出低于目标，按原指令拒绝，不搜索另一个ExactIn输入掩盖原ExactOut失败。原交易的物理输出可大于目标实收；bin/订单fulfilled/remaining/vault dust与原字节一致，API始终返回目标实际到账。订单fee split、LP fee growth、protocol fee、oracle、active bin和顺序后状态均参与差分。全bin耗尽采用独立分支以保留每层整数输入的舍入。

192原始银行覆盖双向三层组合和边界；161初始成功、31拒绝，含16组三步连续结算（共193成功步骤），每个初始成功均由原SBF验证输入上限减一必失败。新fixture与阶段52 ExactIn fixture分开，服务测试仅引用本服务副本。参考复验检查用户真实Token余额、mint总量守恒、WSOL lamports backing和失败回滚；普通/`-OO`篡改控制覆盖虚报输入、改订单字节、删除预算失败、失败交易偷偷改银行。

统一验收入口：`make -C integration meteora-orders-exact-out-current-check`。同源码报告：`integration/artifacts/local/core-meteora-orders-exact-out-validation-20261111.json`。本批仅新增固定orders策略的单池ExactOut，不代表订单管理、奖励领取、策略交叉、可信公共银行或公开交易执行已完成。

阶段53最终限定验收：源码`5f54b5c66c2a93b7cc1247b7550f35fc7acf6434c40def45a22c708c4c176b25`；28检查/34次Rust执行、两个工作区全目标check和相关Clippy通过，9实际ExactOut API计划均经生产投影/新LiteSVM原SBF、实际资金与完整后状态验证，2来源故障恢复和1次Router重启通过。普通/`-OO`参考控制各5项；公共类型、301份合同复制及进度文档验证通过。整体请求与生产资格仍未完成。


### 前端兑换量联调页（阶段54，2026-11-12）

`frontend/`是零构建依赖的静态联调页。它通过`/v1/quote-options`读取当前Router ReplicaView中的链、ready状态、Token、Pool、协议和adapter版本，再根据用户选择调用`/v1/quote`或`/v1/quote-exact-out`。页面展示实际返回的路线、输入/输出、退款、快照、图版本、计划哈希和原始错误，浏览器端不复制协议数学，也不构建或发送交易。

Router新增CORS层以支持独立静态服务器联调；API token仍通过Bearer头传递。当前实现使用宽松CORS便于本地开发，生产环境必须改为配置化来源白名单。元数据接口按当前ReplicaView聚合，状态未就绪时明确返回`ready=false`并保留空池/Token集合，不把配置声明当成已解析状态。

启动方式：先启动Router，再在`frontend/`执行`npm run dev`，访问`http://127.0.0.1:5173`；用`?api=`切换Router地址。此页面只证明已接入的解析和报价能力可视化联调，不能把当前限定范围验收扩展为全链/全DEX生产准入。


### 前端状态刷新和路由约束（阶段55，2026-11-12）

`/v1/quote-options`现在返回`viewId`、`graphVersion`、`snapshotRef`、`poolCount`和当前协议集合。前端每10秒重新读取一次，选择链后展示当前解析版本；输入变化采用短防抖并取消未完成请求，避免旧快照报价覆盖新输入。`maxHops`随QuoteRequest的`constraints`发送，路由结果仍由Router图搜索和协议数学决定。

该刷新机制只提高联调可见性，不改变解析权威性和报价准入：Router未获得ready ReplicaView时前端显示未就绪，报价失败保留原始错误。前端仍不复制协议公式、状态解析或交易执行逻辑。


### 协议报价能力和执行能力边界（阶段56，2026-11-12）

`/v1/quote-options`的Pool行现在包含`quoteModes`和`execution`。`quoteModes`由Router引用的协议数学支持列表生成，避免前端维护协议白名单；`execution.quote`表示当前可生成报价，`unsignedPlan`和`simulation`明确表示当前未开放。前端只展示这些能力，不把ExactOut报价误认为可执行交易。

后续实现顺序固定为只读动态账户预检查、unsigned交易计划、独立模拟、签名提交和回执结算。每一步都必须复用当前QuoteView和计划哈希，账户或状态不满足时拒绝，不在前端补造协议规则。


### PumpSwap卖出目标与执行准备（阶段57，2026-11-12）

PumpSwap经典/WSOL normal与boost策略新增卖出方向的最小输入目标：对LP/protocol/creator三项ceil fee按10000余数枚举并反解CP，不假设净输出严格单调。实际到账可高于目标，路由/优化器/计划/API统一保留target_output和outputSurplus。48原始目标中42成功、42原程序input-minus-one拒绝，14高价格案例有额外到账；原储备/fee账户/volume/native backing完整后状态参与差分。

`POST /v1/quotes/{id}/prepare`限定Meteora单池ExactIn/Out，选择当前缓存中已验证计划并拒绝过期/非当前view。只读RPC取服务配置，不接受任意节点URL；以finalized/minContextSlot批量读取执行账户并逐字节核对池/bin/oracle/扩展依赖，校验programdata代码及Clock、用户Token状态与余额、fee payer与费用。未填写账户则按owner/mint/token program派生ATA；不存在时返回blockers，已有账户可显式填写。当前不创建账户，也不自动包装SOL。

纯Solana线格式编码位于`execution-plan::solana_unsigned`，包含单签名legacy message、权限合并、账户索引、blockhash、短向量和1232-byte包限制。prepare返回零签名transaction与计划绑定及有效高度；需要后续独立模拟。测试对实际API输出/浏览器下载wire使用solders解码并在LiteSVM本地签名执行固定原SBF，核对实际到账与完整订单/bin/oracle后状态，不能将这个本地执行当生产模拟。

前端改用Router实际snake_case/nested path；金额为最小单位整数且保持BigInt精度。输入/view变化、网络失败和过期都撤销旧报价和待签名下载；认证token只按Router origin保留于当前session，不跨origin复用。CORS白名单通过`api.cors_allowed_origins`配置，缺省仅本地5173；默认不启用cookie凭据。真实Chrome测试请求本地生产Api handler和原账户银行，不stub报价响应。测试harness不是正式Parser/RPC生产来源，也不增加服务类型。

统一入口：`make -C integration quote-execution-current-check`；原始参考恢复：`make -C integration quote-execution-reference-refresh`。凭证：`integration/artifacts/local/core-quote-execution-validation-20261112.json`，明确区分历史静态检查与本批实际浏览器及原程序执行。

阶段57最终同源码有限验收：`282e47e86c62d1936e3b184683a479be518346d1c5b497e429c27054aaa98fc3`，24检查/24次Rust测试/13项真实Chrome联调通过。浏览器请求生产handler和内核、实际下载待签名消息并在本地原SBF验证699→1000及反例，未广播公网；本批没有新增normal Parser/PG-Kafka流水线或可信公网历史资格声明。


### 标准收款ATA创建与RPC模拟（阶段58，2026-11-12）

Meteora单池prepare可加入标准收款ATA幂等创建，仅允许规范派生地址及固定Associated Token Program代码；缺失源账户仍阻塞。多指令message将CreateIdempotent放在swap之前，payer资金需覆盖创建租金和网络费之和。已有目标账户不重复收取创建租金；Token2022收款创建未开放。

simulate可选字段对实际unsigned transaction做只读RPC模拟，sigVerify=false且不替换blockhash；模拟后源账户扣款/目标账户到账必须精确等于同一报价的实际输入输出，仍检验mint/owner/冻结状态/WSOL backing。失败或金额不一致拒绝下载。结果明确为RPC_SIMULATED，与生产独立VERIFIED资格分开。

测试模拟RPC使用固定原SBF的LiteSVM simulate_transaction并确认银行未提交变化，实际下载字节另行本地签名执行核对699→1000、租金2039280/总费用2044280、完整后状态和失败原子回滚。前端prepareSeq与quoteSeq分开，账户/创建/模拟选项改变立即撤销旧下载资格。复验入口：make -C integration account-preparation-current-check。生产签名广播、SOL包装、其他协议构建及来源资格继续保留。

阶段58最终同源码有限验收：17检查、8次Rust测试、17项真实Chrome业务联调通过，源码`e8ef710a9a2594b02be813d211cd929aed23bcbf2202fbd79cbdb7db8920c517`。实际创建+swap下载wire原SBF699→1000、租金/总费用/Token守恒/完整后状态和失败回滚通过；模拟仍是RPC_SIMULATED，未开放生产VERIFIED、签名或广播。


### WSOL输入原生资金准备（阶段59，2026-11-12）

prepare新增wrapSourceSol，只对Meteora输入WSOL/经典SPL的单池模板生效，QuoteRequest/数学仍使用WSOL。输入ATA不存在时只能创建caller规范ATA，按实际余额与输入预算差额加入System Transfer(2)和SyncNative(17)，与swap编译进同一消息。已有足额WSOL不额外包装；包装金额、创建租金和交易费合计校验payer余额，expectedResidualWsol显式报告预计剩余，sourceWsolRetained标识不关闭输入账户。

模拟源扣款以原余额加包装量为起点计算；新输入ATA的租金和WSOL backing参与原程序参考核对。原反向订单银行目标1000，需要1464 WSOL最小单位；请求输入预算2000、50bps滑点时，实际计划上限1472，包入1472、预计残余8仍为WSOL，不声称SOL退款。参考工具对完整下载wire原程序执行，并用上限1463控制验证创建/转账/sync及swap失败时一起回滚。

统一复验入口：make -C integration native-funding-current-check；原始执行银行生成工具：integration/tools/ci/meteora-wsol-bank.py。仍不签名或广播公网、不自动unwrap，生产VERIFIED资格与其他协议构建保持原边界。

阶段59最终同源码有限验收：20项检查、9次Rust测试、20项真实Chrome联调通过，源码`c632df0a687a1a3c7f603ac4ac0ffbddeefdf943d49f356cf9bff17f49d8fc71`。浏览器下载wire由固定原SBF验证：包装1472、消费1464、到账1000、保留WSOL8，租金2039280/总SOL支出2045752；控制交易全部包装步骤原子回滚。未新增公网发送、unwrap或生产VERIFIED资格。

阶段59证据保存修正后最终复验：源码`a767377942c5eed4079d545a85ce97e6f0f7b81b7c4e69989d2ca96166562bae`，20检查/9次Rust/20项Chrome联调全部通过，wrap/ATA/已有账户三类原执行输出均位于独立运行目录，不覆盖历史原始文件。


### 临时WSOL结算与退款（阶段60，2026-11-12）

prepare新增refundWrappedSource及unwrapDestinationSol。前者必须同时显式选择wrapSourceSol且不传sourceTokenAccount，以新建账户承接整个计划最大输入，兑换后关闭并退回未用SOL和租金；现有WSOL余额不参与本次临时账户补资。后者仅用于经典WSOL输出、caller等于recipient、不传destinationTokenAccount且不创建ATA的场景。报价buyToken仍是WSOL，prepare/simulation的settlementAsset明确为native:solana:sol。

账户地址按caller+ASCII seed+经典Token程序SHA256派生，seed由planHash前20位与xone-in-/xone-out-组成。顺序为System CreateAccountWithSeed、Token InitializeAccount3、可选System转入/SyncNative、可选收款ATA创建、swap、Token CloseAccount；payer兼base/owner，只有caller一个签名。派生地址预先存在即拒绝；close重新派生目标，不能接受任意用户账户。

响应区分rentLamports（全部预付租金）、rentRefundLamports（临时账户退租）、expectedNativeRefundLamports（关闭转回总额）、expectedPayerLamportDelta（含网络费的SOL净变化）。模拟要求关闭账户返回null，对payer的System账户校验owner/数据及余额差额；扣除网络费和持续存在的新建账户租金后还原实际swap收付，再与原报价精确比较。预付资金仍需覆盖整个交易，即使同笔最后会退租也不能提前抵扣。

受控回归覆盖输出699→1000/净SOL-4000、输入包1472/实际1464/退租及余款2039288/净SOL-6464、已有3000 WSOL不动、ExactIn/ExactOut、失败原子回滚、已占用seed账户和篡改close/native差额。复验：make -C integration temporary-wsol-current-check，最终结果见DEVELOPMENT_PROGRESS.md阶段60。独立生产VERIFIED、其他协议模板和公网交易资格保持原进度。

阶段60最终同源码限定验收：24项检查、12次Rust测试执行、24项真实Chrome业务联调通过；源码`bcda23963eea12acd05140b71397c1b15dbd281087508f384436170c87228869`。下载wire的原SBF执行/只读模拟、原账户保持、完整池后状态和失败回滚均通过，仍保持限定协议及账户场景。


### Token2022收款账户准备（阶段61，2026-11-13）

Meteora既有固定Token2022 profile的createDestinationAta现使用实际Token2022程序参与ATA派生和CreateIdempotent指令。支持已有178-byte TransferFeeAmount账户，以及ATA程序创建的182-byte TransferFeeAmount+ImmutableOwner账户；逐项解析TLV，拒绝未知/重复/错误长度/type。此范围不扩展TransferHook、其他mint扩展或未知程序。

创建租金按每种实际账户长度分别查询：经典165-byte、上述Token2022 ATA 182-byte，避免混合创建时直接乘固定租金。rentLamports为预付全部租金，destinationRentLamports为持久收款账户租金；临时输入refundWrappedSource仍只退临时165-byte账户租金及未用输入。

完整mint/Token2022程序/epoch/vault快照必须一致。对报价重放的完整vault变化推导gross输出与转账费，响应expectedDestinationTransferFee；simulate以收款可用余额差核对net，并额外核对TransferFeeAmount增长，拒绝“净输出正确但withheld错误”。源用户withheld保持不变。前端展示转账费、182字节及租金，生成真实unsigned wire后本地原SBF独立执行/回滚。复验make -C integration token2022-preparation-current-check，结果见阶段61进度。

阶段61最终同源码限定验收：30项检查、15次Rust测试执行、27项真实Chrome业务联调通过，源码`e62c0b4895e5f01576f5860b446da2c0e8cce2a593d21c285b3ca47cf4fbc480`；13原程序成功执行及对应回滚、178/182-byte账户和withheld/租金差分均通过。真实浏览器跨Router地址时按地址重新填入认证令牌，保持原会话隔离逻辑。全量设计、其他Token扩展和生产资格仍保持原进度。


### 协议QuoteView、能力注册与独立preparation（阶段62）

state-replica在父QuoteView内按protocol+adapter生成ProtocolQuoteView。每个子视图绑定父ID、SnapshotRef、graphVersion、排序后的完整池stateHash和版本化能力内容；共享父撤销令牌，不维护另一个可独立漂移的状态银行。quote-options返回protocolViews及每池protocolViewId，ExactIn/ExactOut返回实际使用的protocolViewIds。前端对父/协议视图同时核对，避免跨版本旧结果恢复下载。

protocol-math::capabilities统一描述当前注册数学版本/报价模式及执行层边界。未知池保留NO_SEARCHABLE_EDGE诊断；数学可报价与RPC账户准备/unsigned/独立模拟/钱包能力分别表示。当前preparation适配仍限Meteora，其他协议不会因为新增数学内核就获得交易权限。

动态账户工作迁入dex-router内部xone-execution-preparation crate。quote-api负责授权/限流、缓存计划hash和期限、单池选择以及前后当前QuoteView检查；独立层接收不可变SolanaQuote、配置RPC和严格SolanaRequest，检查本地链/币种方向/金额/计划hash契约后，执行有界RPC闭包、账户/程序/epoch/租金资金检查与unsigned构建。它不依赖quote-api/用户缓存，不签名广播，普通RPC模拟仍单独标记。

Raydium CPMM固定f4b9b263经典SPL模型新增独立输出驱动ExactOut，精确复现输入/输出creator-fee两分支、pre-fee整数上取整、原协议费用拆分和完整raw/净账本。路由及计划仅开放单池；原native-helper语料覆盖1832目标（583成功、1249拒绝），不是原SBF/CPI或生产资格的替代。

阶段62还补入61个显式注册模型的能力目录（无数据不生成虚构QuoteView）、撤权后的只读诊断视图，以及18个三步/54步Raydium输出驱动账本序列和共享池分配测试。

阶段62本批同源码有限验收通过：36检查、34次Rust测试执行、28项真实Chrome业务联调，源码`49c9d63d92859897c3d842aa848c771a9473e832905d5d392a9014de68089c7c`。这只冻结本批基础能力；六阶段主线仍继续独立模拟与钱包/回执等后续实现，未开放全协议或生产VERIFIED。


### Raydium CP-Swap execution preparation（阶段63）

独立preparation层增加经典SPL Raydium单池适配器。先用Snapshot模型/PDA和固定布局校验program/config/pool/mint/vault/user/Clock，再解析动态observation依赖并构建13账户的原swap_base_input或swap_base_output指令。Token代码版本从对应Raydium快照核对，不能沿用Meteora测试银行的SPL哈希。Clock/完整费用账本或程序改变拒绝。

该模板仅已有账户，不新增ATA、不包装/解包SOL；新能力模型/2将accountCreation与nativeFunding分开，前端按实际执行能力禁用未实现选项。元数据刷新同时核对protocolViewIds，解决父SnapshotRef未变化但能力版本改变时旧下载仍保留的问题。

原SBF实际后状态与原native helper数学账本分别验证。ObservationState::update即使不到十五秒换槽阈值也会累计价格、改写last_update_timestamp；独立参考必须调用原函数比对完整4075字节，不能用“oracle不变”替代。输入上限减一必须回滚所有交易账户，仅允许网络费扣款。仍不获得生产VERIFIED或钱包提交能力。

阶段63本批同源码有限验收通过：41检查、35次Rust测试执行、30项真实Chrome联调；源码`1eebd84144b09042493000f64ca1d5ad7d1f14f4049fd56b5483774c3234b4e1`。3组Raydium实际wire原SBF/输入减一回滚、完整Pool/vault/Oracle与原13组Meteora历史回归通过。独立模拟服务/钱包仍继续后续主线。


### 配置化本地SBF独立执行（阶段64）

api.solana_runtime_manifest为空时保持关闭。Router加载manifest后核对Python、43个运行时文件和本地bank摘要，复制运行时到私有目录并嵌入自身固定worker；禁止相对逃逸和任意包/扩展路径。配置字段在两Rust/Go一致，解析服务非空拒绝。

POST /v1/quotes/{id}/preparations/{hash}/simulate-independent从服务器缓存选择已经验证的unsigned字节，独立Python进程实际运行原SBF/Token CPI。Pool/bank模型及程序/账户布局、Clock/epoch、Meteora原bin/oracle/vault和Raydium账本一致检查；收付、原生余额/租金/手续费、withheld与准备结果核对，原银行未提交变化。原协议代码不调用Router数学。

有界输入/输出、并发、deadline和真实compute budget；worker取消/失败终止子进程。返回LOCAL_SBF_SIMULATED和runtime/bank/worker/wire/后账户摘要及evidenceHash，仍标记受控本地银行、非生产资格。普通RPC模拟接口保持独立。preparation缓存仅256项，绑定quoteId和当前QuoteView；重组/期限/参数变化不能复用证据。钱包签名与广播仍待后续接入。

阶段64同源码有限验收通过：46检查、39次Rust测试执行、34项Chrome联调，源码`6c475c5b77de9062690a7f658342f05e751399b7d54d09ffb61a9bf18bc83429`。隔离实际wire、真实CU/金额/Token2022 withheld、原银行不提交变化、缓存QuoteView绑定和各拒绝场景均通过，签名/广播/回执继续后续主线。


### 钱包签名、显式发送与回执日志（阶段65）

Wallet Standard使用wallet-standard:register-wallet/app-ready和standard:connect、solana:signTransaction（serialized bytes），不调用一键签名发送。用户账户/链必须与quote caller相符；返回交易不得改变已模拟消息，后端ring Ed25519再次验证。私钥始终在钱包，服务不生成用户签名。

api.transaction_submission_enabled默认false；开启必须具备api.solana_runtime_manifest、transaction_receipt_journal、transaction_submission_chains和transaction_max_pending（1..256）。前端prepareHash+evidenceHash绑定，签名返回与参数/协议视图变更撤销。发送前按当前genesis/blockheight/isBlockhashValid/fee/账户重新sigVerify模拟，准备的真实收付/租金/withheld必须仍一致；持久SUBMITTING意图先fsync，再单次sendTransaction。

receipt journal使用独占文件锁、私有文件、序号/哈希链和同步；终端记录及pending恢复不依赖quote缓存，重复请求不重发。RPC丢回包返回UNKNOWN并按签名查询，重启仍保持；完整损坏拒绝、截断尾恢复，追加IO异常停止写入。日志16MiB/4096记录及pending有界，达到限制明确拒绝，不声称无限存储或跨节点锁。

getTransaction必须与原signed wire、caller签名和status slot匹配；实际pre/post token/native余额满足minOut/maxIn才确认结算。Token2022净可用余额/退款持久租金/WSOL解包分别计算；字段缺失或结果不符保持pending/mismatch，不能用计划金额替换实际回执。前端按Router地址保存最后签名和会话选择，刷新查询状态而不重签/重发。所有测试发送仅本地fixture；生产VERIFIED和全协议/链资格不随本功能自动打开。

阶段65最终同源码有限验收通过：51检查、44次Rust测试执行、40项真实Chrome联调，源码`a30eee92f6dd0ecb59d117d3af30dc9201802121b766efc7b2a798bfcfb23814`；八类发送/确认/失败/未知/过期/回执及重启、消息签名、独立证据门禁、前端拒签/篡改/迟到/刷新恢复与最多一次发送均通过。这里只关闭已实现Solana模板的六阶段基础链路，不提升其他协议模板/可信来源或生产资格。


### 同次RPC账户捕获与独立模拟（阶段66）

prepare_solana_captured返回独立DTO与AccountSnapshot（chain/genesis/context slot/present及null缺失账户）。HTTP缓存保留原字节但公开响应只带accountSnapshot摘要，snapshotHash进入preparationHash；独立模拟直接使用同次已校验快照，不重新读RPC或合并context，slot推进须重新取得匹配Pool/Clock。

RuntimeManifest.rpc_capture=true允许banks=[]。模拟绑定捕获摘要/chain/genesis/slot及Pool，要求编译消息所有账户明确present或absent；captured Rent恢复真实租金参数，原SBF/Token ProgramData代码/部署slot及VM环境System/ATA/Memo身份核对，缺依赖不借用默认VM账户。快照缓存Arc共享、8MiB/项和32MiB账户JSON序列化字节合计/256项限制，与QuoteView/期限共同撤销。

结果RPC_CAPTURE_SBF_SIMULATED明确注明PREPARATION_RPC_CAPTURE；同context与minContextSlot均不升级成可信历史/共识银行或生产VERIFIED，VM特征仍由固定运行时指定。钱包发送仍需匹配独立证据、真实签名、当前genesis/blockhash/费用/账户的signed RPC预检及用户显式确认。

阶段66最终同源码有限验收通过：51检查、47次Rust测试执行、41项真实Chrome联调；源码`9f8dd4845e0d50795bfa8861e5751755be5d3723a6e047898100dd1478e48d4f`。无预装钱包银行的实时账户捕获、完整后状态/slot推进再报价、32MiB账户JSON缓存预算、固定模式兼容、capture-only模拟到钱包发送及回执恢复通过。仍只覆盖已实现的7个preparation模型，不将61模型全量或可信生产银行资格标为完成。


### PumpSwap classic/boost执行适配（阶段67）

独立preparation增加固定PumpSwap/Fee/经典Token适配，完整12模型依赖、程序/ProgramData/ATA代码与部署slot、Clock/user/实际Token账户身份及资金核对。caller等于recipient等于已声明user；该user也作payer使消息单签名。按固定IDL显式构建sell、buy、buy_exact_quote_in及pool-v2/buyback尾账户，buy选项track_volume关闭；当前只支持已有用户账户，不静默加入未知创建或资金转换指令。

PumpSwap sell目标模式是固定输入+最小输出，最小输入可导致实际surplus。generic SolanaQuote只对此已验证协议/方向允许目标小于预计实际输出，其余模型保留原相等约束。HTTP prep新增minBuyAmount并入哈希（ExactOut目标/ExactIn计划滑点最低），回执使用它；实际quotedOutput与最终到账继续单独保留。旧日志未携带该字段时维持原计算，避免破坏旧回执。

完整Pump程序分配10MiB，含Fee等的base64闭包约15MiB；RPC/单快照/银行/worker输入边界提高到16MiB、总缓存JSON字节32MiB保持，仍要求单context读取。worker加载实际PumpSwap/Fee/Token，捕获模型全部账户data/owner/lamports一致检查，回执按独立SBF证据+签名后实时模拟+实际meta核对。九组矩阵另对原IDL角色、wire、原完整后状态、Token守恒/WSOL backing及失败回滚做独立参考；不把helper预设成功当模拟。

能力模型/4对两明确版本开启代码能力；运行时和发送配置仍另行控制实际可用性，生产VERIFIED/其他未知模型不自动开放。前端签名已有发送记录后显示状态应看回执，避免仍显示“尚未广播”；刷新恢复按签名查询且不重发。

能力模型/4还明确executionRouteScope=SINGLE_POOL_SINGLE_ROUTE；前端按实际单路单池及协议匹配启用准备，资金选项跟随实际报价池/币对，禁止借链上另一池的能力开放当前PumpSwap或复合路线。模型单测覆盖拆单、多跳、协议不符和不可用池。

阶段67最终同源码有限验收：源码`09d3bea3332787a5fc908e32eef0f71371a9d8f7a5106d50ed800433f741790a`，56项检查、51次Rust测试执行、46项真实Chrome联调全部通过。九组canonical/boost双向ExactIn/ExactOut及额外到账原wire/完整后状态、Token守恒/WSOL backing和失败回滚参考通过；五组真实测试签名、单次loopback发送、重启恢复及minBuyAmount回执通过。费用程序/Pool/Clock变更、冻结/错误币种/缺失/余额不足和不支持选项拒绝通过；既有Meteora经典/Token2022/WSOL、Raydium、隔离SBF边界、八类生命周期、严格Clippy和普通/-OO反例及文档继续回归。309合同/12工具副本/164公共来源、前端模型和钱包单测、桌面/移动布局通过。运行产物与源码/工具/运行时/银行摘要已冻结；仅受控钱包与loopback发送，未提交公网交易。当前执行准备模型9/61，其余52模型、Pump原生SOL及自动资金账户功能不在本批完成范围。


### Pump原生SOL曲线执行（阶段68）

独立preparation增加固定Pump经典曲线sell/buy/buy_exact_sol_in。原生侧沿用账户DTO字段但显式sourceNativeSol/destinationNativeSol，绑定native:solana:sol及声明user的System账户；SPL侧严格校验已有Token账户。caller/recipient/payer必须相同，原始账户角色及双尾账户按IDL构建，volume/partial关闭。

买入准备资金为requiredInput加网络费，quotedInput为预计实际原生交易支出；卖出固定计算Token输入及minBuyAmount，quotedOutput继续保留可能大于目标的实际原生到账。独立worker、signed RPC预检、最终meta均用lamports恢复净交易金额，费用另外披露。原生账户与payer重复出现在模拟结果时核对同一账户数据，禁止用Token amount偏移解析System账户。

能力模型/5新增nativeSolTrading；nativeFunding继续表示自动SOL/WSOL资金转换，曲线不开放该选项或自动ATA。仅经典曲线模板；毕业迁移、其他Token和volume策略仍需独立实现。五组原型固定SBF/完整后状态/守恒/失败回滚及七反例通过，实际API和Chrome结果在阶段68最终记录后回填；不提升生产VERIFIED。

阶段68最终同源码有限验收：源码`c27be25a2476eb7ff3585504ef1fb80ed2c9a1b8b2597fc5695f8dd43fbbb8b4`，31项检查、26次Rust测试执行、51项真实Chrome联调全部通过。五组实际API原wire、模型全部后状态、Token/SOL守恒、网络费和失败完整回滚参考通过；五组原生买卖真实测试签名、单次loopback发送、重复提交及重启回执通过。独立模拟、signed RPC预检和meta回执均从lamports恢复交易净额，最后一组实际1/10971、费用5000、用户净增5971及最低到账1000分列。既有Meteora/Token2022/WSOL/PumpSwap/Raydium及八类钱包状态、RPC捕获/缓存预算、严格Clippy、普通/-OO反例和文档、桌面/移动布局继续回归通过。310合同/12工具副本/164公共来源通过。初始手动预检运行时路径缺失及拒绝场景复用过期报价的测试问题已修正；每个拒绝场景取得新报价，不改生产期限/超时。失败记录保留，不计入通过；最终运行固定源码、工具、银行/运行时、下载、回执与截图摘要。当前执行准备模板10/61，其余51模型、自动收款ATA及毕业迁移等功能保持未完成。


### Pump原生收款ATA和预存租金（阶段69）

新增显式createDestinationAta组合：仅原生买入输出的规范经典SPL ATA，创建六meta固定且与swap同一user签名。账户未存在或为canonical地址上的空、非executable System账户时，初始化前Token数量为零；按RPC最小租金减System预存余额补差，已足额时不再收租金。其他owner、非空数据或不匹配PDA拒绝。

destinationMinimumRentLamports/destinationPrefundedLamports/rentLamports分别记录最小租金、已有预存和本次补差，进入preparationHash。独立worker和signed RPC资金核对仅对原生创建分支允许该System前状态；后状态必须正确初始化Token账户，实际Native交易扣款剔除网络费和本次租金，receipt实际payerLamportDelta单独显示。

原五组+四创建/预存银行直接从固定程序恢复。原型wire767字节、完整后状态、租金补差和失败原子回滚已通过；能力/6仅开放经典曲线输出ATA，其他资金转换和毕业迁移仍未实现，最终API/Chrome/回执同源码结果在进度文档回填。

阶段69最终同源码有限验收：源码`1d1b4e54c0709645134a57a83001b53c2e6325e7d8f4f8cfb00a97908c82ebe7`，22项检查、6次Rust测试执行、56项真实Chrome联调全部通过。九组实际API wire含原五组和四个创建/预存场景，完整原程序后状态、Token/SOL守恒、最小输出/输入控制及失败原子回滚参考通过。普通/-OO各10个反例通过，含租金冒充交易输入、漏计租金、重编译ATA其他owner。实际API签名发送及重启回执核对补差2039280/2039279/0，实付3/到账1000/fee5000与用户净变动-2044283/-2044282/-5003分列；受控发送时故障注入的已创建ATA兑换失败回执为FAILED且不重发。已有ATA显式勾选不重复创建/收租金，原生输出拒绝ATA选项。Chrome缺失账户阻止下载、显式创建+原swap下载、捕获独立SBF、用户仅签名/确认发送/实际净SOL回执和刷新恢复通过，既有所有浏览器联调继续通过。310合同/12工具副本/164公共来源、严格Clippy、前端模型和钱包单测及普通/-OO文档通过。新测试初始发送配置缺少挂起上限被严格拒绝，补齐测试显式配置后通过；未改生产校验，失败记录保留不计通过。最终源码、运行时/工具/银行、日志、原始执行/回滚、下载/签名回执和截图摘要已冻结。其余链/协议、资金转换与迁移等继续开发；不提升生产资格。


### 阶段70：PumpSwap账户与SOL/WSOL资金

classic_funding仅用于已明确支持的经典SPL PumpSwap模板：规范ATA幂等创建、空System账户预存租金补差、WSOL差额包装以及本次新建临时账户的退款/解包。现有用户账户永不被关闭，包装仅WSOL输入、解包仅WSOL输出且同payer/recipient，临时seed绑定planHash。全部指令账户进入同次RPC快照，仍只支持单池单路/单签名legacy与1232字节上限。

WSOL初始化前的空System账户余额超过租金时，超过部分成为有效初始WSOL。独立worker以原始账户和真实rent reserve计算，signed simulation核对实际新增租金与准备承诺，变化时重新准备。最终回执允许账户在签名前被预存或提前幂等创建，使用meta的pre/post native与Token余额求实际租金和有效初值，不依赖准备估算。已有WSOL账户在签名前后收到额外SOL且wire包含SyncNative时，回执按真实reserve还原被同步的已有资金；这些资金不冒充成交。交易实际输入/输出、账户租金、网络费和payer SOL净变动分别报告。

复验入口make -C integration pump-amm-funding-current-check；本批于2026-11-14完成验收，34项检查、26次Rust测试执行、72项Chrome联调通过；22个新增资金API银行和44组实际原wire参考（含既有协议/浏览器下载）验证完整后状态与回滚。结果及同源码摘要记录在DEVELOPMENT_PROGRESS.md。能力模型/7扩展两个现有模型，仍10/61具备执行准备，其他51模型及毕业迁移不随资金选项开放而完成。


### 阶段71：Orca精确输出目标

固定cc683ec9经典SPL五个Orca模型的ExactOut使用原始manager对应的输出指定分支，固定delta为输出量并使用对应舍入，计算输入及上取整费用，按输出目标封顶并递减剩余输出。静态与自适应模型共享Q64步进，后者仍按费用组更新Oracle、奖励、tick/混合数组和协议费用；完整目标、数组范围和消费者步数预算保持严格校验。

600个独立原始native manager请求/响应、完整前后raw bank及状态投影绑定来源；8条连续状态序列和三个自适应模型的双池四份额独立网格用于路由与最小原始输入对照。原程序上游源码没有修改；自有参考适配器增加output mode。该证据不执行SPL CPI、不等于SBF/钱包交易验证。能力模型/8只增加报价模式，Orca执行标签继续false；单池单路及明确网格范围不外推全市场最优。同源码验收28项检查、101次Rust测试执行、91项Chrome联调通过。600个native场景及600条服务流程分别核对；服务层包含584次对应HTTP目标报价和16条状态准入拒绝。源码摘要及完整范围见DEVELOPMENT_PROGRESS.md。复验make -C integration orca-exact-out-current-check，恢复数据入口orca-exact-out-reference。


回执界面按Router和查询版本隔离，切换清除旧显示并保留本地签名记录；返回原地址后只做签名查询。发送前在当前浏览器会话按Router保存用户已确认的交易签名，晚到的提交响应不造成恢复线索丢失；恢复和切换均不触发重发。该前端状态隔离与Orca native数学参考分开验证。


### 阶段72：Raydium CLMM输出目标与选费位置

六类固定6d7a0f82经典SPL模型使用原始输出指定分支。输出端收费先对净目标上取整还原价格驱动量，原始步进按净目标封顶并将必要舍入差记入费用；输入端收费则对原始输入上取整加费。LP/协议/fund份额逐步分别取整，保留费用币种对应的增长、跨tick奖励、Observation与动态费变量。无有效输出/预算不足/数组或整数边界失败时不提交候选状态。

独立参考运行原始swap_internal/compute_swap，构造器经原始Seed/PDA规则产生同配置、同价格/流动性的不同物理池并配置完整Bitmap；构造池未声称真实部署。280步进向量包含零费率输出余量，1266场景、6序列、12网格、12个原生序列化前后检查点用于数学/解析/路由/API对照。最终同源码27项检查、304次Rust测试执行、109项Chrome联调通过；947次HTTP目标成功、307次HTTP拒绝和12次状态准入拒绝分别记录。原生序列化前后12个检查点已解析一致，完整范围/摘要见DEVELOPMENT_PROGRESS.md。数据恢复make -C integration rayclmm-exact-out-reference；同源码复验rayclmm-exact-out-current-check。能力模型/9仅增加报价模式，执行准备仍10/61；六模型unsigned/SBF/钱包模板继续独立实现。


### 阶段73：标准V3目标与递归回调路径

基础Uniswap V3和无LM的Pancake V3通过独立目标接口接入既有Q96输出驱动内核。每个TickBitmap word单独步进/取费，反向计算目标再按同一物理状态正向重放；仅同factory、同模板简单路径，重复池/环/混合模板和未知资金自引用拒绝。基础状态投影只保留下一次价格报价所需的price/tick/liquidity/bitmap/gross-net，不包含完整LP费用或Oracle账本。

固定原始Factory/Pool通过真实create/initialize/mint构造头寸，原始swap负数指定输出并由受控ERC20调用器实际回调付款；完整目标/maxInput由测试调用器约束。原始费用、观察值、Token余额、交易回执和失败观察状态回滚独立记录。原始两跳执行采用递归回调付款，和仅累加单池报价分别核对。能力模型/10仅增加报价能力，同源码24项检查、45次Rust测试执行、115项Chrome通过；原始80成功/16拒绝、80预算回滚、4实际两跳与2网格范围和源码摘要见DEVELOPMENT_PROGRESS.md；复现make -C integration v3-exact-out-reference，验收v3-exact-out-current-check。


### Immutable WeightedPool输出目标与物理余额

`balancer-v2-weighted2-equal`、`balancer-v2-weighted2-static`、`balancer-v2-weighted-multi` 通过独立输出驱动的GIVEN_OUT计算接入 `/v1/quote-exact-out`。先缩放输出和余额，按原WeightedMath次序执行向上取整的base/exponent/pow及输入乘法，再将输入向上反缩放，最后按 `1-fee` 向上补费。原始输出不得超过缩放输出余额的30%；现金及加费后的最终现金执行uint112约束，中间值执行uint256约束。

多币池始终保留一个完整资产向量。路由边包含显式 `from_token`，参与币对按完整token顺序查找，`zero_for_one` 仅在索引0→1时为true；任何其他索引不能借用该布尔值推断币对。更新输入和输出现金后同步reserve0/reserve1，其余资产和全部身份数据保持。单路线仅支持一个物理池，允许多路线按顺序重新计算同池状态；拆单结果只承诺配置网格及预算内的搜索范围。

能力版本 `xone-protocol-capabilities/11` 分别声明EXACT_OUT与执行准备。数学计划绑定显式token流、输出目标、最大输入、报价视图和快照。三个模型的执行准备/unsigned/钱包标签保持false；Managed/Composable/BPT/rate/Stable需独立实现。原回调参考与前端复验说明在 `integration/tools/reference/weighted-exact-out/README.md`；开发进度和实际验证结果只记录于根进度文档。


### Standard Stable2输出目标

`balancer-v2-stable2` 的独立GIVEN_OUT内核使用目标输出修改缩放后的输出余额，以原invariant求输入余额，再按原StableMath补一scaled单位、输入向上反缩放、最后加raw费用。保持uint256每步运算和uint112原始/最终现金限制。StableMath没有Weighted的30%输入/输出限制；完整目标仍不得耗尽输出现金。参数沿用同一固定块的精确amplification与10幂scaling，费率/Token/程序准入不变。

路线限定一个物理池，可按原有配置网格选择双池或按顺序重算同池。池/Vault/实现等内部地址不可作为未建模外部资金。API绑定原报价、数学计划与子QuoteView，过期或撤销后保留原证据诊断但禁止构建。能力版本/12新增Stable2的EXACT_OUT，执行准备与钱包标签保持false。

参考边界与复现见 `integration/tools/reference/stable-exact-out/README.md`。它执行原始数学和现金库的包装，不提供StablePool/Vault部署、ERC20转账或执行交易证明。开发进度只记录于根进度文档。


### Static singleton CL输出目标

静态零hook的 `uniswap-v4` 与 `pancake-infinity-cl` 以正数amountSpecified驱动原始输出目标，逐word/tick计算最大可交付输出；段内反解sqrt价格，输入与费用向上舍入，输出截到剩余目标。跨初始化tick按方向调整liquidity，空流动性段和位图证明边界保持有界；不完整目标、signed128 BalanceDelta与uint160价格限制失败时拒绝整条路线。PoolId为manager/rawPoolId，计划必须绑定相同manager，不能将rawPoolId解释成独立合约地址。

单路线只有一个物理池，拆分在已有网格内按当前状态逐次重算。浅池不能完成整单时可按可行份额参与候选；无可行分配时拒绝。能力版本/13新增两个基础模型的EXACT_OUT，advanced/hook/native/动态LP费用模型不借用此路径，执行准备/unsigned/钱包标签保持false。

原Pool/CLPool库的正数目标参考、seed及限制见 `integration/tools/reference/singleton-exact-out/README.md`。模型后状态仅为价格/tick/liquidity等下一次报价所需投影，不包含完整Manager资金结算或LP费账；实际Token转账和可执行交易模板需独立验收。进度只记录于根进度文档。


### Static Infinity Bin完整树输出目标

`pancake-infinity-bin` 的输出接口使用原BinHelper.getAmountsIn的顺序：每个访问bin取min(剩余目标,输出储备)，Q128价格将其向上换为净输入，再向上补swapFee，protocol费用按该fee比例向下分配。完整输出跨bin沿已证明完整树搜索；初始化但无输出储备的bin和脱离树的active尘埃按原访问规则计步，失败时不提交任何物理状态。单bin/累计金额分别限制uint128与原signed128 BalanceDelta，保留原MAX_LIQUIDITY_PER_BIN检查。

输入和输出接口共享同一完整物理后状态投影：app现金按用户gross输入/输出，bin按扣protocol后的输入，protocol累计按原uint256环绕语义；tree成员及total_shares不随swap改变。原burn导致active不在树时仍先处理其locked尘埃，随后只使用真实树成员。报价更新中的App/Vault余额和全局protocol计数为明确的投影，不表示已执行完整Manager结算。

单池路线按manager/rawPoolId绑定计划，双池网格及同池顺序分配复用已有优化器。能力版本/14新增本静态标准模型的EXACT_OUT，advanced/hook/native/动态LP费与执行模板不借此开放。原库/真实burn参考与复现说明在 `integration/tools/reference/infinity-bin-exact-out/README.md`，进度仅记录于根进度文档。


### Managed Snapshot输出目标

`balancer-v2-managed-snapshot` 使用同blockHash读取的 `getNormalizedWeights()` 与 `getSwapFeePercentage()` 反解输出目标。权重压缩/插值的结果直接参与weightOut/weightIn向上取整，不能重归一化成1e18；Managed2–38币正权重域与不可变Weighted的严格归一化域分开，输出算术共用并保留原30% scaled输出上限。计算输入向上反缩放后再加费用，原始与最终现金为uint112，每个uint256中间值按原顺序检查。

现有解析准入已经要求swaps enabled且protocol/management swap fee为0。公开数学guard API对关闭开关/非零费用明确拒绝；其他Managed费账、BPT/AUM或fee/schedule版本不借用本model。完整物理余额向量与显式from_token用于任意币对，单池路线和原有网格内的顺序重算保持；协议视图/计划绑定相同权重和费用快照。能力版本/15只新增明确EXACT_OUT，执行准备/unsigned/钱包仍false。

原ManagedPool回调参考使用测试Vault facade及显式unlimited code size，不证明生产部署/完整Vault/ERC20转移。参考说明在 `integration/tools/reference/managed-snapshot-exact-out/README.md`，实际开发进度见根进度文档。


### Composable普通币对输出目标

`balancer-v2-composable-regular` 的输出数学先保留完整注册顺序确定BPT索引，再将BPT从scaled余额向量中移除。普通币索引按BPT位置映射，使用原invariant求输入余额，补一scaled单位、输入反缩放向上，最后加raw费用。完整输出不能耗尽目标现金；每步uint256及初始/最终uint112现金保持原边界。BPT现金、原数组顺序和其他资产不变，StableMath输出不套Weighted的30%上限。

单池路线仅支持underlying，BPT图节点继续排除；注入BPT边、BPT输入/输出数学请求、含BPT的数学计划和HTTP报价均拒绝。双池网格和同池顺序分配复用现有物理状态求解。能力版本/16的范围为SINGLE_POOL_UNDERLYING_ORDERED_ALLOCATION，执行准备/unsigned/钱包仍false。无rate-provider profile不接缓存刷新/外部rate/BPT供应和待结费用。

原始StableMath测试包装、BPT位置和资金证明边界详见 `integration/tools/reference/composable-exact-out/README.md`；它不代表完整ComposableStablePool/Vault/ERC20执行。开发进度只记录于根进度文档。


### Raydium经典SPL的显式ATA／WSOL资金选项

Raydium CP-Swap creator经典模型复用classic_funding：无选项时原swap指令/wire不变；createDestinationAta只创建 canonical收款ATA，wrapSourceSol只针对WSOL输入并补不足额，refundWrappedSource使用独立seed临时账户，unwrapDestinationSol要求WSOL输出/caller=recipient且无指定已有目的账户。已有用户账户不关闭，账户占用、非法组合、owner/mint/frozen/资金不足均拒绝或给blocker。

资金指令按prefix→原Ray swap→suffix顺序编译为单签名消息，创建/包装失败和swap失败整体原子回滚。System/ATA/ProgramData可空证明及Rent加入同次RPC捕获，ATA固定代码hash/System Native身份保留，独立SBF不refetch或混合其他context。模拟/收据以源/目的/付款人三个前后账户核对实际Token与Native变化，区分持久租金、临时租金退款、WSOL包装和网络费；预存空System地址只补租金差额。

能力版本/17在已有Raydium执行准备模型内增加账户创建和native funding，native_sol_trading仍false。签名/广播/回执沿既有Wallet Standard流程，生产默认关闭，RPC_CAPTURE_SBF_SIMULATED不升级历史bank/独立生产VERIFIED。复验入口为 `make -C integration raydium-funding-current-check`，实际进度仅见根进度文档。


<a id="orca-classic-execution"></a>

## Orca经典SPL执行与资金

Router的静态/奖励Whirlpool模型使用legacy swap，逐字节验证完整Pool与固定/动态tick数组、程序/Token代码、PDA、经典mint/vault、Oracle空态和同次Clock。单次原生swap最多选取三个方向数组，B-to-A按tick spacing平移；只有一或两个已证明数组时重复最后地址填槽，原builder去重，禁止虚构窗口外账户。捕获的WSOL Vault可能有SOL捐赠，背书允许覆盖token余额且保留原lamports，不执行隐式sync。ComputeBudget原生账户与1,400,000单位指令均进入捕获和交易绑定，独立运行时配置容量不足时明确拒绝。

classic_funding只按显式请求添加ATA/包装/临时退款/解包，预存租金和本次补差分开。独立SBF复用同次捕获而不refetch，Clock缓存设置后恢复捕获账户的原lamports；同一处理也用于本地只读RPC和测试钱包运行时，防止把VM默认sysvar值冒充源账户。钱包签名/确认广播/回执沿既有API，生产默认关闭。adaptive、Token2022和其他程序版本不借用本能力；复验入口为 `make -C integration orca-execution-current-check`，进度仅记根开发文档。

<a id="orca-adaptive-classic-execution"></a>

## Orca自适应经典SPL执行

adaptive、adaptive-rewards、adaptive-dynamic继续使用固定程序的legacy swap。初始化Oracle的254字节按既有模型重建，包括Whirlpool绑定、交易启用时间、费率控制/衰减/波动状态及16/16/128预留区；准备和独立SBF分别逐字节比对。编译消息必须将该Oracle标为可写，静态空Oracle维持只读，拒绝用扩展权限或字段编辑绕过模型准入。经典mint/vault、ProgramData/Token代码/部署slot、PDA、Clock和完整tick数组沿已有验证；动态数组不等于Token2022扩展。

银行生成器只对受控原生struct账户显式附加已保留的实际程序字节；slot、Clock和ProgramData上下文对齐不构成历史bank共识，WSOL币对的Pool/Oracle/array PDA和raw绑定均重新推导。54场景分别覆盖三个模型的普通币对、WSOL、双向ExactIn/Out、显式资金/临时账户及预存租金。原生参考、实际API wire原SBF、模拟、钱包回执和Chrome必须分别取证；独立草稿证据不可替代正式API/QuoteView绑定。复验 `make -C integration orca-adaptive-execution-current-check`，进度只记根文档。

<a id="rayclmm-classic-execution"></a>

## Raydium CLMM经典SPL执行

固定6d7a0f82的静态、奖励、Token-fee单池指令采用`swap_v2`。完整Pool、Observation、每个已声明数组、配置费用/PDA/Clock/mint/vault、program/ProgramData和Token原代码分别核对。未知动态状态、扩展bitmap、自定义seed和挂单数量不借用此模板；tick order cohort无挂单的历史值与原Pool opaque填充保留，WSOL Vault只验证背书覆盖且保留捐赠lamports。ComputeBudget指令与账户闭包明确绑定，数组按原bitmap可用方向选择，不伪造源窗口外账户。

`swap_v2`同时声明Tokenkeg、Token2022和Memo；经典资产仍仅Tokenkeg。附属Token2022闭包在生成银行时明确取锁定运行时并锁定原payload摘要，Memo则显式加载保留来源的完整原字节；两者不可由VM不同默认程序补足。独立模拟复用同次RPC捕获，完整Pool/Observation/数组/Vault、实际收付、费用/租金和失败全账户回滚由外部原程序重放分别核对。复验 `make -C integration rayclmm-execution-current-check`，实际开发进度只记根文档。

<a id="rayclmm-declared-advanced-execution"></a>

## Raydium已声明扩展状态执行

六种既有经典SPL模型各按独立profile验证。其中Bitmap、自定义seed和动态费额外捕获完整1832字节扩展（原discriminator、Pool绑定、112+112个u64及PDA）；非零seed按原u16LE参与Pool PDA和bump，不把零seed编码成额外非空种子。动态费保留34字节所有原字段，包括有符号tick spacing参考、波动参数/引用/累计与时间，1130之后保留区必须为零；静态模型仍要求1096之后为默认状态。扩展在原swap_v2的剩余账户第一位，后接当前方向实际数组，不能借用池外账户或请求参数提供费率。

162受控银行包含三个模型各三种收费模式的普通币对/重新推导PDA的WSOL币对、双向输入/输出目标、ATA/包装/临时退款解包和预存租金。程序闭包与classic模板一致，Token2022资产与订单/未知组合仍拒绝；组件原SBF证据、正式API wire、钱包回执和Chrome应分别验收。复验 `make -C integration rayclmm-advanced-execution-current-check`，完成状态仅记根进度。

SBF运行时程序映像的宿主生命周期：构造线程只保留至多两个已验证代码映像，线程退出或LRU淘汰时释放缓存持有引用，活跃Engine/子进程保持自己的引用；账户银行、并发许可和各Engine的计算／超时配置不共享。每次Engine构造仍读取并校验源程序摘要，复用时再将保留程序逐字节与刚验证源数据及编译worker比较，Unix程序文件只读；任何差异明确拒绝，不静默重建绕过。此处理消除多API宿主重复冷复制的开销，不提高生产15秒等预算，也不把程序映像复用当作来源／执行验证。worker异常仅记录输入／输出已传字节计数、耗时及子进程退出标志，辅助区分启动、管道与执行问题，不记录账户或交易负载。

`prepared_evm::simulate_prepared_exact_out`接收完整ExactOut准备组件，重算内层交易及外层目标/maxSell/未用请求预算；结果保留`preparation_hash`和可选`exact_out_preparation_hash`，原unsigned字节摘要及simulation receipt继续独立。准备的`unused_quoted_input_budget`来自用户请求预算；实际执行的`unused_input_budget`来自calldata内经滑点收紧的最大输入，二者不可互换。二者均留在调用者账户，不发生退款转账。

配置`revm-fixture`后，普通`POST /v1/quotes/{quoteId}/simulate`支持已缓存的EVM单池ExactOut计划。worker使用独立`exact_out_bound`，与旧`bound`恰好二选一，接口不允许请求覆盖目标模式、交易或费用。返回`quoteKind=EXACT_OUT`，保留原plan/snapshot、actual input、target收款、未用编码预算及实际native费；`/internal/quotes/{id}/evidence`保存原报价与独立模拟证据。ExactOut共享原worker资源、取消和期限，缓存证据发布前/写入时复核失效；`VERIFIED`、build和生产准入继续按各自条件拒绝。

原Router02 ExactOut独立执行扩展到已有完整绑定所支持的无重复token/物理池简单路径。VM对每个touched pool分别核对源码/银行/getter/锁并观察swap后状态，使用一次真实交易和共享预算；最多8hop仍由绑定层限制，原交易差分现覆盖1–3hop双向。来源继续为显式提供的完整声明银行；旧ExactIn多池绑定需另行实现，不能只删除校验放行。

`bind_v2_execution`的完整V2 ExactIn分支支持1–8hop简单路径：每步校验Pool/Factory/源码/源bank及token方向，token和物理池不重复，重新计算全部分配并按顺序绑定所有touched池。多hop不混用旧reserve-only版本；拆单/共享DAG需要独立执行模板。正常prepare-evm/本地worker沿用同一绑定，界面准备判断允许双模式单路径多hop，未准入池和重复池保持拒绝。

`StateAnchor.base_fee_per_gas`可显式记录来源H的base fee。准备资金观察与此值核对，实际REVM交易继续使用`execution_environment.base_fee`（H+1），二者都进入银行/执行承诺，不能互相覆盖。旧bundle省略该可选字段时保留原同费率约束；若真实H与H+1不同，应明确提供源费用，否则拒绝准备复用。H+1超过交易cap始终拒绝，此声明不授予源header的共识或状态根证明。

`POST /v1/quotes/{id}/preparations/{hash}/simulate-evm`执行服务端当前缓存的RPC准备，body仅空或{}；双模式共用同二进制worker和取消/预算。结果保持原SimulationOutcome，并以`preparedExecution`保留原unsigned字节摘要/组件hash/ExactOut外hash和独立commitment，再绑定`apiPreparationHash`及`rpcPreparationHash`。父进程重算返回commitment；准备被替换时旧hash不可执行，旧在途结果在写证据锁内再次核对。组件序列化载荷限制单个64KiB/整体32MiB，额度随quote失效释放，在途操作保留所需引用。配置的提供银行仍须与RPC观察的链/源H/资金/账户一致，不满足则拒绝，不自动退回eth_call或虚构状态。当前接口不签名、不广播，也不升级VERIFIED/当前链/完整费用资格。

界面按`execution.evmIndependentLocalSimulation`判断是否开放EVM本地独立执行，运行时银行必须匹配当前快照源身份。浏览器对准备中原unsigned字节执行WebCrypto SHA-256，再核对quote/plan/view、API/RPC/组件hash、ExactOut外hash及实际输入/收款；费用参数变更立即撤销旧wire和执行证据。正常API响应显式给出quoteViewId，ExactIn报价给出kind。Solana能力和钱包行为保持各自语义，本地EVM成功不开放签名广播或生产VERIFIED。

Go analytics可显式设置`service.role: analytics`和`service.mode: journal-continuous`，复用原有限`journal`模式的完整只读消费/耐久游标。每轮完成后按`journal.poll_interval_ms`等待，周期不重叠，SIGTERM取消等待或读取并清理reader；任何坏事实、保留缺口、topic变更或读取失败仍持久撤销发布并停止，不自动跳过。每链`analysis_interval_ms`必须1–86400000，poll必须1–60000；必须Kafka且control disabled。该角色不打开online KV或租约，不发布日志，不计作第四种服务。原`journal`模式仍单次完成后退出，持续ClickHouse和回补/shadow流程另行验收。

<a id="public-bootstrap-checkpoints"></a>

## 公开不可变Protobuf checkpoint

设计§10.5，阶段102。Go授权`POST /internal/checkpoints`返回原`manifests`及新增`public_checkpoints`，后者含manifest路径、manifest原字节SHA256和元数据。仅真实reducer Bootstrap提供snapshot与committed Kafka cursor时发布；无cursor的HTTP开发模式返回空数组。服务在发布manifest前后核对owner、schema/adapter、assignment/writer/canonical/generation/scope；旧本地JSON格式独立保留。

对象为`checkpoints/<chain>/public-v2/<payload-sha256>/{bootstrap.pb,manifest.json}`，Protobuf直接使用公共`dex.v1.OwnerBootstrap`。manifest版本2、codec固定，reference含块/seq和全部资格标识，cursor的next_offset采用规范十进制字符串保留i64精度；payload固定bootstrap.pb，声明精确size/SHA256。先文件fsync/目录fsync发布payload，再不可替换hard-link发布manifest；已有精确字节可续传，损坏对象拒绝，不写latest。发布后的资格守卫失败仍可能留下物理对象，调用方不得把其存在解释为可用目录声明。

Go `ParseBootstrapManifest`/`ReadBootstrap`及两Rust `kafka_transport::checkpoint::parse_manifest`/`decode`先验证调用者可信pin和64KiB元数据上限，再按声明长度读取/核对payload（全局最大64MiB，运行时可更小）；拒绝非canonical JSON、重复/未知字段、路径替换、非canonical Protobuf以及元数据与payload的引用/cursor差异。本地读取拒绝对象symlink；配置root及其祖先为operator管理。不可变对象目录不供不可信本地进程写入。

`make -C integration public-bootstrap-current-check`冻结源码/固定输入；当轮Go实际导出3组宽整数向量，两Rust独立target执行互通并核验导出文件前后摘要。此格式校验不证明owner权威、快照当前性、topic UUID/保留窗；远端下载和正常恢复调用方仍须单独验证这些资格。

## 远端checkpoint HTTP对象接口

阶段103，配置`journal.checkpoint_store={"base_url":"https://objects.example/checkpoint-bucket","allow_plaintext_development":false}`。该对象API必须提供GET和支持`If-None-Match: *`的原子条件PUT；200/201/204写成功或409/412竞争后都读回原始字节确认。路径只由配置base和公开payload SHA派生为`/public-v2/<sha>/{bootstrap.pb,manifest.json}`，没有listing/latest，也不跟随manifest内任何外部链接。base为无尾斜杠规范ASCII DNS/IPv4地址和可选端口/简单路径；当前不接受IPv6/IDN/URL转义。开发HTTP仅127.0.0.1且显式开关。

凭据使用`journal.source_auth_tokens[base_url]`，不回退到父URL/其他源token，不进公开hash；客户端禁环境proxy、重定向和自动解压。服务入口在本地发布后上传，先校验本地完整数据、检查来源guard，确认远端payload，再guard/manifest/guard，成功才返回remote_checkpoints。失败可能保留物理对象，后续续传仅接受同字节。远端manifest摘要必须由可信独立调用方提供，不能从下载数据自行生成pin；`Remote.Fetch`先核验该pin及manifest限额才请求payload，限定精确对象长度、统一ctx/整轮timeout，最后跨格式校验状态和cursor。

运行`make -C integration remote-checkpoint-current-check`。定向测试覆盖实际独立HTTP端口、正常服务handler、条件竞争/并发续传、半上传、冲突、假成功/回读损坏、缺对象、错误pin、流式/声明超限、编码和跳转、ctx取消/截止时间。对象API并非S3 SigV4，AWS/GCS类存储需提供兼容网关；当前不声明canonical可用目录、router自动远端恢复或真实独立故障域耐久。

## 当前owner资格化目录与router远端恢复

阶段104。Go成功发布并回读两个远端对象后，写同目录`remote-<base-url-sha256>.json`不可变完成标记，再做来源guard。授权`GET /internal/chains/{chain}/checkpoint-catalog`按当前完整Bootstrap重新计算内容SHA，只在本地manifest/payload/该store完成标记全部精确一致时返回`{manifest_url,manifest_sha256,manifest}`，返回前再核对当前reference和cursor。该入口不扫描latest/高度候选，不上传任何新对象；源更新后若尚未发布同状态对象，则503。标记是给可信owner检查的完成依据，本地文件存在本身不赋予远端来源资格。

配置checkpoint_store的正常dex-router Kafka恢复先请求配置owner的目录，以其独立给定pin和owner/assignment/chain/topic校验限定对象；store请求只用source_auth_tokens中精确base_url的token。先64KiB manifest、再声明精确长度payload，禁止redirect/content-encoding，整轮timeout，校验格式/原字节摘要及状态和cursor后，再请求owner同目录确认未变。随后仍走正常bootstrap年龄、Kafka cursor保留窗/有界尾窗和Replica owner/代际校验；仅下载完成不发布视图。远端错误记录值无关错误码，可选择同一配置owner的当前bootstrap，全部失败由原sync_sources暂停。日志恢复来源区分REMOTE_CHECKPOINT_AND_KAFKA与OWNER_CHECKPOINT_AND_KAFKA。

`make -C integration checkpoint-recovery-current-check`包含同轮Go原字节HTTP下载和明确启用的独立Kafka冷恢复/正常HTTP报价。目录当前是owner在线控制面；自动发布、canonical公告、离线目录、旧快照追平及独立故障域恢复属于后续范围。默认不开生产准入或VERIFIED报价。

## Go周期checkpoint发布生命周期

阶段105（2026-11-18）。`journal.checkpoint_store.publish_interval_ms`省略/0为手动，100..3600000为Go online正常服务周期；两个Rust配置副本允许识别该字段，但拒绝在Rust服务/analytics启用，避免未实现设置默默生效。正常Start后先做一轮，再在每轮完成后等待interval，不积累ticker。后台与授权POST /internal/checkpoints共用同一可取消发布通道和整轮request_timeout_ms截止时间；局部snapshot/cursor读取后，网络阶段仍按原epoch/owner/schema/scope guard核对，无actor锁跨网络。

Service.Close先取消整个checkpoint生命周期，取消进行中的手动和后台HTTP请求，等待worker及占用发布通道的请求退出，再关闭reducer/PG和日志；父Start ctx取消只停止周期，不使独立手动调用凭空失效。目录入口关闭后不返回资格。原JSON/Protobuf/remote返回字段保留；后台没有真实cursor时不伪造位置，也不推进owner日志。

授权`GET /internal/checkpoints/status`返回enabled/active/interval/completed_cycles/failed_cycles；metrics增加xone_checkpoint_cycles_total与xone_checkpoint_failures_total。completed包含无对象可发布的正常轮次，不能读成成功上传数；有确认远端对象才记录checkpoint.uploaded，失败记录固定错误码checkpoint.failed。正常HTTP服务WriteTimeout取至少5秒及发布请求预算加1秒，以便在预算截止后返回错误响应。

`make -C integration checkpoint-worker-current-check`包含真实正常服务handler/独立HTTP对象端口的race、关闭和取消测试，Go状态/cursor不变检查，以及同轮Go原wire下载和实际Kafka冷恢复回归。周期上传和读取当前目录属于已实现范围，旧快照追平与canonical离线目录继续单列。

## 多链checkpoint周期的公平性

阶段106。后台使用配置链顺序的轮转，每次只发布一条链，选择时立即推进下一位置，因此某链持续503、缺cursor或发布超时不会让后续链永久等待。每条链发布有自己的request_timeout_ms预算；仍通过全服务同一gate串行执行，与手动任务共享取消/Close规则。日志增加chain_key以区分失败范围。手动POST /internal/checkpoints保持整批首错响应，不推进或重置后台轮转。

publish_interval_ms是相邻链轮次之间的等待，完成一个N链轮转至少需要N个interval和上传时间。`make -C integration checkpoint-fairness-current-check`跑双链正常服务race：受控第一链对象持续失败，第二链仍上传/更新generation，第一链修复后恢复，全部网络串行；旧取消/关闭/手动回归保留。真实公共链与多故障域资格仍按原验收要求单列。

## 旧远端checkpoint与有限Kafka尾窗

阶段107。每次完整远端回读与来源guard后，可原子更新本地`public-v2/candidate-<store-sha>.json`提示。它只记录同store的已完成对象位置，不是ready声明。授权`checkpoint-recovery-catalog`精确校验候选的pin/manifest/payload/不可变完成标记，再将它与当前owner committed head的reference/cursor绑定；同代资格字段必须全部一致，seq/块/offset单调且offset差≤max_tail_records。当前head通过reducer BootstrapHead锁内读取，只拷贝身份与cursor，沿用完整Bootstrap的ready/租约/关闭规则。响应前再次核对当前头，变化则拒绝。

正常router Kafka恢复先试此目录，再可回退原exact-current远端/owner当前bootstrap。旧checkpoint下载后在私有Replica按同代连续BLOCK_COMMIT重放，实际帧数和初始lag不超max_tail_records，整轮request_timeout_ms含目录/下载/Reader构造/重放/最终目录复核。每个complete unit先通过原Replica连续性、池依赖及scope验证；目标cursor若处在BEGIN/CHUNK中间、最终seq/hash/ref不同、保留窗过期/高水位倒退、未知kind/reorg/新代均失败。所有中间图与cursor仅在私有任务中，最终完整状态及Reader经owner目录二次核对后一次发布到正常replica，日志来源REMOTE_CHECKPOINT_BOUNDED_REPLAY。

`make -C integration checkpoint-replay-current-check`显式执行独立Kafka旧快照+两个后续块的私有追平/正常HTTP报价，覆盖目录撤权、半事务边界和帧预算；原同轮Go wire HTTP下载、周期/关闭/多链公平性和exact-current冷恢复回归。跨代历史重放、离线canonical目录、真实topic重建/保留/多区故障域仍需后续验收。

## Rust owner公开checkpoint本地发布

阶段108。两个Rust kafka-transport副本提供`checkpoint::local::{encode,write,read}`，使用与Go相同OwnerBootstrap protobuf及规范manifest版本2。写入`checkpoints/<chain>/public-v2/<payload-sha>/{bootstrap.pb,manifest.json}`，payload先fsync再manifest硬链接不可替换/目录fsync；原字节幂等、冲突拒绝、manifest发布前后guard。符号链接对象/namespace拒绝；配置root/祖先需operator管理，新目录0700/文件0600（Unix）。文件读取先验证manifest pin/上限，再按精确payload长度解码，不把物理存在当owner资格。

正常Rust parser新增授权POST /internal/checkpoints。先checked_current做来源/lease资格，锁内提取同reference committed bootstrap；无真实cursor的http-development返回空public数组，Kafka缺cursor拒绝。后台blocking写任务持有owned发布gate，取消时flag撤权；关闭先发送stop并等待gate排空再关闭来源/lease。写完成再次checked_current核对同代owner/schema/adapter/scope，成功才返回publication。该轮remote_checkpoints与旧manifests均为空，Rust远端上传/周期待接。

`make -C integration rust-checkpoint-publication-current-check`运行两个workspace transport/独立导出、Go读取各自3组原manifest/payload（宽offset/u64/2^200）、Rust正常服务HTTP与ready/scope竞争回归，以及严格Clippy/Go vet和当前parser程序构建。raw导出按当轮目录隔离并核验前后摘要；未设置导出目录时跨语言Go测试显式skip，统一门禁要求实际PASS。

## Rust owner远端条件上传与当前目录

阶段109。正常Rust POST /internal/checkpoints在本地发布后读取/校验同一原manifest/payload，经固定checkpoint_store base派生URL上传；source_auth_tokens只取精确base键。GET已有对象时只接受精确字节；缺失对象使用If-None-Match:*，200/201/204或409/412均回读原字节确认，先payload后manifest，checked_current/owner/lease/代际guard围绕各阶段。禁环境proxy、redirect及Content-Encoding，元数据/流式读取均有上限，整轮沿用request_timeout_ms。token不进入返回值或固定错误码。

全部确认后，同一local对象目录写`remote-<base-sha>.json`不可变证书；双Rust local模块提供objects/certify/certificate，固定证书名称格式、64KiB上限、原publication pin/manifest一致及发布前后guard。正常授权GET /internal/chains/{chain}/checkpoint-catalog按当前bootstrap重新编码内容ID，复核本地manifest/payload/cert和同store预期URL，再次检查完整reference/cursor后返回。目录不访问云存储或把其持续可用当作假设；实际下载仍需核验远端对象，远端损坏/缺失不能因此通过恢复。

HTTP发布select关闭watch：Close发stop后立即撤掉待完成网络future，阻塞文件任务持owned gate直到退出，之后才释放租约。无store时保留阶段108空remote数组；有store配置时失败返回503，物理半对象可保留但不赋目录资格。Rust周期和旧对象恢复目录尚未启用。

`make -C integration rust-remote-checkpoint-current-check`包含两个独立HTTP端口的正常owner发布/目录及故障控制，关闭在500ms内打断挂起对象请求；Go独立解码该次原Solana fixture protobuf和宽offset，以明确受控source范围证明互通。真实Solana bank/独立云存储与生产准入继续按原要求验收。

## Rust周期发布、关闭信号与配置摘要

阶段110。publish_interval_ms默认0仍为手动；100..3600000允许Go/Rust online parser，禁止dex-router及analytics启用。Rust正常Start后在同一服务内按配置链顺序轮转，每轮只发布一链、先推进位置，完成后等待interval；同一owned gate串行化手动和后台写入。失败不产生资格也不阻塞后续链，恢复后再用现有条件上传/原字节幂等。status与metrics区分active、completed_cycles和failed_cycles，空轮次不是上传成功。

正常serve收到shutdown信号时先发送stop watch再等待HTTP graceful drain，周期/手动远端future随即取消。Close加入周期任务、等待本地blocking写任务持有的gate排空，再关闭来源/lease；停止后tick不再采集，末尾started=false。统一服务入口通过可注入shutdown future运行同一serve_started_until逻辑，测试不需要向开发者进程发送信号。

Rust ConfigSnapshot的public_hash现在与Go一样先规范化typed API/journal再去除auth_token/source_auth_tokens，避免同一输入因字段省略而产生语言差异；optional checkpoint的false/0与省略同义，周期设置实际变化仍改变hash。economic_hash继续绑定原routing/simulation/chains。对于省略默认值的旧Rust配置，新版本加载后的public_hash可能变化，按原要求重启，不复用旧配置下的准备/缓存。两Rust读取同轮Go导出的5份配置与摘要，凭据只用测试假值且不进公开hash。

`make -C integration rust-periodic-checkpoint-current-check`覆盖双链故障/更新/恢复/默认手动、正常关闭信号中断挂起HTTP/加入任务、Go原worker回归、两语言配置/摘要及Rust API/程序构建；公共真实Solana bank、Rust旧对象recovery目录、保留/配额和独立故障域仍独立验收。

## Rust owner旧对象目录与版本2恢复策略

阶段111。Rust成功发布后写store专属candidate提示，目录GET /internal/chains/{chain}/checkpoint-recovery-catalog按原pin/固定URL校验manifest/payload/cert，结合checked_current和Store.bootstrap_head锁内同次committed reference/cursor，对返回前再次检查。candidate可变但不ready，损坏/符号链接/任意外部URL、过期owner/不同store或原对象丢失都拒绝。原exact-current目录继续只返回精确当前对象。

RemotePublication/RecoveryCatalog和recovery_target现在由两个Rust kafka-transport副本统一实现。控制目录schema1仍要求同generation；schema2允许旧checkpoint到当前head之间generation单调前进，其他owner/assignment/writer/canonical/schema/adapter/scope必须一致。Rust正常owner当前以完整ONLINE_BASELINE_COMMIT推进，Store仍拒绝未知BLOCK_COMMIT；router在私有Replica重放时，v2完整baseline通过install_baseline验证，普通BLOCK_COMMIT仍通过原连续apply，seq/cursor/完整transaction边界和最终head全部精确匹配。writer变更、reorg、scope/schema变更继续失败并尝试当前来源。

`make -C integration rust-owner-recovery-current-check`在同一冻结批次起独立Kafka，运行两个不同当前测试程序：Rust owner维持真实正常HTTP和对象API，提交真实Kafka旧baseline与两次完整baseline；独立router通过原恢复路径私有追平/正常sync/实际HTTP报价，缺owner凭据无ready。测试使用受控Solana fixture来源和显式测试producer，不声明实网bank/PG lease或跨区域存储资格。目录/旧对象数据另由Go独立解码，原v1 EVM两块追平与exact-current Kafka回归显式运行。

## 公开checkpoint本地逻辑容量准入

阶段112。`journal.checkpoint_store.local_budget={"max_bytes":1073741824,"max_files":1000,"max_entries":2048}`为显式按链public-v2命名空间限制，省略则沿用原行为。参数非空时bytes1..1TiB、files1..100000、entries≥files且≤1000000，非法设置在加载时拒绝。它进入公开配置hash但不改变经济策略hash；Go3组当前配置供双Rust实际load核对。

两owner在单一共享发布gate中先编码当次public manifest/payload，规划精确remote证书及candidate字节/名字，再有界扫描现存namespace。所有普通文件（含未知及遗留staging）计入bytes/files，每个文件和目录计入entries；链接/非普通对象/深度超过4、枚举超过max_entries或现有/预计逻辑占用超限均失败。读取不跟随对象链接，逐entry检查取消。既有对象只按现存大小和必要增量规划，不因重复发布完整相同对象再收一次持久容量。失败在公开对象write/远端request之前，不删除旧candidate/cert/对象。

Go周期路径改为仅写公共Protobuf和远端元数据，避免每轮再复制旧本地JSON；手动POST仍返回旧JSON manifests兼容，旧手动JSON/底层KV不在public-v2预算内。status暴露local_budget，nil表示未启用。此为逻辑文件准入，非实际磁盘块/WAL/metadata/原子临时副本的空间预留；要求同namespace仅一个owner进程写入，不替代跨进程锁或远端总存储预算。自动保留策略后续接入，当前满额明确拒绝新发布。

`make -C integration checkpoint-budget-current-check`验证精确边界、旧对象重试/新代拒绝、未知文件与目录/链接/取消，以及两正常owner配额不足无public写入/remote请求；两个Rust读取Go当轮实际对象和Usage/配置hash，原独立正常owner-router Kafka追平显式回归。

## Go只读checkpoint保留计划

阶段113。配置checkpoint_store.local_retention例如`{"keep_latest":2,"min_age_ms":3600000,"max_remove":2,"max_objects":32,"max_entries":256,"max_read_bytes":1048576}`。字段全部显式且验证有限上限；当前仅Go online预览实现，两个Rust compiler可读取相同Go配置并核对hash，但Rust owner/router/analytics启用会被CHECKPOINT_RETENTION_OWNER拒绝。

认证GET /internal/chains/{chain}/checkpoint-retention-plan使用发布gate、整轮request_timeout_ms和Service关闭取消；先取得完整Bootstrap，结束再核对精确ref/cursor。读取现存public-v2而不写入，所有候选文件验证规范格式并保护其指向SHA（包括旧store），当前完整state内容SHA永远保护。对象只有原manifest/payload完整解码及本store回读证书都匹配才可进入年龄/keep_latest规则，未来writer/canonical/assignment/seq被保护且不挤占当前域最近备份数量。最小年龄同时要求链anchor与本地manifest均足够旧；按seq最旧先选且不超max_remove。

计划返回applied=false、每对象保护/可选原因、selected、完整性覆盖、scanned_entries/read_bytes、namespace_sha256与plan_id。命名空间摘要口径FILE_METADATA_AND_VERIFIED_KNOWN_OBJECT_BYTES：已验证对象绑定完整原字节，未知/不完整/其他owner只metadata并保留；不能误称整个磁盘内容摘要。policy/当前owner/cursor/观察时刻一起参与planId。执行器必须另行重验owner、文件和范围，不能仅凭一个planId删除。

`make -C integration checkpoint-retention-plan-current-check`覆盖6对象/2store候选、当前/最近2份保护/只选最旧2项、min-age/metadata变化、取消/各预算/损坏/符号链接，以及正常Go HTTP认证/只读/owner过期/关闭，无删除且无远端请求。2份Go policy配置经双Rust核对公有/经济摘要；实际回收、自动触发、崩溃恢复和Rust计划为后续工作。

## Rust只读保留计划与全字段独立对照

阶段114。local_retention现在允许Go/Rust online parser，router/analytics拒绝；两owner的GET /internal/chains/{chain}/checkpoint-retention-plan均为认证只读预览。两个Rust kafka-transport提供retention::Policy/Plan/plan，规则和Go一致，保留CURRENT_STATE/所有store候选/LATEST_BACKUP/age及未来身份，完整解码并核对本store原证书后才可列eligible，未知/不完整只metadata不选。

正常Rust接口先checked_current并锁内取同ref的Bootstrap，owned发布gate随阻塞扫描任务保留；stop/取消flag和每次读/枚举的精确BootstrapHead守卫阻止旧source计划，结束再核对完整reference和cursor。Close可取消等待中的preview，等待者取消不会释放别人持有的gate。HTTP预览不联系对象API，也不写文件；过期owner/关闭返回拒绝。

`make -C integration rust-retention-plan-current-check`让两个Rust直接读Go当轮保留的同一6对象/2store候选目录及原观察时刻，完整JSON比较包含policy、保护原因、选择集合、bytes/files/entries/read_bytes、namespace_sha256与plan_id；同目录重复分析不改变文件。mtime以无损i64 ns进入证据，异常超范围时钟和非法UTF8路径两语言均拒绝。完整对象文件摘要消费前后固定，两个正常HTTP预览和实际Kafka恢复显式回归。全部计划仍applied=false，执行删除/自动保留/崩溃恢复尚未接入。

## 显式Go checkpoint回收和可恢复隔离

阶段115。local_retention新增reclaim_enabled（默认false）及plan_ttl_ms；启用仅允许Go online、TTL1000..600000ms和显式24..8192字节API auth token。两个Rust compiler核对同样设置/公有hash和去凭据，但Rust服务启用执行会CHECKPOINT_RECLAIM_OWNER拒绝。旧配置继续只读预览，不静默自动删除。

新增认证POST /internal/chains/{chain}/checkpoint-retention/apply，JSON仅`{"plan_id":"<preview sha256>","observed_at_ms":<preview time>}`。server先验证固定≤1024字节JSON/重复与未知字段/类型，再拿相同checkpoint gate与总request_timeout_ms。只有新鲜owner的ref/cursor/原policy/原观察时刻/目录复算planId相同且未过TTL，才使用固定选择。公共library入口要求authority guard，runtime逐次检查完整BootstrapHead/取消/关闭；没有传入路径或待删列表的接口。

逐选中对象再次核对原本地文件哈希、完整manifest/payload和store certificate，并GET配置base下的原manifest/payload核对精确长度/原字节SHA。不会向远端发DELETE，不跟随journal中旧URL执行网络。通过后先把完整文件列表/哈希、source/计划、阶段及累计计数原子fsync到私有retirements/state.json（最大1MiB、4096叶文件），再rooted rename public-v2/<sha>→retirements/<op>/<sha>并fsync两父目录，持久化QUARANTINED后逐个固定叶文件释放/目录fsync，最后持久化DONE/COMPLETE。无递归RemoveAll，额外文件/链接/内容变化会停止并保留journal。

POST .../checkpoint-retention/resume只接受`{"operation_id":"<pending id>"}`，不重新选对象；当前授权owner可在HEAD_FIRST新writer/generation后重新验证同链/owner单调身份、当前保留规则和全部candidate（含指向隔离对象的candidate），重读同store原备份及尚存文件，再从rename已做但阶段未写、部分文件释放、目录已删或complete回执前继续。旧pending绑定store；配置切换地址拒绝接续，不把旧URI当新凭据目标。旧操作完成后可切换store发起新计划，累计计数保持。

GET .../checkpoint-retention/status是只读最后操作/累计计数观察。mutation结果区分applied、idempotent、receipt_confirmed；最终回执未确认时COMPLETION_UNCONFIRMED/applied=false，complete已fsync但响应未送达时可读journal/幂等重试，重试会fsync确认。只保留一个active/最后完成记录和累计计数，非永久审计历史。budgeted_read_bytes包含按原长度预留的remote GET内容，budgeted_entries计入本轮控制/目录扫描；两个上限沿用显式policy。未知结果/失败仍有待恢复状态，不能无限重选、跳过保护或重复计数。

未完成隔离的文件仍占磁盘，因此Go正常checkpoint发布先读pending journal；pending或损坏则暂停该链新归档（其他链轮转继续），不把隔离副本算作可用新容量。在线KV、canonical journal、owner cursor不因cache回收而改变。清理只处理固定格式生成的空工作目录/未提交staging文件，未知非空证据保留。

`make -C integration checkpoint-reclaim-current-check`运行九阶段中断、四次真实子进程退出且无defer、fresh owner恢复、源/计划/namespace/保留/云端/预算/文件变化拒绝、正常Go与对象API两个HTTP端口的apply/resume/status、单ledger与累计幂等、删除本地副本后的远端protobuf恢复，并回归原双Rust预览/同目录全JSON互通及实际Kafka owner-router恢复。全部执行只操作测试生成temp对象；本轮没有自动触发、Rust执行、真实对象锁或多区生产耐久资格。

### Go周期checkpoint自动配额维护（阶段116）

在阶段115 local_retention策略中显式设置`auto_reclaim: true`才开启。前提为Go online owner、`reclaim_enabled: true`、有效`plan_ttl_ms`、`checkpoint_store.local_budget`、正`publish_interval_ms`和24..8192字节管理API token；省略/false保持原行为。Rust配置可读取并核对同一Go配置摘要，Rust owner执行仍拒绝。

正常周期worker在同一publication gate和总timeout内，先检查/恢复原pending冻结集合；未完成不归档，完成后本周期不再选择新集合。若无pending且公开cache准入因准确容量超限失败，最多调用一次AutoReclaim并重新准入一次；空eligible不创建journal，未知目录/读取预算等失败不触发删除。普通`POST /internal/checkpoints`保持显式手动回收要求。回收继续先核验配置store原远端备份、当前ref/cursor/candidates/最新保留与耐久journal，线上owner状态不变。

checkpoint status及Prometheus增加自动尝试/失败/完成/空计划计数，checkpoint.reclaimed日志带automatic=true；不另开清理worker或HTTP自调用。`make -C integration checkpoint-auto-reclaim-current-check`验证正常周期达到配额后一个批次恢复发布、无eligible无忙循环、pending恢复与下一新选择分周期、远端不可用/恢复、关闭取消及手动发布语义，并回归真实进程崩溃、双owner预览及实际Kafka独立router恢复。范围仍为测试生成本地对象与单机loopback服务。

### v2-complete native Pair/Factory状态承诺（阶段117）

在known-evm-native-commitments-v2模式下，正常ReadState读完v2-complete后强制读取Pair/Factory的EIP-1186证明，原native session共享请求/响应/证明/输出预算及context。复核receipt/state authority后按当前blockHash/requireCanonical取proof，将原runtime SHA256与MPT account Keccak双绑定；Pair slot5/6/7的Factory/token0/token1、slot8储备/最后时间、slot9/a累计价、slotc锁，与读取的Pool逐项比较；Factory原slot2嵌套getPair映射两方向均证明。Coreth逻辑slot沿用既有物理key转换，不另造proof解释器。

任何不一致都拒绝整个pool batch并耗尽原session，不能再次尝试使用剩余预算；正常最终canonical/settling重验保持。legacy模式不伪称新资格。Token balanceOf仍缺对应代码/layout或完整执行证明，RPC header并非共识checkpoint，因此本轮不开放生产VERIFIED。

`make -C integration v2-native-state-proof-current-check`冻结源码与输入，使用原合约bank/runtime/storage构造真实MPT和合成自哈希header，通过正常Adapter入口验证成功与篡改/缺proof/预算/取消反例，并运行EVM全包、原有正常native服务、wire/config/reducer、契约、vet和Go当前程序构建。合成header不宣称实际公链部署/共识资格。

### v2-complete native Token余额证明（阶段118）

阶段117的native状态路径现在继续对Token0/1的balanceOf(pool)验证。读取同native block的account proof及实际代码，验证代码Keccak；使用固定Berlin稳定指令只读重放，不声明完整链规则。SLOAD前若slot没有证明便立即停止，不允许默认零参与分支；由原session读取sealed逻辑slot证明后在新overlay重放，最多8slot/9pass，每pass100kGas。已证明空storage root可安全得到零，无须单独请求key。输入固定36byte，输出严格32byte，代码最多24576byte；caller/origin/value固定零，与parser eth_call相同。

CALL/DELEGATECALL、外部账户、区块/费用/GAS、写入、PUSH0及不支持指令均拒绝；read-only内核不带网络/外部状态读取，只有上层受同一总预算约束的proof请求。每opcode检查ctx，私有停止sentinel仅在本边界恢复，其他panic不吞掉；overlay/内存DB每pass关闭。该结果只证明view返回值，不升级税费/rebase/转账行为、共识或生产VERIFIED资格。

`make -C integration v2-native-balance-proof-current-check`使用原runtime/storage真实MPT及明确合成header，验证正常六份Pair/Factory/Token证明请求、两余额与原bank一致、零值分支与空root、Getter/承诺/代码/缺proof/超8slot和不支持路径；回归全部v2/native/contract-storage及普通服务，检查现有geth锁定版本的三个新增传递依赖、契约、vet和构建。117报告是历史冻结范围，118报告记录当前源码的增强路径。

### EVM当前头确认策略（阶段119）

现有confirmation_tag对EVM只允许精确latest/safe/finalized。数字高度、earliest/pending、大小写或空白别名及空值在Go和双Rust编译器中均报CONFIG_CHAIN_CONFIRMATION，普通Go New在打开状态目录前同样校验。HEAD_FIRST的当前源策略不能混入固定历史游标；显式历史worker按其独立区间参数运行。

source-evidence同时披露配置confirmation_tag及confirmation_policy=CONFIGURED_PROVIDER_TAG，consensus_verified保持false。当前标签是provider声明语义，是否有能力提供由实际RPC探测/读取决定，不支持safe/finalized不能回退到latest或用URL数量形成新共识。该限制不改变Solana每profile finalized要求，也不宣称生产VERIFIED。

`make -C integration confirmation-policy-current-check`验证同轮Go15份完整配置（3通过/12拒绝）经双Rust独立编译的hash/拒绝码、Go普通构造前无目录写入、HTTP策略披露与不支持safe无降级、当前实际二进制check-config，以及两Rust配置全测/严格Clippy、Go格式/vet/构建、契约/进度/模块与输入冻结。

### 真实当前V2来源与公开字节重放（阶段120）

`make -C integration current-v2-public-source-check`固定Ethereum DAI/WETH原Uniswap V2 Pair/Factory及两个公开HTTPS读取端点，构建当前Go owner和Rust router并实际启动普通进程，关闭history且保留明确gap。native header/body/receipt/Pair/Factory/Token view按原总预算运行，RPC原响应由normal raw archive耐久记录；normal HTTP开发日志消费后双方向ExactIn/Out与独立Python整数结果核对，费用/VERIFIED仍保持明确未实现语义。

离线阶段验证Go .raw的外层原Payload字节hash、内层Ref/owner ID、内容hash/长度和provider ID，按原provider分开封闭loopback服务；同请求JSON顺序变化保留原文件，语义冲突/未知请求/写方法拒绝。实际停一router由另一无KV完整副本继续报价，冷重启恢复；owner同KV重新资格化当前基线且writer/generation/seq递增，接管不冒充reorg。五类getter/proof/不可用/header变更拒绝owner启动及router报价，所有进程/服务在成功或异常后清理。

native会话现区分来源拒绝与真实配额耗尽：拒绝保存terminal标志并撤销剩余额度，后续即使误填充counter仍拒绝；原始source错误返回而不叠加虚构预算错误。真实耗尽仍按原预算码返回。`--capture <本地live运行目录>`只做历史原字节回放，报告明确live_public_source_observed=false，不能替代--live资格。本轮仅一固定真实部署的当前state与INDICATIVE数学/恢复联验，不授予生产共识/独立交易执行收款、全市场、实际长停机、PG/Kafka或多区资格。

### 真实V2固定公开bank离线恢复（阶段121）

原始公开响应数据遵循仓库既有本地dataset政策。可跟踪metadata位于integration/tools/reference/current-v2-public/source-metadata.json，固定实际block26438995及32原raw/准确压缩hash。使用`python3 integration/tools/acceptance/restore_current_v2_fixture.py --bundle <准确responses.json.gz>`或--capture <原raw目录>恢复integration及Go自己的本地副本；脚本无网络、验证全部pin和嵌套source身份后才发布，不覆盖未知数据、不放宽hash。缺数据明确失败。

解包只读取固定gzip JSON/base64成员，不解tar、不解释任意路径，压缩与解码各32MB上限、最多1024成员，原raw总128MB，原成员/Envelope/Owner/内层响应全校验；重复/链接/逃逸/变更hash/输出冲突拒绝。Go测试仅读自己fixture和普通RPC adapter；实际公开header/body/receipt/MPT/token view与原完整Pool精确匹配，坏header/proof拒绝。使用与normal live相同60秒总来源读取上限适配真实大块race回归，不修改产品资源配置；首次预检10秒超时已明确定位。

`make -C integration current-v2-fixed-source-check`构建当前服务并只用固定公开源+封闭loopback运行正常owner/双router、无KV重建/owner升代与五故障控制；live_public_source_observed=false，不把历史捕获当当前实网。源码/所有本地固定输入及压缩包hash绑定到本轮report。固定输入不包含生产共识/原交易执行后余额或链费资格。

### 固定实际V2 bank原执行差分（阶段122）

当前测试源额外固定actual block26439090和33份原始RPC/raw。原字节码与MPT proof验证header-root、account code/storage/nonce/balance及原storage，不接受默认未知零。两个额外proof保存的是实际getProof Result原投影，未保留完整HTTP envelope；metadata明确该限制，且每份仍要对原header stateRoot重新验证。恢复命令`restore_current_v2_fixture.py --profile execution --bundle <固定responses.json.gz> --extra-proofs <固定extra-proofs.json>`只接受精确pin，无网络。

测试独立geth1.16.7执行固定本地合成EOA/原生币资金覆盖的原WETH deposit、transfer及原Pair.swap三call，实际DAI收款delta和全部Pair/raw后状态由执行观察生成。相同起点q+1必须返回原UniswapV2: K，防止低报也成交却误以为精度通过；Rust当轮独立quote_exact_in比较output及完整nextPool。账户/槽初值全部证明，未声明依赖/生命周期拒绝，每call2Mgas与总1Mopcode/64slot/4account为测试资源上限，不是费用模型。ctx逐opcode检查、未知账户/槽与取消资源反例通过后才可生成输出。

该边界是原交易逻辑的本地call序列，EOA/原生币资金显式合成（不声称真实该EOA的nonce/code/balance），不能称为已签名原Router完整交易、真实用户余额、未来区块执行保证或完整链费。`make -C integration public-v2-original-execution-current-check`冻结当轮Go执行输出并由Rust直接独立读取/完整比较，回归固定原v2连续bank、actual原证明/完整状态、普通native服务、守卫/模块/vet/Clippy/构建；P0-3费用与P0-4可信完整交易执行仍独立推进。

### EVM native cost内核与费用范围（阶段123）

execution-preparation::evm_native_cost集中核算Legacy/EIP1559原生执行费及最大reserve：Legacy有效单价为声明gasPrice，EIP1559为baseFee+min(priority,maxFee-baseFee)。gasUsed必须处于21000..已绑定gasLimit、base/priority不超cap，全部结果checked uint256；准备阶段同一reserve核算不变，simulation API使用execution base而非准备source base，逐项核对原worker测量。最大reserve不当作实际fee，reserveHeadroom不当作Token退款。

正常simulation响应保留原fees字段并增加costs版本/chainKey/binding、gasUnits字符串、gasNative、nativeKnownSubtotal、reserve/headroom和execution的burn/tip分量。burn/tip不再累计到execution小计；未知l1DataNative/approvalNative/totalNativeEstimate/net保持null，missingComponents列链特定data费/额外授权资金操作/可靠Token估值，deductedFromQuoteOutput=false。来源仍为PROVIDED_LOCAL_STATE/LOCAL_EVM_ONLY，不授予VERIFIED或FAST净排序资格。

`make -C integration evm-native-cost-current-check`固定56份当轮独立Python整数向量，由Rust完整读取比较；实际HTTP原Router worker执行ExactIn/Out，核对native caller/coinbase delta、有效价、成本与quote/binding保留，回归原16银行H与H+1费用区分、准备/VM/协议拒绝和资源取消、严格Clippy/构建/源与输入hash。P0-3的完整链费源、L2、授权/资金操作和可靠估值仍需后续实现。

### EVM原生费用余额源绑定（阶段124）

parent在序列化实际worker输入时保存caller与H+1 coinbase的原bank余额；已声明不存在可得零，未声明不可默认零。EIP1559返回的caller_before/coinbase_before先与该输入精确绑定，再要求zero-value bound ERC20范围的余额守恒：不同账户扣execution fee/增tip；同账户两个before/after投影相同且净扣base burn。防止同时平移测量前后余额仍通过差额检查。未知余额、scope外native value流、无效账户/uint256溢出或不守恒拒绝。

响应保留原费用语义，补executionCoinbase、costs.nativeUnit=EVM_NATIVE_ATOMS和balanceReconciliation（MATCHED或Legacy测量缺失UNAVAILABLE）。字段名MATCHED只表示指定local bank/上下文核验，不是生产VERIFIED；价格/额外费用缺项继续null。

`make -C integration evm-native-balance-audit-current-check`在原费用门禁上，实际HTTP原Router ExactIn/Out各运行分离/alias两个情景，共四owned worker，核对真实native debit/credit。alias是显式合成H+1 coinbase环境（新环境标识，source H保持），原native数据/所有fee算术/准备/VM/取消/Clippy/构建和冻结输入完整回归，不广播任何交易。

### EVM最大资金准入与付款人净扣款（阶段125）

evm_native_cost::validate_maximum_funding要求原worker输入的payer native before覆盖已绑定gasLimit×maxFee；Legacy也必须满足该准入。执行gross charge保持原gasNative/nativeKnownSubtotal含义；audited_payer_cost复用原余额守恒，以实际before-after生成payerNetNativeDebit，alias将priorityFee单列priorityReturnedToPayerNative，要求net+returned=gross。零base/自付tip导致零net也不能跳过最大资金。

costs明确nativeKnownSubtotalKind=GROSS_EXECUTION_CHARGE、payerCostModelVersion=source-bound-native-payer/1及maximumFundingAdmission。Legacy缺实际after时两个net字段保持null。make -C integration evm-native-payer-cost-current-check覆盖四actual HTTP worker及源/输入冻结回归。完整链费、可靠Token换算与净收益评分仍不具资格。

### 无可信成本时的正常报价排序（阶段126）

正常ExactIn在链费/可靠Token估值均未具备时，用零数值的排序扣项优化毛输出，并继续遵守maxHops/maxSplits/候选/网格/预算；这不代表实际链费为零。ExactOut沿用最小requiredSellAmount目标。两模式costs给出unknown Gas/native/data/授权/总费用null、chainFeeCompleteness/tokenValuationStatus=UNAVAILABLE，net不输出；quality.costRanking公布raw目标与限定搜索范围。

原proposalActivationCostBuyToken现在表示实际proposal计算采用的值（零），ignoredConfiguredActivationCostBuyToken单列未采用的配置值，configuredActivationCostApplied=false。既有独立plan projector继续使用实际proposal cost重放，同一planHash内的allocation保持一致。optimizer独立显式cost接口保留用于测试/研究，不能被视为已具备可靠成本来源。make -C integration cost-unavailable-ranking-current-check包含三池独立整数反例、原ExactOut独立交易grid、预算/用户约束与四HTTP原执行费用回归。

### Solana准备消息原生费用与资金分类（阶段127）

Solana普通准备器的共享Capture增加单次getFeeForMessage观察，保存实际查询消息、minimum/observed slot及canonical解码RPC Result投影摘要（非原始HTTP envelope）；ready后统一核对消息与unsigned transaction（单签名legacy/零签名/1232字节上限）、费用字段与账户快照payer余额。费用minimum slot必须等于捕获账户slot，observed可以更高，来源明确MIN_CONTEXT_SLOT_NOT_SAME_BANK；不能仅因满足minContextSlot就声称同一bank。

网络费用小计只含RPC实际返回的message fee，租金押金、预期可退/留存租金、swap/wrap本金单列。总资金=fee+rent+principal，checked u64且原payer余额覆盖；预期退款不能抵消交易开始前的资金要求。costs含chain/genesis、plan/view、message/fee-response/account-snapshot摘要及blockhash/有效高度；加入后重新seal preparationHash，正常API及独立SBF/签名流程保持完整绑定。sourceAuthority为PREPARATION_RPC_OBSERVATION，chainFeeCompleteness为PREPARED_MESSAGE_RPC_ESTIMATE；总估值与net仍null，不升级历史银行/生产资格。make -C integration solana-native-cost-current-check验证正式20个HTTP/SBF场景和资金/捕获/钱包回归。

阶段127冻结验证完成：20个正式HTTP准备/原SBF与独立Python审计、资金/捕获/钱包回归、准备/API严格Clippy及正常构建通过。单次费用观察与账户slot可以不同；该费用来源仍为RPC估计，不授予历史bank、完整费用、净排序或生产VERIFIED资格。

### Solana签名提交前费用上下文（阶段128）

preflight先继承准备账户及fee的最新slot，当前getFeeForMessage须返回有效context且fee金额不变，账户读取/sigVerify=true仿真/sendTransaction的minContextSlot依次前进。回执feePreflight记录原签名message及transaction摘要、准备/链/fee Result投影和各slot，仅声明RPC观测，非同bank或生产资格。

新增可选feePreflight对旧schemaVersion=1日志省略序列化，旧entry/identity hash保持精确；新字段进入日志和不可变identity。并发重复reserve以已保存的凭证为准，允许同一签名的观察时间变化但不改写首次intent；后续update变更/删除凭证仍拒绝。make -C integration solana-fee-preflight-current-check冻结原SOL/WSOL钱包、实际签名一次发送与旧日志兼容验证。

阶段128冻结验收完成：实际八种签名生命周期、六种费用/slot守卫、并发一次发送、重启凭证不变与旧v1精确日志兼容通过；Pump/Raydium原SOL/WSOL资金钱包回归和严格构建通过。仍为提交RPC观察约束，通用net估值、净收益路由与可信bank/链规则资格继续未完成。

### Solana限定结算的原生同单位经济净额（阶段129，限定本地验证）

native_economics对正常SBF证据和签名回执共用核算：仅原生SOL或显式临时WSOL退款/解包可用NATIVE_UNIT_IDENTITY。实际input+fee或output-fee形成单侧net估值，未涉及的一侧明确unavailable reason；fee超过output保持null，零值保留。经济净额排除可退/留存资本押金，原生现金另按实际fee/rent/principal核对；不调整原quote或optimizer排序。

来源分开标明本地SBF、RPC捕获SBF和RPC回执，均不升级历史bank/生产VERIFIED。实现已通过正常HTTP/11场景原SBF及实际签名回执、独立原余额审计和16项冻结门禁；20原准备家族回归通过。签名wire/meta是受控本地原程序执行的实际结果，不冒称主网或原始HTTP归档。

### 已执行Solana费用后金额的前端展示（阶段130，限定本地验证）

nativeEconomicsText仅展示当前transaction/preparation/chain绑定的经济净额，区分本地SBF与RPC回执来源，留存租金作为资本另列。原金额字符串/BigInt保留精度；旧costs缺失为空、普通Token或负净输出显式不可用，FAST报价不伪造净排名。首批自动检查通过但被截图复核拒绝后，已将链特定签名区与耐久回执区分离；五类正常SBF/签名回执、重载实际可见、18前端单测/16浏览器检查和8项冻结门禁及桌面/手机截图复核通过。保留首次失败结果，旧回执仍可读；通用价格、净排名和失败交易费用核算继续未完成。

### Solana失败签名交易网络费（阶段131，限定本地验证）

`GET /v1/transactions/{signature}`对已确认InstructionError读取原签名getTransaction；`failureFee`与成功`settlement`分开，绑定签名/wire hash/slot/status err、meta投影摘要及实际fee。payer只扣fee，其他已报告native/Token前后状态回滚，已知原有Token账户元数据必需。processed/元数据缺失及confirmed保持可重查；finalized完整核对才FAILED，矛盾SETTLEMENT_MISMATCH。费用观察可随签名消失撤销，旧v1日志兼容；不宣称完整账户data回滚证明、nonce支持、通用失败类别或可信历史bank，净额保持null。

阶段131限定验收通过：三类实际失败签名原程序、18元数据故障与恢复、30独立审计篡改及原成功/钱包回归；13门禁、52次Rust和冻结源码/输入摘要通过。原账户data完整回滚证明、其他TransactionError/nonce和生产资格仍不在该核验范围内。

### 失败交易网络费的页面语义（阶段132，限定本地验证）

回执区依据failureFee展示已核对的RPC失败网络费/付款人原生差额/已报告余额回滚；confirmed标待最终确认，元数据未齐标待核验，矛盾或旧回执不猜零。chain、preparation、signed transaction、signature、slot、错误与确认状态均需一致，金额为规范u64字符串并以BigInt验证守恒。失败不展示成功收付和net；成功对照继续显示原5971净收款。刷新/重载只查签名，不自动再次发送。

阶段132限定验收：9门禁/22前端测试/13真实浏览器检查，三失败与一成功对照及重载/手机原图人工复核通过；原生资金/账户创建失败不保留租金，成功对照净收5971，二者不混用。费用与bank资格仍遵循原API边界。

### Legacy本地执行原生币测量（阶段133，限定本地验证）

正常worker为Legacy和EIP1559都从原image及实际afterstate返回native_fees。Legacy effective gas price等于明确gasPrice，priority=gasPrice−baseFee；caller/coinbase分离时payer扣gross execution，重合时净扣burn、自付priority分列返回。API校验before绑定原worker输入、最大reserve及同一checked费用模型，再返回payerNetNativeDebit；旧Legacy无测量仍保持未知，EIP1559缺测量仍拒绝。完整链费/市场估值/当前主网链规则资格不由该测量推断。

阶段133限定验收通过：14门禁/148次Rust、16原银行、Legacy/EIP1559各四正常HTTP和56独立整数向量；成功/回退/HALT的实际原生测量、source-bound前余额、alias自付priority和最大reserve守卫通过。旧缺测量兼容保留，完整链费/价格与生产链规则资格仍未开放。


### OP Regolith历史L1数据费（阶段134，算术内核）

`execution-preparation::op_l1_cost`提供`decode_parameters`和`data_cost`共享Rust入口。只接受OP/Base已知Regolith至Ecotone前的时间域及Bedrock属性ABI，完整编码user wire按4/16计数据Gas，加入原overhead，乘原L1 baseFee/scalar后向下除1e6；gas与中间乘积保留宽整数，最终native fee限uint256，显式零有效。结果保留属性/交易字节摘要与原L1时间/区块，调用方必须另外认证交易格式/签名、deposit/链身份、block承诺与时效；本模块不赋予这些资格。输入预算由调用方明确传入。

固定原op-geth函数以独立Go执行，与三笔留存公链signed wire及RPC l1Fee/l1GasUsed对照；synthetic算术向量只验证整数，非真实执行证明。当前没有将该历史内核接入正常报价链费/付款人完整净成本或路由排名；现行OP模型、Ecotone首块和后续版本、完整来源/链规则与生产资格仍关闭。

阶段134内核限定验收通过：16门禁/147次Rust、75原Go向量（三原signed交易与72算术探针）、普通/优化参考守卫及8次原worker HTTP回归。完整来源接线与现行OP费用继续待实现，报告与首批文档失败记录见进度文档。


### OP Regolith费用输入来源与历史重放（阶段135）

Go普通native来源及archive replay在同一header/body/receipt证明之后，派生可选`OPRegolithFeeInputs`；仅历史Regolith时间域启用。首L1属性deposit的发送者/目标/零mint和value/system标志/260字节ABI必须正确，user原wire匹配body的hash/index、chain ID并恢复签名者；投影只有事实，无L1费用数学。分配与保留费用计入原共享递减证明预算，不新增RPC调用；正常最后锚点不变。非支持链/版本不提供该投影。

历史记录保存可选投影，重放用原证据重新派生并核对，旧缺省仅兼容原事实，不是零费用。compact header摘要兼容RawMessage保存时的空白压缩；attributes/raw user字节精确保留。Rust消费正常来源导出的三笔输入再对原op-geth核算的测试不授予共识、独立L1证明、完整现行OP执行或报价净成本资格，正常API全链费接线仍待继续。

阶段135追加sourceHash核验：L1-info deposit必须使用原domain 1，由属性内L1 hash和sequence计算并与原wire的sourceHash一致；原Optimism方法固定摘要并执行25独立参考。该内部关联不授予L1包含性/最终性资格。首批总控已主动取消并保留，补充后重跑完整验收。

阶段135限定验收通过：18门禁、148次Rust及209次Go（203 EVM race、6正常服务race），三原signed输入从普通读取/历史重放交Rust与原参考核算一致；25原sourceHash及12投影篡改/6来源故障/2发布拒绝/15语义预算控制通过。源码、911固定输入、45运行pins及8参考摘要一致；旧缺省归档与空白兼容保留。完整现行OP费用/报价净成本接线和可信估值继续未完成。

### OP packed属性及显式现代费用算术（阶段136）

`execution-preparation::op_packed_cost`只解码164/176/178三种精确selector/长度，不代替chain/fork/首块选择。显式Ecotone按4/16字节Gas，Fjord对完整user wire采用固定原FastLZ长度/最小100字节的scaled回归；宽中间值保留、最终uint256。Isthmus operator为gas×scalar/1e6+constant，Jovian为gas×scalar×100+constant；属性缺operator参数时不可用，不默认零。来源、签名、fee-charged gas/refund及时效需调用方认证，16MiB硬预算与调用方更低预算并用。DA footprint仅保留属性，不加成一笔假费用。

当日原OP块159697405的完整JSON/逐笔receipt经普通native读取及历史重放验证header/body/receipt承诺，49用户签名恢复后交固定原op-geth函数和Rust对照。605输入中556为明确算术探针；额外RPC费用字段不是receipt trie承诺。旧Regolith入口不扩展，正常报价完整链费/净扣款、版本选择与生产资格仍待接线。阶段136限定验收已通过18门禁/150次Rust/6次Go和605原参考（49原signed用户、556探针、103溢出），冻结源码/输入及正常来源重导出摘要一致；结果统一记录到主进度文档。

### OP现代费用版本来源与真实升级首块（阶段137）

普通native coverage/历史归档可选`op_packed_fee_inputs`仅在同块header/body/receipts重验后派生；记录固定native语义、chain/版本、属性/deposit sourceHash和完整user wire/签名者/index及承诺累计Gas差分，无费用金额。四升级首块保留旧属性ABI且仅deposit，不推断缺省operator为零，任何user拒绝；后继采用新规则。现代和旧Regolith字段按时间域隔离，旧归档缺字段继续原事实。

真实升级创建deposit省略to的normal解码兼容空目标，对照固定原op-geth；删除普通call目标仍因原txID/body承诺不符拒绝。8实际升级块/128后继原signed用户和当日49用户经普通读取/归档重放交Rust对固定原函数；60投影篡改/15语义预算故障与4升级user、1unsigned后继控制不扩大来源权限。阶段137最终19门禁/153次Rust/214次Go通过，冻结源码/911固定输入/45运行pins/148参考及正常来源重导出摘要一致，原Go状态读取与Rust费用内核限定联验完成；正常报价完整链费/净估值和生产资格继续未开放。


### OP完整费用账户与原状态转换（阶段138）

`execution-preparation::op_chain_cost`计算明确零value fee-only范围的execution/L1 data/operator总费，分开effective预付、fee cap最大资金及execution/operator退款；独立的源before/执行after按payer/coinbase/三vault的实际地址聚合，base fee入vault而非burn。ABI与显式版本不符、缺参数/账户、金额总和溢出、资金不足及不支持native value拒绝；源与签名认证由调用链负责，组件结果不自动获得可信来源资格。

完整固定原op-geth源码经归档逐文件核验，在独立模块无改动执行`core.ApplyMessage`，真实本地signed Legacy/type1/type2、五费用规则、成功/退款/REVERT/HALT及五coinbase别名共320场景，955原状态转换核对费用账户、nonce、回滚和精确最大资金边界。初始wrapper曾因未完成storage初始化而误报过低Gas；独立REVM差分发现后取消验收，先Finalise(false)并硬查原始/当前storage相等，撤回基于失真参考的共享算术/API改动。原客户端本地bank不等同历史公链bank；修正初始状态后256个历史版本场景由独立REVM核对Gas/费用账户/nonce/目标storage一致，Jovian64个场景明确无REVM资格；该对照已加入本轮门禁，阶段138统一24门禁/151次Rust、320原Go场景955状态转换/256独立REVM与8正常HTTP回归通过，冻结源码/911固定输入/45运行pins/1419参考及五产物摘要一致，正常报价完整链费/净估值及生产资格继续未开放。


### OP完整签名与严格历史VM（阶段139）

`simulation::op_transaction::authenticate_user`认证完整EIP155 Legacy/type1/type2签名wire，校验chain/hash/恢复caller、规范RLP及整数/签名/访问列表预算，私有`AuthenticatedUser`仅提供只读环境。`revm_backend::execute_op_user`消费该对象，REVM实际执行字段和L1计费完整wire始终来自同笔交易；范围为OP/Base已固定Regolith到Isthmus七规则的零value普通Call。Jovian及之后、upgrade activation首块、deposit/blob/type4、原生本金/内部原生转移不具备本入口资格。

执行前在同一StrictDB预算校验L1Block账户、实际storage和属性费用输入，checked最大资金；执行后独立共用完整费用组件核验实际Gas/费用账户/付款人净扣款，且在审计后再检查取消/期限。既有Shanghai路径显式mainnet handler并拒绝OP字段，现有HTTP/worker不自动改用OP。来源owner仍需认证block/state和全部依赖，库内签名恢复不提供历史bank/共识资格。本轮20门禁/160次Rust、448原signed场景/1337原状态转换、896产品执行/441资金拒绝、180归档公链签名认证与8正常Shanghai HTTP回归通过；最终限定结果见[阶段139](../DEVELOPMENT_PROGRESS.md#plan-2026-11-18-op-signed-execution)。


### OP认证BOUND与完整Router执行（阶段140）

`evm_fixture::simulate_bound_op_user`与`simulate_bound_op_user_exact_out`分别复用既有BOUND重验、完整代码身份/Pair状态、Token balanceOf与实际后状态观察；签名caller/to/value/data与计划必须一致，全部nonce/Gas/费用来自私有认证对象。同一OP规则的观察不计费/不改nonce并丢状态，主swap才执行完整L1/operator/VM费用；完整signed wire与属性额外计入共享输入预算。历史OP Ecotone+按原客户端无blob规则使用excess0/BLOBBASEFEE1。

结果单列`op_fees`的原交易hash/type、完整checked链费/付款人净扣和费用账户before/after；Shanghai继续原native_fees格式及hash兼容，正常unsigned worker/API拒绝OP费用回执，尚未开放正常OP执行。18门禁/157次Rust与8正常Shanghai HTTP回归通过；448个本地构造完整Router的原op-geth差分、448次BOUND及448次直接OP执行一致，覆盖七规则、Legacy/type2、ExactIn/Out与全部声明账户/code/nonce/storage；不提供历史bank/共识或当前Jovian及后续执行资格。最终进度见[阶段140](../DEVELOPMENT_PROGRESS.md#plan-2026-11-18-op-bound-router)。


### OP受限worker签名协议（阶段141）

现有私有`WorkerRequest`可选`op_user`携带完整原signed wire/hash/属性，严格hex/预算并在实际worker认证，再核验请求nonce/Gas/fee/chain与认证环境一致。只开放唯一BOUND ExactIn或Out，禁止prepared混用，历史fork/银行/全部预算继续由阶段140路径检查；无新服务/端口/交易发送。旧请求省略字段保持hash/序列化形状，正常API仍只传None且拒绝未绑定OP费用回执。19门禁/159次Rust通过，七规则56实际router子进程完整回执一致、17拒绝与8旧Shanghai HTTP回归通过，actual worker二进制及源码/固定输入在整个冻结区间一致，结果见[阶段141](../DEVELOPMENT_PROGRESS.md#plan-2026-11-18-op-signed-worker)。


### OP费用回执源一致性核验（阶段142）

`revm_backend::audit_op_fee_measurements`使用调用者保留的`OpExecutionRequest`和严格源image，按共享checked费用模型重算全部链费、最大资金、退款和payer自收credit/net；L1Block slot和费用账户均经过有界读取，要求报告before精确匹配源银行及唯一角色集合，after差额精确守恒。失败读遥测保留，取消/期限检查贯穿调用；返回读消耗必须计入父进程总预算。接口不执行VM，不能将费用一致性核验等同真实执行证明；当前正常OP API尚未接入，源银行/现行VM/完整报价净估值与生产资格不升级。实现和冻结结果见[阶段142](../DEVELOPMENT_PROGRESS.md#plan-2026-11-18-op-fee-measurements)。


### 正常历史OP待签名交易和独立执行（阶段143）

配置的本地`LocalSimulationBundle`可选`op_attributes_calldata`；只接受已明确准入的七历史规则和属性格式。正常build按缓存ExactIn/Out重新BOUND，返回完整`rpcTransaction`和nonce/Gas/Legacy或type2费用、H源/H+1环境及build hash；未签名前完整链费未知，ready=false且用途限定历史本地仿真。正常simulate新增可选`signedOpTransaction:{transactionHash,signedWire}`，属性来自固定bundle，其他调用者覆盖字段拒绝；源无OP时仍保持旧空请求及Shanghai模式。

`worker_op::OpReceiptCommitment`在父进程保留重新认证的签名和BOUND/source/limits，重算worker receipt/source摘要与nonce，再调用阶段142有界费用核验。父进程读数从worker已消费后的剩余预算分配，`costs.totalUsage`公开合计；所有有效worker仍重新核验实际二进制SHA，取消、期限和permit持有到回收。OP费用按execution（base vault/priority）、L1 data、operator、最大资金/预付/退款及payer净扣分开；额外资金操作与可信Token估值仍未知，不提升当前OP VM、可信bank、报价净排序或生产VERIFIED。原Shanghai章节只适用于无OP配置的请求；本节是显式限定扩展，OP prepared路径仍关闭。冻结结果及范围见[阶段143](../DEVELOPMENT_PROGRESS.md#plan-2026-11-18-op-normal-api)。

### OP预编译加密后端与上限（阶段144）

固定REVM19.7显式启用c-kzg/blst。历史Ecotone起提供实际KZG，Isthmus加入七个BLS；OP专用wrapper对三BLS执行513760/488448/235008字节上限、BN使用112687字节上限，先检查原Gas再拒绝输入，不进入超长原生密码学调用。该注册表同时用于真实签名执行及只读观察，原上游加密运算/折扣表复用。验收使用原op-geth语料与完整签名状态转换并复跑正常HTTP；不因此开放Jovian及后续或生产VERIFIED。具体状态和报告见DEVELOPMENT_PROGRESS.md阶段144。

阶段144统一验收通过，具体输入和报告绑定见进度文档。固定sha2依赖在dev/test使用opt-level=3，消除大可执行文件未优化摘要的期限压力；父进程和子进程仍每次完整读取文件并核对独立摘要，不使用身份缓存，原期限保持。当前OP规则的后续适配须尊重原registry的Karst边界，原op-geth历史执行不能充当当前链客户端资格。


### 历史OP Mainnet Jovian用户VM（阶段145）

`OpFork::Jovian`严格限定chain10在1764691201之后、1783526401之前，激活首块和Karst及之后拒绝；Base仍停在已核验历史范围。固定Isthmus handler验证源bank后，仅内部operator scalar缓存乘1e8，匹配Jovian的raw scalar×gas×100；真实source slot8不改，独立checked最大资金/费用和父进程来源审计均使用Jovian规则。预编译调用顺序不变，四上限改为81984/288960/278784/156672，真实执行及观察共用。

正常历史build/签名simulate使用178字节Jovian属性并沿用完整BOUND、实际worker身份、总预算和父进程费用源核验。单笔用户VM不执行DA footprint区块准入/共识，不授予Karst/当前链或可信bank资格；验证状态以进度文档阶段145为准。

阶段145统一冻结通过：64原签名场景、128产品执行/63不足1拒绝及128独立费用源审计，7056原预编译输入/160签名转换、32正常OP及8旧Shanghai HTTP均通过。当前Karst适配仍须独立验证Osaka相关Gas/输入限制/CLZ及具体客户端来源，历史Jovian不能充当现行链资格；最终source/输入/原报告见进度文档。


### Karst预编译组件与现代原参考（阶段146）

`karst_precompiles`在原产品锁版本上适配Osaka MODEXP的三个1024字节限制、500最低价和复杂度/迭代倍率，复用原模幂运算；零输出长度仍按exponent实际价格计费。P256价格改6900，Karst BN输入上限57600，现代OP表的cap/Gas顺序从未修改的固定原OP组件对照。历史Jovian及之前表保持各自原顺序。

`OpFork::Karst`当前只提供组件表，正常历史准入仍拒绝Karst，直到CLZ/Gas cap/完整原签名转换及正常API验收闭合。原参考在隔离Rust1.99环境编译固定官方spec/precompiles与REVM43.0.3，产品保持1.90及原Cargo.lock；认证原archive、锁依赖archive和实际源目录，组件资格不升级为完整现代客户端/共识或可信bank。最终状态见进度文档阶段146。

阶段146统一冻结30门禁/171次Rust通过，9452原组件输入与产品逐成功/Gas/输出/OutOfGas一致，32正常OP和8旧Shanghai HTTP回归通过。完整Karst VM仍不准入；下一轮补CLZ及交易Gas上限并验证完整原状态转换，不能仅凭预编译通过升级当前链资格。最终摘要和证据见进度文档阶段146。


### Karst CLZ/Gas cap与完整原VM组件（阶段147）

Karst的0x1e CLZ在instruction table的inspector包装之前注册，执行Gas 5并使用安全栈接口；每步仍统一核验指令/取消/期限。普通交易Gas cap 2^24在源bank读取前检查，内部观察受自身预算限制。Jovian/operator私有缓存倍率适配也用于Karst组件，真实源slot不变。

本轮固定完整官方op-revm与REVM43.0.3，不改原handler；完整signed wire独立恢复、费用/nonce/code/storage转换和拒绝回滚在468明确本地场景对照。超宽来源basefee显式改为现代原BlockEnv的u64最大值，保留宽signed费用字段并记录转换，不宣称原超宽环境获现代资格。公开Karst schedule仍关闭，测试仅对同一私有内核绕过schedule，正常Karst API/可信bank/区块派生/共识资格未升级；最终结果见进度文档阶段147。

阶段147统一冻结35门禁/173次Rust通过，468原场景（401产品执行/67拒绝）逐Gas/输出/费用账户/code/nonce/storage一致；257 CLZ结果、真实签名Gas临界值、指令预算和取消/期限守卫通过。9452预编译及32正常OP/8旧HTTP回归通过。公开Karst准入继续关闭，下一轮完成正常BOUND/worker/父审计/API后再扩大规则准入，源bank/共识和报价净估值仍须分别验证。

### 配置预言机授权与交易费用换算（阶段149）

对应设计§13.1/14.2/14.4/15.3。仅`dex-router`的`revm-fixture`可配置`simulation.native_valuation`：

```json
{
  "observations_file": "prices/source-block.json",
  "policies": [{
    "source_id": "operator-approved-source",
    "chain_key": "evm:10",
    "genesis_hash": "<配置中同链的canonical 0x加64位小写hex>",
    "token": "<canonical EVM Token地址>",
    "signers": {"source-a": "<32字节公钥的小写hex>", "source-b": "<另一32字节公钥的小写hex>"},
    "threshold": 2,
    "max_age_ms": 60000,
    "max_validity_ms": 60000
  }]
}
```

占位符必须替换为真实配置值。私钥不存入配置或请求；公钥、阈值和时间策略进入public/economic hash，轮换配置后旧policy签名失效。至少两把不同批准公钥不自动代表两个独立来源，可靠来源的选择由部署方负责。

价格book采用`schema_version=1, prices=[{observation, signatures}]`，最多1MiB/128价格；每observation包含schema_version、policy_hash、anchor、token、token_atoms、native_atoms、observed_at_ms、valid_until_ms。anchor完整包含chain_key、genesis_hash、block_number、block_hash、parent_hash、timestamp_ms，必须与加载bank相同。签名消息为ASCII `xone-native-token-price/1:`拼接共享规范JSON `hash(observation)`的64位hex文本，Ed25519签名按`{key_id, signature}`编码。全部提供的签名均须有效，重复/未知key、未知字段和错误anchor拒绝启动；book在服务生命周期内不可变。

比率表示`token_atoms / native_atoms`，直接作用于两币原子单位，不另乘decimals。费用为`ceil(payerNetNativeDebit * token_atoms / native_atoms)`，保留完整乘积后除法，仅最终结果限制uint256；该净扣来自worker执行后父进程的源余额和链费审计，不能把gross费用再次扣给自收tip的付款人。观察时间不得晚于源块，source/execution均检查age，execution等于valid_until即过期；使用历史块时钟，不冒充当前市价。

`costs.transactionEconomics`分别提供`feeAdjustedBuyAmount`（实际输出减换算费用）和`feeAdjustedSellAmount`（实际输入加换算费用），并返回book/policy/observation hash、签名者、anchor、时间、bindingHash、付款人、recipient与不可用原因。缺价/过期/未来价格、溢出、输出不足保持null；失败执行返回`EXECUTION_NOT_SUCCESSFUL`。这只是单笔已执行交易网络费核算，额外授权/资金交易成本与报价候选排序另行实现；顶层报价net及完整总成本继续null。

验收生成器`integration/tools/acceptance/native_valuation_vectors.py`使用公开固定测试key和合成比率，只证明配置签名授权及独立整数核算。正常API不携带这些私钥，也不接收请求者价格；当前未提供live market adapter、可信bank/共识或生产VERIFIED。阶段149最终验证状态以进度文档为准。


### 逐完整候选费用评分（阶段150）

设计§13.1/13.3/13.5的优化器组件入口为`optimize_with_costs`与`exact_out::optimize_with_costs`。回调收到经共享状态顺序重放的完整allocation，返回`Some(cost)`或`None`；成本分别使用buy/sell Token原子单位。回调所属服务负责实际执行费用和来源/时效/单位授权，并遵守已有取消/总期限；优化器不认证价格或bank，HTTP请求不能借此提供任意成本。

每个完整可行候选都在次数/时间预算内评分；部分seed不计完整交易成本，任意方案成本入口关闭只适用于可加目标的独立池DP。任一完整候选缺成本时，返回相同已评估集合的毛额最优，`cost_adjustment_applied=false`；模型错误和uint256成本溢出直接失败。ExactIn比较输出减成本的有符号整数，负数不截零；ExactOut比较所需输入加成本的完整宽度整数。`ranking.signed_objective_token_units`表示该经济目标，`selected_cost_token_units`另列，allocation中的旧net字段不代表本入口的费用后目标。

`grid_exhaustive`仅涉及传入候选、有序整数网格、max_splits和每候选最多一次；预算截断明确披露。结果不证明网格间/范围外最优，也不授予真实成本或生产资格。阶段150只补算法组件，正常报价在执行/成本模型接线前继续原毛额回退。


### 正常报价本地候选执行费用（阶段151）

可选配置`simulation.candidate_cost_max_plans: 1..16`用于正常ExactIn/ExactOut的受限本地逐完整候选执行；默认0省略序列化。仅Router/revm-fixture、已配置native_valuation且routing.max_splits=1可启用，避免把当前V2单路线BOUND外推为拆单交易。三语言配置验证与public/economic身份同步。

每候选保留独立plan/binding/calldata、源view及固定deadline，通过现有实际worker、父费用源核验与授权价格取得buy/sell Token原子费用，再交逐候选优化器比较。评分后最终Context不续期；额外simulate和内部evidence保持同计划。独立仿真permit、每请求计划数/总期限、取消及子进程回收约束同时生效。

`quality.costRanking.scope=LOCAL_EVM_TRANSACTION_EXECUTION_FEES_ONLY`，`costAdjustmentApplied`仅指该局部目标。`optimizer`包含有限已评估完整候选计数及宽整数经济目标，`simulationAttempts/maxSimulationPlans/simulationBudgetExhausted`说明实际执行预算，`unavailableReasons`说明统一毛额回退，`candidateExecutions`和`selectedExecution`绑定各自planHash。达到计划数不会把剩余候选成本当零；worker协议/身份/繁忙/超时错误直接失败。未签名OP在评分前保持未知，`OP_SIGNED_ENVELOPE_REQUIRED`不执行候选，也不猜测完整压缩wire费用。

额外授权/资金操作、通用链额外费、可信bank和真实市价仍未具备；`completeQuoteCostRanking=false`、`productionVerified=false`，正常报价继续INDICATIVE，顶层完整net/总费保持null。此处的配置签名价格没有独立来源/市场正确性或bank共识证明，验收只用固定测试签名者和合成比率；不应把局部费用目标作为完整报价净额。

### 批准的Ethereum Chainlink USD双feed（阶段152）

对应设计§13.1/14.2/15.3/21.2–21.3。`PricePolicy.market_feed`是可选策略；省略保持旧配置和观察的规范hash。启用时仅准入`evm:1`及Ethereum创世身份，固定native decimals=18、Token decimals=0..36、feed decimals=0..18。每条feed批准具体proxy/aggregator地址、两者runtime SHA256、USD描述、正int256答案上下界和最长24小时age；native描述必须为`ETH / USD`。这些字段进入三语言配置身份，改变部署或策略会使旧签名失效。

`PriceObservation.market_feed`包含原始code及`aggregator()/decimals()/description()/latestRoundData()` ABI返回、Token decimals返回、provider摘要和实际`source_read_at_ms`。产品严格解码并校验代码/地址/描述/精度，拒绝脏宽度、非正答案、未来/过期更新和不匹配轮次。本限定profile要求正roundId、`answeredInRound == roundId`及非零`startedAt <= updatedAt`，这是准入限制，不是所有AggregatorV3实现的通用规则。`updatedAt`与采集时间分列，重新读取旧轮次不能续期。

整数比率为`nativeUsdAnswer * 10^(tokenFeedDecimals + tokenDecimals) / (tokenUsdAnswer * 10^(nativeFeedDecimals + nativeDecimals))`，全宽中间积经GCD约分后两侧分别检查uint256；执行费用仍在完整乘积除法后只ceil一次。整个原始证据和比率由既有观察签名覆盖，服务授权时重新推导并要求一致；批准feed缺证据、普通策略夹带feed或移除feed试图降级均拒绝。实际执行时间继续检查源轮次和观察到期，既有父费用审计、候选绑定及未知回退不变。`buyPrice/sellPrice.marketFeed`披露具体来源/更新时间/代码摘要和证据hash。

操作方使用`integration/tools/market/capture_chainlink.py --policy POLICY.json --rpc-url PUBLIC_HTTPS_URL --validator PATH_TO_xone-market-price --output NEW_DIRECTORY [--block-hash HASH]`。校验器通过`cargo build --locked --manifest-path dex-router/Cargo.toml -p xone-execution-preparation --bin xone-market-price`构建。它从stdin读取最多1MiB的`{policy,anchor,market_feed,valid_until_ms}`，只输出严格推导的observation及`signing_message_hex`，不会签名或修改配置。采集器默认读取configured provider的finalized标签；显式hash模式不声明最终性，所有feed/code调用均采用EIP1898 `blockHash/requireCanonical`，结束时再次检查同高度canonical hash。

采集器保存原始响应、请求和摘要；只允许五种只读RPC、32次请求、每响应256KiB、总2MiB和60秒网络总期限。每次读取在拥有并回收的子进程内执行，缓慢分段响应不能无限延长总期限。禁止重定向及带用户信息/query/fragment的URL，失败诊断不输出端点或provider异常文本。签署方须独立审核同源块、部署/币种身份、上下界及观测，再以既有格式组装PriceBook；核心Router不持有生产私钥。

该适配器的真实性边界是“配置签署方认证的固定块RPC返回”，没有独立storage proof、bank共识或来源独立性证明。公开采集中的固定测试公钥与宽测试上下界不属于生产批准配置，离线重放保留原捕获时间，只验证当时的feed，不代表当前可用价格。服务内book仍生命周期不可变；持续live刷新/轮换、可信bank和全部链/Token覆盖继续未完成，不能据此开放VERIFIED或完整报价净额。当前限定结果见进度文档阶段152。


### 选中候选费用的正常报价披露（阶段153）

对应设计§13.1/15.3/15.5。显式启用候选执行费用模型时，正常ExactIn/ExactOut的`costs`复用`quality.costRanking.selectedExecution.costs`：发布前核对选中planHash、候选证据映射、quoteKind、binding、caller/recipient、实际输入输出及父费用审计。已应用评分时，费用必须存在且与优化器选中费用相等；错配直接拒绝。选中计划无执行证据或执行失败且未应用评分时，保持原未知费用，不借用其他计划。

公开费用`scope=SELECTED_LOCAL_CANDIDATE_TRANSACTION`、`selectedPlanHash`与`sourceEvidence`绑定同一计划/执行bundle/worker/view/snapshot。`quoteRankingApplied`描述本轮候选比较；内层`transactionEconomics.quoteRankingApplied`仍描述单笔核算自身边界。`tokenValuationStatus=CONFIGURED_ORACLE_AUTHORIZED_WITH_PER_SIDE_AVAILABILITY`只说明至少一侧有授权换算，各侧实际可用性仍以buyTokenFee/sellTokenFee为准。缺价允许披露已核验的局部原币费用。

`completeQuoteCosts=false`，完整`totalNativeEstimate/netBuyAmountEstimate/netSellAmountEstimate`保持null，`deductedFromQuoteOutput=false`；activation配置仍不冒充交易费。局部费用披露不授予生产验证、完整净额或额外授权/wrap等操作费用资格。缓存与内部evidence保存同一响应，最终simulate保留同一binding/calldata/outcome。限定验收沿用全部68门禁，新增选中费用逐字段一致性和错配/缺证据单测；执行记录见进度文档阶段153。


### 运行期授权价格发布与报价版本（阶段154）

对应§13.1/15.5/21.3/26.3。`PriceObservation.publication_revision`为签名覆盖的u64发布版本，零默认省略以保持既有hash和签名；新book中所有观察必须同版。`xone-market-price`候选输入可指定该版本，输出包含版本的观察和待签署消息，仍不签名。不得仅改book外层或HTTP参数来伪造版本。

操作方完成独立签署后，先以同目录临时文件/flush/fsync/原子rename替换配置中固定的`native_valuation.observations_file`，再向`POST /internal/native-valuation/reload`发送已认证的空JSON对象`{}`。服务必须配置非空认证token；请求不能指定价格、路径或策略。Unix采用非阻塞打开后核对普通文件句柄，目录/FIFO不进入读取；每次读取上限1MiB/128观察、签名quorum和原chain/genesis/源块/执行时间均重新校验，受现有仿真permit、总期限和取消控制约束；超时任务在发布前检查取消，不可迟到替换当前版本。日志只记录发布摘要/版本及有限资格。

相同规范bookHash重载为幂等；有变化时要求正版本严格递增、固定anchor/执行时间及币种集合完全相同，拒绝同版改写/进程内回退、撤除币种、错误签名、混合版本、未来/过期价格和错源。币种集合变更须随配置/启动单独审核，不通过热刷新悄悄降低完整性。任何错误保留上一个完整不可变快照。

每次报价捕获一个Arc价格快照，所有候选、选中费用披露和最终评分都使用同一bookHash/版本；requestHash和报价缓存身份包括该价格身份。`quality.costRanking.priceBookHash/publicationRevision`、`costs.sourceEvidence`及各候选transactionEconomics形成同源绑定。刷新后同请求会重新报价，旧quoteId的build/prepare/simulate不能套用新价格，返回`NATIVE_VALUATION_QUOTE_PRICE_CHANGED`；内部evidence保留原quote并以`price_current=false`说明其价格已变更。并发发布期间报价可按已固定的版本完成或明确失败，不混用候选价格。

该机制仅支持当前进程、固定本地bank下的显式发布。`restartRollbackProtected=false`；跨重启耐久高水位、自动采集/外部签署发布、持续live bank/共识与生产VERIFIED仍未完成。价格时钟继续使用已执行块时间，不因重载续期；本地签署资格和来源边界不变。完整费用/net和全候选完整净评分继续不可用。

限定回归入口为`python3 integration/tools/acceptance/validate_price_publications.py`。它在冻结前由`native_valuation_publications.py`重建/核对独立测试签名book，再执行原68项和4项发布门禁。生成的固定JSON与原始bank/报告沿用现有忽略目录策略，只提交生成器及验证源码；单独运行发布状态单测前可先运行该生成器。
