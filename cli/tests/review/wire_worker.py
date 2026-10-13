#!/usr/bin/env python3
"""Protocol double. Reads only wire states and expected requests, never UI goldens."""
import json,os,sys,time
from pathlib import Path
wire=json.loads(Path(os.environ['CLI_REVIEW_WIRE']).read_text())
Path(os.environ['CLI_REVIEW_SESSION']).write_text(sys.argv[1])
def emit(messages):
 for m in messages:
  if 'wait_for_file' in m:
   deadline=time.monotonic()+10
   while not Path(m['wait_for_file']).exists():
    if time.monotonic()>deadline:raise AssertionError('stream release never arrived')
    time.sleep(.01)
   continue
  print(m['raw'] if isinstance(m,dict) and 'raw' in m else json.dumps(m),flush=True)
emit(wire['initial'])
if wire.get('exit_after_initial'):sys.exit(0)
for step in wire.get('steps',[]):
 line=sys.stdin.readline()
 if not line:sys.exit(0)
 request=json.loads(line)
 with open(os.environ['CLI_REVIEW_REQUESTS'],'a') as f:f.write(json.dumps(request)+'\n')
 for key,value in step['expect'].items():assert request[key]==value,(key,request,value)
 emit(step['responses'])
for line in sys.stdin:
 with open(os.environ['CLI_REVIEW_REQUESTS'],'a') as f:f.write(line)
 raise AssertionError('unexpected request')
