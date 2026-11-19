// SPDX-License-Identifier: MIT
pragma solidity 0.8.26;

interface SplitToken {
    function balanceOf(address) external view returns (uint256);
    function transferFrom(address, address, uint256) external returns (bool);
    function approve(address, uint256) external returns (bool);
}

interface SplitRouter02 {
    function getAmountsIn(uint256, address[] calldata) external view returns (uint256[] memory);
    function swapExactTokensForTokens(uint256, uint256, address[] calldata, address, uint256)
        external returns (uint256[] memory);
    function swapTokensForExactTokens(uint256, uint256, address[] calldata, address, uint256)
        external returns (uint256[] memory);
}

/// Local execution profile for standard ERC20s only. No arbitrary calls, native
/// currency, delegated payer, retained change or selectable downstream router.
contract OrderedV2Split {
    SplitRouter02 public immutable router;

    struct Leg {
        uint256 amount; // Exact input, or exact output, according to the plan kind.
        address[] path;
    }

    error InvalidPlan();
    error AmountMismatch();

    constructor(SplitRouter02 router_) {
        if (address(router_).code.length == 0) revert InvalidPlan();
        router = router_;
    }

    function execute(bool exactOut, address recipient, uint256 input, uint256 output,
                     uint256 deadline, Leg[] calldata legs) external {
        if (legs.length < 2 || legs.length > 8 || legs[0].path.length < 2 ||
            recipient == address(0) || recipient == address(this) || input == 0 ||
            (exactOut && output == 0) || block.timestamp > deadline) revert InvalidPlan();
        address sell = legs[0].path[0];
        address buy = legs[0].path[legs[0].path.length - 1];
        if (sell == buy) revert InvalidPlan();
        uint256 callerBefore = SplitToken(sell).balanceOf(msg.sender);
        uint256 recipientBefore = SplitToken(buy).balanceOf(recipient);
        uint256 executorBefore = SplitToken(sell).balanceOf(address(this));
        uint256 spent;
        for (uint256 i; i < legs.length; ++i) {
            Leg calldata leg = legs[i];
            if (leg.amount == 0 || leg.path.length < 2 || leg.path.length > 9 ||
                leg.path[0] != sell || leg.path[leg.path.length - 1] != buy) revert InvalidPlan();
            // Query after preceding swaps, so a shared pool uses its updated reserves.
            uint256 pull = exactOut ? router.getAmountsIn(leg.amount, leg.path)[0] : leg.amount;
            if (pull > input - spent) revert AmountMismatch();
            spent += pull;
            if (!SplitToken(sell).transferFrom(msg.sender, address(this), pull) ||
                !SplitToken(sell).approve(address(router), pull)) revert AmountMismatch();
            if (exactOut) router.swapTokensForExactTokens(leg.amount, pull, leg.path, recipient, deadline);
            else router.swapExactTokensForTokens(pull, 0, leg.path, recipient, deadline);
        }
        uint256 received = SplitToken(buy).balanceOf(recipient) - recipientBefore;
        if (callerBefore - SplitToken(sell).balanceOf(msg.sender) != spent ||
            SplitToken(sell).balanceOf(address(this)) != executorBefore ||
            (exactOut ? received != output : (spent != input || received < output))) revert AmountMismatch();
    }
}
