"""Approved acceptance tests. Every mismatch is a regression."""
import gzip,json,sys,unittest
from pathlib import Path
from contracts import load_cases
HERE=Path(__file__).parent
sys.path.insert(0,str(HERE))
from run import evaluate
class ProposedContract(unittest.TestCase):pass

def make_test(case):
 def test(self):
  result=evaluate(case)
  self.assertTrue(result['pass'],case['id']+'\n'+str(result['errors'])+'\n'+result.get('diff',''))
 return test
for i,case in enumerate(load_cases()['cases']):
 setattr(ProposedContract,f'test_{i:03}_{case["id"].replace("-","_")}',make_test(case))
if __name__=='__main__':unittest.main()
