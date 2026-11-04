#!/usr/bin/env python3
"""Install pinned official reference only in an isolated integration runtime."""
from pathlib import Path
import shutil,subprocess
root=Path(__file__).resolve().parents[3];source=root/'integration/tools/reference/v3-sdk';runtime=root/'integration/.runtime/v3-sdk-oracle';runtime.mkdir(parents=True,exist_ok=True)
for name in ['package.json','package-lock.json']:shutil.copyfile(source/name,runtime/name)
subprocess.run(['npm','ci','--prefix',str(runtime),'--ignore-scripts','--no-audit','--no-fund'],check=True)
subprocess.run(['node',str(root/'integration/tools/ci/v3-golden.cjs')],check=True)
