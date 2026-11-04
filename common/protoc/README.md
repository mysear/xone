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

工具版本为 protoc 3.21.12、protoc-gen-go 1.36.6、Rust 1.90.0、prost-build 0.14.4；Rust 生成工具依赖由自己的 Cargo.lock 固定。生成检查在临时目录重新生成并比较两种语言的文件，不改写源码。

Go 服务通过 go.mod 的 replace 引用 `golang`，两个 Rust 服务的 domain crate 引用 `rust` 并重新导出为 `wire`。服务不需要 protoc 或 prost-build。协议升级需同时提交源码、Go/Rust 生成物及适用的测试向量。
