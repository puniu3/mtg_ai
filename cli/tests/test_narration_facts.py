"""Cross-action invariants: life costs and separate stack resolutions stay visible."""
import copy,gzip,json,sys,unittest
from pathlib import Path
HERE=Path(__file__).parent
sys.path.insert(0,str(HERE.parent))
from narration import human_action,render

class NarrationFacts(unittest.TestCase):
 def fixture(self):
  cases=json.loads(gzip.decompress((HERE/'review/engine.json.gz').read_bytes()))
  return copy.deepcopy(next(c for c in cases if c['id']=='cast-0-3-0-false-false')['rows'])
 def test_mana_life_cost_is_visible_even_when_damage_resolves(self):
  rows=self.fixture()
  for i,r in enumerate(rows):
   for side in ('before','after'):
    if i>1 or i==1 and side=='after':r[side]['public']['life'][0]=19
    for c in r[side]['public']['battlefield']:
     if c['object_id']==1:c['card']='Sunbaked Canyon'
   if r['action']['kind']=='tap_for_mana':r['action']['source']['card']='Sunbaked Canyon'
  text=human_action(rows,rows[-1]['after'])
  self.assertIn('あなたのライフ：20→19。',text)
  self.assertIn('botのライフ：20→17。',text)
 def test_response_and_original_spell_resolve_separately(self):
  rows=self.fixture();pending={'object_id':999,'kind':'spell','card':'Lightning Bolt','controller':1,'targets':[{'player':0}]}
  for r in rows:
   for side in ('before','after'):r[side]['public']['stack'].append(copy.deepcopy(pending))
  before=copy.deepcopy(rows[-1]['after']);after=copy.deepcopy(before);after['public']['stack']=[];after['public']['life'][0]=17
  rows.append({'sequence':rows[-1]['sequence']+1,'actor':0,'action':{'kind':'pass'},'before':before,'after':after,'events':[],'outcome':None})
  text=human_action(rows,after)
  self.assertEqual(text.count('稲妻が解決'),2)
  self.assertIn('botのライフ：20→17。',text)
  self.assertIn('あなたのライフ：20→17。',text)
  self.assertNotIn('解決前に対応できます',text)
