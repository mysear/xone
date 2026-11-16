// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.7.0;
pragma experimental ABIEncoderV2;
import "StableMath.sol";
import "pkg/vault/contracts/balances/BalanceAllocation.sol";

// A pure math/scaling wrapper and original Vault cash library. No actual Vault,
// StablePool deployment, ERC20 transfer, join/exit or transaction execution proof.
contract StableOutputRef {
    using FixedPoint for uint256;
    using BalanceAllocation for bytes32;
    function quote(uint256[2] memory raw, uint256 amp, uint256[2] memory scales,
                   uint256 fee, uint256 output, bool zero)
        external view returns(uint256 input, uint256[2] memory next) {
        uint256 i=zero?0:1; uint256 j=1-i;
        input=requiredInput(raw,amp,scales,output,zero).divUp(FixedPoint.ONE.sub(fee));
        bytes32[2] memory packed=[BalanceAllocation.toBalance(raw[0],0,block.number),
                                 BalanceAllocation.toBalance(raw[1],0,block.number)];
        packed[i]=packed[i].increaseCash(input); packed[j]=packed[j].decreaseCash(output);
        next[0]=packed[0].cash(); next[1]=packed[1].cash();
    }
    function requiredInput(uint256[2] memory raw,uint256 amp,uint256[2] memory scales,
                           uint256 output,bool zero) internal pure returns(uint256) {
        uint256 i=zero?0:1; uint256 j=1-i;
        uint256[] memory b=new uint256[](2);
        b[0]=raw[0].mulDown(scales[0]); b[1]=raw[1].mulDown(scales[1]);
        uint256 invariant=StableMath._calculateInvariant(amp,b);
        uint256 amount=output.mulDown(scales[j]);
        return StableMath._calcInGivenOut(amp,b,i,j,amount,invariant).divUp(scales[i]);
    }
}
