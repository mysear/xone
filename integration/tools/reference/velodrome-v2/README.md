# Velodrome V2 固定源码数学参考

固定官方 `velodrome-finance/contracts` 提交 `b3065d8b6702b14b094f9f6046b752cc9f78c43b` 的 `contracts/Pool.sol`，原文件的 GPL-3.0-or-later 标识保留。`source-metadata.json` 保存提交、路径、原始 URL 和 SHA-256。

在已建立固定 `stable-reference` 环境后，执行 `make -C integration velodrome-v2-reference-refresh`。生成器核对源码和 Solidity 0.8.19，原样提取 `_f`、`_d`、`_get_y`、`_getAmountOut`、`_k`，以 Ganache 执行独立 Solidity 包装器；包装器按原池次序扣除输入手续费、计算输出并检查后余额不变量。没有导入服务数学。

原始源码中 `_get_y` 的 `dy==0` 上升分支采用 `_f(x0,y+1)`；已有 Aerodrome 固定版本采用再次缩放的 `_k(x0,y+1)`。服务为 Velodrome 保留单独的协议名和 `b3065d8b-1` adapter，不改变 Aerodrome 旧分支。

生成 216 个双向、稳定/波动、精度、失衡、0/4/300 手续费、dust 和 revert 用例；8 组连续交换共 24 步比较输出及完整两个后余额。另有 30 个极端失衡舍入样本和固定 Aerodrome 原函数控制组；当前这 30 个样本两版本输出相同，不宣称已发现输出差异反例。三个服务只保留其测试直接使用的 JSON 副本；Router 的原源码与包装器本地副本供独立源绑定测试使用。

该参考是 Q1 数学与模型状态验证。包装器没有执行完整 `Pool.swap` 或真实 Token 转账，也没有证明某个 OP 部署与本提交对应。实际部署/历史锚点和生产执行资格继续独立验证。
