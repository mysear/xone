// SPDX-License-Identifier: MIT
pragma solidity 0.8.26;

import {IBinHooks} from "infinity/src/pool-bin/interfaces/IBinHooks.sol";
import {IBinPoolManager} from "infinity/src/pool-bin/interfaces/IBinPoolManager.sol";
import {ICLHooks} from "infinity/src/pool-cl/interfaces/ICLHooks.sol";
import {ICLPoolManager} from "infinity/src/pool-cl/interfaces/ICLPoolManager.sol";
import {IHooks} from "infinity/src/interfaces/IHooks.sol";
import {ILockCallback} from "infinity/src/interfaces/ILockCallback.sol";
import {IPoolManager} from "infinity/src/interfaces/IPoolManager.sol";
import {IVault} from "infinity/src/interfaces/IVault.sol";
import {LPFeeLibrary} from "infinity/src/libraries/LPFeeLibrary.sol";
import {LiquidityConfigurations} from "infinity/src/pool-bin/libraries/math/LiquidityConfigurations.sol";
import {PackedUint128Math} from "infinity/src/pool-bin/libraries/math/PackedUint128Math.sol";
import {TickMath} from "infinity/src/pool-cl/libraries/TickMath.sol";
import {BalanceDelta, BalanceDeltaLibrary} from "infinity/src/types/BalanceDelta.sol";
import {BeforeSwapDelta, toBeforeSwapDelta} from "infinity/src/types/BeforeSwapDelta.sol";
import {Currency, CurrencyLibrary} from "infinity/src/types/Currency.sol";
import {PoolKey} from "infinity/src/types/PoolKey.sol";
import {SettlementToken} from "V4ManagerSettlement.sol";

contract InfinitySettlementHook {
    uint16 public constant getHooksRegistrationBitmap = 0xcc0;
    IVault private immutable vault;
    IPoolManager private immutable manager;

    constructor(IVault vault_, IPoolManager manager_) {
        vault = vault_;
        manager = manager_;
    }

    receive() external payable {}

    function beforeSwap(address, PoolKey calldata key, ICLPoolManager.SwapParams calldata params, bytes calldata data)
        external returns (bytes4, BeforeSwapDelta, uint24)
    {
        return (ICLHooks.beforeSwap.selector, before(key, (params.amountSpecified < 0) == params.zeroForOne, data),
                LPFeeLibrary.OVERRIDE_FEE_FLAG | 17_000);
    }

    function beforeSwap(address, PoolKey calldata key, bool zero, int128 amount, bytes calldata data)
        external returns (bytes4, BeforeSwapDelta, uint24)
    {
        return (IBinHooks.beforeSwap.selector, before(key, (amount < 0) == zero, data),
                LPFeeLibrary.OVERRIDE_FEE_FLAG | 17_000);
    }

    function afterSwap(address, PoolKey calldata key, ICLPoolManager.SwapParams calldata params, BalanceDelta, bytes calldata)
        external returns (bytes4, int128)
    {
        return (ICLHooks.afterSwap.selector, afterSwapDelta(key, (params.amountSpecified < 0) == params.zeroForOne));
    }

    function afterSwap(address, PoolKey calldata key, bool zero, int128 amount, BalanceDelta, bytes calldata)
        external returns (bytes4, int128)
    {
        return (IBinHooks.afterSwap.selector, afterSwapDelta(key, (amount < 0) == zero));
    }

    function before(PoolKey calldata key, bool specifiedIs0, bytes calldata data) private returns (BeforeSwapDelta) {
        require(msg.sender == address(manager));
        if (!abi.decode(data, (bool))) vault.take(specifiedIs0 ? key.currency0 : key.currency1, address(this), 7);
        return toBeforeSwapDelta(7, 0);
    }

    function afterSwapDelta(PoolKey calldata key, bool specifiedIs0) private returns (int128) {
        require(msg.sender == address(manager));
        vault.take(specifiedIs0 ? key.currency1 : key.currency0, address(this), 11);
        return 11;
    }
}

contract InfinityManagerSettlement is ILockCallback {
    using BalanceDeltaLibrary for BalanceDelta;
    using CurrencyLibrary for Currency;
    using PackedUint128Math for uint128;

    IVault private immutable vault;
    address private constant RECIPIENT = address(0x3456);
    uint24 private constant ACTIVE_BIN = 1 << 23;
    int128 private constant AMOUNT = 1_000_000;

    constructor(IVault vault_) {
        vault = vault_;
    }

    receive() external payable {}

    function run(IPoolManager manager, bool bin, bool native, bool zero, bool exactIn, bool omit)
        external payable returns (address, address, address, int128, int128, uint160, int24)
    {
        address token0 = native ? address(0) : address(new SettlementToken());
        address token1 = address(new SettlementToken());
        if (token0 > token1) (token0, token1) = (token1, token0);
        IHooks hook = IHooks(address(new InfinitySettlementHook(vault, manager)));
        uint256 spacing = bin ? 25 : 60;
        PoolKey memory key = PoolKey(Currency.wrap(token0), Currency.wrap(token1), hook, manager,
                                    LPFeeLibrary.DYNAMIC_FEE_FLAG, bytes32((spacing << 16) | 0xcc0));
        PoolKey memory control = PoolKey(key.currency0, key.currency1, IHooks(address(0)), manager,
                                        17_000, bytes32(spacing << 16));
        initialize(key, bin);
        initialize(control, bin);
        lock(key, bin, 0, zero, address(this), false);
        lock(control, bin, 0, zero, address(this), false);

        int128 specified = exactIn ? -AMOUNT : AMOUNT;
        BalanceDelta core = lock(control, bin, specified + 7, zero, address(this), false);
        Currency[2] memory currencies = [key.currency0, key.currency1];
        uint256[2] memory vaultBefore = balances(currencies, address(vault));
        uint256[2] memory payerBefore = balances(currencies, address(this));
        uint256[2] memory hookBefore = balances(currencies, address(hook));
        uint256[2] memory receiverBefore = balances(currencies, RECIPIENT);
        uint256[2] memory appBefore = [vault.reservesOfApp(address(manager), currencies[0]),
                                       vault.reservesOfApp(address(manager), currencies[1])];
        BalanceDelta swap = lock(key, bin, specified, zero, RECIPIENT, omit);
        int128[2] memory hookFees = exactIn == zero ? [int128(7), int128(11)] : [int128(11), int128(7)];
        require((exactIn ? -(zero ? swap.amount0() : swap.amount1()) : (zero ? swap.amount1() : swap.amount0())) == AMOUNT);
        for (uint256 i; i < 2; ++i) {
            Currency currency = currencies[i];
            int128 net = i == 0 ? swap.amount0() : swap.amount1();
            int128 poolDelta = i == 0 ? core.amount0() : core.amount1();
            require(net == poolDelta - hookFees[i], "caller vs app delta");
            require(currency.balanceOf(address(hook)) == hookBefore[i] + uint128(hookFees[i]), "hook balance");
            require(int256(currency.balanceOf(address(vault))) + poolDelta == int256(vaultBefore[i]), "vault balance");
            require(int256(vault.reservesOfApp(address(manager), currency)) + poolDelta == int256(appBefore[i]), "app reserves");
            require(int256(currency.balanceOf(address(this))) == int256(payerBefore[i]) + (net < 0 ? net : int128(0)), "payer balance");
            require(currency.balanceOf(RECIPIENT) == receiverBefore[i] + (net > 0 ? uint128(net) : 0), "receiver balance");
        }
        (uint160 state, int24 tick) = checkState(key, control, bin);
        require(vault.getLocker() == address(0) && vault.getUnsettledDeltasCount() == 0 && vault.getAppDeficitCount() == 0,
                "final Vault transient state");
        return (token0, token1, address(hook), swap.amount0(), swap.amount1(), state, tick);
    }

    function initialize(PoolKey memory key, bool bin) private {
        if (bin) IBinPoolManager(address(key.poolManager)).initialize(key, ACTIVE_BIN);
        else ICLPoolManager(address(key.poolManager)).initialize(key, uint160(1 << 96));
    }

    function checkState(PoolKey memory key, PoolKey memory control, bool bin) private view returns (uint160, int24) {
        if (bin) {
            IBinPoolManager manager = IBinPoolManager(address(key.poolManager));
            (uint24 id,, uint24 lpFee) = manager.getSlot0(key.toId());
            (uint24 controlId,,) = manager.getSlot0(control.toId());
            require(id == controlId && lpFee == 0, "bin slot0");
            (uint128 x, uint128 y,,) = manager.getBin(key.toId(), id);
            (uint128 xControl, uint128 yControl,,) = manager.getBin(control.toId(), id);
            require(x == xControl && y == yControl, "bin reserves");
            return (uint160(id), 0);
        }
        ICLPoolManager manager = ICLPoolManager(address(key.poolManager));
        (uint160 price, int24 tick,, uint24 lpFee) = manager.getSlot0(key.toId());
        (uint160 controlPrice, int24 controlTick,,) = manager.getSlot0(control.toId());
        require(price != uint160(1 << 96) && price == controlPrice && tick == controlTick && lpFee == 0, "CL slot0");
        return (price, tick);
    }

    function lock(PoolKey memory key, bool bin, int128 specified, bool zero, address recipient, bool omit)
        private returns (BalanceDelta)
    {
        return BalanceDelta.wrap(abi.decode(vault.lock(abi.encode(key, bin, specified, zero, recipient, omit)), (int256)));
    }

    function balances(Currency[2] memory currencies, address owner) private view returns (uint256[2] memory) {
        return [currencies[0].balanceOf(owner), currencies[1].balanceOf(owner)];
    }

    function lockAcquired(bytes calldata data) external returns (bytes memory) {
        require(msg.sender == address(vault));
        (PoolKey memory key, bool bin, int128 specified, bool zero, address recipient, bool omit) =
            abi.decode(data, (PoolKey, bool, int128, bool, address, bool));
        BalanceDelta delta;
        if (bin) {
            IBinPoolManager manager = IBinPoolManager(address(key.poolManager));
            if (specified == 0) {
                bytes32[] memory config = new bytes32[](1);
                config[0] = LiquidityConfigurations.encodeParams(1e18, 1e18, ACTIVE_BIN);
                (delta,) = manager.mint(key, IBinPoolManager.MintParams(config, uint128(1e12).encode(1e12), 0), "");
            } else {
                delta = manager.swap(key, zero, specified, abi.encode(omit));
            }
        } else {
            ICLPoolManager manager = ICLPoolManager(address(key.poolManager));
            if (specified == 0) {
                (delta,) = manager.modifyLiquidity(key, ICLPoolManager.ModifyLiquidityParams(-120, 120, 1e12, 0), "");
            } else {
                delta = manager.swap(key, ICLPoolManager.SwapParams(zero, specified,
                    TickMath.getSqrtRatioAtTick(zero ? int24(-90) : int24(90))), abi.encode(omit));
            }
        }
        settle(key.currency0, delta.amount0(), recipient);
        settle(key.currency1, delta.amount1(), recipient);
        return abi.encode(BalanceDelta.unwrap(delta));
    }

    function settle(Currency currency, int128 delta, address recipient) private {
        require(vault.currencyDelta(address(this), currency) == delta, "locker delta");
        if (delta < 0) {
            uint256 amount = uint128(-delta);
            vault.sync(currency);
            if (currency.isNative()) require(vault.settle{value: amount}() == amount);
            else {
                currency.transfer(address(vault), amount);
                require(vault.settle() == amount);
            }
        } else if (delta > 0) {
            vault.take(currency, recipient, uint128(delta));
        }
    }
}
