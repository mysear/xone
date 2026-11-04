// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.7.0;
pragma experimental ABIEncoderV2;
import "pkg/pool-stable/contracts/StableMath.sol";
// Calls original StableMath without rewriting its formulas. Callback fee/scale
// operations are a test wrapper; actual ComposableStablePool is NOT deployed.
contract ComposableMathRef {
    using FixedPoint for uint256;
    struct RegularParams {
        uint256[] raw; uint256 amp; uint256[] scales; uint256 fee;
        uint256 input; uint256 i; uint256 j; uint256 bpt;
    }
    struct BptParams {
        uint256[] raw; uint256 amp; uint256[] scales; uint256 fee;
        uint256 input; uint256 supply; uint256 index; bool tokenToBpt;
    }
    function regular(RegularParams memory p) external pure returns(uint256 out) {
        uint256[] memory balances=new uint256[](p.raw.length-1);
        for(uint256 k=0;k<p.raw.length;k++) if(k!=p.bpt) balances[k>p.bpt?k-1:k]=p.raw[k].mulDown(p.scales[k]);
        uint256 amount=p.input.sub(p.input.mulUp(p.fee)).mulDown(p.scales[p.i]);
        uint256 d=StableMath._calculateInvariant(p.amp,balances);
        out=StableMath._calcOutGivenIn(p.amp,balances,p.i>p.bpt?p.i-1:p.i,p.j>p.bpt?p.j-1:p.j,amount,d).divDown(p.scales[p.j]);
    }
    function bpt(BptParams memory p) external pure returns(uint256 out,uint256 nextSupply) {
        uint256[] memory balances=new uint256[](p.raw.length);
        for(uint256 k=0;k<p.raw.length;k++) balances[k]=p.raw[k].mulDown(p.scales[k]);
        uint256 d=StableMath._calculateInvariant(p.amp,balances);
        if(p.tokenToBpt) {
            uint256[] memory amounts=new uint256[](p.raw.length);amounts[p.index]=p.input.mulDown(p.scales[p.index]);
            out=StableMath._calcBptOutGivenExactTokensIn(p.amp,balances,amounts,p.supply,d,p.fee);
            nextSupply=p.supply.add(out);
        } else {
            out=StableMath._calcTokenOutGivenExactBptIn(p.amp,balances,p.index,p.input,p.supply,d,p.fee).divDown(p.scales[p.index]);
            nextSupply=p.supply.sub(p.input);
        }
    }
}
