#!/usr/bin/env python3
"""Execute the approved display contract. Mismatches fail; never update goldens."""
import gzip,contextlib,copy,difflib,hashlib,io,json,sys,os,select,signal,subprocess,tempfile,time
from pathlib import Path
from unittest.mock import patch
from contracts import load_cases,approved_directory
HERE=Path(__file__).parent;ROOT=HERE.parents[2]
sys.path.insert(0,str(ROOT/'cli'))
import narration
import main as client

def process_case(case):
 with tempfile.TemporaryDirectory(prefix='mtg-ui-wire-') as directory:
  d=Path(directory);(d/'wire.json').write_text(json.dumps(case['args']['wire']))
  env={**os.environ,'CLI_REVIEW_WIRE':str(d/'wire.json'),'CLI_REVIEW_SESSION':str(d/'session'),'CLI_REVIEW_REQUESTS':str(d/'requests')}
  command=[str(ROOT/'play-cli'),'--worker',str(HERE/'wire_worker.py')]
  p=subprocess.Popen(command,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env,bufsize=0)
  prefix=b''
  try:
   if case.get('inputs')==['INTERRUPT']:
    deadline=time.monotonic()+5
    while not prefix.endswith('s: 全体状況\n'.encode()):
     if time.monotonic()>deadline:raise AssertionError('prompt never appeared')
     ready,_,_=select.select([p.stdout],[],[],.2)
     if ready:
      chunk=os.read(p.stdout.fileno(),65536)
      if not chunk:raise AssertionError('exited before prompt')
      prefix+=chunk
    p.send_signal(signal.SIGINT);stdout,stderr=p.communicate(timeout=5)
   else:
    inputs=case.get('inputs',[]);data=''.join(str(x)+'\n' for x in inputs if x is not None).encode()
    stdout,stderr=p.communicate(data,timeout=10)
  finally:
   if p.poll() is None:p.kill();p.wait()
  session=Path((d/'session').read_text()) if (d/'session').exists() else None
  requests=(d/'requests').read_text().splitlines() if (d/'requests').exists() else []
  return {'text':(prefix+stdout).decode(),'stderr':stderr.decode(),'exitcode':p.returncode,'request_count':len(requests),'cleanup':session is None or not session.parent.exists()}

def evaluate(case):
 a=copy.deepcopy(case['args']);process=None;unchanged=copy.deepcopy(a);selection=None;method=case['method']
 if method=='process':process=process_case(case);actual=process['text']
 elif method=='delta':actual=narration.human_action(a['rows'],a['after']) if a['actor']==0 else narration.bot_actions(a['rows'])
 elif method=='render':actual=narration.render(a['previous'],a['state'])
 elif method=='render_progress':actual=narration.render(a['previous'],a['state'],progress=True)
 elif method=='menu':actual=narration.menu_text(a['state'])
 elif method=='status':actual=narration.status_text(a['state'])
 elif method in ('input','combat'):
  inputs=iter(case['inputs']);out=[]
  def read(*args):
   try:value=next(inputs)
   except StopIteration:raise AssertionError('unexpected extra prompt')
   if value is None:raise EOFError
   return value
  if method=='input':selection=client.choose_input(a['state'],read=read,write=out.append)
  else:
   with patch('builtins.input',read),patch.object(client,'emit',out.append):selection=client.combat_editor(a['state'],a['kind'])
  actual='\n'.join(out)
 else:raise AssertionError('unhandled test method '+method)
 errors=[]
 if method in ('input','combat') and next(inputs,'CONSUMED')!='CONSUMED':errors.append('unused scripted input')
 expected=case['expected']['text']
 if actual.encode('utf-8')!=expected.encode('utf-8'):errors.append('UTF-8 bytes differ')
 if 'selection' in case['expected'] and selection!=case['expected']['selection']:errors.append('wrong engine selection')
 if a!=unchanged:errors.append('input observer state mutated')
 if process:
  for key in ('stderr','exitcode','request_count'):
   if process[key]!=case['expected'][key]:errors.append('wrong '+key)
  if not process['cleanup']:errors.append('temporary game retained')
 if len(actual.splitlines())>case['max_lines']:errors.append('too many narration lines')
 for token in case['forbidden']:
  if token in actual:errors.append('forbidden text: '+token)
 return {'id':case['id'],'pass':not errors,'errors':errors,'actual':actual,'process':process,'selection':selection,'expected_bytes':len(expected.encode()),'actual_bytes':len(actual.encode()),'diff':''.join(difflib.unified_diff(expected.splitlines(True),actual.splitlines(True),fromfile='proposed',tofile='current'))}

def main():
 proposal=load_cases();results=[]
 for case in proposal['cases']:
  try:results.append(evaluate(case))
  except Exception as e:results.append({'id':case['id'],'pass':False,'errors':[type(e).__name__+': '+str(e)],'actual':None})
 report={'approval':proposal['approval'],'candidate_sha256':hashlib.sha256((approved_directory()/'cases.json.gz').read_bytes()).hexdigest(),'precedence':'Explicit approved bot keep examples supersede four inherited omissions; see contracts.py. Frozen originals remain unchanged.','passed':sum(r['pass'] for r in results),'failed':sum(not r['pass'] for r in results),'results':results}
 output=Path(sys.argv[1]) if len(sys.argv)>1 else ROOT/'artifacts/cli-coverage-review/results.json'
 output.parent.mkdir(parents=True,exist_ok=True);output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
 print(f"Approved contracts: {report['passed']} pass, {report['failed']} fail. Approval: {report['approval']}")
 return int(bool(report['failed']))
if __name__=='__main__':sys.exit(main())
