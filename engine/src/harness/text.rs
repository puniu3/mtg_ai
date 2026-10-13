//! Stable diagnostic action text used by private replay logs.
use crate::Action;

pub fn action_text(a: Action) -> String {
    match a {
        Action::Attack { objects, len } => format!("Attack {:?}", &objects[..len as usize]),
        Action::Block { pairs, len } => format!("Block {:?}", &pairs[..len as usize]),
        Action::Damage { assignments, len } => format!("Damage {:?}", &assignments[..len as usize]),
        Action::Shuffle { .. } => "Shuffle (environment)".into(),
        _ => format!("{a:?}"),
    }
}
