use super::{evaluate_allocation_context, AllocatedRoute, Allocation, OptimizationExecution};
use num_bigint::BigUint;
use num_traits::Zero;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::Ordering,
};
use xone_domain::{err, Amount, Pool, Result};
use xone_routing::PathQuote;

pub const MAX_SPLITS: usize = 8;

#[derive(Clone, Copy, Debug)]
pub struct OptimizationLimits {
    pub parts: usize,
    pub max_splits: usize,
    /// Includes unsuccessful proposals and partial seed evaluations.
    pub max_evaluations: usize,
}

pub struct OptimizationResult {
    pub allocation: Allocation,
    pub evaluations: usize,
    pub budget_exhausted: bool,
    /// Every ordered allocation on the declared integer grid was considered.
    /// This does not establish an optimum between grid points or outside paths.
    pub grid_exhaustive: bool,
}

struct Solver<'a> {
    pools: &'a [Pool],
    paths: &'a [PathQuote],
    input: &'a Amount,
    cost: &'a Amount,
    limits: OptimizationLimits,
    execution: OptimizationExecution<'a>,
    evaluations: usize,
    best: Option<Allocation>,
    exhausted: bool,
}

impl Solver<'_> {
    fn check_time(&self) -> Result<()> {
        if self.execution.budget.is_some_and(|(cancel, deadline)| {
            cancel.load(Ordering::Relaxed) || std::time::Instant::now() >= deadline
        }) {
            return Err(err("BUDGET_EXHAUSTED"));
        }
        Ok(())
    }

    fn evaluate(&mut self, proposals: &[(usize, usize)]) -> Result<Option<Allocation>> {
        self.check_time()?;
        if self.evaluations >= self.limits.max_evaluations {
            self.exhausted = true;
            return Ok(None);
        }
        self.evaluations += 1;
        let mut used = 0;
        let mut routes = Vec::with_capacity(proposals.len());
        // Cumulative boundaries distribute every atomic-unit remainder exactly.
        for &(index, units) in proposals {
            let before = &self.input.0 * used / self.limits.parts;
            used += units;
            let after = &self.input.0 * used / self.limits.parts;
            if after == before {
                return Ok(None);
            }
            routes.push(AllocatedRoute {
                path: self.paths[index].clone(),
                amount_in: Amount::checked(after - before)?,
                amount_out: Amount::zero(),
            });
        }
        let Ok(allocation) =
            evaluate_allocation_context(self.pools, &routes, self.cost, self.execution.funds)
        else {
            return Ok(None);
        };
        if used == self.limits.parts
            && self.best.as_ref().is_none_or(|best| {
                allocation.net_buy_amount > best.net_buy_amount
                    || (allocation.net_buy_amount == best.net_buy_amount
                        && allocation.gross_buy_amount > best.gross_buy_amount)
            })
        {
            self.best = Some(allocation.clone());
        }
        Ok(Some(allocation))
    }

    /// Additive route outputs permit subset dynamic programming only when
    /// every physical pool is exclusive to a route and has isolated state.
    /// The subset and cumulative boundary retain execution order and rounding.
    fn independent_grid(&mut self, splits: usize) -> Result<bool> {
        if self.paths.len() > 8 || self.limits.parts > 32 {
            return Ok(false);
        }
        let pools = self
            .pools
            .iter()
            .map(|p| (p.pool_id.as_str(), p))
            .collect::<BTreeMap<_, _>>();
        let mut physical = BTreeSet::new();
        for path in self.paths {
            if path.edges.is_empty() {
                return Ok(false);
            }
            for edge in &path.edges {
                let Some(pool) = pools.get(edge.pool_id.as_str()) else {
                    return Ok(false);
                };
                // Vault wallets, base-pool dependencies and manager accounting
                // have cross-pool effects and must use the sequential solver.
                if !xone_domain::protocols::protocol(&pool.protocol)
                    .is_some_and(|p| matches!(p.family, "v2" | "v3" | "aero"))
                    || pool.multi_asset.is_some()
                    || !physical.insert(edge.pool_id.as_str())
                {
                    return Ok(false);
                }
            }
        }
        #[derive(Clone)]
        struct Cell {
            gross: BigUint,
            previous: Option<(usize, usize, usize)>,
        }
        let width = self.limits.parts + 1;
        let count = 1usize << self.paths.len();
        let mut cells: Vec<Option<Cell>> = vec![None; count * width];
        cells[0] = Some(Cell {
            gross: BigUint::zero(),
            previous: None,
        });
        let mut quotes = BTreeMap::<(usize, BigUint), Option<BigUint>>::new();
        let mut best_cell: Option<(BigUint, BigUint, usize)> = None;
        'states: for mask in 0..count {
            if mask.count_ones() as usize >= splits {
                continue;
            }
            for used in 0..self.limits.parts {
                let Some(current) = cells[mask * width + used].clone() else {
                    continue;
                };
                self.check_time()?;
                for index in 0..self.paths.len() {
                    if mask & (1 << index) != 0 {
                        continue;
                    }
                    for end in used + 1..=self.limits.parts {
                        let input = &self.input.0 * end / self.limits.parts
                            - &self.input.0 * used / self.limits.parts;
                        if input.is_zero() {
                            continue;
                        }
                        let key = (index, input.clone());
                        if !quotes.contains_key(&key) {
                            self.check_time()?;
                            if self.evaluations.saturating_add(1) >= self.limits.max_evaluations {
                                self.exhausted = true;
                                break 'states;
                            }
                            self.evaluations += 1;
                            let route = AllocatedRoute {
                                path: self.paths[index].clone(),
                                amount_in: Amount::checked(input)?,
                                amount_out: Amount::zero(),
                            };
                            let quote = evaluate_allocation_context(
                                self.pools,
                                &[route],
                                self.cost,
                                self.execution.funds,
                            )
                            .ok()
                            .map(|a| a.gross_buy_amount.0);
                            quotes.insert(key.clone(), quote);
                        }
                        let Some(output) = quotes.get(&key).and_then(|q| q.as_ref()) else {
                            continue;
                        };
                        let gross = &current.gross + output;
                        let next_mask = mask | (1 << index);
                        let position = next_mask * width + end;
                        if cells[position].as_ref().is_none_or(|c| gross > c.gross) {
                            cells[position] = Some(Cell {
                                gross: gross.clone(),
                                previous: Some((mask, used, index)),
                            });
                            if end == self.limits.parts {
                                let cost = &self.cost.0 * next_mask.count_ones();
                                let net = if gross > cost {
                                    &gross - cost
                                } else {
                                    BigUint::zero()
                                };
                                if best_cell
                                    .as_ref()
                                    .is_none_or(|b| (&net, &gross) > (&b.0, &b.1))
                                {
                                    best_cell = Some((net, gross, next_mask));
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Some((_, _, mut mask)) = best_cell {
            let mut used = self.limits.parts;
            let mut proposal = Vec::new();
            while let Some((previous_mask, previous_used, path)) =
                cells[mask * width + used].as_ref().and_then(|c| c.previous)
            {
                proposal.push((path, used - previous_used));
                mask = previous_mask;
                used = previous_used;
            }
            proposal.reverse();
            self.evaluate(&proposal)?;
        }
        Ok(true)
    }

    fn enumerate(
        &mut self,
        proposals: &mut Vec<(usize, usize)>,
        remaining: usize,
        depth: usize,
    ) -> Result<()> {
        self.check_time()?;
        for index in 0..self.paths.len() {
            if self.exhausted {
                break;
            }
            if proposals.iter().any(|&(p, _)| p == index) {
                continue;
            }
            if depth == 1 {
                proposals.push((index, remaining));
                self.evaluate(proposals)?;
                proposals.pop();
            } else {
                // Balanced allocations first, then every positive composition.
                let target = remaining / depth;
                let mut units = (1..=remaining - depth + 1).collect::<Vec<_>>();
                units.sort_by_key(|u| u.abs_diff(target));
                for amount in units {
                    proposals.push((index, amount));
                    self.enumerate(proposals, remaining - amount, depth - 1)?;
                    proposals.pop();
                    if self.exhausted {
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    /// Gives larger splits a share of work before exhaustive pairs can consume
    /// the budget. All seed amounts are grid points, with shared state replayed.
    fn seed_large_splits(&mut self, max_splits: usize) -> Result<()> {
        let seed_budget = self.limits.max_evaluations / 3;
        for splits in 3..=max_splits {
            let quota = seed_budget / (max_splits - 2);
            if quota == 0 {
                break;
            }
            let end = self.evaluations.saturating_add(quota);
            let mut beam: Vec<(Vec<(usize, usize)>, BigUint)> = vec![(vec![], BigUint::zero())];
            for depth in 0..splits {
                let used = self.limits.parts * depth / splits;
                let units = self.limits.parts * (depth + 1) / splits - used;
                let mut next = Vec::new();
                // Reserve work for all remaining depths, including complete
                // allocations; a broad first layer must not starve the last.
                let layer_end =
                    self.evaluations + end.saturating_sub(self.evaluations) / (splits - depth);
                for (prefix, _) in &beam {
                    for index in 0..self.paths.len() {
                        if self.evaluations >= layer_end || self.exhausted {
                            break;
                        }
                        if prefix.iter().any(|&(p, _)| p == index) {
                            continue;
                        }
                        let mut proposal = prefix.clone();
                        proposal.push((index, units));
                        if let Some(allocation) = self.evaluate(&proposal)? {
                            next.push((proposal, allocation.net_buy_amount.0));
                        }
                    }
                    if self.evaluations >= layer_end || self.exhausted {
                        break;
                    }
                }
                next.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                next.truncate(8);
                beam = next;
                if beam.is_empty() {
                    break;
                }
            }
        }
        Ok(())
    }
}

/// Counts complete grid proposals with saturation at the caller's work limit.
fn proposal_count(paths: usize, parts: usize, splits: usize, cap: usize) -> usize {
    let mut total = 0u128;
    let mut permutations = 1u128;
    let mut compositions = 1u128;
    for k in 1..=splits.min(paths).min(parts) {
        permutations = permutations.saturating_mul((paths - k + 1) as u128);
        if k > 1 {
            compositions = compositions.saturating_mul((parts - k + 1) as u128) / (k - 1) as u128;
        }
        total = total.saturating_add(permutations.saturating_mul(compositions));
        if total > cap as u128 {
            return cap.saturating_add(1);
        }
    }
    total as usize
}

pub fn optimize_with_limits<'a>(
    pools: &'a [Pool],
    paths: &'a [PathQuote],
    input: &'a Amount,
    cost: &'a Amount,
    limits: OptimizationLimits,
    execution: OptimizationExecution<'a>,
) -> Result<OptimizationResult> {
    if paths.is_empty() {
        return Err(err("NO_ROUTE"));
    }
    if !(1..=100).contains(&limits.parts)
        || !(1..=MAX_SPLITS).contains(&limits.max_splits)
        || limits.max_evaluations == 0
        || limits.max_evaluations > 1_000_000
        || paths.len() > 256
        || input.0.is_zero()
    {
        return Err(err("INVALID_OPTIMIZER_BUDGET"));
    }
    let mut solver = Solver {
        pools,
        paths,
        input,
        cost,
        limits,
        execution,
        evaluations: 0,
        best: None,
        exhausted: false,
    };
    let splits = limits.max_splits.min(paths.len()).min(limits.parts);
    let count = proposal_count(paths.len(), limits.parts, splits, limits.max_evaluations);
    solver.enumerate(&mut Vec::new(), limits.parts, 1)?;
    let independent = splits == 1 || (!solver.exhausted && solver.independent_grid(splits)?);
    if !independent && count > limits.max_evaluations && splits >= 3 && !solver.exhausted {
        solver.seed_large_splits(splits)?;
    }
    for k in 2..=if independent { 1 } else { splits } {
        solver.enumerate(&mut Vec::new(), limits.parts, k)?;
        if solver.exhausted {
            break;
        }
    }
    solver.check_time()?;
    let allocation = solver.best.ok_or_else(|| {
        err(if solver.exhausted {
            "BUDGET_EXHAUSTED"
        } else {
            "NO_EXECUTABLE_ALLOCATION"
        })
    })?;
    if allocation.sell_amount != *input {
        return Err(err("INPUT_CONSERVATION"));
    }
    Ok(OptimizationResult {
        allocation,
        evaluations: solver.evaluations,
        budget_exhausted: solver.exhausted,
        grid_exhaustive: !solver.exhausted,
    })
}
