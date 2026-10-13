use mtg_state_design::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};

struct CountAlloc;
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for CountAlloc {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(p, l, n) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc_zeroed(l) }
    }
}
#[global_allocator]
static ALLOCATOR: CountAlloc = CountAlloc;

// Synthetic layout fixture, not a recorded legal game. Card indices use fixture.txt order.
fn burn_world() -> burn::World {
    let mut w = burn::World::default();
    w.identity = Identity {
        next: 1000,
        next_stamp: 1000,
    };
    w.players = [
        PlayerCore {
            life: 12,
            mana: [0; 6],
        },
        PlayerCore {
            life: 10,
            mana: [0; 6],
        },
    ];
    // 43 library + 3 hand + 8 graveyard + 6 battlefield = 60 per player.
    for p in 0..2 {
        let mut remaining = [4u8, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 3, 4, 4, 2, 3];
        for c in [3, 7, 9] {
            w.zones.hand[p][c] += 1;
            remaining[c] -= 1;
        }
        for c in [0, 3, 4, 4, 5, 6, 8, 10] {
            w.zones.graveyard[p][c] += 1;
            remaining[c] -= 1;
        }
        for (n, c) in [1usize, 2, 13, 14, 15, 15].into_iter().enumerate() {
            remaining[c] -= 1;
            w.battlefield
                .push(Permanent {
                    ids: IdRange {
                        first: ObjectId(1 + (p * 6 + n) as u32),
                        count: 1,
                    },
                    origin: Origin::Card {
                        card: Card(c as u16),
                        owner: Player(p as u8),
                    },
                    timestamp: Stamp(1 + (p * 6 + n) as u32),
                    base_controller: Player(p as u8),
                    ..Default::default()
                })
                .unwrap();
        }
        for (c, n) in remaining.into_iter().enumerate() {
            for _ in 0..n {
                w.zones.library[p].push(Card(c as u16)).unwrap();
            }
        }
        assert_eq!(w.zones.library[p].len, 43);
    }
    w.turn_history = BurnHistory {
        land_plays: [1, 0],
        land_entered_mask: 1,
        life_lost_mask: 2,
    };
    w
}
fn progress() -> burn::Progress {
    burn::Progress {
        turn: TurnHeader {
            serial: TurnId(7),
            instance: RecordId(20),
            active: Player(0),
            next_normal: Player(1),
            decision_controller: Player(0),
        },
        phase: burn::Phase::Main { postcombat: false },
    }
}
fn priority() -> burn::Execution {
    burn::Execution::Priority {
        priority: Priority {
            player: Player(0),
            consecutive_passes: 0,
        },
    }
}

fn main() {
    let w = burn_world();
    let main = burn::State::Playing {
        world: w,
        progress: progress(),
        execution: priority(),
    };
    let mut stack_world = w;
    stack_world.zones.hand[0][3] -= 1;
    stack_world
        .stack
        .push(StackRun {
            ids: IdRange {
                first: ObjectId(100),
                count: 1,
            },
            controller: Player(0),
            body: StackBody::Spell {
                origin: Origin::Card {
                    card: Card(3),
                    owner: Player(0),
                },
                face: 0,
                cast: RecordId(101),
            },
            targets: Span { start: 0, len: 1 },
        })
        .unwrap();
    stack_world
        .targets
        .push(Target {
            subject: Subject::Player { player: Player(1) },
            role: 0,
            division: 0,
        })
        .unwrap();
    stack_world
        .cast_records
        .push(CastRecord {
            id: RecordId(101),
            ..Default::default()
        })
        .unwrap();
    let bolt = burn::State::Playing {
        world: stack_world,
        progress: progress(),
        execution: priority(),
    };
    let mut running = burn::Running::default();
    running
        .frames
        .push(Frame::Cast {
            stack: ObjectId(100),
            stage: 1,
            record: RecordId(101),
            selected: Span::default(),
        })
        .unwrap();
    running
        .frames
        .push(Frame::Pay {
            stack: ObjectId(100),
            stage: 0,
            cost: Behavior(1),
            remaining: [0, 0, 0, 1, 0, 0],
        })
        .unwrap();
    running.choice = Choice::Mana {
        player: Player(0),
        casting: RecordId(101),
    };
    let casting = burn::State::Playing {
        world: stack_world,
        progress: progress(),
        execution: burn::Execution::Running { running },
    };
    let mut combat = burn::Combat::default();
    combat.header = CombatHeader {
        id: RecordId(200),
        step: CombatStep::Blockers {},
        first_damage_happened: false,
    };
    combat
        .attackers
        .push(Attacker {
            attackers: IdRange {
                first: ObjectId(2),
                count: 1,
            },
            defender: Subject::Player { player: Player(1) },
            ..Default::default()
        })
        .unwrap();
    combat
        .blocks
        .push(Block {
            blocker: ObjectId(7),
            attacker: ObjectId(2),
        })
        .unwrap();
    let fight = burn::State::Playing {
        world: w,
        progress: burn::Progress {
            phase: burn::Phase::Combat { combat },
            ..progress()
        },
        execution: priority(),
    };

    let mut setup = burn::Setup::default();
    setup.identity = Identity {
        next: 1,
        next_stamp: 1,
    };
    for p in 0..2 {
        let counts = [4u8, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 3, 4, 4, 2, 3];
        let mut drawn = 0;
        for (c, n) in counts.into_iter().enumerate() {
            for _ in 0..n {
                if drawn < 7 {
                    setup.zones.hand[p][c] += 1;
                    drawn += 1;
                } else {
                    setup.zones.library[p].push(Card(c as u16)).unwrap();
                }
            }
        }
    }
    let mulligan = burn::State::Mulligan {
        setup,
        process: Mulligan::default(),
        execution: burn::Execution::Idle,
    };

    // Check only modeled storage operations, not game transitions or snapshot codec.
    let before = ALLOCATIONS.load(Ordering::Relaxed);
    for _ in 0..1000 {
        let mut copy = black_box(main);
        if let burn::State::Playing { world, .. } = &mut copy {
            world.players[0].life -= 1;
            world
                .battlefield
                .push(Permanent {
                    ids: IdRange {
                        first: ObjectId(4000),
                        count: 1000000,
                    },
                    ..Default::default()
                })
                .unwrap();
            black_box(
                world.battlefield.data[12]
                    .ids
                    .split(ObjectId(5000))
                    .unwrap(),
            );
        }
        black_box(copy.live_bytes());
    }
    let allocations = ALLOCATIONS.load(Ordering::Relaxed) - before;
    assert_eq!(allocations, 0);

    println!(
        "{{\"schema_version\":1,\"scope\":\"layout design, not a rules engine\",\"profiles\":{{"
    );
    macro_rules! profile_report {($m:ident)=>{{
        let empty=$m::World::default();
        println!("\"{}\":{{\"state_reserved\":{},\"world_reserved\":{},\"zones_reserved\":{},\"running_reserved\":{},\"combat_reserved\":{},\"parts\":[",stringify!($m),size_of::<$m::State>(),size_of::<$m::World>(),size_of::<$m::Zones>(),size_of::<$m::Running>(),size_of::<$m::Combat>());
        for (i,(name,bytes,live)) in empty.parts().iter().enumerate() {
            if i>0 {print!(",")};println!("{{\"field\":\"{}\",\"reserved\":{},\"empty_live_model\":{}}}",name,bytes,live);
        }
        print!("],\"capacities\":{{");
        for (i,(name,value)) in $m::CAPACITIES.iter().enumerate() {if i>0 {print!(",")};print!("\"{}\":{}",name,value);}
        print!("}}}}");
    }}}
    profile_report!(burn);
    println!(",");
    profile_report!(modern);
    println!(
        "}},\"examples_live_model\":{{\"burn_mulligan\":{},\"burn_main_12_permanents\":{},\"burn_bolt_on_stack\":{},\"burn_casting_choice\":{},\"burn_combat\":{},\"finished\":{}}},",
        mulligan.live_bytes(),
        main.live_bytes(),
        bolt.live_bytes(),
        casting.live_bytes(),
        fight.live_bytes(),
        burn::State::Finished {
            outcome: Outcome::Won { winner: Player(0) }
        }
        .live_bytes()
    );
    print!("\"record_sizes\":{{");
    macro_rules! sizes {($first:ty,$($t:ty),*)=>{{
        print!("\"{}\":{}",stringify!($first),size_of::<$first>());
        $(print!(",\"{}\":{}",stringify!($t),size_of::<$t>());)*
    }}}
    sizes!(
        Card,
        PlayerCore,
        Permanent,
        StackRun,
        ExileGroup,
        ExiledCards,
        TrackedCard,
        Target,
        CastRecord,
        ManaSpent,
        Effect,
        DelayedTrigger,
        Capture,
        FrozenValues,
        Counter,
        SpecialMana,
        Frame,
        PendingEvent,
        Entering,
        ModernHistory,
        BurnHistory,
        Attacker,
        Block
    );
    println!(
        "}},\"storage_operations\":{{\"iterations\":1000,\"allocations\":{}}}}}",
        allocations
    );
}
