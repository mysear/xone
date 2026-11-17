#!/usr/bin/env python3
"""Normal API and actual owned worker over original multi-pool banks."""
from pathlib import Path
import sys
from evm_dynamic_fee_http import run, require
if __name__=='__main__':
    require(len(sys.argv)==5,'Expected BINARY OUTPUT_DIRECTORY KIND CASE_ID')
    run(Path(sys.argv[1]),Path(sys.argv[2]),exact_out=sys.argv[3]=='EXACT_OUT',path_case=sys.argv[4])
