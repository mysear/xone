# XONE 兑换量与执行准备联调

静态前端直接调用 `dex-router`，页面不维护兑换公式。链、Token、可用方向、协议、Pool、View 和 graph version 来自当前可报价图；金额使用最小单位整数，支持 ExactIn 和 ExactOut。输入变化会撤销旧结果并防抖请求，解析版本变化、失联和报价到期会使旧结果失效。

## 启动

先启动解析器和 Router，使 Router 获得可用 ReplicaView：

```bash
cd frontend
npm run dev
```

浏览器访问 `http://127.0.0.1:5173`。Router 默认地址为 `http://127.0.0.1:8080`，也可以通过 `?api=` 指定。填写实际 caller/recipient；不再自动填入测试地址。原始整数金额不经过 JavaScript Number 转换；Token decimals/symbol 尚未提供，页面明确展示最小单位。

API Token 按 Router 地址保存在当前浏览器会话的 `sessionStorage` 中，关掉会话即失效；切换 Router 不会自动携带另一地址的凭据。前端不从源码中读取部署凭据。

## CORS

Router 的 `api.cors_allowed_origins` 可配置允许的前端 origin，例如：

```yaml
api:
  cors_allowed_origins:
    - http://127.0.0.1:5173
    - https://quotes.example.com
```

配置为空时只允许本地 `http://127.0.0.1:5173` 和 `http://localhost:5173`，不再向任意网站反射 origin。认证规则继续生效，不启用跨源 cookies。该字段是已有 api 配置的一部分，片段不能替代服务完整配置文件。

## 执行准备

当前 `/v1/quotes/{quoteId}/prepare` 支持已验证的 Meteora 单池 ExactIn/ExactOut 模板。Router 对自身配置的链 RPC 发出只读请求，检查：

- genesis、finalized 账户批次、minContextSlot、Clock、program/code 版本；
- Pool、Oracle、BinArray、bitmap/Token2022 依赖与报价快照一致；
- 用户源/目标 Token 账户的 owner/mint/冻结状态及 WSOL backing；
- 输入余额、fee payer 和 `getFeeForMessage` 的费用余额。

输入账户不填时按 caller/recipient、mint 和 Token program 派生 ATA；也可填写已存在的账户地址。缺失源账户仍返回 blockers；缺失标准 SPL 收款 ATA 可勾选“加入创建指令”，同一笔待签名交易会先创建再 swap。该选项只允许 recipient/mint/Token program 派生的规范 ATA，核对固定 Associated Token Program 代码并计入租金；Token2022 收款账户创建和 SOL 包装仍未开放。账户满足条件后生成 legacy message 和带零签名槽的 transaction，前端可以下载完整 JSON，其中包含 planHash、snapshot、requiredSigners、blockhash 和有效块高。这个文件尚未签名、尚未广播，仍须生产独立验证和有效性复核。

勾选“只读模拟并核对实际到账”后，Router 调用自身配置 RPC 的 `simulateTransaction`，使用完整 unsigned transaction 和原 blockhash，不替换 blockhash；模拟后源/目标 Token 账户的实际收付必须与同一报价一致。成功标记为 `RPC_SIMULATED`，不等于生产 VERIFIED；失败或金额不一致不返回可下载的 unsigned transaction。创建租金与网络费分开展示，余额必须覆盖两项之和。

公开 `/build` 与 VERIFIED 的准入不因新增 `/prepare` 改变。准备结果只证明指定检查时刻的账户/消息满足本地模板条件，不证明交易会在将来的链状态中成交。

## 测试

```bash
npm run check
npm test
```

真实浏览器联调工具：`frontend/test/browser.mjs`。它使用显式测试 harness 启动生产 Api handler 和数学内核，读取受控原 SBF 账户银行，浏览器请求不使用预制报价响应。运行前先构建：

```bash
cargo +1.90.0 build --locked --manifest-path ../dex-router/Cargo.toml -p xone-quote-api --example frontend_harness
```

需要本机 Playwright Node 模块和 Chrome；可用 `PLAYWRIGHT_MODULE` 指向模块目录，再运行 `node test/browser.mjs`。统一复验入口见 `make -C ../integration quote-execution-current-check`。测试包括报价、实际账户检查、待签名下载、坏账户拒绝、异步竞争、状态撤销/恢复、自动报价和移动布局。所有签名/执行对照仅在本地 LiteSVM 使用确定性测试密钥完成，不发送公网交易。

账户创建与模拟复验：`make -C ../integration account-preparation-current-check`。测试用的模拟 RPC 实际启动固定原 SBF 的 LiteSVM 模拟，不用预制成功报价；另对实际浏览器下载字节本地签名执行，核对租金、收付、后状态以及 swap 失败时创建账户一并回滚。

独立运行Rust执行准备测试时，需安装`solders==0.29.0`，或用`XONE_SOLANA_PYTHON_PATH`指向已安装该版本的目录。服务测试使用自身`test/tools`中的模拟worker和自身fixture，不引用相邻服务的测试源码。统一验收工具会先验证已固定运行时摘要，再传入对应本地依赖目录；依赖、原始银行和运行报告均不提交仓库。

## 输入 WSOL 的原生 SOL 补资

Meteora 单池报价的输入 Token 必须仍为 WSOL。勾选“输入为 WSOL 时用 SOL 补足余额”后，`/prepare`发送`wrapSourceSol=true`：

1. 验证输入确实为经典 Token program 的 WSOL，现有账户需归caller所有且余额/backing一致。
2. 输入账户缺失时，只允许caller的规范WSOL ATA；加入CreateIdempotent。
3. 按`requiredInput - existingWsolBalance`的正差额加入System transfer和SyncNative，再执行swap。
4. payer必须能覆盖全部SOL包装金额、账户租金和交易费，任一不足都会阻止下载。

ExactOut按执行计划最大输入上限（结合报价与滑点、且不超过请求预算）准备资金，因此原程序实际扣款较少时会留下WSOL。响应通过`wrapLamports`、`sourceAccountCreation`、`expectedResidualWsol`、`sourceWsolRetained`和`totalFundingLamports`单独展示；不会自动关闭已有账户或把WSOL余额当作已退回SOL。关闭/unwrap与真正的钱包提交仍需后续模板。

补资和swap属于同一笔待签名交易。只读模拟以“原WSOL余额+注入SOL”对比最终WSOL余额核对实际扣款，不会把包装动作误计成兑换收益。参考测试核对真实SOL扣款、WSOL backing、实际Token到账和失败时完整回滚；无公网交易。

复验入口：`make -C ../integration native-funding-current-check`。独立测试数据恢复工具为`integration/tools/ci/meteora-wsol-bank.py`，需要已有固定原程序银行/运行时依赖；测试数据和下载包仍保留本地。

统一验收将每次浏览器下载文件与三类原程序执行记录保存在独立运行目录，防止复验覆盖历史凭证；单独运行参考脚本时可用第二个命令行参数指定输出JSON路径。


临时WSOL结算（阶段60）：输入WSOL可显式选择“隔离临时账户并退回剩余SOL”，同时勾选包装且不指定源账户；输出WSOL可选择“解包为SOL”，要求caller与recipient相同、不指定目标账户且不创建收款ATA。两种操作只关闭本次新建的seed账户，界面分别显示关闭转回总额、其中退租以及预计payer SOL净变化，已有WSOL账户不关闭。报价仍以WSOL为Token，执行准备明确标注原生SOL结算。

复验入口：`make -C ../integration temporary-wsol-current-check`。真实Chrome下载的两种unsigned交易另在受控原程序银行执行，检查实际余额、已有账户不变及失败回滚。公网签名广播、Token2022创建和其他协议交易模板继续按进度文档推进。


Token2022收款ATA（阶段61）：对已经接入的Meteora转账费版本，可勾选“缺失收款ATA时加入创建指令”，程序按实际Token程序派生地址和扩展长度计租金。页面展示Token2022转账费及182字节账户长度，模拟到账为扣费后可用数量。该选项可与隔离输入SOL退款组合；总预付租金与临时退租分别展示。其他未知Token2022扩展仍拒绝。

复验入口：`make -C ../integration token2022-preparation-current-check`；数据恢复工具为`integration/tools/ci/meteora-token2022-execution-bank.py`，需已有固定原程序银行/运行时。


协议视图与执行层（阶段62）：链/池元数据新增版本化能力和protocolViewId，报价返回实际protocolViewIds；页面同时核对父视图与协议视图，并显示能力版本/执行阻塞原因。Raydium CPMM creator经典SPL新增有限独立ExactOut报价，当前仍未开放其交易准备。Meteora既有账户/WSOL/Token2022准备逻辑已迁到独立xone-execution-preparation组件，页面和下载格式保持兼容。

统一复验：`make -C ../integration pipeline-foundation-current-check`。六阶段整体进度见主开发进度文档，能力标签为false的协议执行项仍是未完成项。


Raydium CPMM执行准备（阶段63）：经典SPL已有输入/输出账户可检查、生成ExactIn/ExactOut unsigned交易并进行RPC模拟；缺失账户仍阻塞。该协议的自动建户和SOL包装/解包尚未实现，界面会禁用对应选项。前端也会在元数据刷新时核对协议能力视图，版本变化撤销旧报价/下载。

复验：`make -C ../integration raydium-preparation-current-check`；本地数据恢复工具为`integration/tools/ci/raydium-execution-bank.py`，依赖已有保留原账户银行/原native参考。


独立本地SBF模拟（阶段64）：配置Router的api.solana_runtime_manifest后，当前池存在匹配银行时可在执行准备完成后点击“独立本地模拟”。返回实际输入/输出、CU和证据摘要；这是原SBF执行受控本地bank，普通RPC模拟与生产资格仍分别标识。更换报价、账户或准备选项会清空旧证据；接口不签名、不广播。

生成测试manifest：`python3 integration/tools/ci/solana-sbf-runtime-manifests.py`（在项目根目录执行），需已有固定原银行/运行时；复验`make -C ../integration solana-independent-current-check`。


钱包生命周期（阶段65）：安装支持Wallet Standard只签名功能的钱包后，可连接当前Solana链账户。必须先报价、准备并完成独立本地模拟；“仅请求签名”由用户在钱包确认，服务再验证消息未变和Ed25519签名。还需单独勾选币种/数量/收款确认才能广播；发送默认关闭、按链显式配置。

配置项：api.transaction_submission_enabled、transaction_receipt_journal、transaction_submission_chains、transaction_max_pending，并要求solana_runtime_manifest。签名或视图/账户变化会清空发送资格。状态可能为处理中、已确认、最终确认、失败、过期、未知或回执不符；刷新页面恢复最后签名查询，不自动重签/重发。受控模拟银行不代表可在公网交易或生产资格。

复验：`make -C ../integration wallet-lifecycle-current-check`，测试钱包和loopback RPC只在显式联调harness启用；未保存用户私钥，必要代码之外的银行/运行时/日志仍不提交。


实时RPC快照独立模拟（阶段66）：可在固定SBF runtime manifest中设置`rpc_capture: true`并使用空`banks: []`，无需为每个钱包预装账户银行。Router在执行准备同次账户批读中保留完整快照，对外只返回摘要；独立模拟使用该快照并与当前报价/准备哈希绑定。界面标注“RPC账户快照，非历史银行证明”，不将RPC读取当作生产资格。

复验：`make -C ../integration rpc-capture-current-check`。测试manifest生成工具会产生`rpc-capture.json`；原固定银行模式仍保持兼容。


PumpSwap经典/boost（阶段67）：已有经典SPL用户账户且caller/recipient为声明user时，可准备ExactIn/ExactOut、下载单签名交易、原SBF独立模拟及钱包确认发送。卖出目标可能产生额外到账，例如目标1000/实际48185，最低到账与实际收款、5000网络费分别展示；回执保留真实额外到账。自动建户、SOL包装/解包和其他Token版本仍未开放。

复验：`make -C ../integration pump-amm-preparation-current-check`。数据生成`integration/tools/ci/pump-amm-execution-bank.py`复用已保留固定程序；完整闭包约15MiB而单批/快照/模拟输入限制16MiB，总账户缓存预算仍32MiB，生成银行及大程序文件不提交。


Pump原生SOL曲线（阶段68）：固定经典SPL版本、已有Token账户、声明user作为caller/recipient/payer，提供sell、buy、buy_exact_sol_in的ExactIn/ExactOut准备、单签名交易和独立模拟/钱包链路。原生资产使用native:solana:sol，输入或输出账户默认该user；Token侧仍默认ATA。交易金额和网络费分开结算，SOL余额净变动另列，例如实际收到10971且费用5000时净增5971。

能力模型/5新增nativeSolTrading以描述原生SOL交易；nativeFunding仍表示SOL/WSOL自动资金转换，此曲线不开放包装/解包或自动建户。复验make -C ../integration pump-curve-preparation-current-check；毕业迁移指令、其他Token/volume版本和其他链模板仍需独立完成。


Pump经典曲线收款ATA（阶段69）：买入时可显式勾选缺失ATA创建，将幂等创建和兑换放入同一单签名交易；目标地址已存在空System账户并预存SOL时，按实际已有余额只补足租金。Token卖出收原生SOL不开放ATA选项。准备结果分列交易金额、租金最小值、已有预存余额、本次补足额和5000网络费，回执另列用户实际SOL净变动；失败会原子回滚新账户。

能力模型/6只对明确经典曲线模板扩展收款ATA，自动SOL/WSOL转换和毕业迁移仍未接入。复验make -C ../integration pump-native-ata-current-check；九个银行由同一生成工具恢复，生成银行、程序二进制、依赖和临时回执不提交。


PumpSwap classic/boost账户与SOL资金（阶段70）：单池单路报价支持显式创建缺失收款ATA、从SOL补足WSOL输入差额、使用本次新建临时WSOL账户退回未使用的输入资金或将输出解包为SOL。只有兑换方向为WSOL且当前实际路由具备nativeFunding能力时开放包装/解包选项；临时账户只在本次创建并关闭，不关闭已有用户账户。空System规范ATA的预存SOL先用于租金，超过租金的部分在WSOL初始化后成为已有余额，不能冒充本次成交。界面分别显示预存余额、租金补差、包装额、退租、网络费和用户SOL净变动。

能力模型/7为两个已实现PumpSwap模板开放账户和资金选项；其他Token/volume版本、毕业迁移指令及其余链/DEX模板仍按各自能力标签披露。回执从真实pre/post balances和Token余额推导实际新增租金与初始WSOL，准备阶段的估算不替代实际成交归因。复验make -C ../integration pump-amm-funding-current-check；生成银行、独立运行时、二进制、临时签名/回执和截图均不提交。


Orca ExactOut（阶段71，能力模型/8）：已有静态、奖励、自适应、自适应奖励和混合动态数组五个经典SPL模型现支持精确输出目标。前端可选择双向币种，按当前QuoteView计算目标到账所需输入并显示路由；600个固定原始manager场景、连续状态和双池份额网格用于对照。当前这五个模型仍只开放报价，尚未提供unsigned/preparation、独立SBF或钱包执行能力，编辑账户字段也不会启用这些按钮。原始native manager参考与SBF/CPI交易验证分别记录。复验make -C ../integration orca-exact-out-current-check；需要恢复生成数据时先运行orca-exact-out-reference，生成物和下载依赖不提交。


回执显示交易币对与原报价ID，并按Router地址恢复：切换Router时清除页面上的旧回执与旧签名状态，保留原地址的本地历史；返回原Router后按已保存签名查询，继续显示实际确认状态。回执请求带查询版本，跨Router往返后的旧响应或错误不会覆盖当前结果。用户确认发送后先保存已签名交易的签名，因此发送响应晚于Router切换时仍可回原地址恢复，恢复只查询而不重发。


Raydium CLMM ExactOut（阶段72，能力模型/9）：固定6d7a0f82的静态、奖励、协议Token选费、Bitmap、Custom Seed和动态费六类经典SPL模型支持双向目标输出报价。输入/输出币种收费按原始分支分别舍入，显示目标到账与所需原始输入；947个成功/319个拒绝参考场景、280个单步向量及12组双池份额网格用于对照。这里的Token选费不代表Token2022转账扩展。当前六模型仍只开放报价，交易准备/SBF/钱包能力继续保持未实现标签。复验make -C ../integration rayclmm-exact-out-current-check，生成数据恢复入口rayclmm-exact-out-reference。


标准V3 ExactOut（阶段73，能力模型/10）：Uniswap V3及明确无LM的Pancake V3经典ERC20模型可选择双向币种和精确输出目标。原始Pool合约经真实mint/回调验证单池、同factory简单两跳和双费率池网格，报价字段仍表示原始输入，费用后净链成本未实现时继续明确为空。基础模型仅有定价状态投影，原程序费用/Oracle与钱包余额另作参考观察；这些不授予生产unsigned/钱包执行能力。Slipstream和Pancake LM继续使用各自上下文模型。复验make -C ../integration v3-exact-out-current-check，恢复入口v3-exact-out-reference。
