"""Capture real worker boundaries for review; does not author expected output."""
import gzip,json,sys,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
sys.path.insert(0,str(ROOT/'cli'))
from main import Worker
records=[]
for starting in (0,1):
 with tempfile.TemporaryDirectory(prefix='mtg-ui-review-') as d:
  worker=Worker(ROOT/'engine/target/release/play-worker',Path(d)/'game.jsonl',7319,starting,800000)
  try:
   previous=None;state=worker.state
   records.append({'id':f'opening-{starting}','previous':None,'state':state,'input':None})
   for label,kind in [('mulligan-1','mulligan'),('bottom-1','bottom'),('mulligan-2','mulligan'),('bottom-2a','bottom'),('bottom-2b','bottom'),('keep-5','keep')]:
    selected=next(c for c in state['editor']['choices'] if c['action']['kind']==kind)
    previous=state;state=worker.act({'index':selected['index']})
    records.append({'id':f'{label}-{starting}','previous':previous,'state':state,'input':str(int(selected['index'])+1),'selection':{'index':selected['index']},'action':selected['action']})
  finally:worker.close()
out=Path(__file__).parent/'live.json.gz'
out.write_bytes(gzip.compress(json.dumps(records,ensure_ascii=False,separators=(',',':')).encode(),mtime=0))
print(f'{len(records)} live boundaries captured')
