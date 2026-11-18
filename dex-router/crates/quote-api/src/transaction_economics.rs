//! A server-configured price book is independent of worker output and HTTP
//! request bodies. Apply it only after the parent has audited native fees.
use serde_json::{json, Value};
use std::io::Read;
use xone_config::ConfigSnapshot;
use xone_domain::{err, hash, Amount, Result};
use xone_execution_plan::Context;
use xone_execution_preparation::native_valuation::{
    evaluated_amounts, AuthorizedPrice, PriceAnchor, PriceBook, Settlement,
};
use xone_simulation::evm_fixture::FixtureState;

const MAX_PRICE_BYTES: u64 = 1024 * 1024;
pub(crate) struct ValuationState {
    prices: Vec<AuthorizedPrice>,
    anchor: PriceAnchor,
    execution_timestamp_ms: u64,
    book_hash: String,
}
impl ValuationState {
    pub(crate) fn load(config: &ConfigSnapshot, fixture: &FixtureState) -> Result<Option<Self>> {
        let Some(policy) = &config.config.simulation.native_valuation else {
            return Ok(None);
        };
        policy.validate()?;
        let path = config.resolve(&policy.observations_file);
        let file =
            std::fs::File::open(path).map_err(|_| err("NATIVE_VALUATION_BOOK_UNREADABLE"))?;
        let metadata = file
            .metadata()
            .map_err(|_| err("NATIVE_VALUATION_BOOK_UNREADABLE"))?;
        if !metadata.is_file() || metadata.len() > MAX_PRICE_BYTES {
            return Err(err("NATIVE_VALUATION_BOOK_LIMIT"));
        }
        let mut bytes = Vec::new();
        file.take(MAX_PRICE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| err("NATIVE_VALUATION_BOOK_UNREADABLE"))?;
        if bytes.len() as u64 > MAX_PRICE_BYTES {
            return Err(err("NATIVE_VALUATION_BOOK_LIMIT"));
        }
        let book: PriceBook =
            serde_json::from_slice(&bytes).map_err(|_| err("NATIVE_VALUATION_BOOK_INVALID"))?;
        let source = &fixture.state_anchor;
        let anchor = PriceAnchor {
            chain_key: source.chain_key.clone(),
            genesis_hash: source.genesis_hash.clone(),
            block_number: source.block_number,
            block_hash: source.block_hash.clone(),
            parent_hash: source.parent_hash.clone(),
            timestamp_ms: source
                .timestamp
                .checked_mul(1000)
                .ok_or_else(|| err("NATIVE_VALUATION_TIME"))?,
        };
        if book.prices.iter().any(|p| p.observation.anchor != anchor) {
            return Err(err("NATIVE_VALUATION_ANCHOR"));
        }
        let execution_timestamp_ms = fixture
            .execution_environment
            .timestamp
            .checked_mul(1000)
            .ok_or_else(|| err("NATIVE_VALUATION_TIME"))?;
        if execution_timestamp_ms < anchor.timestamp_ms {
            return Err(err("NATIVE_VALUATION_TIME"));
        }
        let prices = book.authorize(&policy.policies)?;
        Ok(Some(Self {
            prices,
            anchor,
            execution_timestamp_ms,
            book_hash: hash(&book)?,
        }))
    }
    pub(crate) fn apply(&self, context: &Context, response: &mut Value) -> Result<()> {
        if context.chain_key != self.anchor.chain_key {
            return Err(err("NATIVE_VALUATION_ANCHOR"));
        }
        if response["verification"]["success"] != true {
            response["costs"]["transactionEconomics"] = json!({"scope":"EVALUATED_TRANSACTION_NETWORK_FEES_ONLY","status":"EXECUTION_NOT_SUCCESSFUL","quoteRankingApplied":false,"productionVerified":false});
            return Ok(());
        }
        if response["costs"]["balanceReconciliation"]["status"] != "MATCHED"
            || response["costs"]["nativeUnit"] != "EVM_NATIVE_ATOMS"
        {
            return Err(err("NATIVE_VALUATION_FEE_AUDIT"));
        }
        let number = |v: &Value| -> Result<Amount> {
            v.as_str()
                .ok_or_else(|| err("NATIVE_VALUATION_FEE_AUDIT"))?
                .parse()
        };
        let input = number(&response["outcome"]["actual_input_consumed"])?;
        let output = number(&response["outcome"]["actual_buy_amount"])?;
        let debit = number(&response["costs"]["payerNetNativeDebit"])?;
        let price = |token: &str| self.prices.iter().find(|p| p.token() == token);
        let mut economics = evaluated_amounts(
            Settlement {
                input: &input,
                output: &output,
                debit: &debit,
                sell_token: &context.sell_token,
                buy_token: &context.buy_token,
                anchor: &self.anchor,
                at_ms: self.execution_timestamp_ms,
            },
            price(&context.sell_token),
            price(&context.buy_token),
        )?;
        economics["priceBookHash"] = self.book_hash.clone().into();
        economics["bindingHash"] = response["bindingHash"].clone();
        economics["feePayer"] = response["transaction"]["from"].clone();
        economics["recipient"] = context.recipient.clone().into();
        economics["grossChainFeeNative"] = response["costs"]["nativeKnownSubtotal"].clone();
        economics["bankAuthority"] = "PROVIDED_LOCAL_STATE".into();
        response["costs"]["transactionEconomics"] = economics;
        Ok(())
    }
}
