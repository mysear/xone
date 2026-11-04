// SPDX-License-Identifier: GPL-2.0-or-later
pragma solidity 0.8.26;
import {BinPool} from "infinity/src/pool-bin/libraries/BinPool.sol";
import {TreeMath} from "infinity/src/pool-bin/libraries/math/TreeMath.sol";
import {BinHelper} from "infinity/src/pool-bin/libraries/BinHelper.sol";
import {PackedUint128Math} from "infinity/src/pool-bin/libraries/math/PackedUint128Math.sol";
import {PriceHelper} from "infinity/src/pool-bin/libraries/PriceHelper.sol";
import {BinSlot0} from "infinity/src/pool-bin/types/BinSlot0.sol";
import {BalanceDelta, BalanceDeltaLibrary} from "infinity/src/types/BalanceDelta.sol";
import {ProtocolFeeLibrary} from "infinity/src/libraries/ProtocolFeeLibrary.sol";

/// Executes original BinPool/TreeMath on isolated fixtures. Never a manager identity.
contract InfinityBinServiceMath {
    using BinPool for BinPool.State;
    using BinHelper for bytes32;
    using PackedUint128Math for bytes32;
    using BalanceDeltaLibrary for BalanceDelta;
    using ProtocolFeeLibrary for uint16;
    BinPool.State private state;
    uint16 public step;
    struct Bin { uint24 id; uint128 reserve0; uint128 reserve1; uint256 shares; }
    struct Result { int128 delta0; int128 delta1; uint24 swapFee; uint128 protocolAmount; }
    function seed(uint24 activeId, uint16 binStep, uint24 lpFee, uint24 protocolFee, Bin[] calldata bins) external {
        require(state.slot0.activeId() == 0, "seed once");
        require(binStep >= 1 && binStep <= 100 && lpFee <= 100000, "profile");
        require((protocolFee & 4095) <= 4000 && (protocolFee >> 12) <= 4000, "protocol");
        state.initialize(activeId, protocolFee, lpFee); step = binStep;
        for (uint256 i; i < bins.length; ++i) {
            Bin calldata b = bins[i];
            require(i == 0 || b.id > bins[i-1].id, "sorted ids");
            require(b.shares > 0, "initialized shares");
            state.reserveOfBin[b.id] = bytes32(uint256(b.reserve0) | uint256(b.reserve1) << 128);
            state.shareOfBin[b.id] = b.shares;
            (,state.level0) = TreeMath.add(state.level0, state.level1, state.level2, b.id);
        }
    }
    function raw(bytes32 slot) external view returns (bytes32 value) { assembly { value := sload(slot) } }
    function slot0() external view returns (bytes32) { return BinSlot0.unwrap(state.slot0); }
    function bin(uint24 id) external view returns (uint128,uint128,uint256,uint256) { return state.getBin(step,id); }
    function tree(uint24 id) external view returns (bytes32,bytes32,bytes32,bool) {
        return (state.level0,state.level1[bytes32(uint256(id)>>16)],state.level2[bytes32(uint256(id)>>8)],TreeMath.contains(state.level2,id));
    }
    function next(bool zero,uint24 id) external view returns (uint24) { return state.getNextNonEmptyBin(zero,id); }
    function price(uint24 id) external view returns (uint256) { return PriceHelper.getPriceFromId(id,step); }
    function fill(uint24 id,uint128 left,bool zero) external view returns (uint128 used,uint128 out,uint128 fee,uint128 protocol,uint256 p) {
        p = PriceHelper.getPriceFromId(id,step);
        bytes32 reserves = state.reserveOfBin[id];
        if (reserves.isEmpty(!zero)) return (0,0,0,0,p);
        uint24 packed = state.slot0.protocolFee();
        uint16 pf = zero ? uint16(packed & 4095) : uint16(packed >> 12);
        uint24 sf = pf.calculateSwapFee(state.slot0.lpFee());
        bytes32 amounts = zero ? bytes32(uint256(left)) : bytes32(uint256(left) << 128);
        (bytes32 ai,bytes32 ao,bytes32 fees) = reserves.getAmountsOut(sf,step,zero,id,amounts);
        used = ai.decode(zero); out = ao.decode(!zero); fee = fees.decode(zero);
        protocol = fees.getProtocolFeeAmt(packed,sf).decode(zero);
    }
    function swap(uint128 amount,bool zero) external returns (Result memory r) {
        require(amount > 0 && amount < uint128(1)<<127, "signed input");
        (BalanceDelta d,BinPool.SwapState memory s) = state.swap(BinPool.SwapParams(zero,step,0,-int128(amount)));
        return Result(d.amount0(),d.amount1(),s.swapFee,s.feeAmountToProtocol.decode(zero));
    }
}

/// Metadata-only ERC20 fixture. No token transfers or Vault settlement are claimed.
contract InfinityBinServiceCurrency {
    uint8 public constant decimals = 18;
    string public constant symbol = "IBS";
    function balanceOf(address) external pure returns (uint256) { return 0; }
}
