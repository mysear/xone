#!/usr/bin/env python3
"""Selected executed-plan fee disclosure and every prior source/VM gate."""
from karst_api_vectors import ROOT
from validate_market_feeds import main


if __name__ == '__main__':
    main(output_base=ROOT/'integration/artifacts/local/selected-candidate-cost-check',
         extra_report=dict(selected_candidate_quote_cost_disclosure_supported=True,
                           selected_costs_do_not_include_complete_quote_operations=True),
         verification_scope='SELECTED_LOCAL_CANDIDATE_PLAN_COSTS_PARENT_AUDITED_NATIVE_FEES_AUTHORIZED_PER_SIDE_CONVERSION_CACHE_AND_SOURCE_BINDING_PLUS_ALL_MARKET_NORMAL_CANDIDATE_AND_HISTORICAL_GATES_NOT_COMPLETE_QUOTE_NET_OR_PRODUCTION')
