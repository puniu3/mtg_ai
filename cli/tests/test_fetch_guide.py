"""Regression for fetch during Guide's trigger: real audited engine transitions.

Regenerate inputs only with:
 cargo run --release --manifest-path cli/tests/review/engine/Cargo.toml -- fetch-guide
Expected text below is specified independently of the formatter.
"""
import gzip,json,sys,unittest
from pathlib import Path
HERE=Path(__file__).parent
sys.path.insert(0,str(HERE.parent))
from narration import render,menu
from main import StreamOutput,choose_input
from test_streaming import progress_at

EXPECTED='''あなたは血染めのぬかるみを生け贄に捧げ、聖なる鋳造所をタップ状態で出しました。あなたのライフ：20→19。
あなたは感動的な眺望所を公開して手札に加えました。
戦闘ダメージで、あなたのライフ：19→17。

あなたの2ターン目・戦闘前メインまで進みました。乾燥台地を引きました。
ライフ：あなた17／bot19。
あなたの戦場：聖なる鋳造所。
botの戦場：山、ゴブリンの先達（2/2）。両方タップ。
手札8枚：溶岩の撃ち込み、裂け目の稲妻2枚、稲妻のらせん2枚、乾燥台地2枚、感動的な眺望所。
botの手札4枚。土地は未プレイ。
1. 何もせず優先権を渡す
2. 溶岩の撃ち込みを唱える（botに3点ダメージ）
3. 乾燥台地を出す
4. 感動的な眺望所を出す
5. 裂け目の稲妻を待機する
s: 全体状況'''

def cases():return json.loads(gzip.decompress((HERE/'fixtures/fetch-guide.json.gz').read_bytes()))
def expected(c):
 if c['id'].endswith('tapped'):return EXPECTED
 return EXPECTED.replace('をタップ状態で出しました。あなたのライフ：20→19。','を2点払って出しました。あなたのライフ：20→17。').replace('戦闘ダメージで、あなたのライフ：19→17。','戦闘ダメージで、あなたのライフ：17→15。').replace('ライフ：あなた17／bot19。','ライフ：あなた15／bot19。')

class FetchGuide(unittest.TestCase):
 def test_payment_reveal_combat_and_draw_remain_separate(self):
  for c in cases():
   with self.subTest(case=c['id']):
    self.assertEqual(render(c['previous'],c['state']).encode(),expected(c).encode())
    paid=c['id'].endswith('paid');rows=c['state']['timeline']
    entry=next(r for r in rows if r['action']['kind']=='shockland_entry')
    foundry=next(x for x in entry['after']['public']['battlefield'] if x['card']=='Sacred Foundry')
    self.assertEqual(foundry['tapped'],not paid)
    self.assertEqual(entry['after']['public']['life'][0],17 if paid else 19)
    self.assertEqual(c['state']['observation']['public']['life'][0],15 if paid else 17)
    options=menu(c['previous']);self.assertEqual(len(options),4)
    self.assertFalse(any('見つけない' in label for label,_ in options))
    selected=choose_input(c['previous'],read=lambda:'4' if paid else '3')
    self.assertEqual(selected,{'index':'3' if paid else '2'})
 def test_every_atomic_prefix_streams_without_misattributing_damage(self):
  for c in cases():
   with self.subTest(case=c['id']):
    chunks=[];display=StreamOutput(chunks.append);display.begin(c['previous'])
    for r in c['state']['timeline']:
     if r['sequence']==c['state']['sequence']:break
     display.progress(progress_at(c['state'],r))
     self.assertTrue(expected(c).startswith(''.join(chunks)),r['sequence'])
     if r['action']['kind']=='shuffle':
      self.assertEqual(''.join(chunks),expected(c).split('\n')[0])
    display.finish(c['state']);self.assertEqual(''.join(chunks),expected(c)+'\n')
