"""Source-derived coverage denominator; additions to engine enums fail the guard."""
import gzip,json,re
from pathlib import Path
HERE=Path(__file__).parent;ROOT=HERE.parents[2]
ACTION_MAP={'Pass':['pass'],'Land':['play_land'],'Cast':['cast'],'Tap':['tap_for_mana'],'Pay':['pay_mana'],'Fetch':['fetch'],'LandPlan':['play_shockland'],'FetchPlan':['fetch_plan'],'SearchPlan':['search_plan'],'Canyon':['sacrifice_canyon_to_draw'],'Suspend':['suspend'],'Shock':['shockland_entry'],'Search':['fetch_selection'],'Shuffle':['shuffle'],'OrderTrigger':['order_trigger'],'Attack':['attack'],'Block':['block'],'Damage':['assign_damage'],'SuspendTarget':['suspend_target'],'DeclineSuspend':['decline_suspend'],'Mulligan':['mulligan','keep'],'Bottom':['bottom'],'Discard':['discard']}
def enum_body(source,name):
 start=source.index('pub enum '+name+' {');i=source.index('{',start);depth=1;j=i+1
 while depth:
  depth+=(source[j]=='{')-(source[j]=='}');j+=1
 return source[i+1:j-1]
def inventory():
 lib=(ROOT/'engine/src/lib.rs').read_text();view=(ROOT/'engine/src/harness/view.rs').read_text();actions=(ROOT/'engine/src/harness/actions.rs').read_text()
 variants=re.findall(r'^    ([A-Z]\w*)\s*(?:\{|,)',enum_body(lib,'Action'),re.M)
 phase_body=view[view.index('out["phase"] = json!(match progress.phase'):view.index('out["public"] = self.board_view();',view.index('out["phase"] = json!(match progress.phase'))]
 phases=sorted(set(re.findall(r'=> "([a-z_]+)"',phase_body)+['mulligan','opening']))
 editors=sorted(set(re.findall(r'"kind":"([a-z]+)"',actions[actions.index('pub fn editor('):actions.index('pub fn editor_index(')])))
 return {'action_variants':variants,'action_kinds':sorted({k for ks in ACTION_MAP.values() for k in ks}),'unmapped_variants':sorted(set(variants)-set(ACTION_MAP)),'phases':phases,'editors':editors,'cards':list(range(16))}
def original_coverage():
 states=json.loads(gzip.decompress((HERE.parent/'fixtures/observations.json.gz').read_bytes()));kinds=set();phases=set();editors=set();cards=set()
 for s in states:
  if s['status']=='human':phases.add(s['observation']['phase'])
  if s.get('editor'):editors.add(s['editor']['kind'])
  for r in s.get('timeline',[]):
   kinds.add(r['action']['kind'])
   if 'card_id' in r['action']:cards.add(r['action']['card_id'])
 return {'action_kinds_observed':sorted(kinds),'phases_at_input_boundaries':sorted(phases),'editors_observed':sorted(editors),'cards_observed_as_actions':sorted(cards),'warning':'Presence in a transcript is not separate branch/combination coverage.'}
if __name__=='__main__':print(json.dumps({'inventory':inventory(),'original':original_coverage()},ensure_ascii=False,indent=2))
