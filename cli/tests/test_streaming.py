"""Streaming must show confirmed text before the bot finishes, without changing bytes."""
import copy,gzip,json,os,select,subprocess,sys,tempfile,time,unittest
from pathlib import Path
HERE=Path(__file__).parent;ROOT=HERE.parents[1]
sys.path.insert(0,str(HERE.parent))
from main import StreamOutput
from narration import render
REVIEW=HERE/'review'
sys.path.insert(0,str(REVIEW))
from contracts import load_full_game

def capture():
 gold=load_full_game()
 records=json.loads(gzip.decompress((REVIEW/'full-game-capture.json.gz').read_bytes()))['records']
 return gold,records

def progress_at(state,row):
 return {**state,'sequence':row['sequence'],'status':'thinking','outcome':None,'editor':None,'observation':row['after'],'timeline':[r for r in state['timeline'] if r['sequence']<=row['sequence']]}

class Streaming(unittest.TestCase):
 def test_every_recorded_atomic_prefix_preserves_approved_bytes(self):
  gold,records=capture();count=0
  for i in range(1,len(records)):
   previous=records[i-1]['state'];state=records[i]['state'];chunks=[];display=StreamOutput(chunks.append);display.begin(previous)
   for row in state['timeline']:
    if previous['sequence']<row['sequence']<state['sequence'] and not row.get('outcome'):
     progress=progress_at(state,row)
     with self.subTest(frame=i,sequence=row['sequence']):
      display.progress(progress)
      self.assertTrue((render(previous,state)+'\n').startswith(''.join(chunks)))
      before=list(chunks);display.progress(progress);self.assertEqual(chunks,before)
     count+=1
   display.finish(state)
   wanted=next(f['output'] for f in gold['frames'] if f['capture_frame']==i and f['input']!='s')
   self.assertEqual(''.join(chunks),wanted)
  self.assertGreater(count,200)
 def test_real_cli_flushes_before_delayed_bot_boundary(self):
  gold,records=capture();state=records[6]['state'];previous=records[5]['state']
  row=next(r for r in state['timeline'] if r['sequence']>previous['sequence'] and r['after'].get('turn')==3 and r['after']['phase']=='precombat_main')
  progress=progress_at(state,row)
  with tempfile.TemporaryDirectory(prefix='mtg-stream-test-') as directory:
   d=Path(directory);release=d/'release';steps=[]
   for i,r in enumerate(records[1:],1):
    responses=[{'state':r['state']}]
    if i==6:responses=[{'state':progress},{'wait_for_file':str(release)},*responses]
    steps.append({'expect':{'op':'act','sequence':records[i-1]['state']['sequence'],'selection':r['selection']},'responses':responses})
   (d/'wire').write_text(json.dumps({'initial':[{'state':records[0]['state']}],'steps':steps}))
   env={**os.environ,'CLI_REVIEW_WIRE':str(d/'wire'),'CLI_REVIEW_SESSION':str(d/'session'),'CLI_REVIEW_REQUESTS':str(d/'requests')}
   p=subprocess.Popen([str(ROOT/'play-cli'),'--worker',str(REVIEW/'wire_worker.py')],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env)
   data=''.join(f['input']+'\n' for f in gold['frames'] if f['input'] is not None).encode();prefix=b''
   try:
    p.stdin.write(data);p.stdin.flush();deadline=time.monotonic()+5
    needle='botの2ターン目・戦闘前メインまで進みました。botは公開済みの溶岩の撃ち込みを引き、手札6枚。ライフ：あなた19／bot17。'.encode()
    while needle not in prefix and time.monotonic()<deadline:
     ready,_,_=select.select([p.stdout],[],[],.1)
     if ready:
      chunk=os.read(p.stdout.fileno(),65536)
      if not chunk:break
      prefix+=chunk
    self.assertIn(needle,prefix,'Confirmed attack and turn heading must arrive while the bot boundary is withheld')
    self.assertNotIn('botは感動的な眺望所を出しました。'.encode(),prefix)
    self.assertFalse(release.exists());self.assertIsNone(p.poll())
    release.touch();tail,err=p.communicate(timeout=10)
    self.assertEqual(p.returncode,0,err.decode());self.assertEqual(err,b'')
    self.assertEqual(prefix+tail,''.join(f['output'] for f in gold['frames']).encode())
    self.assertEqual(len((d/'requests').read_text().splitlines()),22)
    self.assertFalse(Path((d/'session').read_text()).parent.exists())
   finally:
    if p.poll() is None:p.kill()
    p.communicate()
