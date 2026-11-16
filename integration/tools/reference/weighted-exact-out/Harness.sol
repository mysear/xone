// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.7.0;
pragma experimental ABIEncoderV2;
import "pkg/pool-weighted/contracts/WeightedPool.sol";
import "pkg/vault/contracts/balances/BalanceAllocation.sol";

contract OutputTokenRef {
    uint8 public decimals;
    constructor(uint8 d) { decimals = d; }
}
// Test callback facade: original onSwap and packed cash accounting only.
// This does not implement a full Vault or transfer ERC20 tokens.
contract OutputVaultRef {
    using BalanceAllocation for bytes32;
    function registerPool(IVault.PoolSpecialization specialization) external view returns(bytes32) {
        return bytes32((uint256(uint160(msg.sender)) << 96) | (uint256(specialization) << 80) | 1);
    }
    function registerTokens(bytes32, IERC20[] calldata, address[] calldata) external pure {}
    function getProtocolFeesCollector() external view returns(address) { return address(this); }
    function quote(WeightedPool pool, uint256[] memory raw, IERC20[] memory tokens,
                   uint256 output, uint256 i, uint256 j)
        external returns(uint256 input, uint256[] memory next) {
        bytes32[] memory packed = new bytes32[](raw.length);
        for (uint256 k=0; k<raw.length; k++) packed[k]=BalanceAllocation.toBalance(raw[k],0,block.number);
        IPoolSwapStructs.SwapRequest memory request=IPoolSwapStructs.SwapRequest(
            IVault.SwapKind.GIVEN_OUT,tokens[i],tokens[j],output,pool.getPoolId(),block.number,address(this),address(this),"");
        input=pool.onSwap(request,raw[i],raw[j]);
        packed[i]=packed[i].increaseCash(input); packed[j]=packed[j].decreaseCash(output);
        next=new uint256[](raw.length);
        for(uint256 k=0;k<raw.length;k++) next[k]=packed[k].cash();
    }
}
