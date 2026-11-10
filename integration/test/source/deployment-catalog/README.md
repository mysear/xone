# R01 公有部署／历史证据目录

本目录是有限的真实观测集：8 链、16 个 chain/protocol、14 个经 factory 查询并读取代码的样例池，以及 3 个完整 Solana ProgramData。所有外部输入均来自无需账号、API key 的公共 HTTPS。目录中的 `version` 使用实际 runtime SHA-256；Solana 另包含解码得到的 upgrade slot。`profiles` 表示本项目代码识别的 profile，不能证明该 profile 对应当前部署源码。

`manifest.json` 固定全部数据文件和本任务工具／测试的逐字节清单；默认 CLI 也检查缺失、额外文件和改动。`catalog.json` 包含逐项原始证据引用和 SHA-256，`scope.json` 固定声明范围的 chain、protocol、runtime version、地址、H/hash。缺行、重复、范围外行、错误地址或 H/hash 不能通过范围校验。`catalog-ethereum.json` / `scope-ethereum.json` 是明确较窄的 Ethereum V2 单部署验收范围，不代表整个 R01 已完成。

## 实际采集范围

| 链 | 固定高度／slot | 部署项 | 实际附加证据 |
|---|---:|---|---|
| Ethereum | 26355865 | Uniswap V2 Pair、V3 Factory | V2 头／交易根／账户 MPT／代码证明、10 源文件精确重建、本地原 bytecode 执行；V3 真实代码及完整编译输入 |
| Base | 54137385 | Uniswap V3、Aerodrome V2、Slipstream | 真实 factory／pool 代码、stable／volatile pool、clone implementation 与 getter |
| BSC | 132121281 | Pancake V2／V3 | 真实 factory／pool 代码；主源旧状态缺失时改用公共备用源并核对同高 hash |
| Solana | 466345524、466345531、466345538 | Raydium CPMM／CLMM、Orca Whirlpools | 同一次 finalized 账户读取中的 Program→ProgramData、owner／executable、upgrade slot／authority、完整 ELF payload、独立 getBlock 观测 |
| Arbitrum One | 521960667 | Uniswap V3 | 真实 factory／pool 代码及完整编译输入 |
| OP Mainnet | 159209547 | Uniswap V3、Velodrome V2 | 真实 factory／pool 代码、stable／volatile pool、implementation getter |
| Polygon PoS | 96875546 | Uniswap V3 | 真实 factory／pool 代码；公共备用 RPC 的同高读取 |
| Avalanche C-Chain | 99636537、99636765 | Uniswap V3、LFJ Liquidity Book v2.1 | 真实 factory／pool 代码、LB pair implementation、完整公开源码编译输入 |

SBF payload 含 allocation padding，合计 12,979,699 字节；不把有 padding 的整个 payload 擅自认作原始编译文件。ProgramData 二进制头固定解码后才接受 payload：Program discriminator=2、ProgramData discriminator=3、offset=45、ELF 魔数、升级权限 option 和 slot 都需成立。`context.slot` 是账户读取的 bank 上下文；`getBlock(slot)` 是独立的区块观测，不构成历史 bank 状态证明，也不把 `minContextSlot` 当精确历史选择器。

## 已完成的绑定与边界

- Ethereum V2 Pair `0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc`，H26355865，hash `0xbcbf29e3d91c7c707827ae95fbaced24fe8df6b704b0db277d16f45f40e50ffd`。runtime SHA-256 为 `8b5db55fa9ab3b9527508d4abe0b39eb588bf310270c8e04b3f38214e8ba63b4`。官方 SDK factory 常量加实际 `getPair(USDC,WETH)` 返回值绑定池地址。原 Go provenance 验证器检查完整 Ethereum 头、交易根、账户 MPT、代码承诺；原 artifact 的完整 metadata、10 个源文件、solc `0.5.16+commit.9c3226ce`、settings 与实际 runtime 逐字节重建。
- 6 链 Uniswap V3 Factory、BSC Pancake V3 Factory、Avalanche LFJ v2.1 Factory 共 8 项，通过公开 Standard JSON、官方 compiler build SHA-256 和实际固定 H runtime 重建。编译器声明的 immutable slot 只允许零占位符按实际 runtime 中一致的值链接，并保留具体 slot/value；其他字节，包括所有 metadata，必须完全一致。加上上一项，原始输入已有 9 个部署成功重建；本轮精确输入恢复再增加 4 项，合计 13 个 EVM 部署。
- 通用 Sourcify 输入的 Ethereum V2、Aerodrome V2、Slipstream、Pancake V2、Velodrome V2 共 5 项未通过严格 byte 比对。Ethereum V2 有上一项的独立原包精确重建；其余 4 项现已另存完整匹配输入与重建结果于 `metadata-exact/exact-source-manifest.json` 和 `exact-source-report-complete.json`。原始失败细节仍保存在 `public-source-build-report.json`，不剥除或替换 CBOR metadata 来获得成功。公开验证站的 `match` 字段没有验收权威。
- 实际采集并核对 Aerodrome／Velodrome／Slipstream factory getter、LFJ pair implementation，以及样例池 EIP-1167 代理的实际目标代码。普通 factory 的 EIP-1967 槽为零只证明该槽读取结果，不能证明无自定义升级；未完成升级语义判断的条目继续报告缺项。
- Ethereum V2 本地执行使用原发布 bytecode，在不监听公网的进程内 Ganache bank 真实创建 factory、Pair、Router 和 token 并执行 10 例：6 成功、4 回退。核对完整保留 bank case、原始签名交易、receipt、实际账户代码、执行前后状态及收款余额差；包含转账税收款反例。它是 `evm:31337` 的原 runtime 本地回放，**不是** H26355865 原链历史状态／原链交易执行。未向公链发送交易。

严格 R01 范围的资格规则是：明确 H/hash 下检查过的状态／代码承诺 + 精确公开源码编译重建 + 原 runtime 的独立实际收款执行。用户允许本地原合约执行，因此明确限定的 Ethereum 单部署范围可通过；这不代表共识验证、原链历史仿真、全部版本、D1/Q2 全范围或生产就绪。其余条目必须维持 `NOT_QUALIFIED`，逐项给出状态证明、升级、source/compiler、执行或 domain profile 缺项。

Solana 3 项尚无与实际部署 SBF 匹配的 compiler/toolchain build 和原 SBF 本地收款执行，也无该历史 bank 的完整状态承诺证明；CPMM 两个实际候选编译结果及不匹配证据见下文。现有 Raydium／Orca reference manifest 已按原文件哈希引用；它们的源码提交/native profile 不能替代当前 ProgramData 身份。LFJ 与 Velodrome 请求的 profile 未在当前 Go／Rust domain 同时存在，目录没有自行补 Go／Rust 支持。

## 本轮新增：不可变历史锚点与精确 metadata

`history-catalog.json` / `history-scope.json` 保留 8 条逐地址／version／H／hash 的历史记录。同一 runtime 可以有多个锚点；新 H 不覆盖旧 H26355865。`history-catalog-qualified.json` / `history-scope-qualified.json` 明确只声明 3 条 fresh 固定状态参考：V3 Factory 的源码验证关联池参考、A478 V2 Pair 和 C2E9 V3 Pool 的直接执行参考。没有在旧 H、B4 Pair 或 USDC Pool 上成功执行的记录仍有精确缺项，不能共享另一个池的执行资格。

fresh finalized H=26360354，hash=`0x78624305628d1e517ceedaca636ece8846570a3212f27dc18d0a74f7d4b71bf5`，state root=`0xe63155935d082ad1c9100d7fe49d3c97c6a680cecdea32de065794fe27041073`。`history/ethereum-fresh-04` 保留 248 笔原交易、12 个账户 proof、19 个 storage slot 和 47 个 lazy 公开 prestate 读取。Go provenance CLI 验证头／交易根／非空代码账户；EthereumJS Trie 独立验证全部账户（含 EOA／不存在／零 storage）及每次 lazy 读取。重新执行时 archive 无网络 fallback，未记录读取直接失败。

每例从同一原始状态 snapshot 开始，使用原 funded public sender 进行本地 impersonation，以其已证明 ETH 调用原 WETH deposit、approve 与原 router/pool swap。没有 balance／storage／code override，没有用户私钥，没有向公链发交易。输入均为 `100000000000000` wei WETH；原 V2 DAI/WETH Pair `0xa478c2975ab1ea89e8196811f51a7b7ade33eb11` 实际净收款 `223470537681971279` wei DAI；原 V3 DAI/WETH 3000 Pool `0xc2e9f25be6257c210d7adf0d4cd6e3e881ba25f8` 为 `223468916253220467` wei DAI。receipt、交易、完整余额差、环境及 opcode trace 摘要均保存并离线比对。`ethereum-fresh-03` 的真实 USDC `Blacklistable: account is blacklisted` 失败也保留，没有修改 blacklist 或存储以制造成功。

执行边界必须随结果一起使用：Ganache 7.9.2 使用 Shanghai VM、chain ID 1、本地 coinbase `...cafe`、30M gas limit 和明确的时间递增。初始化产生本地 H+1 sparse overlay；其重构 header 不含全部新 Ethereum 字段，local hash／state root 不等于原链完整世界状态承诺。`environment-differences.json` 逐字段保存差异；当前 trace 环境 opcode 只见 TIMESTAMP 4 次，未见 BLOCKHASH。验证器拒绝未绑定 BLOCKHASH 语义、未声明状态覆盖或当前硬分叉等价声明。因此结果是固定原链公开 prestate 上的明确本地环境参考，**不是原链已挖出的交易，也未声明当前硬分叉完全等价**。原本地新部署 10 case 继续独立保留。

四项 exact source 的修复均经过实际 solc 重编译并比较完整部署字节：

| 部署 | 精确输入差异 | 结果 |
|---|---|---|
| Aerodrome V2 | 恢复原 13 项 remapping，以 runtime CBOR IPFS digest 约束 metadata 候选，随后 solc 0.8.19 全字节重建 | 含 metadata 一致；getter／immutable 绑定实际 clone implementation |
| Slipstream | 删除与该编译目标无关的全局 libraries settings，保留完整原 source／remapping／solc 0.7.6 | 含 metadata 一致；implementation 同 H 绑定 |
| Velodrome V2 | 同样恢复无多余 libraries 的原设置 | 含 metadata 一致；implementation 同 H 绑定 |
| Pancake V2 | 原多文件 `contracts/` 布局、公开原 V2 body、CRLF、solc 0.5.16、runs 200 | Factory 全 19,084 bytes 一致，包含嵌入 Pair metadata 和 Factory metadata |

`r01_public_catalog_exact_sources.cjs` 每次重新编译，检查官方 solc pin、完整 source/settings、compiler-declared immutable、metadata 原文、getter selector／目标／同 H 实际 implementation code；旧 Sourcify 失败报告无须篡改。实现目标源码／池实例的其他历史升级和执行仍分别验收。

R02 Tesla 的复用入口为 `v3-pool-template/r02-tesla-sidecar.json`。包含完整 Standard JSON、官方 Pool deployedBytecode、immutableReferences、AST 声明映射与两个 proof 绑定实际池；compiler cache 为 `integration/artifacts/local/r01-public-compilers/soljson-v0.7.6+commit.7338295f.js`。7 个 immutable 为 original、factory、token0、token1、fee、tickSpacing、maxLiquidityPerTick。两个池实际 runtime SHA 不同，而 compiler 模板相同；缓存须同时保留实例绑定和实际 runtime 身份。`metadata.bytecodeHash=none` 是官方原始编译设置，仍比较完整原 CBOR，未裁 metadata。

## Solana 实际 SBF 构建结果

`solana-verifiable/index.json` 保存三个程序的公开 OSEC status 原文；三个返回均为 `is_verified=false`。status 中的 repo／commit 是公开候选，不能证明当前 ProgramData。未要求任何私有构建服务、账户或 API key。

已真实完成两次 CPMM 候选 SBF 编译，源文件与锁文件均不修改，依赖由 Cargo.lock checksum 绑定的公开 crate archive 恢复：

| 固定 commit | 工具链 | 真实 strip 后文件 | 与当前 payload 比较 |
|---|---|---:|---|
| `cfdb70a8ca9ea62bb5c304d4492ac0fc371ae8ce` | 官方 Solana 1.17.0 / platform-tools v1.37 / rustc 1.68.0-dev；580 crate | 535,688 bytes；SHA `ba34c2cd783cc0d0f90b5a7a3a8afc0df580066427e6c479c25cd8a93eeb756d` | byte 18 首次不同；旧 EM_BPF 247 对当前 EM_SBPF 263 |
| `f4b9b26342afe9122d5ff67cd7a4f19ded4ac0a5` | 官方 Agave 3.1.10 / platform-tools v1.52 / rustc 1.89.0-dev；689 crate | 722,640 bytes；SHA `b977a8fbcd122bf33eb7c5b6bb95b9e1e4b1444942b057ad31b56a01db1fd7da` | byte 24 首次不同；仍不匹配 |

CLMM／Orca 使用同一官方 Agave 3.1.10 / platform-tools v1.52 / rustc 1.89.0-dev 完成实际构建，未修改上游源码：

| 程序／固定 commit | 原 source pin／依赖 | SBF 文件／SHA-256 | 完整比较 |
|---|---|---|---|
| CLMM `6d7a0f82d2f293c553c69aaf943377367c6c4cc6` | 105 源文件；733 registry crate；Git uint `43a0303b9c2a23a00508816bd6d5a2e9381f143c` 原公开归档 | 1,498,424 bytes；`aae024e896ffe63ce9fc6bc662e2a3432b69d587f062a5bb5bec573786f56303` | byte 40 首次不同 |
| Orca `cc683ec92acb5e7606caa3d8e514d9383e9c7c11` | 原归档逐文件 pin；319 registry crate | 1,583,304 bytes；`365cfbd092cee1a87a30a67e47fbd0ae83d5bbd5026fdc80439c7c5cf25d8d78` | byte 24 首次不同 |

对应 `raydium-clmm-actual-build-attempt.json` / `orca-whirlpools-actual-build-attempt.json` 和 `*-actual-sbf-comparison.json`，可用 `r01_public_catalog_sbf_build.py raydium-clmm` / `orca-whirlpools` 从任务内恢复目录重放。CLMM 首次缺 Git directory-source checksum 的失败、首次 artifact 名称识别错误均保留，之后 official pipeline 完成并正确识别输出。Orca exit 0 但 LLVM 日志含 **39 条栈帧覆盖诊断**；报告记录 `executable_runtime_validity_claimed=false`，没有执行／安全合格声明。SDK 发布包的 syscall 清单为空产生 unknown-syscall 后处理警告，也原样保留。构建成功只证明产生候选 ELF，不能升级为已部署重建或执行合格。

当前 CPMM 完整 payload 是 793,824 bytes，SHA `c383e1c5b1f578f3d1bf42f12360fca67225b9adda0f3b6c293ad0d6b966818d`。ELF 声明范围为 749,848 bytes，但它只是 file extent 下界；即使后面为零，仍不能凭此宣称 executable 正确长度。只有实际 compiler artifact 的全部字节精确匹配 payload 前缀，并验证其后全部为零，`compare_programdata_build` 才接受 allocation padding。任意 `rstrip(0)` 或随意 strip 未知尾巴均不接受；这里的 strip 仅为官方 SDK 明确的 `llvm-objcopy --strip-all` 构建后处理。

`cpmm-modern-build-replay.json` 是实际官方 `cargo-build-sbf` pipeline 在本任务工具链目录内重放的 exit 0 记录。编译后的 76 个原 source pin 全部重验，见 `modern-source-postbuild-verification.json`。`cpmm-actual-sbf-comparison.json` / `cpmm-modern-actual-sbf-comparison.json` 保留两次完整比较，并接入主目录逐条重验；失败构建不会令 source qualification 变为 true。CLMM／Orca 也已实际编译，结果见下表；三个程序都尚无原 SBF 的历史 bank 收款执行与相应状态承诺。

官方构建器存在已处理的行为：`--skip-tools-install` 没有阻止后处理 `strip.sh` 重新调用 `install.sh`。首次 pipeline 成功编译后，在用户缓存临时写入 Criterion 和一个平台下载片段；本次新建缓存已识别并移回任务目录、停止下载，未安装全局工具链。报告 `modern-sdk-postprocess-remediation.json` 完整记录这次异常；该次 pipeline 不声称零网络。随后使用任务内 SDK dependencies／版本 marker 和已下载官方 llvm 完成后处理，官方 pipeline 再重放 exit 0；数据、工具链、Cargo／Rustup cache 和输出均在 `integration/artifacts/local/r01-public-sbf-runtime`。大工具链不进 git、不修改服务预算。

## 离线验收

从仓库根执行。Python 3、Node、原 provenance CLI 和本项目既有公共 reference 依赖是本地工具前提；所有网络采集与运行验收分开。Node 库为原 reference 已使用的 `ethers@5.8.0`、`ganache@7.9.2`、`solc@0.8.19` wrapper，不要求外部服务。编译器 cache 丢失时，`sources.py restore` 仅从固定官方 solc URL 下载并核对独立保留的官方 build list／SHA-256，不改变服务配置。

```sh
python3 -m unittest discover -s integration/tools/acceptance/tests -p 'test_r01_public_catalog*.py'
python3 -OO -m unittest discover -s integration/tools/acceptance/tests -p 'test_r01_public_catalog*.py'

# 仅目录／原始绑定校验，exit 0 不声明资格。
python3 integration/tools/acceptance/r01_public_catalog.py

# 明确较窄范围：重验头／MPT、重编译、重新执行原 bytecode，必须合格。
python3 integration/tools/acceptance/r01_public_catalog.py \
  --catalog integration/test/source/deployment-catalog/catalog-ethereum.json \
  --scope integration/test/source/deployment-catalog/scope-ethereum.json \
  --replay --require-qualified

# 全范围原始数据与 source/compiler 重放；输出逐条资格。
python3 integration/tools/acceptance/r01_public_catalog.py --replay

# 全范围严格合格门槛：当前必须 exit 1，不能给 16 项整体通过。
python3 integration/tools/acceptance/r01_public_catalog.py --replay --require-qualified

# 三条明确 fresh 固定原状态参考，离线原 runtime／compiler／proof 重验。
python3 integration/tools/acceptance/r01_public_catalog_history.py verify-catalog \
  --directory integration/test/source/deployment-catalog/history/ethereum-fresh-04 \
  --catalog integration/test/source/deployment-catalog/history-catalog-qualified.json \
  --scope integration/test/source/deployment-catalog/history-scope-qualified.json \
  --require-qualified --out integration/artifacts/local/r01-public-history-checked.json

# 全部 8 条历史锚点重验；未执行地址／旧状态项保持 NOT_QUALIFIED。
python3 integration/tools/acceptance/r01_public_catalog_history.py verify-catalog \
  --directory integration/test/source/deployment-catalog/history/ethereum-fresh-04

# 单独恢复公开编译器／重跑通用公开源码重建。
python3 integration/tools/acceptance/r01_public_catalog_sources.py restore
python3 integration/tools/acceptance/r01_public_catalog_sources.py replay \
  --out integration/artifacts/local/r01-public-source-replay-manual.json
```

不带 `--replay` 的 retained report 没有认证资格。重放时对保留 scope 的所有实际代码重新编译；保存的 `passed:true` 或源码报告不能独自令目录成为 `QUALIFIED`。保留 case 的历史 Node 安装路径只作为历史 metadata，重放完整确定性 bank case 和 setup，不要求相同安装绝对路径。

## 有界公有采集与复现

工具只允许列明的只读 JSON-RPC method，拒绝发交易方法、凭据 URL、账号／key 路径和凭据 query。RPC endpoint 另须属于代码内明确的无账号公有 endpoint 清单，不能用未审阅的 provider path 藏 key。RPC timeout=18 秒、响应最多 16 MiB、EVM 普通观测／样例池最多 2 MiB，并限制线程数、每链／每项调用数。Solana 读取已有 16 MiB 上限内的完整 ProgramData。文档默认 2 MiB；已公开的 V3 编译输入约 2.2 MiB，原 2 MiB 拒绝记录和后续有界 4 MiB 原文都保留。官方 solc 分发文件另有独立 32 MiB 文件上限，未修改任何服务预算。

```sh
# 输出目录必须是新的、且位于本任务允许的写集内；不会覆盖原采集。
python3 integration/tools/acceptance/r01_public_catalog_capture.py docs \
  --out integration/artifacts/local/r01-public-recapture-documents
python3 integration/tools/acceptance/r01_public_catalog_capture.py capture \
  --seed integration/test/source/deployment-catalog/seed.json \
  --out integration/artifacts/local/r01-public-recapture-observations
python3 integration/tools/acceptance/r01_public_catalog_samples.py \
  --input integration/test/source/deployment-catalog/captured-deployments.json \
  --out integration/artifacts/local/r01-public-recapture-samples
```

`seed.json` 中 Ethereum 显式固定旧 H；其他链重新采集会取得新的 finalized 观测，因此不会重现旧的 hash。Ethereum V3 同一旧 H 的新 `eth_getProof` 请求实际遭遇 publicnode 403、drpc 400、1rpc `distance to target block exceeds maximum proof window`；见 `ethereum-v3-proof-probes`。旧 H 的这项证明未获得；本轮已新增 fresh H26360354 的完整证明和固定状态执行，旧失败与旧 fixture 均原样保留，未以源码重建代替账户承诺。旧证据的可重复性由保存的原始响应、源 hash 和离线重放保证，不能承诺任意免费 RPC 永久保留旧状态。公共 RPC 的失败、限额拒绝、403、`missing trie node` 都作为失败观测保留，不伪造成功。

目前样例池备用源写在 `sample-retry-endpoints.json`；`samples.py --endpoints` 会先核对该源同高头。`build.py` 仅从保留采集合成目录／scope，不访问网络。Sourcify API 原文、全部编译 source content、完整 settings 和官方 compiler 列表 pin 见 `verified-source-*` / `public-source-build-inputs.json`；compiler 大文件仅在 `integration/artifacts/local/r01-public-compilers`，可按 pin 公开恢复。

## 给主线程的挂钩建议

本任务没有改动 `DEVELOPMENT_PROGRESS.md`、Go／Rust 应用源码、`integration/Makefile` 或 CI progress bindings，也没有提交／reset。

1. R01.1 / `7.1.r01-deployment-evidence` 可挂本目录完整性、默认结构与原始数据校验；报告必须展示 16 项中实际 `NOT_QUALIFIED` 的项目。
2. fresh 固定 H 参考可挂 `r01_public_catalog_history.py verify-catalog --catalog .../history-catalog-qualified.json --scope .../history-scope-qualified.json --directory .../history/ethereum-fresh-04 --require-qualified`；严格限定三条记录并展示本地环境差异。
3. R01.2 / `12.14.r01-deployed-source-rebuild` 可挂明确的 `scope-ethereum.json --replay --require-qualified`，其成功只覆盖这一条记录；全范围 `--replay` 可展示 13 个成功 EVM 部署；fresh 历史参考另挂下述严格 scope。
4. 整体 R01 的完成门槛使用完整 `scope.json --replay --require-qualified`，当前应失败。不能把较窄 scope、reference profile、配置 hash 或通用 compiler 的局部成功推广到全 R01。
5. 后续逐链实现相应 header／状态承诺校验、闭合升级语义，补充精确源码输入、原 runtime/program 执行与收款证据；Solana 需补历史 bank 承诺、实际 SBF 可重复 build／执行。每个新增版本都应新增精确 scope 和反例，不能改已有失败条目为手工成功。
