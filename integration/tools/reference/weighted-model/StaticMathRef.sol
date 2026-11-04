// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.7.0;
import "pkg/solidity-utils/contracts/math/LogExpMath.sol";
import "pkg/solidity-utils/contracts/math/FixedPoint.sol";
// Test-only visibility wrapper; the original library implementations are unchanged.
contract StaticMathRef {
    function rawPow(uint256 x, uint256 y) external pure returns(uint256) { return LogExpMath.pow(x,y); }
    function powUp(uint256 x, uint256 y) external pure returns(uint256) { return FixedPoint.powUp(x,y); }
}
