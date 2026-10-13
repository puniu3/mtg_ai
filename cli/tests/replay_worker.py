#!/usr/bin/env python3
"""Recorded observer transport: validates actions, never loads expected UI text."""
import gzip,json,os,sys
from pathlib import Path
p=Path(__file__).parent/'fixtures'
states=json.loads(gzip.decompress((p/'observations.json.gz').read_bytes()))
requests=json.loads((p/'requests.json').read_text())
if os.environ.get('CLI_TEST_SESSION_PATH'):
 Path(os.environ['CLI_TEST_SESSION_PATH']).write_text(sys.argv[1])
print(json.dumps({'state':states[0]}),flush=True)
for expected,state in zip(requests,states[1:]):
 line=sys.stdin.readline()
 if not line:break
 actual=json.loads(line)
 assert actual['op']==expected['op'] and actual['sequence']==expected['sequence'] and actual['selection']==expected['selection'],(actual,expected)
 print(json.dumps({'state':state}),flush=True)
