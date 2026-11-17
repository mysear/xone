#!/usr/bin/env python3
"""Actual RPC preparation -> exact cached wire -> owned worker in the same retained bank."""
from pathlib import Path
import sys
from evm_dynamic_fee_http import run,require
if __name__=='__main__':
    require(len(sys.argv)==5,'Expected BINARY OUTPUT_DIRECTORY KIND RPC_MODE')
    run(Path(sys.argv[1]),Path(sys.argv[2]),exact_out=sys.argv[3]=='EXACT_OUT',prepared_mode=sys.argv[4])
