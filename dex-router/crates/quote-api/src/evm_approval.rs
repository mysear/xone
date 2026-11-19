//! Caller approval for the quoted input at a particular observed state.
use serde_json::{json, Value};
use xone_domain::{err, Amount, Result};

pub(super) struct Requirement {
    pub token: String,
    pub owner: String,
    pub spender: String,
    pub input: Amount,
}

impl Requirement {
    pub fn disclose(&self, allowance: Option<&Amount>, binding: &str, authority: &str) -> Value {
        let status = match allowance {
            Some(n) if n >= &self.input => "SUFFICIENT",
            Some(_) => "APPROVAL_REQUIRED",
            None => "UNKNOWN",
        };
        json!({"status":status,"scope":"CALLER_ERC20_ALLOWANCE_FOR_QUOTED_INPUT",
            "bindingHash":binding,"token":self.token,"owner":self.owner,"spender":self.spender,
            "requiredInput":self.input,"observedAllowance":allowance,"authority":authority,
            "additionalApprovalNative":if status == "SUFFICIENT" {Some("0")} else {None}})
    }

    pub fn observed_allowance(&self, outcome: &Value, phase: &str) -> Result<Amount> {
        let bad = || err("SIMULATION_APPROVAL_OBSERVATION");
        let mut rows = outcome["observations"]
            .as_array()
            .ok_or_else(bad)?
            .iter()
            .filter(|o| o["phase"] == phase);
        let row = rows.next().ok_or_else(bad)?;
        let word = |address: &str| -> Result<String> {
            xone_domain::protocols::validate_address(address)?;
            Ok(format!("{:0>64}", &address[2..]))
        };
        let call = format!("0xdd62ed3e{}{}", word(&self.owner)?, word(&self.spender)?);
        if rows.next().is_some() || row["target"] != self.token || row["calldata"] != call {
            return Err(bad());
        }
        let bytes = row["output"]
            .as_str()
            .and_then(|s| s.strip_prefix("0x"))
            .filter(|s| s.len() == 64)
            .ok_or_else(bad)?;
        Amount::from_be_bytes(&hex::decode(bytes).map_err(|_| bad())?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_spender_evidence_is_required_for_zero_approval_cost() {
        let r = Requirement {
            token: format!("0x{:040x}", 1),
            owner: format!("0x{:040x}", 2),
            spender: format!("0x{:040x}", 3),
            input: "11".parse().unwrap(),
        };
        let row = json!({"phase":"split_caller_allowance_before", "target":r.token,
            "calldata":format!("0xdd62ed3e{:064x}{:064x}",2,3),"output":format!("0x{:064x}",11)});
        let allowance = r
            .observed_allowance(
                &json!({"observations":[row]}),
                "split_caller_allowance_before",
            )
            .unwrap();
        let sufficient = r.disclose(Some(&allowance), "a", "PROVIDED_LOCAL_STATE");
        assert_eq!(sufficient["additionalApprovalNative"], "0");
        let low = r.disclose(Some(&"10".parse().unwrap()), "a", "PROVIDED_LOCAL_STATE");
        assert_eq!(low["status"], "APPROVAL_REQUIRED");
        assert!(low["additionalApprovalNative"].is_null());
        assert_eq!(r.disclose(None, "a", "UNAVAILABLE")["status"], "UNKNOWN");
        for changed in [
            json!([]),
            json!([row, row]),
            {
                let mut other = row.clone();
                other["calldata"] = json!(format!("0xdd62ed3e{:064x}{:064x}", 2, 4));
                json!([other])
            },
            {
                let mut other = row.clone();
                other["output"] = json!("0x01");
                json!([other])
            },
        ] {
            assert!(r
                .observed_allowance(
                    &json!({"observations":changed}),
                    "split_caller_allowance_before"
                )
                .is_err());
        }
    }
}
