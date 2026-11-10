# 协议源码与生成类型

```text
protoc/
  proto/dex/v1/state.proto
  golang/go.mod
  golang/dexpb/state.pb.go
  rust/Cargo.toml
  rust/dexpb/{lib.rs,dex.v1.rs}
  generator/cmd/xone-proto-generator/main.rs  # 维护时使用，服务不依赖
  Makefile
  check.py
```

唯一生成入口：

```bash
make -C common/protoc generate
make -C common/protoc check
```

工具版本为 protoc 3.21.12、protoc-gen-go 1.30.0、Rust 1.90.0、prost-build 0.14.4；Rust 生成工具依赖由自己的 Cargo.lock 固定。生成检查在临时目录重新生成并比较两种语言的文件，不改写源码。

Go 服务通过 go.mod 的 replace 引用 `golang`，两个 Rust 服务的 domain crate 引用 `rust` 并重新导出为 `wire`。服务不需要 protoc 或 prost-build。协议升级需同时提交源码、Go/Rust 生成物及适用的测试向量。

LFJ Liquidity Book v2.1.0 uses independent `PoolState.lfj` field31. The model
retains packed fee/volatility/oracle parameters, raw uint128 reserves and protocol
fees, a complete bounded ordered bin set with shares, source identities, the
fixed block/time and the needed Oracle ring samples. `block_number` and
`timestamp` have explicit optional scalar presence: an observed zero is preserved,
and an absent bank coordinate is rejected. Every Amount message carries exactly
32 big-endian bytes. Unknown models and ambiguous parallel state variants remain
rejected. C++-encoded field31 snapshots, including explicit zero bank coordinates,
are checked by both Go and Rust; fixed-zero values do not imply source qualification.

Slipstream contextual fixed-version quotes use independent `PoolState.slipstream_context`
field32. The state keeps bounded complete Oracle observations, initialized tick
staking/fee/reward ledgers, factory/module/Voter version identities and explicit
caller discount/view metadata. Required scalar zero/false observations use
optional protobuf presence; missing fields fail instead of becoming defaults.
A math-carried `derived=true` state is accepted only as an internal execution
state and is rejected as a canonical source snapshot. Legacy Slipstream admission
and its nonzero-module rejection remain separate.
