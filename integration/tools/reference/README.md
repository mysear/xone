# 参考验证代码与本地缓存

本目录在版本控制中保留参考验证代码、生成器、测试程序、构建定义、依赖锁文件、版本与 SHA-256 清单、许可证及说明文档。下载的上游源码、编译器、第三方包、构建输出和执行证据作为本地缓存，由 `integration/.gitignore` 排除。

清理版本控制只移除 Git 索引中的缓存文件，不删除已有工作目录中的文件。首次克隆后，需要准备相应缓存和测试数据才能运行参考验证。子目录文档中的完整源码树、冻结产物和执行证据描述其运行时输入；这些内容不保证随当前 Git 提交分发。

## 内容归属

| 保留在 Git 中 | 保存在本地、按需准备 |
| --- | --- |
| 自写 harness、验证器、生成器、测试代码 | 下载得到的 `upstream/`、`deps/`、`sources/` 和第三方 `packages/` |
| Cargo/npm 清单、锁文件、编译器来源信息 | `soljson*.js` 编译器程序及下载压缩包 |
| `source-*.json`、`*-pins.json`、冻结清单等版本与校验信息 | `build-info/`、第三方构建结果、编译生成的字节码 |
| 输入矩阵、协议定义、许可证、README | golden/fixture、执行回执、运行日志、生成报告和运行时元数据 |

第三方许可证的独立副本保留在 `licenses/`，其索引记录原路径与内容哈希。原目录中已受版本控制的许可证和来源信息也予以保留。

## 准备上游源码

从项目根目录执行所需命令。每个命令只负责对应参考依赖，不代表完整验收通过：

```sh
python3 integration/tools/ci/orca-reference-fetch.py --fetch
python3 integration/tools/ci/rayclmm-reference-fetch.py --fetch
python3 integration/tools/ci/raydium-reference-fetch.py --fetch
python3 integration/tools/ci/weighted-reference-fetch.py
```

Orca 和 Raydium 下载器按固定提交恢复归档和原始目录，并验证记录的哈希。Weighted 下载器将源码放入 `integration/.runtime/weighted-reference/sources/`。其他依赖准备入口见 `tools/ci/*-oracle-setup.py` 及各参考目录的 README；部分入口同时生成测试数据，需先安装其指定工具链。

以下依赖的获取信息继续保留：

- Balancer 导入源码：`balancer-multi/<revision>.json` 保存仓库、固定提交及逐文件哈希；已有 `balancer-multi-fetch.py` 在清单存在时会验证缓存，缺少文件时应先按清单恢复源码。
- Solidity 0.7.1：两个参考目录中的 `compiler/solc-0.7.1-origin.json` 保存 npm 压缩包地址、包哈希和 `soljson` 文件哈希。
- Balancer build-info：`balancer-managed-v2-aum/source-pins.json` 保存每个原始文件的固定提交地址、原始内容哈希及压缩文件哈希。恢复压缩文件时必须同时满足现有验证器的校验要求。
- Uniswap 发布包：`evm-execution/*-registry.json` 保存下载地址和包完整性字段，`evm-execution/source-pins.json` 保存版本、压缩包与逐文件哈希。

不能用“最新版本”替代固定版本，也不能通过修改校验值使不同输入通过验证。

## 恢复历史参考缓存

清理前提交 `ba62769a7b4aa0437f0024280d9fe0e740a7a6ed` 仍保留在 Git 历史中。在尚未准备缓存的工作目录，可选择所需路径恢复历史下载内容，例如：

```sh
git restore --source=ba62769a7b4aa0437f0024280d9fe0e740a7a6ed --worktree -- \
  integration/tools/reference/balancer-multi/sources \
  integration/tools/reference/balancer-managed-fees/compiler/soljson-v0.7.1.js \
  integration/tools/reference/balancer-managed-v2-aum/compiler/soljson-v0.7.1.js \
  integration/tools/reference/balancer-managed-v2-aum/build-info \
  integration/tools/reference/evm-execution/packages
```

此命令只更新指定工作目录路径，不重新将缓存加入索引；已有同路径缓存会被历史版本覆盖，因此仅在缓存缺失或确需恢复固定历史版本时使用。浅克隆或裁剪过历史的仓库可能不包含该提交，届时应使用上述固定来源准备缓存。

## 生成数据与运行检查

依赖就绪后，按各目录 README 和 `integration/Makefile` 的 `*-reference-refresh`、生成器及检查入口准备数据，再运行相应测试。测试数据已经从仓库中排除，历史缓存恢复不等于所有 golden 或验收数据已就绪。

执行回执、`generation.json`、`runtime-metadata.json`、`verification.json`、`verified.json`、字节码及日志留在本地。冻结清单中的历史哈希仍用于核对指定输入，不表示本次工作目录中的代码或新生成结果已通过验收；输入变化后应重新运行对应生成与验证步骤。
