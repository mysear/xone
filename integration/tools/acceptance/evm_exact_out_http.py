#!/usr/bin/env python3
"""Actual ExactOut HTTP and owned worker execution."""
from pathlib import Path
import sys
from evm_dynamic_fee_http import run, require
if __name__ == '__main__':
    require(len(sys.argv)==3, 'Expected BINARY OUTPUT_DIRECTORY')
    run(Path(sys.argv[1]),Path(sys.argv[2]),exact_out=True)
