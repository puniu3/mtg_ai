"""Resolved Eidolon triggers must survive turn summaries and pending spell narration.
Inputs are audited engine captures; expected text is authored independently.
Regenerate inputs: cargo run --release --manifest-path cli/tests/review/engine/Cargo.toml -- eidolon-response
"""
import copy,gzip,json,sys,unittest
from pathlib import Path
HERE=Path(__file__).parent
sys.path.insert(0,str(HERE.parent))
from narration import render,human_action,describe_action,eidolon_changes
from main import StreamOutput
from test_streaming import progress_at

MENU='''1. 何もせず優先権を渡す
2. ボロスの魔除けを唱える（あなたのパーマネントにターン終了時まで破壊不能）
3. ボロスの魔除けを唱える（botに4点ダメージ）
4. ボロスの魔除けを唱える（自分の大歓楽の幻霊にターン終了時まで二段攻撃）
5. 灼陽大峡谷の能力を起動する（1マナを支払い、峡谷をタップして生け贄に捧げ、1枚引く）
s: 全体状況'''
EXPECTED={
'eidolon-bot-11':'''botの5ターン目・アップキープです。まだドロー前です。botの手札2枚。ライフ：あなた9／bot11。
大歓楽の幻霊の誘発で、botのライフ：13→11。
スタック（解決順）：botの裂け目の稲妻（対象：あなた）。
'''+MENU,
'eidolon-bot-9':'''裂け目の稲妻が解決しました。あなたのライフ：9→6。
botは山をタップし、批判家刺殺を唱えました。大歓楽の幻霊の誘発で、botのライフ：11→9。
botの5ターン目・戦闘前メインまで進みました。
スタック（解決順）：botの批判家刺殺（対象：あなた）。
'''+MENU,
'eidolon-bot-7':'''批判家刺殺が解決しました。あなたのライフ：6→3。
botは山をタップし、批判家刺殺を唱えました。大歓楽の幻霊の誘発で、botのライフ：9→7。
スタック（解決順）：botの批判家刺殺（対象：あなた）。
'''+MENU}

def cases():return json.loads(gzip.decompress((HERE/'fixtures/eidolon-response.json.gz').read_bytes()))
def frames():return [c for c in cases() if c['id'] in EXPECTED]

# Superseded verbatim by the approved display-policy proposal. Keep the
# historical literals above as provenance; never derive expectations from UI.
sys.path.insert(0,str(HERE/'review'))
from contracts import load_cases
EXPECTED={c['id']:c['expected']['text'] for c in load_cases()['cases'] if c['id'] in EXPECTED}

class EidolonNarration(unittest.TestCase):
 def test_reported_three_boundaries_match_bytes_and_exactly_one_trigger_each(self):
  for c in frames():
   with self.subTest(case=c['id']):
    text=render(c['previous'],c['state'])
    self.assertEqual(text.encode(),EXPECTED[c['id']].encode())
    self.assertEqual(text.count('大歓楽の幻霊の誘発で、'),1)
 def test_all_atomic_prefixes_are_consistent_and_never_announce_future_damage(self):
  for c in frames():
   with self.subTest(case=c['id']):
    parts=[];display=StreamOutput(parts.append);display.begin(c['previous'])
    for row in c['state']['timeline']:
     if not c['previous']['sequence']<row['sequence']<c['state']['sequence']:continue
     display.progress(progress_at(c['state'],row))
     self.assertTrue(EXPECTED[c['id']].startswith(''.join(parts)))
    display.finish(c['state'])
    self.assertEqual(''.join(parts),EXPECTED[c['id']]+'\n')
 def test_both_casters_both_controllers_pending_and_resolved_spells(self):
  cs=[c for c in cases() if c['id'].startswith('eidolon-cast-')]
  self.assertEqual(len(cs),32)
  for c in cs:
   for dialogue in (False,True):
    with self.subTest(case=c['id'],dialogue=dialogue):
     caster=c['actor'];label='あなた' if caster==0 else 'bot'
     needle=f'大歓楽の幻霊の誘発で'+('、' if dialogue else '')+f'{label}のライフ：20→18。'
     text=describe_action(c['rows'],dialogue)
     self.assertEqual(text.count(needle),1,text)
     if c['id'].endswith('pending'):
      self.assertTrue(c['after']['public']['stack'])
      self.assertNotIn('が解決しました',text)
 def test_turn_summary_without_upkeep_stop_keeps_trigger_damage(self):
  first,second,_=frames();s=second['state']
  row=next(r for r in s['timeline'] if r['after']['phase']=='precombat_main' and r['sequence']>first['state']['sequence'])
  state=progress_at(s,row)
  text=render(first['previous'],state)
  self.assertEqual(text.count('大歓楽の幻霊の誘発で、botのライフ：13→11。'),1)
 def test_life_cost_with_trigger_still_pending_is_not_trigger_damage(self):
  c=next(c for c in cases() if c['id'].startswith('eidolon-cast-'))
  r=copy.deepcopy(next(r for r in c['rows'] if eidolon_changes([r])))
  r['after']['public']['stack']=copy.deepcopy(r['before']['public']['stack'])
  r['action']={'kind':'tap_for_mana'}
  self.assertEqual(eidolon_changes([r]),'')
