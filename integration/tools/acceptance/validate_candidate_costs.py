#!/usr/bin/env python3
"""Whole-candidate optimizer cost hooks with all scoped price and VM gates."""
from karst_api_vectors import ROOT
from validate_native_valuation import main as validate


def main():
    cargo=['cargo','+1.90.0'];manifest=['--locked','--manifest-path','dex-router/Cargo.toml']
    validate(output_base=ROOT/'integration/artifacts/local/candidate-cost-check',extra_checks=[
        ('candidate-cost-optimizer',cargo+['test']+manifest+['-p','xone-optimizer','--test','candidate_costs','--test','grid_oracle','--test','split','--test','exact_out']),
        ('candidate-cost-normal-api-fallback',cargo+['test']+manifest+['-p','xone-quote-api','--test','cost_fallback']),
        ('candidate-cost-optimizer-lint',cargo+['clippy']+manifest+['-p','xone-optimizer','--all-targets','--','-D','warnings']),
    ],extra_required={
        'candidate-cost-optimizer':[
            'candidate_costs_match_independent_ordered_integer_grid_including_shared_state',
            'negative_candidate_scores_are_not_clamped_or_ranked_by_gross_output',
            'one_missing_candidate_cost_restores_raw_best_for_the_same_evaluated_set',
            'candidate_cost_errors_width_budget_and_cancellation_are_not_silenced',
            'exact_out_costs_match_all_original_router_grid_results_without_inverting_exact_in',
            'three_and_four_way_splits_match_independent_integer_oracle_with_remainders_and_costs',
            'eight_disjoint_routes_are_solved_on_the_complete_grid_with_finite_work',
            'shared_eight_way_scope_seeds_larger_allocations_before_pairs_spend_the_budget',
            'shared_v3_sequential_allocations_match_official_sdk_oracle_in_both_orders',
            'ordered_grid_allocations_match_original_router_transactions_and_all_pair_ledgers',
            'exact_out_solver_matches_exhaustive_original_grid_and_preserves_a_bounded_fallback',
        ],
        'candidate-cost-normal-api-fallback':[
            'unavailable_cost_exact_in_ignores_unvalued_penalty_and_matches_independent_grid',
            'unavailable_cost_fallback_preserves_user_constraints_and_budget_exhaustion',
        ],
    },extra_report=dict(optimizer_whole_candidate_cost_callback_supported=True,
        optimizer_cost_callback_confers_price_or_bank_authority=False,
        optimizer_signed_negative_objectives_supported=True,
        optimizer_incomplete_costs_uniform_raw_fallback=True,
        optimizer_nonadditive_costs_use_ordered_enumeration=True,
        normal_quote_cost_model_connected=False),
        verification_scope='BOUNDED_OPTIMIZER_WHOLE_CANDIDATE_COST_CALLBACK_ORDERED_GRID_UNIFORM_FALLBACK_SIGNED_OBJECTIVES_PLUS_FULL_NATIVE_VALUATION_AND_KARST_REGRESSION_NOT_NORMAL_QUOTE_NET_RANKING_LIVE_PRICE_TRUSTED_BANK_OR_PRODUCTION')


if __name__=='__main__':main()
