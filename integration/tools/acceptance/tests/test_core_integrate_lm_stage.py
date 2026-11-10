import os
from pathlib import Path
import stat
import sys
import tempfile
import time
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from core_integrate_lm_stage import copy_source


class SourceCopyFreshnessTests(unittest.TestCase):
    def test_old_staged_source_preserves_bytes_and_mode_but_invalidates_build_cache(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "staged.rs"
            target = Path(directory) / "replacement.rs"
            source.write_text("pub struct AddedWireField;\n")
            source.chmod(0o640)
            os.utime(source, (946684800, 946684800))
            before = time.time()
            copy_source(source, target)
            self.assertEqual(target.read_bytes(), source.read_bytes())
            self.assertEqual(stat.S_IMODE(target.stat().st_mode), 0o640)
            self.assertGreater(target.stat().st_mtime, source.stat().st_mtime)
            self.assertGreaterEqual(target.stat().st_mtime, before - 1)
