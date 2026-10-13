"""Author review candidates from frozen inputs. NEVER import the CLI formatter.
Running this authors a new proposal; it does not grant human approval.
"""
import copy,gzip,json,hashlib
from pathlib import Path
HERE=Path(__file__).parent
if (HERE/'approval.json').exists():raise SystemExit('Approved goldens are frozen; author a separate proposal for further changes.')
EN=['Goblin Guide','Monastery Swiftspear','Eidolon of the Great Revel','Lightning Bolt','Lava Spike','Rift Bolt','Skewer the Critics','Boros Charm','Searing Blaze','Lightning Helix','Arid Mesa','Bloodstained Mire','Sunbaked Canyon','Inspiring Vantage','Sacred Foundry','Mountain']
JA=['ゴブリンの先達','僧院の速槍','大歓楽の幻霊','稲妻','溶岩の撃ち込み','裂け目の稲妻','批判家刺殺','ボロスの魔除け','焼尽の猛火','稲妻のらせん','乾燥台地','血染めのぬかるみ','灼陽大峡谷','感動的な眺望所','聖なる鋳造所','山']
N=dict(zip(EN,JA));cases=[]
GROUPS={'opening':'初手・マリガン','land':'土地・フェッチ・ドロー','spell':'呪文・対象・誘発','combat':'戦闘・個別割り当て','turn':'進行・優先権','status':'全体状況・公開情報','input':'入力・終了・通信','menu':'行動メニュー','ending':'終局'}
def seat(p):return ['あなた','bot'][p]
def cards(xs):return '、'.join(N[x['card']]+(str(x['count'])+'枚' if x['count']>1 else '') for x in xs) or '空'
def pack(xs):return [{'card':EN[c],'card_id':c,'count':n} for c,n in xs]
def add(id,title,group,method,args,text,tags=(),origin='observer contract fixture',selection=None,max_lines=None,forbidden=(),inputs=None):
 expected={'text':text}
 if selection is not None:expected['selection']=selection
 c={'id':id,'title':title,'group':group,'origin':origin,'method':method,'args':args,'expected':expected,'covers':list(dict.fromkeys(tags)),'max_lines':max_lines if max_lines is not None else len(text.splitlines()),'forbidden':list(forbidden)}
 if inputs is not None:c['inputs']=inputs
 cases.append(c)
def menu(labels):return '\n'.join([f'{i}. {t}' for i,t in enumerate(labels,1)]+['s: 全体状況'])
PASS=menu(['何もせず優先権を渡す'])
def base():
 players=[{'hand':{'known':[],'size':0,'unknown_count':0},'graveyard':[],'ordinary_exile':[],'library':{'size':50,'known_positions':[]}} for _ in (0,1)]
 return {'schema':'burn-agent-view-v1','viewer':0,'actor':0,'active_player':0,'turn':3,'phase':'precombat_main','players':players,'public':{'life':[20,20],'battlefield':[],'stack':[],'suspended':[],'mana':[{k:0 for k in 'WUBRGC'} for _ in (0,1)],'this_turn':{'land_plays':[0,0],'land_entered':[False,False],'life_lost':[False,False]},'execution':{'kind':'priority','player':0,'consecutive_passes':0}}}
def state(o=None,actions=None,seq=10):
 return {'sequence':seq,'starting':0,'status':'human','outcome':None,'observation':o or base(),'editor':{'kind':'choices','choices':[{'index':str(i),'action':a} for i,a in enumerate(actions or [{'kind':'pass'}])]},'timeline':[]}
def perm(c,id,p=0,tapped=False,**kw):
 x={'card':EN[c],'card_id':c,'object_id':id,'owner':p,'controller':p,'count':1,'tapped':tapped,'indestructible':False}
 if c<3:x.update(power=[2,1,2][c],toughness=[2,2,2][c],damage_marked=0,summoning_sick=False,double_strike=False)
 x.update(kw);return x
def row(before,after,action,p=0,seq=11,outcome=None,events=()):return {'sequence':seq,'actor':p,'before':copy.deepcopy(before),'after':copy.deepcopy(after),'action':action,'events':list(events),'outcome':outcome}
def delta(id,title,group,rows,expected,tags=(),actor=0,origin='observer contract fixture'):
 if group=='ending':
  ps=state(rows[0]['before'],seq=0);end=state(rows[-1]['after'],seq=rows[-1]['sequence']);end['timeline']=rows;end['status']='finished';end['outcome']=rows[-1]['outcome'];end['editor']=None
  expected=expected.replace('。あなたの勝利です。','。\nあなたの勝利です。').replace('。あなたの敗北です。','。\nあなたの敗北です。')
  add(id,title,group,'render',{'previous':ps,'state':end},expected,tags,origin)
 else:
  add(id,title,group,'delta',{'rows':rows,'actor':actor,'after':rows[-1]['after']},expected,tags,origin,forbidden=['非公開','pay_mana','fetch_selection','shuffle','order_trigger'])


# Legal-engine event fixtures, including private bot fetch intent redaction.
engine=json.loads(gzip.decompress((HERE/'engine.json.gz').read_bytes()))
for e in engine:
 id=e['id'];p=e['actor'];who=seat(p);opp=seat(1-p);before=e['before'];after=e['after'];rows=e['rows'];a=rows[0]['action'];k=a['kind'];card=N.get(a.get('card'),'');tags=['actor.'+str(p)]+['action.'+r['action']['kind'] for r in rows];group='land';text=''
 if id.startswith('fetch-'):
  src=N[(a.get('source') or {})['card']];kind=id.split('-')[-1]
  if kind=='fail':text=f'{who}は{src}を生け贄に捧げました。土地は取得しませんでした。{who}のライフ：20→19。'
  else:
   dest='山' if kind=='mountain' else '聖なる鋳造所';entry='を2点払って出しました' if kind=='paid' else 'をタップ状態で出しました' if kind=='tapped' else 'を出しました'
   text=f'{who}は{src}を生け贄に捧げ、{dest}{entry}。{who}のライフ：20→{17 if kind=="paid" else 19}。'
  tags+=['fetch.'+kind,'fetch.collapsed','card.'+str(10 if '10-' in id else 11)]
 elif id.startswith(('land-','shock-','fastland-')):
  new=next(x for x in after['public']['battlefield'] if x['card']==a['card'] and x['object_id'] not in {y['object_id'] for y in before['public']['battlefield']})
  text=('botは' if p else '')+card+('をタップ状態で出しました。' if new['tapped'] else 'を出しました。')
  if before['public']['life'][p]!=after['public']['life'][p]:text+=f'{who}のライフ：20→18。'
  text+=('手札：1→0枚。' if not p else '')
  tags+=['land.'+('tapped' if new['tapped'] else 'untapped'),'card.'+str(EN.index(a['card']))]
 elif id.startswith('canyon-draw-'):
  text=f'{who}は灼陽大峡谷を生け贄に捧げ、'+('稲妻のらせんを引きました。' if p==0 else '1枚引きました。')+f'{who}の手札：0→1枚。'
  tags+=['canyon.draw','mana.pool_payment' if id.endswith('true') else 'mana.land_payment','card.12']
 elif id.startswith('suspend-'):
  text=('山をタップし、' if not p else 'botは山をタップし、')+'裂け目の稲妻を待機しました（時間カウンター1個）。'+('手札：1→0枚。' if not p else '')
  group='spell';tags+=['suspend.place','card.5']
 else:
  group='spell';c=EN.index(a['card']);tags+=['card.'+str(c)];paid=[N[r['action']['source']['card']] for r in rows if r['action']['kind']=='tap_for_mana'];payment='と'.join(paid)
  cost=before['public']['life'][p]-after['public']['life'][p] if not e['outcome'] else 0
  if id.startswith('lethal-spell-'):
   text=('botは' if p else '')+payment+'をタップし、稲妻が解決しました。'+('あなたの敗北です。botの勝利。' if p else 'あなたの勝利です。')
   tags+=['ending.spell','ending.stale_snapshot'];group='ending'
  elif c<3:
   creature=next(x for x in after['public']['battlefield'] if x['card']==a['card']);feature='召喚酔い' if c==2 else '速攻'
   text=('botは' if p else '')+payment+f'をタップし、{card}が戦場に出ました（{creature["power"]}/{creature["toughness"]}・{feature}）。'+('手札：1→0枚。' if not p else '')
   tags+=['creature.'+feature]
  elif c==7 and a.get('mode')!='4 damage to player':
   effect='自分のパーマネントがターン終了時まで破壊不能になりました' if a['mode']=='your permanents gain indestructible this turn' else f'{opp}のゴブリンの先達がターン終了時まで二段攻撃を得ました'
   text=f'{who}は{payment}をタップし、ボロスの魔除けを解決しました。{effect}。'+('手札：1→0枚。' if not p else '')
   tags+=['charm.indestructible' if 'indestructible' in a['mode'] else 'charm.double']
  elif c==8:
   n=3 if id.endswith('true') else 1;text=f'{who}は{payment}をタップし、焼尽の猛火が解決しました。{opp}のライフ：20→{20-n}。{opp}のゴブリンの先達に{n}点ダメージ。'
   text+=f'{opp}のゴブリンの先達が戦場を離れました。' if n==3 else ''
   text+=('手札：1→0枚。' if not p else '');tags+=['blaze.landfall' if n==3 else 'blaze.normal','target.two']
  elif a.get('targets') and 'object_id' in a['targets'][0]:
   text=f'{who}は{payment}をタップし、{card}が解決しました。{opp}のゴブリンの先達に3点ダメージ。{opp}のゴブリンの先達が戦場を離れました。'
   if c==9:text+=f'{who}のライフ：20→23。'
   text+=('手札：1→0枚。' if not p else '');tags+=['target.creature','creature.death']
  elif id.startswith('trigger-'):
   trigger=int(id.split('-')[-1]);text=f'{who}は山をタップし、稲妻を唱えました。'
   if trigger==2:text+=f'大歓楽の幻霊の誘発で{who}のライフ：20→18。'
   else:text+=f'{who}の僧院の速槍は果敢で2/3。'
   text+=f'稲妻が解決し、{opp}のライフ：20→17。'+('手札：1→0枚。' if not p else '')
   tags+=['trigger.eidolon' if trigger==2 else 'trigger.prowess']
  else:
   damage=4 if c==7 else 3
   text=f'{who}は{payment}をタップし、{card}'+('を絢爛で唱えました。' if c==6 and a['spectacle'] else 'を通常コストで唱えました。' if c==6 else 'が解決しました。')+f'{opp}のライフ：20→{20-damage}。'
   if c==9:text+=f'{who}のライフ：20→23。'
   text+=('手札：1→0枚。' if not p else '')
   tags+=['target.player']
   if c==6:tags+=['skewer.spectacle' if a['spectacle'] else 'skewer.normal']
   if c==7:tags+=['charm.damage']
   if c==9:tags+=['helix.heal']
 delta(id,f'{who}：{id}',group,rows,text,tags,p,e['origin'])

# Live repeated mulligans: enumerate *all* seven drawn cards, then bottom choices.
live=json.loads(gzip.decompress((HERE/'live.json.gz').read_bytes()))
for r in live:
 id=r['id'];s=r['state'];o=s['observation'];h=o['players'][0]['hand'];ns=cards(h['known']);text='';tags=['opening.'+str(s['starting']),'phase.mulligan']
 if id.startswith('opening'):
  text='バーン同士、あなたが'+('先手' if s['starting']==0 else '後手')+f'です。\n初手7枚：{ns}。\n'+menu(['キープする','マリガンする']);tags+=['mulligan.initial']
 elif id.startswith('mulligan'):
  n=o['mulligans_taken'][0];text=f'{n}回目のマリガン。7枚引き直しました。\n手札7枚：{ns}。\n{n}枚をライブラリーの一番下に戻してください。残り{n}枚。\n'
  text+=menu([N[c['action']['card']] for c in s['editor']['choices']]);tags+=['mulligan.redraw','mulligan.multiple' if n>1 else 'mulligan.first','action.mulligan','hand.duplicates']
 elif id.startswith('bottom'):
  n=o['mulligans_taken'][0];left=h['size']-(7-n)
  text=N[r['action']['card']]+f'をライブラリーの一番下に戻しました。手札：{h["size"]+1}→{h["size"]}枚。\n'+f'手札{h["size"]}枚：{ns}。\n'
  if left:text+=f'戻すカードは残り{left}枚です。\n'+menu([N[c['action']['card']] for c in s['editor']['choices']])
  else:text+=menu(['キープする','マリガンする'])
  tags+=['action.bottom','mulligan.partial_bottom' if left else 'mulligan.bottom_done']
 else:continue # The continuing game is covered by the frozen approved transcript and turn-boundary cases below.
 add(id,id,'opening','render',{'previous':r['previous'],'state':s},text,tags,'real worker capture; seed 7319, 800000 steps',inputs=[r['input']] if r['input'] else [])

# Menu action alphabet: explicit user-facing labels and selection identities.
labels=[({'kind':'pass'},'何もせず優先権を渡す'),({'kind':'keep'},'キープする'),({'kind':'mulligan'},'マリガンする'),({'kind':'decline_suspend'},'裂け目の稲妻を唱えず、追放領域に残す'),({'kind':'suspend','card':EN[5]},'裂け目の稲妻を待機する')]
for c in range(10,16):
 if c==14:
  for pay in (False,True):labels.append(({'kind':'play_shockland','card':EN[c],'pay_2_life':pay},('2点のライフを支払って聖なる鋳造所を出す' if pay else '聖なる鋳造所をタップ状態で出す')))
 else:labels.append(({'kind':'play_land','card':EN[c]},JA[c]+'を出す'))
for k in ('bottom','discard'):
 labels.append(({'kind':k,'card':EN[15]},'山' if k=='bottom' else '山を捨てる'))
for src in (10,11):
 for dest,pay in [(15,False),(14,False),(14,True),(None,False)]:
  for kind in ('fetch_plan','search_plan'):
   a={'kind':kind,'source':{'card':EN[src],'object_id':9},'card':EN[dest] if dest is not None else None,'pay_2_life':pay}
   label=(JA[src]+'を起動して' if kind=='fetch_plan' else '')+('土地を見つけない' if dest is None else JA[dest]+('をタップ状態で出す' if dest==14 and not pay else 'を出す'))+('（追加で2点支払い）' if pay else '')
   labels.append((a,label))
labels+=[({'kind':'sacrifice_canyon_to_draw','source':{'card':EN[12],'object_id':8}},'灼陽大峡谷の能力を起動する（1マナを支払い、峡谷をタップして生け贄に捧げ、1枚引く）')]
for i,(action,label) in enumerate(labels):
 s=state(actions=[action]);kind=action['kind'];tags=['menu.'+kind,'action.'+kind,'editor.choices']
 add(f'menu-{i:02}',label,'menu','menu',{'state':s},menu([label]),tags)
# Direct targeting menus, modes, alternate cost, and permanent identity disambiguation.
for card in range(10):
 target={'player':1};a={'kind':'cast','card':EN[card],'targets':[] if card<3 else [target]};label=JA[card]+'を唱える'
 if card==7:a['mode']='4 damage to player';label+='（botに4点ダメージ）'
 elif card==8:a['targets']+=[{'object_id':7,'controller':1,'card':EN[0]}];label+='（botとbotのゴブリンの先達に各1点ダメージ）'
 elif card==6:a['spectacle']=False;label=JA[card]+'を通常コストで唱える（3マナ、botに3点ダメージ）'
 elif card>=3:label+='（botに3点ダメージ'+('、あなたは3点回復' if card==9 else '')+'）'
 add(f'menu-cast-{card}',label,'menu','menu',{'state':state(actions=[a])},menu([label]),['menu.cast','card.'+str(card)])
for target,label in [({'player':1},'bot'),({'object_id':7,'card':EN[0],'controller':1},'botのゴブリンの先達')]:
 add('menu-suspend-'+label,label,'menu','menu',{'state':state(actions=[{'kind':'suspend_target','target':target}])},menu([label+'を対象に裂け目の稲妻を唱える（マナ不要、3点ダメージ）']),['menu.suspend_target','action.suspend_target'])

# Fixed whole-state base; each test varies a public facet independently.
BASE_STATUS='あなたの2ターン目・戦闘前メイン。ライフ：あなた20／bot20。\n戦場：双方とも空。\n手札0枚：空。\nbotの手札0枚。\nあなたの墓地：空。botの墓地：空。\nライブラリー：あなた50枚／bot50枚。\n土地は未プレイ。\n'+PASS
add('status-empty','空のスタック・待機・マナを省略','status','status',{'state':state()},BASE_STATUS,['status.empty','status.graveyard_only_s','status.library'],forbidden=['スタック：','待機：','マナプール','非公開','アンタップ'])
for field in ('damage','sick','double','indestructible','tapped','duplicates'):
 o=base();c=perm(0,7);label='ゴブリンの先達（2/2）'
 if field=='damage':c['damage_marked']=1;label='ゴブリンの先達（2/2・1点ダメージ）'
 if field=='sick':c=perm(2,7,summoning_sick=True);label='大歓楽の幻霊（2/2・召喚酔い）'
 if field=='double':c['double_strike']=True;label='ゴブリンの先達（2/2・二段攻撃）'
 if field=='indestructible':c['indestructible']=True;label='ゴブリンの先達（2/2・破壊不能）'
 if field=='tapped':c['tapped']=True;label+='（タップ）'
 o['public']['battlefield']=[c]
 if field=='duplicates':o['public']['battlefield'].append(perm(0,8));label='ゴブリンの先達 #7（2/2）、ゴブリンの先達 #8（2/2）'
 text=BASE_STATUS.replace('戦場：双方とも空。','あなたの戦場：'+label+'。\nbotの戦場：空。')
 add('status-'+field,field,'status','status',{'state':state(o)},text,['status.'+field])
for p in (0,1):
 o=base();o['public']['mana'][p]['R']=2;text=BASE_STATUS.replace('土地は未プレイ。',seat(p)+'のマナプール：R2。\n土地は未プレイ。')
 add('status-mana-'+str(p),'マナ残存','status','status',{'state':state(o)},text,['status.mana','actor.'+str(p)])
 o=base();o['public']['suspended']=[{'object_id':8,'owner':p,'card':EN[5],'time_counters':1}];text=BASE_STATUS.replace('土地は未プレイ。',f'待機：{seat(p)}の裂け目の稲妻（時間カウンター1個）。\n土地は未プレイ。')
 add('status-suspend-'+str(p),'待機残存','status','status',{'state':state(o)},text,['status.suspend'])
o=base();o['players'][1]['hand']={'size':4,'known':pack([(3,2)]),'unknown_count':2};text=BASE_STATUS.replace('botの手札0枚。','botの手札4枚：うち稲妻2枚が既知。')
add('status-known-hand','公開済みカードだけ注記','status','status',{'state':state(o)},text,['knowledge.hand_counts'],forbidden=['非公開'])
o=base();o['players'][1]['hand']={'size':4,'known':[],'unknown_count':4};text=BASE_STATUS.replace('botの手札0枚。','botの手札4枚。')
add('status-unknown-hand','枚数だけ表示','status','status',{'state':state(o)},text,['knowledge.unknown'],forbidden=['非公開'])
o=base();o['players'][1]['library']['known_positions']=[{'card':EN[3],'card_id':3,'position_from_top':1}];text=BASE_STATUS.replace('土地は未プレイ。','botのライブラリーの一番上：稲妻（公開済み）。\n土地は未プレイ。')
add('status-known-top','公開済みライブラリートップ','status','status',{'state':state(o)},text,['knowledge.library_top'])
o=base();o['public']['stack']=[{'object_id':21,'kind':'spell','card':EN[3],'controller':0,'targets':[{'player':1}]},{'object_id':20,'kind':'spell','card':EN[9],'controller':1,'targets':[{'object_id':7,'card':EN[0],'controller':0}]}];text=BASE_STATUS.replace('土地は未プレイ。','スタック（次に解決する順）：あなたの稲妻（bot）→ botの稲妻のらせん（あなたのゴブリンの先達）。\n土地は未プレイ。')
add('status-stack-order','スタック順と対象','status','status',{'state':state(o)},text,['status.stack','stack.order','target.identity'])
o=base();o['players'][0]['graveyard']=pack([(3,2)]);o['players'][1]['ordinary_exile']=pack([(5,1)]);text=BASE_STATUS.replace('あなたの墓地：空。','あなたの墓地：稲妻2枚。').replace('土地は未プレイ。','botの追放領域：裂け目の稲妻。\n土地は未プレイ。')
add('status-zones','墓地・待機以外の追放','status','status',{'state':state(o)},text,['status.graveyard','status.exile'])

# Input syntax and side effects. These cases call the actual input loop, not a substitute parser.
for raw in ['1','1.',' 1 ','01']:
 add('input-valid-'+repr(raw),'数字入力 '+repr(raw),'input','input',{'state':state()},'', ['input.number','input.whitespace','input.trailing_dot'],selection={'index':'0'},max_lines=0,inputs=[raw])
for i,raw in enumerate(['','0','-1','2','99','x','1.0','1..','１２','99999999999999999999999999999999']):
 text='選択肢にない番号です。1〜1 または s を入力してください。\n'+PASS
 add(f'input-invalid-{i}',repr(raw),'input','input',{'state':state()},text,['input.invalid'],selection={'index':'0'},inputs=[raw,'1'])
for key in ['q',None]:
 add('input-exit-'+str(key),'終了 '+str(key),'input','input',{'state':state()},'',['input.quit' if key else 'input.eof'],inputs=[key]);cases[-1]['expected']['selection']=None
add('input-status-repeat','sを連続入力しても状態・番号は変わらない','input','input',{'state':state()},BASE_STATUS+'\n'+BASE_STATUS,['input.s','input.no_mutation'],selection={'index':'0'},inputs=['s','s','1'])

# Combat editors: small numbered prompts rather than exponential subset lists.
combat_o=base();combat_o['phase']='declare_attackers';combat_o['public']['battlefield']=[perm(0,7),perm(0,8),perm(2,9,1)]
s=state(combat_o);s['editor']={'kind':'attack','attackers':[7,8]}
add('attack-menu-multiple','複数攻撃の入口','combat','menu',{'state':s},menu(['攻撃しない','全員で攻撃する','攻撃するクリーチャーを個別に選ぶ']),['editor.attack','combat.multiple'])
# The no-attack position is fixed for one attacker and multiple attackers.
single=state(combat_o);single['editor']={'kind':'attack','attackers':[7]}
add('attack-menu-single','1体でも攻撃しないは1番','combat','menu',{'state':single},menu(['攻撃しない','ゴブリンの先達で攻撃する']),['editor.attack','combat.single','combat.fixed_no_attack'])
for mode,attack_state in [('single',single),('multiple',s)]:
 for number,selected in [('1',[]),('2',attack_state['editor']['attackers'])]:
  add('attack-input-'+mode+'-'+number,'攻撃選択 '+mode+'：'+number,'combat','input',{'state':attack_state},'',['editor.attack','combat.fixed_no_attack','input.selection_mapping'],selection={'attackers':selected},inputs=[number])
attack_prompts=menu(['攻撃しない','攻撃する'])
text='ゴブリンの先達 #7の攻撃\n'+attack_prompts+'\nゴブリンの先達 #8の攻撃\n'+attack_prompts
add('attack-select','同名2体を識別し一方だけ攻撃','combat','combat',{'state':s,'kind':'attack'},text,['combat.identity','combat.subset'],selection={'attackers':[8]},inputs=['1','2'])
s=state(combat_o);s['editor']={'kind':'block','blockers':[7,8],'attackers':[9]}
text='ゴブリンの先達 #7のブロック\n'+menu(['ブロックしない','大歓楽の幻霊 #9をブロック'])+'\nゴブリンの先達 #8のブロック\n'+menu(['ブロックしない','大歓楽の幻霊 #9をブロック'])
add('block-double','同じ攻撃者を2体でブロック','combat','combat',{'state':s,'kind':'block'},text,['editor.block','combat.double_block'],selection={'blocks':[[7,9],[8,9]]},inputs=['2','2'])
s=state(combat_o);s['editor']={'kind':'damage','groups':[{'source':9,'targets':[7,8],'power':2}]}
text='大歓楽の幻霊 #9 → ゴブリンの先達 #7のダメージ\n'+menu(['0点','1点','2点'])
add('damage-split','ダメージを分配し残りを自動割当','combat','combat',{'state':s,'kind':'damage'},text,['editor.damage','combat.damage_split'],selection={'damage':[[1,1]]},inputs=['2'])
for key in ('q',None):
 add('combat-exit-'+str(key),'割当途中で終了','combat','combat',{'state':s,'kind':'damage'},text,['combat.quit','combat.eof'],inputs=[key]);cases[-1]['expected']['selection']=None
# Proposal: s inside an editor returns to THAT prompt, never displays the outer menu.
status_combat=BASE_STATUS.replace('戦闘前メイン','攻撃クリーチャー指定ステップ').replace('戦場：双方とも空。','あなたの戦場：ゴブリンの先達 #7（2/2）、ゴブリンの先達 #8（2/2）。\nbotの戦場：大歓楽の幻霊（2/2）。').replace('土地は未プレイ。\n','').removesuffix(PASS).rstrip('\n')
add('combat-status','割当中sで元の問いへ戻る','combat','combat',{'state':s,'kind':'damage'},text+'\n'+status_combat+'\n'+text,['combat.s','input.no_mutation'],selection={'damage':[[1,1]]},inputs=['s','2'])
add('combat-invalid','割当中の不正入力','combat','combat',{'state':s,'kind':'damage'},text+'\n選択肢にない番号です。\n'+text,['combat.invalid'],selection={'damage':[[1,1]]},inputs=['99','2'])

# Further semantic combinations are appended below, before freezing this candidate file.
# Fetch boundaries: aggregate only within one uninterrupted interaction.
fetch=next(e for e in engine if e['id']=='fetch-1-10-paid')
delta('fetch-response-window','フェッチ起動に応答できる位置で区切る','land',fetch['rows'][:2],'botは乾燥台地を生け贄に捧げました。botのライフ：20→19。\nスタック：botの土地を探す能力。',['fetch.response','stack.pending'],1,'prefix of real engine fetch capture')
delta('fetch-after-response','応答後の解決だけを説明','land',fetch['rows'][2:],'botは聖なる鋳造所を2点払って出しました。botのライフ：19→17。',['fetch.resume','fetch.no_repeat_cost'],1,'suffix of real engine fetch capture')
# Remaining entry payment and search are distinct only after a cancelled fetch plan.
for pay,label in [(False,'タップ状態で出す'),(True,'2点支払って出す')]:
 add('menu-entry-'+str(pay),'取得時の出し方を選び直す','menu','menu',{'state':state(actions=[{'kind':'shockland_entry','pay_2_life':pay}])},menu([label]),['menu.shockland_entry','fetch.plan_cancelled'])
for value,label in [(None,'土地を見つけない'),({'card':EN[15],'card_id':15},'山')]:
 add('menu-fetch-selection-'+str(value is None),'土地取得の単独選択','menu','menu',{'state':state(actions=[{'kind':'fetch_selection','selection':value}])},menu([label]),['menu.fetch_selection','fetch.plan_cancelled'])

# Opponent private actions must not disclose card names, even when another layer mistakenly supplies one.
for secret in ('private','Boros Charm'):
 b=base();b['phase']='mulligan';b.pop('turn');b['players'][1]['hand']['size']=7;n=copy.deepcopy(b);n['players'][1]['hand']['size']=6
 delta('bot-bottom-'+secret,'相手のボトムは枚数だけ','opening',[row(b,n,{'kind':'bottom','card':secret},1)],'botは手札1枚をライブラリーの一番下に戻しました。botの手札：7→6枚。',['knowledge.bot_bottom','action.bottom'],1)
# Seven mulligans: no eighth option; zero-card keep remains intelligible.
o=base();o['phase']='mulligan';o['mulligans_taken']=[7,0];o.pop('turn');s=state(o,[{'kind':'keep'}]);
add('mulligan-limit','7回マリガン後は0枚でキープ','opening','menu',{'state':s},menu(['キープする']),['mulligan.limit'])
# Full status works in opening/mulligan despite absent public board/turn fields.
o=copy.deepcopy(live[1]['state']['observation']);s=state(o,[{'kind':'bottom','card':EN[15]}]);text='初手選択。あなたのマリガン：1回。\n手札7枚：'+cards(o['players'][0]['hand']['known'])+'。\nbotの手札7枚。\n'+menu(['山'])
add('mulligan-status','初手選択中のsで存在しないターンを作らない','opening','status',{'state':s},text,['mulligan.s'])

# Priority pass always numbered one, including a source list with a different index order.
actions=[{'kind':'play_land','card':EN[15]},{'kind':'pass'}];s=state(actions=actions)
add('pass-first','優先権パスは元のエンジンindexに関係なく1番','menu','menu',{'state':s},menu(['何もせず優先権を渡す','山を出す']),['priority.fixed_one'])
add('pass-first-selection','表示1番から正しいengine indexへ','input','input',{'state':s},'',['priority.fixed_one','input.selection_mapping'],selection={'index':'1'},inputs=['1'])
# Same-name targets must be distinguishable.
o=base();o['public']['battlefield']=[perm(0,7,1),perm(0,8,1)]
actions=[{'kind':'pass'}]+[{'kind':'cast','card':EN[3],'targets':[{'object_id':id,'card':EN[0],'controller':1}]} for id in (7,8)]
add('menu-target-identity','同名の対象2体を識別','menu','menu',{'state':state(o,actions)},menu(['何もせず優先権を渡す','稲妻を唱える（botのゴブリンの先達 #7に3点ダメージ）','稲妻を唱える（botのゴブリンの先達 #8に3点ダメージ）']),['target.identity','menu.cast'])
# A friendly target is permitted only when the engine supplies that legal choice.
a={'kind':'cast','card':EN[3],'targets':[{'object_id':7,'card':EN[0],'controller':0}]}
add('menu-friendly-response','相手のらせん対象を自分で除去する応答','menu','menu',{'state':state(actions=[{'kind':'pass'},a])},menu(['何もせず優先権を渡す','稲妻を唱える（自分のゴブリンの先達に3点ダメージ）']),['target.friendly_exception'])

# Unresolved casts: never narrate damage/healing before resolution.
for p in (0,1):
 e=next(e for e in engine if e['id']==f'cast-{p}-3-0-false-false');end=next(i for i,r in enumerate(e['rows']) if r['action']['kind']=='pay_mana')+1
 delta('cast-pending-'+str(p),'呪文への応答待ち','spell',e['rows'][:end],f'{seat(p)}は聖なる鋳造所をタップし、{seat(1-p)}を対象に稲妻を唱えました。解決前に対応できます。'+('手札：1→0枚。' if not p else ''),['stack.pending','spell.not_resolved'],p,'prefix of real engine spell capture')
# Target disappears: the spell has no legal target, so Helix must not heal.
b=base();b['public']['life']=[12,10];b['public']['stack']=[{'object_id':22,'kind':'spell','card':EN[9],'controller':1,'targets':[{'object_id':7,'controller':0,'card':EN[0]}]}];n=copy.deepcopy(b);n['public']['stack']=[]
delta('helix-no-target','らせんの対象消失・回復なし','spell',[row(b,n,{'kind':'pass'},0)],'稲妻のらせんは対象がなくなり、解決されませんでした。',['target.fizzle','helix.no_heal'],1)
# A remaining player target of Blaze is still affected if its creature target disappears.
b=base();b['public']['stack']=[{'object_id':23,'kind':'spell','card':EN[8],'controller':1,'targets':[{'player':0},{'object_id':7,'controller':0,'card':EN[0]}]}];n=copy.deepcopy(b);n['public']['stack']=[];n['public']['life'][0]=17
delta('blaze-one-target','猛火の一部対象消失','spell',[row(b,n,{'kind':'pass'},0)],'焼尽の猛火が解決しました。あなたのライフ：20→17。クリーチャーの対象は戦場にいません。',['target.partial','target.two'],1)
# A response during the other player's turn must keep the actual caster's identity.
e=next(e for e in engine if e['id']=='cast-0-3-0-false-false');rr=copy.deepcopy(e['rows'])
for r in rr:
 for part in ('before','after'):r[part]['active_player']=1;r[part]['turn']=4
before=state(rr[0]['before'],seq=0);after=state(rr[-1]['after'],seq=len(rr));after['timeline']=rr
add('response-on-bot-turn','相手ターン中に自分が唱えた呪文','spell','render',{'previous':before,'state':after},'聖なる鋳造所をタップし、稲妻が解決しました。botのライフ：20→17。手札：1→0枚。\n'+PASS,['actor.response_turn','target.player'])
# Two simultaneous trigger sources: summarize each relevant effect, suppress ordering internals.
b=base();b['public']['battlefield']=[perm(1,7),perm(1,8)];n=copy.deepcopy(b)
for c in n['public']['battlefield']:c['power']=2;c['toughness']=3
rr=[row(b,b,{'kind':'order_trigger','trigger_id':50}),row(b,n,{'kind':'pass'},seq=12)]
delta('trigger-multiple','複数の果敢と自動誘発順','spell',rr,'あなたの僧院の速槍 #7と #8は果敢で2/3。',['trigger.multiple','action.order_trigger'])
# Suspend tick, cast, decline, nonzero remaining counter, and multiple suspended cards.
for start,end in [(2,1),(1,0)]:
 b=base();b['phase']='upkeep';b['public']['suspended']=[{'card':EN[5],'owner':0,'object_id':7,'time_counters':start}];n=copy.deepcopy(b);n['public']['suspended'][0]['time_counters']=end
 delta('suspend-counter-'+str(start),'待機カウンターの進行','spell',[row(b,n,{'kind':'pass'})],f'あなたの裂け目の稲妻の時間カウンター：{start}→{end}個。',['suspend.counter','suspend.ready' if end==0 else 'suspend.not_ready'])
b=base();b['phase']='upkeep';b['public']['suspended']=[{'card':EN[5],'owner':0,'object_id':7,'time_counters':0}];n=copy.deepcopy(b)
delta('suspend-decline','待機から唱えない','spell',[row(b,n,{'kind':'decline_suspend'})],'裂け目の稲妻を唱えず、追放領域に残しました。',['suspend.decline','action.decline_suspend'])
b=base();b['public']['suspended']=[{'card':EN[5],'owner':0,'object_id':7,'time_counters':1},{'card':EN[5],'owner':0,'object_id':8,'time_counters':2}]
text=BASE_STATUS.replace('土地は未プレイ。','待機：あなたの裂け目の稲妻 #7（時間カウンター1個）、あなたの裂け目の稲妻 #8（時間カウンター2個）。\n土地は未プレイ。')
add('suspend-multiple','待機複数枚の識別','status','status',{'state':state(b)},text,['suspend.multiple'])
# Guide reveals: land is added to hand; nonland stays on top; drawing consumes top knowledge.
for card,dest in [(15,'hand'),(3,'library_top')]:
 b=base();n=copy.deepcopy(b)
 if dest=='hand':n['players'][1]['hand']={'size':1,'known':pack([(card,1)]),'unknown_count':0}
 else:n['players'][1]['library']['known_positions']=[{'card':EN[card],'position_from_top':1}]
 event={'kind':'reveal_top','player':1,'card':EN[card],'destination':dest}
 text='botは'+JA[card]+'を公開し'+('て手札に加えました。' if dest=='hand' else '、ライブラリーの一番上に残しました。')
 delta('guide-reveal-'+dest,'先達の公開 '+dest,'spell',[row(b,n,{'kind':'pass'},events=[event])],text,['event.reveal_top','guide.'+dest])
# Combat narratives include blocks, damage, deaths and double strike, not just attacks.
b=base();b['phase']='declare_attackers';b['public']['battlefield']=[perm(0,7)];n=copy.deepcopy(b);n['phase']='postcombat_main'
delta('combat-no-attack','攻撃しない','combat',[row(b,n,{'kind':'attack','attackers':[]})],'攻撃せず、戦闘後メインまで進みました。',['combat.none','action.attack'])
b=base();b['phase']='declare_blockers';b['public']['battlefield']=[perm(0,7,1,True),perm(2,8)];n=copy.deepcopy(b);n['public']['battlefield']=[];n['phase']='end_combat'
a={'kind':'block','blocks':[{'blocker':{'object_id':8,'card':EN[2],'controller':0},'attacker':{'object_id':7,'card':EN[0],'controller':1}}]}
delta('combat-trade','ブロックで相打ち','combat',[row(b,n,a)],'大歓楽の幻霊でbotのゴブリンの先達をブロックしました。戦闘ダメージで両方が戦場を離れました。',['combat.trade','action.block','creature.death'])
b=base();b['phase']='first_strike_damage';b['public']['battlefield']=[perm(0,7,0,True,double_strike=True),perm(2,8,1)];n=copy.deepcopy(b);n['public']['battlefield']=n['public']['battlefield'][:1]
delta('combat-first-damage','二段攻撃の先制ダメージ','combat',[row(b,n,{'kind':'pass'})],'先制戦闘ダメージでbotの大歓楽の幻霊が戦場を離れました。',['combat.first_damage','combat.double_strike'])
b=copy.deepcopy(n);b['phase']='combat_damage';n=copy.deepcopy(b)
delta('combat-blocker-gone','ブロッカーが消えてもトランプルなしでは通らない','combat',[row(b,n,{'kind':'pass'})],'ゴブリンの先達はブロックされているため、botへの戦闘ダメージはありません。',['combat.blocker_gone'])
b=base();b['phase']='combat_damage';b['public']['battlefield']=[perm(0,7,0,True),perm(2,8,1,indestructible=True)];n=copy.deepcopy(b);n['public']['battlefield'][1]['damage_marked']=2
rr=[row(b,n,{'kind':'assign_damage','assignments':[{'source':{'object_id':7,'card':EN[0]},'target':{'object_id':8,'card':EN[2]},'damage':2}]})]
delta('combat-indestructible','破壊不能とダメージ標示','combat',rr,'botの大歓楽の幻霊に2点の戦闘ダメージ。破壊不能のため戦場に残ります。',['combat.indestructible','action.assign_damage'])

# Every observable phase name, including internal-only phases, is checked via s.
PHASES={'not_started':'開始前','opening':'ゲーム開始処理','untap':'アンタップ・ステップ','upkeep':'アップキープ','draw':'ドロー・ステップ','precombat_main':'戦闘前メイン','begin_combat':'戦闘開始ステップ','declare_attackers':'攻撃クリーチャー指定ステップ','declare_blockers':'ブロック・クリーチャー指定ステップ','first_strike_damage':'先制戦闘ダメージ・ステップ','combat_damage':'戦闘ダメージ・ステップ','end_combat':'戦闘終了ステップ','postcombat_main':'戦闘後メイン','end_step':'終了ステップ','cleanup':'クリンナップ・ステップ'}
for phase,label in PHASES.items():
 o=base();o['phase']=phase;text=BASE_STATUS.replace('戦闘前メイン',label)
 if phase!='precombat_main':text=text.replace('土地は未プレイ。\n','')
 add('phase-'+phase,label,'turn','status',{'state':state(o)},text,['phase.'+phase])
# No-event passes name the destination even when staying within the same turn.
for p in (0,1):
 b=base();b['active_player']=p;b['turn']=3+p;n=copy.deepcopy(b);n['phase']='declare_attackers';rr=[row(b,n,{'kind':'pass'},p)]
 ps=state(b);s=state(n,seq=11);s['timeline']=rr
 add('pass-destination-'+str(p),'パス後の到達地点','turn','render',{'previous':ps,'state':s},f'{seat(p)}の2ターン目・攻撃クリーチャー指定ステップまで進みました。\n'+PASS,['turn.destination','action.pass'])
# First turn of the starting bot: no fictional draw or preceding human turn.
b=base();b['phase']='mulligan';b.pop('turn');b['players'][0]['hand']={'size':0,'known':[],'unknown_count':0};n=base();n['active_player']=1;n['turn']=1;n['players'][1]['hand']['size']=7
rr=[row(b,n,{'kind':'keep'},1)];ps=state(b);ps['starting']=1;s=state(n,seq=11);s['starting']=1;s['timeline']=rr
add('turn-bot-first','後手開始時の相手ターンは簡略表示','turn','render',{'previous':ps,'state':s},'初手が決まりました。\nbotの1ターン目・戦闘前メインまで進みました。先手のためドローはありません。botの手札7枚。ライフ：あなた20／bot20。\n'+PASS,['turn.bot_brief','turn.first_no_draw'],forbidden=['戦場：','あなたのターンを終え'])
# Cleanup discards show the remaining hand without dumping the whole board.
b=base();b['phase']='cleanup';b['players'][0]['hand']={'size':8,'known':pack([(15,8)]),'unknown_count':0};n=copy.deepcopy(b);n['players'][0]['hand']={'size':7,'known':pack([(15,7)]),'unknown_count':0}
delta('cleanup-discard','終了時に手札上限まで捨てる','turn',[row(b,n,{'kind':'discard','card':EN[15]})],'山を捨てました。手札：8→7枚。',['action.discard','cleanup.discard'])
# Terminal outcomes never invent final life from the worker's preterminal observation.
for reason,winner,title,expected in [('game',0,'勝利','あなたの勝利です。'),('game',1,'敗北','あなたの敗北です。botの勝利。'),('draw',None,'引き分け','引き分けです。'),('concede',0,'相手投了','botが投了しました。あなたの勝利です。')]:
 b=base();s=state(b,seq=11);s['status']='finished';s['outcome']={'reason':reason,'winner':winner};s['editor']=None
 add('ending-'+title,title,'ending','render',{'previous':state(b),'state':s},expected,['ending.'+reason,'ending.stale_snapshot'],forbidden=['s: 全体状況','ライフ：'])
# Preserve pass #1 on all priority menu fixtures, not just one transcript.
for c in cases:
 if c['method']=='menu' and c['args']['state'].get('editor',{}).get('kind')=='choices':
  if any(x['action']['kind']=='pass' for x in c['args']['state']['editor']['choices']):c['covers'].append('priority.fixed_one')

# Turn headings are independently tested as complete render calls, not only phase translations.
b=base();b['turn']=2;b['active_player']=1;b['phase']='end_step';b['players'][0]['hand']={'size':1,'known':pack([(3,1)]),'unknown_count':0}
u=copy.deepcopy(b);u['turn']=3;u['active_player']=0;u['phase']='upkeep'
d=copy.deepcopy(u);d['phase']='draw';d['players'][0]['hand']={'size':2,'known':pack([(3,1),(15,1)]),'unknown_count':0};d['players'][0]['library']['size']=49
n=copy.deepcopy(d);n['phase']='precombat_main';rr=[row(b,u,{'kind':'pass'},1),row(u,d,{'kind':'pass'},0,12),row(d,n,{'kind':'pass'},0,13)]
ps=state(b);s=state(n,seq=13);s['timeline']=rr
text='あなたの2ターン目・戦闘前メインまで進みました。山を引きました。\nライフ：あなた20／bot20。\n戦場：双方とも空。\n手札2枚：稲妻、山。\nbotの手札0枚。土地は未プレイ。\n'+PASS
add('own-turn-full','無事象アップキープを飛ばしてドロー後に全体表示','turn','render',{'previous':ps,'state':s},text,['turn.own_full','turn.skip_upkeep','turn.drawn_card'],forbidden=['アップキープです','アンタップしました','墓地：'])
# Opponent upkeep with action: do not claim main phase or a draw that has not occurred.
b=base();b['phase']='end_step';u=copy.deepcopy(b);u['turn']=4;u['active_player']=1;u['phase']='upkeep';u['players'][1]['hand']['size']=4
s=state(u,seq=11);s['timeline']=[row(b,u,{'kind':'pass'})]
add('bot-upkeep-stop','相手アップキープの応答待ち','turn','render',{'previous':state(b),'state':s},'botの2ターン目・アップキープです。まだドロー前です。botの手札4枚。ライフ：あなた20／bot20。\n'+PASS,['turn.upkeep_stop','turn.bot_brief'],forbidden=['1枚引き','戦闘前メイン','戦場：'])
# Own upkeep can contain a non-suspend event; never invent Rift Bolt counters.
b=base();b['turn']=2;b['active_player']=1;b['phase']='end_step';u=base();u['phase']='upkeep';s=state(u,seq=11);s['timeline']=[row(b,u,{'kind':'pass'},1)]
text='あなたの2ターン目・アップキープです。まだドロー前です。\nライフ：あなた20／bot20。\n戦場：双方とも空。\n手札0枚：空。\nbotの手札0枚。\n'+PASS
add('own-upkeep-no-rift','待機以外のアップキープ停止','turn','render',{'previous':state(b),'state':s},text,['turn.upkeep_stop','turn.no_invented_trigger'],forbidden=['裂け目','時間カウンター','まだドロー前です。\n待機'])
# Previously revealed library card moves into the opponent's known hand when drawn.
b=base();b['phase']='upkeep';b['turn']=4;b['active_player']=1;b['players'][1]['library']['known_positions']=[{'card':EN[3],'position_from_top':1}]
n=copy.deepcopy(b);n['phase']='draw';n['players'][1]['library']['known_positions']=[];n['players'][1]['library']['size']=49;n['players'][1]['hand']={'size':1,'known':pack([(3,1)]),'unknown_count':0}
delta('known-card-drawn','公開トップを引いたときの情報移動','turn',[row(b,n,{'kind':'pass'},1)],'botは公開済みの稲妻を引きました。botの手札：0→1枚。',['knowledge.draw','knowledge.clear_top'],1)
# Shuffling clears known positions and stays silent when it is purely an internal transition.
b=base();b['players'][1]['library']['known_positions']=[{'card':EN[3],'position_from_top':1}];n=copy.deepcopy(b);n['players'][1]['library']['known_positions']=[]
delta('shuffle-internal','シャッフルを単独で読み上げない','land',[row(b,n,{'kind':'shuffle'},1)],'',['knowledge.shuffle','action.shuffle'],1)
# Status ignores diagnostic-only material and does not infer hidden card names from it.
o=base();o['players'][1]['hand']={'size':4,'known':[],'unknown_count':4};o['players'][1]['library']['remaining']=[{'card':'HIDDEN_SENTINEL','min':0,'max':4}];o['players'][1]['decklist']=[{'card':'HIDDEN_SENTINEL','count':4}]
add('status-private-diagnostics','診断用の残り枚数からカード名を読み上げない','status','status',{'state':state(o)},BASE_STATUS.replace('botの手札0枚。','botの手札4枚。'),['knowledge.diagnostics'],forbidden=['HIDDEN_SENTINEL','非公開'])
# Compressed permanent ranges and mixed tap states must preserve counts and identity.
o=base();o['public']['battlefield']=[perm(15,7,count=3),perm(13,10,tapped=True)];text=BASE_STATUS.replace('戦場：双方とも空。','あなたの戦場：山3枚。感動的な眺望所はタップ。\nbotの戦場：空。')
add('status-permanent-count','同名パーマネントの圧縮件数','status','status',{'state':state(o)},text,['status.permanent_count','status.mixed_tap'])
# Damage, buffs and marked damage are cleared at cleanup without re-reading the whole battlefield.
b=base();b['phase']='cleanup';b['public']['battlefield']=[perm(1,7,power=3,toughness=4,damage_marked=1,double_strike=True,indestructible=True)];n=copy.deepcopy(b);n['public']['battlefield']=[perm(1,7)]
delta('cleanup-expire','果敢・二段攻撃・破壊不能・ダメージの終了','turn',[row(b,n,{'kind':'pass'})],'僧院の速槍は1/2に戻りました。ターン終了までの二段攻撃・破壊不能が終了し、ダメージが取り除かれました。',['cleanup.effects'])
# Nonempty mana is visible, but paying it is not an additional prompt/log line.
b=base();b['public']['mana'][0]['R']=1;n=copy.deepcopy(b);n['public']['mana'][0]['R']=0
delta('mana-internal-payment','内部のマナ支払いだけなら無言','spell',[row(b,n,{'kind':'pay_mana','mana':{'R':1}},0)],'',['mana.auto_silent','action.pay_mana'])
# The same numeric grammar applies within nested editors.
s=state(combat_o);s['editor']={'kind':'damage','groups':[{'source':9,'targets':[7,8],'power':2}]};text='大歓楽の幻霊 #9 → ゴブリンの先達 #7のダメージ\n'+menu(['0点','1点','2点'])
add('combat-trailing-dot','個別選択でも番号の末尾ピリオドを受け付ける','combat','combat',{'state':s,'kind':'damage'},text,['input.trailing_dot','combat.number'],selection={'damage':[[1,1]]},inputs=['2.'])
# Multiple damage sources maintain independent remaining power.
s=state(combat_o);s['editor']={'kind':'damage','groups':[{'source':7,'targets':[9],'power':2},{'source':8,'targets':[9],'power':2}]}
add('damage-multiple-sources','一意の割当は質問せず2体分を送る','combat','combat',{'state':s,'kind':'damage'},'',['combat.multiple_sources'],selection={'damage':[[2],[2]]},inputs=[])
# End of library and simultaneous loss use outcome, not a guessed final life value.
for reason,text in [('library','あなたの敗北です。botの勝利。'),('simultaneous','引き分けです。')]:
 b=base();b['phase']='draw';b['players'][0]['library']['size']=0;s=state(b,seq=11);s['status']='finished';s['outcome']={'reason':'game' if reason=='library' else 'draw','winner':1 if reason=='library' else None};s['editor']=None;s['timeline']=[row(b,b,{'kind':'pass'},1,outcome=s['outcome'])]
 add('ending-'+reason,reason,'ending','render',{'previous':state(b),'state':s},text,['ending.'+reason,'ending.stale_snapshot'],forbidden=['ライフ：','s: 全体状況'])

# More interaction combinations: suspended creature damage, private mulligans, and mana costs.
b=base();b['phase']='upkeep';b['public']['suspended']=[{'card':EN[5],'owner':0,'object_id':8,'time_counters':0}];b['public']['battlefield']=[perm(0,7,1)];n=copy.deepcopy(b);n['public']['battlefield']=[];n['public']['suspended']=[]
delta('suspend-kill-creature','待機からクリーチャーを焼く','spell',[row(b,n,{'kind':'suspend_target','target':{'object_id':7,'card':EN[0],'controller':1}})],'待機から裂け目の稲妻を唱え、botのゴブリンの先達に3点ダメージ。botのゴブリンの先達が戦場を離れました。',['suspend.creature','action.suspend_target'])
b=base();b['phase']='mulligan';b.pop('turn');b['mulligans_taken']=[0,0];b['players'][1]['hand']={'size':7,'known':[],'unknown_count':7};n=copy.deepcopy(b);n['mulligans_taken']=[0,1]
delta('bot-mulligan-private','相手の引き直しはカード名を出さない','opening',[row(b,n,{'kind':'mulligan'},1)],'botは1回目のマリガンを行い、7枚引き直しました。',['mulligan.bot','knowledge.unknown'],1)
# Damage marking on a surviving creature is distinct from death.
b=base();b['public']['battlefield']=[perm(1,7,1,power=3,toughness=4)];n=copy.deepcopy(b);n['public']['battlefield'][0]['damage_marked']=3
b['public']['stack']=[{'object_id':8,'kind':'spell','card':EN[3],'controller':0,'targets':[{'object_id':7,'card':EN[1],'controller':1}]}]
delta('spell-creature-survives','火力を受けたクリーチャーが残る','spell',[row(b,n,{'kind':'pass'},1)],'稲妻が解決し、botの僧院の速槍に3点ダメージ。速槍は3/4で戦場に残ります。',['creature.survives','status.damage'])
# In-progress mana pool with both actual Burn colors.
o=base();o['public']['mana'][0]['W']=1;o['public']['mana'][0]['R']=2
add('status-mana-colors','白と赤のマナを区別','status','status',{'state':state(o)},BASE_STATUS.replace('土地は未プレイ。','あなたのマナプール：W1、R2。\n土地は未プレイ。'),['status.mana','mana.colors'])
# Transient untap/counter/order actions on a skipped turn are not separate narration.
b=base();b['public']['battlefield']=[perm(15,7,tapped=True)];n=copy.deepcopy(b);n['turn']=5;n['public']['battlefield'][0]['tapped']=False
rr=[row(b,n,{'kind':'pass'})]
delta('untap-internal','自分のターン開始時の土地アンタップを省略','turn',rr,'',['turn.implicit_untap'])
# Global and editor digit grammars must agree (ASCII decimal with optional trailing dot).
s=state(combat_o);s['editor']={'kind':'damage','groups':[{'source':9,'targets':[7,8],'power':2}]};text='大歓楽の幻霊 #9 → ゴブリンの先達 #7のダメージ\n'+menu(['0点','1点','2点'])
add('combat-non-ascii','全角数字を通常メニューと同じ扱いにする','combat','combat',{'state':s,'kind':'damage'},text+'\n選択肢にない番号です。\n'+text,['combat.invalid','input.invalid'],selection={'damage':[[1,1]]},inputs=['２','2'])

for i,(ability,card,description) in enumerate([
 ('prowess',1,'僧院の速槍の果敢（ターン終了時まで+1/+1）'),
 ('Eidolon of the Great Revel: 2 damage to caster',2,'大歓楽の幻霊の誘発（あなたに2点ダメージ）'),
 ("Goblin Guide: reveal defending player's top card",0,'ゴブリンの先達の誘発（あなたのライブラリーの一番上を公開）'),
 ('fetch land, then shuffle',10,'乾燥台地の能力（土地を探す）'),
 ('Sunbaked Canyon: draw a card',12,'灼陽大峡谷の能力（1枚引く）'),
 ('suspend: remove a time counter',5,'裂け目の稲妻の待機（時間カウンターを1個取り除く）'),
 ('suspend: cast Rift Bolt',5,'裂け目の稲妻の待機（唱えるか選ぶ）')]):
 o=base();o['public']['stack']=[{'object_id':20,'kind':'ability','controller':1,'ability':ability,'source':{'object_id':7,'card':EN[card],'controller':1},'affected_player':0,'targets':[]}]
 text=BASE_STATUS.replace('土地は未プレイ。','スタック（次に解決する順）：botの'+description+'。\n土地は未プレイ。')
 add('stack-ability-'+str(i),description,'status','status',{'state':state(o)},text,['stack.ability','ability.'+ability])
# Terminal burn during blockers must not be misreported as combat damage.
e=next(e for e in engine if e['id']=='lethal-spell-1');rr=copy.deepcopy(e['rows'])
for r in rr:
 for part in ('before','after'):r[part]['phase']='declare_blockers'
delta('ending-burn-during-block','ブロック中の火力による終局','ending',rr,'botは山をタップし、稲妻が解決しました。あなたの敗北です。botの勝利。',['ending.spell_in_combat','ending.stale_snapshot'],1)
# Internal execution bookkeeping must not become English user messages.
o=base();o['public']['execution']={'kind':'choice','choice':{'kind':'order_triggers'},'pending_triggers':[{'id':22,'ability':'ORDER_SENTINEL'}],'entering':[]}
add('status-internal-execution','内部の選択・誘発順を表示しない','status','status',{'state':state(o)},BASE_STATUS,['internal.execution'],forbidden=['ORDER_SENTINEL','order_triggers','entering'])

# Stale stack targets may have an ID but no surviving object/card record.
o=base();o['public']['stack']=[{'object_id':20,'kind':'spell','card':EN[9],'controller':1,'targets':[{'object_id':7}]}]
text=BASE_STATUS.replace('土地は未プレイ。','スタック（次に解決する順）：botの稲妻のらせん（対象 #7・戦場にいない）。\n土地は未プレイ。')
add('status-missing-target','消えた対象の所有者やカード名を捏造しない','status','status',{'state':state(o)},text,['target.missing_identity','stack.pending'])
add('input-too-long','過大な数字も再入力扱い','input','input',{'state':state()},'選択肢にない番号です。1〜1 または s を入力してください。\n'+PASS,['input.invalid','input.large_number'],selection={'index':'0'},inputs=['9'*5000,'1'])
b=base();s=state(b,seq=11);s['status']='finished';s['outcome']={'reason':'concede','winner':1};s['editor']=None
add('ending-human-concede','自分の投了が通知された場合','ending','render',{'previous':state(b),'state':s},'あなたが投了しました。botの勝利。',['ending.concede'],forbidden=['s: 全体状況'])

# Process-level conversations use the real CLI stdin/stdout and an observer-only wire double.
for starting in (0,1):
 chain=[r for r in live if r['id'].endswith('-'+str(starting)) and not r['id'].startswith('keep-5')]
 steps=[];inputs=[];outputs=[]
 for i,r in enumerate(chain):
  outputs.append(next(c['expected']['text'] for c in cases if c['id']==r['id']))
  if i:
   inputs.append(r['input']);steps.append({'expect':{'op':'act','sequence':chain[i-1]['state']['sequence'],'selection':r['selection']},'responses':[{'state':r['state']}]})
 inputs.append('q')
 add('process-mulligans-'+str(starting),'実CLIで2回マリガン→カード戻し→終了','opening','process',{'wire':{'initial':[{'state':chain[0]['state']}],'steps':steps}},'\n'.join(outputs)+'\n',['process.dialogue','mulligan.first','mulligan.multiple','mulligan.partial_bottom','input.no_mutation'],origin='real worker boundary replay',inputs=inputs)
 cases[-1]['expected'].update(stderr='',exitcode=0,request_count=5)
first=copy.deepcopy(live[0]['state']);opening_text=next(c['expected']['text'] for c in cases if c['id']=='opening-0')+'\n'
for key,tag in [('q','quit'),(None,'eof'),('INTERRUPT','interrupt')]:
 add('process-'+tag,'実CLIで'+tag,'input','process',{'wire':{'initial':[{'state':first}],'steps':[]}},opening_text,['process.cleanup','input.'+tag],inputs=[key])
 cases[-1]['expected'].update(stderr='\n終了しました。\n' if tag=='interrupt' else '',exitcode=0,request_count=0)
for id,initial,err in [('worker-eof',[],'対戦ワーカーが停止しました。\n'),('worker-invalid-json',[{'raw':'!broken'}],'対戦ワーカーから不正な応答を受信しました。\n'),('worker-error',[{'error':'Illegal action'}],'対戦ワーカーがエラーを返しました：Illegal action\n'),('worker-state-error',[{'state':{'error':'Capacity'}}],'対戦ワーカーがエラーを返しました：Capacity\n')]:
 add(id,id,'input','process',{'wire':{'initial':initial,'steps':[],'exit_after_initial':True}},'',['process.cleanup','transport.'+id],inputs=[])
 cases[-1]['expected'].update(stderr=err,exitcode=1,request_count=0)
# Thinking/progress and duplicate old-state replies must not duplicate user-facing narration.
r=next(r for r in live if r['id']=='mulligan-1-0');thinking=copy.deepcopy(first);thinking['status']='thinking'
wire={'initial':[{'state':thinking},{'state':first}],'steps':[{'expect':{'op':'act','sequence':first['sequence'],'selection':r['selection']},'responses':[{'state':first},{'state':thinking},{'state':r['state']}]}]}
add('process-stale-progress','途中経過と重複応答を表示しない','input','process',{'wire':wire},opening_text+next(c['expected']['text'] for c in cases if c['id']=='mulligan-1-0')+'\n',['transport.stale','transport.thinking','input.selection_mapping'],inputs=['2','q'])
cases[-1]['expected'].update(stderr='',exitcode=0,request_count=1)

# Missing wire fields must be treated as a protocol error rather than a traceback.
add('worker-missing-fields','必要な応答フィールド欠落','input','process',{'wire':{'initial':[{'state':{}}],'steps':[],'exit_after_initial':True}},'',['transport.schema','process.cleanup'],inputs=[])
cases[-1]['expected'].update(stderr='対戦ワーカーから不正な応答を受信しました。\n',exitcode=1,request_count=0)
# s / invalid inputs remain local; no engine request is sent.
text='初手選択。あなたのマリガン：0回。\n手札7枚：'+cards(first['observation']['players'][0]['hand']['known'])+'。\nbotの手札7枚。\n'+menu(['キープする','マリガンする'])
add('process-local-inputs','sと無効入力はワーカーを進めない','input','process',{'wire':{'initial':[{'state':first}],'steps':[]}},opening_text+text+'\n選択肢にない番号です。1〜2 または s を入力してください。\n'+menu(['キープする','マリガンする'])+'\n',['process.dialogue','input.s','input.invalid','input.no_mutation'],inputs=['s','99','q'])
cases[-1]['expected'].update(stderr='',exitcode=0,request_count=0)
# Attach the implementation-independent line-noise guard to every delta case.
for c in cases:
 if c['method']=='delta':c['forbidden']+=['墓地：','ライブラリーを切り直','アンタップ状態']

# Human-readable scenario titles, without altering inputs or expectations.
engine_by_id={e['id']:e for e in engine}
for c in cases:
 if c['id'] not in engine_by_id:continue
 e=engine_by_id[c['id']];a=e['rows'][0]['action'];who=seat(e['actor'])
 if c['id'].startswith('fetch-'):
  target=c['id'].split('-')[-1];dest={'paid':'聖なる鋳造所・2点払い','tapped':'聖なる鋳造所・タップ','mountain':'山','fail':'取得なし'}[target]
  c['title']=who+'のフェッチ：'+N[a['source']['card']]+' → '+dest
 elif c['id'].startswith('cast-'):
  target='クリーチャー' if any('object_id' in t for t in a.get('targets',[])) else 'プレイヤー' if a.get('targets') else '対象なし'
  mode={'4 damage to player':'4点','your permanents gain indestructible this turn':'破壊不能','target creature gains double strike this turn':'二段攻撃'}.get(a.get('mode'),'絢爛' if a.get('spectacle') else '')
  c['title']=who+'の'+N[a['card']]+'：'+target+('・'+mode if mode else '')
 elif c['id'].startswith('canyon-draw-'):c['title']=who+'の峡谷でドロー（'+('マナプールから支払い' if c['id'].endswith('true') else '土地から支払い')+'）'
 elif a.get('card'):c['title']=who+'の'+N[a['card']]+'：'+c['title'].split('：')[-1]

# Store reviewed candidates; none become approved merely because a test passes.
payload={'schema':1,'approval':'pending_explicit_human_approval','scope':'Fixed 16-card Burn engine UI; all exported actions, phases, editor modes, terminal and transport classes. Not an exhaustive enumeration of all possible games.','groups':GROUPS,'cases':cases}
(HERE/'candidates.json.gz').write_bytes(gzip.compress((json.dumps(payload,ensure_ascii=False,indent=2)+'\n').encode(),mtime=0))
print(f'Authored {len(cases)} candidate cases; awaiting human approval')
