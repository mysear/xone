# R01 公有部署／历史证据目录

<a id="fresh-evm-anchors"></a>

## EVM 固定块复验

### 从已入库的证据统一复验

在独立检出中运行以下命令。准备步骤安装固定版本的 Node 参考依赖、校验 Solidity 0.8.19 编译器并构建两个服务。复验入口校验六个归档的 SHA-256，重放八个固定块和 11 项部署。

```sh
mkdir -p integration/.runtime/stable-reference integration/.runtime/receipt-root-reference
cp integration/tools/reference/stable-models/package*.json integration/.runtime/stable-reference/
cp integration/tools/reference/receipt-root/package*.json integration/.runtime/receipt-root-reference/
npm ci --prefix integration/.runtime/stable-reference --ignore-scripts --no-audit --no-fund
npm ci --prefix integration/.runtime/receipt-root-reference --ignore-scripts --no-audit --no-fund
PYTHONPATH=integration/tools/acceptance python3 -c 'from r01_public_catalog_evm_history import prepare_helper_compiler; prepare_helper_compiler()'
mkdir -p replay-bin
(cd chain-parser-go && go build -trimpath -o ../replay-bin/chain-parser-go ./cmd/chain-parser)
cargo +1.90.0 build --locked --manifest-path dex-router/Cargo.toml \
    --target-dir dex-router/target --bin dex-router
python3 integration/tools/acceptance/r01_fixed_evm_replay.py \
    --parser-bin replay-bin/chain-parser-go --router-bin dex-router/target/debug/dex-router \
    --out integration/artifacts/local/r01-public-fixed-suite
```

每次运行使用一个尚未创建的 `r01-public-` 输出目录。`fixed-replay-report.json` 记录八项状态证明、Go 回执核验日志和 11 项常规服务报价的路径与摘要。回放要求 HTTP 200、逐项匹配原池实际收款，且没有未归档的 RPC 请求或回放期间的公网请求。此复验覆盖池级毛额。链费和生产执行资格另有独立验收。

### 采集新的固定块

在仓库根目录执行以下命令。每链写入独立目录，保留链 ID、创世块、primary 的 finalized 完整块与 witness 的同高度头。两源身份均与 `integration/config/chain-targets.json` 一致才会选用。重建区块头和交易根时，使用 `r01_public_catalog_evm_commitments.cjs` 导出的 `verifyBody(chain, header)`，其中 `header` 是 `anchor.json` 的 `header` 引用所指 RPC 回应的 `response.result`。Avalanche 的目标高度状态证明还需要其后继结算头。

```sh
mkdir -p integration/test/source/deployment-catalog/evm-continuation-v2
r01_run=$(mktemp -d integration/test/source/deployment-catalog/evm-continuation-v2/anchors.XXXXXXXX)
for chain in ethereum base bsc arbitrum optimism polygon avalanche; do
    python3 integration/tools/acceptance/r01_public_catalog_evm_history.py anchor \
        --chain "$chain" --directory "$r01_run/$chain" &&
    python3 integration/tools/acceptance/r01_public_catalog_evm_history.py catalog \
        --chain "$chain" --directory "$r01_run/$chain"
done
```

每链的 `deployments.json` 记录官方目录、工厂及池的 runtime 摘要、升级槽与固定块 RPC 回应。用同一目录启动原池执行时，运行 `r01_public_catalog_evm_history.py capture --chain "$chain" --directory "$r01_run/$chain" --catalog "$r01_run/$chain/deployments.json"`。采集会下载和校验固定的官方 Solidity 0.8.19 编译器，离线重放复用采集时使用的编译器。

固定块的原执行已完成但公共 RPC 暂未返回完整预状态时，可以对同一目录运行 `r01_public_catalog_evm_history.py recover-prestate --directory "$r01_run/$chain"`。命令按已归档的读取集重新获取缺失证明，保留原始请求和响应。独立验证器核对区块承诺和归档读取值后，发布 `prestate-evidence-v2.json`。覆盖不全时可在原高度重试。

若固定块的原始端点已裁剪历史状态，可用 `--proof-endpoint` 指定已登记的公共 RPC。采集先核对该端点的链 ID、创世块和目标高度的块哈希及状态根，再获取证明。原执行回放继续使用同一份归档。

### BSC 近期块证明

若公共 RPC 仅在 `latest` 保留证明，使用前一轮 BSC 原池执行目录推导账户与存储键，包括新 callback 地址对应的余额及授权键。工具保留批量原始响应，验证账户和存储证明与同一块的根一致。原池执行后再核对实际读取集，并从独立 RPC 保存后续 finalized 观测和同高度块头。

```sh
python3 integration/tools/acceptance/r01_public_catalog_evm_seed.py \
    --chain bsc --previous "$bsc_previous" --directory "$bsc_new" \
    --tag latest --batch-latest
python3 integration/tools/acceptance/r01_public_catalog_evm_history.py replay --directory "$bsc_new"
python3 integration/tools/acceptance/r01_public_catalog_evm_history.py receipts --directory "$bsc_new"
```

`$bsc_new` 使用尚未创建的目录。先用已有 BSC 部署目录运行 `capture` 可生成 `$bsc_previous`。下方固定块回执命令可复验新块的头、交易根及回执根。

Avalanche 的后继结算头可用 `r01_public_catalog_evm_history.py settlement --directory "$r01_run/avalanche"` 采集。预状态验证器读取该目录中的 `native-successors.json`，校验每个原始响应的摘要，并用精确后继结算状态根核验 Coreth 账户及存储证明。

六条链均可用 `r01_public_catalog_evm_history.py receipts --directory "$r01_run/$chain"` 采集固定块回执。Avalanche 先用 `settlement` 采集后继结算头，再由 `receipts` 保留该结算窗口内的交易和回执。将 `XONE_R01_RECEIPT_INPUTS` 设为各 `receipt-input.json` 的绝对路径列表，以冒号分隔。在 `chain-parser-go` 执行 `go test ./internal/chain/evm -run '^TestPublicCatalogFixedReceiptRoot$' -count=1 -v`，离线重建头哈希、交易根及回执根。

当前 bitmap 字较密集时，可在 getter 输入中指定 `"trace_format": "indexed-proof-ledger/v2"`。报告以索引引用原始证明节点和来源，逐次 getter 调用保留其读取区间，便于按原顺序还原。

### Base Slipstream 固定块报价复验

在仓库根目录运行以下命令。`slipstream_base` 指向带有 `prestate-evidence-v2.json` 的 Base 原执行归档，`slipstream_responses` 指向留存的固定块 RPC 响应目录，`slipstream_original_capture` 指向该池的 `capture.json`。重建工具校验每份响应的固定高度、账户和请求槽集合，然后独立验证合并后的状态证明及原始状态读取。新目录中的 `proof-inputs.json` 记录全部输入的 SHA-256。

```sh
slipstream_rebuilt=integration/test/source/deployment-catalog/evm-normal-v4/base-slipstream-rebuilt
python3 integration/tools/acceptance/r01_public_catalog_slipstream_archive.py \
    --capture "$slipstream_original_capture" --base "$slipstream_base" \
    --proof "$slipstream_responses"/context-proof-*.json "$slipstream_responses"/ticks-proof-*.json \
    --code "$slipstream_responses/0x0ad08370c76ff426f534bb2affd9b5555338ee68.code.json" \
    --out "$slipstream_rebuilt"
```

运行报价复验前准备 EVM 参考执行依赖，以及识别已部署费用模块的 Go Parser 和 Rust Router 构建产物。`--work` 与 `--out` 使用尚未创建的目录。

```sh
python3 integration/tools/acceptance/r01_public_catalog_slipstream_normal.py \
    --capture "$slipstream_rebuilt/capture.json" \
    --work integration/test/source/deployment-catalog/evm-normal-v4/base-slipstream-replay \
    --out integration/artifacts/local/r01-public-base-slipstream-replay \
    --parser-bin "$slipstream_parser_bin" --router-bin "$slipstream_router_bin"
```

工具先在固定块原合约上分片重放 getter，再让常规 Parser 和 Router 通过本地 RPC 消费重放结果。`proof-bound-report.json` 记录 getter 输出、进程报告与原执行的 SHA-256，并要求 Router 的池级收款与原池收款一致、回放无未记录的 RPC 请求。高度 63,180,533 的验收值是 3,264 次 getter 和 166,349 个收款原子单位。

### 2026-11-28 固定块复验包

在干净的仓库检出中，从根目录展开 `evidence/fixed-evm-2026-11-28.tar.gz`。压缩包保留 990 份原始输入及其仓库相对路径，SHA-256 为 `4f19190de2bd8da36e0c1b28cfc34544ded0f86f5170a485190b0638fbe61cf4`。

```sh
evidence_root=integration/test/source/deployment-catalog
tar -xkf "$evidence_root/evidence/fixed-evm-2026-11-28.tar.gz"
PYTHONPATH=integration/tools/acceptance python3 -c 'from r01_public_catalog_evm_history import prepare_helper_compiler; prepare_helper_compiler()'
```

### Ethereum Uniswap V2 与 V3 固定块原池复验

`evidence/ethereum-uniswap-26511767.tar.gz` 保留 125 份文件，SHA-256 为 `48f6a43e8837cbb63b4a11c12164308932b7436bdd98f6680c9fbe70fa0dcdd1`。dRPC 和 PublicNode 核对创世哈希及 finalized 高度 26,511,767 的哈希与状态根。独立验证重建该块的 226 笔交易和回执、11 个预状态账户、32 个存储槽与原池执行的 60 次公网预状态读取。两项原池重放分别收到 165,056 和 165,469 个原子单位。

安装本页上方的 Node 参考依赖并准备固定编译器后，在独立检出的根目录运行以下命令。

```sh
evidence_root=integration/test/source/deployment-catalog
tar -xkf "$evidence_root/evidence/ethereum-uniswap-26511767.tar.gz"
fixed_block="$evidence_root/evm-continuation-v2/ethereum.2nsy6ES2"
node integration/tools/acceptance/r01_public_catalog_evm_commitments.cjs "$fixed_block"
python3 integration/tools/acceptance/r01_public_catalog_evm_history.py replay --directory "$fixed_block"
receipt_input="$PWD/$fixed_block/receipt-input.json"
(cd chain-parser-go && XONE_R01_RECEIPT_INPUTS="$receipt_input" \
    go test -race -count=1 -v ./internal/chain/evm -run '^TestPublicCatalogFixedReceiptRoot$')
```

常规报价归档 `evidence/ethereum-uniswap-normal-26511767.tar.gz` 包含 21 份文件，SHA-256 为 `c948c52ca5d11b91532b1f2b5516797202b5fc97a89144385893d04b1fa0610d`。额外的 17 份公开 RPC 证明响应均按原块状态根校验。V2 的 4 次 getter 和 V3 的 257 次 getter 重放通过。常规 Go Parser 和 Rust Router 对两项报价均返回 HTTP 200，分别匹配原池的 165,056 和 165,469 原子单位。复验期间公网请求和未归档 RPC 请求均为零。

```sh
tar -xkf "$evidence_root/evidence/ethereum-uniswap-normal-26511767.tar.gz"
getter_root="$evidence_root/evm-normal-v4/ethereum-26511767.CorGKXrn"
for pool in ethereum-uniswap-v2 ethereum-uniswap-v3; do
    getter_case="$getter_root/$pool"
    python3 integration/tools/acceptance/r01_public_catalog_evm_normal.py \
        --case-directory "$getter_case" --probe "$getter_case/probe-01.json" \
        --out "integration/artifacts/local/r01-public-$pool-fixed-h"
done
```

### 2026-11-28 八链部署目录

`evidence/eight-chain-deployments-2026-11-28.tar.gz` 包含 216 份文件，SHA-256 为 `d7dfea4ce9388f7be14c7555501b72577486f31a525a17e45ea03eec1e419b23`。归档保留 8 链的 16 项部署记录、15 个 EVM 样例池、3 个 Solana ProgramData 和相应的公开源响应。BSC 两项池查询使用同高度备用源。部署观察、源码复建、状态证明和生产准入分别由验收报告标记。

```sh
evidence_root=integration/test/source/deployment-catalog
tar -xkf "$evidence_root/evidence/eight-chain-deployments-2026-11-28.tar.gz"
catalog_root="$evidence_root/recapture-2026-11-28.JK5g7GVQ"
python3 integration/tools/acceptance/r01_public_catalog_build.py --observations "$catalog_root"
python3 integration/tools/acceptance/r01_public_catalog.py \
    --catalog "$catalog_root/catalog.json" --scope "$catalog_root/scope.json" \
    --inventory "$catalog_root/manifest.json"
```

归档中的清单固定本轮验收工具的哈希。重建命令会按当前检出更新工具清单，并核对采集范围、部署地址、运行时代码和样例池。生产资格仍需各部署的精确源码、状态证明和运行时执行验收。

### 其余固定块复验目录

固定块验证和原池重放使用 `evm-continuation-v2` 下的目录。

| 链和高度 | 输入目录 |
|---|---|
| BSC 136,158,531 | `bsc-head-seed.gFmAjg` |
| BSC 136,159,243 | `bsc-v2-getter-seed.umiwliu4` |
| Polygon 98,087,772 | `polygon-getter-anchor.6x8jv5hh` |
| Avalanche 101,595,781 | `recent-avalanche.qGKJLz` |

将 `fixed_block` 设为表中的目录路径，运行 `node integration/tools/acceptance/r01_public_catalog_evm_commitments.cjs "$fixed_block"` 和 `python3 integration/tools/acceptance/r01_public_catalog_evm_history.py replay --directory "$fixed_block"`。Go 回执联验读取同目录的 `receipt-input.json`。

常规报价复验使用 `evm-normal-v4` 下的 `bsc-pancake-v2-finalized.pefduaif/input-00.json` 和 `polygon-getter-proof.3q923t3l/input.json`。对任一输入先运行 `node integration/tools/acceptance/r01_public_catalog_evm_readonly.cjs "$getter_input" "$getter_probe"`，再运行 `python3 integration/tools/acceptance/r01_public_catalog_evm_normal.py --case-directory "$getter_case" --probe "$getter_probe" --out "$getter_output"`。`getter_case` 是输入所在目录，`getter_probe` 是新的报告文件，`getter_output` 是 `integration/artifacts/local` 下的新 `r01-public-` 目录。

Base Slipstream 按上一节的命令重建。`slipstream_base` 使用 `evm-continuation-v2/anchor-iteration-ee66n2qk/base`，`slipstream_responses` 使用 `evm-normal-v4/base-slipstream-dependencies.hoknqrom`，`slipstream_original_capture` 使用该响应目录下的 `context-cached._mgho3yq/capture.json`。这些路径都相对于 `evidence_root`。

### Base Uniswap V3 与 Aerodrome V2 固定块报价

展开固定块复验包后，再展开 `evidence/base-v3-aerodrome-v2-63180533.tar.gz`。此包包含 24 份额外输入，SHA-256 为 `a4978ab541680182016564b6ed37dd6758acc49740169611ea7bad2512280493`。用 `evm-continuation-v2/anchor-iteration-ee66n2qk/base` 回放原池并运行 Go 回执联验。运行状态证明验证时，传入固定块目录、空的第二参数和 `prestate-evidence-v2.json`。

在 `evm-normal-v4/base-uniswap-v3-fixed-h.ustwe78e` 使用 `probe-01.json`，在 `evm-normal-v4/base-aerodrome-v2-fixed-h.80092axw` 使用 `probe-02.json`。将相应目录与报告传给 `r01_public_catalog_evm_normal.py` 的 `--case-directory` 和 `--probe`。两条报价均返回 HTTP 200，分别匹配原池实际收款 166,255 和 166,299 原子单位。Go 验证 22 笔固定块回执。

### OP Mainnet 与 Arbitrum One 固定块报价

从仓库根目录展开 `evidence/op-arbitrum-fixed-2026-11-28.tar.gz`。包内有 462 份文件，SHA-256 为 `d461ed5e2c3e698d247107f3814ff1a3cce562b7957cb3e428d093047ced31f3`。按上一节准备固定编译器。

在 `evm-continuation-v2/anchor-iteration-ee66n2qk` 下分别使用 `optimism` 与 `arbitrum` 作为固定块目录。两条链均运行原池离线重放，并将各自的 `receipt-input.json` 传给 Go 的 `TestPublicCatalogFixedReceiptRoot`。状态证明验证使用 `r01_public_catalog_evm_commitments.cjs`。对 Arbitrum 额外传入空的第二参数及第三参数 `prestate-evidence-v2.json`，以读取补充证明后的状态输入。

按上一节运行常规 Parser 与 Router 验收。以下路径相对于 `evidence_root`，原始报告由工具重新执行 getter 后核对。

| 协议与高度 | 常规报价目录（`evm-normal-v4` 下） | Getter 报告 | 实际收款及报价原子单位 |
|---|---|---|---:|
| OP Mainnet Uniswap V3，160,113,647 | `optimism-uniswap-v3-160113647-indexed` | 固定块目录下的 `op-normal-indexed-probe.json` | 166,230 |
| OP Mainnet Velodrome V2，160,113,647 | `optimism-velodrome-v2-160113647` | 固定块目录下的 `velo-normal-probe-02.json` | 165,448 |
| Arbitrum One Uniswap V3，528,150,486 | `arbitrum-uniswap-v3-fixed-h.el56r46q` | 常规报价目录下的 `probe-01.json` | 166,253 |

三条报价均返回 HTTP 200，RPC 缺项和回放外网请求均为零。Velodrome V2 的回放包含两条公共链路一致返回的 OP 高度 1 交易回执，供 Parser 启动时读取。Go 分别验证 75 笔 OP 回执和 3 笔 Arbitrum 回执。

### Avalanche Uniswap V3 固定块报价

从仓库根目录展开 `evidence/avalanche-uniswap-v3-101597996.tar.gz`，SHA-256 为 `9de2500bc750f8428d89e93cda26ee3bb09741d543cba63c65774328d950e85f`。按上一节准备编译器，使用 `evm-continuation-v2/avalanche-getter-anchor.43nmhr50` 核验状态承诺并重放原池，使用其中的 `receipt-input.json` 执行 Go 回执根联验。

常规报价使用 `evm-normal-v4/avalanche-getter-proof.7jbwf7xl/input.json`。按上一节执行 getter 和常规报价复验。传入预编译的 Parser 或 Router 时，请将二进制文件放在当前检出内，供报告记录路径及摘要。目标块高为 101,597,996，块 101,598,001 提交其 Coreth 状态根及 101,597,995–101,597,996 的回执根。136 次固定块 getter 通过，覆盖当前 bitmap 字内的 128 个初始化 tick。Router 返回 HTTP 200，池级输出与原池收款均为 659 个原子单位。

### Avalanche LFJ v2.1 固定块报价

从仓库根目录展开 `evidence/avalanche-lfj-v21-101598757.tar.gz`，SHA-256 为 `f654014552fc0a2bad187beff4fd9adc111d40423b274b3201cd427a3bf87c35`。按前述步骤准备编译器，使用 `evm-continuation-v2/avalanche-lfj-anchor.EbsiIkbo` 复验状态承诺、原池执行和 Go 回执根。目标块 101,598,757 的状态根与回执根由块 101,598,759 提交。

常规报价使用 `evm-normal-v4/avalanche-lfj-proof.no6DPSRo/input.json`，运行前述 getter 和常规报价命令，后者可传入 `--timeout-seconds 150`。原运行时共执行 4,848 次 getter，双向遍历全部 1,209 个非空 bin，验证储备、份额及代币余额。此样本的预言机索引为零。Router 返回 HTTP 200，池级输出与原池收款均为 134 个原子单位。移除 `proof-005.json` 的引用时，重放检测到缺失的 MPT 存储节点。

### BSC Pancake V3 LM 固定块报价

从仓库根目录展开 `evidence/bsc-pancake-v3-136166651.tar.gz`，SHA-256 为 `167e36ec2217ef40498ac0c9e6cc78aeaebcd4fa897a9aa3dfae9943075cc5fe`。在块目录 `evm-continuation-v2/bsc-lm-proof.CnFonFFC` 复验原池和回执。调用状态承诺 CLI 时传入块目录、空输出路径和 `prestate-evidence-v2.json`，以同时核对三代前序 LM 账户。

用 `evm-normal-v4/bsc-lm-proof.CnFonFFC/input.json` 运行 getter 和常规报价复验，后者可传入 `--timeout-seconds 150`。块 136,166,651 的 26 份补充证明覆盖 2,100 条预言机观测、当前 bitmap 字内的 253 个初始化 tick、MasterChef 以及第四代 LM 和三代前序 LM 的读取。4,168 次 getter 通过。Router 返回 HTTP 200，池级输出与原池收款均为 55,536,458,068,512,518 个原子单位。Go 核验同块全部 85 笔回执。

压缩包也保留七份源码响应及链上代码记录。准备固定 Solidity 0.7.6 和 0.8.10 编译器后，运行 `node integration/tools/ci/pancake-v3-bsc-artifacts.cjs` 和 `node integration/tools/ci/pancake-bsc-predecessor-artifacts.cjs`。六个部署在核对 immutable 绑定后完整 runtime 一致，第一代 LM 的执行体一致且 CBOR 来源摘要有差异。

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
