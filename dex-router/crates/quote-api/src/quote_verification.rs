use super::{
    err, evm_receipt, hash, json, local_simulation, now_ms, Api, Instant, QuoteView, Result, Value,
};
use std::sync::Arc;
use xone_domain::singleton::ZERO_ADDRESS;

impl Api {
    pub(super) fn verified_backend(
        &self,
        verification: &str,
        chain: &str,
        sell: &str,
        buy: &str,
    ) -> Result<Option<Arc<local_simulation::LocalSimulation>>> {
        if verification != "VERIFIED" {
            return Ok(None);
        }
        let backend = self.local_simulation.as_ref().filter(|backend| {
            chain == self.config.config.simulation.evm_cancun_verified_chain
                && sell != ZERO_ADDRESS
                && buy != ZERO_ADDRESS
                && !backend.has_op_source()
                && backend.fixture().execution_environment.fork == "cancun"
                && backend
                    .fixture()
                    .execution_environment
                    .parent_beacon_block_root
                    .is_some()
        });
        backend
            .cloned()
            .map(Some)
            .ok_or_else(|| err("INDEPENDENT_SIMULATION_UNAVAILABLE"))
    }

    pub(super) async fn verify_quote(
        &self,
        backend: Arc<local_simulation::LocalSimulation>,
        selected: local_simulation::Selection,
        deadline: Instant,
    ) -> Result<(Value, Value)> {
        let chain = self
            .config
            .config
            .chains
            .iter()
            .find(|chain| chain.key == selected.context().chain_key)
            .ok_or_else(|| err("UNKNOWN_CHAIN"))?;
        if !backend.matches_snapshot(&selected.view().reference, &chain.genesis_hash) {
            return Err(err("INDEPENDENT_SIMULATION_UNAVAILABLE"));
        }
        self.check_verified_view(selected.view(), selected.expires())?;
        let replay = backend
            .clone()
            .execute(self.config.clone(), selected.clone(), deadline)
            .await
            .map_err(verification_error)?;
        let native = evm_receipt::compare_simulation(
            self,
            backend.fixture(),
            selected.context(),
            &replay,
            deadline,
        )
        .await
        .map_err(verification_error)?;
        if !native["settlement"].is_object()
            || native["settlement"]["actualInputConsumed"] != selected.response()["consumedIn"]
            || native["settlement"]["actualBuyAmount"] != selected.response()["grossBuyAmount"]
        {
            return Err(err("SIMULATION_FAILED"));
        }
        self.check_verified_view(selected.view(), selected.expires())?;
        let simulator_version = &replay["outcome"]["simulator_version"];
        let plan_hash = &selected.response()["planHash"];
        let evidence = json!({
            "planHash":plan_hash,"snapshotRef":selected.view().reference,
            "requestHash":selected.response()["requestHash"],"workerHash":replay["workerHash"],
            "bundleHash":replay["bundleHash"],"simulatorVersion":simulator_version,
            "nodeSimulation":native,
        });
        let verification = json!({
            "level":"VERIFIED","success":true,"planHash":plan_hash,
            "simulatorVersion":simulator_version,"evidenceId":hash(&evidence)?,
        });
        Ok((verification, evidence))
    }

    pub(super) async fn recheck_verified_quote(
        &self,
        evidence: Option<&Value>,
        view: &QuoteView,
        expires: u64,
        deadline: Instant,
    ) -> Result<()> {
        self.check_verified_view(view, expires)?;
        let source = &view.reference.block;
        let evidence = evidence.ok_or_else(|| err("SIMULATION_FAILED"))?;
        let native = &evidence["nodeSimulation"];
        let execution_hash = native["executionContextBlockHash"]
            .as_str()
            .filter(|_| native["sourceBlockHash"] == source.hash)
            .ok_or_else(|| err("SIMULATION_FAILED"))?;
        evm_receipt::recheck_simulation_context(self, source, execution_hash, deadline)
            .await
            .map_err(verification_error)?;
        self.check_verified_view(view, expires)
    }

    fn check_verified_view(&self, view: &QuoteView, expires: u64) -> Result<()> {
        self.check_view(view)?;
        if self.replica.view(&view.reference.block.chain_key)?.view_id != view.view_id {
            return Err(err("QUOTE_STATE_CHANGED"));
        }
        if expires <= now_ms() {
            return Err(err("QUOTE_EXPIRED"));
        }
        Ok(())
    }
}

fn verification_error(failure: local_simulation::Failure) -> xone_domain::Error {
    err(match failure.code.as_str() {
        "SIMULATION_TIMEOUT" | "SIMULATION_DEADLINE" => "BUDGET_EXHAUSTED",
        "SIMULATION_BUSY" => "RATE_LIMITED",
        "EVM_NODE_RECEIPT_REORGED" | "EVM_NODE_SIMULATION_REORGED" => "SNAPSHOT_REORGED",
        "EXECUTION_ADAPTER_UNSUPPORTED" | "EVM_NODE_SIMULATION_UNSUPPORTED" => {
            "INDEPENDENT_SIMULATION_UNAVAILABLE"
        }
        _ => "SIMULATION_FAILED",
    })
}
