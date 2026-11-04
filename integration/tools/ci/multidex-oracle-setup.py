#!/usr/bin/env python3
"""Pin reference packages outside service runtimes; no package install scripts."""
from pathlib import Path
import shutil,subprocess
root=Path(__file__).resolve().parents[3];source=root/'integration/tools/reference/multidex';runtime=root/'integration/.runtime/multidex-oracle';runtime.mkdir(parents=True,exist_ok=True)
for name in ['package.json','package-lock.json']:shutil.copyfile(source/name,runtime/name)
subprocess.run(['npm','ci','--prefix',str(runtime),'--ignore-scripts','--no-audit','--no-fund'],check=True)
subprocess.run(['node',str(root/'integration/tools/ci/multidex-golden.cjs')],check=True)
