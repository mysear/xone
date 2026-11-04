'use strict';
const fs=require('node:fs'),path=require('node:path');
const ref=path.resolve(__dirname,'../reference/infinity-bin-service');
const receipt=path.join(ref,'deploy-receipt.json');fs.writeFileSync(receipt,JSON.stringify({passed:false,state:'RUNNING'})+'\n');
const {compile,sha,requireThat}=require('./infinity-bin-service-compile.cjs');
let raw;
(async()=>{try{
 const {result,out,base}=compile(); const ganache=require(path.join(base,'node_modules/ganache')),{ethers}=require(path.join(base,'node_modules/ethers'));
 raw=ganache.provider({logging:{quiet:true},chain:{hardfork:'shanghai',chainId:31337,allowUnlimitedContractSize:false},wallet:{deterministic:true},miner:{blockGasLimit:100000000}});
 const p=new ethers.providers.Web3Provider(raw),s=p.getSigner();p.pollingInterval=10;
 const va=out.contracts['infinity/src/Vault.sol'].Vault,ma=out.contracts['infinity/src/pool-bin/BinPoolManager.sol'].BinPoolManager;
 for(const a of [va,ma])requireThat(a.evm.deployedBytecode.object.length/2<=24576,'EIP170');
 const v=await new ethers.ContractFactory(va.abi,va.evm.bytecode.object,s).deploy({gasLimit:15000000});await v.deployed();
 const m=await new ethers.ContractFactory(ma.abi,ma.evm.bytecode.object,s).deploy(v.address,{gasLimit:15000000});await m.deployed();
 const report={passed:true,evm:'Ganache7.9.2 Shanghai (Cancun getters only; no settlement claim)',unlimited_contract_size:false,manager:m.address,vault:v.address,manager_code_sha256:sha(Buffer.from((await p.getCode(m.address)).slice(2),'hex')),vault_code_sha256:sha(Buffer.from((await p.getCode(v.address)).slice(2),'hex')),manager_vault:await m.vault(),slot0:(await m.getSlot0(ethers.constants.HashZero)).map(String),bin:(await m.getBin(ethers.constants.HashZero,8388608)).map(String),next:String(await m.getNextNonEmptyBin(ethers.constants.HashZero,true,8388608)),extsload:await m['extsload(bytes32)'](ethers.constants.HashZero),key:(await m.poolIdToPoolKey(ethers.constants.HashZero)).map(String),compile_evidence:result};
 await (await v.registerApp(m.address)).wait();report.registered=await v.isAppRegistered(m.address);
 fs.writeFileSync(path.join(ref,'deployment-evidence.json'),JSON.stringify(report,null,2)+'\n');fs.writeFileSync(receipt,JSON.stringify({passed:true,manager:m.address,vault:v.address,evidence_sha256:sha(fs.readFileSync(path.join(ref,'deployment-evidence.json')))},null,2)+'\n');console.log(JSON.stringify({...report,compile_evidence:undefined}));
 }catch(e){fs.writeFileSync(receipt,JSON.stringify({passed:false,error:String(e),stack:e.stack},null,2)+'\n');console.error(e);process.exitCode=1;}finally{if(raw)await raw.disconnect();}})();
