// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity ^0.7.0;
pragma experimental ABIEncoderV2;
import "pkg/pool-weighted/contracts/smart/ManagedPool.sol";
import "pkg/pool-weighted/contracts/lib/WeightCompression.sol";
import "pkg/pool-weighted/contracts/lib/GradualValueChange.sol";
// The actual ManagedPool contract is compiled and deployed separately.
// This exposes untouched upstream weight libraries for decoding/interpolation vectors.
contract ManagedWeightsRef {
    using WeightCompression for uint256;
    using FixedPoint for uint256;
    function weights(uint64[] memory starts,uint64[] memory ends,uint256 denormSum,
                     uint32 startTime,uint32 endTime) external view returns(uint256[] memory out) {
        out=new uint256[](starts.length);
        for(uint256 i=0;i<starts.length;i++) {
            uint256 a=uint256(starts[i]).decompress(64,1e22);
            uint256 b=uint256(ends[i]).decompress(64,1e22);
            out[i]=GradualValueChange.getInterpolatedValue(a,b,startTime,endTime).divDown(denormSum);
        }
    }
}
