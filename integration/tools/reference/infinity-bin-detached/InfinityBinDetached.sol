// SPDX-License-Identifier: GPL-2.0-or-later
pragma solidity 0.8.26;
import {BinPool} from "infinity/src/pool-bin/libraries/BinPool.sol";
import {BinHelper} from "infinity/src/pool-bin/libraries/BinHelper.sol";
import {ProtocolFeeLibrary} from "infinity/src/libraries/ProtocolFeeLibrary.sol";
import {BinPosition} from "infinity/src/pool-bin/libraries/BinPosition.sol";
import {TreeMath} from "infinity/src/pool-bin/libraries/math/TreeMath.sol";
import {PackedUint128Math} from "infinity/src/pool-bin/libraries/math/PackedUint128Math.sol";
import {PriceHelper} from "infinity/src/pool-bin/libraries/PriceHelper.sol";
import {BinSlot0} from "infinity/src/pool-bin/types/BinSlot0.sol";
import {BalanceDelta,BalanceDeltaLibrary} from "infinity/src/types/BalanceDelta.sol";

/// Bounded original-library burn/swap evidence. This is never a Manager substitute.
contract InfinityBinDetached {
    using BinPool for BinPool.State;
    using BinHelper for bytes32;
    using ProtocolFeeLibrary for uint16;
    using BinPosition for mapping(bytes32 => BinPosition.Info);
    using BinPosition for BinPosition.Info;
    using PackedUint128Math for bytes32;
    using BalanceDeltaLibrary for BalanceDelta;
    BinPool.State private state;
    uint16 public step;
    address public owner;
    bytes32 public salt;
    struct Bin { uint24 id; uint128 reserve0; uint128 reserve1; uint256 shares; }
    struct SwapResult { bool ok; bytes reason; int128 delta0; int128 delta1; uint24 swapFee; uint128 protocolAmount; }
    function seed(uint24 activeId,uint16 binStep,uint24 lpFee,uint24 protocolFee,Bin[] calldata bins,address positionOwner,bytes32 positionSalt) external {
        require(state.slot0.activeId()==0,"seed once");
        require(binStep>=1 && binStep<=100 && lpFee<=100000,"profile");
        require((protocolFee&4095)<=4000 && protocolFee>>12<=4000,"protocol");
        state.initialize(activeId,protocolFee,lpFee);step=binStep;owner=positionOwner;salt=positionSalt;
        for(uint256 i;i<bins.length;++i){Bin calldata b=bins[i];require(i==0 || b.id>bins[i-1].id,"sorted IDs");require(b.shares>=1000,"minimum share");
            state.reserveOfBin[b.id]=PackedUint128Math.encode(b.reserve0,b.reserve1);state.shareOfBin[b.id]=b.shares;
            state.positions.get(owner,b.id,salt).addShare(b.shares-1000);
            (,state.level0)=TreeMath.add(state.level0,state.level1,state.level2,b.id);
        }
    }
    function burn(uint24 id,uint256 shares) external returns(int128 delta0,int128 delta1,bytes32 amount){
        uint256[] memory ids=new uint256[](1);ids[0]=id;uint256[] memory amounts=new uint256[](1);amounts[0]=shares;
        (BalanceDelta d,,bytes32[] memory out)=state.burn(BinPool.BurnParams(owner,ids,amounts,salt));
        return(d.amount0(),d.amount1(),out[0]);
    }
    function slot0() external view returns(bytes32){return BinSlot0.unwrap(state.slot0);}
    function bin(uint24 id) external view returns(uint128,uint128,uint256,uint256){return state.getBin(step,id);}
    function price(uint24 id) external view returns(uint256){return PriceHelper.getPriceFromId(id,step);}
    function positionKey(uint24 id) external view returns(bytes32){return BinPosition.calculatePositionKey(owner,id,salt);}
    function position(uint24 id) external view returns(uint256){return state.positions.get(owner,id,salt).share;}
    function raw(bytes32 slot) external view returns(bytes32 value){assembly{value:=sload(slot)}}
    function tree(uint24 id) external view returns(bytes32,bytes32,bytes32,bool){return(state.level0,state.level1[bytes32(uint256(id)>>16)],state.level2[bytes32(uint256(id)>>8)],TreeMath.contains(state.level2,id));}
    function next(bool zero,uint24 id) external view returns(uint24){return state.getNextNonEmptyBin(zero,id);}
    function fill(uint24 id,uint128 input,bool zero) external view returns(uint128 used,uint128 out,uint128 fee,uint128 protocol) {
        bytes32 reserves=state.reserveOfBin[id];if(reserves.isEmpty(!zero))return(0,0,0,0);
        uint24 packed=state.slot0.protocolFee();uint16 pf=zero?uint16(packed&4095):uint16(packed>>12);
        uint24 sf=pf.calculateSwapFee(state.slot0.lpFee());bytes32 amounts=zero?bytes32(uint256(input)):bytes32(uint256(input)<<128);
        (bytes32 ai,bytes32 ao,bytes32 fees)=reserves.getAmountsOut(sf,step,zero,id,amounts);
        return(ai.decode(zero),ao.decode(!zero),fees.decode(zero),fees.getProtocolFeeAmt(packed,sf).decode(zero));
    }
    function execute(uint128 input,bool zero) external returns(SwapResult memory r){
        require(msg.sender==address(this),"self");require(input>0 && input<uint128(1)<<127,"exact input");
        (BalanceDelta d,BinPool.SwapState memory s)=state.swap(BinPool.SwapParams(zero,step,0,-int128(input)));
        return SwapResult(true,"",d.amount0(),d.amount1(),s.swapFee,s.feeAmountToProtocol.decode(zero));
    }
    function attempt(uint128 input,bool zero) external returns(SwapResult memory r){try this.execute(input,zero) returns(SwapResult memory v){return v;}catch(bytes memory reason){r.reason=reason;}}
}
