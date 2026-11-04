#![recursion_limit = "512"]
#![allow(deprecated)]
//! Original program structs are the authority for bytes and native post-state.
#[path = "../identity.rs"] mod identity;
use identity::{R, require, hash};
use anchor_lang::{prelude::*, AccountDeserialize, AccountSerialize, Discriminator};
use anchor_lang::__private::{bytemuck, base64::{engine::general_purpose::STANDARD, Engine}};
use serde_json::{json, Value};
use sha2::{Digest,Sha256};
use std::{cell::{RefCell,RefMut}, io::{self,BufRead,Write}, str::FromStr};
use ::whirlpool::{ID, errors::ErrorCode, manager::{swap_manager,fee_rate_manager::FeeRateManager}, math::sqrt_price_from_tick_index, state::*, util::SwapTickSequence};
const TOKEN:&str="TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const LOADER:&str="BPFLoaderUpgradeab1e11111111111111111111111";
const IMMUTABLE:&str="BPFLoader2111111111111111111111111111111111";
const CLOCK:&str="SysvarC1ock11111111111111111111111111111111";
const SYSVAR:&str="Sysvar1111111111111111111111111111111111111";
const PROGRAM_CODE:&str="XONE synthetic Orca Whirlpool proof fixture; NOT deployed program bytecode; cc683ec9";
const TOKEN_CODE:&str="XONE synthetic classic SPL Token proof fixture; NOT deployed token bytecode";
const BASE_TIME:u64=1793577600;
fn key(label:&str)->Pubkey {
    for n in 0u64.. { let bytes:[u8;32]=Sha256::digest(format!("xone-orca-adaptive-service:{label}:{n}").as_bytes()).into(); let k=Pubkey::new_from_array(bytes); if k.is_on_curve(){return k} }
    unreachable!()
}
fn pk(v:&Value)->R<Pubkey>{Pubkey::from_str(v.as_str().ok_or("missing pubkey")?).map_err(|e|e.to_string())}
fn n(v:&Value)->R<u64>{v.as_u64().ok_or_else(||"expected u64".into())}
fn wide(v:&Value)->R<u128>{v.as_str().ok_or("expected decimal string")?.parse().map_err(|e|format!("invalid decimal: {e}"))}
fn j<T:serde::Serialize>(v:T)->Value{json!(v)}
fn pack<T:AccountSerialize>(v:&T)->R<Vec<u8>>{let mut b=Vec::new();v.try_serialize(&mut b).map_err(|e|e.to_string())?;Ok(b)}
fn pack_pod<T:bytemuck::Pod+Discriminator>(v:&T)->Vec<u8>{let mut b=T::DISCRIMINATOR.to_vec();b.extend_from_slice(bytemuck::bytes_of(v));b}
fn unpack_pod<T:bytemuck::Pod+Discriminator>(b:&[u8])->R<T>{
    require(b.len()==8+std::mem::size_of::<T>(),"zero-copy length differs")?;
    require(&b[..8]==T::DISCRIMINATOR,"zero-copy discriminator differs")?;
    Ok(bytemuck::pod_read_unaligned(&b[8..]))
}
fn rpc(owner:&str, executable:bool, bytes:Vec<u8>)->Value{json!({"lamports":10000000,"owner":owner,"executable":executable,"rentEpoch":0,"space":bytes.len(),"data":[STANDARD.encode(bytes),"base64"]})}
fn bytes(bank:&Value,address:&str)->R<Vec<u8>>{
    let a=&bank["accounts"][address];require(a["data"][1]=="base64","RPC encoding differs")?;
    let b=STANDARD.decode(a["data"][0].as_str().ok_or("missing RPC base64")?).map_err(|e|e.to_string())?;
    require(a["space"]==b.len(),"RPC space differs")?;Ok(b)
}
fn put(bank:&mut Value,address:&str,owner:&str,executable:bool,b:Vec<u8>){bank["accounts"][address]=rpc(owner,executable,b);}
fn set_clock(bank:&mut Value,slot:u64,time:u64)->R<()>{
    let mut b=Vec::new();b.extend_from_slice(&slot.to_le_bytes());b.extend_from_slice(&((time-1200)as i64).to_le_bytes());
    b.extend_from_slice(&831u64.to_le_bytes());b.extend_from_slice(&832u64.to_le_bytes());b.extend_from_slice(&(time as i64).to_le_bytes());
    put(bank,CLOCK,SYSVAR,false,b);bank["slot"]=json!(slot);bank["timestamp"]=json!(time);
    let sources=bank["accounts"].as_object().ok_or("accounts missing")?.keys().map(|k|(k.clone(),json!(slot))).collect::<serde_json::Map<_,_>>();
    bank["source_slots"]=json!(sources);Ok(())
}
fn patterned<const N:usize>(seed:u8)->[u8;N]{std::array::from_fn(|i| ((i+usize::from(seed))%255+1)as u8)}
fn array_address(pool:Pubkey,start:i32)->(Pubkey,u8){Pubkey::find_program_address(&[b"tick_array",pool.as_ref(),start.to_string().as_bytes()],&ID)}
fn constants(c:AdaptiveFeeConstants)->Value {json!({"filter_period":j(c.filter_period),"decay_period":j(c.decay_period),"reduction_factor":j(c.reduction_factor),"adaptive_fee_control_factor":j(c.adaptive_fee_control_factor),"max_volatility_accumulator":j(c.max_volatility_accumulator),"tick_group_size":j(c.tick_group_size),"major_swap_threshold_ticks":j(c.major_swap_threshold_ticks),"reserved":j(c.reserved)})}
fn variables(v:AdaptiveFeeVariables)->Value {json!({"last_reference_update_timestamp":j(v.last_reference_update_timestamp),"last_major_swap_timestamp":j(v.last_major_swap_timestamp),"volatility_reference":j(v.volatility_reference),"tick_group_index_reference":j(v.tick_group_index_reference),"volatility_accumulator":j(v.volatility_accumulator),"reserved":j(v.reserved)})}
fn rewards(rs:[WhirlpoolRewardInfo;3])->Value{json!(rs.iter().map(|r|json!({"mint":r.mint.to_string(),"vault":r.vault.to_string(),"extension":r.extension,"emissions_per_second_x64":r.emissions_per_second_x64.to_string(),"growth_global_x64":r.growth_global_x64.to_string(),"initialized":r.initialized()})).collect::<Vec<_>>())}
fn tick(t:Tick,index:i32)->Value{
    let (initialized,net,gross,a,b,r)=(t.initialized,t.liquidity_net,t.liquidity_gross,t.fee_growth_outside_a,t.fee_growth_outside_b,t.reward_growths_outside);
    json!({"tick":index,"initialized":initialized,"liquidity_net":net.to_string(),"liquidity_gross":gross.to_string(),"fee_growth_outside_a":a.to_string(),"fee_growth_outside_b":b.to_string(),"reward_growths_outside":r.map(|v|v.to_string())})
}
struct Loaded {pool:Whirlpool,oracle:Oracle,arrays:Vec<(Pubkey,FixedTickArray)>}
fn load(config:&Value,bank:&Value)->R<Loaded>{
    let pool=Whirlpool::try_deserialize(&mut bytes(bank,config["id"].as_str().ok_or("missing pool id")?)?.as_slice()).map_err(|e|e.to_string())?;
    let oracle=unpack_pod::<Oracle>(&bytes(bank,config["fixture"]["oracle"].as_str().ok_or("missing oracle id")?)?)?;
    let mut arrays=Vec::new();
    for address in config["fixture"]["tick_arrays"].as_array().ok_or("missing array addresses")? {
        arrays.push((pk(address)?,unpack_pod::<FixedTickArray>(&bytes(bank,address.as_str().ok_or("invalid array id")?)?)?));
    }
    require(pool.is_initialized_with_adaptive_fee_tier(),"pool is not adaptive")?;
    require(oracle.whirlpool==pk(&config["id"])?,"oracle pool differs")?;
    require(arrays.len()==3,"expected exactly three fixed arrays")?;
    Ok(Loaded{pool,oracle,arrays})
}
fn raw_amount(bank:&Value,address:&str)->R<u64>{let b=bytes(bank,address)?;require(b.len()==165,"vault length differs")?;Ok(u64::from_le_bytes(b[64..72].try_into().map_err(|_|"vault amount")?))}
fn native(config:&Value,bank:&Value)->R<Value>{
    let s=load(config,bank)?;let p=s.pool;let o=s.oracle;
    let arrays:Vec<_>=s.arrays.into_iter().map(|(address,a)|{
        let start=a.start_tick_index;let ts=a.ticks;let pool=a.whirlpool;
        let bitmap=ts.iter().enumerate().fold(0u128,|b,(i,t)|if t.initialized{b|(1u128<<i)}else{b});
        json!({"address":address.to_string(),"whirlpool":pool.to_string(),"layout":"fixed","start_tick_index":start,"initialized_bitmap":bitmap.to_string(),"ticks":ts.into_iter().enumerate().map(|(i,t)|tick(t,start+i as i32*i32::from(p.tick_spacing))).collect::<Vec<_>>()})
    }).collect();
    let (oracle_pool,gate,reserved)=(o.whirlpool,o.trade_enable_timestamp,o.reserved);
    let mut vaults=Vec::new();let mut mints=Vec::new();
    for (mint,vault) in [(p.token_mint_a,p.token_vault_a),(p.token_mint_b,p.token_vault_b)]{
        let v=bytes(bank,&vault.to_string())?;let m=bytes(bank,&mint.to_string())?;
        require(m.len()==82,"mint length differs")?;
        vaults.push(json!({"address":vault.to_string(),"mint":Pubkey::new_from_array(v[..32].try_into().map_err(|_|"vault mint")?).to_string(),"authority":Pubkey::new_from_array(v[32..64].try_into().map_err(|_|"vault authority")?).to_string(),"amount":raw_amount(bank,&vault.to_string())?.to_string(),"state":v[108],"data_base64":STANDARD.encode(&v)}));
        mints.push(json!({"address":mint.to_string(),"decimals":m[44],"is_initialized":m[45]==1,"supply":u64::from_le_bytes(m[36..44].try_into().map_err(|_|"mint supply")?).to_string(),"data_base64":STANDARD.encode(&m)}));
    }
    let clock=bytes(bank,CLOCK)?;
    let fields:Vec<_>=clock.chunks_exact(8).map(|x|{let mut b=[0;8];b.copy_from_slice(x);u64::from_le_bytes(b)}).collect();
    require(fields.len()==5,"Clock length differs")?;
    Ok(json!({"whirlpool":config["id"],"whirlpools_config":p.whirlpools_config.to_string(),"whirlpool_bump":p.whirlpool_bump,"tick_spacing":p.tick_spacing,"fee_tier_index_seed":p.fee_tier_index_seed,"fee_tier_index":p.fee_tier_index(),"is_initialized_with_adaptive_fee":p.is_initialized_with_adaptive_fee_tier(),"fee_rate":p.fee_rate,"protocol_fee_rate":p.protocol_fee_rate,
        "liquidity":p.liquidity.to_string(),"sqrt_price_x64":p.sqrt_price.to_string(),"tick_current_index":p.tick_current_index,"protocol_fee_owed_a":p.protocol_fee_owed_a.to_string(),"protocol_fee_owed_b":p.protocol_fee_owed_b.to_string(),"fee_growth_global_a":p.fee_growth_global_a.to_string(),"fee_growth_global_b":p.fee_growth_global_b.to_string(),"token_mint_a":p.token_mint_a.to_string(),"token_mint_b":p.token_mint_b.to_string(),"token_vault_a":p.token_vault_a.to_string(),"token_vault_b":p.token_vault_b.to_string(),"reward_last_updated_timestamp":p.reward_last_updated_timestamp,"reward_infos":rewards(p.reward_infos),
        "oracle":{"address":config["fixture"]["oracle"],"whirlpool":oracle_pool.to_string(),"trade_enable_timestamp":gate,"adaptive_fee_info":{"constants":constants(o.adaptive_fee_constants),"variables":variables(o.adaptive_fee_variables)},"reserved":reserved.to_vec()},"tick_arrays":arrays,"raw_vault_a":raw_amount(bank,&p.token_vault_a.to_string())?.to_string(),"raw_vault_b":raw_amount(bank,&p.token_vault_b.to_string())?.to_string(),"mints":mints,"vaults":vaults,
        "bank_slot":bank["slot"],"bank_unix_timestamp":bank["timestamp"],"source_slots":bank["source_slots"],"clock":{"slot":fields[0],"epoch_start_timestamp":fields[1]as i64,"epoch":fields[2],"leader_schedule_epoch":fields[3],"unix_timestamp":fields[4]as i64}}))
}
fn initial(pool_index:u8)->R<(Value,Value)>{
    let config_key=key("config");let mut mints=[key("mint-a"),key("mint-b")];mints.sort_by_key(|k|k.to_bytes());
    let spacing=if pool_index==0{64u16}else{32u16};let tier_index=1024u16+u16::from(pool_index);
    let (pool_key,bump)=Pubkey::find_program_address(&[b"whirlpool",config_key.as_ref(),mints[0].as_ref(),mints[1].as_ref(),&tier_index.to_le_bytes()],&ID);
    let (oracle_key,oracle_bump)=Pubkey::find_program_address(&[b"oracle",pool_key.as_ref()],&ID);
    let (tier_key,tier_bump)=Pubkey::find_program_address(&[b"fee_tier",config_key.as_ref(),&tier_index.to_le_bytes()],&ID);
    let (programdata,pd_bump)=Pubkey::find_program_address(&[ID.as_ref()],&Pubkey::from_str(LOADER).map_err(|e|e.to_string())?);
    let vaults=[key(&format!("vault-{pool_index}-a")),key(&format!("vault-{pool_index}-b"))];
    let authority=key("reward-authority");
    let extensions=[authority.to_bytes(),WhirlpoolExtensionSegmentPrimary{control_flags:0,reserved:[0;30]}.to_bytes(),WhirlpoolExtensionSegmentSecondary{reserved:[0;32]}.to_bytes()];
    let p=Whirlpool{whirlpools_config:config_key,whirlpool_bump:[bump],tick_spacing:spacing,fee_tier_index_seed:tier_index.to_le_bytes(),fee_rate:3000+u16::from(pool_index)*1000,protocol_fee_rate:2500,liquidity:1000000,sqrt_price:sqrt_price_from_tick_index(0),tick_current_index:0,protocol_fee_owed_a:77,protocol_fee_owed_b:88,token_mint_a:mints[0],token_vault_a:vaults[0],token_mint_b:mints[1],token_vault_b:vaults[1],fee_growth_global_a:123456789012345678,fee_growth_global_b:234567890123456789,reward_last_updated_timestamp:BASE_TIME,reward_infos:std::array::from_fn(|i|WhirlpoolRewardInfo{extension:extensions[i],..Default::default()})};
    require(p.is_initialized_with_adaptive_fee_tier(),"adaptive fee tier equals spacing")?;
    let mut o=Oracle::default();
    o.initialize(pool_key,Some(BASE_TIME),spacing,10,100,5000,99999,1000000,8,512).map_err(|e|e.to_string())?;
    o.adaptive_fee_constants.reserved=patterned::<16>(11+pool_index);
    o.adaptive_fee_variables=AdaptiveFeeVariables{last_reference_update_timestamp:BASE_TIME,last_major_swap_timestamp:BASE_TIME,volatility_reference:900000,tick_group_index_reference:0,volatility_accumulator:1000000,reserved:patterned::<16>(67+pool_index)};
    o.reserved=patterned::<128>(121+pool_index);
    require(pack(&p)?.len()==Whirlpool::LEN,"Whirlpool pack length differs")?;
    require(pack_pod(&o).len()==Oracle::LEN,"Oracle pack length differs")?;
    let span=88*i32::from(spacing);let starts=[-span,0,span];
    let arrays:Vec<_>=starts.into_iter().map(|start|array_address(pool_key,start)).collect();
    let config=json!({"id":pool_key.to_string(),"protocol":"orca-whirlpools-adaptive","adapter_version":"orca-whirlpools/cc683ec9-spl-adaptive-1","program_id":ID.to_string(),"token0":mints[0].to_string(),"token1":mints[1].to_string(),"fee_numerator":p.fee_rate,"fee_denominator":1000000,"accounts":[pool_key.to_string(),vaults[0].to_string(),vaults[1].to_string(),config_key.to_string(),mints[0].to_string(),mints[1].to_string()],
        "orca_whirlpool":{"expected_programdata":programdata.to_string(),"expected_program_code_sha256":hash(PROGRAM_CODE.as_bytes()),"expected_deployed_slot":200,"expected_token_program_code_sha256":hash(TOKEN_CODE.as_bytes()),"tick_array_starts":starts,"max_steps":8192},
        "fixture":{"pool_bump":bump,"oracle":oracle_key.to_string(),"oracle_bump":oracle_bump,"adaptive_fee_tier":tier_key.to_string(),"fee_tier_bump":tier_bump,"fee_tier_index":tier_index,"tick_spacing":spacing,"tick_arrays":arrays.iter().map(|a|a.0.to_string()).collect::<Vec<_>>(),"tick_array_bumps":arrays.iter().map(|a|a.1).collect::<Vec<_>>(),"programdata_bump":pd_bump,"token_program":TOKEN,"clock":CLOCK,"synthetic_proof_identity":true}});
    let mut bank=json!({"accounts":{},"source_slots":{}});
    put(&mut bank,&pool_key.to_string(),&ID.to_string(),false,pack(&p)?);
    put(&mut bank,&oracle_key.to_string(),&ID.to_string(),false,pack_pod(&o));
    let cfg=WhirlpoolsConfig{fee_authority:key("fee-authority"),collect_protocol_fees_authority:key("protocol-authority"),reward_emissions_super_authority:authority,default_protocol_fee_rate:p.protocol_fee_rate,feature_flags:0};
    put(&mut bank,&config_key.to_string(),&ID.to_string(),false,pack(&cfg)?);
    let tier=AdaptiveFeeTier{whirlpools_config:config_key,fee_tier_index:tier_index,tick_spacing:spacing,initialize_pool_authority:key("initialize-authority"),delegated_fee_authority:key("delegated-authority"),default_base_fee_rate:p.fee_rate,filter_period:10,decay_period:100,reduction_factor:5000,adaptive_fee_control_factor:99999,max_volatility_accumulator:1000000,tick_group_size:8,major_swap_threshold_ticks:512};
    let mut tier_bytes=pack(&tier)?;tier_bytes.extend_from_slice(&patterned::<128>(151+pool_index));require(tier_bytes.len()==AdaptiveFeeTier::LEN,"tier reserved length differs")?;
    put(&mut bank,&tier_key.to_string(),&ID.to_string(),false,tier_bytes);
    for side in 0..2{let mut mint=vec![0;82];mint[36..44].copy_from_slice(&10000000000000u64.to_le_bytes());mint[44]=if side==0{6}else{9};mint[45]=1;put(&mut bank,&mints[side].to_string(),TOKEN,false,mint);
        let mut vault=vec![0;165];vault[..32].copy_from_slice(mints[side].as_ref());vault[32..64].copy_from_slice(pool_key.as_ref());vault[64..72].copy_from_slice(&(100000000000u64+side as u64*10000000000).to_le_bytes());vault[108]=1;put(&mut bank,&vaults[side].to_string(),TOKEN,false,vault);}
    for ((address,_),start) in arrays.iter().zip(starts){
        let ticks=std::array::from_fn(|i|{let index=start+i as i32*i32::from(spacing);if [-4,-2,-1,0,1,2,4].map(|x|x*i32::from(spacing)).contains(&index){Tick{initialized:true,liquidity_net:if index<0{20000}else if index>0{-20000}else{0},liquidity_gross:200000,fee_growth_outside_a:37,fee_growth_outside_b:53,reward_growths_outside:[0;3]}}else{Tick::default()}});
        let a=FixedTickArray{start_tick_index:start,ticks,whirlpool:pool_key};let b=pack_pod(&a);require(b.len()==FixedTickArray::LEN,"fixed array pack length differs")?;put(&mut bank,&address.to_string(),&ID.to_string(),false,b);
    }
    let mut b=2u32.to_le_bytes().to_vec();b.extend_from_slice(programdata.as_ref());put(&mut bank,&ID.to_string(),LOADER,true,b);
    let mut b=3u32.to_le_bytes().to_vec();b.extend_from_slice(&200u64.to_le_bytes());b.push(1);b.extend_from_slice(key("upgrade-authority").as_ref());b.extend_from_slice(PROGRAM_CODE.as_bytes());put(&mut bank,&programdata.to_string(),LOADER,false,b);
    put(&mut bank,TOKEN,IMMUTABLE,true,TOKEN_CODE.as_bytes().to_vec());set_clock(&mut bank,800000+u64::from(pool_index)*100,BASE_TIME+1)?;
    bank["native_state"]=native(&config,&bank)?;Ok((config,bank))
}
fn loaded_mut(cell:&RefCell<FixedTickArray>)->LoadedTickArrayMut<'_>{RefMut::map(cell.borrow_mut(),|x|x as &mut dyn TickArrayType)}
fn trade(config:&Value,bank:&Value,params:&Value)->R<Value>{
    let s=load(config,bank)?;let mut p=s.pool;let mut o=s.oracle;
    let left=params["a_to_b"].as_bool().ok_or("direction missing")?;
    let amount:u64=wide(&params["amount"])?.try_into().map_err(|_|"amount exceeds u64")?;
    let time=n(&params["timestamp"])?;require(time==n(&bank["timestamp"])?,"trade time differs from bank Clock")?;
    let info=AdaptiveFeeInfo{constants:o.adaptive_fee_constants,variables:o.adaptive_fee_variables};
    let mut pm_bytes=pack(&p)?;let mut om_bytes=pack_pod(&o);let pool_key=pk(&config["id"])?;let oracle_key=pk(&config["fixture"]["oracle"])?;
    let mut lp=10000000u64;let mut lo=10000000u64;
    let pi=AccountInfo::new(&pool_key,false,true,&mut lp,&mut pm_bytes,&ID,false,0);
    let oi=AccountInfo::new(&oracle_key,false,true,&mut lo,&mut om_bytes,&ID,false,0);
    let pa=Account::<Whirlpool>::try_from(&pi).map_err(|e|e.to_string())?;
    let accessor=OracleAccessor::new(&pa,oi).map_err(|e|e.to_string())?;
    if !accessor.is_trade_enabled(time).map_err(|e|e.to_string())? { let e:anchor_lang::error::Error=ErrorCode::TradeIsNotEnabled.into();return Ok(json!({"status":"native_rejected","error":e.to_string(),"error_origin":"original OracleAccessor::is_trade_enabled and original swap instruction TradeIsNotEnabled","post_accounts":bank["accounts"]})); }
    let cells:Vec<_>=s.arrays.iter().map(|(_,a)|RefCell::new(*a)).collect();
    let span=88*i32::from(p.tick_spacing);let shift=if left{0}else{i32::from(p.tick_spacing)};
    let first=s.arrays.iter().position(|(_,a)|{let start=a.start_tick_index;p.tick_current_index>=start-shift&&p.tick_current_index<start+span-shift}).ok_or("no starting array")?;
    let order:Vec<usize>=if left{(0..=first).rev().collect()}else{(first..cells.len()).collect()};
    let mut sequence=SwapTickSequence::new(loaded_mut(&cells[order[0]]),order.get(1).map(|i|loaded_mut(&cells[*i])),order.get(2).map(|i|loaded_mut(&cells[*i])));
    let mut fee_manager=FeeRateManager::new(left,p.tick_current_index,time,p.fee_rate,&Some(info.clone())).map_err(|e|e.to_string())?;
    fee_manager.update_volatility_accumulator().map_err(|e|e.to_string())?;let initial_rate=fee_manager.get_total_fee_rate();
    let update=match swap_manager::swap(&p,&mut sequence,amount,wide(&params["sqrt_price_limit"])?,true,left,time,&Some(info)){
        Ok(u)=>u,Err(e)=>return Ok(json!({"status":"native_rejected","error":e.to_string(),"error_origin":"unchanged original swap_manager::swap","post_accounts":bank["accounts"]}))};
    let (input,output)=if left{(update.amount_a,update.amount_b)}else{(update.amount_b,update.amount_a)};
    require(input==amount,"generator requires fulfilled exact input")?;
    let (lp_fee,protocol_fee)=(update.lp_fee,update.next_protocol_fee);
    p.update_after_swap(update.next_liquidity,update.next_tick_index,update.next_sqrt_price,update.next_fee_growth_global,update.next_reward_infos,update.next_protocol_fee,left,time);
    o.update_adaptive_fee_variables(update.next_adaptive_fee_info.ok_or("native adaptive post info missing")?.variables);
    drop(sequence);
    let mut next=bank.clone();put(&mut next,&pool_key.to_string(),&ID.to_string(),false,pack(&p)?);put(&mut next,&oracle_key.to_string(),&ID.to_string(),false,pack_pod(&o));
    for ((address,_),cell) in s.arrays.into_iter().zip(cells){put(&mut next,&address.to_string(),&ID.to_string(),false,pack_pod(&cell.into_inner()));}
    let mut transfers=Vec::new();
    for (address,is_input) in [(p.token_vault_a,left),(p.token_vault_b,!left)]{
        let address=address.to_string();let before=raw_amount(bank,&address)?;
        let after=if is_input{before.checked_add(input).ok_or("vault input overflow")?}else{before.checked_sub(output).ok_or("vault output underflow")?};
        let mut b=bytes(bank,&address)?;b[64..72].copy_from_slice(&after.to_le_bytes());put(&mut next,&address,TOKEN,false,b);
        transfers.push(json!({"vault":address,"before":before.to_string(),"after":after.to_string(),"direction":if is_input{"credit_input"}else{"debit_output"},"amount":if is_input{input.to_string()}else{output.to_string()}}));
    }
    Ok(json!({"status":"fulfilled","amount_in":input.to_string(),"amount_out":output.to_string(),"amount_remaining":(amount-input).to_string(),"lp_fee":lp_fee.to_string(),"protocol_fee":protocol_fee.to_string(),"fee_amount":(lp_fee+protocol_fee).to_string(),"initial_total_fee_rate":initial_rate,"next_state":native(config,&next)?,"post_accounts":next["accounts"],"transfer_model":{"kind":"classic_spl_nominal_exact_transfer_fixture_arithmetic","executed_spl_cpi":false,"vault_deltas":transfers}}))
}
fn generate()->R<Value>{
    let mut scenarios=Vec::new();
    for pool_index in 0..2 {for left in [true,false]{
        let (config,mut bank)=initial(pool_index)?;let mut banks=Vec::new();let times=[BASE_TIME+1,BASE_TIME+1,BASE_TIME+11,BASE_TIME+111];
        for (step,amount) in [15000u64,200,1200,3000].into_iter().enumerate(){
            let slot=800000+u64::from(pool_index)*100+step as u64;set_clock(&mut bank,slot,times[step])?;bank["native_state"]=native(&config,&bank)?;
            let limit=if left{-88*i32::from(config["fixture"]["tick_spacing"].as_u64().ok_or("spacing")? as u16)+i32::from(config["fixture"]["tick_spacing"].as_u64().ok_or("spacing")? as u16)}else{176*i32::from(config["fixture"]["tick_spacing"].as_u64().ok_or("spacing")? as u16)-i32::from(config["fixture"]["tick_spacing"].as_u64().ok_or("spacing")? as u16)};
            let params=json!({"amount":amount.to_string(),"a_to_b":left,"sqrt_price_limit":sqrt_price_from_tick_index(limit).to_string(),"timestamp":times[step],"max_steps":8192,"amount_specified_is_input":true});
            let expected=trade(&config,&bank,&params)?;require(expected["status"]=="fulfilled","planned native trade rejected")?;
            bank["next_trade"]=json!({"params":params,"expected":expected});banks.push(bank.clone());
            bank["accounts"]=expected["post_accounts"].clone();bank["next_trade"]=Value::Null;
        }
        set_clock(&mut bank,800000+u64::from(pool_index)*100+4,BASE_TIME+112)?;bank["native_state"]=native(&config,&bank)?;
        let params=json!({"amount":u64::MAX.to_string(),"a_to_b":left,"sqrt_price_limit":sqrt_price_from_tick_index(if left{-400000}else{400000}).to_string(),"timestamp":BASE_TIME+112,"max_steps":8192,"amount_specified_is_input":true});
        let expected=trade(&config,&bank,&params)?;require(expected["status"]=="native_rejected","native proof exhaustion must reject")?;
        bank["next_trade"]=json!({"params":params,"expected":expected});banks.push(bank);
        scenarios.push(json!({"name":format!("pool-{pool_index}-{}-continuous",if left{"a-to-b"}else{"b-to-a"}),"config":config,"banks":banks}));
    }}
    Ok(json!({"schema":"orca-adaptive-service-native-v1","metadata":{"layouts":{"whirlpool":Whirlpool::LEN,"oracle":Oracle::LEN,"oracle_body":std::mem::size_of::<Oracle>(),"adaptive_fee_constants":AdaptiveFeeConstants::LEN,"adaptive_fee_variables":AdaptiveFeeVariables::LEN,"fixed_tick_array":FixedTickArray::LEN,"tick":Tick::LEN,"config":WhirlpoolsConfig::LEN,"adaptive_fee_tier":AdaptiveFeeTier::LEN,"classic_spl_mint":82,"classic_spl_account":165,"clock":40},"source":{"revision":"cc683ec92acb5e7606caa3d8e514d9383e9c7c11","original_types":["Whirlpool::try_serialize/try_deserialize","Oracle repr(C,packed) Pod","FixedTickArray repr(C,packed) Pod","AdaptiveFeeTier::try_serialize + marked allocation reserve","WhirlpoolsConfig::try_serialize"],"execution":["OracleAccessor::is_trade_enabled","swap_manager::swap","Whirlpool::update_after_swap","Oracle::update_adaptive_fee_variables"]},"code_identity":{"synthetic_proof_identity":true,"not_deployed_attestations":true,"borrowed_from":"integration/tools/reference/orca-whirlpools/src/lib.rs::PROGRAM_CODE/TOKEN_CODE","program_code_utf8":PROGRAM_CODE,"program_code_sha256":hash(PROGRAM_CODE.as_bytes()),"token_program_code_utf8":TOKEN_CODE,"token_program_code_sha256":hash(TOKEN_CODE.as_bytes())},"packing":{"pool_oracle_tick_arrays":"original unchanged program serialize or zero-copy Pod bytes, re-read before every native trade","config":"original unchanged program serialize","mint_vault_clock":"explicit classic SPL 82/165 and Clock 40 fixture construction; no token program execution","adaptive_fee_tier_tail":"128 fixture allocation reserve bytes appended after original serialization","reserved":"distinct nonzero Oracle/constants/variables/tier allocation reserve patterns; reward slot0 authority preserved, slot1/2 extensions zero","pool_pda_seeds":["whirlpool","config","mint_a","mint_b","fee_tier_index.to_le_bytes()"],"tick_array_pda_seeds":["tick_array","pool","start_tick_index decimal ASCII"],"oracle_pda_seeds":["oracle","pool"]},"limitations":["native only; no SBF, deployment, CPI, validator transaction or RPC finality proof","classic SPL, uninitialized rewards and three contiguous complete fixed arrays only","max_steps=8192 is consumer budget; original native manager has no step counter","rejected native swap mutations discarded by fixture commit-on-success; no validator rollback claim"]},"scenarios":scenarios}))
}
fn run()->R<()>{
    let mut observed=identity::verify(std::path::Path::new(env!("CARGO_MANIFEST_DIR")),false)?;
    let compiled:Value=serde_json::from_str(env!("ORCA_REFERENCE_CLOSURE")).map_err(|e|e.to_string())?;
    observed["compiler"]=compiled["compiler"].clone();require(observed==compiled,"native service closure changed after compilation")?;
    observed["executed_binary_sha256"]=json!(hash(&std::fs::read(std::env::current_exe().map_err(|e|e.to_string())?).map_err(|e|e.to_string())?));
    let mut out=io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines(){let req:Value=serde_json::from_str(&line.map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        let value=match req["action"].as_str(){Some("identity")=>observed.clone(),Some("generate")=>{let mut v=generate()?;v["identity"]=observed.clone();v},Some("state")=>native(&req["config"],&req["bank"])? ,Some("trade")=>trade(&req["config"],&req["bank"],&req["params"])? ,_=>return Err("unknown native service action".into())};
        serde_json::to_writer(&mut out,&value).map_err(|e|e.to_string())?;out.write_all(b"\n").map_err(|e|e.to_string())?;out.flush().map_err(|e|e.to_string())?;
    }Ok(())
}
fn main(){if let Err(e)=run(){eprintln!("native service refused: {e}");std::process::exit(1);}}
