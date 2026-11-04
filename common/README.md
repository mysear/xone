# 公共协议

`protoc/` 统一管理协议源码和生成类型。服务只引用已生成类型，不保存协议副本，也不在服务构建时生成代码。

- `protoc/proto/dex/v1/state.proto`：唯一协议源码。
- `protoc/golang/dexpb/`：Go 生成类型包 `dexpb`，所属 module 为 `xone/common/protoc/golang`。
- `protoc/rust/dexpb/`：Rust 生成类型及库入口，所属 crate 为 `xone-proto`。
- `protoc/generator/`：集中使用的 Rust 生成工具。

更新协议后执行 `make -C common/protoc generate`，提交协议和两种语言生成物；执行 `make -C common/protoc check` 检查生成物是否匹配。生成工具与普通服务构建分离，详见 [protoc/README.md](protoc/README.md)。

服务的 Go replace / Cargo path 依赖这里，因此构建时需保留 xone 的相邻目录结构。Docker 构建 context 为 xone 根目录。公共测试样本和 golden 的维护源仍位于 `integration/test`。
