"""Approved bot opening and automatic upkeep survive every worker update."""
import sys,unittest
from pathlib import Path
HERE=Path(__file__).parent
sys.path[:0]=[str(HERE.parent),str(HERE/'review')]
from contracts import load_cases
from main import StreamOutput
from test_streaming import progress_at

class PolicyStreaming(unittest.TestCase):
 def test_automatic_upkeep_prefixes_preserve_chronology(self):
  for c in load_cases()['cases']:
   if not c['id'].startswith('policy-bot-rift-auto-'):continue
   with self.subTest(case=c['id']):
    previous,state=c['args']['previous'],c['args']['state']
    expected=c['expected']['text'];chunks=[];stream=StreamOutput(chunks.append);stream.begin(previous)
    for row in state['timeline']:
     if row['sequence']<=previous['sequence']:continue
     stream.progress(progress_at(state,row));text=''.join(chunks)
     self.assertTrue(expected.startswith(text),row['sequence'])
     if row['after']['public']['life'][0]==20:self.assertNotIn('あなたのライフ：20→17',text)
     if row['after']['public']['life'][1]==20:self.assertNotIn('botのライフ：20→18',text)
    self.assertEqual(''.join(chunks),expected)
    self.assertNotIn('s: 全体状況',expected)

 def test_bot_opening_decisions_are_not_repeated_or_hidden(self):
  from narration import render
  for c in load_cases()['cases']:
   if not c['id'].startswith('policy-bot-opening-'):continue
   with self.subTest(case=c['id']):
    rows=c['args']['rows'];before=rows[0]['before'];after=c['args']['after']
    previous={'sequence':0,'observation':before}
    state={'sequence':rows[-1]['sequence'],'starting':int(c['id'].split('-')[-2]),'status':'human','observation':after,'timeline':rows,'editor':None}
    text=render(previous,state)
    for line in c['expected']['text'].splitlines():self.assertEqual(text.count(line),1,text)
    self.assertNotIn('botは手札1枚をライブラリー',text)

if __name__=='__main__':unittest.main()
