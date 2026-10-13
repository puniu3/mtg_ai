"""Validate completeness, approval and provenance of the acceptance tests.
The acceptance suite is included in default discovery.
"""
import gzip,hashlib,json,sys,unittest
from pathlib import Path
HERE=Path(__file__).parent/'review'
sys.path.insert(0,str(HERE))
from inventory import inventory
class ReviewCatalog(unittest.TestCase):
 @classmethod
 def setUpClass(cls):
  cls.proposal=json.loads(gzip.decompress((HERE/'candidates.json.gz').read_bytes()))
  cls.cases=cls.proposal['cases'];cls.tags={t for c in cls.cases for t in c['covers']}
 def test_all_source_actions_phases_cards_and_editors_have_cases(self):
  source=inventory();self.assertFalse(source['unmapped_variants'])
  for kind in source['action_kinds']:self.assertIn('action.'+kind,self.tags)
  for phase in source['phases']:self.assertIn('phase.'+phase,self.tags)
  for editor in source['editors']:self.assertIn('editor.'+editor,self.tags)
  for card in source['cards']:self.assertIn('card.'+str(card),self.tags)
 def test_unique_ids_methods_and_expected_bytes(self):
  ids=[c['id'] for c in self.cases];self.assertEqual(len(ids),len(set(ids)))
  for c in self.cases:
   self.assertIn(c['method'],['delta','render','menu','status','input','combat','process'])
   self.assertIsInstance(c['expected']['text'],str);self.assertLessEqual(len(c['expected']['text'].splitlines()),c['max_lines'])
   for token in c['forbidden']:self.assertNotIn(token,c['expected']['text'],c['id'])
 def test_expected_text_is_not_generated_by_implementation(self):
  source=(HERE/'author_cases.py').read_text()
  self.assertNotIn('import narration',source);self.assertNotIn('from narration',source)
  self.assertNotIn('import main',source);self.assertNotIn('from main',source)
  self.assertEqual(self.proposal['approval'],'pending_explicit_human_approval')
 def test_engine_fixtures_are_conserved_and_sequential(self):
  for e in json.loads(gzip.decompress((HERE/'engine.json.gz').read_bytes())):
   self.assertEqual(e['origin'],'engine fixture; applied transitions and inventory audited')
   previous=e['before']
   for i,r in enumerate(e['rows'],1):
    self.assertEqual(r['sequence'],i);self.assertEqual(r['before'],previous);previous=r['after']
   self.assertEqual(previous,e['after'])
 def test_original_golden_remains_approved_and_unchanged(self):
  p=HERE.parent/'fixtures';expected=json.loads((p/'approval.json').read_text())['sha256']
  self.assertEqual(hashlib.sha256((p/'approved.json').read_bytes()).hexdigest(),expected)

 def test_approved_proposal_is_frozen(self):
  lock=json.loads((HERE/'proposal-lock.json').read_text())
  self.assertEqual(lock['approval'],'approved')
  self.assertEqual(hashlib.sha256((HERE/lock['candidate_file']).read_bytes()).hexdigest(),lock['sha256'])
 def test_no_identical_inputs_have_conflicting_expectations(self):
  seen={}
  for c in self.cases:
   key=json.dumps([c['method'],c['args'],c.get('inputs')],sort_keys=True)
   if key in seen:self.assertEqual(seen[key],c['expected'],c['id'])
   seen[key]=c['expected']
