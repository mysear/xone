// Nominal exact-transfer ERC20 reference dependency. No callback/rebase/fee.
pragma solidity 0.8.19;
contract Tokens {
    uint8 public immutable decimals;
    uint256 public totalSupply;
    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;
    event Transfer(address indexed from, address indexed to, uint256 value);
    constructor(uint8 d) { decimals = d; }
    function mint(address to, uint256 value) external {totalSupply += value; balanceOf[to] += value; emit Transfer(address(0),to,value);}
    function approve(address to,uint256 value) external returns(bool) {allowance[msg.sender][to]=value; return true;}
    function transfer(address to,uint256 value) external returns(bool) {_transfer(msg.sender,to,value);return true;}
    function transferFrom(address from,address to,uint256 value) external returns(bool) {
        uint256 a=allowance[from][msg.sender]; if(a!=type(uint256).max) allowance[from][msg.sender]=a-value;
        _transfer(from,to,value);return true;
    }
    function _transfer(address from,address to,uint256 value) internal {balanceOf[from]-=value;balanceOf[to]+=value;emit Transfer(from,to,value);}
}
