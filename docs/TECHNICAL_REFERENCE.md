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
