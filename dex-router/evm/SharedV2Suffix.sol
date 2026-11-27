// SPDX-License-Identifier: MIT
pragma solidity 0.8.26;

import {SplitRouter02, SplitToken} from "./OrderedV2Split.sol";

/// Atomic fan-in for pinned Router02 routes over standard ERC20s.
contract SharedV2Suffix {
    SplitRouter02 public immutable router;
    bool private executing;

    struct Prefix {
        uint256 amount;
        address[] path;
    }

    struct Balances {
        uint256 caller;
        uint256 recipient;
        uint256 sell;
        uint256 join;
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
        execute(false, recipient, input, minOutput, deadline, prefixes, suffix);
    }

    function executeExactOut(
        address recipient,
        uint256 maxInput,
        uint256 output,
        uint256 deadline,
        Prefix[] calldata prefixes,
        address[] calldata suffix
    ) external {
        execute(true, recipient, maxInput, output, deadline, prefixes, suffix);
    }

    function execute(
        bool exactOut,
        address recipient,
        uint256 input,
        uint256 output,
        uint256 deadline,
        Prefix[] calldata prefixes,
        address[] calldata suffix
    ) private {
        if (executing || prefixes.length < 2 || prefixes.length > 8 || input == 0 ||
            (exactOut && output == 0) ||
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
            if (prefix.amount == 0 || prefix.path[0] != sell ||
                prefix.path[prefix.path.length - 1] != join ||
                prefix.path.length + suffix.length > 10) revert InvalidPlan();
            for (uint256 j; j + 1 < prefix.path.length; ++j) {
                for (uint256 k = 1; k + 1 < suffix.length; ++k) {
                    if (prefix.path[j] == suffix[k]) revert InvalidPlan();
                }
            }
        }

        Balances memory before = Balances({
            caller: SplitToken(sell).balanceOf(msg.sender),
            recipient: SplitToken(buy).balanceOf(recipient),
            sell: SplitToken(sell).balanceOf(address(this)),
            join: SplitToken(join).balanceOf(address(this))
        });
        uint256 requiredJoin = exactOut ? router.getAmountsIn(output, suffix)[0] : 0;
        uint256 unspent = runPrefixes(exactOut, input, deadline, prefixes, sell);
        uint256 joined = SplitToken(join).balanceOf(address(this)) - before.join;
        if (exactOut ? joined != requiredJoin : unspent != 0 || joined == 0) revert AmountMismatch();
        swap(exactOut, joined, output, suffix, recipient, deadline);
        uint256 received = SplitToken(buy).balanceOf(recipient) - before.recipient;
        if (before.caller - SplitToken(sell).balanceOf(msg.sender) != input - unspent ||
            SplitToken(sell).balanceOf(address(this)) != before.sell ||
            SplitToken(join).balanceOf(address(this)) != before.join ||
            (exactOut ? received != output : received < output)) revert AmountMismatch();

        executing = false;
    }

    function runPrefixes(
        bool exactOut,
        uint256 input,
        uint256 deadline,
        Prefix[] calldata prefixes,
        address sell
    ) private returns (uint256 unspent) {
        unspent = input;
        for (uint256 i; i < prefixes.length; ++i) {
            Prefix calldata prefix = prefixes[i];
            uint256 pull = exactOut ? router.getAmountsIn(prefix.amount, prefix.path)[0] : prefix.amount;
            if (pull > unspent) revert AmountMismatch();
            unspent -= pull;
            if (!SplitToken(sell).transferFrom(msg.sender, address(this), pull)) revert AmountMismatch();
            swap(exactOut, pull, exactOut ? prefix.amount : 0, prefix.path, address(this), deadline);
        }
    }

    function swap(
        bool exactOut,
        uint256 input,
        uint256 output,
        address[] calldata path,
        address recipient,
        uint256 deadline
    ) private {
        if (!SplitToken(path[0]).approve(address(router), input)) revert AmountMismatch();
        if (exactOut) router.swapTokensForExactTokens(output, input, path, recipient, deadline);
        else router.swapExactTokensForTokens(input, output, path, recipient, deadline);
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
