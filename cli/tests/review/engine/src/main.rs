//! Test-only capture of legal rule transitions. No UI formatter or expected text.
use burn_engine::{*,harness::*,terminal_bot::macro_control::{Controls,Mode,primitive}};
use mtg_state_design::burn::{World,Execution};
use serde_json::{Value as V,json};
fn base(actor:Player)->Engine {
 let mut w=World::default(); w.identity.next=1;
 for seat in 0..2 {w.players[seat].life=20;for c in [MOUNTAIN,FOUNDRY,BOLT,GUIDE,SWIFT,CANYON,VANTAGE,HELIX] {w.zones.library[seat].push(c).unwrap();}}
 Engine::fixture(w,actor,3+actor.0 as u32)
}
struct Capture {p:Position,c:Controls,rows:Vec<V>,before:V,last:V,outcome:V}
impl Capture {
 fn new(e:Engine)->Self {let p=Position::new(e);let before=p.observe(Player(0)).agent_view();Self {p,c:Controls::new(Player(0),Mode::named("both"),None),rows:vec![],before:before.clone(),last:before,outcome:V::Null}}
 fn step(&mut self,actor:Player,a:Action){
  let before=self.p.observe(Player(0)).agent_view();
  let visible=if actor==Player(1)&&matches!(a,Action::Bottom{..}){json!({"kind":"bottom","card":"private"})}else{self.p.observe(Player(0)).action_view(if actor==Player(1)&&!matches!(a,Action::LandPlan{..}){primitive(a)}else{a})};
  let events=self.c.apply(&mut self.p,actor,a).unwrap(); self.p.audit().unwrap();
  if let Some(outcome)=self.p.engine.finished(){self.outcome=match outcome{Outcome::Won{winner}=>json!({"winner":winner.0,"reason":"game"}),Outcome::Conceded{loser}=>json!({"winner":1-loser.0,"reason":"concede"}),_=>json!({"winner":null,"reason":"draw"})};self.last=before.clone();}else{self.last=self.p.observe(Player(0)).agent_view();}
  self.rows.push(json!({"sequence":self.rows.len()+1,"actor":actor.0,"action":visible,"before":before,"after":self.last,"events":events.iter().map(|e|e.agent_view()).collect::<Vec<_>>(),"outcome":self.outcome}));
 }
 fn settle(&mut self){let mut rng=Random(94);for _ in 0..150 {
  if self.p.engine.finished().is_some(){return}
  if self.p.engine.world().stack.len==0 && matches!(self.p.engine.execution(),Execution::Priority{..}) {return}
  let (actor,a)=match self.c.step(&self.p.engine).unwrap(){AutoStep::Automatic{actor,action,..}=>(actor,action),AutoStep::Decision{actor,actions}=>{let a=(0..actions.len()).map(|i|actions.get(i)).find(|a|*a==Action::Pass).unwrap_or_else(||actions.get(0));(actor,a)},AutoStep::Chance=>(self.p.engine.actor().unwrap_or(Player(0)),chance_action(&self.p.engine,&mut rng).unwrap()),AutoStep::Finished(_)=>return};self.step(actor,a);
 }panic!("settle watchdog")}
 fn emit(&self,id:&str,actor:Player){println!("{}",json!({"id":id,"origin":"engine fixture; applied transitions and inventory audited","actor":actor.0,"before":self.before,"after":self.last,"rows":self.rows,"outcome":self.outcome}));}
}
fn cast(card:Card,target:Subject,mode:u8,spectacle:bool)->Action{Action::Cast{card,targets:[target,Subject::Absent],mode,spectacle}}
fn main(){
 if std::env::args().any(|a| a=="guide-auto") {guide_auto();return}
 if std::env::args().any(|a| a=="policy-goldens") {policy_goldens();return}
 if std::env::args().any(|a| a=="eidolon-response") {eidolon_response();return}
 if std::env::args().any(|a| a=="fetch-guide") {fetch_guide();return}
 for seat in 0..2 {let actor=Player(seat);let other=Player(1-seat);
  for source in [MESA,MIRE] {for (dest,pay,label) in [(Some(MOUNTAIN),false,"mountain"),(Some(FOUNDRY),false,"tapped"),(Some(FOUNDRY),true,"paid"),(None,false,"fail")]{
   let mut e=base(actor);let id=e.enter(source,actor,false).unwrap();let mut c=Capture::new(e);c.step(actor,Action::FetchPlan{object:id,card:dest,pay});c.settle();c.emit(&format!("fetch-{seat}-{}-{label}",source.0),actor);
  }}
  for (card,label) in [(MESA,"mesa"),(MIRE,"mire"),(CANYON,"canyon"),(VANTAGE,"vantage"),(MOUNTAIN,"mountain")] {
   let mut e=base(actor);e.world_mut().zones.hand[seat as usize][card.0 as usize]=1;let mut c=Capture::new(e);c.step(actor,Action::Land{card});c.settle();c.emit(&format!("land-{seat}-{label}"),actor);
  }
  for pay in [false,true] {let mut e=base(actor);e.world_mut().zones.hand[seat as usize][FOUNDRY.0 as usize]=1;let mut c=Capture::new(e);c.step(actor,Action::LandPlan{card:FOUNDRY,pay});c.settle();c.emit(&format!("shock-{seat}-{pay}"),actor);}
  let mut e=base(actor);for _ in 0..3{e.enter(MOUNTAIN,actor,false).unwrap();}e.world_mut().zones.hand[seat as usize][VANTAGE.0 as usize]=1;let mut c=Capture::new(e);c.step(actor,Action::Land{card:VANTAGE});c.emit(&format!("fastland-{seat}-fourth"),actor);
  for pool in [false,true] {let mut e=base(actor);let id=e.enter(CANYON,actor,false).unwrap();if pool{e.world_mut().players[seat as usize].mana[RED]=1}else{e.enter(MOUNTAIN,actor,false).unwrap();}let mut c=Capture::new(e);c.step(actor,Action::Canyon{object:id});c.settle();c.emit(&format!("canyon-draw-{seat}-{pool}"),actor);}
  for card in [GUIDE,SWIFT,EIDOLON,BOLT,SPIKE,RIFT,SKEWER,CHARM,HELIX] {
   let modes:Vec<(u8,bool,bool)>=if card==CHARM{vec![(0,false,false),(1,false,false),(2,true,false)]}else if card==SKEWER{vec![(0,false,false),(0,false,true)]}else if [BOLT,RIFT,HELIX].contains(&card){vec![(0,false,false),(0,true,false)]}else{vec![(0,false,false)]};
   for (mode,creature,spectacle) in modes {
    let mut e=base(actor);e.world_mut().zones.hand[seat as usize][card.0 as usize]=1;for _ in 0..3{e.enter(FOUNDRY,actor,false).unwrap();}
    if spectacle{e.world_mut().turn_history.life_lost_mask|=1<<other.0;}
    let target=if creature{Subject::Object{id:e.enter(GUIDE,other,false).unwrap()}}else if [GUIDE,SWIFT,EIDOLON].contains(&card)||(card==CHARM&&mode==1){Subject::Absent}else{Subject::Player{player:other}};
    let mut c=Capture::new(e);c.step(actor,cast(card,target,mode,spectacle));c.settle();c.emit(&format!("cast-{seat}-{}-{mode}-{creature}-{spectacle}",card.0),actor);
   }
  }
  for landfall in [false,true] {let mut e=base(actor);e.world_mut().zones.hand[seat as usize][BLAZE.0 as usize]=1;for _ in 0..2{e.enter(MOUNTAIN,actor,false).unwrap();}e.world_mut().turn_history.land_entered_mask=if landfall{1<<seat}else{0};let id=e.enter(GUIDE,other,false).unwrap();let mut c=Capture::new(e);c.step(actor,Action::Cast{card:BLAZE,targets:[Subject::Player{player:other},Subject::Object{id}],mode:0,spectacle:false});c.settle();c.emit(&format!("blaze-{seat}-{landfall}"),actor);}
  for ability in [SWIFT,EIDOLON]{let mut e=base(actor);e.world_mut().zones.hand[seat as usize][BOLT.0 as usize]=1;e.enter(MOUNTAIN,actor,false).unwrap();e.enter(ability,actor,false).unwrap();let mut c=Capture::new(e);c.step(actor,cast(BOLT,Subject::Player{player:other},0,false));c.settle();c.emit(&format!("trigger-{seat}-{}",ability.0),actor);}
  let mut e=base(actor);e.world_mut().zones.hand[seat as usize][RIFT.0 as usize]=1;e.enter(MOUNTAIN,actor,false).unwrap();let mut c=Capture::new(e);c.step(actor,Action::Suspend);c.settle();c.emit(&format!("suspend-{seat}"),actor);
  let mut e=base(actor);e.world_mut().players[other.0 as usize].life=3;e.world_mut().zones.hand[seat as usize][BOLT.0 as usize]=1;e.enter(MOUNTAIN,actor,false).unwrap();let mut c=Capture::new(e);c.step(actor,cast(BOLT,Subject::Player{player:other},0,false));c.settle();c.emit(&format!("lethal-spell-{seat}"),actor);
 }
}

// No human decision throughout the opponent's attack and Guide resolution.
fn guide_auto() {
 for top in [VANTAGE,BOLT] {
  let mut e=base(Player(1));
  e.enter(GUIDE,Player(1),false).unwrap();
  let lib=&mut e.world_mut().zones.library[0];
  let pos=(0..lib.len as usize).find(|&i|lib.data[i]==top).unwrap();
  lib.data.swap(pos,lib.len as usize-1);
  let mut c=Capture::new(e);
  let previous=json!({"sequence":0,"starting":0,"status":"thinking","observation":c.last,"editor":null,"timeline":[],"outcome":null});
  let mut attacked=false;
  for _ in 0..100 {
   if c.last["phase"]=="postcombat_main" {break}
   let (actor,action)=match c.c.step(&c.p.engine).unwrap() {
    AutoStep::Automatic{actor,action,..}=>(actor,action),
    AutoStep::Decision{actor,actions}=>{
     assert_eq!(actor,Player(1),"human must have no decision during this attack");
     let all:Vec<_>=(0..actions.len()).map(|i|actions.get(i)).collect();
     let action=all.iter().copied().find(|a|matches!(a,Action::Attack{len:1,..}))
      .or_else(||all.iter().copied().find(|a|*a==Action::Pass)).unwrap();
     (actor,action)
    },_=>panic!("unexpected Guide fixture boundary")
   };
   attacked|=matches!(action,Action::Attack{len:1,..});c.step(actor,action);
  }
  assert!(attacked);assert_eq!(c.last["phase"],"postcombat_main");
  assert_eq!(c.p.engine.world().players[0].life,18);
  assert_eq!(c.p.engine.world().zones.hand[0][top.0 as usize],if top==VANTAGE {1}else{0});
  let state=json!({"sequence":c.rows.len(),"starting":0,"status":"thinking","observation":c.last,"editor":null,"timeline":c.rows,"outcome":null});
  println!("{}",json!({"id":if top==VANTAGE {"guide-auto-land"}else{"guide-auto-spell"},"origin":"Audited engine transitions; no human decision during attack, reveal or combat damage","previous":previous,"state":state}));
 }
}

// Reproduce the reported response to Guide, including the following draw.
fn fetch_guide() {
 for pay in [false,true] {
 let mut w=World::default();w.identity.next=1;
 w.players[0].life=20;w.players[1].life=19;

 for (card,count) in [(SPIKE,1),(RIFT,2),(HELIX,2),(MESA,1)] {w.zones.hand[0][card.0 as usize]=count;}
 w.zones.hand[1][BOLT.0 as usize]=4;w.zones.graveyard[1][MESA.0 as usize]=1;
 for seat in 0..2 {for (card,count) in DECK_COUNTS.iter().enumerate() {
  let field=if (seat==0 && Card(card as u16)==MIRE) || (seat==1 && [MOUNTAIN,GUIDE].contains(&Card(card as u16))) {1}else{0};
  for _ in 0..count-w.zones.hand[seat][card]-w.zones.graveyard[seat][card]-field {w.zones.library[seat].push(Card(card as u16)).unwrap();}
 }}
 let mut e=Engine::fixture(w,Player(1),2);
 let mire=e.enter(MIRE,Player(0),false).unwrap();e.enter(MOUNTAIN,Player(1),true).unwrap();e.enter(GUIDE,Player(1),false).unwrap();
 let mut c=Capture::new(e);let mut rng=Random(94);
 // Walk normal rules to declare attackers and expose the Guide trigger to player 0.
 for _ in 0..50 {
  let o=&c.last;
  if o["public"]["stack"].as_array().unwrap().iter().any(|x|x["ability"].as_str().unwrap_or("").contains("reveal defending")) && c.p.engine.actor()==Some(Player(0)) {break}
  let (actor,a)=match c.c.step(&c.p.engine).unwrap(){
   AutoStep::Automatic{actor,action,..}=>(actor,action),
   AutoStep::Decision{actor,actions}=>{let a=(0..actions.len()).map(|i|actions.get(i)).find(|a|matches!(a,Action::Attack{len:1,..})).or_else(||(0..actions.len()).map(|i|actions.get(i)).find(|a|*a==Action::Pass)).unwrap();(actor,a)},
   _=>panic!("unexpected opening boundary")};c.step(actor,a);
 }
 assert_eq!(c.last["phase"],"declare_attackers");assert_eq!(c.p.engine.actor(),Some(Player(0)));
 c.rows.clear();c.before=c.last.clone();
 let previous=json!({"sequence":0,"starting":0,"status":"human","observation":c.before,"editor":c.c.actions(&c.p.engine).unwrap().editor(&c.p.observe(Player(0))),"outcome":null});
 c.step(Player(0),Action::FetchPlan{object:mire,card:Some(FOUNDRY),pay});
 for _ in 0..150 {
  if c.last["turn"]==3 && c.last["phase"]=="precombat_main" {break}
  let (actor,a)=match c.c.step(&c.p.engine).unwrap(){
   AutoStep::Automatic{actor,action,..}=>(actor,action),
   AutoStep::Decision{actor,actions}=>{let a=(0..actions.len()).map(|i|actions.get(i)).find(|a|*a==Action::Pass).unwrap_or_else(||actions.get(0));(actor,a)},
   AutoStep::Chance=>{
    // A legal deterministic shuffle whose top cards are Vantage then Mesa.
    let mut a=chance_action(&c.p.engine,&mut rng).unwrap();
    if let Action::Shuffle{ref mut order,len}=a {
     let lib=&c.p.engine.world().zones.library[0];
     for (offset,card) in [(1,VANTAGE),(2,MESA)] {
      let pos=(0..len as usize).find(|&i|lib.data[order[i] as usize]==card).unwrap();order.swap(pos,len as usize-offset);
     }
    }
    (Player(0),a)
   },AutoStep::Finished(_)=>panic!("unexpected game end")};c.step(actor,a);
 }
 assert_eq!(c.last["phase"],"precombat_main");assert_eq!(c.last["turn"],3);
 assert_eq!(c.p.engine.world().players[0].life,if pay{15}else{17});
 assert_eq!(c.p.engine.world().zones.hand[0][VANTAGE.0 as usize],1);
 assert_eq!(c.p.engine.world().zones.hand[0][MESA.0 as usize],2);
 let state=json!({"sequence":c.rows.len(),"starting":0,"status":"human","observation":c.last,"editor":c.c.actions(&c.p.engine).unwrap().editor(&c.p.observe(Player(0))),"timeline":c.rows,"outcome":null});
 println!("{}",json!({"id":if pay{"fetch-guide-paid"}else{"fetch-guide-tapped"},"origin":"Real engine transitions; deck inventory audited after every action; deterministic legal shuffle","previous":previous,"state":state}));
 }
}

impl Capture {
 fn snapshot(&self)->V {
  json!({"sequence":self.rows.len(),"starting":0,"status":"human","observation":self.last,"editor":self.c.actions(&self.p.engine).unwrap().editor(&self.p.observe(Player(0))),"timeline":self.rows,"outcome":self.outcome})
 }
 fn pass_step(&mut self,skewer:bool) {
  let (actor,action)=match self.c.step(&self.p.engine).unwrap() {
   AutoStep::Automatic{actor,action,..}=>(actor,action),
   AutoStep::Decision{actor,actions}=>{
    let all:Vec<_>=(0..actions.len()).map(|i|actions.get(i)).collect();
    let selected=if skewer && actor==Player(1) {all.iter().copied().find(|a|matches!(a,Action::Cast{card,targets,spectacle:true,..} if *card==SKEWER && targets[0]==Subject::Player{player:Player(0)}))}else{None};
    let a=selected.or_else(||all.iter().copied().find(|a|matches!(a,Action::SuspendTarget{target:Subject::Player{player:Player(0)}})))
     .or_else(||all.iter().copied().find(|a|*a==Action::Pass))
     .unwrap_or(all[0]);(actor,a)
   },_=>panic!("unexpected fixture boundary")};
  self.step(actor,action);
 }
}
fn eidolon_response() {
 // Both caster seats and either trigger controller, with pending/resolved spells.
 for seat in 0..2 {for owner in 0..2 {for card in [BOLT,SWIFT,CHARM,HELIX] {
  let actor=Player(seat);let mut e=base(actor);
  e.world_mut().zones.hand[seat as usize][card.0 as usize]=1;
  for _ in 0..3 {e.enter(FOUNDRY,actor,false).unwrap();}
  e.enter(EIDOLON,Player(owner),false).unwrap();
  let target=if [SWIFT,CHARM].contains(&card){Subject::Absent}else{Subject::Player{player:Player(1-seat)}};
  let mut c=Capture::new(e);c.step(actor,cast(card,target,if card==CHARM{1}else{0},false));
  for _ in 0..50 {
   if c.p.engine.world().players[seat as usize].life==18 {break}c.pass_step(false);
  }
  assert_eq!(c.p.engine.world().players[seat as usize].life,18);
  c.emit(&format!("eidolon-cast-{seat}-{owner}-{}-pending",card.0),actor);
  c.settle();c.emit(&format!("eidolon-cast-{seat}-{owner}-{}-resolved",card.0),actor);
 }}}
 let mut w=World::default();w.identity.next=1;w.players[0].life=9;w.players[1].life=13;
 w.zones.hand[0][SWIFT.0 as usize]=1;w.zones.hand[0][CHARM.0 as usize]=1;
 w.zones.hand[1][RIFT.0 as usize]=1;w.zones.hand[1][SKEWER.0 as usize]=2;
 let mut e=Engine::fixture(w,Player(1),8);
 let field=[vec![EIDOLON,FOUNDRY,FOUNDRY,CANYON],vec![MOUNTAIN,MOUNTAIN,MOUNTAIN]];
 for seat in 0..2 {for card in &field[seat] {e.enter(*card,Player(seat as u8),false).unwrap();}}
 for seat in 0..2 {for (card,count) in DECK_COUNTS.iter().enumerate() {
  let n=count-e.world().zones.hand[seat][card]-field[seat].iter().filter(|c|c.0 as usize==card).count() as u8;
  for _ in 0..n {e.world_mut().zones.library[seat].push(Card(card as u16)).unwrap();}
 }}
 let mut c=Capture::new(e);c.step(Player(1),Action::Suspend);c.settle();
 for _ in 0..100 {if c.last["turn"]==9 && c.last["phase"]=="postcombat_main" {break}c.pass_step(false);}
 assert_eq!(c.last["turn"],9);assert_eq!(c.last["phase"],"postcombat_main");
 let mut previous=c.snapshot();
 for (card,bot_life) in [("Rift Bolt",11),("Skewer the Critics",9),("Skewer the Critics",7)] {
  for _ in 0..100 {
   c.pass_step(card!="Rift Bolt");
   let stack=c.last["public"]["stack"].as_array().unwrap();
   if c.last["turn"]==10 && c.p.engine.actor()==Some(Player(0)) && stack.len()==1 && stack[0]["card"]==card && c.p.engine.world().players[1].life==bot_life {break}
  }
  assert_eq!(c.p.engine.world().players[1].life,bot_life);
  assert_eq!(c.last["public"]["stack"][0]["card"],card);
  let state=c.snapshot();println!("{}",json!({"id":format!("eidolon-bot-{bot_life}"),"origin":"Real engine transitions from constructed position; inventory audited after each operation","previous":previous,"state":state}));previous=state;
 }
}

// Fixture authoring only: no UI implementation or bot policy changes.
fn policy_goldens() {
 for starting in 0..2 {for count in 0..3 {
  let mut rng=Random(7319);let e=Engine::new([deck(&mut rng),deck(&mut rng)],Player(starting)).unwrap();
  let mut c=Capture::new(e);let mut taken=0;
  if starting==0 {c.step(Player(0),Action::Mulligan{take:false});c.rows.clear();c.before=c.last.clone();}
  for _ in 0..100 {
   let actor=c.p.engine.actor().unwrap_or(Player(1));
   if actor==Player(0) && c.last["phase"]=="mulligan" {c.step(actor,Action::Mulligan{take:false});continue}
   let action=match c.c.step(&c.p.engine).unwrap() {
    AutoStep::Chance=>chance_action(&c.p.engine,&mut rng).unwrap(),
    AutoStep::Automatic{action,..}=>action,
    AutoStep::Decision{actions,..}=>{
     let all:Vec<_>=(0..actions.len()).map(|i|actions.get(i)).collect();
     if all.iter().any(|a|matches!(a,Action::Mulligan{..})) {
      let take=taken<count;if take {taken+=1;}Action::Mulligan{take}
     }else{all[0]}
    },_=>panic!("opening ended too early")};
   c.step(actor,action);
   if actor==Player(1) && matches!(action,Action::Mulligan{take:false}) {break}
  }
  assert_eq!(taken,count);assert_eq!(c.last["players"][1]["hand"]["size"],7-count);
  c.emit(&format!("policy-bot-opening-{starting}-{count}"),Player(1));
 }}
 for variant in 0..3 {
  let mut w=World::default();w.identity.next=1;w.players[0].life=20;w.players[1].life=20;w.zones.hand[1][RIFT.0 as usize]=1;
  let mut e=Engine::fixture(w,Player(1),8);let mut field=[vec![],vec![MOUNTAIN,MOUNTAIN,MOUNTAIN]];
  if variant>0 {field[0].push(EIDOLON);}if variant==2 {field[0].push(SWIFT);}
  let mut target=Subject::Player{player:Player(0)};
  for seat in 0..2 {for card in &field[seat] {let id=e.enter(*card,Player(seat as u8),false).unwrap();if *card==SWIFT {target=Subject::Object{id};}}}
  for seat in 0..2 {for (card,count) in DECK_COUNTS.iter().enumerate() {
   let n=count-e.world().zones.hand[seat][card]-field[seat].iter().filter(|c|c.0 as usize==card).count() as u8;
   for _ in 0..n {e.world_mut().zones.library[seat].push(Card(card as u16)).unwrap();}
  }}
  let mut c=Capture::new(e);c.step(Player(1),Action::Suspend);c.settle();
  for _ in 0..100 {if c.last["turn"]==9 && c.last["phase"]=="postcombat_main" {break}c.pass_step(false);}
  // A normal draw may leave a land in the human hand; it cannot answer Rift.
  assert_eq!(c.last["turn"],9);assert_eq!(c.last["phase"],"postcombat_main");
  let previous=c.snapshot();
  for _ in 0..100 {
   if c.last["turn"]==10 && c.last["phase"]=="precombat_main" {break}
   let select=match c.c.step(&c.p.engine).unwrap(){AutoStep::Decision{actor:Player(1),actions}=>{
    (0..actions.len()).map(|i|actions.get(i)).find(|a|*a==Action::SuspendTarget{target})},_=>None};
   if let Some(a)=select {c.step(Player(1),a);}else{c.pass_step(false);}
  }
  assert_eq!(c.last["turn"],10);assert_eq!(c.last["phase"],"precombat_main");
  let mut state=c.snapshot();state["status"]=json!("thinking");state["editor"]=V::Null;
  println!("{}",json!({"id":format!("policy-bot-rift-auto-{variant}"),"origin":"Real engine transitions; each inventory audited; automatic upkeep reaches precombat main","previous":previous,"state":state}));
 }
}
