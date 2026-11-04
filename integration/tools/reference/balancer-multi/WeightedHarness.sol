// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.7.0;
pragma experimental ABIEncoderV2;
import "pkg/pool-weighted/contracts/WeightedPool.sol";
import "pkg/vault/contracts/balances/BalanceAllocation.sol";

contract MultiTokenRef {
    uint8 public decimals;
    constructor(uint8 d) { decimals = d; }
}
// A test Vault facade, original onSwap and original packed cash accounting.
// No ERC20 transfers, actual Vault, joins/exits, asset managers or deployment proof.
contract MultiVaultRef {
    mapping(bytes32=>IERC20[]) private registered;
    function getAumFeePercentage() external pure returns(uint256) { return 0; }
    function getSwapFeePercentage() external pure returns(uint256) { return 0; }
    function getPoolTokens(bytes32 id) external view returns(IERC20[] memory,uint256[] memory,uint256) { return (registered[id],new uint256[](registered[id].length),block.number); }
    using BalanceAllocation for bytes32;
    function registerPool(IVault.PoolSpecialization specialization) external view returns(bytes32) {
        return bytes32((uint256(uint160(msg.sender)) << 96) | (uint256(specialization) << 80) | 1);
    }
    function registerTokens(bytes32 id, IERC20[] calldata tokens, address[] calldata) external { registered[id]=tokens; }
    function getProtocolFeesCollector() external view returns(address) { return address(this); }
    function quote(IMinimalSwapInfoPool pool, bytes32 poolId, uint256[] memory raw,
                   IERC20[] memory tokens, uint256 input, uint256 i, uint256 j)
        external returns(uint256 out, uint256[] memory next) {
        bytes32[] memory packed = new bytes32[](raw.length);
        for(uint256 k=0;k<raw.length;k++) packed[k]=BalanceAllocation.toBalance(raw[k],0,block.number);
        IPoolSwapStructs.SwapRequest memory request=IPoolSwapStructs.SwapRequest(
            IVault.SwapKind.GIVEN_IN,tokens[i],tokens[j],input,poolId,block.number,address(this),address(this),"");
        out=pool.onSwap(request,raw[i],raw[j]);
        packed[i]=packed[i].increaseCash(input); packed[j]=packed[j].decreaseCash(out);
        next=new uint256[](raw.length);
        for(uint256 k=0;k<raw.length;k++) next[k]=packed[k].cash();
    }
}
