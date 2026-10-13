"""Load frozen approvals and explicit precedence corrections, never runtime output."""
import gzip,hashlib,json
from pathlib import Path
HERE=Path(__file__).parent

def load_legacy_cases():
 approval=json.loads((HERE/'approval.json').read_text())
 raw=(HERE/'candidates.json.gz').read_bytes()
 assert hashlib.sha256(raw).hexdigest()==approval['approved_files']['candidates.json.gz']
 proposal=json.loads(gzip.decompress(raw))
 # The completed dialogue was approved after the earlier isolated examples.
 # These three wording corrections apply that later style to equivalent cases.
 overlay=json.loads((HERE/'supersessions.json').read_text())
 assert hashlib.sha256((HERE/'supersessions.json').read_bytes()).hexdigest()==approval['supersessions_sha256']
 cases={c['id']:c for c in proposal['cases']}
 for change in overlay:
  c=cases[change['id']]
  assert c['expected']['text']==change['previous']
  c['expected']['text']=change['approved'];c['max_lines']=len(change['approved'].splitlines())
 proposal['approval']='approved'
 return proposal

def approved_directory():
 directory=HERE/'next-proposal'
 approval=json.loads((directory/'approval.json').read_text())
 assert approval['golden_approved'] and approval['implementation_authorized']
 raw=(directory/'manifest.json').read_bytes()
 assert hashlib.sha256(raw).hexdigest()==approval['manifest_sha256']
 manifest=json.loads(raw)
 assert manifest==approval['manifest']
 for name,digest in manifest['files'].items():
  assert hashlib.sha256((directory/name).read_bytes()).hexdigest()==digest,name
 return directory

def load_cases():
 proposal=json.loads(gzip.decompress((approved_directory()/'cases.json.gz').read_bytes()))
 # The approved explicit keep examples supersede four inherited examples
 # that still omit the same bot decision. Preserve both frozen source sets.
 cases={c['id']:c for c in proposal['cases']}
 keep=cases['policy-bot-opening-0-0']['expected']['text']
 for id in ('mulligan-1-0','process-mulligans-0','process-stale-progress'):
  c=cases[id];anchor='1枚をライブラリーの一番下に戻してください。残り1枚。\n'
  assert c['expected']['text'].count(anchor)==1
  c['expected']['text']=c['expected']['text'].replace(anchor,anchor+keep+'\n')
  c['max_lines']+=1
 c=cases['turn-bot-first']
 c['expected']['text']=keep+'\n'+c['expected']['text'];c['max_lines']+=1
 proposal['approval']='approved'
 return proposal

def load_full_game():
 return json.loads((approved_directory()/'full-game.json').read_text())
