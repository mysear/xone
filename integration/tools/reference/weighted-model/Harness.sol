// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.7.0;
pragma experimental ABIEncoderV2;
import "pkg/pool-weighted/contracts/WeightedPool.sol";
import "pkg/vault/contracts/balances/BalanceAllocation.sol";

// Test-only token and Vault facade. This executes the original unmodified
// WeightedPool constructor/onSwap and original packed Vault balance operations.
// It does not implement ERC20 transfers, the full Vault, joins/exits or deployment proof.
contract RefToken {
    uint8 public decimals;
    function setDecimals(uint8 d) external { decimals = d; }
}
contract WeightedVaultRef {
    using BalanceAllocation for bytes32;
    function registerPool(IVault.PoolSpecialization specialization) external view returns(bytes32) {
        return bytes32((uint256(uint160(msg.sender)) << 96) | (uint256(specialization) << 80) | 1);
    }
    function registerTokens(bytes32, IERC20[] calldata, address[] calldata) external pure {}
    function getProtocolFeesCollector() external view returns(address) { return address(this); }
    function quote(WeightedPool pool, uint256[2] memory raw, IERC20[2] memory tokens, uint256 input, bool zero)
        external returns(uint256 out, uint256[2] memory next)
    {
        uint256 i=zero?0:1; uint256 j=1-i;
        bytes32[2] memory balances=[BalanceAllocation.toBalance(raw[0],0,block.number),BalanceAllocation.toBalance(raw[1],0,block.number)];
        IPoolSwapStructs.SwapRequest memory request=IPoolSwapStructs.SwapRequest(IVault.SwapKind.GIVEN_IN,tokens[i],tokens[j],input,pool.getPoolId(),block.number,address(this),address(this),"");
        out=pool.onSwap(request,raw[i],raw[j]);
        balances[i]=balances[i].increaseCash(input);
        balances[j]=balances[j].decreaseCash(out);
        next[0]=balances[0].cash(); next[1]=balances[1].cash();
    }
}
