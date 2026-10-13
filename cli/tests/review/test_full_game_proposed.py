"""Approved full-game golden: exact bytes and engine requests."""
import gzip,hashlib,json,sys,unittest
from pathlib import Path
HERE=Path(__file__).parent;ROOT=HERE.parents[2]
sys.path.insert(0,str(HERE))
from run import evaluate
from contracts import load_full_game
class FullGameProposal(unittest.TestCase):
 def test_full_game_bytes_and_requests(self):
  raw=(HERE/'full-game.json').read_bytes();gold=json.loads(raw);lock=json.loads((HERE/'full-game-lock.json').read_text())
  self.assertEqual(hashlib.sha256(raw).hexdigest(),lock['sha256'])
  gold=load_full_game()
  capture_raw=(HERE/gold['capture_file']).read_bytes()
  self.assertEqual(hashlib.sha256(capture_raw).hexdigest(),gold['capture_sha256'])
  records=json.loads(gzip.decompress(capture_raw))['records'];frames=gold['frames']
  wire={'initial':[{'state':records[0]['state']}],'steps':[{'expect':{'op':'act','sequence':records[i-1]['state']['sequence'],'selection':r['selection']},'responses':[{'state':r['state']}]} for i,r in enumerate(records) if i]}
  expected=''.join(f['output'] for f in frames)
  case={'id':'full-game-seed4','method':'process','args':{'wire':wire},'inputs':[f['input'] for f in frames if f['input'] is not None],'expected':{'text':expected,'stderr':'','exitcode':0,'request_count':len(records)-1},'max_lines':len(expected.splitlines()),'forbidden':['番号／','模擬対戦を開始します','fetch_selection','pay_mana','shuffle','order_trigger','解決前に対応できます']}
  result=evaluate(case)
  result_path=ROOT/'artifacts/cli-coverage-review/full-game-result.json'
  result_path.parent.mkdir(parents=True,exist_ok=True)
  result_path.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n')
  self.assertEqual(result['errors'],[],f"Approved dialogue differs: {result['errors']}; see artifacts/cli-coverage-review/full-game-result.json")
if __name__=='__main__':unittest.main()
