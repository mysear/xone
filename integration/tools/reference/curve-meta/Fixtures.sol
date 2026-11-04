// SPDX-License-Identifier: MIT
pragma solidity 0.8.19;

// Isolated standard ERC20 bookkeeping. Mint/configure are fixture setup only.
contract BookkeepingERC20 {
    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;
    uint256 public totalSupply;
    uint8 public decimals;
    bool private configured;
    event Transfer(address indexed from, address indexed to, uint256 amount);
    event Approval(address indexed owner, address indexed spender, uint256 amount);
    function configure(uint8 d) external { require(!configured); configured = true; decimals = d; }
    function mint(address to, uint256 amount) external {
        require(to != address(0)); totalSupply += amount; balanceOf[to] += amount;
        emit Transfer(address(0), to, amount);
    }
    function approve(address spender, uint256 amount) external returns (bool) {
        allowance[msg.sender][spender] = amount; emit Approval(msg.sender, spender, amount); return true;
    }
    function transfer(address to, uint256 amount) external returns (bool) {
        _transfer(msg.sender, to, amount); return true;
    }
    function transferFrom(address from, address to, uint256 amount) external returns (bool) {
        uint256 approved = allowance[from][msg.sender];
        require(approved >= amount, "allowance");
        if (approved != type(uint256).max) { allowance[from][msg.sender] = approved - amount; }
        _transfer(from, to, amount); return true;
    }
    function _transfer(address from, address to, uint256 amount) private {
        require(to != address(0), "zero receiver"); require(balanceOf[from] >= amount, "balance");
        balanceOf[from] -= amount; balanceOf[to] += amount; emit Transfer(from, to, amount);
    }
}

// Supplies one virtual price. No 3pool trading, deposit, or withdrawal executes.
contract SuppliedVirtualPrice {
    uint256 public value;
    function setVirtualPrice(uint256 v) external { require(v > 0); value = v; }
    function get_virtual_price() external view returns (uint256) { return value; }
}

contract CloneFactory {
    event Created(address indexed pool, address indexed implementation);
    function create(address implementation, bytes calldata initialization) external returns (address pool) {
        bytes memory init = abi.encodePacked(hex"3d602d80600a3d3981f3",
            hex"363d3d373d3d3d363d73", implementation, hex"5af43d82803e903d91602b57fd5bf3");
        assembly { pool := create(0, add(init, 32), mload(init)) }
        require(pool != address(0), "create");
        (bool ok, bytes memory reason) = pool.call(initialization);
        if (!ok) { assembly { revert(add(reason, 32), mload(reason)) } }
        emit Created(pool, implementation);
    }
}
