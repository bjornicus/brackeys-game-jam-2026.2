//! Centralized, tunable gameplay configuration resources.

use bevy::prelude::Resource;

/// Starting combat and actor tuning values.
#[derive(Resource, Clone, Copy, Debug)]
pub(crate) struct CombatConfig {
    pub(crate) lightning_range: f32,
    pub(crate) lightning_damage: f32,
    pub(crate) lightning_visible_lifetime: f32,
    pub(crate) projectile_speed: f32,
    pub(crate) projectile_damage: f32,
    pub(crate) projectile_collision_radius: f32,
    pub(crate) projectile_maximum_lifetime: f32,
    pub(crate) enemy_maximum_health: f32,
    pub(crate) enemy_speed: f32,
    pub(crate) enemy_attack_distance: f32,
    pub(crate) player_maximum_health: f32,
    pub(crate) player_combat_hitbox_size: bevy::prelude::Vec2,
    pub(crate) player_invulnerability_duration: f32,
    pub(crate) teleport_cooldown: f32,
    pub(crate) shockwave_radius: f32,
    pub(crate) shockwave_animation_duration: f32,
    pub(crate) stun_duration: f32,
}

impl Default for CombatConfig {
    fn default() -> Self {
        Self {
            lightning_range: 220.0,
            lightning_damage: 40.0,
            lightning_visible_lifetime: 0.18,
            projectile_speed: 500.0,
            projectile_damage: 30.0,
            projectile_collision_radius: 6.0,
            projectile_maximum_lifetime: 3.0,
            enemy_maximum_health: 100.0,
            enemy_speed: 55.0,
            enemy_attack_distance: 64.0,
            player_maximum_health: 100.0,
            player_combat_hitbox_size: bevy::prelude::Vec2::new(48.0, 72.0),
            player_invulnerability_duration: 0.5,
            teleport_cooldown: 5.0,
            shockwave_radius: 120.0,
            shockwave_animation_duration: 0.4,
            stun_duration: 2.0,
        }
    }
}

/// Tunable trauma-based camera shake state and parameters.
#[derive(Resource, Clone, Copy, Debug)]
pub(crate) struct CameraShakeConfig {
    pub(crate) trauma: f32,
    pub(crate) trauma_per_stun: f32,
    pub(crate) trauma_decay_per_second: f32,
    pub(crate) exponent: f32,
    pub(crate) max_rotation: f32,
    pub(crate) max_translation: f32,
    pub(crate) noise_speed: f32,
}

impl Default for CameraShakeConfig {
    fn default() -> Self {
        Self {
            trauma: 0.0,
            trauma_per_stun: 0.45,
            trauma_decay_per_second: 1.2,
            exponent: 2.0,
            max_rotation: 0.08,
            max_translation: 12.0,
            noise_speed: 20.0,
        }
    }
}
