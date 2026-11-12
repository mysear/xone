# 固定 Pump classic/SOL 参考

官方 docs 固定到 `639b87fb45c3adeffa6f7c4a1bf0cb85e392bf22`，npm SDK 固定 `@pump-fun/pump-sdk@4.3.1`（包完整性及文件摘要见 `source-pins.json`）。下载的 MIT SDK 和公开 IDL 存在忽略的缓存目录中。交易数学以本文指定的原始程序执行为有限参考，不能假定 SDK 辅助报价总与程序的小额舍入一致。

`public-bank-pins.json` 记录一次公开 finalized RPC 的 Pump、费用程序、ProgramData、Global 和 FeeConfig。原程序哈希分别为 `7a19affaa2c6187e04a5fe0ea298cee81fa5eb8c24cfcc30d85f615e89f7b02d` 和 `4f2955d0f6b40cc9ff7daa18ccad7d19119ba1249759f4e7e3bb691beee6b35e`。这些观测不等同于独立认证历史银行。

```sh
python3 integration/tools/ci/pump-reference-fetch.py
# 缓存缺失时恢复源码，并提供保持原摘要的本地公开RPC采集文件恢复账户：
python3 integration/tools/ci/pump-reference-fetch.py --fetch --capture integration/artifacts/local/pump-public-acquisition-20261111.json
python3 integration/tools/ci/pump-bonding-reference.py
python3 integration/tools/ci/pump-bonding-decoder.py
python3 integration/tools/acceptance/family_reference.py --reexecute
```

执行依赖集成环境现有的固定 solders/LiteSVM 与其 runtime lock。native harness 明确构造 curve、classic mint、SPL token 账户、用户、费用 vault 和 volume 账户；交易网络费使用独立付款人，用户实际 SOL/token 变化与交易费用分离。买回、协议和 creator 费用由原费用程序 CPI 决定并检查实际分账。

服务 profile 只支持 classic Token/native SOL、非 mayhem/cashback/holder 奖励、无自定义 creator fee、关闭 volume 跟踪且没有奖励期的既存用户账户模板。只开放 sell 与 buy_exact_sol_in 的 ExactIn，输出原生 SOL 是单独的 `native:solana:sol` 资产；不会自动视为 WSOL。完成曲线不再生成路由，也不会凭空推导迁移后的 AMM 储备。

缺失原始账户或程序升级时不能通过修改摘要继续验收。新用户账户创建/租金、Token2022、非SOL报价、PumpSwap、毕业迁移、ExactOut、明确的 wrap 及多协议交易仍需独立实现。测试与本地执行的通过不建立生产准入、真实公开银行或原子交易资格。
