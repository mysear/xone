// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.7.0;
pragma experimental ABIEncoderV2;
import "pkg/pool-weighted/contracts/smart/ManagedPool.sol";
import "pkg/vault/contracts/balances/BalanceAllocation.sol";

contract ManagedFeesToken { uint8 public decimals; constructor(uint8 d) { decimals=d; } }
// The pool is the separately deployed, unmodified original ManagedPool.
// This Vault facade stores full raw cash and makes original join/swap callbacks.
contract ManagedFeesVault {
    using BalanceAllocation for bytes32;
    mapping(bytes32=>IERC20[]) private tokens;
    mapping(bytes32=>uint256[]) private cash;
    address private collector;
    constructor(address c) { collector=c==address(0)?address(this):c; }
    function getAumFeePercentage() external pure returns(uint256) { return 0; }
    function getSwapFeePercentage() external pure returns(uint256) { return 0; }
    function getProtocolFeesCollector() external view returns(address) { return collector; }
    function registerPool(IVault.PoolSpecialization s) external view returns(bytes32) {
        return bytes32((uint256(uint160(msg.sender))<<96)|(uint256(s)<<80)|1);
    }
    function registerTokens(bytes32 id,IERC20[] calldata t,address[] calldata) external { tokens[id]=t; }
    function getPoolTokens(bytes32 id) external view returns(IERC20[] memory,uint256[] memory,uint256) { return(tokens[id],cash[id],block.number); }
    function initialize(ManagedPool pool,address recipient,uint256[] memory amounts) external {
        bytes32 id=pool.getPoolId();
        (uint256[] memory joined,)=pool.onJoinPool(id,recipient,recipient,new uint256[](amounts.length),block.number,0,abi.encode(uint256(0),amounts));
        cash[id]=joined;
    }
    // Caller supplies raw balances to exercise packed Vault bounds and rollback.
    function quote(ManagedPool pool,uint256[] memory raw,uint256 input,uint256 i,uint256 j)
        external returns(uint256 out,uint256[] memory next) {
        bytes32 id=pool.getPoolId(); IERC20[] memory t=tokens[id];
        bytes32[] memory packed=new bytes32[](raw.length);
        for(uint256 k=0;k<raw.length;k++) packed[k]=BalanceAllocation.toBalance(raw[k],0,block.number);
        IPoolSwapStructs.SwapRequest memory req=IPoolSwapStructs.SwapRequest(IVault.SwapKind.GIVEN_IN,t[i],t[j],input,id,block.number,msg.sender,msg.sender,"");
        out=pool.onSwap(req,raw[i],raw[j]);
        packed[i]=packed[i].increaseCash(input); packed[j]=packed[j].decreaseCash(out);
        next=new uint256[](raw.length);
        for(uint256 k=0;k<raw.length;k++) next[k]=packed[k].cash();
        cash[id]=next;
    }
}
// Independent observation of the exact selected internal scaled pre/post values.
// Original private fee payer runs inside ManagedPool; this observer never mints.
contract ManagedFeesObservation {
    using FixedPoint for uint256;
    function observe(uint256[8] memory a) external pure returns(uint256[9] memory v) {
        // a: raw in/out balances, weights, scales, fee, raw gross input.
        v[5]=a[0].mulDown(a[4]);v[6]=a[1].mulDown(a[5]);
        v[0]=(a[7]-a[7].mulUp(a[6])).mulDown(a[4]);
        v[1]=v[0].divUp(a[6].complement());
        v[2]=WeightedMath._calcOutGivenIn(v[5],a[2],v[6],a[3],v[0]);
        uint256[] memory w=new uint256[](2);w[0]=a[2];w[1]=a[3];
        uint256[] memory b=new uint256[](2);b[0]=v[5];b[1]=v[6];
        v[3]=WeightedMath._calculateInvariant(w,b);
        v[7]=v[5].add(v[1]);v[8]=v[6].sub(v[2]);b[0]=v[7];b[1]=v[8];
        v[4]=WeightedMath._calculateInvariant(w,b);
    }
}
