//! Adopted generation-1 144→16→1 policy: terminal rollouts and PUCT priors.
//! Scores never replace terminal outcomes or truncate a rollout.
use super::features::{ACTION_DIM, STACK_STATE_DIM, action_features, stack_state_features};
use super::no_suicide::SearchChoices;
use super::*;
use std::sync::OnceLock;

const HIDDEN: usize = 16;
const INPUT: usize = STACK_STATE_DIM + ACTION_DIM;

struct Net {
    // Contiguous hidden lanes let the compiler vectorize independent neurons.
    // Each lane still sums inputs in their original order (no reassociation).
    w1: Vec<[f32; HIDDEN]>,
    b1: Vec<f32>,
    w2: Vec<f32>,
    b2: f32,
}
fn model() -> &'static Net {
    static MODEL: OnceLock<Net> = OnceLock::new();
    MODEL.get_or_init(|| {
        let file: serde_json::Value =
            serde_json::from_str(include_str!("rollout_model.json")).unwrap();
        let v = &file["policy"];
        assert_eq!(v["input"], INPUT);
        assert_eq!(v["hidden"], HIDDEN);
        let values = |key: &str, len: usize| {
            let x: Vec<f32> = v[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_f64().unwrap() as f32)
                .collect();
            assert_eq!(x.len(), len);
            assert!(x.iter().all(|n| n.is_finite()));
            x
        };
        let original = values("w1", INPUT * HIDDEN);
        let w1 = (0..INPUT)
            .map(|i| std::array::from_fn(|h| original[h * INPUT + i]))
            .collect();
        Net {
            w1,
            b1: values("b1", HIDDEN),
            w2: values("w2", HIDDEN),
            b2: v["b2"].as_f64().unwrap() as f32,
        }
    })
}

fn scores<'a>(
    e: &'a Engine,
    actor: Player,
    space: &'a ActionSpace,
    choices: &'a SearchChoices,
) -> impl Iterator<Item = (u128, f32)> + 'a {
    let net = model();
    let state = stack_state_features(e, actor);
    let mut base = hidden_sum(&net.w1[..STACK_STATE_DIM], state);
    for h in 0..HIDDEN {
        base[h] = net.b1[h] + base[h];
    }
    (0..choices.len()).map(move |rank| {
        let a = action_features(e, actor, space.get(choices.index(rank)));
        let action = hidden_sum(&net.w1[STACK_STATE_DIM..], a);
        let mut score = net.b2;
        for h in 0..HIDDEN {
            let z = base[h] + action[h];
            score += z.max(0.) * net.w2[h];
        }
        (rank, score)
    })
}

#[inline]
fn hidden_sum<const N: usize>(weights: &[[f32; HIDDEN]], input: [f32; N]) -> [f32; HIDDEN] {
    // Match Iterator::sum's identity, including signed zero.
    let mut sums = [-0.; HIDDEN];
    for (weights, x) in weights.iter().zip(input) {
        for h in 0..HIDDEN {
            sums[h] += weights[h] * x;
        }
    }
    sums
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_lanes_preserve_scalar_sum_bits() {
        let net = model();
        let mut rng = Random(0x706f6c696379);
        // Dense and sparse inputs, signs, zero, and mixed magnitudes exercise
        // rounding and cancellation. Compare bits, not an epsilon tolerance.
        for case in 0..4096 {
            let input: [f32; INPUT] = std::array::from_fn(|_| {
                if case == 0 {
                    return 0.;
                }
                if case == 1 {
                    return -0.;
                }
                let n = rng.below(2049) as i32 - 1024;
                if case % 2 == 0 && rng.below(4) != 0 {
                    return 0.;
                }
                n as f32 * 2_f32.powi(rng.below(25) as i32 - 16)
            });
            let state: [f32; STACK_STATE_DIM] = input[..STACK_STATE_DIM].try_into().unwrap();
            let action: [f32; ACTION_DIM] = input[STACK_STATE_DIM..].try_into().unwrap();
            for (start, actual) in [
                (0, hidden_sum(&net.w1[..STACK_STATE_DIM], state)),
                (
                    STACK_STATE_DIM,
                    hidden_sum(&net.w1[STACK_STATE_DIM..], action),
                ),
            ] {
                let end = if start == 0 { STACK_STATE_DIM } else { INPUT };
                for h in 0..HIDDEN {
                    let expected: f32 = (start..end).map(|i| net.w1[i][h] * input[i]).sum();
                    assert_eq!(
                        actual[h].to_bits(),
                        expected.to_bits(),
                        "case {case}, lane {h}"
                    );
                }
            }
        }
    }
}

pub(super) fn prior(
    e: &Engine,
    actor: Player,
    space: &ActionSpace,
    choices: &SearchChoices,
) -> Option<Vec<f64>> {
    if choices.len() > 64 {
        return None;
    }
    let scores: Vec<_> = scores(e, actor, space, choices)
        .map(|(_, score)| score)
        .collect();
    let max = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max) as f64;
    let mut p: Vec<_> = scores.iter().map(|&x| (x as f64 - max).exp()).collect();
    let total: f64 = p.iter().sum();
    let n = p.len() as f64;
    for x in &mut p {
        *x = 0.9 * (*x / total) + 0.1 / n;
    }
    Some(p)
}

pub(super) fn choose(
    e: &Engine,
    actor: Player,
    space: &ActionSpace,
    choices: &SearchChoices,
    rng: &mut Random,
) -> u128 {
    if choices.len() > 64 || rng.below(5) == 0 {
        return choices.index(rng.below(choices.len()));
    }
    let mut best = f32::NEG_INFINITY;
    let mut candidates = Vec::new();
    for (rank, score) in scores(e, actor, space, choices) {
        if score > best {
            best = score;
            candidates.clear();
            candidates.push(rank);
        } else if score == best {
            candidates.push(rank);
        }
    }
    choices.index(candidates[rng.below(candidates.len() as u128) as usize])
}
