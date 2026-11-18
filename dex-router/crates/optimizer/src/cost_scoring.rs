//! Whole-allocation costs supplied by the service's execution/cost model.
//! The callback must bind its cost and atomic token unit to this allocation,
//! source and request. This module does not authenticate a price or a bank.
//! `None` marks incomplete coverage and disables cost ranking for the entire
//! evaluated set. Callback errors abort; they are never treated as zero costs.
use crate::{exact_out::ExactOutAllocation, Allocation};
use num_bigint::BigInt;
use serde::Serialize;
use std::cmp::Ordering;
use xone_domain::{Amount, Result};

#[derive(Clone, Debug, Serialize)]
pub struct CostRanking {
    pub model_version: &'static str,
    pub scope: &'static str,
    pub objective: &'static str,
    pub evaluated_candidates: usize,
    pub priced_candidates: usize,
    pub unavailable_candidates: usize,
    pub cost_adjustment_applied: bool,
    /// Signed, full-width comparison value; negative output is never clamped.
    pub signed_objective_token_units: Option<String>,
    pub selected_cost_token_units: Option<Amount>,
}

/// Swap amounts retain their ordinary allocation semantics. Only `ranking`
/// describes the economic comparison. No chain/price trust is conferred here.
pub struct CostOptimization<T> {
    pub optimization: T,
    pub ranking: CostRanking,
}

pub(crate) trait Candidate: Clone {
    const OBJECTIVE: &'static str;
    const FALLBACK_OBJECTIVE: &'static str;
    fn objective(&self, cost: &Amount) -> BigInt;
    fn compare_score(a: &BigInt, b: &BigInt) -> Ordering;
    fn compare_raw(&self, other: &Self) -> Ordering;
}
impl Candidate for Allocation {
    const OBJECTIVE: &'static str = "MAXIMUM_OUTPUT_MINUS_CANDIDATE_COST";
    const FALLBACK_OBJECTIVE: &'static str = "MAXIMUM_GROSS_BUY_AMOUNT";
    fn objective(&self, cost: &Amount) -> BigInt {
        BigInt::from(self.gross_buy_amount.0.clone()) - BigInt::from(cost.0.clone())
    }
    fn compare_score(a: &BigInt, b: &BigInt) -> Ordering {
        a.cmp(b)
    }
    fn compare_raw(&self, other: &Self) -> Ordering {
        self.gross_buy_amount.cmp(&other.gross_buy_amount)
    }
}
impl Candidate for ExactOutAllocation {
    const OBJECTIVE: &'static str = "MINIMUM_INPUT_PLUS_CANDIDATE_COST";
    const FALLBACK_OBJECTIVE: &'static str = "MINIMUM_REQUIRED_SELL_AMOUNT";
    fn objective(&self, cost: &Amount) -> BigInt {
        BigInt::from(self.required_input.0.clone()) + BigInt::from(cost.0.clone())
    }
    fn compare_score(a: &BigInt, b: &BigInt) -> Ordering {
        b.cmp(a)
    }
    fn compare_raw(&self, other: &Self) -> Ordering {
        other
            .required_input
            .cmp(&self.required_input)
            .then_with(|| self.gross_output().cmp(other.gross_output()))
            .then_with(|| other.routes.len().cmp(&self.routes.len()))
    }
}

pub(crate) type CostEvaluator<'a, A> = dyn FnMut(&A) -> Result<Option<Amount>> + 'a;
pub(crate) struct Scorer<'a, A> {
    evaluator: &'a mut CostEvaluator<'a, A>,
    best: Option<(A, BigInt, Amount)>,
    evaluated: usize,
    unavailable: usize,
}
impl<'a, A: Candidate> Scorer<'a, A> {
    pub(crate) fn new(evaluator: &'a mut CostEvaluator<'a, A>) -> Self {
        Self {
            evaluator,
            best: None,
            evaluated: 0,
            unavailable: 0,
        }
    }
    pub(crate) fn consider(&mut self, allocation: &A) -> Result<()> {
        self.evaluated += 1;
        let Some(cost) = (self.evaluator)(allocation)? else {
            self.unavailable += 1;
            return Ok(());
        };
        cost.to_be_bytes()?;
        let objective = allocation.objective(&cost);
        if self.best.as_ref().is_none_or(|(old, score, _)| {
            A::compare_score(&objective, score)
                .then_with(|| allocation.compare_raw(old))
                .is_gt()
        }) {
            self.best = Some((allocation.clone(), objective, cost));
        }
        Ok(())
    }
    pub(crate) fn finish(self, fallback: A) -> (A, CostRanking) {
        let applied = self.evaluated > 0 && self.unavailable == 0;
        let mut ranking = CostRanking {
            model_version: "xone-candidate-cost-ranking/1",
            scope: "EVALUATED_COMPLETE_ALLOCATIONS_WITHIN_ORDERED_GRID",
            objective: if applied {
                A::OBJECTIVE
            } else {
                A::FALLBACK_OBJECTIVE
            },
            evaluated_candidates: self.evaluated,
            priced_candidates: self.evaluated - self.unavailable,
            unavailable_candidates: self.unavailable,
            cost_adjustment_applied: applied,
            signed_objective_token_units: None,
            selected_cost_token_units: None,
        };
        if applied {
            if let Some((allocation, objective, cost)) = self.best {
                ranking.signed_objective_token_units = Some(objective.to_string());
                ranking.selected_cost_token_units = Some(cost);
                return (allocation, ranking);
            }
        }
        (fallback, ranking)
    }
}
