#!/usr/bin/env python3
"""Design 13.1 raw-amount fallback when reliable fees and valuation are absent."""
from validate_evm_native_cost import validate
if __name__=='__main__':validate(balance_audit=True,payer_cost=True,cost_fallback=True)
