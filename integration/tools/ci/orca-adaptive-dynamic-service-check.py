#!/usr/bin/env python3
"""Fail-closed complete native replay; all guards remain enabled under Python -OO."""
from pathlib import Path
import subprocess
import sys

if __name__ == '__main__':
    subprocess.run([sys.executable, str(Path(__file__).with_name('orca-adaptive-dynamic-service-golden.py')), '--check', *sys.argv[1:]], check=True)
