#!/usr/bin/env python3
"""Line-oriented MTG client; Python standard library only, existing Rust worker policy."""
import argparse,json,os,re,secrets,subprocess,sys,tempfile,uuid
from pathlib import Path
from narration import render,status_text,menu,menu_text,name,full
ROOT=Path(__file__).resolve().parents[1]

def emit(text):
 sys.stdout.write(text+'\n');sys.stdout.flush()

def number_input(raw, limit):
 if not re.fullmatch(r'[0-9]+\.?',raw):return None
 digits=raw.rstrip('.').lstrip('0') or '0'
 if len(digits)>len(str(limit)):return None
 value=int(digits)
 return value-1 if 1<=value<=limit else None

def choose_input(state,read=input,write=emit):
 options=menu(state)
 while True:
  try:raw=read().strip()
  except EOFError:return None
  if raw=='q':return None
  if raw=='s':write(status_text(state));continue
  number=number_input(raw,len(options))
  if number is not None:return options[number][1]
  write(f'選択肢にない番号です。1〜{len(options)} または s を入力してください。\n'+menu_text(state))

class StreamOutput:
 """Flush certified text prefixes; completion must preserve every emitted byte."""
 def __init__(self,write=None):
  self.write=write or self.stdout
  self.begin(None)
 @staticmethod
 def stdout(text):
  sys.stdout.write(text);sys.stdout.flush()
 def begin(self,previous):
  self.previous=previous;self.written='';self.sequence=-1
 def append(self,text):
  if not text.startswith(self.written):raise RuntimeError('確定済みの表示と対戦結果が一致しません。')
  tail=text[len(self.written):]
  if tail:self.write(tail)
  self.written=text
 def progress(self,state):
  if state['sequence']<=self.sequence:return
  self.sequence=state['sequence']
  text=render(self.previous,state,progress=True)
  # An automatic choice can temporarily hide an already certified tail.
  if self.written.startswith(text):return
  self.append(text)
 def finish(self,state):self.append(render(self.previous,state)+'\n')

class Worker:
 def __init__(self,binary,session,seed,starting,steps,progress=None):
  self.progress=progress
  self.error_file=open(session.parent/'worker.stderr','ab')
  self.process=subprocess.Popen([str(binary),str(session),str(seed),str(starting),str(steps),'--steps','--trace'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=self.error_file,text=True,encoding='utf-8')
  try:self.state=self.read_boundary(-1)
  except BaseException:
   self.close();raise
 def read_boundary(self,after):
  while True:
   line=self.process.stdout.readline()
   if not line:raise RuntimeError('対戦ワーカーが停止しました。')
   try:
    row=json.loads(line)
    if row.get('error'):raise RuntimeError('対戦ワーカーがエラーを返しました：'+str(row['error']))
    s=row['state']
    if s.get('error'):raise RuntimeError('対戦ワーカーがエラーを返しました：'+str(s['error']))
    sequence=s['sequence'];status=s['status']
    if not isinstance(sequence,int) or not isinstance(status,str):raise ValueError('schema')
    if sequence>after and status in ('human','finished'):
     if not isinstance(s.get('observation'),dict):raise ValueError('observation')
     self.state=s;return s
    if sequence>after and status=='thinking' and self.progress:
     if not isinstance(s.get('observation'),dict):raise ValueError('observation')
     self.progress(s)
   except (ValueError,KeyError,TypeError,AttributeError) as e:
    raise RuntimeError('対戦ワーカーから不正な応答を受信しました。') from e
 def act(self,selection):
  old=self.state['sequence'];r={'op':'act','id':str(uuid.uuid4()),'sequence':old,'selection':selection}
  self.process.stdin.write(json.dumps(r)+'\n');self.process.stdin.flush()
  return self.read_boundary(old)
 def close(self):
  if self.process.poll() is None:
   self.process.stdin.close()
   try:self.process.wait(timeout=5)
   except subprocess.TimeoutExpired:self.process.terminate();self.process.wait(timeout=5)
  self.process.stdout.close();self.error_file.close()

def combat_editor(state,kind):
 e=state['editor'];lookup={c['object_id']:c for c in state['observation'].get('public',{}).get('battlefield',[])}
 def pick(title,labels):
  text=title+'\n'+'\n'.join(f'{i}. {x}' for i,x in enumerate(labels,1))+'\ns: 全体状況';emit(text)
  while True:
   try:v=input().strip()
   except EOFError:return None
   if v=='q':return None
   if v=='s':emit(full(state['observation'],True));emit(text);continue
   number=number_input(v,len(labels))
   if number is not None:return number
   emit('選択肢にない番号です。\n'+text)
 def label(oid):return name(lookup[oid]['card'])+f' #{oid}'
 if kind=='attack':
  ids=[]
  for oid in e['attackers']:
   selected=pick(label(oid)+'の攻撃', ['攻撃しない','攻撃する'])
   if selected is None:return None
   if selected:ids.append(oid)
  return {'attackers':ids}
 if kind=='block':
  pairs=[]
  for oid in e['blockers']:
   selected=pick(label(oid)+'のブロック',['ブロックしない']+[label(a)+'をブロック' for a in e['attackers']])
   if selected is None:return None
   if selected:pairs.append([oid,e['attackers'][selected-1]])
  return {'blocks':pairs}
 result=[]
 for g in e['groups']:
  left=g['power'];amounts=[]
  for i,target in enumerate(g['targets']):
   if i==len(g['targets'])-1:n=left
   else:
    n=pick(label(g['source'])+' → '+label(target)+'のダメージ',[str(n)+'点' for n in range(left+1)])
    if n is None:return None
   amounts.append(n);left-=n
  result.append(amounts)
 return {'damage':result}

def main():
 ap=argparse.ArgumentParser(description='番号で対戦。s: 全体状況、q / Ctrl-D: 終了。')
 ap.add_argument('--seed',type=int);ap.add_argument('--starting',type=int,choices=[0,1],default=0)
 ap.add_argument('--budget-steps',type=int,default=800000);ap.add_argument('--worker',type=Path,default=ROOT/'engine/target/release/play-worker')
 args=ap.parse_args()
 if args.budget_steps<=0:ap.error('--budget-steps は正数です')
 os.umask(0o077)
 if not args.worker.exists():raise RuntimeError('先に cargo build --release --manifest-path engine/Cargo.toml --bin play-worker を実行してください。')
 seed=args.seed if args.seed is not None else secrets.randbits(64)
 with tempfile.TemporaryDirectory(prefix='mtg-cli-') as directory:
  worker=None
  try:
   display=StreamOutput()
   worker=Worker(args.worker.resolve(),Path(directory)/'game.jsonl',seed,args.starting,args.budget_steps,progress=display.progress)
   state=worker.state;display.finish(state)
   while state['status']!='finished':
    selection=choose_input(state)
    if selection is None:break
    if 'edit' in selection:selection=combat_editor(state,selection['edit'])
    if selection is None:break
    previous=state;display.begin(previous)
    if 'primitive_selections' in selection:
     for sel in selection['primitive_selections']:state=worker.act(sel)
    else:state=worker.act(selection)
    display.finish(state)
  finally:
   if worker:worker.close()

if __name__=='__main__':
 try:main()
 except KeyboardInterrupt:print('\n終了しました。',file=sys.stderr)
 except (OSError,ValueError,RuntimeError) as e:print(str(e),file=sys.stderr);sys.exit(1)
