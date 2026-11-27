// SPDX-License-Identifier: MIT
pragma solidity 0.8.26;

import {IHooks} from "v4/src/interfaces/IHooks.sol";
import {IPoolManager} from "v4/src/interfaces/IPoolManager.sol";
import {IUnlockCallback} from "v4/src/interfaces/callback/IUnlockCallback.sol";
import {LPFeeLibrary} from "v4/src/libraries/LPFeeLibrary.sol";
import {StateLibrary} from "v4/src/libraries/StateLibrary.sol";
import {TickMath} from "v4/src/libraries/TickMath.sol";
import {TransientStateLibrary} from "v4/src/libraries/TransientStateLibrary.sol";
import {BalanceDelta, BalanceDeltaLibrary, toBalanceDelta} from "v4/src/types/BalanceDelta.sol";
import {BeforeSwapDelta, toBeforeSwapDelta} from "v4/src/types/BeforeSwapDelta.sol";
import {Currency, CurrencyLibrary} from "v4/src/types/Currency.sol";
import {PoolIdLibrary} from "v4/src/types/PoolId.sol";
import {PoolKey} from "v4/src/types/PoolKey.sol";
import {ModifyLiquidityParams, SwapParams} from "v4/src/types/PoolOperation.sol";
import {V4SingleSwap} from "V4SingleSwap.sol";

contract SettlementToken {
    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;

    constructor() {
        balanceOf[msg.sender] = 1e24;
    }

    function transfer(address to, uint256 amount) external returns (bool) {
        balanceOf[msg.sender] -= amount;
        balanceOf[to] += amount;
        return true;
    }

    function approve(address spender, uint256 amount) external {
        allowance[msg.sender][spender] = amount;
    }

    function transferFrom(address payer, address to, uint256 amount) external returns (bool) {
        allowance[payer][msg.sender] -= amount;
        balanceOf[payer] -= amount;
        balanceOf[to] += amount;
        return true;
    }
}

contract SettlementHook {
    IPoolManager private immutable manager;
    uint24 private constant SWAP_FEE = 17_000;
    int128 private constant SPECIFIED = 7;
    int128 private constant UNSPECIFIED = 11;

    constructor(IPoolManager poolManager) {
        manager = poolManager;
    }

    receive() external payable {}

    function beforeSwap(address, PoolKey calldata key, SwapParams calldata params, bytes calldata hookData)
        external returns (bytes4, BeforeSwapDelta, uint24)
    {
        require(msg.sender == address(manager));
        bool specifiedIs0 = (params.amountSpecified < 0) == params.zeroForOne;
        if (!abi.decode(hookData, (bool))) {
            manager.take(specifiedIs0 ? key.currency0 : key.currency1, address(this), uint128(SPECIFIED));
        }
        return (IHooks.beforeSwap.selector, toBeforeSwapDelta(SPECIFIED, 0), LPFeeLibrary.OVERRIDE_FEE_FLAG | SWAP_FEE);
    }

    function afterSwap(address, PoolKey calldata key, SwapParams calldata params, BalanceDelta, bytes calldata)
        external returns (bytes4, int128)
    {
        require(msg.sender == address(manager));
        bool specifiedIs0 = (params.amountSpecified < 0) == params.zeroForOne;
        manager.take(specifiedIs0 ? key.currency1 : key.currency0, address(this), uint128(UNSPECIFIED));
        return (IHooks.afterSwap.selector, UNSPECIFIED);
    }
}

contract V4ManagerSettlement is IUnlockCallback {
    using CurrencyLibrary for Currency;
    using BalanceDeltaLibrary for BalanceDelta;
    using PoolIdLibrary for PoolKey;
    using StateLibrary for IPoolManager;
    using TransientStateLibrary for IPoolManager;

    IPoolManager private immutable manager;
    IHooks private hook;
    address private constant RECIPIENT = address(0x3456);
    int256 private constant AMOUNT = 1_000_000;

    constructor(IPoolManager poolManager) {
        manager = poolManager;
    }

    receive() external payable {}

    function installHook(bytes32 salt) external {
        require(address(hook) == address(0));
        hook = IHooks(address(new SettlementHook{salt: salt}(manager)));
        require(uint160(address(hook)) & 0x3fff == 0xcc);
    }

    function run(bool nativeCurrency, bool zeroForOne, bool exactIn, bool failure, V4SingleSwap executor)
        external payable returns (address, address, int128, int128, uint160, int24)
    {
        PoolKey memory key = createKey(nativeCurrency);
        PoolKey memory control = PoolKey(key.currency0, key.currency1, 17_000, 60, IHooks(address(0)));
        uint160 price = uint160(1 << 96);
        manager.initialize(key, price);
        manager.initialize(control, price);
        manager.unlock(abi.encode(key, int256(0), zeroForOne, address(this), false));
        manager.unlock(abi.encode(control, int256(0), zeroForOne, address(this), false));

        int256 specified = exactIn ? -AMOUNT : AMOUNT;
        BalanceDelta core = BalanceDelta.wrap(abi.decode(manager.unlock(
            abi.encode(control, specified + 7, zeroForOne, address(this), false)
        ), (int256)));
        uint256[2] memory managerBefore = balances(key, address(manager));
        uint256[2] memory callerBefore = balances(key, address(this));
        uint256[2] memory hookBefore = balances(key, address(hook));
        uint256[2] memory receiverBefore = balances(key, RECIPIENT);
        bool specifiedIs0 = exactIn == zeroForOne;
        int128[2] memory hookFees = specifiedIs0 ? [int128(7), int128(11)] : [int128(11), int128(7)];
        BalanceDelta expected = toBalanceDelta(core.amount0() - hookFees[0], core.amount1() - hookFees[1]);
        BalanceDelta swap = address(executor) == address(0)
            ? BalanceDelta.wrap(abi.decode(manager.unlock(abi.encode(key, specified, zeroForOne, RECIPIENT, failure)), (int256)))
            : executeSwap(executor, key, zeroForOne, exactIn, expected, failure);
        require(BalanceDelta.unwrap(swap) == BalanceDelta.unwrap(expected), "swap deltas");
        require((exactIn ? -(zeroForOne ? swap.amount0() : swap.amount1()) : (zeroForOne ? swap.amount1() : swap.amount0())) == AMOUNT, "specified amount");
        for (uint256 i; i < 2; ++i) {
            Currency currency = i == 0 ? key.currency0 : key.currency1;
            int128 net = i == 0 ? swap.amount0() : swap.amount1();
            require(currency.balanceOf(address(hook)) == hookBefore[i] + uint128(hookFees[i]), "hook balance");
            if (net < 0) {
                require(currency.balanceOf(address(this)) + uint128(-net) == callerBefore[i], "input balance");
                require(currency.balanceOf(RECIPIENT) == receiverBefore[i], "input recipient");
            } else {
                require(currency.balanceOf(address(this)) == callerBefore[i], "output caller");
                require(currency.balanceOf(RECIPIENT) == receiverBefore[i] + uint128(net), "output balance");
            }
            int128 poolDelta = i == 0 ? core.amount0() : core.amount1();
            require(int256(currency.balanceOf(address(manager))) + poolDelta == int256(managerBefore[i]), "manager balance");
        }
        (uint160 hookedPrice, int24 hookedTick, , uint24 liveFee) = manager.getSlot0(key.toId());
        (uint160 controlPrice, int24 controlTick, , ) = manager.getSlot0(control.toId());
        require(hookedPrice != price && hookedPrice == controlPrice && hookedTick == controlTick && liveFee == 0, "pool state");
        require(!manager.isUnlocked() && manager.getNonzeroDeltaCount() == 0, "final lock state");
        return (Currency.unwrap(key.currency0), Currency.unwrap(key.currency1), swap.amount0(), swap.amount1(), hookedPrice, hookedTick);
    }

    function prepareReplay(bool nativeCurrency) external payable returns (address, address) {
        PoolKey memory key = createKey(nativeCurrency);
        manager.initialize(key, uint160(1 << 96));
        manager.unlock(abi.encode(key, int256(0), false, address(this), false));
        address token0 = Currency.unwrap(key.currency0);
        address token1 = Currency.unwrap(key.currency1);
        if (token0 != address(0)) require(SettlementToken(token0).transfer(msg.sender, 1e15));
        require(SettlementToken(token1).transfer(msg.sender, 1e15));
        return (token0, token1);
    }

    function createKey(bool nativeCurrency) private returns (PoolKey memory) {
        address token0 = nativeCurrency ? address(0) : address(new SettlementToken());
        address token1 = address(new SettlementToken());
        if (token0 > token1) (token0, token1) = (token1, token0);
        return PoolKey(Currency.wrap(token0), Currency.wrap(token1), LPFeeLibrary.DYNAMIC_FEE_FLAG, 60, hook);
    }

    function executeSwap(V4SingleSwap executor, PoolKey memory key, bool zeroForOne, bool exactIn,
                         BalanceDelta expected, bool tightLimit) private returns (BalanceDelta) {
        uint128 paid = uint128(-(zeroForOne ? expected.amount0() : expected.amount1()));
        uint128 received = uint128(zeroForOne ? expected.amount1() : expected.amount0());
        Currency input = zeroForOne ? key.currency0 : key.currency1;
        uint128 inputLimit = exactIn ? paid : (tightLimit ? paid - 1 : paid + 1000);
        uint128 outputLimit = received + (exactIn && tightLimit ? 1 : 0);
        uint256 value = input.isAddressZero() ? inputLimit : 0;
        SettlementToken token = SettlementToken(Currency.unwrap(input));
        if (!input.isAddressZero()) token.approve(address(executor), inputLimit);
        (uint128 actualIn, uint128 actualOut) = executor.execute{value: value}(
            V4SingleSwap.Order(key, zeroForOne, !exactIn, inputLimit, outputLimit, RECIPIENT, block.timestamp)
        );
        require(address(executor).balance == 0, "no retained native funding");
        if (!input.isAddressZero()) {
            require(token.allowance(address(this), address(executor)) == inputLimit - actualIn, "unused ERC20 allowance");
        }
        return zeroForOne ? toBalanceDelta(-int128(actualIn), int128(actualOut))
                          : toBalanceDelta(int128(actualOut), -int128(actualIn));
    }

    function balances(PoolKey memory key, address owner) private view returns (uint256[2] memory) {
        return [key.currency0.balanceOf(owner), key.currency1.balanceOf(owner)];
    }

    function unlockCallback(bytes calldata data) external returns (bytes memory) {
        require(msg.sender == address(manager));
        (PoolKey memory key, int256 specified, bool zeroForOne, address receiver, bool omit) =
            abi.decode(data, (PoolKey, int256, bool, address, bool));
        BalanceDelta delta;
        if (specified == 0) {
            (delta,) = manager.modifyLiquidity(key, ModifyLiquidityParams(-120, 120, 1e12, 0), "");
        } else {
            delta = manager.swap(key, SwapParams(zeroForOne, specified, TickMath.getSqrtPriceAtTick(zeroForOne ? int24(-90) : int24(90))), abi.encode(omit));
        }
        require(manager.currencyDelta(address(this), key.currency0) == delta.amount0());
        require(manager.currencyDelta(address(this), key.currency1) == delta.amount1());
        settle(key.currency0, delta.amount0(), receiver);
        settle(key.currency1, delta.amount1(), receiver);
        return abi.encode(BalanceDelta.unwrap(delta));
    }

    function settle(Currency currency, int128 delta, address receiver) private {
        if (delta < 0) {
            uint256 amount = uint128(-delta);
            manager.sync(currency);
            if (currency.isAddressZero()) {
                manager.settle{value: amount}();
            } else {
                currency.transfer(address(manager), amount);
                manager.settle();
            }
        } else if (delta > 0) {
            manager.take(currency, receiver, uint128(delta));
        }
    }
}
