import unittest
from multi_model_smoke import catalog
class PhysicalCatalog(unittest.TestCase):
 def test_all_six_normal_profiles_have_native_reference_and_full_tokens(self):
  items=catalog();self.assertEqual(len(items),6);self.assertEqual(len({i['pool']['pool_id']for i in items}),6)
  for item in items:
   p=item['pool'];s=p['multi_asset'];self.assertEqual(len(item['config']['multi_asset']['tokens']),len(s['assets']));self.assertGreater(int(item['case']['expected']['amount_out']),0)
   if p['protocol']=='balancer-v2-composable-regular':self.assertEqual(s['weights'],[]);self.assertEqual(sum(a['token']==p['pool_id']for a in s['assets']),1);self.assertTrue(all(int(a['scaling'])in{10**p for p in range(18,37)}for a in s['assets']))
