// SPDX-License-Identifier: MIT
pragma solidity 0.8.26;
import {ReferenceTypes} from "./ReferenceTypes.sol";
import {Pool} from "v4/src/libraries/Pool.sol";
import {TickMath} from "v4/src/libraries/TickMath.sol";
import {Slot0, Slot0Library} from "v4/src/types/Slot0.sol";
import {BalanceDelta, BalanceDeltaLibrary} from "v4/src/types/BalanceDelta.sol";

import {PoolKey} from "v4/src/types/PoolKey.sol";
import {PoolId, PoolIdLibrary} from "v4/src/types/PoolId.sol";
import {Hooks} from "v4/src/libraries/Hooks.sol";
import {SwapParams as HookSwapParams} from "v4/src/types/PoolOperation.sol";
import {BeforeSwapDelta} from "v4/src/types/BeforeSwapDelta.sol";

/// @notice Executes the unchanged pinned Pool.swap, with explicitly seeded test storage.
contract V4AdvancedReference is ReferenceTypes {
    using Pool for Pool.State;
    using Slot0Library for Slot0;
    using BalanceDeltaLibrary for BalanceDelta;
    Pool.State private state;

    function priceAtTick(int24 tick) external pure returns (uint160) {
        return TickMath.getSqrtPriceAtTick(tick);
    }
    function maxLiquidity(int24 spacing) external pure returns (uint128) {
        return Pool.tickSpacingToMaxLiquidityPerTick(spacing);
    }
    function _seed(Seed calldata seed) private {
        state.initialize(seed.sqrtPriceX96, 0);
        state.setProtocolFee(seed.protocolFee);
        state.slot0 = state.slot0.setTick(seed.tick);
        state.liquidity = seed.liquidity;
        for (uint256 i; i < seed.words.length; ++i) state.tickBitmap[seed.words[i].pos] = seed.words[i].bitmap;
        for (uint256 i; i < seed.ticks.length; ++i) {
            TickSeed calldata t = seed.ticks[i];
            state.ticks[t.tick].liquidityGross = t.gross;
            state.ticks[t.tick].liquidityNet = t.net;
        }
    }
    function _swap(int24 spacing, uint256 input, bool zero, uint160 limit) private returns (Result memory r) {
        require(input <= uint256(type(int256).max), "input is not negative int256 representable");
        (BalanceDelta delta, uint256 protocolAmount, uint24 fee, Pool.SwapResult memory next) = state.swap(
            Pool.SwapParams(-int256(input), spacing, zero, limit, 0)
        );
        int128 paid = zero ? delta.amount0() : delta.amount1();
        int128 received = zero ? delta.amount1() : delta.amount0();
        r = Result(uint256(int256(received)), uint256(-int256(paid)), input - uint256(-int256(paid)),
                   next.sqrtPriceX96, next.tick, next.liquidity, fee, protocolAmount);
    }
    function quote(Seed calldata seed, uint256 input, bool zero, uint160 limit) external returns (Result memory) {
        _seed(seed);
        return _swap(seed.spacing, input, zero, limit);
    }
    function sequence(Seed calldata seed, uint256[] calldata inputs, bool[] calldata zeros, uint160[] calldata limits)
        external returns (Result[] memory results)
    {
        require(inputs.length == zeros.length && inputs.length == limits.length);
        _seed(seed);
        results = new Result[](inputs.length);
        for (uint256 i; i < inputs.length; ++i) results[i] = _swap(seed.spacing, inputs[i], zeros[i], limits[i]);
    }

    function keyId(PoolKey memory key) external pure returns (bytes32) {
        return PoolId.unwrap(PoolIdLibrary.toId(key));
    }
    function dispatch(PoolKey memory key, bytes calldata data) external returns (bool) {
        require(Hooks.isValidHookAddress(key.hooks, key.fee), "invalid hook config");
        require(uint160(address(key.hooks)) & 0xcc == 0, "callback requires execution");
        HookSwapParams memory params = HookSwapParams(true, -100, 1);
        (int256 amount, BeforeSwapDelta beforeDelta, uint24 fee) = Hooks.beforeSwap(key.hooks, key, params, data);
        (BalanceDelta delta, BalanceDelta hookDelta) = Hooks.afterSwap(key.hooks, key, params, BalanceDelta.wrap(0), data, beforeDelta);
        return amount == -100 && BeforeSwapDelta.unwrap(beforeDelta) == 0 && fee == 0 && BalanceDelta.unwrap(delta) == 0 && BalanceDelta.unwrap(hookDelta) == 0;
    }
    function quoteAdvanced(Seed calldata seed, uint256 input, bool zero, uint160 limit) external returns (Result memory) {
        _seed(seed);
        state.setLPFee(seed.lpFee);
        return _swap(seed.spacing, input, zero, limit);
    }
}
