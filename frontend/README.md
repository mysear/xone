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
