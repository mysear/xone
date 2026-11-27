// SPDX-License-Identifier: MIT
pragma solidity 0.8.26;

import {IPoolManager} from "v4/src/interfaces/IPoolManager.sol";
import {IUnlockCallback} from "v4/src/interfaces/callback/IUnlockCallback.sol";
import {TickMath} from "v4/src/libraries/TickMath.sol";
import {BalanceDelta, BalanceDeltaLibrary} from "v4/src/types/BalanceDelta.sol";
import {Currency, CurrencyLibrary} from "v4/src/types/Currency.sol";
import {PoolKey} from "v4/src/types/PoolKey.sol";
import {SwapParams} from "v4/src/types/PoolOperation.sol";

interface V4FundingToken {
    function transferFrom(address payer, address recipient, uint256 amount) external returns (bool);
}

/// Executes a single V4 swap with the user's funds and verifies the recipient's balance change.
contract V4SingleSwap is IUnlockCallback {
    using BalanceDeltaLibrary for BalanceDelta;
    using CurrencyLibrary for Currency;

    error InvalidSwap();
    error AmountMismatch();
    error UnauthorizedCallback();
    error ReentrantSwap();

    IPoolManager public immutable manager;
    bool private active;

    struct Order {
        PoolKey key;
        bool zeroForOne;
        bool exactOut;
        uint128 inputLimit;
        uint128 outputLimit;
        address recipient;
        uint256 deadline;
    }

    constructor(IPoolManager manager_) {
        if (address(manager_).code.length == 0) revert InvalidSwap();
        manager = manager_;
    }

    function execute(Order calldata order) external payable returns (uint128 paid, uint128 received) {
        if (active) revert ReentrantSwap();
        if (order.recipient == address(0) || order.recipient == address(this) ||
            order.recipient == address(manager) || order.inputLimit == 0 || order.outputLimit == 0 ||
            order.inputLimit > uint128(type(int128).max) || order.outputLimit > uint128(type(int128).max) ||
            block.timestamp > order.deadline) revert InvalidSwap();

        Currency input = order.zeroForOne ? order.key.currency0 : order.key.currency1;
        if (msg.value != (input.isAddressZero() ? order.inputLimit : 0)) revert InvalidSwap();

        active = true;
        (paid, received) = abi.decode(manager.unlock(abi.encode(order, msg.sender)), (uint128, uint128));
        if (msg.value > paid) CurrencyLibrary.ADDRESS_ZERO.transfer(msg.sender, msg.value - paid);
        active = false;
    }

    function unlockCallback(bytes calldata data) external returns (bytes memory) {
        if (msg.sender != address(manager) || !active) revert UnauthorizedCallback();
        (Order memory order, address payer) = abi.decode(data, (Order, address));
        Currency input = order.zeroForOne ? order.key.currency0 : order.key.currency1;
        Currency output = order.zeroForOne ? order.key.currency1 : order.key.currency0;
        BalanceDelta delta = manager.swap(
            order.key,
            SwapParams({
                zeroForOne: order.zeroForOne,
                amountSpecified: order.exactOut ? int256(uint256(order.outputLimit)) : -int256(uint256(order.inputLimit)),
                sqrtPriceLimitX96: order.zeroForOne ? TickMath.MIN_SQRT_PRICE + 1 : TickMath.MAX_SQRT_PRICE - 1
            }),
            abi.encode(false)
        );
        int128 inputDelta = order.zeroForOne ? delta.amount0() : delta.amount1();
        int128 outputDelta = order.zeroForOne ? delta.amount1() : delta.amount0();
        if (inputDelta >= 0 || outputDelta <= 0) revert AmountMismatch();
        uint128 paid = uint128(uint256(-int256(inputDelta)));
        uint128 received = uint128(outputDelta);
        if (order.exactOut ? paid > order.inputLimit || received != order.outputLimit
                           : paid != order.inputLimit || received < order.outputLimit) revert AmountMismatch();

        manager.sync(input);
        if (input.isAddressZero()) {
            if (manager.settle{value: paid}() != paid) revert AmountMismatch();
        } else {
            uint256 beforeInput = input.balanceOf(payer);
            if (!V4FundingToken(Currency.unwrap(input)).transferFrom(payer, address(manager), paid) ||
                beforeInput - input.balanceOf(payer) != paid || manager.settle() != paid) revert AmountMismatch();
        }

        uint256 beforeOutput = output.balanceOf(order.recipient);
        manager.take(output, order.recipient, received);
        if (output.balanceOf(order.recipient) - beforeOutput != received) revert AmountMismatch();
        return abi.encode(paid, received);
    }
}
