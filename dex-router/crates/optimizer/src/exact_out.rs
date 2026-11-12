//! Ordered output-target allocation with exact integer conservation.
use crate::{OptimizationLimits, MAX_SPLITS};
use num_bigint::BigUint;
use num_traits::Zero;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};
use xone_domain::{err, Amount, Pool, Result};
use xone_routing::{
    exact_out::{evaluate_path, ExactOutPathQuote},
    Edge, Funds,
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExactOutProposal {
    pub edges: Vec<Edge>,
    pub amount_out: Amount,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExactOutAllocation {
    pub required_input: Amount,
    pub target_output: Amount,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gross_buy_amount: Option<Amount>,
    pub input_budget: Amount,
    pub unused_input_budget: Amount,
    pub routes: Vec<ExactOutPathQuote>,
}
impl ExactOutAllocation {
    pub fn gross_output(&self) -> &Amount {
        self.gross_buy_amount
            .as_ref()
            .unwrap_or(&self.target_output)
    }
}
#[derive(Clone, Debug)]
pub struct ExactOutOptimization {
    pub allocation: ExactOutAllocation,
    pub evaluations: usize,
    pub budget_exhausted: bool,
    pub grid_exhaustive: bool,
}

/// The budget is a pull limit; unused budget is not a transferred/refunded asset.
pub fn evaluate_allocation(
    pools: &[Pool],
    proposals: &[ExactOutProposal],
    output: &Amount,
    max_input: &Amount,
    funds: Funds<'_>,
) -> Result<ExactOutAllocation> {
    if pools
        .first()
        .is_some_and(|p| p.chain_key.starts_with("solana:"))
        && (output.0.bits() > 64 || max_input.0.bits() > 64)
    {
        return Err(err("EXACT_OUT_SOLANA_AMOUNT_WIDTH"));
    }
    if proposals.is_empty()
        || output.0.is_zero()
        || output.0.bits() > 256
        || max_input.0.bits() > 256
    {
        return Err(err("EXACT_OUT_INVALID_ALLOCATION"));
    }
    let mut state = pools
        .iter()
        .map(|p| (p.pool_id.clone(), p.clone()))
        .collect::<BTreeMap<_, _>>();
    if state.len() != pools.len() {
        return Err(err("DUPLICATE_PHYSICAL_POOL"));
    }
    xone_domain::validate_pool_dependencies(pools.iter())?;
    let (mut routes, mut input, mut delivered, mut targeted, mut tokens) = (
        Vec::new(),
        BigUint::zero(),
        BigUint::zero(),
        BigUint::zero(),
        None,
    );
    for proposal in proposals {
        if proposal.amount_out.0.is_zero() {
            return Err(err("EXACT_OUT_ZERO_OUTPUT"));
        }
        let quote = evaluate_path(&mut state, &proposal.edges, &proposal.amount_out, funds)?;
        let pair = (
            quote
                .tokens
                .first()
                .cloned()
                .ok_or_else(|| err("INVALID_PATH"))?,
            quote
                .tokens
                .last()
                .cloned()
                .ok_or_else(|| err("INVALID_PATH"))?,
        );
        if tokens.as_ref().is_some_and(|previous| previous != &pair) {
            return Err(err("ALLOCATION_TOKEN_MISMATCH"));
        }
        tokens = Some(pair);
        input += &quote.amount_in.0;
        if quote.output_target() != &proposal.amount_out
            || quote.amount_out.0 < proposal.amount_out.0
        {
            return Err(err("EXACT_OUT_TARGET_NOT_CONSERVED"));
        }
        targeted += &proposal.amount_out.0;
        delivered += &quote.amount_out.0;
        if input.bits() > 256 || delivered.bits() > 256 || targeted.bits() > 256 {
            return Err(err("EXACT_OUT_UINT256_OVERFLOW"));
        }
        routes.push(quote);
    }
    if targeted != output.0 || delivered < targeted {
        return Err(err("EXACT_OUT_TARGET_NOT_CONSERVED"));
    }
    if input > max_input.0 {
        return Err(err("EXACT_OUT_MAX_INPUT"));
    }
    Ok(ExactOutAllocation {
        required_input: Amount::checked(input.clone())?,
        target_output: output.clone(),
        gross_buy_amount: (delivered != output.0).then_some(Amount(delivered)),
        input_budget: max_input.clone(),
        unused_input_budget: Amount::checked(&max_input.0 - input)?,
        routes,
    })
}
struct Solver<'a> {
    pools: &'a [Pool],
    paths: &'a [ExactOutPathQuote],
    output: &'a Amount,
    max_input: &'a Amount,
    funds: Funds<'a>,
    limits: OptimizationLimits,
    control: Option<(&'a AtomicBool, Instant)>,
    evaluations: usize,
    best: Option<ExactOutAllocation>,
    exhausted: bool,
    over_budget: bool,
}
impl Solver<'_> {
    fn check(&self) -> Result<()> {
        if self.control.is_some_and(|(cancel, deadline)| {
            cancel.load(Ordering::Relaxed) || Instant::now() >= deadline
        }) {
            Err(err("BUDGET_EXHAUSTED"))
        } else {
            Ok(())
        }
    }
    fn consider(&mut self, proposals: &[ExactOutProposal]) -> Result<()> {
        self.check()?;
        if self.evaluations >= self.limits.max_evaluations {
            self.exhausted = true;
            return Ok(());
        }
        self.evaluations += 1;
        match evaluate_allocation(
            self.pools,
            proposals,
            self.output,
            self.max_input,
            self.funds,
        ) {
            Ok(candidate) => {
                if self.best.as_ref().is_none_or(|old| {
                    candidate.required_input.0 < old.required_input.0
                        || (candidate.required_input == old.required_input
                            && (candidate.gross_output().0 > old.gross_output().0
                                || (candidate.gross_output() == old.gross_output()
                                    && candidate.routes.len() < old.routes.len())))
                }) {
                    self.best = Some(candidate);
                }
            }
            Err(e) => {
                self.over_budget |= e.0 == "EXACT_OUT_MAX_INPUT";
            }
        }
        Ok(())
    }
    fn enumerate(
        &mut self,
        used: &mut BTreeSet<usize>,
        parts: usize,
        proposals: &mut Vec<ExactOutProposal>,
    ) -> Result<()> {
        self.check()?;
        if self.exhausted {
            return Ok(());
        }
        if parts == self.limits.parts {
            // Complete single paths were seeded before enumeration.
            if proposals.len() > 1 {
                self.consider(proposals)?;
            }
            return Ok(());
        }
        if proposals.len() >= self.limits.max_splits {
            return Ok(());
        }
        let remaining_only = proposals.len() + 1 == self.limits.max_splits;
        for index in 0..self.paths.len() {
            if used.contains(&index) {
                continue;
            }
            let start = if remaining_only {
                self.limits.parts
            } else {
                parts + 1
            };
            for end in start..=self.limits.parts {
                self.check()?;
                if self.exhausted {
                    return Ok(());
                }
                let before = &self.output.0 * parts / self.limits.parts;
                let after = &self.output.0 * end / self.limits.parts;
                let amount = after - before;
                if amount.is_zero() {
                    continue;
                }
                proposals.push(ExactOutProposal {
                    edges: self.paths[index].edges.clone(),
                    amount_out: Amount::checked(amount)?,
                });
                used.insert(index);
                self.enumerate(used, end, proposals)?;
                used.remove(&index);
                proposals.pop();
            }
        }
        Ok(())
    }
}
/// Exhaustiveness covers provided paths, ordered allocation grid, max_splits and
/// each path at most once. Chain costs and the wider market are outside this claim.
pub fn optimize<'a>(
    pools: &'a [Pool],
    paths: &'a [ExactOutPathQuote],
    output: &'a Amount,
    max_input: &'a Amount,
    limits: OptimizationLimits,
    funds: Funds<'a>,
    control: Option<(&'a AtomicBool, Instant)>,
) -> Result<ExactOutOptimization> {
    if pools
        .first()
        .is_some_and(|p| p.chain_key.starts_with("solana:"))
        && (output.0.bits() > 64 || max_input.0.bits() > 64)
    {
        return Err(err("EXACT_OUT_SOLANA_AMOUNT_WIDTH"));
    }
    if !(1..=100).contains(&limits.parts)
        || !(1..=MAX_SPLITS).contains(&limits.max_splits)
        || limits.max_evaluations == 0
        || output.0.is_zero()
        || output.0.bits() > 256
        || max_input.0.bits() > 256
    {
        return Err(err("EXACT_OUT_OPTIMIZATION_BUDGET"));
    }
    if paths.is_empty() {
        return Err(err("NO_ROUTE"));
    }
    let limits = OptimizationLimits {
        max_splits: limits.max_splits.min(paths.len()),
        ..limits
    };
    let mut unique = BTreeSet::new();
    for path in paths {
        let key = path
            .edges
            .iter()
            .map(|e| (e.pool_id.clone(), e.zero_for_one, e.to.clone()))
            .collect::<Vec<_>>();
        if !unique.insert(key) {
            return Err(err("EXACT_OUT_DUPLICATE_CANDIDATE"));
        }
    }
    let mut solver = Solver {
        pools,
        paths,
        output,
        max_input,
        funds,
        limits,
        control,
        evaluations: 0,
        best: None,
        exhausted: false,
        over_budget: false,
    };
    let mut seeds = paths.iter().collect::<Vec<_>>();
    seeds.sort_by(|a, b| {
        (b.output_target() == output)
            .cmp(&(a.output_target() == output))
            .then_with(|| a.amount_in.0.cmp(&b.amount_in.0))
    });
    for path in seeds {
        solver.consider(&[ExactOutProposal {
            edges: path.edges.clone(),
            amount_out: output.clone(),
        }])?;
        if solver.exhausted {
            break;
        }
    }
    solver.enumerate(&mut BTreeSet::new(), 0, &mut Vec::new())?;
    solver.check()?;
    let allocation = solver.best.ok_or_else(|| {
        err(if solver.exhausted {
            "BUDGET_EXHAUSTED"
        } else if solver.over_budget {
            "EXACT_OUT_MAX_INPUT"
        } else {
            "NO_ROUTE"
        })
    })?;
    Ok(ExactOutOptimization {
        allocation,
        evaluations: solver.evaluations,
        budget_exhausted: solver.exhausted,
        grid_exhaustive: !solver.exhausted,
    })
}
