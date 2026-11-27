// SPDX-License-Identifier: MIT
pragma solidity 0.8.26;

import {SplitRouter02, SplitToken} from "./OrderedV2Split.sol";

/// Atomic fan-in for pinned Router02 routes over standard ERC20s.
contract SharedV2Suffix {
    SplitRouter02 public immutable router;
    bool private executing;

    struct Prefix {
        uint256 amountIn;
        address[] path;
    }

    error InvalidPlan();
    error AmountMismatch();

    constructor(SplitRouter02 router_) {
        if (address(router_).code.length == 0) revert InvalidPlan();
        router = router_;
    }

    function executeExactIn(
        address recipient,
        uint256 input,
        uint256 minOutput,
        uint256 deadline,
        Prefix[] calldata prefixes,
        address[] calldata suffix
    ) external {
        if (executing || prefixes.length < 2 || prefixes.length > 8 || input == 0 ||
            recipient == address(0) || recipient == address(this) || block.timestamp > deadline ||
            prefixes[0].path.length < 2 || suffix.length < 2) revert InvalidPlan();
        executing = true;

        address sell = prefixes[0].path[0];
        address join = suffix[0];
        address buy = suffix[suffix.length - 1];
        if (sell == join || buy == join || sell == buy) revert InvalidPlan();
        checkPath(suffix, sell);
        for (uint256 i; i < prefixes.length; ++i) {
            Prefix calldata prefix = prefixes[i];
            checkPath(prefix.path, buy);
            if (prefix.amountIn == 0 || prefix.path[0] != sell ||
                prefix.path[prefix.path.length - 1] != join ||
                prefix.path.length + suffix.length > 10) revert InvalidPlan();
            for (uint256 j; j + 1 < prefix.path.length; ++j) {
                for (uint256 k = 1; k + 1 < suffix.length; ++k) {
                    if (prefix.path[j] == suffix[k]) revert InvalidPlan();
                }
            }
        }

        uint256 callerBefore = SplitToken(sell).balanceOf(msg.sender);
        uint256 recipientBefore = SplitToken(buy).balanceOf(recipient);
        uint256 sellBefore = SplitToken(sell).balanceOf(address(this));
        uint256 joinBefore = SplitToken(join).balanceOf(address(this));
        uint256 unspent = input;
        for (uint256 i; i < prefixes.length; ++i) {
            Prefix calldata prefix = prefixes[i];
            if (prefix.amountIn > unspent) revert AmountMismatch();
            unspent -= prefix.amountIn;
            if (!SplitToken(sell).transferFrom(msg.sender, address(this), prefix.amountIn)) revert AmountMismatch();
            swap(prefix.amountIn, 0, prefix.path, address(this), deadline);
        }
        if (unspent != 0) revert AmountMismatch();
        uint256 joined = SplitToken(join).balanceOf(address(this)) - joinBefore;
        if (joined == 0) revert AmountMismatch();
        swap(joined, minOutput, suffix, recipient, deadline);
        if (callerBefore - SplitToken(sell).balanceOf(msg.sender) != input ||
            SplitToken(sell).balanceOf(address(this)) != sellBefore ||
            SplitToken(join).balanceOf(address(this)) != joinBefore ||
            SplitToken(buy).balanceOf(recipient) - recipientBefore < minOutput) revert AmountMismatch();

        executing = false;
    }

    function swap(uint256 input, uint256 minimum, address[] calldata path, address recipient, uint256 deadline)
        private
    {
        if (!SplitToken(path[0]).approve(address(router), input)) revert AmountMismatch();
        router.swapExactTokensForTokens(input, minimum, path, recipient, deadline);
    }

    function checkPath(address[] calldata path, address forbidden) private pure {
        if (path.length < 2 || path.length > 8) revert InvalidPlan();
        for (uint256 i; i < path.length; ++i) {
            if (path[i] == forbidden || path[i] == address(0)) revert InvalidPlan();
            for (uint256 j; j < i; ++j) {
                if (path[i] == path[j]) revert InvalidPlan();
            }
        }
    }
}
