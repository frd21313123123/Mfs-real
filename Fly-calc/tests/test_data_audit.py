import importlib.util
from pathlib import Path
import unittest
path = Path(__file__).resolve().parents[1] / 'scripts/audit_data.py'
spec=importlib.util.spec_from_file_location('audit_data',path)
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)

class IdentityTests(unittest.TestCase):
    def test_same_fix_name_at_different_locations_is_not_merged(self):
        self.assertNotEqual(module.point_key('DUP',55,37),module.point_key('DUP',51,-1))
    def test_coordinate_precision_is_explicit(self):
        self.assertEqual(module.point_key('DUP','55.123456','37.123456'),('DUP',55.1235,37.1235))

if __name__=='__main__':unittest.main()
