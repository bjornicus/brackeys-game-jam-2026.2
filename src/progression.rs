use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};

/// Optional active abilities that can be unlocked during a run.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum Skill {
    Projectile,
    Stun,
    Teleport,
}

/// Stable map-tile identity for any runtime map placement.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct PlacementId {
    pub x: i32,
    pub y: i32,
}

/// Permanent-within-a-run progression. Runtime effects remain outside this snapshot.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Resource, Serialize)]
pub struct RunProgress {
    pub activated_terminals: std::collections::BTreeSet<PlacementId>,
    pub collected_pickups: std::collections::BTreeSet<PlacementId>,
    pub defeated_enemies: std::collections::BTreeSet<PlacementId>,
    pub unlocked_skills: std::collections::BTreeSet<Skill>,
    pub armor_collected: u32,
}

/// The latest checkpoint's permanent progress and respawn tile.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Resource, Serialize)]
pub struct CheckpointSnapshot {
    pub progress: RunProgress,
    pub respawn: PlacementId,
}

impl Default for CheckpointSnapshot {
    fn default() -> Self {
        Self {
            progress: RunProgress::default(),
            respawn: PlacementId::default(),
        }
    }
}
