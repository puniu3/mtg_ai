"""Japanese text UI over observer-safe engine facts. No fixture or expected-output reads."""
from collections import Counter
NAMES=dict(zip(['Goblin Guide','Monastery Swiftspear','Eidolon of the Great Revel','Lightning Bolt','Lava Spike','Rift Bolt','Skewer the Critics','Boros Charm','Searing Blaze','Lightning Helix','Arid Mesa','Bloodstained Mire','Sunbaked Canyon','Inspiring Vantage','Sacred Foundry','Mountain'],['ゴブリンの先達','僧院の速槍','大歓楽の幻霊','稲妻','溶岩の撃ち込み','裂け目の稲妻','批判家刺殺','ボロスの魔除け','焼尽の猛火','稲妻のらせん','乾燥台地','血染めのぬかるみ','灼陽大峡谷','感動的な眺望所','聖なる鋳造所','山']))
PHASE={'mulligan':'初手選択','upkeep':'アップキープ','draw':'ドロー・ステップ','precombat_main':'戦闘前メイン','begin_combat':'戦闘開始ステップ','declare_attackers':'攻撃クリーチャー指定ステップ','declare_blockers':'ブロック・クリーチャー指定ステップ','combat_damage':'戦闘ダメージ・ステップ','end_combat':'戦闘終了ステップ','postcombat_main':'戦闘後メイン','end_step':'終了ステップ','cleanup':'クリンナップ・ステップ'}
def name(c):return NAMES.get(c,c or '')
def who(p):return 'あなた' if p==0 else 'bot'
def turn(o):return (o.get('turn',1)+1)//2
def hand(o,p=0):return o['players'][p]['hand']
def known(cards):return '、'.join(name(c['card'])+(f"{c['count']}枚" if c.get('count',1)>1 else '') for c in cards) or '空'
def life(o,p):return o.get('public',{}).get('life',[20,20])[p]
def bf(o,p):return [x for x in o.get('public',{}).get('battlefield',[]) if x['controller']==p]
PHASE.update(not_started='開始前',opening='ゲーム開始処理',untap='アンタップ・ステップ',first_strike_damage='先制戦闘ダメージ・ステップ')
def identity(c,o=None,force=False):
 duplicate=o and sum(x.get('card')==c.get('card') and x.get('controller')==c.get('controller') for x in bf(o,c.get('controller',0)))>1
 return name(c.get('card'))+(f" #{c['object_id']}" if 'object_id' in c and (force or duplicate) else '')
def subject(t,o=None,menu=False):
 if 'player' in t:return who(t['player'])
 if not t.get('card'):return f"対象 #{t.get('object_id','?')}・戦場にいない"
 return ('自分の' if menu and t.get('controller')==0 else who(t.get('controller',0))+'の')+identity(t,o)
def card_text(c,o=None,details=True):
 text=identity(c,o) if 'power' in c else name(c['card'])
 if c.get('count',1)>1:text+=str(c['count'])+'枚'
 if 'power' in c:
  traits=[f"{c['power']}/{c['toughness']}"]
  if details:
   if c.get('damage_marked'):traits.append(str(c['damage_marked'])+'点ダメージ')
   if c.get('summoning_sick') and c.get('controller')==0:traits.append('召喚酔い')
   if c.get('double_strike'):traits.append('二段攻撃')
   if c.get('indestructible'):traits.append('破壊不能')
  text+='（'+'・'.join(traits)+'）'
 return text
def battlefield(o,p):
 cards=sorted(bf(o,p),key=lambda c:('power' in c,c.get('object_id',0)))
 if not cards:return '空。'
 untapped=[card_text(c,o) for c in cards if not c['tapped']];tapped=[card_text(c,o) for c in cards if c['tapped']]
 if not untapped:return '、'.join(tapped)+('（タップ）。' if len(tapped)==1 else '。'+('両方' if len(tapped)==2 else 'すべて')+'タップ。')
 return '、'.join(untapped)+('。'+'、'.join(tapped)+'はタップ。' if tapped else '。')
def known_suffix(o):
 cs=hand(o,1)['known']
 return ('（公開済み：'+'、'.join(name(c['card'])+str(c['count'])+'枚' for c in cs)+'）' if cs else '')+'。'
def stack_text(o,compact=False):
 stack=o.get('public',{}).get('stack',[])
 if not stack:return ''
 texts=[]
 for c in stack:
  owner=who(c['controller']);a=c.get('ability');src=c.get('source',{})
  if a:
   label=identity(src,o)
   if a=='prowess':desc=label+'の果敢'+('' if compact else '（ターン終了時まで+1/+1）')
   elif '2 damage to caster' in a:desc=label+'の誘発（'+who(c.get('affected_player',0))+'に2点'+('' if compact else 'ダメージ')+'）'
   elif 'reveal defending' in a:desc=label+'の誘発（'+who(1-c['controller'])+'のライブラリーの一番上を公開）'
   elif a=='fetch land, then shuffle':desc=label+'の能力（土地を探す）'
   elif a=='Sunbaked Canyon: draw a card':desc=label+'の能力（1枚引く）'
   elif a=='suspend: remove a time counter':desc='裂け目の稲妻の待機（時間カウンターを1個取り除く）'
   elif a=='suspend: cast Rift Bolt':
    if compact and len(stack)==1:return 'スタック：裂け目の稲妻を待機から唱える能力。'
    desc='裂け目の稲妻の待機（唱えるか選ぶ）'
   else:desc=label+'の能力'
   texts.append(owner+'の'+desc)
  else:
   targets='、'.join(subject(t,o) for t in c.get('targets',[]))
   texts.append(owner+'の'+name(c['card'])+('（'+('対象：' if compact else '')+targets+'）' if targets else ''))
 return ('スタック（解決順）：' if compact else 'スタック（次に解決する順）：')+(' → ' if compact else '→ ').join(texts)+'。'
def extra(o,compact=False):
 p=o.get('public',{});lines=[];suspended=p.get('suspended',[])
 if suspended:
  lines.append('待機：'+'、'.join(who(c['owner'])+'の'+name(c['card'])+(f" #{c['object_id']}" if sum(x['card']==c['card'] and x['owner']==c['owner'] for x in suspended)>1 else '')+f"（時間カウンター{c['time_counters']}個）" for c in suspended)+'。')
 if p.get('stack'):lines.append(stack_text(o,compact))
 for p,m in enumerate(p.get('mana',[])):
  if any(m.values()):lines.append(who(p)+'のマナプール：'+'、'.join(k+str(m[k]) for k in 'WUBRGC' if m.get(k))+'。')
 return lines

def action_label(a,o):
 k=a['kind'];c=name(a.get('card'));ts='と'.join(subject(t,o,menu=True) for t in a.get('targets',[]))
 if k=='pass':return '何もせず優先権を渡す'
 if k=='keep':return 'キープする'
 if k=='mulligan':return 'マリガンする'
 if k=='play_land':return c+'を出す'
 if k=='play_shockland':return ('2点のライフを支払って'+c+'を出す') if a['pay_2_life'] else c+'をタップ状態で出す'
 if k=='suspend':return c+'を待機する'
 if k=='fetch':return name(a['source']['card'])+'を生け贄に捧げ、1点支払って土地を探す'
 if k=='suspend_target':return subject(a['target'],o,menu=True)+'を対象に裂け目の稲妻を唱える（マナ不要、3点ダメージ）'
 if k=='decline_suspend':return '裂け目の稲妻を唱えず、追放領域に残す'
 if k=='sacrifice_canyon_to_draw':return name(a['source']['card'])+'の能力を起動する（1マナを支払い、峡谷をタップして生け贄に捧げ、1枚引く）'
 if k=='cast':
  mode=a.get('mode','');effect=''
  if mode=='your permanents gain indestructible this turn':effect='あなたのパーマネントにターン終了時まで破壊不能'
  elif mode=='target creature gains double strike this turn':effect=ts+'にターン終了時まで二段攻撃'
  elif mode=='4 damage to player':effect=ts+'に4点ダメージ'
  elif a['card']=='Searing Blaze':effect=ts+'に'+('、上陸で各3点ダメージ' if o.get('public',{}).get('this_turn',{}).get('land_entered',[False])[0] else '各1点ダメージ')
  elif a['card'] in ('Lightning Bolt','Lava Spike','Rift Bolt','Skewer the Critics','Lightning Helix'):effect=ts+'に3点ダメージ'+('、あなたは3点回復' if a['card']=='Lightning Helix' else '')
  method='を唱える'
  if a['card']=='Skewer the Critics':method='を'+('絢爛コスト' if a.get('spectacle') else '通常コスト')+'で唱える';effect=('赤1マナ、' if a.get('spectacle') else '3マナ、')+effect
  return c+method+('（'+effect+'）' if effect else '')
 if k in ('fetch_plan','search_plan'):
  source=name(a.get('source',{}).get('card','フェッチ'))
  dest=c+('をタップ状態で出す' if c=='聖なる鋳造所' and not a['pay_2_life'] else 'を出す') if c else '土地を見つけない'
  return (source+'を起動して' if k=='fetch_plan' else '')+dest+('（追加で2点支払い）' if a['pay_2_life'] else '')
 if k=='shockland_entry':return '2点支払って出す' if a['pay_2_life'] else 'タップ状態で出す'
 if k in ('bottom','discard'):return c if k=='bottom' else c+'を捨てる'
 if k=='fetch_selection':return name((a.get('selection') or {}).get('card','土地を見つけない'))
 if k=='order_trigger':return f"誘発{a['trigger_id']}を選ぶ"
 return k

def menu(state):
 e=state.get('editor') or {};o=state['observation'];items=[]
 if e.get('kind')=='choices':
  for c in sorted(e['choices'],key=lambda c:c['action']['kind']!='pass'):
   a=c['action']
   if a['kind']=='play_land' and a['card']=='Sacred Foundry':
    for pay in (False,True):items.append((action_label({'kind':'play_shockland','card':a['card'],'pay_2_life':pay},o),{'primitive_selections':[{'index':c['index']},{'index':'1' if pay else '0'}]}))
   else:items.append((action_label(a,o),{'index':c['index']}))
 elif e.get('kind')=='attack':
  ids=e['attackers'];lookup={c['object_id']:c for c in bf(o,0)}
  # No attack is always first; individual choices avoid exponential menus.
  if len(ids)==1:items=[('攻撃しない',{'attackers':[]}), (name(lookup[ids[0]]['card'])+'で攻撃する',{'attackers':ids})]
  else:items=[('攻撃しない',{'attackers':[]}),('全員で攻撃する',{'attackers':ids}),('攻撃するクリーチャーを個別に選ぶ',{'edit':'attack'})]
 elif e.get('kind') in ('block','damage'):items=[('割り当てを選ぶ',{'edit':e['kind']})]
 return items

def menu_text(state):return '\n'.join(f'{i}. {text}' for i,(text,_) in enumerate(menu(state),1))+'\ns: 全体状況'
def full(o,status=False):
 if o['phase']=='mulligan':return f"初手選択。あなたのマリガン：{o.get('mulligans_taken',[0])[0]}回。\n手札{hand(o)['size']}枚："+known(hand(o)['known'])+f"。\nbotの手札{hand(o,1)['size']}枚。"
 lines=[]
 if status:lines.append(f"{who(o.get('active_player',0))}の{turn(o)}ターン目・{PHASE.get(o['phase'],o['phase'])}。ライフ：あなた{life(o,0)}／bot{life(o,1)}。")
 else:lines.append(f'ライフ：あなた{life(o,0)}／bot{life(o,1)}。')
 if not bf(o,0) and not bf(o,1):lines.append('戦場：双方とも空。')
 else:lines.extend(['あなたの戦場：'+battlefield(o,0),'botの戦場：'+battlefield(o,1)])
 lines.append(f"手札{hand(o)['size']}枚："+known(hand(o)['known'])+'。')
 lines.append(f"botの手札{hand(o,1)['size']}枚"+known_suffix(o))
 if status:
  graves=[who(p)+'の墓地：'+known(o['players'][p]['graveyard'])+'。' for p in (0,1)]
  lines.extend(graves if all(o['players'][p]['graveyard'] for p in (0,1)) else [''.join(graves)])
  lines.append(f"ライブラリー：あなた{o['players'][0]['library']['size']}枚／bot{o['players'][1]['library']['size']}枚。")
  for p in (0,1):
   for c in o['players'][p]['library'].get('known_positions',[]):
    if c.get('position_from_top')==1:lines.append(who(p)+'のライブラリーの一番上：'+name(c['card'])+'（公開済み）。')
   if o['players'][p].get('ordinary_exile'):lines.append(who(p)+'の追放領域：'+known(o['players'][p]['ordinary_exile'])+'。')
 lines.extend(extra(o,not status))
 if o['phase']=='precombat_main' and not o.get('public',{}).get('this_turn',{}).get('land_plays',[0])[0]:
  if status:lines.append('土地は未プレイ。')
  elif lines[-1].startswith('botの手札'):lines[-1]+='土地は未プレイ。'
 return '\n'.join(lines)
def status_text(state):return full(state['observation'],True)+'\n'+menu_text(state)
def changes(rows,p):return [(life(r['before'],p),life(r['after'],p)) for r in rows if not r.get('outcome') and life(r['before'],p)!=life(r['after'],p)]
def taps(rows):return [name(r['action']['source']['card']) for r in rows if r['action']['kind']=='tap_for_mana']
def hand_delta(before,after,p=0,owner=False):
 b=hand(before,p)['size'];a=hand(after,p)['size']
 return (who(p)+'の' if owner else '')+f'手札：{b}→{a}枚。' if b!=a else ''
def life_delta(before,after,p):return f'{who(p)}のライフ：{life(before,p)}→{life(after,p)}。' if life(before,p)!=life(after,p) else ''
def draw_name(rows,p):
 for r in rows:
  if r['before']['phase']!='draw' and r['after']['phase']=='draw':return drawn(r['before'],r['after'],p)
 return None
def drawn(before,after,p):
 diff=Counter({c['card']:c['count'] for c in hand(after,p)['known']})-Counter({c['card']:c['count'] for c in hand(before,p)['known']})
 return name(next(iter(diff))) if diff else None

def objects(o):return {c['object_id']:c for p in (0,1) for c in bf(o,p)}
def left_field(before,after):
 current=objects(after)
 return [c for oid,c in objects(before).items() if oid not in current]
def death(c,o=None):return who(c['controller'])+'の'+identity(c,o)+'が戦場を離れました。'
def payment_text(rows,compress=False):
 paid=taps(rows)
 if compress and len(paid)>1 and len(set(paid))==1:return paid[0]+str(len(paid))+'枚'
 return 'と'.join(paid)
def reveal_text(rows):
 return ''.join(who(e['player'])+'は'+name(e['card'])+'を公開し'+('て手札に加えました。' if e['destination']=='hand' else '、ライブラリーの一番上に残しました。') for r in rows for e in r.get('events',[]))
def power_changes(before,after):
 old=objects(before)
 return [c for oid,c in objects(after).items() if oid in old and 'power' in c and (c['power'],c['toughness'])!=(old[oid]['power'],old[oid]['toughness'])]
def prowess_text(before,after,dialogue=False):
 cs=power_changes(before,after)
 if not cs:return ''
 c=cs[0];pt=f"{c['power']}/{c['toughness']}"
 if len(cs)>1 and len({(x['controller'],x['card'],x['power'],x['toughness']) for x in cs})==1:
  if dialogue:return name(c['card'])+f'{len(cs)}体は果敢でそれぞれ{pt}。'
  return who(c['controller'])+'の'+name(c['card'])+' '+ 'と '.join('#'+str(x['object_id']) for x in cs)+f'は果敢で{pt}。'
 return ''.join(who(c['controller'])+'の'+identity(c,after)+f"は果敢で{c['power']}/{c['toughness']}。" for c in cs)
def eidolon_changes(rows,dialogue=False):
 texts=[]
 for r in rows:
  if r.get('outcome'):continue
  stack=r['before'].get('public',{}).get('stack',[])
  if not stack or '2 damage to caster' not in stack[0].get('ability',''):continue
  a=stack[0];p=a.get('affected_player',0)
  if any(x['object_id']==a['object_id'] for x in r['after'].get('public',{}).get('stack',[])):continue
  if life(r['before'],p)==life(r['after'],p):continue
  src=a.get('source',{});label=identity(src,r['before']) if dialogue else name(src.get('card','Eidolon of the Great Revel'))
  texts.append(label+'の誘発で'+('、' if dialogue else '')+life_delta(r['before'],r['after'],p))
 return ''.join(texts)

def action_blocks(rows):
 # A user decision or bot action includes its automatic payments and resolution.
 starts={'play_land','play_shockland','cast','suspend','suspend_target','decline_suspend','fetch','fetch_plan','search_plan','sacrifice_canyon_to_draw','attack','block','discard','mulligan','bottom','keep','assign_damage'}
 blocks=[];block=[]
 for row in rows:
  if row['action']['kind'] in starts and not (row['action']['kind']=='block' and not row['action']['blocks']) and block:
   blocks.append(block);block=[]
  block.append(row)
  # A completed fetch must not absorb the trigger/combat that follows it.
  # Keep its search, entry choice and shuffle together as one action.
  fetch_done=row['action']['kind']=='shuffle' and any(r['action']['kind']=='fetch_selection' for r in block)
  reveal_done=bool(row.get('events')) and block[0]['action']['kind'] in ('pass','shuffle','order_trigger')
  old_stack=row['before'].get('public',{}).get('stack',[])
  new_ids={x['object_id'] for x in row['after'].get('public',{}).get('stack',[])}
  if fetch_done or reveal_done or (len([x for x in old_stack if x.get('kind')=='spell'])>1 and old_stack[0].get('kind')=='spell' and old_stack[0]['object_id'] not in new_ids):
   blocks.append(block);block=[]
 if block:blocks.append(block)
 return blocks

def _describe_action(rows,dialogue=False,ordinal=1):
 if not rows:return ''
 r=rows[0];a=r['action'];k=a['kind'];p=r['actor'];before=r['before'];after=rows[-1]['after'];c=name(a.get('card'));actor=who(p);opp=1-p
 pay=payment_text(rows,dialogue);prefix=('botは' if p else '')+(pay+'をタップし、' if pay else '')
 hs=hand_delta(before,after,p) if p==0 else ''
 dead=left_field(before,after);new=[x for oid,x in objects(after).items() if oid not in objects(before)]
 terminal=any(r.get('outcome') for r in rows)
 if k in ('play_land','play_shockland'):
  permanent=next((x for x in new if x['card']==a['card']),None)
  return ('botは' if p else '')+c+('をタップ状態で出しました。' if permanent and permanent['tapped'] else 'を出しました。')+life_delta(before,after,p)+hs
 if k in ('fetch','fetch_plan','search_plan','fetch_selection','shockland_entry'):
  source=a.get('source',{});dest=next((x for x in new if 'power' not in x),None)
  pending=any('fetch land' in x.get('ability','') for x in after.get('public',{}).get('stack',[]))
  start=actor+'は'+name(source.get('card'))+'を生け贄に捧げ' if source else actor+'は'
  if pending:return start+'ました。'+life_delta(before,after,p)+'\nスタック：'+actor+'の土地を探す能力。'
  if not dest:return start+'ました。土地は取得しませんでした。'+life_delta(before,after,p)
  paid=any(r['action']['kind']=='shockland_entry' and r['action']['pay_2_life'] for r in rows)
  entry='を2点払って出しました。' if paid else 'をタップ状態で出しました。' if dest['tapped'] else 'を出しました。'
  return start+('、' if source else '')+name(dest['card'])+entry+life_delta(before,after,p)
 if k=='sacrifice_canyon_to_draw':
  card=drawn(before,after,p) if p==0 else None
  text=('' if dialogue and p==0 else actor+'は')+(pay+'をタップし、' if dialogue and pay else '')+'灼陽大峡谷を生け贄に捧げ、'+(card+'を引きました。' if card else '1枚引きました。')
  return text+hand_delta(before,after,p,not dialogue)
 if k=='suspend':return prefix+c+'を待機しました（時間カウンター1個）。'+hs
 if k=='decline_suspend':return '裂け目の稲妻を唱えず、追放領域に残しました。'
 if k=='bottom':return c+'をライブラリーの一番下に戻しました。'+hand_delta(before,after,p) if p==0 else ''
 if k=='mulligan':return actor+f"は{before.get('mulligans_taken',[0,0])[p]+1}回目のマリガンを選びました。"
 if k=='discard':return (c if p==0 else 'botは'+c)+'を捨てました。'+hs
 if k=='keep':return f"botは手札{7-before.get('mulligans_taken',[0,0])[1]}枚でキープしました。" if p==1 else ''
 if k=='suspend_target':
  target=a['target'];pending=any(x.get('card')=='Rift Bolt' for x in after.get('public',{}).get('stack',[]))
  trigger=eidolon_changes(rows,dialogue)
  if pending:return subject(target,before)+'を対象に、裂け目の稲妻を待機から唱えました（マナ不要）。'+trigger
  text='待機から裂け目の稲妻を唱え、'+subject(target,before)+'に3点ダメージ。'
  return trigger+text+''.join(death(x,before) for x in dead)+''.join(life_delta(before,after,q) for q in (0,1) if q!=p or not trigger)
 if k=='cast':
  trigger=eidolon_changes(rows,dialogue);prowess=prowess_text(before,after,dialogue)
  pending=not terminal and any(x.get('card')==a['card'] and x.get('controller')==p for x in after.get('public',{}).get('stack',[]))
  if pending:
   if not dialogue:return actor+'は'+(pay+'をタップし、' if pay else '')+'と'.join(subject(t,before) for t in a.get('targets',[]))+'を対象に'+c+'を唱えました。解決前に対応できます。'+trigger+hs
   return prefix+('と'.join(subject(t,before) for t in a.get('targets',[]))+'を対象に' if a.get('targets') else '')+c+'を唱えました。'+trigger+hs
  creature=next((x for x in new if x['card']==a['card']),None)
  if creature:
   feature='召喚酔い' if creature.get('summoning_sick') else '速攻'
   entry=c+f"が戦場に出ました（{creature['power']}/{creature['toughness']}・{feature}）。"
   if trigger and dialogue:
    siblings=sum(x['card']==a['card'] for x in bf(before,p))
    if siblings:entry=entry.replace(c+'が',str(siblings+1)+'体目が');trigger=trigger.replace('大歓楽の幻霊の誘発で、','戦場の大歓楽の幻霊の誘発で')
    return prefix+c+'を唱えました。'+trigger+entry+hs
   return prefix+trigger+entry+hs
  if dialogue and p==1 and a.get('targets') and all('player' in t for t in a['targets']):
   label=(str(ordinal)+'枚目の' if ordinal>1 else '')+c
   text=trigger+prefix+'と'.join(subject(t,before) for t in a['targets'])+'を対象に'+label+'を解決しました。'
   if not terminal:text+=''.join(life_delta(before,after,t['player']) for t in a['targets'])+prowess
   return text
  if terminal:return trigger+prefix+c+'が解決しました。'
  targets=a.get('targets',[]);creature_targets=[t for t in targets if 'object_id' in t];mode=a.get('mode','')
  intro=prefix if dialogue else actor+'は'+(pay+'をタップし、' if pay else '')
  if mode in ('your permanents gain indestructible this turn','target creature gains double strike this turn'):
   effect='自分のパーマネントがターン終了時まで破壊不能になりました' if 'indestructible' in mode else subject(targets[0],before)+'がターン終了時まで二段攻撃を得ました'
   return trigger+actor+'は'+(pay+'をタップし、' if pay else '')+'ボロスの魔除けを解決しました。'+effect+'。'+hs
  if a['card']=='Searing Blaze':
   amount=3 if before.get('public',{}).get('this_turn',{}).get('land_entered',[False,False])[p] else 1
   if dialogue:
    text=intro+c+'を唱えました。'+trigger+prowess+'\n' if trigger else intro
    text+=c+'が解決しました。'+('上陸で' if amount==3 else '')+who(opp)+'と'+(subject(creature_targets[0],before) if creature_targets else '対象のクリーチャー')+f'に各{amount}点ダメージ。'+life_delta(before,after,opp)
    text+=''.join(death(x,before) for x in dead)
    if not trigger:text+=prowess
    return (text.replace('あなたのゴブリンの先達','ゴブリンの先達') if p==1 else text)+hs
   return trigger+intro+c+'が解決しました。'+life_delta(before,after,opp)+''.join(subject(t,before)+f'に{amount}点ダメージ。' for t in creature_targets)+''.join(death(x,before) for x in dead)+hs
  if (trigger or prowess) and not dialogue and not creature_targets:
   return intro+c+'を唱えました。'+trigger+prowess+c+'が解決し、'+life_delta(before,after,opp)+hs
  text=trigger+intro+c+('を絢爛で唱えました。' if a['card']=='Skewer the Critics' and a.get('spectacle') else 'を通常コストで唱えました。' if a['card']=='Skewer the Critics' else 'が解決しました。')
  text+=''.join(life_delta(before,after,t['player']) for t in targets if 'player' in t)
  text+=''.join(subject(t,before)+'に3点ダメージ。' for t in creature_targets)+''.join(death(x,before) for x in dead)
  if a['card']=='Lightning Helix':text+=life_delta(before,after,p)
  if dialogue:text+=prowess
  return text+hs
 if k=='attack':
  attackers=a.get('attackers',[])
  if not attackers:return '攻撃せず、戦闘後メインまで進みました。' if not dialogue else ''
  label='、'.join(name(x['card']) for x in attackers)
  if dialogue and len(attackers)>1 and len({x['card'] for x in attackers})==1:label=name(attackers[0]['card'])+str(len(attackers))+'体'
  text=('botの' if p else '')+label+'がタップして攻撃。'+reveal_text(rows)
  ds=changes(rows,opp)
  if ds:text+='ブロックされず、'+f'{who(opp)}のライフ：{ds[0][0]}→{ds[-1][1]}。'
  text+=''.join(death(x,before) for x in dead)
  return text
 if k=='block':
  blocks=a.get('blocks',[])
  if not blocks:return describe_resolution(rows,dialogue)
  text=''.join(identity(x['blocker'],before)+'で'+subject(x['attacker'],before)+'をブロックしました。' for x in blocks)
  if len(dead)==2:return text+'戦闘ダメージで両方が戦場を離れました。'
  return text+''.join(death(x,before) for x in dead)
 if k=='assign_damage':
  text=''
  for x in a.get('assignments',[]):
   target=objects(after).get(x['target']['object_id'],objects(before).get(x['target']['object_id'],x['target']))
   text+=subject(target,before)+'に'+str(x['damage'])+'点の戦闘ダメージ。'
   if target.get('indestructible'):text+='破壊不能のため戦場に残ります。'
  return text
 return describe_resolution(rows,dialogue)

def describe_action(rows,dialogue=False,ordinal=1):
 text=_describe_action(rows,dialogue,ordinal)
 if not rows:return text
 # Mana abilities can cost life even when the spell is still on the stack.
 costs=[r for r in rows if r['action']['kind']=='tap_for_mana' and not r.get('outcome') and life(r['before'],r['actor'])!=life(r['after'],r['actor'])]
 for p in (0,1):
  paid=[r for r in costs if r['actor']==p]
  if paid:
   delta=f"{who(p)}のライフ：{life(paid[0]['before'],p)}→{life(paid[-1]['after'],p)}。"
   if delta not in text:text+=delta
 return text

def describe_resolution(rows,dialogue=False):
 if not rows:return ''
 before=rows[0]['before'];after=rows[-1]['after'];dead=left_field(before,after)
 if before.get('turn')!=after.get('turn'):return ''
 text=reveal_text(rows)
 if text:return text
 if any('fetch land' in c.get('ability','') for c in before.get('public',{}).get('stack',[])) and any(r['action']['kind']=='fetch_selection' for r in rows):
  start=next(i for i,r in enumerate(rows) if r['action']['kind']=='fetch_selection')
  return describe_action(rows[start:],dialogue)
 if before['phase']=='cleanup':return ''
 if before['phase']=='first_strike_damage' and dead:return ''.join('先制戦闘ダメージで'+death(c,before) for c in dead)
 if before['phase']=='combat_damage' and not dead:
  combatants=[c for c in bf(before,0) if c.get('double_strike') and c.get('tapped')]
  if combatants and not changes(rows,1):return name(combatants[0]['card'])+'はブロックされているため、botへの戦闘ダメージはありません。'
 combat=[r for r in rows if r['after']['phase'] in ('first_strike_damage','combat_damage') and r['before']['phase']!=r['after']['phase']]
 damage=''.join(life_delta(r['before'],r['after'],p) for r in combat for p in (0,1))
 if damage:return '戦闘ダメージで、'+damage+''.join(death(c,before) for c in dead)
 text+=eidolon_changes(rows,dialogue)+prowess_text(before,after,dialogue)
 stack=before.get('public',{}).get('stack',[]);remaining={x['object_id'] for x in after.get('public',{}).get('stack',[])}
 for spell in stack:
  if spell.get('kind')!='spell' or spell['object_id'] in remaining:continue
  if any(x.get('outcome') for x in rows):continue
  card=name(spell['card']);created=[c for oid,c in objects(after).items() if oid not in objects(before) and c['card']==spell['card']]
  if created:
   c=created[0];entry=card+f"が戦場に出ました（{c['power']}/{c['toughness']}・"+('召喚酔い' if c.get('summoning_sick') else '速攻')+'）。'
   text+=('\n' if text else '')+entry;continue
  if text and dialogue:text+='\n'
  targets=spell.get('targets',[]);missing=[t for t in targets if 'object_id' in t and t['object_id'] not in objects(before)]
  if targets and len(missing)==len(targets):text+=card+'は対象がなくなり、解決されませんでした。';continue
  targets_c=[t for t in targets if 'object_id' in t]
  if targets_c:
   valid=[t for t in targets_c if t['object_id'] in objects(before)]
   if valid:
    text+=card+'が解決し、'+''.join(subject(t,before)+'に3点ダメージ。' for t in valid)
    for t in valid:
     if t['object_id'] in objects(after):
      c=objects(after)[t['object_id']];text+=('速槍' if c['card']=='Monastery Swiftspear' else name(c['card']))+f"は{c['power']}/{c['toughness']}で戦場に残ります"+(f"（{c['damage_marked']}点ダメージ）" if c.get('damage_marked') else '')+'。'
    text+=''.join((identity(c,before)+'が戦場を離れました。' if dialogue else death(c,before)) for c in dead)
   else:text+=card+'が解決しました。'+''.join(life_delta(before,after,p) for p in (0,1))+'クリーチャーの対象は戦場にいません。'
  else:text+=card+'が解決しました。'+''.join(life_delta(before,after,p) for p in (0,1))
 if text:return text
 new=[c for oid,c in objects(after).items() if oid not in objects(before)]
 if new:
  return ''.join(name(c['card'])+f"が戦場に出ました（{c['power']}/{c['toughness']}・"+('召喚酔い' if c.get('summoning_sick') else '速攻')+'）。' for c in new if 'power' in c)
 if before['phase']!='draw' and after['phase']=='draw':
  p=after.get('active_player',0);card=drawn(before,after,p)
  return who(p)+'は'+('公開済みの' if p else '')+card+'を引きました。'+hand_delta(before,after,p,True) if card else ''
 return ''.join(life_delta(before,after,p) for p in (0,1))+''.join(death(c,before) for c in dead)

def human_action(rows,final):return '\n'.join(filter(None,(describe_action(b) for b in action_blocks(rows))))
def bot_actions(rows):return human_action(rows,rows[-1]['after'] if rows else {})

def outcome_text(outcome):
 winner=outcome.get('winner');reason=outcome.get('reason')
 if reason=='concede':return ('botが投了しました。あなたの勝利です。' if winner==0 else 'あなたが投了しました。botの勝利。')
 return '引き分けです。' if winner is None else 'あなたの勝利です。' if winner==0 else 'あなたの敗北です。botの勝利。'
def destination(o):return f"{who(o.get('active_player',0))}の{turn(o)}ターン目・{PHASE.get(o['phase'],o['phase'])}まで進みました。"
def bot_heading(o,starting,draw=None):
 early=o['phase']=='upkeep'
 text=(f'botの{turn(o)}ターン目・アップキープです。まだドロー前です。' if early else destination(o))
 if early:text+='botの'
 elif o['turn']==1 and starting==1:text+='先手のためドローはありません。botの'
 else:text+='botは'+('公開済みの'+draw+'を引き、' if draw else '1枚引き、')
 text+=f"手札{hand(o,1)['size']}枚"
 text+=('。' if draw else known_suffix(o))
 return text+f'ライフ：あなた{life(o,0)}／bot{life(o,1)}。'
def settled_block(block):
 """Whether further automatic steps can still change this action's description."""
 end=block[-1]['after'];public=end.get('public',{});kind=block[0]['action']['kind']
 if block[-1]['action']['kind']=='shuffle' and any(r['action']['kind']=='fetch_selection' for r in block):return True
 if public.get('stack') or public.get('execution',{}).get('kind')!='priority':return False
 if kind in ('attack','block'):return end['phase'] in ('end_combat','postcombat_main','end_step')
 return kind in ('play_land','play_shockland','cast','suspend','fetch','fetch_plan','search_plan','sacrifice_canyon_to_draw','discard')

def opening_decisions(rows):
 return '\n'.join(describe_action([r]) for r in rows if r['actor']==1 and r['action']['kind'] in ('mulligan','keep'))

def upkeep_events(rows):
 """Resolved events stay chronological even when there is no input boundary."""
 texts=[]
 for r in rows:
  if r['before']['phase']!='upkeep':continue
  if r['action']['kind']=='suspend_target':
   texts.append(who(r['actor'])+'は'+subject(r['action']['target'],r['before'])+'を対象に、裂け目の稲妻を待機から唱えました（マナ不要）。')
  elif r['before'].get('public',{}).get('stack'):
   stack=r['before']['public']['stack']
   text=describe_resolution([r],dialogue=stack[0].get('kind')!='spell')
   if text:texts.append(text)
 return texts

def render(previous,state,progress=False):
 o=state['observation'];parts=[];confirmed=''
 if previous is None:
  if o['phase']=='mulligan':
   heading='バーン同士、あなたが'+('先手' if state['starting']==0 else '後手')+f"です。\n初手{hand(o)['size']}枚："+known(hand(o)['known'])+'。'
   decisions=opening_decisions(state.get('timeline',[]))
   if decisions:heading+='\n'+decisions
   return heading if progress else heading+'\n'+menu_text(state)
  if progress:return ''
  return full(o,True)+'\n'+menu_text(state)
 prev=previous['observation'];rows=[r for r in state.get('timeline',[]) if previous['sequence']<r['sequence']<=state['sequence']]
 if o['phase']=='mulligan':
  if progress:return ''
  actions=[r for r in rows if r['actor']==0 and r['action']['kind'] in ('mulligan','bottom')]
  if actions:
   r=actions[-1];n=o.get('mulligans_taken',[0])[0];size=hand(o)['size'];names=known(hand(o)['known'])
   if r['action']['kind']=='mulligan':parts.append(f'{n}回目のマリガン。7枚引き直しました。\n手札7枚：{names}。\n{n}枚をライブラリーの一番下に戻してください。残り{n}枚。')
   else:
    parts.append(name(r['action']['card'])+f'をライブラリーの一番下に戻しました。手札：{size+1}→{size}枚。\n手札{size}枚：{names}。')
    left=size-(7-n)
    if left:parts.append(f'戻すカードは残り{left}枚です。')
  decisions=opening_decisions(rows)
  if decisions:parts.append(decisions)
  parts.append(menu_text(state));return '\n'.join(parts)
 if prev['phase']=='mulligan':
  decisions=opening_decisions(rows)
  if decisions:parts.append(decisions)
  parts.append('初手が決まりました。');confirmed='\n'.join(parts)
 turns=[]
 for row in rows:
  for obs in (row['before'],row['after']):
   t=obs.get('turn')
   if t is not None and t not in turns:turns.append(t)
 if o.get('turn') is not None and o['turn'] not in turns:turns.append(o['turn'])
 skipped_attack=False
 for t in turns:
  rr=[r for r in rows if r['before'].get('turn')==t]
  observations=[r['before'] for r in rr]+[r['after'] for r in rr if r['after'].get('turn')==t]
  if o.get('turn')==t:observations.append(o)
  if not observations:continue
  new=t!=prev.get('turn');p=observations[0].get('active_player',0)
  main=next((x for x in observations if x['phase']=='precombat_main'),None)
  upkeep=upkeep_events(rr) if new and p==1 else []
  if progress and new and main is None and o.get('turn')==t and not upkeep:return confirmed
  early=main is None and o.get('turn')==t and o['phase']=='upkeep'
  start=o if early else main or observations[0];d=draw_name(rr,p)
  if new:
   if parts and parts[-1]!='初手が決まりました。' and not skipped_attack:parts.append('')
   if p:
    entry=next((x for x in observations if x['phase']=='upkeep'),start) if upkeep else start
    heading=bot_heading(entry,state.get('starting',0),None if upkeep else d)
    if skipped_attack:heading='攻撃せず、'+heading;skipped_attack=False
    parts.append(heading)
    if upkeep:
     parts.extend(upkeep)
     if main is not None:parts.append(bot_heading(main,state.get('starting',0),d))
   else:
    heading=f'あなたの{turn(start)}ターン目・アップキープです。まだドロー前です。' if early else ('先手のためドローはありません。' if t==1 and state.get('starting')==0 else '')+destination(start)+(d+'を引きました。' if d else '')
    parts.extend([heading,full(start)])
   # The turn snapshot replaces routine upkeep/draw narration, but a life
   # total alone does not explain resolved damage triggers during that time.
   entry_rows=[r for r in rr if early or (main is not None and r['before']['phase'] in ('upkeep','draw'))]
   entry_damage='' if upkeep else eidolon_changes(entry_rows,True)
   if entry_damage:parts.append(entry_damage)
   confirmed='\n'.join(parts)
  # Turn entry has already summarized automatic upkeep and draw operations.
  active_rows=[r for r in rr if (not new or not early) and (not new or main is None or r['before']['phase'] not in ('upkeep','draw'))]
  # Routine turn-end resets are reflected in state, without standalone notices.
  boundaries=[r for r in active_rows if r['before'].get('turn')!=r['after'].get('turn')]
  active_rows=[r for r in active_rows if r not in boundaries]
  no_attack=any(r['action']['kind']=='attack' and not r['action']['attackers'] for r in active_rows)
  if no_attack and p==0 and o.get('turn')!=t:skipped_attack=True
  blocks=action_blocks(active_rows)
  spell_counts=Counter()
  for block_index,block in enumerate(blocks):
   # Draw-only transitions belong to the turn heading or the explicit draw line below.
   if all(r['action']['kind'] in ('pass','shuffle','order_trigger','pay_mana') for r in block) and not any(r['before'].get('public',{}).get('stack') for r in block):
    block=[r for r in block if not (r['before']['phase']!='draw' and r['after']['phase']=='draw')]
    if not block:continue
   a=block[0]['action']
   if a['kind']=='cast':spell_counts[a['card']]+=1
   text=describe_action(block,True,spell_counts.get(a.get('card'),1))
   if text and p==1 and no_attack and a['kind']=='cast' and block[0]['before']['phase']=='postcombat_main':text=text.replace('botは','botは攻撃せず、戦闘後メインに',1).replace('をタップし、','をタップして、',1)
   if text:parts.append(text)
   if block_index<len(blocks)-1 or settled_block(block):confirmed='\n'.join(parts)
  if o.get('turn')!=t:confirmed='\n'.join(parts)
  if not new and d and p==0:parts.append(d+'を引きました。'+hand_delta(prev,o))
  if not state.get('outcome') and o.get('turn')==t:
   if (not new and prev['phase']!=o['phase']) or (new and not early and start['phase']!=o['phase']):parts.append(destination(o))
   if not (new and p==0) and o.get('public',{}).get('stack'):parts.append(stack_text(o,True))
   if any(c.get('ability')=='suspend: cast Rift Bolt' for c in o.get('public',{}).get('stack',[])):parts.append('裂け目の稲妻を唱える対象を選んでください。')
 if progress:return confirmed
 if state.get('outcome'):parts.append(outcome_text(state['outcome']))
 else:parts.append(menu_text(state))
 return '\n'.join(parts)
