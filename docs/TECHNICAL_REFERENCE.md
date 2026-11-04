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
