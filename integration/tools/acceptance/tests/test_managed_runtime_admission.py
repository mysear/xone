"""Reference-only EIP170 artifacts cannot authorize a service runtime template."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
ROOT=Path(__file__).resolve().parents[4]
spec=importlib.util.spec_from_file_location('managed_runtime_projection',ROOT/'integration/tools/ci/managed-fees-runtime-golden.py')
projection=importlib.util.module_from_spec(spec);spec.loader.exec_module(projection)
class ManagedRuntimeAdmission(unittest.TestCase):
 def test_reference_only_compiler_output_is_never_projected(self):
  with tempfile.TemporaryDirectory()as temp:
   ref=Path(temp)
   for deployable,length in [(False,27545),(True,27545),(False,20000)]:
    (ref/'runtime-metadata.json').write_text(json.dumps({'revision':'e281ff40583c39bf1d46c322334b2b13936e0e33','eip170_deployable':deployable,'runtime_bytes':length}))
    with patch.object(projection,'REF',ref),self.assertRaisesRegex(ValueError,'reference-only or oversized'):
     projection.generate()
   self.assertEqual([p.name for p in ref.iterdir()],['runtime-metadata.json'])
if __name__=='__main__':unittest.main()
