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

输入账户不填时按 caller/recipient、mint 和 Token program 派生 ATA；也可填写已存在的账户地址。缺失账户会返回 blockers，当前不会自动创建 ATA 或包装 SOL。账户满足条件后生成 legacy message 和带零签名槽的 transaction，前端可以下载完整 JSON，其中包含 planHash、snapshot、requiredSigners、blockhash 和有效块高。这个文件尚未签名、尚未广播，仍须独立模拟和有效性复核。

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
