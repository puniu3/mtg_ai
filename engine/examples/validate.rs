mod support;
use burn_engine::*;
use mtg_state_design::burn::{Execution, State};
use support::*;
#[global_allocator]
static ALLOC: Allocator = Allocator;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let n: usize = args.get(1).and_then(|x| x.parse().ok()).unwrap_or(10000);
    let audit = !args.iter().any(|a| a == "--no-audit");
    let start = std::time::Instant::now();
    let mut steps = 0u64;
    let mut high = [0u16; 8];
    let mut wins = [0u32; 3];
    counting(true);
    for seed in 1..=n {
        let mut rng = verification::Rng(seed as u64 * 0x9e3779b9);
        let ds = [verification::deck(&mut rng), verification::deck(&mut rng)];
        let mut e = Engine::new(ds, Player((seed % 2) as u8)).unwrap();
        // Some games mulligan, independently of the example keep-only policy.
        let wanted = [(seed % 4) as u8, ((seed / 4) % 4) as u8];
        let mut taken = [0; 2];
        let mut turns = 0;
        for step in 0..20000 {
            if let Some(out) = e.finished() {
                wins[match out {
                    Outcome::Won { winner } => winner.0 as usize,
                    _ => 2,
                }] += 1;
                break;
            }
            assert!(step < 19999, "decision limit seed {seed}");
            if audit {
                assert_deck(&e);
            }
            if let State::Playing {
                world, progress, ..
            } = &e.state
            {
                turns = turns.max(progress.turn.serial.0);
                let v = [
                    world.battlefield.len,
                    world.stack.len,
                    world.effects.len,
                    world.targets.len,
                    world.captures.len,
                    world.zones.tracked.len,
                    world.counters.len,
                    if let Execution::Running { running } = e.execution() {
                        running.triggers.len
                    } else {
                        0
                    },
                ];
                for i in 0..high.len() {
                    high[i] = high[i].max(v[i]);
                }
            }
            let result = if matches!(
                e.state,
                State::Mulligan {
                    execution: Execution::Idle,
                    ..
                }
            ) {
                let p = e.actor().unwrap();
                let i = p.0 as usize;
                let take = taken[i] < wanted[i];
                taken[i] += u8::from(take);
                e.apply(p, Action::Mulligan { take })
            } else {
                verification::drive_step(&mut e, &mut rng)
            };
            assert!(
                result.is_ok(),
                "seed {seed} step {step} turn {turns}: {result:?}"
            );
            steps += 1;
        }
    }
    let allocations = counting(false);
    let elapsed = start.elapsed().as_secs_f64();
    assert_eq!(allocations, 0);
    println!(
        "{}",
        serde_json::json!({"games":n,"decisions":steps,"seconds":elapsed,"games_per_second":n as f64/elapsed,"decisions_per_second":steps as f64/elapsed,"allocations":allocations,"state_bytes":size_of::<Engine>(),"inventory_audit":audit,"outcomes":wins,"high_water":{"battlefield":high[0],"stack":high[1],"effects":high[2],"targets":high[3],"captures":high[4],"tracked":high[5],"counters":high[6],"pending_triggers":high[7]},"policy":"weak randomized; mulligan 0..3; seed 1..N"})
    );
}
