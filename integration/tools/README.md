# 开发检查工具

`ci/contracts.py` 检查各服务实际引用的 golden、debug 样本及固定版本 Rust 公共实现副本的一致性。协议源码和两种语言生成物只在 `common/protoc` 管理；`make -C integration contract-test` 先运行公共生成一致性检查，再运行各服务的契约测试。

`ci/boundary.py` 检查解析器/报价器的职责与计算模块依赖边界。基础设施联调、临时认证配置和验收脚本位于 `acceptance`。`ci/prepare-compose.py` 生成普通 RPC 服务的单文件配置；不再向服务目录创建开发 token 文件。
