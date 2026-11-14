// SPDX-License-Identifier: GPL-2.0-or-later
pragma solidity =0.7.6;
interface IPool {
 function token0() external view returns(address);
 function token1() external view returns(address);
 function mint(address,int24,int24,uint128,bytes calldata) external returns(uint256,uint256);
 function swap(address,bool,int256,uint160,bytes calldata) external returns(int256,int256);
}
interface IToken {function transfer(address,uint256) external returns(bool);}
contract ReferenceToken {
 mapping(address=>uint256) public balanceOf;
 function mint(address to,uint256 amount)external {require(balanceOf[to]+amount>=balanceOf[to]);balanceOf[to]+=amount;}
 function transfer(address to,uint256 amount)external returns(bool){require(balanceOf[msg.sender]>=amount);balanceOf[msg.sender]-=amount;require(balanceOf[to]+amount>=balanceOf[to]);balanceOf[to]+=amount;return true;}
}
// Controlled original-contract test funder. No production routing authority.
contract ReferenceCaller {
 address public owner=msg.sender;
 mapping(address=>bool) public trusted;
 address[] private pools;
 bool[] private zeros;
 uint256 private paid;
 bool private active;
 function register(address p)external {require(msg.sender==owner&&!active);trusted[p]=true;}
 function mintPosition(address p,int24 lower,int24 upper,uint128 liquidity)external returns(uint256 a,uint256 b){
  require(msg.sender==owner&&!active&&trusted[p]);active=true;pools.push(p);zeros.push(true);
  (a,b)=IPool(p).mint(address(this),lower,upper,liquidity,abi.encode(uint256(0)));delete pools;delete zeros;active=false;
 }
 function run(address[] calldata ps,bool[] calldata zs,address recipient,uint256 output,uint256 maxInput)external returns(uint256){
  require(msg.sender==owner&&!active&&ps.length>0&&ps.length==zs.length&&output>0&&output<=uint256(type(int256).max));
  for(uint256 i=0;i<ps.length;i++){require(trusted[ps[i]]);for(uint256 j=0;j<i;j++)require(ps[i]!=ps[j]);pools.push(ps[i]);zeros.push(zs[i]);}
  active=true;paid=0;uint256 last=ps.length-1;
  (int256 a,int256 b)=IPool(ps[last]).swap(recipient,zs[last],-int256(output),limit(zs[last]),abi.encode(last));
  require(uint256(-(zs[last]?b:a))==output,'FULL_OUTPUT');require(paid<=maxInput,'MAX_INPUT');
  uint256 result=paid;delete pools;delete zeros;active=false;return result;
 }
 function limit(bool zero)private pure returns(uint160){return zero?4295128740:1461446703485210103287273052203988822378723970341;}
 function pay(uint256 a,uint256 b,bytes memory data)private {
  uint256 index=abi.decode(data,(uint256));require(active&&index<pools.length&&msg.sender==pools[index]);
  if(a>0)require(IToken(IPool(msg.sender).token0()).transfer(msg.sender,a));
  if(b>0)require(IToken(IPool(msg.sender).token1()).transfer(msg.sender,b));
 }
 function swapCallback(int256 a,int256 b,bytes memory data)private {
  uint256 index=abi.decode(data,(uint256));require(active&&index<pools.length&&msg.sender==pools[index]);require(a>0||b>0);
  uint256 amount=uint256(a>0?a:b);
  if(index==0){require(paid+amount>=paid);paid+=amount;pay(a>0?uint256(a):0,b>0?uint256(b):0,data);}
  else {
   bool zero=zeros[index-1];(int256 x,int256 y)=IPool(pools[index-1]).swap(msg.sender,zero,-int256(amount),limit(zero),abi.encode(index-1));
   require(uint256(-(zero?y:x))==amount,'FULL_HOP');
  }
 }
 function uniswapV3MintCallback(uint256 a,uint256 b,bytes calldata d)external {pay(a,b,d);}
 function pancakeV3MintCallback(uint256 a,uint256 b,bytes calldata d)external {pay(a,b,d);}
 function uniswapV3SwapCallback(int256 a,int256 b,bytes calldata d)external {swapCallback(a,b,d);}
 function pancakeV3SwapCallback(int256 a,int256 b,bytes calldata d)external {swapCallback(a,b,d);}
}
