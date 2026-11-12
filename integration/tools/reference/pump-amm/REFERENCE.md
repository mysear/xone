# 固定 PumpSwap 与迁移参考

本参考使用 `pump-bonding/source-pins.json` 中固定的官方 docs 提交 `639b87fb45c3adeffa6f7c4a1bf0cb85e392bf22`、IDL 与 MIT SDK。程序身份为 `pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA`，原程序代码摘要固定为 `6a7ad34596d83b4c435b32f458a56e718e9f82eb4ad38e4316616d15d24d8ff7`，费用程序继续固定 `4f2955d0f6b40cc9ff7daa18ccad7d19119ba1249759f4e7e3bb691beee6b35e`。`public-bank-pins.json` 记录一次 finalized 公共读取的程序、ProgramData、GlobalConfig 和 FeeConfig；单次读取不构成可信历史银行资格。

```sh
python3 integration/tools/ci/pump-amm-reference-fetch.py
# 原始缓存缺失时，只允许保持原摘要的采集文件恢复：
python3 integration/tools/ci/pump-amm-reference-fetch.py --fetch --capture integration/artifacts/local/pump-amm-public-acquisition-20261111.json
python3 integration/tools/ci/pump-amm-reference.py
python3 integration/tools/ci/pump-amm-decoder.py
python3 integration/tools/ci/pump-migration-reference.py
python3 integration/tools/acceptance/solana_expanded_reference.py --reexecute
make -C integration solana-expanded-current-check
```

`pump-amm-classic-wsol` 是独立 program/profile/adapter 的 canonical PumpSwap v1：classic X/backed WSOL Y、无 mayhem/cashback/holder/custom creator fee、没有 v2 费用桶、已存在的用户 volume 账户和关闭的交易跟踪。`sell` 与 `buy_exact_quote_in` 提供 ExactIn；`buy` 提供 token 目标 ExactOut。LP/协议/creator 费独立 ceil，buyback 从协议费按 Global 拆分；保留12个完整账户及19个实际依赖。字段36复用完整程序/账户载体，校验由 profile 分派，不允许把287-byte AMM Pool当166-byte曲线。

所有程序执行在既有固定 LiteSVM/solders 环境中，网络费使用独立付款人。构造的银行、账户资金、时间与本地 slot 均明确属于受控输入。协议费用 ATA 和 creator vault 的真实 Token/WSOL lamport 变化同时检查。原测试输入与程序文件按仓库策略保存在忽略的缓存/数据目录，不能用更新后的链上程序替换原摘要。

`pump-bonding-sol-migration` 单独保留曲线用于迁移的 Token 库存。迁移参考实际执行原 Pump `migrate`，由其 CPI 调用原 PumpSwap、Token、Token2022、System、ATA 程序创建 Pool、vault、LP mint、boost vault；LP mint 使用 Token2022 不表示交易资产的 Token2022 组合已被支持。再次执行验证幂等性；后续三类交易直接交接这次创建的真实 Pool 和两个 vault，而不是重建一个价格相似的池。

canonical 目标解析只产生身份。只有源银行真正观测并验证目标完整依赖后，目标市场才有报价边；完成/已迁移曲线保留源行并排除。源配置仍显式声明观测目标，自动发现/EnsurePoolScope、隐式 SOL 包装、公开交易构建、生产独立模拟和净链费用未获本地证据以外的资格。

原migrate创建的boost市场带有已知正virtual_quote_reserves。canonical v1会读取该值，并以真实vault+virtual定价和选费用档；virtual不增加可付余额，sell仍必须覆盖真实vault扣款。额外严格`pump-amm-classic-wsol-boost`版本只接受正值，完整保留原Pool字节。迁移参考同时保存实际原程序InsufficientRealQuoteReserves反例，防止把定价库存当成提款能力。Curve SOL钱包和AMM WSOL ATA以实际地址识别共享资金，完成曲线只保留来源、不参与候选冲突。
