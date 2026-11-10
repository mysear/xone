# Solana 公开来源、构建与原生重放技术契约

本目录定义公开 ProgramData 的部署身份绑定、完整 SBF 编译产物比较、sourcebank 输入格式及原生重放验收规则。资格必须由验证器根据原始字节和新的本地执行重算，不能从保存的 passed/status 字段取得。

## 原部署 SBF 执行

`r01_public_catalog_solana_bank.py` 直接加载原 catalog 固定的完整 ProgramData payload，包括所有 allocation padding。它按 Program 地址推导 ProgramData PDA，核对 Program 指针、loader owner、executable 标志、upgrade slot/authority、完整 payload SHA-256；再从一次新的 finalized `getMultipleAccounts` 保存 Program、ProgramData、池、vault、mint、Clock、tick arrays 等账户。同一次读取的 Clock.slot 必须等于 RPC context.slot。

运行环境是公开 solders 0.29.0 的原生 LiteSVM：真实 SBF 解释/执行、Solana transaction processor、CPI 与账户状态存储。**它不是从原始 ledger/snapshot 恢复的完整 Agave `Bank`，没有原 Bank hash/祖先链/历史 feature set 证明。** `sourcebank.json` 是公开 RPC 账户快照的输入清单，不是原生 validator snapshot。源码是否匹配不影响该原 SBF 执行的独立 authority。

从公开 RPC 保存的原 classic SPL Token 程序 ELF 替换 LiteSVM 默认 Token 程序；Memo 也使用采集的原程序。三项均使用真实池/vault/配置/观察状态，只在本地注入固定种子的 payer 及两个明确列出的 Token 账户。输入账户本地资助，输出账户从 7 个最小单位开始；收款 owner 与 payer 不同。WSOL 输入账户同时带正确 native reserve/lamports。没有用户私钥，没有向公链提交交易。

| 部署 | 新账户快照 slot | 输入最小单位 | 收款净增（最小单位） | CU | 滑点反例错误码 |
|---|---:|---:|---:|---:|---:|
| Raydium CPMM / WSOL-FUSE | 466416447 | 1,000,000 | 337,524,356 FUSE | 25,070 | 6005 ExceededSlippage |
| Raydium CLMM / WSOL-USDC | 466415607 | 1,000,000 | 90,091 USDC | 47,904 | 6018 TooLittleOutputReceived |
| Orca Whirlpools / SOL-USDC | 466415621 | 1,000,000 | 90,094 USDC | 29,833 | 6036 AmountOutBelowMinimum |

每项先将 minimum output 设为 u64::MAX：要求指定滑点错误码，两个用户 Token 账户、两个 vault、池状态完全回滚。随后 minimum=1 执行成功：从原始 SPL Token 账户字节计算净收款，并核对 input -amount、input vault +amount、output vault -net 的守恒和实际 Token CPI。手续费不会凭预填字段计算。证据保存签名交易字节、账户顺序/权限、原指令 bytes、完整日志/CU、原始 before/after 账户。

- CPMM 输入：`cpmm-sourcebank-correct/sourcebank.json`；结果：`cpmm-native-execution.json`。
- CLMM 输入：`clmm-sourcebank/sourcebank.json`；结果：`clmm-native-execution.json`。
- Orca 输入：`orca-sourcebank/sourcebank.json`；结果：`orca-native-execution.json`。
- `runtime-lock.json` 将实际 native library 与官方 PyPI wheel 的声明 SHA-256 及原文件绑定；安装只在 workspace 的 `integration/artifacts/local/r01-solana-rebuild/python`。

独立验收必须重新运行原 VM，不能只读取结果 JSON 的 passed/verified 字段：

```sh
python3 integration/tools/acceptance/r01_public_catalog_solana_bank.py verify \
  --sourcebank integration/test/source/deployment-catalog/solana-rebuild/cpmm-sourcebank-correct/sourcebank.json \
  --evidence integration/test/source/deployment-catalog/solana-rebuild/cpmm-native-execution.json
python3 integration/tools/acceptance/r01_public_catalog_solana_bank.py verify \
  --sourcebank integration/test/source/deployment-catalog/solana-rebuild/clmm-sourcebank/sourcebank.json \
  --evidence integration/test/source/deployment-catalog/solana-rebuild/clmm-native-execution.json
python3 integration/tools/acceptance/r01_public_catalog_solana_bank.py verify \
  --sourcebank integration/test/source/deployment-catalog/solana-rebuild/orca-sourcebank/sourcebank.json \
  --evidence integration/test/source/deployment-catalog/solana-rebuild/orca-native-execution.json
```

此命令核对 RPC 原始 HTTP body hash 与解析对象、文件 pins、部署绑定、runtime pins，然后新建 LiteSVM 再执行两笔交易，要求保存结果与新重放逐项完全一致。修改保存的净收款或仅写 `historical_original_bank_verified=true` 不能通过。无网络重放。

这三例限于当前池的小额 classic Token swap。没有声称 Token-2022 transfer fee/hooks、所有 tick crossing、流动性/奖励/管理指令或完整协议状态空间已验收。

## 源码与编译环境定位

四个旧候选及其失败报告全部保留，见 `baseline-audit.json` 与原 `solana-verifiable/`。`builds/` 中六份完成实编的比较结果均为 matched=false；数值和原始日志用于独立复核。

| 程序 | 构建样例的公开 commit | 编译目标与环境 | 编译文件 bytes | 第一个真实差异 |
|---|---|---|---:|---:|
| CPMM | 3d69122790e97cd6aff24e8ba95940793773f58c | Rust 1.89 / platform-tools v1.52；官方 Linux 原始 SBF sysroot；macOS native host | 749848 | 2332 |
| CLMM | 6d7a0f82d2f293c553c69aaf943377367c6c4cc6 | Rust 1.89 / platform-tools v1.52；官方 Linux 原始 SBF sysroot；macOS native host | 1498416 | 988 |
| Orca | cc683ec92acb5e7606caa3d8e514d9383e9c7c11 | Rust 1.79 / LLVM18 / v1.44，mainnet + 默认 whirlpool-entrypoint；Linux SBF sysroot；macOS native host；Cargo1.89 解析原 lock | 1495896 | 1450 |

表中三份比较报告的逐字节观测为：**完整 ELF header、完整 `.rodata`、`.dynamic`、`.dynsym`、`.dynstr`、`.shstrtab` 及所有 section 长度一致；`.text`、`.data.rel.ro`、`.rel.dyn` 存在真实差异，matched=false。** 源码 identity 的 authority 为 candidate，不能根据这些相同项推断全部源代码或语义已等价。

实际发现与实验证据：

1. CPMM 部署中有 `InitializeV2`；旧 f4b9b263 候选没有。公开 3d691227 含此实现，重建从旧 722640 bytes 收敛到 749848 bytes。API 保存的公开 commit 时间为 2026-11-01，而 upgrade 时间为 2026-10-31；公开发布时间不能被改写为部署构建时间，也不能凭时间关系排除后来公开的同一份源码。
2. CLMM 原 macOS v1.52 编译比部署多 8 bytes；原 `--remap-path-prefix=/Users/runner=/home/runner` 新编译完全没有改变产物，因为相关字符串来自已经编译的 sysroot。使用官方 Linux 原始 SBF rlibs 后完整 `.rodata` 和 ELF 长度一致，仍有真实 code/relocation 差异。
3. Orca 仓库官方 `scripts/setup.sh` 指定 Solana2.2.1；相应官方 SDK `install.sh` 固定 platform-tools v1.44，release 声明 Rust1.79/LLVM18。旧 v1.52 报告含 39 个 stack overwrite diagnostics，不能据此授予实际部署源码匹配资格。v1.44 的实编能够完成；Cargo1.79 会解析失败于原 lock 中 dev dependency 的 edition2024 manifests，故保留原始失败，使用 Cargo1.89 解析原 lock、保持 target rustc1.79。将依赖源路径通过编译器 `--remap-path-prefix` 映射为原相对 `src/...` 后，`.rodata` 和所有节长度一致。该命令属于公开输入上的定位实验，其 authority 不包括 publisher 精确原构建命令。
4. 这些步骤没有编辑上游源码/锁，也没有修改最终 ELF bytes 来凑 match。`llvm-objcopy --strip-all` 只按官方 SDK 标准构建后处理运行，随后比较它输出的**整个文件**；没有额外删 header、metadata 或尾部。
5. 另尝试完整 Linux host 编译：公开 Blink 在 workspace 从源码构建，官方 Linux rustc 的 `-Vv` 在公开 Ubuntu24.04 rootfs 下成功，但 Cargo 启动编译器子进程时遭兼容层 SIGKILL，日志有 clone3/线程退出相关未实现系统调用。该报告不构成完整 Linux native host 编译证据。混合 host 是否产生这些 code/relocation 差异属于未证明的原因假设。

`builds/` 保留实际命令、环境、源码/锁 pins、toolchain/library pins、日志引用、产物 SHA-256 和严格比较。`recipes/` 保存可重跑脚本；脚本使用已经存在的公共缓存，源档/官方 tool archive 的 acquisition receipts 在 `public/` 与原 `solana-verifiable/`。大工具、原生库、targets、registry/vendor 均只在 workspace 的 local cache；没有全局安装。

## 严格比较与隔离算法

`r01_public_catalog_solana_rebuild.py` 自己解析 ELF64 little-endian 的 SBF headers、program headers、section headers 和 string table，逐范围检查溢出/截断/重复 section name。它保存每节原始范围和 SHA-256，定位 machine、header、code、只读数据、relocations 等差异域。

接受源码重建的必要字节条件只有：

1. 实际编译文件的完整字节与 ProgramData payload 的同长前缀逐字节相等；
2. 该文件长度之后的 payload **每一个字节**都是零；
3. 编译文件、源码/lock、工具及命令 provenance 另行绑定。

section 一致或 diagnostic path 一致只用于隔离原因，不能升级资格。任何 header/metadata/指令/重定位差异均失败；不允许为了匹配而任意裁剪 artifact。`compare_artifact` 的 matched 只表示严格字节条件，没有授予源码 provenance 或历史 Bank 资格。

## Upgrade、buffer 与历史 Bank 边界

三项实际 ProgramData slot：CPMM463522946、CLMM459932378、Orca457152296。`public/*-upgrade-decoded.json` 保存原升级交易、buffer 地址、authority 与 logs；Orca upgrade 位于 Squads 的 inner instruction。第一次仅支持 v0 的 getBlock 请求遭遇区块内 v1 交易错误，原始失败响应保留；后续 v1 请求成功。

`public/*-buffer-summary.json` 与原交易文件保存有界的 buffer write 抽查。CPMM 最后一个 88 bytes write 到 offset749760，CLMM 最后一个 816 bytes write 到 offset1497600；已取得的对应切片全部等于当前完整部署 payload。Orca 的 429 原始失败和成功重试分别保留。抽查没有重建整个 buffer，也没有从 Write 指令中取得 compiler/source commit。不能把这条字节 provenance 当成源码证明。

原部署目录 slot466345524/531/538 与表中本地执行输入快照 slot 不同。实际公开 RPC 反例 `historical-minContextSlot-probe.json`：`minContextSlot=466345524` 返回 context.slot466417577。`minimumLedgerSlot` 观测为466298446，`getFirstAvailableBlock` 为0；区块可查不等于任意历史账户/bank 可选。

**历史原 Bank 的验收前置条件**：取得包含目标 slot 之前状态的真实 validator snapshot，配齐目标区间 ledger/shreds 与 cluster feature/epoch 环境，校验 snapshot/ancestor/bank hashes，恢复到原目标 Bank，再在隔离副本中执行原交易或受控分支并保留证明。公开 RPC 账户观测、升级/写入交易和原 SBF 的受控原生重放，均不能替代上述历史 Bank 证明。所有来源采集接口和构建依赖采用公共资料及开源软件，无私有供应商凭据前置条件。

## 证据清单格式与验收

`catalog-overlay.json` 按部署 id 关联独立 native execution evidence、source candidate comparisons 与各自 authority。`tool-inventory.json` 使用 catalog manifest 的 tools 记录格式，列出 acceptance 工具与测试的 SHA-256。`inventory.json` 定义本目录其余数据的逐字节清单；清单文件自身的哈希由其上层 catalog manifest 承诺。既有失败证据属于可核验输入，不得以状态字段覆盖。

```sh
python3 -m unittest discover -s integration/tools/acceptance/tests -p 'test_r01_public_catalog*.py'
python3 -OO -m unittest discover -s integration/tools/acceptance/tests -p 'test_r01_public_catalog*.py'
```

普通与优化模式都使用异常检查，不依赖会被 `-OO` 删除的裸 assert。新增反例涵盖 PDA/ProgramData 换址、upgrade/context 错位、owner/executable、payload 改字节、header 跳过、非零裁尾、伪造历史 Bank 标志、RPC body 与解析值不一致、native runtime pins、Token recipient 绑定、篡改保存净收款等。

## 固定 RPC 与正常 Parser→Router 差分契约

`r01_public_catalog_solana_process.py` 将同一 sourcebank 作为只读 loopback JSON-RPC：仅回答保存的 genesis、固定 slot 的原始 getBlock、已捕获地址的 getMultipleAccounts。重复读取返回相同账户 bytes、Clock.slot、Clock.unix_timestamp 和 blockhash；未知地址、其他 slot、写交易及其他 RPC 方法明确报错。捕获为 null 的 Oracle 与未捕获地址严格区分。账户补集必须以另一个 sourcebank 版本采集，不能并入旧固定 bank 冒充同高状态。

配置使用正常 `chain-parser-rust --config`（service.mode=rpc）及正常 `dex-router --config`，保留 project binary 的执行路径并保存其字节副本、运行前后 SHA-256。没有 fixture-parser、手填 Pool、内部 Math 替代进程或修改的 Go/Rust 生产实现。链 key 为 solana:mainnet-beta，source authority 仍是明确的离线固定样本。HTTP-development journal 是已有开发传输模式；此路径不构成 PG/Kafka 验收。

离线 freshness 使用已有测试配置允许的 315360000000 ms 上限，quote TTL=60000 ms；它只放宽测试配置，不刷新 Clock 或 blockTime。报价限定一个真实池、一个 hop、无 split，输入为1000000 WSOL 最小单位，caller/recipient 取原 SBF 交易中的不同账户。

```sh
python3 integration/tools/acceptance/r01_public_catalog_solana_process.py prepare \
  --output integration/artifacts/local/r01-solana-process-inputs
python3 integration/tools/acceptance/r01_public_catalog_solana_process.py run \
  --output integration/artifacts/local/r01-solana-process-normal-run \
  --timeout 45 \
  --parser-bin chain-parser-rust/target/debug/chain-parser-rust \
  --router-bin dex-router/target/debug/dex-router
```

每个输出目录保留 normal config、Parser/Router 日志、RPC 请求与响应 hash、typed baseline（若有）、真实 `/v1/quote` HTTP 响应和比较结果。只有 HTTP200、固定 bank/池 scope 绑定、consumedIn=1000000、refund=0 和逐最小单位相等才允许授予相应差分资格；运行中 binary 改变会撤销资格。`netBuyAmountEstimate=null` 保持 unknown，禁止用 gross 或 minBuyAmount 代替 quote net。

账户样本中原 SPL Token 的公开身份为：ProgramData `3gvYRKWyXRR9xKWe1ZjPhLY5ZJRN7KDB4rFZFGoJfFk2`，upgrade slot419472000，原 payload108600 bytes，SHA-256 `8190d3f7ceb6cb7a7a8d8924bff89f9f611e15ce1f806f2b6237f3311a98f697`，owner为 Upgradeable Loader。正常 parser 若只接受 immutable loader，必须保留 `TOKEN_PROGRAM_VERSION_UNSUPPORTED`，不得将账户改写为旧 loader 或拿36字节 Program 头 hash 冒充 ELF hash。`process/` 的原始观测展示该拒绝路径及 Router `503 CHAIN_NOT_READY`，没有将缺失报价写成0或数学差分通过。
