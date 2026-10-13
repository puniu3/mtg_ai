//! Agent-facing facts derived only from the redacted observation. No sampling,
//! recommendations, or estimates of the opponent's actual hidden allocation.
use super::*;
use serde_json::{Value, json};

fn card(c: Card) -> Value {
    json!({"card_id":c.0,"card":NAMES[c.0 as usize]})
}
fn counts(values: &[u32; 16]) -> Vec<Value> {
    values
        .iter()
        .enumerate()
        .filter(|(_, n)| **n > 0)
        .map(|(i, n)| {
            let mut v = card(Card(i as u16));
            v["count"] = json!(n);
            v
        })
        .collect()
}
fn mana(values: [u32; 6]) -> Value {
    json!({"W":values[0],"U":values[1],"B":values[2],"R":values[3],"G":values[4],"C":values[5]})
}
fn ability(id: u16) -> &'static str {
    match id {
        PROWESS => "prowess",
        EIDOLON_TRIGGER => "Eidolon of the Great Revel: 2 damage to caster",
        GUIDE_TRIGGER => "Goblin Guide: reveal defending player's top card",
        FETCH => "fetch land, then shuffle",
        DRAW => "Sunbaked Canyon: draw a card",
        TIME => "suspend: remove a time counter",
        SUSPEND_CAST => "suspend: cast Rift Bolt",
        INDESTRUCTIBLE => "indestructible",
        DOUBLE => "double strike",
        ATTACK_CHOICE => "declare attackers",
        BLOCK_CHOICE => "declare blockers",
        DAMAGE_CHOICE => "assign combat damage",
        FETCH_CHOICE => "choose a land to fetch",
        DISCARD_CHOICE => "discard to hand size",
        SUSPEND_CHOICE => "choose Rift Bolt target or decline",
        MULLIGAN_BOTTOM => "put a card on the bottom",
        SHUFFLE_CHOICE => "shuffle library",
        _ => "unknown ability",
    }
}
fn charm_mode(mode: u32) -> &'static str {
    match mode {
        0 => "4 damage to player",
        1 => "your permanents gain indestructible this turn",
        2 => "target creature gains double strike this turn",
        _ => "unknown mode",
    }
}

impl PublicEvent {
    pub fn agent_view(self) -> Value {
        match self {
            Self::RevealTop { player, card: c } => json!({
                "kind":"reveal_top", "player":player.0, "card_id":c.0,
                "card":NAMES[c.0 as usize],
                "destination":if land(c) { "hand" } else { "library_top" }
            }),
        }
    }
}

impl Observation {
    fn object_view(&self, id: ObjectId) -> Value {
        let mut v = json!({"object_id":id.0});
        if matches!(
            self.template.state,
            State::Playing { .. } | State::Opening { .. }
        ) {
            if let Some(p) = self.template.permanent(id) {
                v["card"] = json!(NAMES[card_of(p).0 as usize]);
                v["controller"] = json!(p.base_controller.0);
            } else if let Some(c) = entries(&self.template.world().zones.tracked)
                .iter()
                .find(|c| c.id == id)
            {
                // This engine tracks only public suspended cards.
                if matches!(c.zone, OwnedZone::Exile { .. }) {
                    v["card"] = json!(NAMES[c.card.0 as usize]);
                    v["owner"] = json!(c.owner.0);
                }
            }
        }
        v
    }
    fn subject_view(&self, subject: Subject) -> Value {
        match subject {
            Subject::Absent => Value::Null,
            Subject::Player { player } => json!({"player":player.0}),
            Subject::Object { id } => self.object_view(id),
        }
    }
    fn source_view(&self, source: AbilitySource) -> Value {
        match source {
            AbilitySource::Fixed { source } => self.object_view(source),
            AbilitySource::Parallel { first } => self.object_view(first),
            AbilitySource::None => Value::Null,
        }
    }
    /// Use only for the viewer's legal actions or already-public actions.
    /// Private bottom selections must be redacted by the transcript adapter.
    pub fn action_view(&self, a: Action) -> Value {
        match a {
            Action::LandPlan { card: c, pay } => {
                json!({"kind":"play_shockland","card":NAMES[c.0 as usize],"pay_2_life":pay,"enters_tapped":!pay})
            }
            Action::FetchPlan { object, card, pay } => {
                json!({"kind":"fetch_plan","source":self.object_view(object),"card":card.map(|c|NAMES[c.0 as usize]),"pay_2_life":pay})
            }
            Action::SearchPlan { card, pay } => {
                json!({"kind":"search_plan","card":card.map(|c|NAMES[c.0 as usize]),"pay_2_life":pay})
            }
            Action::Pass => json!({"kind":"pass"}),
            Action::Cast {
                card: c,
                targets,
                mode,
                spectacle,
            } => {
                let mut v = card(c);
                v["kind"] = json!("cast");
                v["targets"] = json!(
                    targets
                        .iter()
                        .filter(|t| **t != Subject::Absent)
                        .map(|t| self.subject_view(*t))
                        .collect::<Vec<_>>()
                );
                if c == CHARM {
                    v["mode"] = json!(charm_mode(mode as u32));
                }
                if c == SKEWER {
                    v["spectacle"] = json!(spectacle);
                }
                v
            }
            Action::Land { card: c } | Action::Bottom { card: c } | Action::Discard { card: c } => {
                let mut v = card(c);
                v["kind"] = json!(match a {
                    Action::Land { .. } => "play_land",
                    Action::Bottom { .. } => "bottom",
                    _ => "discard",
                });
                v
            }
            Action::Tap { object, color } => {
                json!({"kind":"tap_for_mana","source":self.object_view(object),"color":(["W","U","B","R","G","C"][color as usize])})
            }
            Action::Pay { mana: paid } => json!({"kind":"pay_mana","mana":mana(paid)}),
            Action::Fetch { object } | Action::Canyon { object } => {
                json!({"kind":if matches!(a,Action::Fetch{..}) {"fetch"} else {"sacrifice_canyon_to_draw"},"source":self.object_view(object)})
            }
            Action::Suspend => json!({"kind":"suspend","card":"Rift Bolt"}),
            Action::Shock { pay } => {
                json!({"kind":"shockland_entry","pay_2_life":pay,"enters_tapped":!pay})
            }
            Action::Search { card: c } => json!({"kind":"fetch_selection","selection":c.map(card)}),
            Action::Shuffle { .. } => json!({"kind":"shuffle"}),
            Action::OrderTrigger { trigger } => {
                json!({"kind":"order_trigger","trigger_id":trigger.0})
            }
            Action::Attack { objects, len } => {
                json!({"kind":"attack","attackers":objects[..len as usize].iter().map(|id| self.object_view(*id)).collect::<Vec<_>>()})
            }
            Action::Block { pairs, len } => {
                json!({"kind":"block","blocks":pairs[..len as usize].iter().map(|(b,a)| json!({"blocker":self.object_view(*b),"attacker":self.object_view(*a)})).collect::<Vec<_>>()})
            }
            Action::Damage { assignments, len } => {
                json!({"kind":"assign_damage","assignments":assignments[..len as usize].iter().map(|(s,t,n)| json!({"source":self.object_view(*s),"target":self.subject_view(*t),"damage":n})).collect::<Vec<_>>()})
            }
            Action::SuspendTarget { target } => {
                json!({"kind":"suspend_target","target":self.subject_view(target)})
            }
            Action::DeclineSuspend => json!({"kind":"decline_suspend"}),
            Action::Mulligan { take } => json!({"kind":if take {"mulligan"} else {"keep"}}),
        }
    }
    fn player_cards(&self, owner: usize) -> Value {
        let z = zones(&self.template).unwrap();
        let present = inventory(&self.template).unwrap();
        let known_hand = z.hand[owner].map(u32::from);
        let mut known_library = [0u32; 16];
        let positions = self.memory.library[owner]
            .iter()
            .rev()
            .enumerate()
            .filter_map(|(i, c)| {
                c.map(|c| {
                    known_library[c.0 as usize] += 1;
                    let mut v = card(c);
                    v["position_from_top"] = json!(i + 1);
                    v
                })
            })
            .collect::<Vec<_>>();
        let unknown_hand = self.sizes[owner] as u32 - known_hand.iter().sum::<u32>();
        let unknown_library = z.library[owner].len as u32 - known_library.iter().sum::<u32>();
        let mut hand_min = known_hand;
        let mut hand_and_library = [0u32; 16];
        let mut library_exact = true;
        let mut library_counts = Vec::new();
        for c in 0..16 {
            // Library card values have been replaced by Card(16). Inventory
            // includes known hands, public zones, stack and pending entries.
            let pool = self.totals[owner][c] - present[owner][c] - known_library[c];
            hand_and_library[c] = pool + known_hand[c] + known_library[c];
            hand_min[c] += pool.saturating_sub(unknown_library);
            let min = known_library[c] + pool.saturating_sub(unknown_hand);
            let max = known_library[c] + pool.min(unknown_library);
            library_exact &= min == max;
            if self.totals[owner][c] > 0 {
                let mut v = card(Card(c as u16));
                if min == max {
                    v["count"] = json!(min);
                } else {
                    v["min"] = json!(min);
                    v["max"] = json!(max);
                }
                library_counts.push(v);
            }
        }
        json!({
            "player":owner,
            "decklist":counts(&self.totals[owner]),
            "hand":{"size":self.sizes[owner],"known":counts(&hand_min),"unknown_count":self.sizes[owner] as u32 - hand_min.iter().sum::<u32>()},
            "library":{"size":z.library[owner].len,"composition_is_exact":library_exact,"remaining":library_counts,"known_positions":positions},
            "hand_and_library_remaining":counts(&hand_and_library),
            "graveyard":counts(&z.graveyard[owner].map(u32::from)),
            "ordinary_exile":counts(&z.ordinary_exile[owner].map(u32::from))
        })
    }
    fn execution_view(&self) -> Value {
        match self.template.execution() {
            Execution::Idle => json!({"kind":"idle"}),
            Execution::Priority { priority } => {
                json!({"kind":"priority","player":priority.player.0,"consecutive_passes":priority.consecutive_passes})
            }
            Execution::Running { running } => {
                let choice = match running.choice {
                    Choice::None => json!({"kind":"none"}),
                    Choice::Select {
                        player,
                        rule,
                        min,
                        max,
                        ..
                    } => {
                        json!({"kind":"select","player":player.0,"purpose":ability(rule.0),"min":min,"max":max})
                    }
                    Choice::Number { player, min, max } => {
                        json!({"kind":"number","player":player.0,"min":min,"max":max})
                    }
                    Choice::Order { player, .. } => {
                        json!({"kind":"order_triggers","player":player.0})
                    }
                    Choice::YesNo { player } => json!({"kind":"yes_no","player":player.0}),
                    Choice::Mana { player, .. } => json!({"kind":"pay_mana","player":player.0}),
                    Choice::Chance { rule, .. } => {
                        json!({"kind":"chance","purpose":ability(rule.0)})
                    }
                };
                json!({"kind":"choice","choice":choice,
                    "pending_triggers":entries(&running.triggers).iter().map(|t|json!({"id":t.ids.first.0,"controller":t.controller.0,"ability":ability(t.ability.0),"source":self.source_view(t.source)})).collect::<Vec<_>>(),
                    "entering":entries(&running.entering).iter().filter_map(|x| match x.origin { Origin::Card { card:c, owner } => Some(json!({"card":NAMES[c.0 as usize],"owner":owner.0,"controller":x.controller.0})), _ => None }).collect::<Vec<_>>()})
            }
        }
    }
    fn board_view(&self) -> Value {
        let e = &self.template;
        let w = e.world();
        let battlefield = entries(&w.battlefield).iter().map(|p| {
            let c = card_of(p);
            let mut v = json!({"object_id":p.ids.first.0,"count":p.ids.count,"card_id":c.0,"card":NAMES[c.0 as usize],"owner":owner_of(p).0,"controller":p.base_controller.0,"tapped":p.status&TAPPED!=0});
            if let Some((power,toughness)) = e.power_toughness(p.ids.first) {
                v["power"] = json!(power); v["toughness"] = json!(toughness);
                v["damage_marked"] = json!(p.damage);
                v["summoning_sick"] = json!(p.status & CONTROLLED == 0 && c != GUIDE && c != SWIFT);
                v["double_strike"] = json!(e.has_ability(p.ids.first, DOUBLE));
            }
            v["indestructible"] = json!(e.has_ability(p.ids.first, INDESTRUCTIBLE));
            v
        }).collect::<Vec<_>>();
        let stack = entries(&w.stack).iter().rev().map(|s| {
            let mut v = json!({"object_id":s.ids.first.0,"controller":s.controller.0,
                "targets":entries(&w.targets)[s.targets.start as usize..(s.targets.start+s.targets.len) as usize].iter().map(|t|self.subject_view(t.subject)).collect::<Vec<_>>()});
            match s.body {
                StackBody::Spell { origin: Origin::Card { card:c, .. }, cast, .. } => {
                    v["kind"] = json!("spell"); v["card"] = json!(NAMES[c.0 as usize]);
                    if let Some(record) = entries(&w.cast_records).iter().find(|r|r.id==cast) {
                        if c == CHARM { v["mode"] = json!(charm_mode(record.modes)); }
                        if c == SKEWER { v["spectacle"] = json!(matches!(record.route,CastRoute::Alternative{..})); }
                    }
                }
                StackBody::Ability { definition, source, capture } => {
                    v["kind"] = json!("ability"); v["ability"] = json!(ability(definition.0));
                    v["source"] = self.source_view(source);
                    if matches!(definition.0, GUIDE_TRIGGER | EIDOLON_TRIGGER) { v["affected_player"] = json!(e.capture_value(capture)); }
                }
                _ => { v["kind"] = json!("other"); }
            }
            v
        }).collect::<Vec<_>>();
        let suspended = entries(&w.zones.tracked).iter().filter(|c|matches!(c.zone,OwnedZone::Exile{..})).map(|c| {
            let time = entries(&w.counters).iter().filter(|x|x.subject==Subject::Object{id:c.id} && x.kind==Symbol(1)).map(|x|x.amount).sum::<u32>();
            json!({"object_id":c.id.0,"card":NAMES[c.card.0 as usize],"owner":c.owner.0,"time_counters":time,"counter_removal":"owner's upkeep"})
        }).collect::<Vec<_>>();
        json!({"life":w.players.map(|p|p.life),"mana":w.players.map(|p|mana(p.mana)),"battlefield":battlefield,
            "stack_order":"next_to_resolve_first","stack":stack,"suspended":suspended,
            "this_turn":{"land_plays":w.turn_history.land_plays,"land_entered":[w.turn_history.land_entered_mask&1!=0,w.turn_history.land_entered_mask&2!=0],"life_lost":[w.turn_history.life_lost_mask&1!=0,w.turn_history.life_lost_mask&2!=0]},
            "execution":self.execution_view()})
    }
    /// Stable, compact agent view. The legacy diagnostic summary stays separate.
    pub fn agent_view(&self) -> Value {
        let mut out = json!({"schema":"burn-agent-view-v1","viewer":self.viewer.0,"actor":self.template.actor().map(|p|p.0),
            "card_count_basis":"known fixed decklists; exact counts or bounds, never sampled hidden cards",
            "players":[self.player_cards(0),self.player_cards(1)]});
        match &self.template.state {
            State::Playing { progress, .. } => {
                out["turn"] = json!(progress.turn.serial.0);
                out["active_player"] = json!(progress.turn.active.0);
                out["phase"] = json!(match progress.phase {
                    Phase::NotStarted => "not_started",
                    Phase::Beginning {
                        step: BeginningStep::Untap,
                    } => "untap",
                    Phase::Beginning {
                        step: BeginningStep::Upkeep {},
                    } => "upkeep",
                    Phase::Beginning {
                        step: BeginningStep::Draw {},
                    } => "draw",
                    Phase::Main { postcombat: false } => "precombat_main",
                    Phase::Main { postcombat: true } => "postcombat_main",
                    Phase::Combat { combat } => match combat.header.step {
                        CombatStep::Begin => "begin_combat",
                        CombatStep::Attackers {} => "declare_attackers",
                        CombatStep::Blockers {} => "declare_blockers",
                        CombatStep::FirstDamage {} => "first_strike_damage",
                        CombatStep::NormalDamage {} => "combat_damage",
                        CombatStep::End {} => "end_combat",
                    },
                    Phase::Ending { cleanup: false } => "end_step",
                    Phase::Ending { cleanup: true } => "cleanup",
                });
                out["public"] = self.board_view();
                if let Phase::Combat { combat } = progress.phase {
                    out["public"]["combat"] = json!({
                        "attackers":entries(&combat.attackers).iter().map(|a| json!({"attacker":self.object_view(a.attackers.first),"defender":self.subject_view(a.defender),"was_blocked":a.was_blocked})).collect::<Vec<_>>(),
                        "blocks":entries(&combat.blocks).iter().map(|b|json!({"blocker":self.object_view(b.blocker),"attacker":self.object_view(b.attacker)})).collect::<Vec<_>>()});
                }
            }
            State::Mulligan { process, .. } => {
                out["phase"] = json!("mulligan");
                out["mulligans_taken"] = json!(process.taken);
                out["execution"] = self.execution_view();
            }
            State::Opening { .. } => {
                out["phase"] = json!("opening");
                out["public"] = self.board_view();
            }
            _ => unreachable!("observation requires a live position"),
        }
        out
    }
}

#[cfg(test)]
#[path = "../../tests/support/agent_view.rs"]
mod tests;
