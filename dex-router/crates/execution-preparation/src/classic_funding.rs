//! Classic SPL account creation and explicit native SOL/WSOL funding only.
use super::*;
use xone_execution_plan::solana_wsol as w;
pub(super) struct Addresses {
    pub source: String,
    pub destination: String,
    source_ata: String,
    destination_ata: String,
    pub temporary: Option<String>,
    seed: Option<String>,
}
impl Addresses {
    pub fn new(s: &SolanaQuote, r: &SolanaRequest, input: &str, output: &str) -> Result<Self> {
        if r.wrap_source_sol && input != w::WSOL {
            return Err(err("PREPARATION_WRAP_REQUIRES_WSOL"));
        }
        if r.refund_wrapped_source && (!r.wrap_source_sol || r.source_token_account.is_some()) {
            return Err(err("PREPARATION_REFUND_REQUIRES_TEMP_SOURCE"));
        }
        if r.unwrap_destination_sol
            && (output != w::WSOL
                || s.context.caller != s.context.recipient
                || r.destination_token_account.is_some()
                || r.create_destination_ata)
        {
            return Err(err("PREPARATION_UNWRAP_CONTEXT_UNSUPPORTED"));
        }
        let seed = (r.refund_wrapped_source || r.unwrap_destination_sol).then(|| {
            format!(
                "xone-{}-{}",
                if r.refund_wrapped_source { "in" } else { "out" },
                &s.plan_hash[..20]
            )
        });
        let temporary = seed
            .as_ref()
            .map(|v| w::address(&s.context.caller, v))
            .transpose()?;
        let source_ata = ata(&s.context.caller, input, TOKEN)?;
        let destination_ata = ata(&s.context.recipient, output, TOKEN)?;
        let source = if r.refund_wrapped_source {
            temporary.clone().ok_or_else(bad)?
        } else {
            r.source_token_account.clone().unwrap_or(source_ata.clone())
        };
        let destination = if r.unwrap_destination_sol {
            temporary.clone().ok_or_else(bad)?
        } else {
            r.destination_token_account
                .clone()
                .unwrap_or(destination_ata.clone())
        };
        m::pubkey(&source)?;
        m::pubkey(&destination)?;
        if source == destination {
            return Err(bad());
        }
        if r.create_destination_ata && destination != destination_ata {
            return Err(err("PREPARATION_DESTINATION_NOT_ATA"));
        }
        Ok(Self {
            source,
            destination,
            source_ata,
            destination_ata,
            temporary,
            seed,
        })
    }
}
pub(super) struct Funding {
    pub result: Value,
    pub prefix: Vec<Instruction>,
    pub suffix: Vec<Instruction>,
    rent: u64,
    wrap: u64,
    temporary_rent: u64,
}
fn uninitialized(a: Option<&Account>, address: &str, canonical: &str) -> bool {
    a.is_none()
        || (address == canonical
            && a.is_some_and(|v| v.owner == SYSTEM && !v.executable && v.data.is_empty()))
}
pub(super) async fn prepare(
    rpc: &Rpc,
    bank: &BTreeMap<String, Option<Account>>,
    s: &SolanaQuote,
    r: &SolanaRequest,
    a: &Addresses,
    required: u64,
) -> Result<Funding> {
    let (input, output) = if s.zero {
        (&s.pool.token0, &s.pool.token1)
    } else {
        (&s.pool.token1, &s.pool.token0)
    };
    if a.temporary
        .as_ref()
        .is_some_and(|v| bank.get(v).and_then(Option::as_ref).is_some())
    {
        return Err(err("PREPARATION_TEMP_ACCOUNT_OCCUPIED"));
    }
    let source = bank.get(&a.source).and_then(Option::as_ref);
    let destination = bank.get(&a.destination).and_then(Option::as_ref);
    let source_pending = uninitialized(source, &a.source, &a.source_ata);
    let destination_pending = uninitialized(destination, &a.destination, &a.destination_ata);
    let create_source = source_pending && r.wrap_source_sol;
    let create_destination = destination_pending && r.create_destination_ata;
    if create_source && !r.refund_wrapped_source && a.source != a.source_ata {
        return Err(err("PREPARATION_SOURCE_NOT_ATA"));
    }
    let source_prefund = if source_pending {
        source.map_or(0, |v| v.lamports)
    } else {
        0
    };
    let destination_prefund = if destination_pending {
        destination.map_or(0, |v| v.lamports)
    } else {
        0
    };
    let minimum = if create_source || create_destination || a.temporary.is_some() {
        rpc.call(
            "getMinimumBalanceForRentExemption",
            json!([165,{"commitment":"finalized"}]),
        )
        .await?
        .as_u64()
        .filter(|v| *v > 0)
        .ok_or_else(bad)?
    } else {
        0
    };
    let source_balance = if source_pending {
        if create_source && input == w::WSOL {
            source_prefund.saturating_sub(minimum)
        } else {
            0
        }
    } else {
        token(source.ok_or_else(bad)?, input, &s.context.caller, TOKEN)?
    };
    let destination_balance = if destination_pending {
        if create_destination && output == w::WSOL {
            destination_prefund.saturating_sub(minimum)
        } else {
            0
        }
    } else {
        token(
            destination.ok_or_else(bad)?,
            output,
            &s.context.recipient,
            TOKEN,
        )?
    };
    let wrap = if r.wrap_source_sol {
        required.saturating_sub(source_balance)
    } else {
        0
    };
    let source_rent = if create_source {
        minimum.saturating_sub(source_prefund)
    } else {
        0
    };
    let destination_rent = if create_destination || r.unwrap_destination_sol {
        minimum.saturating_sub(destination_prefund)
    } else {
        0
    };
    let temporary_rent = if a.temporary.is_some() { minimum } else { 0 };
    let rent = source_rent.checked_add(destination_rent).ok_or_else(bad)?;
    let payer = get(bank, &s.context.caller)?;
    if payer.owner != SYSTEM || payer.executable || !payer.data.is_empty() {
        return Err(bad());
    }
    let mut blockers = Vec::new();
    if source_pending && !create_source {
        blockers.push("SOURCE_ACCOUNT_MISSING");
    } else if source_balance.checked_add(wrap).ok_or_else(bad)? < required {
        blockers.push("INSUFFICIENT_TOKEN_BALANCE");
    }
    if destination_pending && !create_destination && !r.unwrap_destination_sol {
        blockers.push("DESTINATION_ACCOUNT_MISSING");
    }
    let refund = if r.refund_wrapped_source {
        temporary_rent
            .checked_add(required.checked_sub(s.expected_input).ok_or_else(bad)?)
            .ok_or_else(bad)?
    } else if r.unwrap_destination_sol {
        temporary_rent
            .checked_add(s.expected_output)
            .ok_or_else(bad)?
    } else {
        0
    };
    let result = json!({"quoteId":s.quote_view_id,"quoteViewId":s.quote_view_id,"snapshotRef":s.snapshot,"planHash":s.plan_hash,"ready":false,"blockers":blockers,"sourceTokenAccount":a.source,"destinationTokenAccount":a.destination,"sourceTokenProgram":TOKEN,"destinationTokenProgram":TOKEN,"destinationAccountSize":165,"requiredInput":required.to_string(),"quotedInput":s.expected_input.to_string(),"quotedOutput":s.expected_output.to_string(),"accountCreation":create_destination,"sourceAccountCreation":create_source,"rentLamports":rent.to_string(),"sourceRentLamports":source_rent.to_string(),"destinationRentLamports":destination_rent.to_string(),"sourceMinimumRentLamports":if create_source {minimum}else{0}.to_string(),"destinationMinimumRentLamports":if create_destination {minimum}else{0}.to_string(),"sourcePrefundedLamports":source_prefund.to_string(),"destinationPrefundedLamports":destination_prefund.to_string(),"rentRefundLamports":temporary_rent.to_string(),"wrapLamports":wrap.to_string(),"expectedNativeRefundLamports":refund.to_string(),"expectedResidualWsol":if input==w::WSOL {source_balance.checked_add(wrap).map(|v|v.saturating_sub(s.expected_input)).ok_or_else(bad)?} else {0}.to_string(),"expectedDestinationInitialTokenBalance":destination_balance.to_string(),"expectedDestinationTransferFee":"0","refundWrappedSource":r.refund_wrapped_source,"unwrapDestinationSol":r.unwrap_destination_sol,"sourceWsolRetained":r.wrap_source_sol && !r.refund_wrapped_source,"temporaryWsolAccount":a.temporary,"temporarySeed":a.seed,"settlementAsset":if r.unwrap_destination_sol {xone_domain::pump::NATIVE_SOL}else{output},"payerLamports":payer.lamports.to_string(),"simulationRequired":true,"signed":false,"submitted":false,"unsignedTransaction":Value::Null});
    let mut prefix = Vec::new();
    let mut suffix = Vec::new();
    if let Some(seed) = &a.seed {
        prefix.extend(w::create(&s.context.caller, seed, temporary_rent)?);
        suffix.push(w::close(&s.context.caller, seed)?);
    }
    if create_source && !r.refund_wrapped_source {
        prefix.push(create_ata(
            &s.context.caller,
            &a.source,
            &s.context.caller,
            input,
            TOKEN,
        ));
    }
    if wrap > 0 {
        let mut data = 2u32.to_le_bytes().to_vec();
        data.extend(wrap.to_le_bytes());
        prefix.push(Instruction {
            program_id: SYSTEM.into(),
            accounts: vec![
                meta(&s.context.caller, true, true),
                meta(&a.source, false, true),
            ],
            data_base64: STANDARD.encode(data),
        });
        prefix.push(Instruction {
            program_id: TOKEN.into(),
            accounts: vec![meta(&a.source, false, true)],
            data_base64: STANDARD.encode([17u8]),
        });
    }
    if create_destination {
        prefix.push(create_ata(
            &s.context.caller,
            &a.destination,
            &s.context.recipient,
            output,
            TOKEN,
        ));
    }
    Ok(Funding {
        result,
        prefix,
        suffix,
        rent,
        wrap,
        temporary_rent,
    })
}
impl Funding {
    pub fn finish(&mut self, s: &SolanaQuote, r: &SolanaRequest, cost: u64) -> Result<bool> {
        let funding = cost
            .checked_add(self.rent)
            .and_then(|v| v.checked_add(self.wrap))
            .ok_or_else(bad)?;
        let payer = self.result["payerLamports"]
            .as_str()
            .ok_or_else(bad)?
            .parse::<u64>()
            .map_err(|_| bad())?;
        if payer < funding {
            self.result["blockers"] = json!([if payer < cost {
                "INSUFFICIENT_FEE_BALANCE"
            } else if payer < cost.checked_add(self.rent).ok_or_else(bad)? {
                "INSUFFICIENT_RENT_BALANCE"
            } else {
                "INSUFFICIENT_WRAP_BALANCE"
            }]);
            return Ok(false);
        }
        let persistent = self.rent.checked_sub(self.temporary_rent).ok_or_else(bad)?;
        let delta = if r.refund_wrapped_source {
            -i128::from(cost) - i128::from(persistent) - i128::from(s.expected_input)
        } else if r.unwrap_destination_sol {
            i128::from(s.expected_output) - i128::from(cost) - i128::from(persistent)
        } else {
            -i128::from(funding)
        };
        self.result["feeLamports"] = cost.to_string().into();
        self.result["totalFundingLamports"] = funding.to_string().into();
        self.result["expectedPayerLamportDelta"] = delta.to_string().into();
        Ok(true)
    }
}
