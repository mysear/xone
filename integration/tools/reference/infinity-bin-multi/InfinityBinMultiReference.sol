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

/// Library execution reference only. No vault, manager callbacks or settlement.
contract InfinityBinMultiReference {
    using BinPool for BinPool.State;
    using BinHelper for bytes32;
    using PackedUint128Math for bytes32;
    using BalanceDeltaLibrary for BalanceDelta;
    using ProtocolFeeLibrary for uint16;
    BinPool.State private state;
    uint24[] private ids;
    uint16 private step;
    uint128 private totalProtocol0;
    uint128 private totalProtocol1;
    struct Bin { uint24 id; uint128 reserve0; uint128 reserve1; }
    struct Seed { uint24 activeId; uint16 binStep; uint24 lpFee; uint24 protocolFee; Bin[] bins; }
    struct Snapshot { uint24 activeId; Bin[] bins; bool[] membership; uint128 protocol0; uint128 protocol1; }
    struct Result { bool ok; bytes reason; int128 delta0; int128 delta1; uint24 swapFee; uint128 protocolAmount; Snapshot post; }
    function seed(Seed calldata s) external {
        for (uint256 i; i < ids.length; ++i) {
            (,state.level0) = TreeMath.remove(state.level0,state.level1,state.level2,ids[i]);
            delete state.reserveOfBin[ids[i]];
        }
        delete ids;
        state.slot0 = BinSlot0.wrap(bytes32(0));
        state.initialize(s.activeId,s.protocolFee,s.lpFee);
        step = s.binStep; totalProtocol0 = 0; totalProtocol1 = 0;
        for (uint256 i; i < s.bins.length; ++i) {
            Bin calldata b = s.bins[i];
            if (i > 0) require(b.id > s.bins[i-1].id,"sorted unique ids");
            ids.push(b.id);
            state.reserveOfBin[b.id] = bytes32(uint256(b.reserve0) | uint256(b.reserve1) << 128);
            (,state.level0) = TreeMath.add(state.level0,state.level1,state.level2,b.id);
        }
    }
    function snapshot() public view returns (Snapshot memory s) {
        s.activeId = state.slot0.activeId(); s.bins = new Bin[](ids.length);
        s.membership = new bool[](ids.length); s.protocol0 = totalProtocol0; s.protocol1 = totalProtocol1;
        for (uint256 i; i < ids.length; ++i) {
            (uint128 x,uint128 y) = state.reserveOfBin[ids[i]].decode();
            s.bins[i] = Bin(ids[i],x,y); s.membership[i] = TreeMath.contains(state.level2,ids[i]);
        }
    }
    function next(bool zero,uint24 id) external view returns (uint24) {
        return state.getNextNonEmptyBin(zero,id);
    }
    // Per-bin evidence comes directly from original helper calls. Full swap is
    // independently executed below, including its reserve/cap/delta checks.
    function fill(uint24 id,uint128 left,bool zero) external view returns
        (uint128 used,uint128 out,uint128 fee,uint128 protocol,uint256 price) {
        bytes32 reserves = state.reserveOfBin[id];
        if (reserves.isEmpty(!zero)) return (0,0,0,0,0);
        uint24 packed = state.slot0.protocolFee();
        uint16 pf = zero ? uint16(packed & 0xfff) : uint16(packed >> 12);
        uint24 sf = pf.calculateSwapFee(state.slot0.lpFee());
        bytes32 amounts = zero ? bytes32(uint256(left)) : bytes32(uint256(left) << 128);
        (bytes32 ai,bytes32 ao,bytes32 fees) = reserves.getAmountsOut(sf,step,zero,id,amounts);
        used = ai.decode(zero); out = ao.decode(!zero); fee = fees.decode(zero);
        protocol = fees.getProtocolFeeAmt(packed,sf).decode(zero);
        price = PriceHelper.getPriceFromId(id,step);
    }
    function execute(uint128 input,bool zero) external returns (Result memory r) {
        require(msg.sender == address(this),"self");
        require(input > 0 && input <= uint128(1)<<127,"signed input domain");
        int128 specified;
        unchecked { specified = -int128(input); }
        (BalanceDelta d,BinPool.SwapState memory s) = state.swap(BinPool.SwapParams(zero,step,0,specified));
        r.ok = true; r.delta0 = d.amount0(); r.delta1 = d.amount1(); r.swapFee = s.swapFee;
        r.protocolAmount = s.feeAmountToProtocol.decode(zero);
        if (zero) totalProtocol0 += r.protocolAmount; else totalProtocol1 += r.protocolAmount;
        r.post = snapshot();
    }
    // A caught external revert lets a transaction prove full atomic rollback.
    function attempt(uint128 input,bool zero) external returns (Result memory r) {
        try this.execute(input,zero) returns (Result memory result) { return result; }
        catch (bytes memory reason) { r.reason = reason; r.post = snapshot(); }
    }
}
