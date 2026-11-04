"""Ordinary fixture checks remain offline; only explicit reference refresh uses Node."""
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

class ReceiptReference(unittest.TestCase):
 def test_known_fixtures_load_without_external_process(self):
  path=Path(__file__).resolve().parents[1]/'receipt_reference.py'
  spec=importlib.util.spec_from_file_location('offline_receipt_reference',path)
  module=importlib.util.module_from_spec(spec)
  with patch('subprocess.run',side_effect=RuntimeError('unexpected external reference execution')):
   spec.loader.exec_module(module)
   root,receipts=module.seal_receipts([{'logs':[]}])
  self.assertEqual(root,module.SINGLE_EMPTY_ROOT)
  self.assertEqual(receipts[0]['status'],'0x1')
  self.assertEqual(receipts[0]['logsBloom'],'0x'+'00'*256)
 def test_unknown_payload_never_silently_uses_a_placeholder_root(self):
  path=Path(__file__).resolve().parents[1]/'receipt_reference.py'
  spec=importlib.util.spec_from_file_location('strict_receipt_reference',path)
  module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
  with self.assertRaises(ValueError):
   module.seal_receipts([{'status':'0x0','logs':[]}])

if __name__=='__main__':unittest.main()
