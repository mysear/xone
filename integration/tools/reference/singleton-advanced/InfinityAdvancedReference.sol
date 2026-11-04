// SPDX-License-Identifier: MIT
pragma solidity 0.8.26;
import {ReferenceTypes} from "./ReferenceTypes.sol";
import {CLPool} from "infinity/src/pool-cl/libraries/CLPool.sol";
import {Tick} from "infinity/src/pool-cl/libraries/Tick.sol";
import {TickMath} from "infinity/src/pool-cl/libraries/TickMath.sol";
import {CLSlot0, CLSlot0Library} from "infinity/src/pool-cl/types/CLSlot0.sol";
import {BalanceDelta, BalanceDeltaLibrary} from "infinity/src/types/BalanceDelta.sol";

import {PoolKey} from "infinity/src/types/PoolKey.sol";
import {PoolId, PoolIdLibrary} from "infinity/src/types/PoolId.sol";
import {Hooks} from "infinity/src/libraries/Hooks.sol";
import {CLHooks} from "infinity/src/pool-cl/libraries/CLHooks.sol";
import {ICLPoolManager} from "infinity/src/pool-cl/interfaces/ICLPoolManager.sol";
import {BeforeSwapDelta} from "infinity/src/types/BeforeSwapDelta.sol";

/// @notice Executes the unchanged pinned CLPool.swap, with explicitly seeded test storage.
contract InfinityAdvancedReference is ReferenceTypes {
    using CLPool for CLPool.State;
    using CLSlot0Library for CLSlot0;
    using BalanceDeltaLibrary for BalanceDelta;
    CLPool.State private state;

    function priceAtTick(int24 tick) external pure returns (uint160) {
        return TickMath.getSqrtRatioAtTick(tick);
    }
    function maxLiquidity(int24 spacing) external pure returns (uint128) {
        return Tick.tickSpacingToMaxLiquidityPerTick(spacing);
    }
    function _seed(Seed calldata seed) private {
        state.initialize(seed.sqrtPriceX96, seed.protocolFee, 0);
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
        (BalanceDelta delta, CLPool.SwapState memory next) = state.swap(
            CLPool.SwapParams(spacing, zero, -int256(input), limit, 0)
        );
        int128 paid = zero ? delta.amount0() : delta.amount1();
        int128 received = zero ? delta.amount1() : delta.amount0();
        r = Result(uint256(int256(received)), uint256(-int256(paid)), input - uint256(-int256(paid)),
                   next.sqrtPriceX96, next.tick, next.liquidity, next.swapFee, next.feeAmountToProtocol);
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
        CLHooks.validatePermissionsConflict(key);
        Hooks.validateHookConfig(key);
        require(uint256(key.parameters) & 0xcc0 == 0, "callback requires execution");
        ICLPoolManager.SwapParams memory params = ICLPoolManager.SwapParams(true, -100, 1);
        (int256 amount, BeforeSwapDelta beforeDelta, uint24 fee) = CLHooks.beforeSwap(key, params, data);
        (BalanceDelta delta, BalanceDelta hookDelta) = CLHooks.afterSwap(key, params, BalanceDelta.wrap(0), data, beforeDelta);
        return amount == -100 && BeforeSwapDelta.unwrap(beforeDelta) == 0 && fee == 0 && BalanceDelta.unwrap(delta) == 0 && BalanceDelta.unwrap(hookDelta) == 0;
    }
    function quoteAdvanced(Seed calldata seed, uint256 input, bool zero, uint160 limit) external returns (Result memory) {
        _seed(seed);
        state.setLPFee(seed.lpFee);
        return _swap(seed.spacing, input, zero, limit);
    }
}
