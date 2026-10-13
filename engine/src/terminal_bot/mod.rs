//! Terminal-outcome search policy and its learned rollout/prior.
//! Interaction primitives are re-exported for existing callers; new external
//! player adapters should depend on `crate::harness` directly.
mod features;
mod no_suicide;
mod rollout_policy;
mod search;
#[cfg(test)]
use crate::harness::actions;
pub use crate::harness::{
    ActionSpace, AutoStep, Observation, Position, PublicEvent, Random, action_text, auto_step,
    chance_action, deck, inventory, resolve_chance,
};
use crate::harness::{chance_owner, zones};
use crate::*;
use mtg_state_design::burn::State;
pub use search::{Budget, Config, Method, SearchReport, SearchSession, ponder_candidate, search};

pub mod macro_control;
pub mod macro_search;
