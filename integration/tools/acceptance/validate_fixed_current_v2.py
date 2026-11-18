#!/usr/bin/env python3
"""Run the existing normal-process gates against the pinned offline public bank."""
import sys
from pathlib import Path
import validate_current_v2

if __name__=='__main__':
    if len(sys.argv)!=1:raise SystemExit('This gate accepts only the pinned offline fixture')
    validate_current_v2.BASE=validate_current_v2.ROOT/'integration/artifacts/local/current-v2-fixed-check'
    sys.argv=[sys.argv[0],'--fixture',str(validate_current_v2.ROOT/'integration/test/source/current-v2-public')]
    validate_current_v2.main()
