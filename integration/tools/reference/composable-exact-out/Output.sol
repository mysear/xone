// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity 0.7.6; pragma experimental ABIEncoderV2; import 'pkg/pool-stable/contracts/StableMath.sol'; contract OutputRef {
using FixedPoint for uint256;
struct P {uint256[] raw;uint256 amp;uint256[] scales;uint256 fee;uint256 output;uint256 i;uint256 j;uint256 bpt;}
function quote(P memory p) external view returns(uint256 input,uint256[] memory next){require(p.i!=p.bpt&&p.j!=p.bpt,'BPT');uint256[] memory b=new uint256[](p.raw.length-1);for(uint256 k;k<p.raw.length;k++)if(k!=p.bpt)b[k>p.bpt?k-1:k]=p.raw[k].mulDown(p.scales[k]);
uint256 d=StableMath._calculateInvariant(p.amp,b);input=StableMath._calcInGivenOut(p.amp,b,p.i>p.bpt?p.i-1:p.i,p.j>p.bpt?p.j-1:p.j,p.output.mulDown(p.scales[p.j]),d).divUp(p.scales[p.i]).divUp(FixedPoint.ONE.sub(p.fee));
next=new uint256[](p.raw.length);for(uint256 k;k<p.raw.length;k++){require(p.raw[k]<(uint256(1)<<112),'raw cash');uint256 balance=p.raw[k];if(k==p.i)balance=balance.add(input);if(k==p.j)balance=balance.sub(p.output);require(balance<(uint256(1)<<112),'final cash');next[k]=balance;}}
}
