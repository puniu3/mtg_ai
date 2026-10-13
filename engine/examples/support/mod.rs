use burn_engine::*;
use mtg_state_design::burn::{Execution, State};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
thread_local! {static COUNT:Cell<Option<usize>>=const{Cell::new(None)};}
pub struct Allocator;
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        COUNT.with(|c| {
            if let Some(n) = c.get() {
                c.set(Some(n + 1))
            }
        });
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        COUNT.with(|c| {
            if let Some(n) = c.get() {
                c.set(Some(n + 1))
            }
        });
        unsafe { System.realloc(p, l, n) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        COUNT.with(|c| {
            if let Some(n) = c.get() {
                c.set(Some(n + 1))
            }
        });
        unsafe { System.alloc_zeroed(l) }
    }
}
pub fn counting(on: bool) -> usize {
    COUNT.with(|c| {
        let n = c.get().unwrap_or(0);
        c.set(on.then_some(0));
        n
    })
}
fn live<T: Copy, const N: usize>(p: &Pool<T, N>) -> &[T] {
    &p.data[..p.len as usize]
}
pub fn inventory(e: &Engine) -> Option<[[u32; 16]; 2]> {
    let (z, w) = match &e.state {
        State::Mulligan { setup, .. } => (&setup.zones, None),
        State::Playing { world, .. } => (&world.zones, Some(world)),
        State::Finished { .. } => return None,
        _ => panic!("unexpected lifecycle"),
    };
    let mut out = [[0; 16]; 2];
    for p in 0..2 {
        for c in 0..16 {
            out[p][c] =
                z.hand[p][c] as u32 + z.graveyard[p][c] as u32 + z.ordinary_exile[p][c] as u32;
        }
        for c in live(&z.library[p]) {
            out[p][c.0 as usize] += 1;
        }
    }
    for t in live(&z.tracked) {
        if !matches!(t.zone, OwnedZone::Library { .. }) {
            out[t.owner.0 as usize][t.card.0 as usize] += 1;
        }
    }
    let mut add = |o: Origin, n: u32| {
        if let Origin::Card { card, owner } = o {
            out[owner.0 as usize][card.0 as usize] += n;
        }
    };
    if let Some(w) = w {
        for b in live(&w.battlefield) {
            add(b.origin, b.ids.count);
        }
        for s in live(&w.stack) {
            if let StackBody::Spell { origin, .. } = s.body {
                add(origin, s.ids.count);
            }
        }
    }
    if let Execution::Running { running } = e.execution() {
        for x in live(&running.entering) {
            add(x.origin, 1);
        }
    }
    Some(out)
}
pub fn assert_deck(e: &Engine) {
    if let Some(inventory) = inventory(e) {
        assert_eq!(inventory, [DECK_COUNTS.map(u32::from); 2]);
    }
}
