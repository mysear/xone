// SPDX-License-Identifier: MIT
pragma solidity 0.8.26;

/// @notice Test-only transport types. No quote arithmetic is implemented here.
abstract contract ReferenceTypes {
    struct Word { int16 pos; uint256 bitmap; }
    struct TickSeed { int24 tick; uint128 gross; int128 net; }
    struct Seed {
        uint160 sqrtPriceX96;
        int24 tick;
        uint128 liquidity;
        int24 spacing;
        uint24 lpFee;
        uint24 protocolFee;
        Word[] words;
        TickSeed[] ticks;
    }
    struct Result {
        uint256 amountOut;
        uint256 consumedIn;
        uint256 remainingIn;
        uint160 sqrtPriceX96;
        int24 tick;
        uint128 liquidity;
        uint24 swapFee;
        uint256 protocolAmount;
    }
}
