use serde::{Deserialize, Serialize};

/// Optional active abilities that can be unlocked during a run.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum Skill {
    Projectile,
    Stun,
    Teleport,
}
