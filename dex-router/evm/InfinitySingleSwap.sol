// SPDX-License-Identifier: MIT
pragma solidity 0.8.26;

import {ILockCallback} from "infinity/src/interfaces/ILockCallback.sol";
import {IPoolManager} from "infinity/src/interfaces/IPoolManager.sol";
import {IVault} from "infinity/src/interfaces/IVault.sol";
import {IBinPoolManager} from "infinity/src/pool-bin/interfaces/IBinPoolManager.sol";
import {ICLPoolManager} from "infinity/src/pool-cl/interfaces/ICLPoolManager.sol";
import {TickMath} from "infinity/src/pool-cl/libraries/TickMath.sol";
import {BalanceDelta, BalanceDeltaLibrary} from "infinity/src/types/BalanceDelta.sol";
import {Currency, CurrencyLibrary} from "infinity/src/types/Currency.sol";
import {PoolKey} from "infinity/src/types/PoolKey.sol";

interface InfinityFundingToken {
    function transferFrom(address payer, address recipient, uint256 amount) external returns (bool);
}

interface InfinityVaultProvider {
    function vault() external view returns (IVault);
}

/// Executes one Infinity swap and settles the signed deltas in the manager's Vault.
contract InfinitySingleSwap is ILockCallback {
    using BalanceDeltaLibrary for BalanceDelta;
    using CurrencyLibrary for Currency;

    error InvalidSwap();
    error AmountMismatch();
    error UnauthorizedCallback();
    error ReentrantSwap();

    IPoolManager public immutable manager;
    IVault public immutable vault;
    bool public immutable isBin;
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

    constructor(IPoolManager manager_, IVault vault_, bool isBin_) {
        if (address(manager_).code.length == 0 || address(vault_).code.length == 0 ||
            InfinityVaultProvider(address(manager_)).vault() != vault_ || !vault_.isAppRegistered(address(manager_))) {
            revert InvalidSwap();
        }
        manager = manager_;
        vault = vault_;
        isBin = isBin_;
    }

    function execute(Order calldata order) external payable returns (uint128 paid, uint128 received) {
        if (active) revert ReentrantSwap();
        if (order.key.poolManager != manager || order.recipient == address(0) || order.recipient == address(this) ||
            order.recipient == address(vault) || order.recipient == address(manager) ||
            order.inputLimit == 0 || order.outputLimit == 0 ||
            order.inputLimit > uint128(type(int128).max) || order.outputLimit > uint128(type(int128).max) ||
            block.timestamp > order.deadline) revert InvalidSwap();

        Currency input = order.zeroForOne ? order.key.currency0 : order.key.currency1;
        if (msg.value != (input.isNative() ? order.inputLimit : 0)) revert InvalidSwap();

        active = true;
        (paid, received) = abi.decode(vault.lock(abi.encode(order, msg.sender)), (uint128, uint128));
        if (msg.value > paid) Currency.wrap(address(0)).transfer(msg.sender, msg.value - paid);
        active = false;
    }

    function lockAcquired(bytes calldata data) external returns (bytes memory) {
        if (msg.sender != address(vault) || !active) revert UnauthorizedCallback();
        (Order memory order, address payer) = abi.decode(data, (Order, address));
        Currency input = order.zeroForOne ? order.key.currency0 : order.key.currency1;
        Currency output = order.zeroForOne ? order.key.currency1 : order.key.currency0;
        int128 specified = order.exactOut ? int128(order.outputLimit) : -int128(order.inputLimit);
        bytes memory hookData = abi.encode(false);
        BalanceDelta delta = isBin
            ? IBinPoolManager(address(manager)).swap(order.key, order.zeroForOne, specified, hookData)
            : ICLPoolManager(address(manager)).swap(order.key, ICLPoolManager.SwapParams({
                zeroForOne: order.zeroForOne,
                amountSpecified: specified,
                sqrtPriceLimitX96: order.zeroForOne ? TickMath.MIN_SQRT_RATIO + 1 : TickMath.MAX_SQRT_RATIO - 1
            }), hookData);
        int128 inputDelta = order.zeroForOne ? delta.amount0() : delta.amount1();
        int128 outputDelta = order.zeroForOne ? delta.amount1() : delta.amount0();
        if (inputDelta >= 0 || outputDelta <= 0) revert AmountMismatch();
        uint128 paid = uint128(uint256(-int256(inputDelta)));
        uint128 received = uint128(outputDelta);
        if (order.exactOut ? paid > order.inputLimit || received != order.outputLimit
                           : paid != order.inputLimit || received < order.outputLimit) revert AmountMismatch();

        vault.sync(input);
        if (input.isNative()) {
            if (vault.settle{value: paid}() != paid) revert AmountMismatch();
        } else {
            uint256 beforeInput = input.balanceOf(payer);
            if (!InfinityFundingToken(Currency.unwrap(input)).transferFrom(payer, address(vault), paid) ||
                beforeInput - input.balanceOf(payer) != paid || vault.settle() != paid) revert AmountMismatch();
        }

        uint256 beforeOutput = output.balanceOf(order.recipient);
        vault.take(output, order.recipient, received);
        if (output.balanceOf(order.recipient) - beforeOutput != received) revert AmountMismatch();
        return abi.encode(paid, received);
    }
}
