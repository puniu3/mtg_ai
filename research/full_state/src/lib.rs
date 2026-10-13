//! Concrete state-layout design, NOT a rules engine.
//! `live_bytes` is an explicit fieldwise wire-size model, NOT a working codec.
//! No Vec, Box, String, pointers or dynamically dispatched behaviors in canonical state.
#![allow(dead_code)]

pub trait Live {
    fn live_bytes(&self) -> usize;
}
macro_rules! primitive {
    ($($t:ty),*) => { $(impl Live for $t { fn live_bytes(&self)->usize {size_of::<Self>()} })* };
}
primitive!(u8, u16, u32, i32, bool);
impl<T: Live, const N: usize> Live for [T; N] {
    fn live_bytes(&self) -> usize {
        self.iter().map(Live::live_bytes).sum()
    }
}
impl<T: Live> Live for Option<T> {
    fn live_bytes(&self) -> usize {
        1 + self.as_ref().map_or(0, Live::live_bytes)
    }
}
macro_rules! record {
    ($name:ident { $($field:ident:$ty:ty),* $(,)? }) => {
        #[derive(Clone,Copy,Debug,Default,PartialEq,Eq, Hash)]
        pub struct $name {$(pub $field:$ty),*}
        impl Live for $name {fn live_bytes(&self)->usize {0 $(+self.$field.live_bytes())*}}
    };
}
// Every sum has an explicit empty/default variant for unused array storage.
// Active records must satisfy their state invariants; defaults are not game constructors.
macro_rules! sum {
    ($name:ident { $empty:ident, $($variant:ident { $($field:ident:$ty:ty),* $(,)? }),* $(,)? }) => {
        #[derive(Clone,Copy,Debug,Default,PartialEq,Eq, Hash)]
        pub enum $name { #[default] $empty, $($variant {$($field:$ty),*}),* }
        impl Live for $name {fn live_bytes(&self)->usize {
            match self { Self::$empty=>1, $(Self::$variant{$($field),*}=>1 $(+$field.live_bytes())*),* }
        }}
    };
}
macro_rules! id {
    ($($name:ident:$t:ty),*) => { $(
        #[derive(Clone,Copy,Debug,Default,PartialEq,Eq, Hash)]
        #[repr(transparent)] pub struct $name(pub $t);
        impl Live for $name {fn live_bytes(&self)->usize {size_of::<$t>()}}
    )* };
}
id!(Card:u16, Definition:u16, Behavior:u16, Symbol:u16, Player:u8,
    ObjectId:u32, EffectId:u32, RecordId:u32, Stamp:u32, TurnId:u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Pool<T: Copy, const N: usize> {
    pub len: u16,
    pub data: [T; N],
}
impl<T: Copy + Default, const N: usize> Default for Pool<T, N> {
    fn default() -> Self {
        assert!(N <= u16::MAX as usize);
        Self {
            len: 0,
            data: [T::default(); N],
        }
    }
}
impl<T: Copy + Live, const N: usize> Live for Pool<T, N> {
    fn live_bytes(&self) -> usize {
        2 + self.data[..self.len as usize]
            .iter()
            .map(Live::live_bytes)
            .sum::<usize>()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapacityError {
    Full,
    IdExhausted,
}
impl<T: Copy, const N: usize> Pool<T, N> {
    pub fn push(&mut self, value: T) -> Result<(), CapacityError> {
        if self.len as usize == N {
            return Err(CapacityError::Full);
        }
        self.data[self.len as usize] = value;
        self.len += 1;
        Ok(())
    }
}

// ID 0 is absent. IDs are not pool indices. Never reuse within a branch.
// Every ID namespace draws from this ONE allocator; rollback restores the allocator.
record!(Identity {
    next: u32,
    next_stamp: u32
});
impl Identity {
    pub fn allocate_range(&mut self, count: u32) -> Result<IdRange, CapacityError> {
        if count == 0 || self.next == 0 {
            return Err(CapacityError::IdExhausted);
        }
        let end = self
            .next
            .checked_add(count)
            .ok_or(CapacityError::IdExhausted)?;
        let out = IdRange {
            first: ObjectId(self.next),
            count,
        };
        self.next = end;
        Ok(out)
    }
}
record!(IdRange {
    first: ObjectId,
    count: u32
});
impl IdRange {
    pub fn contains(&self, id: ObjectId) -> bool {
        id.0 >= self.first.0 && id.0 - self.first.0 < self.count
    }
    // Preserve logical identity and order when one member becomes distinct.
    pub fn split(&self, id: ObjectId) -> Option<(Option<Self>, Self, Option<Self>)> {
        if !self.contains(id) {
            return None;
        }
        let before = id.0 - self.first.0;
        let after = self.count - before - 1;
        Some((
            (before > 0).then_some(Self {
                first: self.first,
                count: before,
            }),
            Self {
                first: id,
                count: 1,
            },
            (after > 0).then(|| Self {
                first: ObjectId(id.0 + 1),
                count: after,
            }),
        ))
    }
}
// Span refers to one named typed item pool, specified by the owning field.
// Compact/rewrite ALL spans together; a span is never a pointer into another snapshot.
record!(Span {
    start: u16,
    len: u16
});
sum!(Subject { Absent, Player{player:Player}, Object{id:ObjectId} });
sum!(Origin { None, Card{card:Card,owner:Player}, Token{definition:Definition,owner:Player} });
sum!(OwnedZone { Hand, Graveyard{}, Library{index:u16}, Exile{group:RecordId}, Outside{} });
record!(TrackedCard {
    id: ObjectId,
    card: Card,
    owner: Player,
    zone: OwnedZone
});
// Tracked hand/GY/exile/outside cards are removed from the anonymous counts.
// Library tracking is ONLY an overlay: the ordered card value stays in Library.
record!(CardCount {
    card: Card,
    owner: Player,
    count: u8
});
record!(PlayerCore {
    life: i32,
    mana: [u32; 6]
});
record!(BurnHistory {
    land_plays: [u8; 2],
    land_entered_mask: u8,
    life_lost_mask: u8
});
record!(ModernHistory {
    land_plays: [u8; 2],
    land_entered_mask: u8,
    life_lost_mask: u8,
    spells: [u32; 2],
    previous_turn_spells: [u32; 2],
    spell_colors: [u8; 2],
    draws: [u32; 2],
    life_gained: [u32; 2],
    permanents_left_mask: u8,
    noncreature_spells: [u32; 2],
    instant_sorcery_spells: [u32; 2],
});
record!(ObjectHistory {
    object: ObjectId,
    entered: TurnId,
    last_attack: TurnId,
    loyalty_uses: u8,
    activated: Span, // Usage items; per ability and scope, not all objects
});
record!(Usage {
    ability: Behavior,
    scope: TurnId,
    count: u32
});
// Status is only intrinsic state: tapped, flipped, phased, face down, transformed,
// controlled continuously since controller's turn began, deathtouch damage marked.
// Current keywords, P/T, controller after effects, and summoning-sick are derived.
record!(Permanent {
    ids: IdRange,
    origin: Origin,
    timestamp: Stamp,
    damage: i32,
    base_controller: Player,
    status: u16,
});
// A grouped run has consecutive IDs and consecutive timestamps by ordinal.
// Same origin/owner/base state; all sparse attributes must be homogeneous too.
record!(Attachment {
    attachment: ObjectId,
    attached_to: Subject
});
record!(Counter {
    subject: Subject,
    kind: Symbol,
    amount: u32,
    timestamp: Stamp
});
sum!(LocalValue {
    None, Named{name:Symbol},ChosenType{kind:Symbol},ChosenPlayer{player:Player},ChosenColor{color:u8},
    ClassLevel{level:u8},Prepared{},Exerted{controller:Player},
    PlotTurn{turn:TurnId},Impending{cast_impending:bool},
    Regeneration{shields:u32},RingBearer{player:Player},CastFacts{record:RecordId},
});
record!(LocalState {
    object: ObjectId,
    value: LocalValue
});
sum!(Expiry {
    Never, EndOfTurn{turn:TurnId},EndOfCombat{combat:RecordId},
    NextStep{player:Player,step:Step,after_turn:TurnId},
    SourceLeaves{source:ObjectId},WhileCondition{effect:EffectId},
});
sum!(ExileBinding { Unlinked,Source{source:ObjectId,ability:Behavior},Effect{effect:EffectId} });
sum!(Visibility { Public,FaceDown{viewers:u8} });
record!(ExileGroup {
    id: RecordId,
    binding: ExileBinding,
    visibility: Visibility
});
record!(ExiledCards {
    group: RecordId,
    cards: CardCount
});
// Permissions/expiry are stored once on the effect. Suspended cards use tracked IDs + Counter.
sum!(ManaRule { Ordinary,Restricted{rule:Behavior,context:RecordId} });
record!(SpecialMana {
    player: Player,
    color: u8,
    count: u32,
    rule: ManaRule,
    expiry: Expiry,
    snow: bool
});

// Choices attached to a stack object have a lifetime distinct from a casting frame.
record!(Target {
    subject: Subject,
    role: u16,
    division: u32
});
sum!(CastRoute {
    Normal,Alternative{rule:Behavior},Flashback{},Escape{},Evoke{},Dash{},
    Bestow{},Suspend{},Plot{},Warp{},Adventure{},Prepared{},
});
record!(PaidCost {
    kind: Behavior,
    count: u32,
    capture: RecordId
});
record!(CastRecord {
    id: RecordId,
    from: OwnedZone,
    route: CastRoute,
    x: u32,
    modes: u32,
    paid: Span, // PaidCost items; only facts read by a scoped card are retained.
    permission: EffectId,
});
record!(ManaSpent {
    cast: RecordId,
    colors: [u32; 6]
});
sum!(AbilitySource { None,Fixed{source:ObjectId},Parallel{first:ObjectId} });
sum!(StackBody {
    Empty,Spell{origin:Origin,face:u8,cast:RecordId},
    Ability{definition:Behavior,source:AbilitySource,capture:RecordId},
    SpellCopy{values:RecordId,cast:RecordId},
});
record!(StackRun {
    ids: IdRange,
    controller: Player,
    body: StackBody,
    targets: Span
});

sum!(Selection {
    None,One{subject:Subject},Objects{items:Span}, // IdRange items
    ExileGroup{group:RecordId},PlayerObjects{player:Player,filter:Behavior},
    All{filter:Behavior},
});
sum!(EffectBody {
    None,Pump{power:i32,toughness:i32},GrantAbility{ability:Behavior},
    Control{controller:Player},Copy{values:RecordId},
    Permission{player:Player,rule:Behavior,earliest:TurnId},
    Replacement{rule:Behavior,capture:RecordId},Prevention{remaining:u32,rule:Behavior},
    Rule{rule:Behavior,capture:RecordId}, // closed handwritten Rust match, NOT a DSL
});
record!(Effect {
    id: EffectId,
    source: ObjectId,
    controller: Player,
    timestamp: Stamp,
    selection: Selection,
    expiry: Expiry,
    body: EffectBody,
});
sum!(TriggerCondition {
    None,Step{player:Player,step:Step,after:TurnId},
    ZoneChange{object:ObjectId,rule:Behavior},Event{rule:Behavior,capture:RecordId},
});
record!(DelayedTrigger {
    id: EffectId,
    source: ObjectId,
    controller: Player,
    ability: Behavior,
    condition: TriggerCondition,
    capture: RecordId,
    remaining: u32,
    expiry: Expiry,
});
record!(PendingTrigger {
    ids: IdRange,
    source: AbilitySource,
    controller: Player,
    ability: Behavior,
    capture: RecordId,
});
sum!(LinkKind { Other,ExiledWith{},MovedTo{},AbilityPair{first:Behavior,second:Behavior} });
record!(Link {
    source: ObjectId,
    destination: Subject,
    kind: LinkKind
});
sum!(Stat { Zero,Fixed{value:i32},Defined{expression:Behavior} });
record!(AbilityValue {
    ability: Behavior,
    binding: RecordId
});
// Frozen values own their referenced item ranges. Copy is flattened; no pointers to live objects.
record!(FrozenValues {
    id: RecordId,
    name: Symbol,
    mana_cost: Definition,
    colors: u8,
    types: u16,
    supertypes: u8,
    subtypes: Span,
    abilities: Span,
    power: Stat,
    toughness: Stat,
    loyalty: i32,
    defense: i32,
});
sum!(CaptureBody {
    None,Number{value:i32},Object{object:ObjectId},Objects{objects:Span},
    MovedCards{cards:Span}, // Tracked ID-range items, not anonymous same-name cards
    DamageSource{source:ObjectId,controller:Player,qualities:u16},
    Characteristics{values:RecordId,controller:Player,counters:Span,status:u16},
    Cast{cast:RecordId},Pair{first:RecordId,second:RecordId},
});
record!(Capture {
    id: RecordId,
    body: CaptureBody
});
record!(Emblem {
    ids: IdRange,
    definition: Definition,
    owner: Player
});
record!(PlayerDesignation {
    player: Player,
    city_blessing: bool,
    ring_level: u8
});
sum!(Companion { Absent,Available{card:Card},Taken{} });

sum!(Step {
    Untap,Upkeep{},Draw{},PrecombatMain{},BeginCombat{},Attackers{},Blockers{},
    FirstDamage{},NormalDamage{},EndCombat{},PostcombatMain{},End{},Cleanup{},
});
record!(Attacker {
    attackers: IdRange,
    defender: Subject,
    was_blocked: bool,
});
record!(Block {
    blocker: ObjectId,
    attacker: ObjectId
});
// Initial version splits individually blocked creatures. Bulk unblocked attacks remain grouped.
sum!(CombatStep { Begin,Attackers{},Blockers{},FirstDamage{},NormalDamage{},End{} });
sum!(BeginningStep { Untap,Upkeep{},Draw{} });
record!(CombatHeader {
    id: RecordId,
    step: CombatStep,
    first_damage_happened: bool
});
sum!(TurnInsertion {
    None,ExtraTurn{player:Player},ExtraCombat{player:Player,with_main:bool},
    Skip{player:Player,step:Step},ControlNextTurn{player:Player,controller:Player,source:ObjectId},
});
record!(ScheduledTurn {
    id: RecordId,
    after: RecordId,
    body: TurnInsertion
});
record!(TurnHeader {
    serial: TurnId,
    instance: RecordId,
    active: Player,
    next_normal: Player,
    decision_controller: Player,
});
record!(Priority {
    player: Player,
    consecutive_passes: u8
});
record!(Mulligan {
    starting: Player,
    taken: [u8; 2],
    kept_mask: u8,
    declared_mask: u8,
    mulligan_mask: u8,
    next: Player
});
record!(Opening {
    next: Player,
    revealed: Span
});
sum!(Outcome { Draw,Won{winner:Player},Conceded{loser:Player} });

// Choice has no generated legal-action list: that is a derived view. The accumulated
// selections, affected event and instruction pointer DO survive a branch/restore.
sum!(ChoiceItem {
    None,Card{card:Card,owner:Player,count:u8},Objects{ids:IdRange},
    Player{player:Player},Number{number:u32},Mode{mode:u16},Record{id:RecordId},
});
sum!(Choice {
    None,Select{player:Player,rule:Behavior,min:u32,max:u32,selected:Span},
    Number{player:Player,min:u32,max:u32},Order{player:Player,objects:Span,chosen:Span},
    YesNo{player:Player},Mana{player:Player,casting:RecordId},
    Chance{rule:Behavior,objects:Span},
});
sum!(Frame {
    None,Cast{stack:ObjectId,stage:u8,record:RecordId,selected:Span},
    Resolve{ability:Behavior,stage:u16,source:ObjectId,controller:Player,capture:RecordId,selected:Span},
    Replace{event:RecordId,applied:Span}, // AppliedReplacement items, per affected event
    Pay{stack:ObjectId,stage:u8,cost:Behavior,remaining:[u32;6]},
    Sba{stage:u8,simultaneous:RecordId},OrderTriggers{player:Player,remaining:Span},
    TurnAction{step:Step,stage:u8,selected:Span},
    MulliganAction{player:Player,stage:u8,bottom:Span},
    OpeningAction{player:Player,ability:Behavior,stage:u8},
});
sum!(Zone { Battlefield,Stack{},Hand{},Library{},Graveyard{},Exile{},Outside{},Command{} });
sum!(EventBody {
    None,Damage{sources:IdRange,target:Subject,amount_each:u32,combat:bool,capture:RecordId},
    Move{objects:IdRange,origin:Origin,from:Zone,to:Zone},
    Draw{player:Player,count:u32},Life{player:Player,delta:i32},
    Counter{subject:Subject,kind:Symbol,delta:i32},
    Enter{candidate:RecordId}, // Entering record; not yet a battlefield object
});
record!(PendingEvent {
    id: RecordId,
    simultaneous: RecordId,
    body: EventBody
});
record!(AppliedReplacement {
    event: RecordId,
    source: ObjectId,
    ability: Behavior,
    effect: EffectId
});
record!(Entering {
    id: RecordId,
    origin: Origin,
    controller: Player,
    face: u8,
    cast: RecordId,
    copy: RecordId,
    selected: Span
});
record!(DamageAssignment {
    sources: IdRange,
    target: Subject,
    amount_each: u32
});
// Tokens and spell/card copies can exist briefly in non-battlefield zones before SBA.
// They must not enter ordinary physical-card counts. Definitions/values still have a lifetime.
record!(TransientObject {
    ids: IdRange,
    origin: Origin,
    zone: OwnedZone,
    controller: Player,
    values: RecordId
});

// Macro only chooses capacities. It does not implement different rule semantics.
// All fields are accounted for by size_of and live_bytes; no uncounted payload arena.
macro_rules! profile {
    ($module:ident; kinds=$k:literal, library=$l:literal, battlefield=$b:literal,
     stack=$s:literal, effects=$e:literal, exile=$x:literal, tracked=$t:literal,
     rare=$r:literal, history=$h:literal, captures=$c:literal, items=$i:literal,
     frames=$f:literal, pending=$q:literal, combat=$a:literal, outside=$o:literal, turn_history=$ht:ty) => {
        pub mod $module {
            use super::*;
            #[derive(Clone,Copy,Debug,PartialEq,Eq, Hash)]
            pub struct Zones {
                pub library:[Pool<Card,$l>;2],
                pub hand:[[u8;$k];2],pub graveyard:[[u8;$k];2],
                pub ordinary_exile:[[u8;$k];2],pub outside:[[u8;$o];2],
                pub tracked:Pool<TrackedCard,$t>,
                pub exile_groups:Pool<ExileGroup,$x>,pub exiled:Pool<ExiledCards,$x>,
            }
            impl Default for Zones {fn default()->Self { Self {
                library:[Pool::default();2],hand:[[0;$k];2],graveyard:[[0;$k];2],
                ordinary_exile:[[0;$k];2],outside:[[0;$o];2],tracked:Pool::default(),
                exile_groups:Pool::default(),exiled:Pool::default(),
            }}}
            impl Live for Zones {fn live_bytes(&self)->usize {
                self.library.live_bytes()+self.hand.live_bytes()+self.graveyard.live_bytes()
                +self.ordinary_exile.live_bytes()+self.outside.live_bytes()+self.tracked.live_bytes()
                +self.exile_groups.live_bytes()+self.exiled.live_bytes()
            }}
            record!(Setup { identity:Identity,zones:Zones,companion:Pool<Companion,$h> });
            #[derive(Clone,Copy,Debug,Default,PartialEq,Eq, Hash)]
            pub struct World {
                pub identity:Identity,pub players:[PlayerCore;2],pub zones:Zones,
                pub battlefield:Pool<Permanent,$b>,pub stack:Pool<StackRun,$s>,
                pub targets:Pool<Target,$i>,pub cast_records:Pool<CastRecord,$s>,pub paid_costs:Pool<PaidCost,$i>,
                pub counters:Pool<Counter,$x>,pub effects:Pool<Effect,$e>,pub delayed:Pool<DelayedTrigger,$x>,
                pub attachments:Pool<Attachment,$r>,pub local:Pool<LocalState,$r>,pub links:Pool<Link,$r>,
                pub special_mana:Pool<SpecialMana,$r>,pub mana_spent:Pool<ManaSpent,$r>,pub emblems:Pool<Emblem,$r>,
                pub designations:Pool<PlayerDesignation,$h>,pub companion:Pool<Companion,$h>,
                pub captures:Pool<Capture,$c>,pub frozen:Pool<FrozenValues,$r>,
                pub object_items:Pool<IdRange,$i>,pub symbol_items:Pool<Symbol,$r>,
                pub ability_items:Pool<AbilityValue,$r>,pub frozen_counters:Pool<Counter,$r>,
                pub turn_history:$ht,
                pub object_history:Pool<ObjectHistory,$r>,pub usages:Pool<Usage,$r>,
                pub schedule:Pool<ScheduledTurn,$r>,
            }
            impl World {
                pub fn parts(&self)->[(&'static str,usize,usize);30] {
                    macro_rules! p {($name:ident)=>{(stringify!($name),size_of_val(&self.$name),self.$name.live_bytes())}}
                    [p!(identity),p!(players),p!(zones),p!(battlefield),p!(stack),p!(targets),p!(cast_records),
                     p!(paid_costs),p!(counters),p!(effects),p!(delayed),p!(attachments),p!(local),p!(links),
                     p!(special_mana),p!(mana_spent),p!(emblems),p!(designations),p!(companion),p!(captures),p!(frozen),
                     p!(object_items),p!(symbol_items),p!(ability_items),p!(frozen_counters),p!(turn_history),
                     p!(object_history),p!(usages),p!(schedule),
                     ("struct_padding",size_of::<Self>()-(size_of::<Identity>()+size_of::<[PlayerCore;2]>()
                      +size_of::<Zones>()+size_of::<Pool<Permanent,$b>>()+size_of::<Pool<StackRun,$s>>()
                      +size_of::<Pool<Target,$i>>()+size_of::<Pool<CastRecord,$s>>()+size_of::<Pool<PaidCost,$i>>()
                      +size_of::<Pool<Counter,$x>>()+size_of::<Pool<Effect,$e>>()+size_of::<Pool<DelayedTrigger,$x>>()
                      +size_of::<Pool<Attachment,$r>>()+size_of::<Pool<LocalState,$r>>()+size_of::<Pool<Link,$r>>()
                      +size_of::<Pool<SpecialMana,$r>>()+size_of::<Pool<ManaSpent,$r>>()+size_of::<Pool<Emblem,$r>>()+size_of::<Pool<PlayerDesignation,$h>>()
                      +size_of::<Pool<Companion,$h>>()+size_of::<Pool<Capture,$c>>()+size_of::<Pool<FrozenValues,$r>>()
                      +size_of::<Pool<IdRange,$i>>()+size_of::<Pool<Symbol,$r>>()+size_of::<Pool<AbilityValue,$r>>()
                      +size_of::<Pool<Counter,$r>>()+size_of::<$ht>()
                      +size_of::<Pool<ObjectHistory,$r>>()+size_of::<Pool<Usage,$r>>()+size_of::<Pool<ScheduledTurn,$r>>()),0)]
                }
            }
            impl Live for World {fn live_bytes(&self)->usize {self.parts().iter().map(|p|p.2).sum()}}
            #[derive(Clone,Copy,Debug,Default,PartialEq,Eq, Hash)]
            pub struct Combat {pub header:CombatHeader,pub attackers:Pool<Attacker,$a>,pub blocks:Pool<Block,$a>,pub first_strikers:Pool<IdRange,$a>}
            impl Live for Combat {fn live_bytes(&self)->usize {self.header.live_bytes()+self.attackers.live_bytes()+self.blocks.live_bytes()+self.first_strikers.live_bytes()}}
            sum!(Phase { NotStarted,Beginning{step:BeginningStep},Main{postcombat:bool},Combat{combat:Combat},Ending{cleanup:bool} });
            record!(Progress { turn:TurnHeader,phase:Phase });
            #[derive(Clone,Copy,Debug,Default,PartialEq,Eq, Hash)]
            pub struct Running {
                pub frames:Pool<Frame,$f>,pub choice:Choice,pub pending:Pool<PendingEvent,$q>,
                pub failed_draw_mask:u8,pub selections:Pool<ChoiceItem,$i>,
                pub triggers:Pool<PendingTrigger,$q>,pub applied:Pool<AppliedReplacement,$q>,
                pub entering:Pool<Entering,$q>,pub assignments:Pool<DamageAssignment,$q>,
                pub transient:Pool<TransientObject,$r>,
            }
            impl Live for Running {fn live_bytes(&self)->usize {
                self.frames.live_bytes()+self.choice.live_bytes()+self.pending.live_bytes()+self.failed_draw_mask.live_bytes()+self.selections.live_bytes()+self.triggers.live_bytes()
                +self.applied.live_bytes()+self.entering.live_bytes()+self.assignments.live_bytes()
                +self.transient.live_bytes()
            }}
            sum!(Execution { Idle,Priority{priority:Priority},Running{running:Running} });
            // Full owner type; inline enum reserves the largest phase/execution.
            // Search snapshots use selected variants and live prefixes, never raw enum bytes.
            sum!(State {
                Uninitialized,
                Mulligan{setup:Setup,process:Mulligan,execution:Execution},
                Opening{world:World,process:Opening,execution:Execution},
                Playing{world:World,progress:Progress,execution:Execution},
                Finished{outcome:Outcome},
            });
            pub const CAPACITIES:&[(&str,usize)]=&[
                ("kinds",$k),("library/player",$l),("battlefield groups",$b),("stack runs",$s),
                ("effects",$e),("exile/counters/delayed each",$x),("tracked cards",$t),
                ("rare pools each",$r),("player/history pools each",$h),("captures",$c),
                ("item pools each",$i),("frames",$f),("pending pools each",$q),
                ("combat pools each",$a),("outside kinds/player",$o),
            ];
        }
    };
}

// Capacity budgets, not claimed empirical maxima. Overflow is an explicit engine error.
profile!(burn; kinds=16,library=60,battlefield=48,stack=16,effects=32,exile=8,tracked=8,
         rare=0,history=0,captures=16,items=48,frames=8,pending=32,combat=24,outside=0,turn_history=BurnHistory);
profile!(modern; kinds=96,library=96,battlefield=128,stack=64,effects=128,exile=64,tracked=64,
         rare=64,history=2,captures=128,items=256,frames=24,pending=128,combat=96,outside=96,turn_history=ModernHistory);

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_preserves_identity_and_does_not_renumber() {
        let range = IdRange {
            first: ObjectId(100),
            count: 100,
        };
        let (l, x, r) = range.split(ObjectId(150)).unwrap();
        assert_eq!(
            l.unwrap(),
            IdRange {
                first: ObjectId(100),
                count: 50
            }
        );
        assert_eq!(
            x,
            IdRange {
                first: ObjectId(150),
                count: 1
            }
        );
        assert_eq!(
            r.unwrap(),
            IdRange {
                first: ObjectId(151),
                count: 49
            }
        );
        assert!(range.split(ObjectId(200)).is_none());
        assert!(range.split(ObjectId(100)).unwrap().0.is_none());
        assert!(range.split(ObjectId(199)).unwrap().2.is_none());
    }
    #[test]
    fn ids_never_wrap_or_reuse() {
        let mut ids = Identity {
            next: u32::MAX - 1,
            next_stamp: 1,
        };
        assert_eq!(ids.allocate_range(1).unwrap().first, ObjectId(u32::MAX - 1));
        assert_eq!(ids.allocate_range(1), Err(CapacityError::IdExhausted));
        assert_eq!(ids.next, u32::MAX);
    }
    #[test]
    fn full_pool_is_not_silent_truncation() {
        let mut p = Pool::<Card, 1>::default();
        p.push(Card(3)).unwrap();
        assert_eq!(p.push(Card(4)), Err(CapacityError::Full));
        assert_eq!(p.data[0], Card(3));
        assert_eq!(p.len, 1);
    }
    #[test]
    fn snapshot_size_ignores_unused_capacity_and_inactive_variants() {
        let mut p = Pool::<Card, 60>::default();
        p.push(Card(5)).unwrap();
        p.data[59] = Card(99);
        assert_eq!(p.live_bytes(), 4);
        assert_eq!(
            burn::State::Finished {
                outcome: Outcome::Won { winner: Player(0) }
            }
            .live_bytes(),
            3
        );
        assert!(size_of::<burn::State>() > 3000);
    }
    #[test]
    fn world_breakdown_accounts_for_all_reserved_bytes() {
        assert_eq!(
            burn::World::default()
                .parts()
                .iter()
                .map(|p| p.1)
                .sum::<usize>(),
            size_of::<burn::World>()
        );
        assert_eq!(
            modern::World::default()
                .parts()
                .iter()
                .map(|p| p.1)
                .sum::<usize>(),
            size_of::<modern::World>()
        );
    }
}
