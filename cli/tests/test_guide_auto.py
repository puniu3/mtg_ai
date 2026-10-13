"""Guide and combat remain visible with no human decision throughout combat.

Inputs: cargo run --release --manifest-path cli/tests/review/engine/Cargo.toml -- guide-auto
The generator asserts every decision belongs to bot and audits every transition.
"""
import gzip,json,sys,unittest
from pathlib import Path
HERE=Path(__file__).parent
sys.path.insert(0,str(HERE.parent))
from main import StreamOutput
from narration import render
from test_streaming import progress_at

class GuideAuto(unittest.TestCase):
 def test_reveal_and_damage_visible_without_human_decision(self):
  cases=json.loads(gzip.decompress((HERE/'fixtures/guide-auto.json.gz').read_bytes()))
  for c in cases:
   with self.subTest(case=c['id']):
    land=c['id'].endswith('land')
    reveal=('感動的な眺望所を公開して手札に加えました。' if land else
            '稲妻を公開し、ライブラリーの一番上に残しました。')
    text=render(c['previous'],c['state'],progress=True)
    self.assertIn(reveal,text)
    self.assertIn('ブロックされず、あなたのライフ：20→18。',text)
    self.assertLess(text.index(reveal),text.index('20→18'))
    self.assertEqual(c['state']['observation']['public']['life'][0],18)
    chunks=[];stream=StreamOutput(chunks.append);stream.begin(c['previous'])
    for row in c['state']['timeline']:
     stream.progress(progress_at(c['state'],row))
     self.assertTrue(text.startswith(''.join(chunks)),row['sequence'])
     if row['after']['public']['life'][0]==20:
      self.assertNotIn('20→18',''.join(chunks))
    self.assertEqual(''.join(chunks),text)
    self.assertNotIn('s: 全体状況',text)

if __name__=='__main__':unittest.main()
