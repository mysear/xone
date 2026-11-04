// SPDX-License-Identifier: GPL-2.0-or-later
pragma solidity 0.8.26;
import {BinPool} from "infinity/src/pool-bin/libraries/BinPool.sol";
import {PriceHelper} from "infinity/src/pool-bin/libraries/PriceHelper.sol";
import {BalanceDelta, BalanceDeltaLibrary} from "infinity/src/types/BalanceDelta.sol";
contract InfinityBinReference {
    using BinPool for BinPool.State;
    using BalanceDeltaLibrary for BalanceDelta;
    BinPool.State private state;
    struct Seed { uint24 activeId; uint16 binStep; uint128 reserve0; uint128 reserve1; uint24 lpFee; uint24 protocolFee; }
    struct Result { uint256 amountOut; uint256 protocolAmount; uint24 swapFee; uint128 reserve0; uint128 reserve1; uint256 priceX128; }
    // One active bin, no initialized tree successors; spilling input reverts in original BinPool.swap.
    function quote(Seed calldata seed, uint128 input, bool zero) external returns (Result memory r) {
        state.initialize(seed.activeId,seed.protocolFee,seed.lpFee);
        state.setLPFee(seed.lpFee);
        state.reserveOfBin[seed.activeId] = bytes32(uint256(seed.reserve0) | uint256(seed.reserve1) << 128);
        (BalanceDelta delta, BinPool.SwapState memory next) = state.swap(BinPool.SwapParams(zero,seed.binStep,0,-int128(input)));
        bytes32 reserves = state.reserveOfBin[seed.activeId];
        r.amountOut = uint256(int256(zero ? delta.amount1() : delta.amount0()));
        r.protocolAmount = zero ? uint128(uint256(next.feeAmountToProtocol)) : uint128(uint256(next.feeAmountToProtocol) >> 128);
        r.swapFee = next.swapFee;
        r.reserve0 = uint128(uint256(reserves)); r.reserve1 = uint128(uint256(reserves) >> 128);
        r.priceX128 = PriceHelper.getPriceFromId(seed.activeId,seed.binStep);
    }
}
