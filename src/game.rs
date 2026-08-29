//! There is no actual game, it will just display the current
//! settings for 5 seconds before going back to the menu.

use bevy::{app::AppExit, prelude::*, window::PrimaryWindow};
use bevy_spritesheet_animation::prelude::*;
use map_support::{
    collision::{self, Collider},
    dialogue::{DialogueCatalog, DialogueLine, Speaker, load_dialogue_catalog},
    map,
    progression::{CheckpointSnapshot, PlacementId, RunProgress, Skill},
    tilemap::{self, TerrainAtlas, TerrainTilemapPlugin},
};
use std::collections::HashSet;

const TEXT_COLOR: Color = Color::srgb(0.9, 0.9, 0.9);
const PAUSE_BUTTON_NORMAL: Color = Color::srgb(0.18, 0.25, 0.38);
const PAUSE_BUTTON_HOVERED: Color = Color::srgb(0.25, 0.42, 0.65);
const PAUSE_BUTTON_PRESSED: Color = Color::srgb(0.12, 0.65, 0.35);

use crate::{
    GameState,
    config::{CameraShakeConfig, CombatConfig},
    game_dialogue::{DialogueSource, request_dialogue},
};

// This plugin contains the game.
pub fn game_plugin(app: &mut App) {
    app.add_plugins(TerrainTilemapPlugin)
        .init_resource::<CollisionDebug>()
        .init_resource::<CombatConfig>()
        .init_resource::<SelectedAttackMode>()
        .init_resource::<CursorWorld>()
        .init_resource::<AttackFeedback>()
        .init_resource::<TeleportCooldown>()
        .init_resource::<CameraShakeConfig>()
        .init_resource::<EnemiesSpawned>()
        .init_resource::<TerminalsSpawned>()
        .init_resource::<PickupsSpawned>()
        .init_resource::<HealthDropSeed>()
        .init_resource::<HealthDropSeedSequence>()
        .insert_resource(StoryDialogueCatalog::load())
        .init_resource::<RestartRequest>()
        .init_resource::<RunProgress>()
        .init_resource::<CheckpointSnapshot>()
        .init_resource::<PlayerSpawn>()
        .init_resource::<RunNeedsSpawn>()
        .add_message::<AttackFired>()
        .add_systems(PreStartup, setup_enemy_animations)
        .add_message::<Damage>()
        .add_message::<DamageApplied>();
    app.add_systems(
        OnEnter(GameState::Game),
        (
            setup_scene,
            setup_instructions,
            setup_player_health_hud,
            spawn_character,
            setup_camera_shake,
            finish_run_setup,
        )
            .chain(),
    )
    .add_systems(OnEnter(GameState::Paused), pause_menu_setup)
    .add_systems(OnEnter(GameState::GameOver), setup_game_over_ui)
    .add_systems(OnEnter(GameState::Restarting), restart_run)
    .add_systems(
        Update,
        (
            update_cursor_world_position,
            select_attack_mode,
            tick_teleport_cooldown,
            control_character,
            activate_touched_terminal,
            trigger_attack_fire_event,
            handle_lightning_attacks,
            spawn_projectiles,
            move_projectiles,
            apply_damage,
            tick_lightning_visuals,
            tick_attack_feedback,
            handle_attack_animation_events,
            update_attack_hud,
            draw_lightning_range_feedback,
            draw_lightning_visuals,
            update_camera,
            pause_game,
            toggle_collision_debug,
            draw_collision_debug,
        )
            .chain()
            .run_if(in_state(GameState::Game)),
    )
    // World triggers are ordered so a terminal wins an overlap, then a pickup can refresh both
    // HUDs before its dialogue transition freezes gameplay on the next frame.
    .add_systems(
        Update,
        activate_touched_pickup
            .after(activate_touched_terminal)
            .before(update_attack_hud)
            .before(update_player_health_hud)
            .run_if(in_state(GameState::Game)),
    )
    .add_systems(
        Update,
        (tick_player_invulnerability, update_player_health_hud)
            .chain()
            .after(apply_damage)
            .run_if(in_state(GameState::Game)),
    )
    // The dialogue overlay pauses gameplay but may obscure a HUD change queued in the
    // preceding frame, so keep this purely-presentational refresh safe in Dialogue.
    .add_systems(
        Update,
        update_player_health_hud.run_if(in_state(GameState::Dialogue)),
    )
    .add_systems(
        Update,
        (begin_player_death, tick_player_death)
            .chain()
            .after(apply_damage)
            .run_if(in_state(GameState::Game)),
    )
    .add_systems(
        Update,
        game_over_action.run_if(in_state(GameState::GameOver)),
    )
    .add_systems(
        Update,
        (
            cast_stun.before(control_character).before(update_enemies),
            tick_shockwaves.after(cast_stun),
            draw_shockwaves,
        )
            .run_if(in_state(GameState::Game)),
    )
    .add_systems(
        Update,
        (
            spawn_enemies_from_map,
            spawn_terminals_from_map,
            spawn_pickups_from_map,
            attach_enemy_sprites,
            spawn_enemy_health_bars,
            tick_stunned_enemies,
            update_enemies,
            draw_enemy_attack_telegraphs,
            begin_enemy_deaths,
            finish_enemy_deaths,
            update_enemy_health_bars,
        )
            .chain()
            .run_if(in_state(GameState::Game)),
    )
    .add_systems(
        Update,
        (pause_menu_action, pause_menu_button_system).run_if(in_state(GameState::Paused)),
    )
    // Restore before cursor conversion/gameplay, then shake only for rendering.
    .add_systems(PreUpdate, restore_camera_transform)
    .add_systems(
        PostUpdate,
        shake_camera
            .before(TransformSystems::Propagate)
            .run_if(in_state(GameState::Game)),
    );
}

/// Player movement speed factor.
const PLAYER_SPEED: f32 = 100.;

/// How quickly should the camera snap to the desired location.
const CAMERA_DECAY_RATE: f32 = 2.;

// Tune these values to match the visible feet/body contact area in the sprite.
const PLAYER_COLLIDER_SIZE: Vec2 = Vec2::new(40.0, 20.0);
const PLAYER_COLLIDER_OFFSET: Vec2 = Vec2::new(0.0, -24.0);

#[derive(Component)]
struct Player;

#[derive(Component, Clone, Copy)]
struct PlayerCollider(Collider);

const ENEMY_COLLIDER_SIZE: Vec2 = Vec2::new(40.0, 20.0);
const ENEMY_COLLIDER_OFFSET: Vec2 = Vec2::new(0.0, -24.0);
const ENEMY_HITBOX_SIZE: Vec2 = Vec2::new(64.0, 80.0);
const ENEMY_SHEET_COLUMNS: usize = 4;
const ENEMY_SHEET_ROWS: usize = 4;
const ENEMY_SHEET_WIDTH: u32 = 256;
const ENEMY_SHEET_HEIGHT: u32 = 256;
const ENEMY_IDLE_ROW: usize = 0;
const ENEMY_MOVE_ROW: usize = 1;
const ENEMY_STUNNED_ROW: usize = 2;
const ENEMY_DEATH_ROW: usize = 3;
const ENEMY_ANIMATION_FRAMES: usize = 4;
const ENEMY_HEALTH_BAR_WIDTH: f32 = 48.0;
const ENEMY_HEALTH_BAR_HEIGHT: f32 = 6.0;
const ENEMY_HEALTH_BAR_OFFSET: Vec2 = Vec2::new(0.0, 46.0);

#[derive(Component)]
struct Enemy;

#[derive(Component, Clone, Copy)]
struct EnemyCollider(Collider);

#[derive(Component, Clone, Copy)]
struct EnemyMovement {
    speed: f32,
    attack_distance: f32,
}

/// The mutually exclusive melee state for a living enemy.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
enum EnemyAttack {
    Ready,
    WindingUp { remaining: f32 },
    Cooldown { remaining: f32 },
}

/// Stun refreshes to the full duration when it is applied again.
#[derive(Component, Clone, Copy, Debug)]
struct Stunned {
    remaining: f32,
}

#[derive(Component, Clone, Debug)]
struct Dying {
    animation: Handle<Animation>,
}

/// One-way player death presentation; the timer only advances during gameplay.
#[derive(Component, Clone, Copy, Debug)]
struct PlayerDying {
    remaining: f32,
}

#[derive(Component)]
enum GameOverAction {
    Continue,
    MainMenu,
}

#[derive(Component)]
struct EnemyHealthBar;

#[derive(Component)]
struct EnemyHealthBarFill;

#[derive(Resource, Default)]
struct EnemiesSpawned(bool);

#[derive(Resource, Default)]
struct TerminalsSpawned(bool);

#[derive(Resource, Default)]
struct PickupsSpawned(bool);

/// Per-run seed for deterministic health-drop eligibility. Checkpoint continuation preserves it.
#[derive(Resource, Clone, Copy, Debug, Eq, PartialEq)]
struct HealthDropSeed(u64);

impl Default for HealthDropSeed {
    fn default() -> Self {
        Self(0x7f4a_7c15_9e37_79b9)
    }
}

/// App-lifetime sequence that supplies a distinct deterministic seed to each new run.
/// It intentionally outlives Main Menu transitions; only the active `HealthDropSeed` is run state.
#[derive(Resource, Clone, Copy, Debug, Eq, PartialEq)]
struct HealthDropSeedSequence(u64);

impl Default for HealthDropSeedSequence {
    fn default() -> Self {
        Self(0xd1b5_4a32_d192_ed03)
    }
}

impl HealthDropSeedSequence {
    fn next_run_seed(&mut self) -> HealthDropSeed {
        self.0 = next_health_drop_seed(self.0);
        HealthDropSeed(self.0)
    }
}

#[derive(Component, Clone, Copy, Debug)]
struct HealthDrop {
    source: PlacementId,
}

/// Reserved for Step 2 collection logic.
#[derive(Component)]
struct HealthDropTrigger;

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
enum PickupKind {
    Skill(Skill),
    ReinforcedArmor,
}

#[derive(Component, Clone, Copy, Debug)]
struct Pickup {
    placement: PlacementId,
    kind: PickupKind,
}

#[derive(Component)]
struct PickupTrigger;

#[derive(Component, Clone, Debug)]
struct Terminal {
    placement: PlacementId,
    dialogue_id: String,
    activated: bool,
}

#[derive(Component)]
struct TerminalTrigger;

#[derive(Resource)]
struct EnemyAnimations {
    idle: Handle<Animation>,
    movement: Handle<Animation>,
    stunned: Handle<Animation>,
    death: Handle<Animation>,
}

#[derive(Resource)]
struct GameMap(map::MapData);

#[derive(Resource, Default)]
struct StoryDialogueCatalog(DialogueCatalog);

impl StoryDialogueCatalog {
    fn load() -> Self {
        match load_dialogue_catalog("story") {
            Ok(catalog) => Self(catalog),
            Err(error) => {
                warn!("Could not load story dialogue catalog: {error}");
                Self::default()
            }
        }
    }
}

#[allow(dead_code)] // Step 5/7 consume this stable identity.
#[derive(Component, Clone, Copy, Debug)]
struct Placement(PlacementId);

/// Marks a top-level entity that belongs to the current disposable game run.
#[derive(Component)]
pub(crate) struct RunScoped;

/// Which clean rebuild the one-frame Restarting state performs.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum RestartIntent {
    #[default]
    NewGame,
    ContinueCheckpoint,
    MainMenu,
}

#[derive(Resource, Debug, Default)]
pub(crate) struct RestartRequest(pub(crate) RestartIntent);

#[derive(Resource, Clone, Copy, Debug, Default)]
struct PlayerSpawn(Vec2);

#[derive(Resource)]
struct RunNeedsSpawn(bool);

impl Default for RunNeedsSpawn {
    fn default() -> Self {
        Self(true)
    }
}

#[derive(Resource, Default)]
struct CollisionDebug {
    enabled: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AttackMode {
    Lightning,
    Projectile,
    Auto,
}

impl AttackMode {
    fn label(self) -> &'static str {
        match self {
            Self::Lightning => "Lightning",
            Self::Projectile => "Projectile",
            Self::Auto => "Auto",
        }
    }
}

#[derive(Resource, Clone, Copy, Debug)]
struct SelectedAttackMode(AttackMode);

impl Default for SelectedAttackMode {
    fn default() -> Self {
        Self(AttackMode::Lightning)
    }
}

#[derive(Resource, Default, Clone, Copy, Debug)]
struct CursorWorld(Option<Vec2>);

#[derive(Resource, Default, Clone, Copy, Debug)]
struct AttackFeedback {
    rejection: Option<AttackRejection>,
}

#[derive(Clone, Copy, Debug)]
struct AttackRejection {
    target: Vec2,
    remaining: f32,
    reason: RejectionReason,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RejectionReason {
    OutOfRange,
    Obstructed,
}

#[derive(Resource, Clone, Copy, Debug, Default)]
struct TeleportCooldown {
    remaining: f32,
    rejection: Option<TeleportRejection>,
    rejection_remaining: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TeleportRejection {
    Busy,
    Cooldown,
    InvalidTerrain,
}

impl TeleportCooldown {
    fn ready(self) -> bool {
        self.remaining <= 0.0
    }
}

#[derive(Component)]
struct AttackHud;

/// Root of the fixed-screen player-health HUD for the current run.
#[derive(Component)]
struct PlayerHealthHud;

#[derive(Component)]
struct PlayerHealthBarFill;

#[derive(Component)]
struct PlayerHealthText;

#[derive(Component, Clone, Copy, Debug)]
struct Invulnerable {
    remaining: f32,
}

#[derive(Component, Clone, Copy, Debug)]
struct PlayerHitFlash;

#[derive(Component, Clone, Copy, Debug)]
struct Hitbox(Collider);

#[allow(dead_code)]
#[derive(Component, Clone, Copy, Debug, PartialEq)]
struct Health {
    current: f32,
    max: f32,
}

#[derive(Message, Clone, Copy, Debug)]
struct Damage {
    target: Entity,
    amount: f32,
}

/// Emitted only after a damage message reduced a target's health.
#[derive(Message, Clone, Copy, Debug)]
struct DamageApplied {
    target: Entity,
    amount: f32,
}

#[derive(Component, Clone, Copy, Debug)]
struct LightningVisual {
    start: Vec2,
    end: Vec2,
    remaining: f32,
}

#[derive(Component, Clone, Copy, Debug)]
struct ShockwaveVisual {
    origin: Vec2,
    elapsed: f32,
    duration: f32,
    radius: f32,
}

#[derive(Component, Clone, Copy, Debug)]
struct UnshakenCameraTransform(Transform);

#[derive(Component, Clone, Copy, Debug)]
struct Projectile {
    owner: Entity,
    faction: Faction,
    direction: Vec2,
    speed: f32,
    radius: f32,
    damage: f32,
    remaining_lifetime: f32,
}

#[allow(dead_code)]
#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
enum Faction {
    Player,
    Enemy,
}

#[derive(Component)]
enum PauseMenuAction {
    Resume,
    Quit,
}

const PLAYER_SHEET_COLUMNS: usize = 4;
const PLAYER_SHEET_ROWS: usize = 12;
const PLAYER_SHEET_WIDTH: u32 = 256;
const PLAYER_SHEET_HEIGHT: u32 = 768;
const PLAYER_IDLE_FRAMES: usize = 2;
const PLAYER_MOVE_FRAMES: usize = 4;
const PLAYER_SHOOT_FRAMES: usize = 4;
const PLAYER_SHOOT_FIRE_FRAME: usize = 1;
const PLAYER_STATES_PER_DIRECTION: usize = 3;
const PLAYER_IDLE_STATE_ROW_OFFSET: usize = 0;
const PLAYER_MOVE_STATE_ROW_OFFSET: usize = 1;
const PLAYER_SHOOT_STATE_ROW_OFFSET: usize = 2;

fn player_animation_row(facing: Facing, state_row_offset: usize) -> usize {
    facing as usize * PLAYER_STATES_PER_DIRECTION + state_row_offset
}

// Let's use a custom resource to store our animations and access them across systems
#[derive(Resource)]
struct PlayerAnimations {
    idle: [Handle<Animation>; 4],
    movement: [Handle<Animation>; 4],
    shoot: [Handle<Animation>; 4],
}

impl PlayerAnimations {
    fn idle_for(&self, facing: Facing) -> &Handle<Animation> {
        &self.idle[facing.animation_index()]
    }

    fn movement_for(&self, facing: Facing) -> &Handle<Animation> {
        &self.movement[facing.animation_index()]
    }

    fn shoot_for(&self, facing: Facing) -> &Handle<Animation> {
        &self.shoot[facing.animation_index()]
    }
}

fn setup_enemy_animations(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut animations: ResMut<Assets<Animation>>,
) {
    let image = assets.load("sprites/enemy_placeholder.png");
    let sheet = Spritesheet::new(&image, ENEMY_SHEET_COLUMNS, ENEMY_SHEET_ROWS);
    let make_animation = |animations: &mut Assets<Animation>, row| {
        animations.add(
            sheet
                .create_animation()
                .add_horizontal_strip(0, row, ENEMY_ANIMATION_FRAMES)
                .build(),
        )
    };
    commands.insert_resource(EnemyAnimations {
        idle: make_animation(&mut animations, ENEMY_IDLE_ROW),
        movement: make_animation(&mut animations, ENEMY_MOVE_ROW),
        stunned: make_animation(&mut animations, ENEMY_STUNNED_ROW),
        death: make_animation(&mut animations, ENEMY_DEATH_ROW),
    });
}

fn setup_scene(
    mut commands: Commands,
    terrain_atlas: Res<TerrainAtlas>,
    game_map: Option<Res<GameMap>>,
    needs_spawn: Res<RunNeedsSpawn>,
) {
    if !needs_spawn.0 {
        return;
    }
    let map = game_map.as_ref().map_or_else(
        || {
            map::load_map("initial").unwrap_or_else(|error| {
                warn!("Could not load initial map: {error}. Using the built-in map.");
                map::MapData::initial()
            })
        },
        |map| map.0.clone(),
    );
    if game_map.is_none() {
        commands.insert_resource(GameMap(map.clone()));
    }
    for terrain in tilemap::spawn_map(&mut commands, &terrain_atlas, &map) {
        commands.entity(terrain).insert(RunScoped);
    }
}

fn finish_run_setup(mut needs_spawn: ResMut<RunNeedsSpawn>) {
    needs_spawn.0 = false;
}

/// Removes the current run and schedules its clean replacement. The map asset stays immutable
/// in `GameMap`; later steps use `RunProgress` to filter its placements during rebuilding.
fn restart_run(
    mut commands: Commands,
    run_entities: Query<Entity, With<RunScoped>>,
    mut intent: ResMut<RestartRequest>,
    mut progress: ResMut<RunProgress>,
    mut checkpoint: ResMut<CheckpointSnapshot>,
    mut selected_mode: ResMut<SelectedAttackMode>,
    mut teleport: ResMut<TeleportCooldown>,
    mut feedback: ResMut<AttackFeedback>,
    mut collision_debug: ResMut<CollisionDebug>,
    mut shake: ResMut<CameraShakeConfig>,
    mut spawn_lifecycle: ParamSet<(
        ResMut<EnemiesSpawned>,
        ResMut<TerminalsSpawned>,
        ResMut<HealthDropSeed>,
        ResMut<HealthDropSeedSequence>,
    )>,
    mut player_spawn: ResMut<PlayerSpawn>,
    mut needs_spawn: ResMut<RunNeedsSpawn>,
    mut cameras: Query<(&UnshakenCameraTransform, &mut Transform), With<Camera2d>>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    for entity in &run_entities {
        commands.entity(entity).despawn();
    }
    for (unshaken, mut transform) in &mut cameras {
        *transform = unshaken.0;
    }
    *selected_mode = SelectedAttackMode::default();
    *teleport = TeleportCooldown::default();
    *feedback = AttackFeedback::default();
    *collision_debug = CollisionDebug::default();
    shake.trauma = 0.0;
    spawn_lifecycle.p0().0 = false;
    spawn_lifecycle.p1().0 = false;
    commands.insert_resource(PickupsSpawned::default());
    needs_spawn.0 = true;

    match intent.0 {
        RestartIntent::NewGame => {
            *spawn_lifecycle.p2() = spawn_lifecycle.p3().next_run_seed();
            *progress = RunProgress::default();
            *checkpoint = CheckpointSnapshot::default();
            *player_spawn = PlayerSpawn::default();
            next_state.set(GameState::Game);
        }
        RestartIntent::ContinueCheckpoint => {
            *progress = checkpoint.progress.clone();
            player_spawn.0 = Vec2::new(
                checkpoint.respawn.x as f32 * map::TILE_SIZE,
                checkpoint.respawn.y as f32 * map::TILE_SIZE,
            );
            next_state.set(GameState::Game);
        }
        RestartIntent::MainMenu => {
            // The active seed is discarded; the app-lifetime sequence remains for the next run.
            *spawn_lifecycle.p2() = HealthDropSeed::default();
            *progress = RunProgress::default();
            *checkpoint = CheckpointSnapshot::default();
            *player_spawn = PlayerSpawn::default();
            next_state.set(GameState::Menu);
        }
    }
    intent.0 = RestartIntent::NewGame;
}

fn spawn_character(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut animations: ResMut<Assets<Animation>>,
    mut atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    spawn: Res<PlayerSpawn>,
    needs_spawn: Res<RunNeedsSpawn>,
    combat_config: Res<CombatConfig>,
    progress: Res<RunProgress>,
) {
    if !needs_spawn.0 {
        return;
    }
    // Create the animations

    let image = assets.load("sprites/character_placeholder.png");

    let spritesheet = Spritesheet::new(&image, PLAYER_SHEET_COLUMNS, PLAYER_SHEET_ROWS);

    let make_strip_animation = |animations: &mut Assets<Animation>, row, frames| {
        animations.add(
            spritesheet
                .create_animation()
                .add_horizontal_strip(0, row, frames)
                .build(),
        )
    };
    let make_shoot_animation = |animations: &mut Assets<Animation>, row| {
        animations.add(
            spritesheet
                .create_animation()
                .add_horizontal_strip(0, row, PLAYER_SHOOT_FRAMES)
                .build(),
        )
    };

    let idle = Facing::ALL.map(|facing| {
        make_strip_animation(
            &mut animations,
            player_animation_row(facing, PLAYER_IDLE_STATE_ROW_OFFSET),
            PLAYER_IDLE_FRAMES,
        )
    });
    let movement = Facing::ALL.map(|facing| {
        make_strip_animation(
            &mut animations,
            player_animation_row(facing, PLAYER_MOVE_STATE_ROW_OFFSET),
            PLAYER_MOVE_FRAMES,
        )
    });
    let shoot = Facing::ALL.map(|facing| {
        make_shoot_animation(
            &mut animations,
            player_animation_row(facing, PLAYER_SHOOT_STATE_ROW_OFFSET),
        )
    });

    // Store the animations as a resource

    commands.insert_resource(PlayerAnimations {
        idle: idle.clone(),
        movement,
        shoot,
    });

    // Spawn the character

    let sprite = spritesheet
        .with_size_hint(PLAYER_SHEET_WIDTH, PLAYER_SHEET_HEIGHT)
        .sprite(&mut atlas_layouts);

    commands.spawn((
        RunScoped,
        Player,
        Faction::Player,
        PlayerCollider(Collider::new(PLAYER_COLLIDER_SIZE, PLAYER_COLLIDER_OFFSET)),
        Hitbox(Collider::new(
            combat_config.player_combat_hitbox_size,
            Vec2::ZERO,
        )),
        restored_player_health(&combat_config, &progress),
        Facing::Right,
        sprite,
        SpritesheetAnimation::new(idle[Facing::Right.animation_index()].clone()),
        Transform::from_xyz(spawn.0.x, spawn.0.y, 2.),
    ));
}

/// Returns the center of an entity's map tile in world coordinates.
fn map_entity_world_center(entity: &map::MapEntity) -> Vec2 {
    Vec2::new(
        entity.x as f32 * map::TILE_SIZE,
        entity.y as f32 * map::TILE_SIZE,
    )
}

fn enemy_spawn_position(entity: &map::MapEntity) -> Vec2 {
    map_entity_world_center(entity)
}

fn spawn_terminals_from_map(
    mut commands: Commands,
    game_map: Option<Res<GameMap>>,
    progress: Res<RunProgress>,
    mut spawned: ResMut<TerminalsSpawned>,
) {
    if spawned.0 {
        return;
    }
    let Some(game_map) = game_map else {
        return;
    };

    let mut placements: Vec<_> = game_map
        .0
        .entities
        .iter()
        .filter(|placement| matches!(placement.kind, map::MapEntityKind::Terminal { .. }))
        .collect();
    placements.sort_by_key(|placement| (placement.y, placement.x));
    for placement in placements {
        let map::MapEntityKind::Terminal { dialogue_id } = &placement.kind else {
            unreachable!("terminal filter must retain only terminal placements");
        };
        let identity = PlacementId {
            x: placement.x,
            y: placement.y,
        };
        let activated = terminal_is_activated(&progress, identity);
        let mut entity = commands.spawn((
            RunScoped,
            Placement(identity),
            Terminal {
                placement: identity,
                dialogue_id: dialogue_id.clone(),
                activated,
            },
            Sprite {
                color: if activated {
                    Color::srgb(0.12, 0.18, 0.24)
                } else {
                    Color::srgb(0.15, 0.7, 1.0)
                },
                custom_size: Some(Vec2::new(48.0, 64.0)),
                ..default()
            },
            Transform::from_translation(map_entity_world_center(placement).extend(1.0)),
        ));
        if !activated {
            entity.insert(TerminalTrigger);
        }
    }
    spawned.0 = true;
}

fn terminal_is_activated(progress: &RunProgress, placement: PlacementId) -> bool {
    progress.activated_terminals.contains(&placement)
}

fn commit_terminal_activation(
    terminal: &mut Terminal,
    progress: &mut RunProgress,
    checkpoint: &mut CheckpointSnapshot,
) {
    terminal.activated = true;
    progress.activated_terminals.insert(terminal.placement);
    checkpoint.progress = progress.clone();
    checkpoint.respawn = terminal.placement;
}

fn terminal_dialogue_lines(catalog: &DialogueCatalog, dialogue_id: &str) -> Vec<DialogueLine> {
    match catalog.conversation(dialogue_id) {
        Ok(lines) => lines.to_vec(),
        Err(error) => {
            error!("Terminal dialogue ID '{dialogue_id}' could not be opened: {error}");
            vec![DialogueLine {
                speaker: Speaker::System,
                text: format!("Terminal dialogue '{dialogue_id}' is unavailable."),
            }]
        }
    }
}

/// Trigger contact includes exact AABB boundary contact.
fn terminal_trigger_overlaps(
    player_position: Vec2,
    player_collider: Collider,
    terminal_position: Vec2,
    trigger_size: Vec2,
) -> bool {
    let player = player_collider.aabb_at(player_position);
    let trigger = collision::Aabb::from_center_size(terminal_position, trigger_size);
    player.min.x <= trigger.max.x
        && player.max.x >= trigger.min.x
        && player.min.y <= trigger.max.y
        && player.max.y >= trigger.min.y
}

fn activate_touched_terminal(
    mut commands: Commands,
    player: Single<
        (
            &Transform,
            &PlayerCollider,
            Option<&Health>,
            Option<&PlayerDying>,
        ),
        With<Player>,
    >,
    mut terminal_queries: ParamSet<(
        Query<(Entity, &Terminal, &Transform), With<TerminalTrigger>>,
        Query<(&mut Terminal, &mut Sprite)>,
    )>,
    mut progress: ResMut<RunProgress>,
    mut checkpoint: ResMut<CheckpointSnapshot>,
    catalog: Res<StoryDialogueCatalog>,
    combat_config: Res<CombatConfig>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    let (player_transform, player_collider, health, dying) = *player;
    if dying.is_some() || health.is_some_and(|health| health.current <= 0.0) {
        return;
    }
    let player_position = player_transform.translation.xy();
    let selected = terminal_queries
        .p0()
        .iter()
        .filter(|(_, _, transform)| {
            terminal_trigger_overlaps(
                player_position,
                player_collider.0,
                transform.translation.xy(),
                combat_config.terminal_trigger_size,
            )
        })
        .map(|(entity, terminal, _)| (entity, terminal.placement, terminal.dialogue_id.clone()))
        .min_by_key(|(_, placement, _)| (placement.y, placement.x));
    let Some((entity, _placement, dialogue_id)) = selected else {
        return;
    };

    let lines = terminal_dialogue_lines(&catalog.0, &dialogue_id);

    let mut terminal_state = terminal_queries.p1();
    let Ok((mut terminal, mut sprite)) = terminal_state.get_mut(entity) else {
        return;
    };
    // This ordering deliberately commits the checkpoint before requesting the modal.
    commit_terminal_activation(&mut terminal, &mut progress, &mut checkpoint);
    sprite.color = Color::srgb(0.12, 0.18, 0.24);
    commands.entity(entity).remove::<TerminalTrigger>();
    request_dialogue(
        &mut commands,
        &mut next_state,
        lines,
        DialogueSource::Terminal,
    );
}

fn pickup_kind_from_map(kind: &map::MapEntityKind) -> Option<PickupKind> {
    match kind {
        map::MapEntityKind::SkillPickup { skill } => Some(PickupKind::Skill(*skill)),
        map::MapEntityKind::ReinforcedArmorPickup => Some(PickupKind::ReinforcedArmor),
        _ => None,
    }
}

fn spawn_pickups_from_map(
    mut commands: Commands,
    game_map: Option<Res<GameMap>>,
    progress: Res<RunProgress>,
    mut spawned: ResMut<PickupsSpawned>,
) {
    if spawned.0 {
        return;
    }
    let Some(game_map) = game_map else {
        return;
    };
    let mut placements: Vec<_> = game_map
        .0
        .entities
        .iter()
        .filter_map(|placement| pickup_kind_from_map(&placement.kind).map(|kind| (placement, kind)))
        .collect();
    placements.sort_by_key(|(placement, _)| (placement.y, placement.x));
    for (placement, kind) in placements {
        let id = PlacementId {
            x: placement.x,
            y: placement.y,
        };
        if progress.collected_pickups.contains(&id) {
            continue;
        }
        let color = match kind {
            PickupKind::Skill(Skill::Projectile) => Color::srgb(0.95, 0.75, 0.12),
            PickupKind::Skill(Skill::Stun) => Color::srgb(0.65, 0.25, 0.9),
            PickupKind::Skill(Skill::Teleport) => Color::srgb(0.1, 0.8, 0.7),
            PickupKind::ReinforcedArmor => Color::srgb(0.45, 0.85, 0.45),
        };
        commands.spawn((
            RunScoped,
            Placement(id),
            Pickup {
                placement: id,
                kind,
            },
            PickupTrigger,
            Sprite {
                color,
                custom_size: Some(Vec2::splat(34.0)),
                ..default()
            },
            Transform::from_translation(map_entity_world_center(placement).extend(1.0)),
        ));
    }
    spawned.0 = true;
}

fn pickup_message(kind: PickupKind, already_owned: bool) -> &'static str {
    if already_owned {
        return "This ability is already acquired.";
    }
    match kind {
        PickupKind::Skill(Skill::Projectile) => {
            "You have unlocked the long-range attack. Press 2 to select it, or press 3 for automatic range selection. Fire with left click."
        }
        PickupKind::Skill(Skill::Stun) => {
            "You have unlocked stun. Press either Shift key or middle click to stun nearby enemies."
        }
        PickupKind::Skill(Skill::Teleport) => {
            "You have unlocked teleport. Right click valid floor to teleport."
        }
        PickupKind::ReinforcedArmor => {
            "You have unlocked Reinforced Armor. Maximum health increased by 50."
        }
    }
}

fn apply_pickup(
    kind: PickupKind,
    progress: &mut RunProgress,
    health: &mut Health,
    config: &CombatConfig,
) -> bool {
    match kind {
        PickupKind::Skill(skill) => progress.unlocked_skills.insert(skill),
        PickupKind::ReinforcedArmor => {
            progress.armor_collected += 1;
            progress.maximum_health_bonus += config.reinforced_armor_maximum_health_increase as u32;
            health.max += config.reinforced_armor_maximum_health_increase;
            health.current = (health.current + config.reinforced_armor_healing).min(health.max);
            true
        }
    }
}

fn restored_player_health(config: &CombatConfig, progress: &RunProgress) -> Health {
    let maximum = config.player_maximum_health + progress.maximum_health_bonus as f32;
    Health {
        current: maximum,
        max: maximum,
    }
}

fn activate_touched_pickup(
    mut commands: Commands,
    player: Single<
        (
            &Transform,
            &PlayerCollider,
            &mut Health,
            Option<&PlayerDying>,
        ),
        With<Player>,
    >,
    pickups: Query<(Entity, &Pickup, &Transform), With<PickupTrigger>>,
    terminals: Query<(&Transform, &Terminal), With<TerminalTrigger>>,
    mut progress: ResMut<RunProgress>,
    config: Res<CombatConfig>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    let (transform, collider, mut health, dying) = player.into_inner();
    if dying.is_some() || health.current <= 0.0 {
        return;
    }
    // Terminal wins every overlap frame, even before deferred trigger removal applies.
    if terminals.iter().any(|(terminal_transform, _)| {
        terminal_trigger_overlaps(
            transform.translation.xy(),
            collider.0,
            terminal_transform.translation.xy(),
            config.terminal_trigger_size,
        )
    }) {
        return;
    }
    let selected = pickups
        .iter()
        .filter(|(_, _, pickup_transform)| {
            terminal_trigger_overlaps(
                transform.translation.xy(),
                collider.0,
                pickup_transform.translation.xy(),
                config.pickup_trigger_size,
            )
        })
        .min_by_key(|(_, pickup, _)| (pickup.placement.y, pickup.placement.x))
        .map(|(entity, pickup, _)| (entity, *pickup));
    let Some((entity, pickup)) = selected else {
        return;
    };
    let already_owned = match pickup.kind {
        PickupKind::Skill(skill) => progress.unlocked_skills.contains(&skill),
        PickupKind::ReinforcedArmor => false,
    };
    progress.collected_pickups.insert(pickup.placement);
    apply_pickup(pickup.kind, &mut progress, &mut health, &config);
    commands.entity(entity).despawn();
    request_dialogue(
        &mut commands,
        &mut next_state,
        vec![DialogueLine {
            speaker: Speaker::System,
            text: pickup_message(pickup.kind, already_owned).to_owned(),
        }],
        DialogueSource::Unlock,
    );
}

fn spawn_enemies_from_map(
    mut commands: Commands,
    game_map: Option<Res<GameMap>>,
    animations: Option<Res<EnemyAnimations>>,
    combat_config: Res<CombatConfig>,
    progress: Option<Res<RunProgress>>,
    mut spawned: ResMut<EnemiesSpawned>,
) {
    if spawned.0 {
        return;
    }
    let (Some(game_map), Some(animations)) = (game_map, animations) else {
        return;
    };

    for placement in &game_map.0.entities {
        if !matches!(placement.kind, map::MapEntityKind::Enemy) {
            continue;
        }
        let placement_id = PlacementId {
            x: placement.x,
            y: placement.y,
        };
        if progress
            .as_ref()
            .is_some_and(|progress| progress.defeated_enemies.contains(&placement_id))
        {
            continue;
        }
        commands.spawn((
            RunScoped,
            Placement(placement_id),
            Enemy,
            Faction::Enemy,
            Facing::Down,
            EnemyCollider(Collider::new(ENEMY_COLLIDER_SIZE, ENEMY_COLLIDER_OFFSET)),
            Hitbox(Collider::new(ENEMY_HITBOX_SIZE, Vec2::ZERO)),
            Health {
                current: combat_config.enemy_maximum_health,
                max: combat_config.enemy_maximum_health,
            },
            EnemyMovement {
                speed: combat_config.enemy_speed,
                attack_distance: combat_config.enemy_attack_distance,
            },
            EnemyAttack::Ready,
            SpritesheetAnimation::new(animations.idle.clone()),
            Transform::from_translation(enemy_spawn_position(placement).extend(2.0)),
        ));
    }
    spawned.0 = true;
}

fn attach_enemy_sprites(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    enemies: Query<Entity, Added<Enemy>>,
) {
    let image = assets.load("sprites/enemy_placeholder.png");
    let sheet = Spritesheet::new(&image, ENEMY_SHEET_COLUMNS, ENEMY_SHEET_ROWS);
    for enemy in &enemies {
        commands.entity(enemy).insert(
            sheet
                .with_size_hint(ENEMY_SHEET_WIDTH, ENEMY_SHEET_HEIGHT)
                .sprite(&mut atlas_layouts),
        );
    }
}

fn enemy_chase_position(
    enemy_position: Vec2,
    player_position: Vec2,
    movement: EnemyMovement,
    collider: Collider,
    delta_secs: f32,
    occupancy: &collision::Occupancy<'_>,
) -> Vec2 {
    let direction = player_position - enemy_position;
    if direction.length() <= movement.attack_distance {
        enemy_position
    } else {
        collision::move_axis_separated(
            enemy_position,
            direction.normalize_or_zero() * movement.speed * delta_secs,
            collider,
            occupancy,
        )
    }
}

#[allow(dead_code)] // Step 10 invokes this when a shockwave applies or refreshes stun.
fn refreshed_stun(duration: f32) -> Stunned {
    // Reapplying stun deliberately resets, rather than stacks, its duration.
    Stunned {
        remaining: duration,
    }
}

fn stun_requested(keyboard: &ButtonInput<KeyCode>, mouse: &ButtonInput<MouseButton>) -> bool {
    keyboard.just_pressed(KeyCode::ShiftLeft)
        || keyboard.just_pressed(KeyCode::ShiftRight)
        || mouse.just_pressed(MouseButton::Middle)
}

/// Shift or middle click casts one shockwave even if several bindings arrive together.
fn cast_stun(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut commands: Commands,
    combat_config: Res<CombatConfig>,
    progress: Option<Res<RunProgress>>,
    mut shake: ResMut<CameraShakeConfig>,
    player: Single<
        (
            &Transform,
            Option<&PlayerAction>,
            Option<&Health>,
            Option<&PlayerDying>,
        ),
        With<Player>,
    >,
    mut enemies: Query<
        (Entity, &Transform, &Hitbox, Option<&mut EnemyAttack>),
        (With<Enemy>, With<Health>, Without<Dying>),
    >,
) {
    // Firing accepts no gameplay input other than pause (handled separately).
    if !progress
        .as_ref()
        .is_none_or(|progress| progress.unlocked_skills.contains(&Skill::Stun))
        || !stun_requested(&keyboard, &mouse)
        || player.1.is_some()
        || player.2.is_some_and(|health| health.current <= 0.0)
        || player.3.is_some()
    {
        return;
    }

    let origin = player.0.translation.xy();
    for (enemy, transform, hitbox, attack) in &mut enemies {
        if collision::circle_intersects_aabb(
            origin,
            combat_config.shockwave_radius,
            hitbox.0.aabb_at(transform.translation.xy()),
        ) {
            if let Some(mut attack) = attack {
                *attack = EnemyAttack::Ready;
            }
            commands
                .entity(enemy)
                .insert(refreshed_stun(combat_config.stun_duration));
        }
    }
    commands.spawn((
        RunScoped,
        ShockwaveVisual {
            origin,
            elapsed: 0.0,
            duration: combat_config.shockwave_animation_duration,
            radius: combat_config.shockwave_radius,
        },
    ));
    shake.trauma = (shake.trauma + shake.trauma_per_stun).clamp(0.0, 1.0);
}

fn tick_stunned_enemies(
    mut commands: Commands,
    time: Res<Time>,
    mut enemies: Query<(Entity, &mut Stunned), With<Enemy>>,
) {
    for (enemy, mut stunned) in &mut enemies {
        stunned.remaining -= time.delta_secs();
        if stunned.remaining <= 0.0 {
            commands.entity(enemy).remove::<Stunned>();
        }
    }
}

fn update_enemies(
    time: Res<Time>,
    game_map: Res<GameMap>,
    animations: Res<EnemyAnimations>,
    combat_config: Option<Res<CombatConfig>>,
    player: Single<(Entity, &Transform, Option<&Health>, Option<&PlayerDying>), With<Player>>,
    mut damage: Option<ResMut<Messages<Damage>>>,
    mut enemies: Query<
        (
            &mut Transform,
            &mut Facing,
            &EnemyCollider,
            Option<&EnemyMovement>,
            Option<&Stunned>,
            Option<&Dying>,
            Option<&mut EnemyAttack>,
            &mut SpritesheetAnimation,
            Option<&mut Sprite>,
        ),
        (With<Enemy>, Without<Player>),
    >,
) {
    let occupancy = collision::Occupancy::terrain_only(&game_map.0);
    let (player_entity, player_transform, player_health, player_dying) = *player;
    let player_position = player_transform.translation.xy();
    let player_is_alive =
        player_dying.is_none() && player_health.is_none_or(|health| health.current > 0.0);
    let (melee_damage, melee_windup, melee_cooldown) =
        combat_config.map_or((20.0, 0.35, 1.0), |config| {
            (
                config.enemy_melee_damage,
                config.enemy_melee_windup,
                config.enemy_melee_cooldown,
            )
        });
    for (
        mut transform,
        mut facing,
        collider,
        movement,
        stunned,
        dying,
        attack,
        mut animation,
        sprite,
    ) in &mut enemies
    {
        if let Some(dying) = dying {
            if animation.animation != dying.animation {
                animation.switch(dying.animation.clone());
            }
            if let Some(mut sprite) = sprite {
                sprite.color = Color::WHITE;
            }
            continue;
        }
        if stunned.is_some_and(|stunned| stunned.remaining > 0.0) {
            if let Some(mut attack) = attack {
                *attack = EnemyAttack::Ready;
            }
            if animation.animation != animations.stunned {
                animation.switch(animations.stunned.clone());
            }
            if let Some(mut sprite) = sprite {
                sprite.color = Color::WHITE;
            }
            continue;
        }
        let Some(movement) = movement else { continue };
        let Some(mut attack) = attack else {
            let position = transform.translation.xy();
            move_enemy_toward_player(
                &mut transform,
                &mut facing,
                &mut animation,
                &animations,
                position,
                player_position,
                *movement,
                collider.0,
                time.delta_secs(),
                &occupancy,
            );
            continue;
        };
        let position = transform.translation.xy();
        let distance = position.distance(player_position);
        if !player_is_alive {
            *attack = EnemyAttack::Ready;
            if animation.animation != animations.idle {
                animation.switch(animations.idle.clone());
            }
            if let Some(mut sprite) = sprite {
                sprite.color = Color::WHITE;
            }
            continue;
        }

        let winding = matches!(*attack, EnemyAttack::WindingUp { .. });
        if let Some(mut sprite) = sprite {
            sprite.color = if winding {
                Color::srgb(1.0, 0.35, 0.35)
            } else {
                Color::WHITE
            };
        }
        match *attack {
            EnemyAttack::Ready if distance <= movement.attack_distance => {
                *attack = EnemyAttack::WindingUp {
                    remaining: melee_windup,
                };
                if animation.animation != animations.idle {
                    animation.switch(animations.idle.clone());
                }
            }
            EnemyAttack::Ready => move_enemy_toward_player(
                &mut transform,
                &mut facing,
                &mut animation,
                &animations,
                position,
                player_position,
                *movement,
                collider.0,
                time.delta_secs(),
                &occupancy,
            ),
            EnemyAttack::WindingUp { remaining } => {
                let remaining = remaining - time.delta_secs();
                if remaining > 0.0 {
                    *attack = EnemyAttack::WindingUp { remaining };
                } else {
                    // The target is checked at impact, so retreating during wind-up is a miss.
                    if position.distance(player_position) <= movement.attack_distance {
                        if let Some(damage) = &mut damage {
                            damage.write(Damage {
                                target: player_entity,
                                amount: melee_damage,
                            });
                        }
                    }
                    *attack = EnemyAttack::Cooldown {
                        remaining: melee_cooldown,
                    };
                }
            }
            EnemyAttack::Cooldown { remaining } => {
                let remaining = remaining - time.delta_secs();
                *attack = if remaining <= 0.0 {
                    EnemyAttack::Ready
                } else {
                    EnemyAttack::Cooldown { remaining }
                };
                if distance > movement.attack_distance {
                    move_enemy_toward_player(
                        &mut transform,
                        &mut facing,
                        &mut animation,
                        &animations,
                        position,
                        player_position,
                        *movement,
                        collider.0,
                        time.delta_secs(),
                        &occupancy,
                    );
                } else if animation.animation != animations.idle {
                    animation.switch(animations.idle.clone());
                }
            }
        }
    }
}

fn move_enemy_toward_player(
    transform: &mut Transform,
    facing: &mut Facing,
    animation: &mut SpritesheetAnimation,
    animations: &EnemyAnimations,
    position: Vec2,
    player_position: Vec2,
    movement: EnemyMovement,
    collider: Collider,
    delta_secs: f32,
    occupancy: &collision::Occupancy<'_>,
) {
    let direction = player_position - position;
    let next_position = enemy_chase_position(
        position,
        player_position,
        movement,
        collider,
        delta_secs,
        occupancy,
    );
    if next_position != position {
        *facing = facing_from_direction(direction, *facing);
        if animation.animation != animations.movement {
            animation.switch(animations.movement.clone());
        }
        transform.translation = next_position.extend(transform.translation.z);
    } else if animation.animation != animations.idle {
        animation.switch(animations.idle.clone());
    }
}

fn draw_enemy_attack_telegraphs(
    combat_config: Res<CombatConfig>,
    enemies: Query<
        (&Transform, &EnemyMovement, &EnemyAttack),
        (With<Enemy>, Without<Dying>, Without<Stunned>),
    >,
    mut gizmos: Gizmos,
) {
    for (transform, movement, attack) in &enemies {
        if let EnemyAttack::WindingUp { remaining } = attack {
            let ratio = (*remaining / combat_config.enemy_melee_windup).clamp(0.0, 1.0);
            gizmos.circle_2d(
                transform.translation.xy(),
                movement.attack_distance * ratio.max(0.12),
                Color::srgb(1.0, 0.12, 0.12),
            );
        }
    }
}

fn spawn_enemy_health_bars(mut commands: Commands, enemies: Query<Entity, Added<Enemy>>) {
    for enemy in &enemies {
        commands.entity(enemy).with_children(|parent| {
            parent.spawn((
                EnemyHealthBar,
                Sprite {
                    color: Color::srgb(0.15, 0.05, 0.05),
                    custom_size: Some(Vec2::new(ENEMY_HEALTH_BAR_WIDTH, ENEMY_HEALTH_BAR_HEIGHT)),
                    ..default()
                },
                Transform::from_translation(ENEMY_HEALTH_BAR_OFFSET.extend(0.1)),
            ));
            parent.spawn((
                EnemyHealthBarFill,
                Sprite {
                    color: Color::srgb(0.2, 0.9, 0.25),
                    custom_size: Some(Vec2::new(ENEMY_HEALTH_BAR_WIDTH, ENEMY_HEALTH_BAR_HEIGHT)),
                    ..default()
                },
                Transform::from_translation(
                    (ENEMY_HEALTH_BAR_OFFSET + Vec2::new(-ENEMY_HEALTH_BAR_WIDTH / 2.0, 0.0))
                        .extend(0.2),
                ),
            ));
        });
    }
}

fn update_enemy_health_bars(
    enemies: Query<(&Health, &Children), (With<Enemy>, Without<Dying>)>,
    mut fills: Query<(&mut Transform, &mut Visibility), With<EnemyHealthBarFill>>,
) {
    for (health, children) in &enemies {
        let ratio = health_bar_ratio(*health);
        for child in children.iter() {
            if let Ok((mut transform, mut visibility)) = fills.get_mut(child) {
                transform.scale.x = ratio;
                *visibility = Visibility::Visible;
            }
        }
    }
}

fn health_bar_ratio(health: Health) -> f32 {
    if health.max <= 0.0 {
        0.0
    } else {
        (health.current / health.max).clamp(0.0, 1.0)
    }
}

/// A small stable mixer avoids a runtime RNG dependency while giving each placed enemy a
/// repeatable per-run roll. The output is uniform enough for the configured gameplay chance.
fn health_drop_roll(seed: u64, placement: PlacementId, chance: f32) -> bool {
    let chance = chance.clamp(0.0, 1.0);
    if chance == 0.0 {
        return false;
    }
    if chance == 1.0 {
        return true;
    }
    let mut value = seed
        ^ (placement.x as i64 as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (placement.y as i64 as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^= value >> 31;
    (value as f64 / u64::MAX as f64) < chance as f64
}

fn next_health_drop_seed(seed: u64) -> u64 {
    let mut next = seed.wrapping_add(0x9e37_79b9_7f4a_7c15);
    next ^= next >> 30;
    next = next.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    next ^ (next >> 27)
}

fn spawn_health_drop(commands: &mut Commands, source: PlacementId, position: Vec3) {
    commands.spawn((
        RunScoped,
        HealthDrop { source },
        HealthDropTrigger,
        Sprite {
            color: Color::srgb(0.2, 0.95, 0.35),
            custom_size: Some(Vec2::splat(28.0)),
            ..default()
        },
        Transform::from_translation(position.with_z(3.0)),
    ));
}

fn begin_enemy_deaths(
    mut commands: Commands,
    animations: Res<EnemyAnimations>,
    health_drop_seed: Res<HealthDropSeed>,
    combat_config: Res<CombatConfig>,
    mut progress: Option<ResMut<RunProgress>>,
    enemies: Query<
        (
            Entity,
            &Health,
            &Transform,
            Option<&Children>,
            Option<&Placement>,
        ),
        (With<Enemy>, Without<Dying>),
    >,
) {
    for (enemy, health, transform, children, placement) in &enemies {
        if health.current <= 0.0 {
            if let (Some(progress), Some(placement)) = (&mut progress, placement) {
                progress.defeated_enemies.insert(placement.0);
            }
            if let Some(placement) = placement
                && health_drop_roll(
                    health_drop_seed.0,
                    placement.0,
                    combat_config.enemy_health_drop_chance,
                )
            {
                spawn_health_drop(&mut commands, placement.0, transform.translation);
            }
            if let Some(children) = children {
                for child in children.iter() {
                    commands.entity(child).insert(Visibility::Hidden);
                }
            }
            commands.entity(enemy).insert(Dying {
                animation: animations.death.clone(),
            });
            commands
                .entity(enemy)
                .remove::<(EnemyMovement, EnemyAttack, Hitbox, Health, Stunned)>();
        }
    }
}

fn finish_enemy_deaths(
    mut commands: Commands,
    enemies: Query<(Entity, &Dying)>,
    mut animation_events: MessageReader<AnimationEvent>,
) {
    for event in animation_events.read() {
        let (AnimationEvent::AnimationEnd { entity, animation }
        | AnimationEvent::AnimationRepetitionEnd {
            entity, animation, ..
        }) = event
        else {
            continue;
        };
        if let Ok((enemy, dying)) = enemies.get(*entity)
            && *animation == dying.animation
        {
            commands.entity(enemy).despawn();
        }
    }
}

fn setup_player_health_hud(mut commands: Commands, needs_spawn: Res<RunNeedsSpawn>) {
    if !needs_spawn.0 {
        return;
    }
    commands.spawn((
        RunScoped,
        PlayerHealthHud,
        Node {
            position_type: PositionType::Absolute,
            top: px(12),
            left: px(12),
            width: px(250),
            flex_direction: FlexDirection::Column,
            row_gap: px(5),
            ..default()
        },
        children![
            (
                Text::new("Health: 100 / 100"),
                PlayerHealthText,
                TextFont {
                    font_size: FontSize::Px(20.0),
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.9, 0.9)),
            ),
            (
                Node {
                    width: percent(100),
                    height: px(18),
                    padding: UiRect::all(px(2)),
                    border: UiRect::all(px(1)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.12, 0.04, 0.04)),
                BorderColor::all(Color::srgb(0.9, 0.32, 0.32)),
                children![(
                    PlayerHealthBarFill,
                    Node {
                        width: percent(100),
                        height: percent(100),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.2, 0.85, 0.3)),
                )],
            ),
        ],
    ));
}

fn setup_instructions(
    mut commands: Commands,
    needs_spawn: Res<RunNeedsSpawn>,
    progress: Res<RunProgress>,
) {
    if !needs_spawn.0 {
        return;
    }
    commands.spawn((
        RunScoped,
        AttackHud,
        Text::new(format_attack_hud(
            AttackMode::Lightning,
            None,
            TeleportCooldown::default(),
            &progress,
        )),
        Node {
            position_type: PositionType::Absolute,
            bottom: px(12),
            left: px(12),
            ..default()
        },
    ));
}

fn pause_game(keyboard: Res<ButtonInput<KeyCode>>, mut game_state: ResMut<NextState<GameState>>) {
    if keyboard.just_pressed(KeyCode::Escape) {
        game_state.set(GameState::Paused);
    }
}

fn toggle_collision_debug(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut collision_debug: ResMut<CollisionDebug>,
) {
    if keyboard.just_pressed(KeyCode::F3) {
        collision_debug.enabled = !collision_debug.enabled;
    }
}

fn draw_collision_debug(
    collision_debug: Res<CollisionDebug>,
    player: Single<(&Transform, &PlayerCollider, &Hitbox), With<Player>>,
    enemies: Query<(&Transform, &EnemyCollider, &Hitbox), With<Enemy>>,
    mut gizmos: Gizmos,
) {
    if collision_debug.enabled {
        let (transform, collider, hitbox) = *player;
        gizmos.rect_2d(
            transform.translation.xy() + hitbox.0.offset,
            hitbox.0.size,
            Color::srgb(0.2, 0.9, 1.0),
        );
        gizmos.rect_2d(
            transform.translation.xy() + collider.0.offset,
            collider.0.size,
            Color::srgb(1.0, 0.0, 1.0),
        );
        for (transform, collider, hitbox) in &enemies {
            gizmos.rect_2d(
                transform.translation.xy() + collider.0.offset,
                collider.0.size,
                Color::srgb(1.0, 0.8, 0.1),
            );
            gizmos.rect_2d(
                transform.translation.xy() + hitbox.0.offset,
                hitbox.0.size,
                Color::srgb(1.0, 0.2, 0.2),
            );
        }
    }
}

fn pause_menu_setup(mut commands: Commands) {
    let button_node = Node {
        width: px(200),
        height: px(65),
        margin: UiRect::all(px(10)),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    };

    commands.spawn((
        DespawnOnExit(GameState::Paused),
        Node {
            width: percent(100),
            height: percent(100),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![(
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::all(px(30)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.1, 0.1, 0.1)),
            children![
                (
                    Text::new("Paused"),
                    TextFont {
                        font_size: FontSize::Px(48.0),
                        ..default()
                    }
                ),
                (
                    Button,
                    button_node.clone(),
                    BackgroundColor(PAUSE_BUTTON_NORMAL),
                    PauseMenuAction::Resume,
                    children![(
                        Text::new("Resume"),
                        TextFont {
                            font_size: FontSize::Px(28.0),
                            ..default()
                        },
                        TextColor(TEXT_COLOR)
                    )]
                ),
                (
                    Button,
                    button_node,
                    BackgroundColor(PAUSE_BUTTON_NORMAL),
                    PauseMenuAction::Quit,
                    children![(
                        Text::new("Quit"),
                        TextFont {
                            font_size: FontSize::Px(28.0),
                            ..default()
                        },
                        TextColor(TEXT_COLOR)
                    )]
                )
            ]
        )],
    ));
}

fn pause_menu_button_system(
    mut interaction_query: Query<
        (&Interaction, &mut BackgroundColor),
        (Changed<Interaction>, With<Button>, With<PauseMenuAction>),
    >,
) {
    for (interaction, mut background_color) in &mut interaction_query {
        *background_color = match interaction {
            Interaction::None => PAUSE_BUTTON_NORMAL.into(),
            Interaction::Hovered => PAUSE_BUTTON_HOVERED.into(),
            Interaction::Pressed => PAUSE_BUTTON_PRESSED.into(),
        };
    }
}

fn pause_menu_action(
    interaction_query: Query<
        (&Interaction, &PauseMenuAction),
        (Changed<Interaction>, With<Button>),
    >,
    mut game_state: ResMut<NextState<GameState>>,
    mut app_exit_writer: MessageWriter<AppExit>,
) {
    for (interaction, action) in &interaction_query {
        if *interaction == Interaction::Pressed {
            match action {
                PauseMenuAction::Resume => game_state.set(GameState::Game),
                PauseMenuAction::Quit => {
                    app_exit_writer.write(AppExit::Success);
                }
            }
        }
    }
}

fn setup_camera_shake(
    mut commands: Commands,
    cameras: Query<(Entity, &Transform), (With<Camera2d>, Without<UnshakenCameraTransform>)>,
    needs_spawn: Res<RunNeedsSpawn>,
) {
    if !needs_spawn.0 {
        return;
    }
    for (camera, transform) in &cameras {
        commands
            .entity(camera)
            .insert(UnshakenCameraTransform(*transform));
    }
}

/// Restore the pre-shake camera transform even while gameplay is paused.
fn restore_camera_transform(
    mut cameras: Query<(&UnshakenCameraTransform, &mut Transform), With<Camera2d>>,
) {
    for (unshaken, mut transform) in &mut cameras {
        *transform = unshaken.0;
    }
}

fn shake_noise(value: f32) -> f32 {
    // Smooth deterministic noise is enough for a short, readable impact shake.
    (value.sin() + (value * 0.73 + 1.7).sin() * 0.5) / 1.5
}

fn shake_camera(
    time: Res<Time>,
    mut shake: ResMut<CameraShakeConfig>,
    mut cameras: Query<(&mut Transform, &mut UnshakenCameraTransform), With<Camera2d>>,
) {
    shake.trauma = shake.trauma.clamp(0.0, 1.0);
    let amount = shake.trauma.powf(shake.exponent);
    let t = time.elapsed_secs() * shake.noise_speed;
    for (mut transform, mut unshaken) in &mut cameras {
        // Update the stored normal transform after the tracking system has run.
        unshaken.0 = *transform;
        transform.translation += Vec3::new(
            shake_noise(t + 100.0) * amount * shake.max_translation,
            shake_noise(t + 200.0) * amount * shake.max_translation,
            0.0,
        );
        transform.rotate_z(shake_noise(t) * amount * shake.max_rotation);
    }
    shake.trauma = (shake.trauma - shake.trauma_decay_per_second * time.delta_secs()).max(0.0);
}

/// Update the camera position by tracking the player.
fn update_camera(
    mut camera: Single<&mut Transform, (With<Camera2d>, Without<Player>)>,
    player: Single<&Transform, (With<Player>, Without<Camera2d>)>,
    time: Res<Time>,
) {
    let Vec3 { x, y, .. } = player.translation;
    let direction = Vec3::new(x, y, camera.translation.z);

    // Applies a smooth effect to camera movement using stable interpolation
    // between the camera position and the player position on the x and y axes.
    camera
        .translation
        .smooth_nudge(&direction, CAMERA_DECAY_RATE, time.delta_secs());
}

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
enum Facing {
    Right = 0,
    Left = 1,
    Up = 2,
    Down = 3,
}

impl Facing {
    const ALL: [Self; 4] = [Self::Right, Self::Left, Self::Up, Self::Down];

    fn animation_index(self) -> usize {
        self as usize
    }

    fn unit_vector(self) -> Vec2 {
        match self {
            Self::Right => Vec2::X,
            Self::Left => Vec2::NEG_X,
            Self::Up => Vec2::Y,
            Self::Down => Vec2::NEG_Y,
        }
    }
}

fn facing_from_direction(direction: Vec2, current: Facing) -> Facing {
    if direction == Vec2::ZERO {
        return current;
    }

    if direction.x.abs() >= direction.y.abs() {
        if direction.x < 0.0 {
            Facing::Left
        } else {
            Facing::Right
        }
    } else if direction.y < 0.0 {
        Facing::Down
    } else {
        Facing::Up
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AttackKind {
    Lightning,
    Projectile,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
struct AttackRequest {
    kind: AttackKind,
    origin: Vec2,
    target: Vec2,
    direction: Vec2,
}

#[derive(Component, Clone, Debug)]
struct PlayerAction {
    request: AttackRequest,
    animation: Handle<Animation>,
    fired: bool,
}

#[allow(dead_code)]
#[derive(Message, Clone, Debug)]
struct AttackFired {
    entity: Entity,
    request: AttackRequest,
}

fn resolve_attack_kind(
    mode: AttackMode,
    origin: Vec2,
    target: Vec2,
    lightning_range: f32,
) -> AttackKind {
    match mode {
        AttackMode::Lightning => AttackKind::Lightning,
        AttackMode::Projectile => AttackKind::Projectile,
        AttackMode::Auto => {
            if origin.distance(target) <= lightning_range {
                AttackKind::Lightning
            } else {
                AttackKind::Projectile
            }
        }
    }
}

fn attack_direction_and_facing(
    origin: Vec2,
    target: Vec2,
    current_facing: Facing,
) -> (Vec2, Facing) {
    let aim = target - origin;
    if aim == Vec2::ZERO {
        (current_facing.unit_vector(), current_facing)
    } else {
        let facing = facing_from_direction(aim, current_facing);
        (aim.normalize(), facing)
    }
}

fn format_attack_hud(
    mode: AttackMode,
    rejection: Option<RejectionReason>,
    teleport: TeleportCooldown,
    progress: &RunProgress,
) -> String {
    let mut controls = String::from("Move: WASD/arrows | Fire: left click");
    if progress.unlocked_skills.contains(&Skill::Stun) {
        controls.push_str(" | Stun: Shift/middle click");
    }
    if progress.unlocked_skills.contains(&Skill::Teleport) {
        let status = match teleport.rejection {
            Some(TeleportRejection::Busy) => "rejected: firing".to_owned(),
            Some(TeleportRejection::Cooldown) => "rejected: cooling down".to_owned(),
            Some(TeleportRejection::InvalidTerrain) => "rejected: blocked terrain".to_owned(),
            None if teleport.ready() => "ready".to_owned(),
            None => format!("{:.1}s", teleport.remaining),
        };
        controls.push_str(&format!(" | Right click: teleport ({status})"));
    }
    controls.push_str(" | Modes: 1 Lightning");
    if progress.unlocked_skills.contains(&Skill::Projectile) {
        controls.push_str(", 2 Projectile, 3 Auto");
    }
    let attack_status = match rejection {
        Some(RejectionReason::OutOfRange) => " | Lightning rejected: target out of range",
        Some(RejectionReason::Obstructed) => " | Lightning rejected: terrain blocks the path",
        None => "",
    };
    format!(
        "{controls} | Selected: {}{} | Pause: Esc | Debug: F3",
        mode.label(),
        attack_status
    )
}

fn validate_attack_request(
    kind: AttackKind,
    origin: Vec2,
    target: Vec2,
    combat_config: CombatConfig,
    map: &map::MapData,
) -> Result<(), RejectionReason> {
    if kind == AttackKind::Lightning {
        if origin.distance(target) > combat_config.lightning_range {
            return Err(RejectionReason::OutOfRange);
        }
        if collision::terrain_blocks_segment(origin, target, map) {
            return Err(RejectionReason::Obstructed);
        }
    }

    Ok(())
}

fn nearest_segment_hit(
    start: Vec2,
    end: Vec2,
    hitboxes: impl IntoIterator<Item = (Entity, Vec2, Collider)>,
) -> Option<Entity> {
    nearest_segment_hit_with_time(start, end, hitboxes).map(|(entity, _)| entity)
}

fn nearest_segment_hit_with_time(
    start: Vec2,
    end: Vec2,
    hitboxes: impl IntoIterator<Item = (Entity, Vec2, Collider)>,
) -> Option<(Entity, f32)> {
    hitboxes
        .into_iter()
        .filter_map(|(entity, position, collider)| {
            collision::segment_first_aabb_intersection(start, end, collider.aabb_at(position))
                .map(|distance| (entity, distance))
        })
        .min_by(|(entity_a, distance_a), (entity_b, distance_b)| {
            distance_a
                .total_cmp(distance_b)
                .then_with(|| entity_a.index().cmp(&entity_b.index()))
        })
}

fn swept_circle_aabb_intersection(
    start: Vec2,
    end: Vec2,
    radius: f32,
    aabb: collision::Aabb,
) -> Option<f32> {
    collision::segment_first_aabb_intersection(
        start,
        end,
        collision::Aabb {
            min: aabb.min - Vec2::splat(radius),
            max: aabb.max + Vec2::splat(radius),
        },
    )
}

fn projectile_terrain_hit_time(
    start: Vec2,
    end: Vec2,
    radius: f32,
    map: &map::MapData,
) -> Option<f32> {
    let min = start.min(end) - Vec2::splat(radius);
    let max = start.max(end) + Vec2::splat(radius);
    let (min_x, min_y) = collision::world_to_tile(min);
    let (max_x, max_y) = collision::world_to_tile(max);

    let mut earliest: Option<f32> = None;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            if map.tile_at(x, y) == Some(map::TerrainTile::Floor) {
                continue;
            }
            if let Some(time) =
                swept_circle_aabb_intersection(start, end, radius, collision::Aabb::tile(x, y))
            {
                earliest = Some(earliest.map_or(time, |current| current.min(time)));
            }
        }
    }
    earliest
}

fn projectile_hitbox_hit_time(
    start: Vec2,
    end: Vec2,
    radius: f32,
    position: Vec2,
    collider: Collider,
) -> Option<f32> {
    swept_circle_aabb_intersection(start, end, radius, collider.aabb_at(position))
}

fn cursor_to_world(
    cursor_position: Option<Vec2>,
    camera: &Camera,
    camera_transform: &GlobalTransform,
) -> Option<Vec2> {
    camera
        .viewport_to_world_2d(camera_transform, cursor_position?)
        .ok()
}

fn update_cursor_world_position(
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<(&Camera, &GlobalTransform), With<Camera2d>>,
    mut cursor_world: ResMut<CursorWorld>,
) {
    let (camera, camera_transform) = *camera;
    cursor_world.0 = cursor_to_world(window.cursor_position(), camera, camera_transform);
}

fn select_attack_mode(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut selected_mode: ResMut<SelectedAttackMode>,
    progress: Option<Res<RunProgress>>,
) {
    if keyboard.just_pressed(KeyCode::Digit1) {
        selected_mode.0 = AttackMode::Lightning;
    } else if keyboard.just_pressed(KeyCode::Digit2)
        && progress
            .as_ref()
            .is_none_or(|progress| progress.unlocked_skills.contains(&Skill::Projectile))
    {
        selected_mode.0 = AttackMode::Projectile;
    } else if keyboard.just_pressed(KeyCode::Digit3)
        && progress
            .as_ref()
            .is_none_or(|progress| progress.unlocked_skills.contains(&Skill::Projectile))
    {
        selected_mode.0 = AttackMode::Auto;
    }
}

fn update_attack_hud(
    selected_mode: Res<SelectedAttackMode>,
    feedback: Res<AttackFeedback>,
    teleport: Res<TeleportCooldown>,
    progress: Res<RunProgress>,
    mut hud: Single<&mut Text, With<AttackHud>>,
) {
    if selected_mode.is_changed()
        || feedback.is_changed()
        || teleport.is_changed()
        || progress.is_changed()
    {
        **hud = Text::new(format_attack_hud(
            selected_mode.0,
            feedback.rejection.map(|rejection| rejection.reason),
            *teleport,
            &progress,
        ));
    }
}

/// Resolves a clicked foot-collider center to the player transform position.
fn teleport_destination(
    foot_center: Vec2,
    collider: Collider,
    occupancy: &collision::Occupancy<'_>,
) -> Option<Vec2> {
    let player_position = foot_center - collider.offset;
    collision::can_occupy(player_position, collider, occupancy).then_some(player_position)
}

fn advance_teleport_cooldown(teleport: &mut TeleportCooldown, delta_secs: f32) {
    teleport.remaining = (teleport.remaining - delta_secs).max(0.0);
    teleport.rejection_remaining -= delta_secs;
    if teleport.rejection_remaining <= 0.0 {
        teleport.rejection = None;
    }
}

fn tick_teleport_cooldown(time: Res<Time>, mut teleport: ResMut<TeleportCooldown>) {
    advance_teleport_cooldown(&mut teleport, time.delta_secs());
}

fn reject_teleport(teleport: &mut TeleportCooldown, rejection: TeleportRejection) {
    teleport.rejection = Some(rejection);
    teleport.rejection_remaining = 0.45;
}

fn move_with_collision(
    transform: &mut Transform,
    collider: &PlayerCollider,
    movement: Vec2,
    map: &map::MapData,
) {
    let occupancy = collision::Occupancy::terrain_only(map);
    let next_position = collision::move_axis_separated(
        transform.translation.xy(),
        movement,
        collider.0,
        &occupancy,
    );
    transform.translation = next_position.extend(transform.translation.z);
}

fn control_character(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut commands: Commands,
    character: Single<(
        Entity,
        &mut SpritesheetAnimation,
        &mut Transform,
        &mut Facing,
        &PlayerCollider,
        Option<&PlayerAction>,
        Option<&Health>,
        Option<&PlayerDying>,
    )>,
    game_map: Res<GameMap>,
    my_animations: Res<PlayerAnimations>,
    selected_mode: Res<SelectedAttackMode>,
    progress: Option<Res<RunProgress>>,
    combat_config: Res<CombatConfig>,
    cursor_world: Res<CursorWorld>,
    mut feedback: ResMut<AttackFeedback>,
    mut teleport: ResMut<TeleportCooldown>,
) {
    let (entity, mut animation, mut transform, mut facing, collider, action, health, dying) =
        character.into_inner();
    if dying.is_some() || health.is_some_and(|health| health.current <= 0.0) {
        return;
    }

    if action.is_some() {
        if progress
            .as_ref()
            .is_none_or(|progress| progress.unlocked_skills.contains(&Skill::Teleport))
            && mouse.just_pressed(MouseButton::Right)
        {
            reject_teleport(&mut teleport, TeleportRejection::Busy);
        }
        return;
    }

    // `cast_stun` runs before this system. Stun wins simultaneous idle input,
    // so a Shift/middle-click cannot also teleport, fire, or move this frame.
    if progress
        .as_ref()
        .is_none_or(|progress| progress.unlocked_skills.contains(&Skill::Stun))
        && stun_requested(&keyboard, &mouse)
    {
        return;
    }

    if progress
        .as_ref()
        .is_none_or(|progress| progress.unlocked_skills.contains(&Skill::Teleport))
        && mouse.just_pressed(MouseButton::Right)
    {
        if !teleport.ready() {
            reject_teleport(&mut teleport, TeleportRejection::Cooldown);
        } else if let Some(target) = cursor_world.0 {
            let occupancy = collision::Occupancy::terrain_only(&game_map.0);
            if let Some(destination) = teleport_destination(target, collider.0, &occupancy) {
                transform.translation = destination.extend(transform.translation.z);
                teleport.remaining = combat_config.teleport_cooldown;
                teleport.rejection = None;
            } else {
                reject_teleport(&mut teleport, TeleportRejection::InvalidTerrain);
            }
        }
        return;
    }

    if mouse.just_pressed(MouseButton::Left)
        && let Some(target) = cursor_world.0
    {
        let origin = transform.translation.xy();
        let kind = resolve_attack_kind(
            selected_mode.0,
            origin,
            target,
            combat_config.lightning_range,
        );
        if let Err(reason) =
            validate_attack_request(kind, origin, target, *combat_config, &game_map.0)
        {
            feedback.rejection = Some(AttackRejection {
                target,
                remaining: 0.45,
                reason,
            });
            return;
        }

        let (direction, aim_facing) = attack_direction_and_facing(origin, target, *facing);
        *facing = aim_facing;

        let shoot_animation = my_animations.shoot_for(*facing).clone();
        animation.switch(shoot_animation.clone());

        commands.entity(entity).insert(PlayerAction {
            request: AttackRequest {
                kind,
                origin,
                target,
                direction,
            },
            animation: shoot_animation,
            fired: false,
        });
        return;
    }

    let mut direction = Vec2::ZERO;

    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        direction.y += 1.;
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        direction.y -= 1.;
    }
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        direction.x -= 1.;
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        direction.x += 1.;
    }

    if direction != Vec2::ZERO {
        *facing = facing_from_direction(direction, *facing);

        let movement_animation = my_animations.movement_for(*facing);
        if animation.animation != *movement_animation {
            animation.switch(movement_animation.clone());
        }

        let move_delta = direction.normalize() * PLAYER_SPEED * time.delta_secs();
        move_with_collision(&mut transform, collider, move_delta, &game_map.0);
    } else {
        let idle_animation = my_animations.idle_for(*facing);
        if animation.animation != *idle_animation {
            animation.switch(idle_animation.clone());
        }
    }
}

fn trigger_attack_fire_event(
    character: Single<
        (
            Entity,
            &SpritesheetAnimation,
            Option<&mut PlayerAction>,
            Option<&Health>,
            Option<&PlayerDying>,
        ),
        With<Player>,
    >,
    mut attack_fired: MessageWriter<AttackFired>,
) {
    let (entity, sprite_animation, action, health, dying) = character.into_inner();
    if dying.is_some() || health.is_some_and(|health| health.current <= 0.0) {
        return;
    }
    let Some(mut action) = action else {
        return;
    };
    if !action.fired
        && sprite_animation.animation == action.animation
        && sprite_animation.progress.frame >= PLAYER_SHOOT_FIRE_FRAME
    {
        attack_fired.write(AttackFired {
            entity,
            request: action.request.clone(),
        });
        action.fired = true;
    }
}

fn handle_lightning_attacks(
    mut commands: Commands,
    combat_config: Res<CombatConfig>,
    mut attack_fired: MessageReader<AttackFired>,
    mut damage: MessageWriter<Damage>,
    players: Query<(&Health, Option<&PlayerDying>), With<Player>>,
    hitboxes: Query<(Entity, &Transform, &Hitbox), With<Health>>,
) {
    for fired in attack_fired.read() {
        if fired.request.kind != AttackKind::Lightning
            || !matches!(
                players.get(fired.entity),
                Ok((health, None)) if health.current > 0.0
            )
        {
            continue;
        }
        commands.spawn((
            RunScoped,
            LightningVisual {
                start: fired.request.origin,
                end: fired.request.target,
                remaining: combat_config.lightning_visible_lifetime,
            },
            Transform::default(),
        ));
        if let Some(target) = nearest_segment_hit(
            fired.request.origin,
            fired.request.target,
            hitboxes
                .iter()
                .filter(|(entity, _, _)| *entity != fired.entity)
                .map(|(entity, transform, hitbox)| (entity, transform.translation.xy(), hitbox.0)),
        ) {
            damage.write(Damage {
                target,
                amount: combat_config.lightning_damage,
            });
        }
    }
}

fn spawn_projectiles(
    mut commands: Commands,
    combat_config: Res<CombatConfig>,
    mut attack_fired: MessageReader<AttackFired>,
    players: Query<(&Health, Option<&PlayerDying>), With<Player>>,
) {
    for fired in attack_fired.read() {
        if fired.request.kind != AttackKind::Projectile
            || !matches!(
                players.get(fired.entity),
                Ok((health, None)) if health.current > 0.0
            )
        {
            continue;
        }
        commands.spawn((
            RunScoped,
            Projectile {
                owner: fired.entity,
                faction: Faction::Player,
                direction: fired.request.direction.normalize_or_zero(),
                speed: combat_config.projectile_speed,
                radius: combat_config.projectile_collision_radius,
                damage: combat_config.projectile_damage,
                remaining_lifetime: combat_config.projectile_maximum_lifetime,
            },
            Sprite {
                color: Color::srgb(1.0, 0.85, 0.2),
                custom_size: Some(Vec2::splat(combat_config.projectile_collision_radius * 2.0)),
                ..default()
            },
            Transform::from_xyz(fired.request.origin.x, fired.request.origin.y, 2.5),
        ));
    }
}

fn move_projectiles(
    mut commands: Commands,
    time: Res<Time>,
    game_map: Res<GameMap>,
    mut damage: MessageWriter<Damage>,
    mut projectiles: Query<(Entity, &mut Projectile, &mut Transform)>,
    hitboxes: Query<
        (Entity, &Transform, &Hitbox, Option<&Faction>),
        (With<Health>, Without<Projectile>),
    >,
) {
    for (projectile_entity, mut projectile, mut transform) in &mut projectiles {
        projectile.remaining_lifetime -= time.delta_secs();
        if projectile.remaining_lifetime <= 0.0 {
            commands.entity(projectile_entity).despawn();
            continue;
        }

        let start = transform.translation.xy();
        let movement = projectile.direction * projectile.speed * time.delta_secs();
        let end = start + movement;

        let terrain_hit = projectile_terrain_hit_time(start, end, projectile.radius, &game_map.0);
        let entity_hit = hitboxes
            .iter()
            .filter(|(entity, _, _, faction)| {
                *entity != projectile.owner
                    && faction.is_none_or(|faction| *faction != projectile.faction)
            })
            .filter_map(|(entity, target_transform, hitbox, _)| {
                projectile_hitbox_hit_time(
                    start,
                    end,
                    projectile.radius,
                    target_transform.translation.xy(),
                    hitbox.0,
                )
                .map(|hit_time| (entity, hit_time))
            })
            .min_by(|(entity_a, time_a), (entity_b, time_b)| {
                time_a
                    .total_cmp(time_b)
                    .then_with(|| entity_a.index().cmp(&entity_b.index()))
            });

        match (terrain_hit, entity_hit) {
            (Some(terrain_time), Some((target, entity_time))) if entity_time < terrain_time => {
                damage.write(Damage {
                    target,
                    amount: projectile.damage,
                });
                commands.entity(projectile_entity).despawn();
            }
            (None, Some((target, _))) => {
                damage.write(Damage {
                    target,
                    amount: projectile.damage,
                });
                commands.entity(projectile_entity).despawn();
            }
            (Some(_), _) => {
                commands.entity(projectile_entity).despawn();
            }
            (None, None) => {
                transform.translation = end.extend(transform.translation.z);
            }
        }
    }
}

fn default_player_health(combat_config: &CombatConfig) -> Health {
    Health {
        current: combat_config.player_maximum_health,
        max: combat_config.player_maximum_health,
    }
}

fn apply_damage(
    mut commands: Commands,
    mut damage: MessageReader<Damage>,
    mut applied: Option<ResMut<Messages<DamageApplied>>>,
    mut health: Query<&mut Health>,
    players: Query<(), With<Player>>,
    invulnerable: Query<(), With<Invulnerable>>,
    combat_config: Option<Res<CombatConfig>>,
) {
    // Components inserted through Commands are deferred, so retain accepted player targets here.
    let mut accepted_players = HashSet::new();
    for damage in damage.read() {
        if damage.amount <= 0.0 || invulnerable.get(damage.target).is_ok() {
            continue;
        }
        let is_player = players.get(damage.target).is_ok();
        if is_player && !accepted_players.insert(damage.target) {
            continue;
        }
        let Ok(mut health) = health.get_mut(damage.target) else {
            continue;
        };
        let before = health.current;
        health.current = (health.current - damage.amount).clamp(0.0, health.max);
        let applied_amount = before - health.current;
        if applied_amount <= 0.0 {
            continue;
        }
        if let Some(applied) = &mut applied {
            applied.write(DamageApplied {
                target: damage.target,
                amount: applied_amount,
            });
        }
        if is_player {
            commands.entity(damage.target).insert((
                Invulnerable {
                    remaining: combat_config
                        .as_deref()
                        .unwrap_or(&CombatConfig::default())
                        .player_invulnerability_duration,
                },
                PlayerHitFlash,
            ));
        }
    }
}

fn tick_player_invulnerability(
    mut commands: Commands,
    time: Res<Time>,
    mut players: Query<(Entity, &mut Invulnerable, Option<&mut Sprite>), With<Player>>,
) {
    for (entity, mut invulnerable, sprite) in &mut players {
        invulnerable.remaining -= time.delta_secs();
        if invulnerable.remaining <= 0.0 {
            commands
                .entity(entity)
                .remove::<(Invulnerable, PlayerHitFlash)>();
            if let Some(mut sprite) = sprite {
                sprite.color = Color::WHITE;
            }
        } else if let Some(mut sprite) = sprite {
            sprite.color = Color::srgb(1.0, 0.45, 0.45);
        }
    }
}

fn begin_player_death(
    mut commands: Commands,
    config: Res<CombatConfig>,
    mut players: Query<
        (Entity, &Health, Option<&mut Sprite>),
        (With<Player>, Without<PlayerDying>),
    >,
    mut feedback: ResMut<AttackFeedback>,
    mut teleport: ResMut<TeleportCooldown>,
    mut shake: ResMut<CameraShakeConfig>,
    mut attack_fired: ResMut<Messages<AttackFired>>,
    mut damage: ResMut<Messages<Damage>>,
) {
    for (entity, health, sprite) in &mut players {
        if health.current > 0.0 {
            continue;
        }
        commands.entity(entity).insert(PlayerDying {
            remaining: config.player_death_presentation_duration,
        });
        commands
            .entity(entity)
            .remove::<(PlayerAction, Hitbox, Invulnerable)>();
        if let Some(mut sprite) = sprite {
            sprite.color = Color::srgb(0.35, 0.08, 0.08);
        }
        *feedback = AttackFeedback::default();
        *teleport = TeleportCooldown::default();
        attack_fired.clear();
        damage.clear();
        shake.trauma = 0.0;
    }
}

fn tick_player_death(
    time: Res<Time>,
    mut players: Query<&mut PlayerDying, With<Player>>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    for mut dying in &mut players {
        dying.remaining -= time.delta_secs();
        if dying.remaining <= 0.0 {
            next_state.set(GameState::GameOver);
        }
    }
}

fn setup_game_over_ui(mut commands: Commands) {
    commands.spawn((
        DespawnOnExit(GameState::GameOver),
        Node {
            width: percent(100),
            height: percent(100),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            flex_direction: FlexDirection::Column,
            row_gap: px(16),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.72)),
        children![
            (
                Text::new("Game Over"),
                TextFont {
                    font_size: FontSize::Px(56.0),
                    ..default()
                },
                TextColor(Color::WHITE)
            ),
            (
                Button,
                GameOverAction::Continue,
                Node {
                    padding: UiRect::all(px(14)),
                    ..default()
                },
                BackgroundColor(PAUSE_BUTTON_NORMAL),
                children![(
                    Text::new("Continue from Last Checkpoint"),
                    TextFont {
                        font_size: FontSize::Px(24.0),
                        ..default()
                    },
                    TextColor(Color::WHITE)
                )]
            ),
            (
                Button,
                GameOverAction::MainMenu,
                Node {
                    padding: UiRect::all(px(14)),
                    ..default()
                },
                BackgroundColor(PAUSE_BUTTON_NORMAL),
                children![(
                    Text::new("Main Menu"),
                    TextFont {
                        font_size: FontSize::Px(24.0),
                        ..default()
                    },
                    TextColor(Color::WHITE)
                )]
            ),
        ],
    ));
}

fn game_over_action(
    buttons: Query<(&Interaction, &GameOverAction), (Changed<Interaction>, With<Button>)>,
    mut restart: ResMut<RestartRequest>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    for (interaction, action) in &buttons {
        if *interaction == Interaction::Pressed {
            restart.0 = match action {
                GameOverAction::Continue => RestartIntent::ContinueCheckpoint,
                GameOverAction::MainMenu => RestartIntent::MainMenu,
            };
            next_state.set(GameState::Restarting);
        }
    }
}

fn player_health_ratio(health: Health) -> f32 {
    if !health.current.is_finite()
        || !health.max.is_finite()
        || health.current < 0.0
        || health.max <= 0.0
    {
        return 0.0;
    }
    (health.current / health.max).clamp(0.0, 1.0)
}

fn player_health_text(health: Health) -> String {
    if !health.current.is_finite() || !health.max.is_finite() || health.max <= 0.0 {
        return "Health: 0 / 0".to_owned();
    }
    format!(
        "Health: {:.0} / {:.0}",
        health.current.clamp(0.0, health.max),
        health.max
    )
}

fn player_health_fill_width(health: Health) -> Val {
    Val::Percent(player_health_ratio(health) * 100.0)
}

fn update_player_health_hud(
    player: Single<&Health, With<Player>>,
    mut text: Single<&mut Text, With<PlayerHealthText>>,
    mut fill: Single<&mut Node, With<PlayerHealthBarFill>>,
) {
    let health = **player;
    text.0 = player_health_text(health);
    fill.width = player_health_fill_width(health);
}

fn tick_lightning_visuals(
    mut commands: Commands,
    time: Res<Time>,
    mut visuals: Query<(Entity, &mut LightningVisual)>,
) {
    for (entity, mut visual) in &mut visuals {
        visual.remaining -= time.delta_secs();
        if visual.remaining <= 0.0 {
            commands.entity(entity).despawn();
        }
    }
}

fn tick_shockwaves(
    mut commands: Commands,
    time: Res<Time>,
    mut shockwaves: Query<(Entity, &mut ShockwaveVisual)>,
) {
    for (entity, mut shockwave) in &mut shockwaves {
        shockwave.elapsed += time.delta_secs();
        if shockwave.elapsed >= shockwave.duration {
            commands.entity(entity).despawn();
        }
    }
}

fn tick_attack_feedback(time: Res<Time>, mut feedback: ResMut<AttackFeedback>) {
    if let Some(mut rejection) = feedback.rejection {
        rejection.remaining -= time.delta_secs();
        feedback.rejection = (rejection.remaining > 0.0).then_some(rejection);
    }
}

fn draw_lightning_range_feedback(
    selected_mode: Res<SelectedAttackMode>,
    progress: Res<RunProgress>,
    combat_config: Res<CombatConfig>,
    feedback: Res<AttackFeedback>,
    player: Single<&Transform, With<Player>>,
    mut gizmos: Gizmos,
) {
    if selected_mode.0 == AttackMode::Lightning
        || (selected_mode.0 == AttackMode::Auto
            && progress.unlocked_skills.contains(&Skill::Projectile))
    {
        gizmos.circle_2d(
            player.translation.xy(),
            combat_config.lightning_range,
            Color::srgba(0.25, 0.65, 1.0, 0.65),
        );
    }

    if let Some(rejection) = feedback.rejection {
        gizmos.circle_2d(rejection.target, 14.0, Color::srgba(1.0, 0.0, 0.0, 0.9));
    }
}

fn draw_shockwaves(mut gizmos: Gizmos, shockwaves: Query<&ShockwaveVisual>) {
    for shockwave in &shockwaves {
        let progress = (shockwave.elapsed / shockwave.duration).clamp(0.0, 1.0);
        let alpha = 1.0 - progress;
        for ring in 0..4 {
            let offset = ring as f32 / 4.0;
            let radius = (progress + offset * 0.22).min(1.0) * shockwave.radius;
            gizmos.circle_2d(
                shockwave.origin,
                radius,
                Color::srgba(0.65, 0.35, 1.0, alpha * (1.0 - offset * 0.5)),
            );
        }
    }
}

fn draw_lightning_visuals(mut gizmos: Gizmos, visuals: Query<&LightningVisual>) {
    for visual in &visuals {
        let delta = visual.end - visual.start;
        let normal = if delta == Vec2::ZERO {
            Vec2::Y
        } else {
            Vec2::new(-delta.y, delta.x).normalize()
        };
        let mut previous = visual.start;
        for i in 1..=6 {
            let t = i as f32 / 6.0;
            let mut point = visual.start.lerp(visual.end, t);
            if i != 6 {
                let offset = if i % 2 == 0 { -8.0 } else { 8.0 };
                point += normal * offset;
            }
            gizmos.line_2d(previous, point, Color::srgba(0.7, 0.95, 1.0, 1.0));
            previous = point;
        }
    }
}

fn handle_attack_animation_events(
    mut commands: Commands,
    character: Single<
        (
            Entity,
            &mut SpritesheetAnimation,
            &Facing,
            Option<&PlayerAction>,
        ),
        With<Player>,
    >,
    my_animations: Res<PlayerAnimations>,
    mut animation_events: MessageReader<AnimationEvent>,
) {
    let (player_entity, mut sprite_animation, facing, action) = character.into_inner();
    let Some(action) = action else {
        return;
    };

    let mut finished = false;
    for event in animation_events.read() {
        match event {
            AnimationEvent::AnimationRepetitionEnd {
                entity, animation, ..
            } if *entity == player_entity && *animation == action.animation => {
                finished = true;
            }
            AnimationEvent::AnimationEnd { entity, animation }
                if *entity == player_entity && *animation == action.animation =>
            {
                finished = true;
            }
            _ => {}
        }
    }

    if finished {
        commands.entity(player_entity).remove::<PlayerAction>();
        sprite_animation.switch(my_animations.idle_for(*facing).clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;

    #[test]
    fn dominant_axis_selects_cardinal_facing() {
        assert_eq!(
            facing_from_direction(Vec2::new(5.0, 2.0), Facing::Left),
            Facing::Right
        );
        assert_eq!(
            facing_from_direction(Vec2::new(-5.0, 2.0), Facing::Right),
            Facing::Left
        );
        assert_eq!(
            facing_from_direction(Vec2::new(2.0, 5.0), Facing::Down),
            Facing::Up
        );
        assert_eq!(
            facing_from_direction(Vec2::new(2.0, -5.0), Facing::Up),
            Facing::Down
        );
    }

    #[test]
    fn horizontal_wins_exact_diagonal_ties() {
        assert_eq!(
            facing_from_direction(Vec2::new(1.0, 1.0), Facing::Down),
            Facing::Right
        );
        assert_eq!(
            facing_from_direction(Vec2::new(-1.0, -1.0), Facing::Up),
            Facing::Left
        );
    }

    #[test]
    fn zero_direction_retains_current_facing() {
        for facing in Facing::ALL {
            assert_eq!(facing_from_direction(Vec2::ZERO, facing), facing);
        }
    }

    #[test]
    fn aim_direction_selects_all_four_facings() {
        assert_eq!(
            attack_direction_and_facing(Vec2::ZERO, Vec2::X, Facing::Left).1,
            Facing::Right
        );
        assert_eq!(
            attack_direction_and_facing(Vec2::ZERO, Vec2::NEG_X, Facing::Right).1,
            Facing::Left
        );
        assert_eq!(
            attack_direction_and_facing(Vec2::ZERO, Vec2::Y, Facing::Down).1,
            Facing::Up
        );
        assert_eq!(
            attack_direction_and_facing(Vec2::ZERO, Vec2::NEG_Y, Facing::Up).1,
            Facing::Down
        );
    }

    #[test]
    fn zero_length_aim_retains_facing_and_uses_facing_unit_vector() {
        for facing in Facing::ALL {
            let (direction, resolved_facing) =
                attack_direction_and_facing(Vec2::ZERO, Vec2::ZERO, facing);
            assert_eq!(resolved_facing, facing);
            assert_eq!(direction, facing.unit_vector());
        }
    }

    #[test]
    fn auto_attack_selects_lightning_at_or_inside_range() {
        assert_eq!(
            resolve_attack_kind(AttackMode::Auto, Vec2::ZERO, Vec2::new(219.9, 0.0), 220.0),
            AttackKind::Lightning
        );
        assert_eq!(
            resolve_attack_kind(AttackMode::Auto, Vec2::ZERO, Vec2::new(220.0, 0.0), 220.0),
            AttackKind::Lightning
        );
        assert_eq!(
            resolve_attack_kind(AttackMode::Auto, Vec2::ZERO, Vec2::new(220.1, 0.0), 220.0),
            AttackKind::Projectile
        );
    }

    #[test]
    fn manual_attack_modes_select_requested_kind() {
        assert_eq!(
            resolve_attack_kind(
                AttackMode::Lightning,
                Vec2::ZERO,
                Vec2::new(999.0, 0.0),
                220.0
            ),
            AttackKind::Lightning
        );
        assert_eq!(
            resolve_attack_kind(
                AttackMode::Projectile,
                Vec2::ZERO,
                Vec2::new(1.0, 0.0),
                220.0
            ),
            AttackKind::Projectile
        );
    }

    #[test]
    fn player_sheet_layout_constants_match_documented_dimensions() {
        assert_eq!(PLAYER_SHEET_COLUMNS * 64, PLAYER_SHEET_WIDTH as usize);
        assert_eq!(PLAYER_SHEET_ROWS * 64, PLAYER_SHEET_HEIGHT as usize);
        assert_eq!(
            PLAYER_STATES_PER_DIRECTION * Facing::ALL.len(),
            PLAYER_SHEET_ROWS
        );

        assert_eq!(
            player_animation_row(Facing::Right, PLAYER_IDLE_STATE_ROW_OFFSET),
            0
        );
        assert_eq!(
            player_animation_row(Facing::Right, PLAYER_MOVE_STATE_ROW_OFFSET),
            1
        );
        assert_eq!(
            player_animation_row(Facing::Right, PLAYER_SHOOT_STATE_ROW_OFFSET),
            2
        );
        assert_eq!(
            player_animation_row(Facing::Left, PLAYER_IDLE_STATE_ROW_OFFSET),
            3
        );
        assert_eq!(
            player_animation_row(Facing::Up, PLAYER_IDLE_STATE_ROW_OFFSET),
            6
        );
        assert_eq!(
            player_animation_row(Facing::Down, PLAYER_SHOOT_STATE_ROW_OFFSET),
            11
        );
    }

    #[test]
    fn lightning_validation_allows_range_equality_and_rejects_outside() {
        let map = map::MapData::initial();
        let config = CombatConfig::default();
        assert_eq!(
            validate_attack_request(
                AttackKind::Lightning,
                Vec2::ZERO,
                Vec2::new(config.lightning_range, 0.0),
                config,
                &map,
            ),
            Ok(())
        );
        assert_eq!(
            validate_attack_request(
                AttackKind::Lightning,
                Vec2::ZERO,
                Vec2::new(config.lightning_range + 0.1, 0.0),
                config,
                &map,
            ),
            Err(RejectionReason::OutOfRange)
        );
    }

    #[test]
    fn lightning_validation_rejects_walls_and_missing_tiles() {
        let mut map = map::MapData::default();
        map.set(0, 0, map::TerrainTile::Floor);
        map.set(1, 0, map::TerrainTile::Wall);
        assert_eq!(
            validate_attack_request(
                AttackKind::Lightning,
                Vec2::ZERO,
                Vec2::new(map::TILE_SIZE, 0.0),
                CombatConfig::default(),
                &map,
            ),
            Err(RejectionReason::Obstructed)
        );

        map.set(1, 0, map::TerrainTile::Floor);
        assert_eq!(
            validate_attack_request(
                AttackKind::Lightning,
                Vec2::ZERO,
                Vec2::new(map::TILE_SIZE * 2.0, 0.0),
                CombatConfig::default(),
                &map,
            ),
            Err(RejectionReason::Obstructed)
        );
    }

    #[test]
    fn lightning_hit_selection_uses_nearest_segment_distance() {
        let first = Entity::from_raw_u32(10).unwrap();
        let second = Entity::from_raw_u32(2).unwrap();
        let hitbox = Collider::new(Vec2::splat(20.0), Vec2::ZERO);

        let hit = nearest_segment_hit(
            Vec2::ZERO,
            Vec2::new(200.0, 0.0),
            [
                (first, Vec2::new(120.0, 0.0), hitbox),
                (second, Vec2::new(60.0, 0.0), hitbox),
            ],
        );

        assert_eq!(hit, Some(second));
    }

    #[derive(Resource, Default)]
    struct FireCount(usize);

    fn count_attack_fired(mut messages: MessageReader<AttackFired>, mut count: ResMut<FireCount>) {
        count.0 += messages.read().count();
    }

    fn flat_test_map() -> map::MapData {
        floor_rect(-1..=1, -1..=1)
    }

    fn floor_rect(
        xs: impl IntoIterator<Item = i32> + Clone,
        ys: impl IntoIterator<Item = i32>,
    ) -> map::MapData {
        map::MapData {
            tiles: ys
                .into_iter()
                .flat_map(|y| {
                    xs.clone().into_iter().map(move |x| map::MapTile {
                        x,
                        y,
                        tile: map::TerrainTile::Floor,
                    })
                })
                .collect(),
            entities: Vec::new(),
        }
    }

    fn test_player_animations() -> PlayerAnimations {
        PlayerAnimations {
            idle: Facing::ALL.map(|_| Handle::default()),
            movement: Facing::ALL.map(|_| Handle::default()),
            shoot: Facing::ALL.map(|_| Handle::default()),
        }
    }

    #[test]
    fn lightning_fire_damages_once_and_visual_expires() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<AttackFired>()
            .add_message::<Damage>()
            .insert_resource(CombatConfig {
                lightning_visible_lifetime: 0.0,
                ..default()
            })
            .add_systems(
                Update,
                (
                    handle_lightning_attacks,
                    apply_damage,
                    tick_lightning_visuals,
                )
                    .chain(),
            );

        let player = app
            .world_mut()
            .spawn((
                Player,
                Health {
                    current: 100.0,
                    max: 100.0,
                },
            ))
            .id();
        let target = app
            .world_mut()
            .spawn((
                Health {
                    current: 100.0,
                    max: 100.0,
                },
                Hitbox(Collider::new(Vec2::splat(20.0), Vec2::ZERO)),
                Transform::from_xyz(50.0, 0.0, 0.0),
            ))
            .id();

        app.world_mut()
            .resource_mut::<Messages<AttackFired>>()
            .write(AttackFired {
                entity: player,
                request: AttackRequest {
                    kind: AttackKind::Lightning,
                    origin: Vec2::ZERO,
                    target: Vec2::new(100.0, 0.0),
                    direction: Vec2::X,
                },
            });
        app.update();
        assert_eq!(
            app.world().entity(target).get::<Health>().unwrap().current,
            60.0
        );

        app.update();
        assert_eq!(
            app.world().entity(target).get::<Health>().unwrap().current,
            60.0
        );

        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .advance_by(std::time::Duration::from_secs_f32(1.0));
        app.update();
        app.update();
        let visual_count = {
            let mut query = app.world_mut().query::<&LightningVisual>();
            query.iter(app.world()).count()
        };
        assert_eq!(visual_count, 0);
    }

    #[test]
    fn integration_auto_attacks_damage_an_enemy_by_range() {
        let target = Vec2::new(120.0, 0.0);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<AttackFired>()
            .add_message::<Damage>()
            .insert_resource(CombatConfig::default())
            .insert_resource(GameMap(floor_rect(-1..=5, -1..=1)))
            .insert_resource(TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_secs_f32(0.5),
            ))
            .add_systems(
                Update,
                (
                    handle_lightning_attacks,
                    spawn_projectiles,
                    move_projectiles,
                    apply_damage,
                )
                    .chain(),
            );
        let player = app
            .world_mut()
            .spawn((
                Player,
                Faction::Player,
                Health {
                    current: 100.0,
                    max: 100.0,
                },
            ))
            .id();
        let enemy = app
            .world_mut()
            .spawn((
                Faction::Enemy,
                Health {
                    current: 100.0,
                    max: 100.0,
                },
                Hitbox(Collider::new(Vec2::splat(20.0), Vec2::ZERO)),
                Transform::from_translation(target.extend(0.0)),
            ))
            .id();

        let lightning_target = Vec2::new(180.0, 0.0);
        let lightning_kind = resolve_attack_kind(
            AttackMode::Auto,
            Vec2::ZERO,
            lightning_target,
            CombatConfig::default().lightning_range,
        );
        assert_eq!(lightning_kind, AttackKind::Lightning);
        app.world_mut()
            .resource_mut::<Messages<AttackFired>>()
            .write(AttackFired {
                entity: player,
                request: AttackRequest {
                    kind: lightning_kind,
                    origin: Vec2::ZERO,
                    target: lightning_target,
                    direction: Vec2::X,
                },
            });
        app.update();
        assert_eq!(
            app.world().entity(enemy).get::<Health>().unwrap().current,
            60.0
        );

        app.world_mut()
            .entity_mut(enemy)
            .get_mut::<Health>()
            .unwrap()
            .current = 100.0;
        let projectile_target = Vec2::new(300.0, 0.0);
        let projectile_kind = resolve_attack_kind(
            AttackMode::Auto,
            Vec2::ZERO,
            projectile_target,
            CombatConfig::default().lightning_range,
        );
        assert_eq!(projectile_kind, AttackKind::Projectile);
        app.world_mut()
            .resource_mut::<Messages<AttackFired>>()
            .write(AttackFired {
                entity: player,
                request: AttackRequest {
                    kind: projectile_kind,
                    origin: Vec2::ZERO,
                    target: projectile_target,
                    direction: Vec2::X,
                },
            });
        app.update();
        assert_eq!(
            app.world().entity(enemy).get::<Health>().unwrap().current,
            70.0
        );
    }

    #[test]
    fn projectile_spawn_uses_normalized_direction_independent_of_click_distance() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<AttackFired>()
            .insert_resource(CombatConfig::default())
            .add_systems(Update, spawn_projectiles);

        let player = app
            .world_mut()
            .spawn((
                Player,
                Health {
                    current: 100.0,
                    max: 100.0,
                },
            ))
            .id();
        for target in [Vec2::new(10.0, 0.0), Vec2::new(1000.0, 0.0)] {
            app.world_mut()
                .resource_mut::<Messages<AttackFired>>()
                .write(AttackFired {
                    entity: player,
                    request: AttackRequest {
                        kind: AttackKind::Projectile,
                        origin: Vec2::ZERO,
                        target,
                        direction: (target - Vec2::ZERO).normalize(),
                    },
                });
        }
        app.update();

        let mut query = app.world_mut().query::<&Projectile>();
        let projectiles: Vec<_> = query.iter(app.world()).collect();
        assert_eq!(projectiles.len(), 2);
        assert!(
            projectiles
                .iter()
                .all(|projectile| projectile.direction == Vec2::X)
        );
    }

    #[test]
    fn projectile_swept_tests_choose_earliest_wall_or_entity() {
        let mut map = floor_rect(0..=3, -1..=1);
        map.set(2, 0, map::TerrainTile::Wall);
        let hitbox = Collider::new(Vec2::splat(20.0), Vec2::ZERO);

        let start = Vec2::ZERO;
        let end = Vec2::new(200.0, 0.0);
        let wall_time = projectile_terrain_hit_time(start, end, 6.0, &map).unwrap();
        let enemy_before_wall =
            projectile_hitbox_hit_time(start, end, 6.0, Vec2::new(70.0, 0.0), hitbox).unwrap();
        let enemy_after_wall =
            projectile_hitbox_hit_time(start, end, 6.0, Vec2::new(170.0, 0.0), hitbox).unwrap();

        assert!(enemy_before_wall < wall_time);
        assert!(wall_time < enemy_after_wall);
    }

    #[test]
    fn projectile_large_delta_hits_entity_once_and_ignores_owner_and_same_faction() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<Damage>()
            .insert_resource(GameMap(floor_rect(-1..=4, -1..=1)))
            .insert_resource(TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_secs_f32(0.5),
            ))
            .add_systems(Update, (move_projectiles, apply_damage).chain());

        let owner = app
            .world_mut()
            .spawn((
                Faction::Player,
                Health {
                    current: 100.0,
                    max: 100.0,
                },
                Hitbox(Collider::new(Vec2::splat(40.0), Vec2::ZERO)),
                Transform::from_xyz(20.0, 0.0, 0.0),
            ))
            .id();
        let ally = app
            .world_mut()
            .spawn((
                Faction::Player,
                Health {
                    current: 100.0,
                    max: 100.0,
                },
                Hitbox(Collider::new(Vec2::splat(40.0), Vec2::ZERO)),
                Transform::from_xyz(50.0, 0.0, 0.0),
            ))
            .id();
        let enemy = app
            .world_mut()
            .spawn((
                Faction::Enemy,
                Health {
                    current: 100.0,
                    max: 100.0,
                },
                Hitbox(Collider::new(Vec2::splat(20.0), Vec2::ZERO)),
                Transform::from_xyz(120.0, 0.0, 0.0),
            ))
            .id();

        app.world_mut().spawn((
            Projectile {
                owner,
                faction: Faction::Player,
                direction: Vec2::X,
                speed: 500.0,
                radius: 6.0,
                damage: 30.0,
                remaining_lifetime: 3.0,
            },
            Transform::from_xyz(0.0, 0.0, 0.0),
        ));

        app.update();
        app.update();

        assert_eq!(
            app.world().entity(owner).get::<Health>().unwrap().current,
            100.0
        );
        assert_eq!(
            app.world().entity(ally).get::<Health>().unwrap().current,
            100.0
        );
        assert_eq!(
            app.world().entity(enemy).get::<Health>().unwrap().current,
            70.0
        );
        let projectile_count = {
            let mut query = app.world_mut().query::<&Projectile>();
            query.iter(app.world()).count()
        };
        assert_eq!(projectile_count, 0);
    }

    #[test]
    fn projectile_nearest_of_multiple_enemies_uses_hit_time_not_spawn_order() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<Damage>()
            .insert_resource(GameMap(floor_rect(-1..=5, -1..=1)))
            .insert_resource(TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_secs_f32(0.5),
            ))
            .add_systems(Update, (move_projectiles, apply_damage).chain());

        let owner = app.world_mut().spawn_empty().id();
        let far_enemy = app
            .world_mut()
            .spawn((
                Faction::Enemy,
                Health {
                    current: 100.0,
                    max: 100.0,
                },
                Hitbox(Collider::new(Vec2::splat(20.0), Vec2::ZERO)),
                Transform::from_xyz(160.0, 0.0, 0.0),
            ))
            .id();
        let near_enemy = app
            .world_mut()
            .spawn((
                Faction::Enemy,
                Health {
                    current: 100.0,
                    max: 100.0,
                },
                Hitbox(Collider::new(Vec2::splat(20.0), Vec2::ZERO)),
                Transform::from_xyz(80.0, 0.0, 0.0),
            ))
            .id();
        app.world_mut().spawn((
            Projectile {
                owner,
                faction: Faction::Player,
                direction: Vec2::X,
                speed: 500.0,
                radius: 6.0,
                damage: 30.0,
                remaining_lifetime: 3.0,
            },
            Transform::from_xyz(0.0, 0.0, 0.0),
        ));

        app.update();
        app.update();

        assert_eq!(
            app.world()
                .entity(near_enemy)
                .get::<Health>()
                .unwrap()
                .current,
            70.0
        );
        assert_eq!(
            app.world()
                .entity(far_enemy)
                .get::<Health>()
                .unwrap()
                .current,
            100.0
        );
    }

    #[test]
    fn projectile_wall_before_enemy_deals_no_damage() {
        let mut map = floor_rect(-1..=4, -1..=1);
        map.set(1, 0, map::TerrainTile::Wall);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<Damage>()
            .insert_resource(GameMap(map))
            .insert_resource(TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_secs_f32(0.5),
            ))
            .add_systems(Update, (move_projectiles, apply_damage).chain());

        let owner = app.world_mut().spawn_empty().id();
        let enemy = app
            .world_mut()
            .spawn((
                Faction::Enemy,
                Health {
                    current: 100.0,
                    max: 100.0,
                },
                Hitbox(Collider::new(Vec2::splat(20.0), Vec2::ZERO)),
                Transform::from_xyz(120.0, 0.0, 0.0),
            ))
            .id();
        app.world_mut().spawn((
            Projectile {
                owner,
                faction: Faction::Player,
                direction: Vec2::X,
                speed: 500.0,
                radius: 6.0,
                damage: 30.0,
                remaining_lifetime: 3.0,
            },
            Transform::from_xyz(0.0, 0.0, 0.0),
        ));

        app.update();
        app.update();

        assert_eq!(
            app.world().entity(enemy).get::<Health>().unwrap().current,
            100.0
        );
        let projectile_count = {
            let mut query = app.world_mut().query::<&Projectile>();
            query.iter(app.world()).count()
        };
        assert_eq!(projectile_count, 0);
    }

    #[test]
    fn projectile_lifetime_cleanup() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<Damage>()
            .insert_resource(GameMap(floor_rect(-10..=10, -1..=1)))
            .insert_resource(TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_secs_f32(0.2),
            ))
            .add_systems(Update, move_projectiles);

        let owner = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((
            Projectile {
                owner,
                faction: Faction::Player,
                direction: Vec2::X,
                speed: 1.0,
                radius: 6.0,
                damage: 30.0,
                remaining_lifetime: 0.1,
            },
            Transform::from_xyz(0.0, 0.0, 0.0),
        ));

        app.update();
        app.update();

        let projectile_count = {
            let mut query = app.world_mut().query::<&Projectile>();
            query.iter(app.world()).count()
        };
        assert_eq!(projectile_count, 0);
    }

    #[test]
    fn enemy_spawns_at_each_map_placement_tile_center() {
        let mut map = floor_rect(0..=2, 0..=0);
        map.place_entity(0, 0, map::MapEntityKind::Enemy);
        map.place_entity(2, 0, map::MapEntityKind::Enemy);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(GameMap(map))
            .insert_resource(CombatConfig::default())
            .insert_resource(EnemyAnimations {
                idle: Handle::default(),
                movement: Handle::default(),
                stunned: Handle::default(),
                death: Handle::default(),
            })
            .init_resource::<EnemiesSpawned>()
            .add_systems(Update, spawn_enemies_from_map);

        app.update();
        let mut positions: Vec<_> = app
            .world_mut()
            .query_filtered::<&Transform, With<Enemy>>()
            .iter(app.world())
            .map(|transform| transform.translation.xy())
            .collect();
        positions.sort_by(|a, b| a.x.total_cmp(&b.x));
        assert_eq!(positions, vec![Vec2::ZERO, Vec2::new(192.0, 0.0)]);
    }

    #[test]
    fn enemy_chase_stops_at_attack_distance_and_slides_against_walls() {
        let movement = EnemyMovement {
            speed: 55.0,
            attack_distance: 64.0,
        };
        let collider = Collider::new(ENEMY_COLLIDER_SIZE, ENEMY_COLLIDER_OFFSET);
        let open_map = floor_rect(-1..=2, -1..=1);
        let open_occupancy = collision::Occupancy::terrain_only(&open_map);
        let approached = enemy_chase_position(
            Vec2::ZERO,
            Vec2::new(96.0, 0.0),
            movement,
            collider,
            1.0,
            &open_occupancy,
        );
        assert_eq!(approached, Vec2::new(55.0, 0.0));
        assert_eq!(
            enemy_chase_position(
                approached,
                Vec2::new(96.0, 0.0),
                movement,
                collider,
                1.0,
                &open_occupancy,
            ),
            approached
        );

        let mut wall_map = floor_rect(-1..=2, -1..=1);
        wall_map.set(1, 0, map::TerrainTile::Wall);
        let wall_occupancy = collision::Occupancy::terrain_only(&wall_map);
        assert_eq!(
            enemy_chase_position(
                Vec2::ZERO,
                Vec2::new(192.0, 0.0),
                movement,
                collider,
                2.0,
                &wall_occupancy,
            ),
            Vec2::ZERO
        );
    }

    #[test]
    fn enemy_update_system_moves_a_spawned_enemy_without_query_conflicts() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(GameMap(floor_rect(-1..=2, -1..=1)))
            .insert_resource(EnemyAnimations {
                idle: Handle::default(),
                movement: Handle::default(),
                stunned: Handle::default(),
                death: Handle::default(),
            })
            .insert_resource(TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_secs_f32(0.1),
            ))
            .add_systems(Update, update_enemies);
        app.world_mut()
            .spawn((Player, Transform::from_xyz(96.0, 0.0, 0.0)));
        let enemy = app
            .world_mut()
            .spawn((
                Enemy,
                Facing::Down,
                EnemyCollider(Collider::new(ENEMY_COLLIDER_SIZE, ENEMY_COLLIDER_OFFSET)),
                EnemyMovement {
                    speed: 55.0,
                    attack_distance: 64.0,
                },
                SpritesheetAnimation::new(Handle::default()),
                Transform::default(),
            ))
            .id();

        app.update();
        app.update();
        assert!(
            app.world()
                .entity(enemy)
                .get::<Transform>()
                .unwrap()
                .translation
                .x
                > 0.0
        );
    }

    fn melee_test_app(player_position: Vec2) -> (App, Entity, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(GameMap(floor_rect(-2..=5, -2..=2)))
            .insert_resource(EnemyAnimations {
                idle: Handle::default(),
                movement: Handle::default(),
                stunned: Handle::default(),
                death: Handle::default(),
            })
            .insert_resource(CombatConfig::default())
            .add_message::<Damage>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_secs_f32(0.1),
            ))
            .add_systems(Update, (update_enemies, apply_damage).chain());
        let player = app
            .world_mut()
            .spawn((
                Player,
                Health {
                    current: 100.0,
                    max: 100.0,
                },
                Transform::from_translation(player_position.extend(2.0)),
            ))
            .id();
        let enemy = app
            .world_mut()
            .spawn((
                Enemy,
                Facing::Right,
                EnemyCollider(Collider::new(ENEMY_COLLIDER_SIZE, ENEMY_COLLIDER_OFFSET)),
                EnemyMovement {
                    speed: 55.0,
                    attack_distance: 64.0,
                },
                EnemyAttack::Ready,
                SpritesheetAnimation::new(Handle::default()),
                Transform::default(),
            ))
            .id();
        (app, player, enemy)
    }

    #[test]
    fn integration_enemy_windup_damage_and_cooldown() {
        let (mut app, player, enemy) = melee_test_app(Vec2::new(64.0, 0.0));
        app.update();
        assert!(matches!(
            app.world().entity(enemy).get::<EnemyAttack>(),
            Some(EnemyAttack::WindingUp { .. })
        ));
        for _ in 0..4 {
            app.update();
        }
        assert_eq!(
            app.world().entity(player).get::<Health>().unwrap().current,
            80.0
        );
        assert!(matches!(
            app.world().entity(enemy).get::<EnemyAttack>(),
            Some(EnemyAttack::Cooldown { .. })
        ));
        for _ in 0..5 {
            app.update();
        }
        // The cooldown is longer than this interval, and invulnerability also blocks repeats.
        assert_eq!(
            app.world().entity(player).get::<Health>().unwrap().current,
            80.0
        );
    }

    #[test]
    fn enemies_do_not_start_melee_against_a_zero_health_player() {
        let (mut app, player, enemy) = melee_test_app(Vec2::new(64.0, 0.0));
        app.world_mut()
            .entity_mut(player)
            .get_mut::<Health>()
            .unwrap()
            .current = 0.0;
        app.update();
        assert!(matches!(
            app.world().entity(enemy).get::<EnemyAttack>(),
            Some(EnemyAttack::Ready)
        ));
        assert_eq!(
            app.world().entity(player).get::<Health>().unwrap().current,
            0.0
        );
    }

    #[test]
    fn integration_retreat_or_stun_cancels_enemy_windup() {
        let (mut app, player, enemy) = melee_test_app(Vec2::new(64.0, 0.0));
        app.update();
        app.world_mut()
            .entity_mut(player)
            .get_mut::<Transform>()
            .unwrap()
            .translation
            .x = 200.0;
        for _ in 0..4 {
            app.update();
        }
        assert_eq!(
            app.world().entity(player).get::<Health>().unwrap().current,
            100.0
        );
        assert!(matches!(
            app.world().entity(enemy).get::<EnemyAttack>(),
            Some(EnemyAttack::Cooldown { .. })
        ));

        let (mut app, player, enemy) = melee_test_app(Vec2::new(64.0, 0.0));
        app.update();
        app.world_mut()
            .entity_mut(enemy)
            .insert(Stunned { remaining: 1.0 });
        app.update();
        assert!(matches!(
            app.world().entity(enemy).get::<EnemyAttack>(),
            Some(EnemyAttack::Ready)
        ));
        for _ in 0..4 {
            app.update();
        }
        assert_eq!(
            app.world().entity(player).get::<Health>().unwrap().current,
            100.0
        );
    }

    #[test]
    fn integration_enemy_defeat_is_checkpoint_filterable() {
        let id = PlacementId { x: 1, y: 0 };
        let mut death_app = App::new();
        death_app
            .add_plugins(MinimalPlugins)
            .insert_resource(RunProgress::default())
            .insert_resource(HealthDropSeed::default())
            .insert_resource(CombatConfig::default())
            .insert_resource(EnemyAnimations {
                idle: Handle::default(),
                movement: Handle::default(),
                stunned: Handle::default(),
                death: Handle::default(),
            })
            .add_systems(Update, begin_enemy_deaths);
        let dying_enemy = death_app
            .world_mut()
            .spawn((
                Enemy,
                Placement(id),
                Health {
                    current: 0.0,
                    max: 100.0,
                },
                Transform::default(),
            ))
            .id();
        death_app.update();
        assert!(death_app.world().entity(dying_enemy).contains::<Dying>());
        assert!(
            death_app
                .world()
                .resource::<RunProgress>()
                .defeated_enemies
                .contains(&id)
        );

        let mut progress = RunProgress::default();
        progress.defeated_enemies.insert(id);
        let mut map = floor_rect(0..=1, 0..=0);
        map.place_entity(id.x, id.y, map::MapEntityKind::Enemy);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(GameMap(map))
            .insert_resource(CombatConfig::default())
            .insert_resource(progress)
            .insert_resource(EnemyAnimations {
                idle: Handle::default(),
                movement: Handle::default(),
                stunned: Handle::default(),
                death: Handle::default(),
            })
            .init_resource::<EnemiesSpawned>()
            .add_systems(Update, spawn_enemies_from_map);
        app.update();
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Enemy>>()
                .iter(app.world())
                .count(),
            0
        );
    }

    #[test]
    fn enemy_health_uses_the_generic_damage_pipeline() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<Damage>()
            .add_systems(Update, apply_damage);
        let enemy = app
            .world_mut()
            .spawn((
                Enemy,
                Health {
                    current: 100.0,
                    max: 100.0,
                },
            ))
            .id();
        app.world_mut()
            .resource_mut::<Messages<Damage>>()
            .write(Damage {
                target: enemy,
                amount: 40.0,
            });
        app.update();
        assert_eq!(
            app.world().entity(enemy).get::<Health>().unwrap().current,
            60.0
        );
    }

    #[test]
    fn health_bar_ratio_is_clamped() {
        assert_eq!(
            health_bar_ratio(Health {
                current: 50.0,
                max: 100.0
            }),
            0.5
        );
        assert_eq!(
            health_bar_ratio(Health {
                current: -1.0,
                max: 100.0
            }),
            0.0
        );
        assert_eq!(
            health_bar_ratio(Health {
                current: 101.0,
                max: 100.0
            }),
            1.0
        );
        assert_eq!(
            health_bar_ratio(Health {
                current: 1.0,
                max: 0.0
            }),
            0.0
        );
    }

    #[test]
    fn stun_resets_duration_and_prevents_movement() {
        assert_eq!(refreshed_stun(2.0).remaining, 2.0);
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(GameMap(floor_rect(-1..=2, -1..=1)))
            .insert_resource(EnemyAnimations {
                idle: Handle::default(),
                movement: Handle::default(),
                stunned: Handle::default(),
                death: Handle::default(),
            })
            .insert_resource(TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_secs_f32(0.1),
            ))
            .add_systems(Update, update_enemies);
        app.world_mut()
            .spawn((Player, Transform::from_xyz(96.0, 0.0, 0.0)));
        let enemy = app
            .world_mut()
            .spawn((
                Enemy,
                Facing::Right,
                Stunned { remaining: 2.0 },
                EnemyCollider(Collider::new(ENEMY_COLLIDER_SIZE, ENEMY_COLLIDER_OFFSET)),
                EnemyMovement {
                    speed: 55.0,
                    attack_distance: 64.0,
                },
                SpritesheetAnimation::new(Handle::default()),
                Transform::default(),
            ))
            .id();
        app.update();
        assert_eq!(
            app.world()
                .entity(enemy)
                .get::<Transform>()
                .unwrap()
                .translation
                .xy(),
            Vec2::ZERO
        );
    }

    #[test]
    fn each_stun_binding_is_recognized() {
        for key in [KeyCode::ShiftLeft, KeyCode::ShiftRight] {
            let mut keyboard = ButtonInput::default();
            keyboard.press(key);
            assert!(stun_requested(&keyboard, &ButtonInput::default()));
        }
        let mut mouse = ButtonInput::default();
        mouse.press(MouseButton::Middle);
        assert!(stun_requested(&ButtonInput::default(), &mouse));
    }

    #[test]
    fn stun_bindings_coalesce_apply_once_and_shockwave_expires() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .insert_resource(CombatConfig {
                shockwave_radius: 20.0,
                shockwave_animation_duration: 0.1,
                stun_duration: 2.0,
                ..default()
            })
            .init_resource::<CameraShakeConfig>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_secs_f32(0.2),
            ))
            .add_systems(Update, (cast_stun, tick_shockwaves).chain());
        app.world_mut().spawn((Player, Transform::default()));
        let in_range = app
            .world_mut()
            .spawn((
                Enemy,
                Health {
                    current: 100.0,
                    max: 100.0,
                },
                Hitbox(Collider::new(Vec2::splat(10.0), Vec2::ZERO)),
                Transform::from_xyz(25.0, 0.0, 0.0),
            ))
            .id();
        let out_of_range = app
            .world_mut()
            .spawn((
                Enemy,
                Health {
                    current: 100.0,
                    max: 100.0,
                },
                Hitbox(Collider::new(Vec2::splat(10.0), Vec2::ZERO)),
                Transform::from_xyz(25.1, 0.0, 0.0),
            ))
            .id();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ShiftLeft);
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Middle);
        app.update();
        assert_eq!(
            app.world()
                .entity(in_range)
                .get::<Stunned>()
                .unwrap()
                .remaining,
            2.0
        );
        assert!(app.world().entity(out_of_range).get::<Stunned>().is_none());
        assert_eq!(
            app.world_mut()
                .query::<&ShockwaveVisual>()
                .iter(app.world())
                .count(),
            1,
            "simultaneous bindings still spawn only one effect"
        );
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear_just_pressed(KeyCode::ShiftLeft);
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .clear_just_pressed(MouseButton::Middle);
        app.update();
        assert_eq!(
            app.world_mut()
                .query::<&ShockwaveVisual>()
                .iter(app.world())
                .count(),
            0
        );
        assert!(app.world().resource::<CameraShakeConfig>().trauma > 0.0);
    }

    #[test]
    fn camera_shake_clamps_decays_and_restores_transform() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(CameraShakeConfig {
                trauma: 2.0,
                trauma_decay_per_second: 1.0,
                ..default()
            })
            .insert_resource(TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_secs_f32(0.25),
            ))
            .add_systems(Update, (restore_camera_transform, shake_camera).chain());
        let normal = Transform::from_xyz(10.0, 20.0, 3.0);
        let camera = app
            .world_mut()
            .spawn((Camera2d, normal, UnshakenCameraTransform(normal)))
            .id();
        app.update();
        assert!(app.world().resource::<CameraShakeConfig>().trauma <= 1.0);
        app.world_mut().resource_mut::<CameraShakeConfig>().trauma = 0.0;
        app.update();
        assert_eq!(
            *app.world().entity(camera).get::<Transform>().unwrap(),
            normal
        );
    }

    #[test]
    fn integration_zero_health_stops_controls_and_delays_game_over() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .init_state::<GameState>()
            .init_resource::<CombatConfig>()
            .init_resource::<AttackFeedback>()
            .init_resource::<TeleportCooldown>()
            .init_resource::<CameraShakeConfig>()
            .add_message::<AttackFired>()
            .add_message::<Damage>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_secs_f32(0.4),
            ))
            .add_systems(
                Update,
                (begin_player_death, tick_player_death)
                    .chain()
                    .run_if(in_state(GameState::Game)),
            );
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Game);
        app.update();
        let player = app
            .world_mut()
            .spawn((
                Player,
                Health {
                    current: 0.0,
                    max: 100.0,
                },
                Hitbox(Collider::new(Vec2::ONE, Vec2::ZERO)),
            ))
            .id();
        app.update();
        assert!(app.world().entity(player).contains::<PlayerDying>());
        assert!(!app.world().entity(player).contains::<Hitbox>());
        // The first 0.4 s frame starts the presentation; it does not skip to GameOver.
        assert_eq!(
            app.world().resource::<State<GameState>>().get(),
            &GameState::Game
        );
        app.update();
        app.update();
        app.update();
        assert_eq!(
            app.world().resource::<State<GameState>>().get(),
            &GameState::GameOver
        );
        let remaining = app
            .world()
            .entity(player)
            .get::<PlayerDying>()
            .unwrap()
            .remaining;
        app.update();
        assert_eq!(
            app.world()
                .entity(player)
                .get::<PlayerDying>()
                .unwrap()
                .remaining,
            remaining
        );
    }

    #[test]
    fn death_is_one_way_ignores_post_mortem_damage_and_waits_for_animation() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<Damage>()
            .add_message::<AnimationEvent>()
            .insert_resource(EnemyAnimations {
                idle: Handle::default(),
                movement: Handle::default(),
                stunned: Handle::default(),
                death: Handle::default(),
            })
            .insert_resource(HealthDropSeed::default())
            .insert_resource(CombatConfig::default())
            .add_systems(
                Update,
                (apply_damage, begin_enemy_deaths, finish_enemy_deaths).chain(),
            );
        let enemy = app
            .world_mut()
            .spawn((
                Enemy,
                Health {
                    current: 20.0,
                    max: 20.0,
                },
                Hitbox(Collider::new(Vec2::ONE, Vec2::ZERO)),
                EnemyMovement {
                    speed: 1.0,
                    attack_distance: 1.0,
                },
                Transform::default(),
            ))
            .id();
        app.world_mut()
            .resource_mut::<Messages<Damage>>()
            .write(Damage {
                target: enemy,
                amount: 20.0,
            });
        app.update();
        app.update();
        assert!(app.world().entity(enemy).contains::<Dying>());
        assert!(!app.world().entity(enemy).contains::<Health>());
        app.world_mut()
            .resource_mut::<Messages<Damage>>()
            .write(Damage {
                target: enemy,
                amount: 20.0,
            });
        app.update();
        assert!(app.world().get_entity(enemy).is_ok());
        let death_animation = app
            .world()
            .entity(enemy)
            .get::<Dying>()
            .unwrap()
            .animation
            .clone();
        app.world_mut()
            .resource_mut::<Messages<AnimationEvent>>()
            .write(AnimationEvent::AnimationEnd {
                entity: enemy,
                animation: death_animation,
            });
        app.update();
        app.update();
        assert!(app.world().get_entity(enemy).is_err());
    }

    #[test]
    fn teleport_uses_shared_occupancy_and_ignores_entities() {
        let collider = Collider::new(PLAYER_COLLIDER_SIZE, PLAYER_COLLIDER_OFFSET);
        let mut map = floor_rect(-1..=1, -1..=1);
        let occupancy = collision::Occupancy::terrain_only(&map);
        assert_eq!(
            teleport_destination(Vec2::ZERO, collider, &occupancy),
            Some(-collider.offset),
            "the clicked point is the feet collider center, not the player sprite origin"
        );

        map.set(1, 0, map::TerrainTile::Wall);
        let occupancy = collision::Occupancy::terrain_only(&map);
        assert_eq!(
            teleport_destination(Vec2::new(map::TILE_SIZE, 0.0), collider, &occupancy),
            None
        );
        assert_eq!(
            teleport_destination(Vec2::new(map::TILE_SIZE / 2.0, 0.0), collider, &occupancy),
            None,
            "a collider overlapping a wall cannot teleport near its edge"
        );

        map.remove(0, 1);
        let occupancy = collision::Occupancy::terrain_only(&map);
        assert_eq!(
            teleport_destination(Vec2::new(0.0, map::TILE_SIZE), collider, &occupancy),
            None,
            "missing sparse terrain blocks teleport"
        );

        let edge_map = floor_rect(0..=0, 0..=0);
        let occupancy = collision::Occupancy::terrain_only(&edge_map);
        let tile_sized = Collider::new(Vec2::splat(map::TILE_SIZE), Vec2::ZERO);
        assert_eq!(
            teleport_destination(Vec2::ZERO, tile_sized, &occupancy),
            Some(Vec2::ZERO)
        );

        // Occupancy deliberately contains terrain only, so enemy/entity overlap is allowed.
        let enemy_position = Vec2::ZERO;
        assert_eq!(
            teleport_destination(enemy_position, collider, &occupancy),
            Some(enemy_position - collider.offset)
        );
    }

    #[test]
    fn teleport_cooldown_ticks_to_readiness_and_reports_busy_rejection() {
        let mut cooldown = TeleportCooldown {
            remaining: 5.0,
            ..default()
        };
        advance_teleport_cooldown(&mut cooldown, 2.0);
        assert_eq!(cooldown.remaining, 3.0);
        advance_teleport_cooldown(&mut cooldown, 2.0);
        assert_eq!(cooldown.remaining, 1.0);
        advance_teleport_cooldown(&mut cooldown, 2.0);
        assert!(cooldown.ready());

        reject_teleport(&mut cooldown, TeleportRejection::Busy);
        assert_eq!(cooldown.rejection, Some(TeleportRejection::Busy));
    }

    #[test]
    fn simultaneous_stun_input_has_priority_over_fire_teleport_and_movement() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .insert_resource(GameMap(flat_test_map()))
            .insert_resource(test_player_animations())
            .insert_resource(SelectedAttackMode(AttackMode::Auto))
            .insert_resource(CombatConfig::default())
            .insert_resource(CursorWorld(Some(Vec2::new(10.0, 0.0))))
            .init_resource::<AttackFeedback>()
            .init_resource::<TeleportCooldown>()
            .add_systems(Update, control_character);
        let player = app
            .world_mut()
            .spawn((
                Player,
                PlayerCollider(Collider::new(PLAYER_COLLIDER_SIZE, PLAYER_COLLIDER_OFFSET)),
                Facing::Right,
                SpritesheetAnimation::new(Handle::default()),
                Transform::from_xyz(0.0, 0.0, 2.0),
            ))
            .id();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ShiftLeft);
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        app.update();

        assert!(!app.world().entity(player).contains::<PlayerAction>());
        assert_eq!(
            app.world()
                .entity(player)
                .get::<Transform>()
                .unwrap()
                .translation
                .xy(),
            Vec2::ZERO
        );
    }

    #[test]
    fn click_creates_one_fire_event_and_locks_movement_until_animation_completion() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<AnimationEvent>()
            .add_message::<AttackFired>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .insert_resource(GameMap(flat_test_map()))
            .insert_resource(test_player_animations())
            .insert_resource(SelectedAttackMode(AttackMode::Auto))
            .insert_resource(CombatConfig::default())
            .insert_resource(CursorWorld(Some(Vec2::new(10.0, 0.0))))
            .init_resource::<AttackFeedback>()
            .init_resource::<TeleportCooldown>()
            .init_resource::<FireCount>()
            .add_systems(
                Update,
                (
                    control_character,
                    trigger_attack_fire_event,
                    count_attack_fired,
                    handle_attack_animation_events,
                )
                    .chain(),
            );

        let player = app
            .world_mut()
            .spawn((
                Player,
                PlayerCollider(Collider::new(PLAYER_COLLIDER_SIZE, PLAYER_COLLIDER_OFFSET)),
                Facing::Right,
                SpritesheetAnimation::new(Handle::default()),
                Transform::from_xyz(0.0, 0.0, 2.0),
            ))
            .id();
        // Enemy animations must not make player-only Single queries ambiguous.
        app.world_mut().spawn((
            Enemy,
            Facing::Left,
            SpritesheetAnimation::new(Handle::default()),
            Transform::from_xyz(100.0, 0.0, 2.0),
        ));

        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .clear_just_pressed(MouseButton::Left);

        assert!(app.world().entity(player).contains::<PlayerAction>());
        assert_eq!(app.world().resource::<FireCount>().0, 0);

        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Right);
        app.update();
        assert_eq!(
            app.world().resource::<TeleportCooldown>().rejection,
            Some(TeleportRejection::Busy)
        );
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .clear_just_pressed(MouseButton::Right);

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        app.update();
        assert_eq!(
            app.world()
                .entity(player)
                .get::<Transform>()
                .unwrap()
                .translation
                .y,
            0.0
        );

        app.world_mut()
            .entity_mut(player)
            .get_mut::<SpritesheetAnimation>()
            .unwrap()
            .progress
            .frame = PLAYER_SHOOT_FIRE_FRAME;
        app.update();
        assert_eq!(app.world().resource::<FireCount>().0, 1);

        app.update();
        assert_eq!(app.world().resource::<FireCount>().0, 1);

        let attack_animation = app
            .world()
            .entity(player)
            .get::<PlayerAction>()
            .unwrap()
            .animation
            .clone();
        app.world_mut()
            .resource_mut::<Messages<AnimationEvent>>()
            .write(AnimationEvent::AnimationRepetitionEnd {
                entity: player,
                animation: attack_animation,
                animation_repetition: 0,
            });
        app.update();
        assert!(!app.world().entity(player).contains::<PlayerAction>());

        app.update();
        assert!(
            app.world()
                .entity(player)
                .get::<Transform>()
                .unwrap()
                .translation
                .y
                > 0.0
        );
    }

    #[derive(Resource, Default)]
    struct AppliedDamageCount(u32);

    fn count_applied_damage(
        mut applied: MessageReader<DamageApplied>,
        mut count: ResMut<AppliedDamageCount>,
    ) {
        count.0 += applied.read().count() as u32;
    }

    #[test]
    fn integration_damage_grants_invulnerability_then_accepts_later_hit() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<CombatConfig>()
            .add_message::<Damage>()
            .add_message::<DamageApplied>()
            .init_resource::<AppliedDamageCount>()
            .add_systems(
                Update,
                (
                    apply_damage,
                    tick_player_invulnerability,
                    count_applied_damage,
                )
                    .chain(),
            );
        let starting_health = default_player_health(app.world().resource::<CombatConfig>());
        assert_eq!(starting_health.current, 100.0);
        assert_eq!(starting_health.max, 100.0);
        let player = app.world_mut().spawn((Player, starting_health)).id();

        for _ in 0..2 {
            app.world_mut()
                .resource_mut::<Messages<Damage>>()
                .write(Damage {
                    target: player,
                    amount: 20.0,
                });
        }
        app.update();
        assert_eq!(
            app.world().entity(player).get::<Health>().unwrap().current,
            80.0
        );
        assert!(app.world().entity(player).contains::<Invulnerable>());
        assert_eq!(app.world().resource::<AppliedDamageCount>().0, 1);

        // The invulnerability component blocks further messages until its gameplay timer expires.
        app.world_mut()
            .resource_mut::<Messages<Damage>>()
            .write(Damage {
                target: player,
                amount: 20.0,
            });
        app.update();
        assert_eq!(
            app.world().entity(player).get::<Health>().unwrap().current,
            80.0
        );
        assert_eq!(app.world().resource::<AppliedDamageCount>().0, 1);

        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(0.51));
        assert!(app.world().resource::<Time>().delta_secs() >= 0.5);
        app.world_mut().run_schedule(Update);
        assert!(!app.world().entity(player).contains::<Invulnerable>());

        app.world_mut()
            .resource_mut::<Messages<Damage>>()
            .write(Damage {
                target: player,
                amount: 20.0,
            });
        app.update();
        assert_eq!(
            app.world().entity(player).get::<Health>().unwrap().current,
            60.0
        );
        assert_eq!(app.world().resource::<AppliedDamageCount>().0, 2);

        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(0.51));
        app.world_mut().run_schedule(Update);
        app.world_mut()
            .resource_mut::<Messages<Damage>>()
            .write(Damage {
                target: player,
                amount: 1_000.0,
            });
        app.update();
        assert_eq!(
            app.world().entity(player).get::<Health>().unwrap().current,
            0.0
        );
        assert_eq!(app.world().resource::<AppliedDamageCount>().0, 3);

        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(0.51));
        app.world_mut().run_schedule(Update);
        for amount in [0.0, -1.0, 20.0] {
            app.world_mut()
                .resource_mut::<Messages<Damage>>()
                .write(Damage {
                    target: player,
                    amount,
                });
        }
        app.update();
        assert_eq!(
            app.world().entity(player).get::<Health>().unwrap().current,
            0.0
        );
        assert_eq!(app.world().resource::<AppliedDamageCount>().0, 3);
    }

    #[test]
    fn health_clamps_and_bar_ratio_is_safe() {
        assert_eq!(
            player_health_ratio(Health {
                current: 150.0,
                max: 100.0
            }),
            1.0
        );
        assert_eq!(
            player_health_ratio(Health {
                current: -2.0,
                max: 100.0
            }),
            0.0
        );
        assert_eq!(
            player_health_ratio(Health {
                current: 5.0,
                max: 0.0
            }),
            0.0
        );
    }

    #[test]
    fn player_health_bar_ratio_and_fill_width_are_safe() {
        let cases = [
            (
                Health {
                    current: 100.0,
                    max: 100.0,
                },
                1.0,
            ),
            (
                Health {
                    current: 25.0,
                    max: 100.0,
                },
                0.25,
            ),
            (
                Health {
                    current: 0.0,
                    max: 100.0,
                },
                0.0,
            ),
            (
                Health {
                    current: 150.0,
                    max: 100.0,
                },
                1.0,
            ),
            (
                Health {
                    current: 10.0,
                    max: 0.0,
                },
                0.0,
            ),
            (
                Health {
                    current: -1.0,
                    max: 100.0,
                },
                0.0,
            ),
            (
                Health {
                    current: f32::NAN,
                    max: 100.0,
                },
                0.0,
            ),
            (
                Health {
                    current: f32::INFINITY,
                    max: 100.0,
                },
                0.0,
            ),
            (
                Health {
                    current: 25.0,
                    max: f32::INFINITY,
                },
                0.0,
            ),
            (
                Health {
                    current: 25.0,
                    max: f32::NEG_INFINITY,
                },
                0.0,
            ),
        ];
        for (health, expected_ratio) in cases {
            assert_eq!(player_health_ratio(health), expected_ratio);
            assert_eq!(
                player_health_fill_width(health),
                Val::Percent(expected_ratio * 100.0)
            );
        }
        assert_eq!(
            player_health_text(Health {
                current: f32::NAN,
                max: 100.0,
            }),
            "Health: 0 / 0"
        );
        assert_eq!(
            player_health_text(Health {
                current: 25.0,
                max: f32::INFINITY,
            }),
            "Health: 0 / 0"
        );
        assert_eq!(
            player_health_text(Health {
                current: -10.0,
                max: 100.0,
            }),
            "Health: 0 / 100"
        );
    }

    #[test]
    fn player_health_bar_refreshes_text_and_fill_in_dialogue() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .init_state::<GameState>()
            .add_systems(
                Update,
                update_player_health_hud.run_if(in_state(GameState::Dialogue)),
            );
        app.world_mut().spawn((
            Player,
            Health {
                current: 35.0,
                max: 150.0,
            },
        ));
        let text = app
            .world_mut()
            .spawn((PlayerHealthText, Text::new("stale")))
            .id();
        let fill = app
            .world_mut()
            .spawn((
                PlayerHealthBarFill,
                Node {
                    width: percent(100),
                    ..default()
                },
            ))
            .id();
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Dialogue);
        app.update();

        assert_eq!(
            app.world().entity(text).get::<Text>().unwrap().0,
            "Health: 35 / 150"
        );
        assert_eq!(
            app.world().entity(fill).get::<Node>().unwrap().width,
            Val::Percent(35.0 / 150.0 * 100.0)
        );
    }

    #[test]
    fn player_health_hud_spawns_once_and_is_run_scoped() {
        let mut app = App::new();
        app.init_resource::<RunNeedsSpawn>()
            .add_systems(Update, setup_player_health_hud);
        app.update();
        app.world_mut().resource_mut::<RunNeedsSpawn>().0 = false;
        app.update();

        let world = app.world_mut();
        assert_eq!(world.query::<&PlayerHealthHud>().iter(world).count(), 1);
        assert_eq!(world.query::<&RunScoped>().iter(world).count(), 1);
        assert_eq!(world.query::<&PlayerHealthBarFill>().iter(world).count(), 1);
        assert_eq!(world.query::<&PlayerHealthText>().iter(world).count(), 1);
    }

    #[test]
    fn terminal_touch_includes_exact_trigger_boundary() {
        let collider = Collider::new(Vec2::new(40.0, 20.0), Vec2::ZERO);
        // Trigger right edge 28 and player left edge 28 meet exactly.
        assert!(terminal_trigger_overlaps(
            Vec2::new(48.0, 0.0),
            collider,
            Vec2::ZERO,
            Vec2::splat(56.0)
        ));
        assert!(!terminal_trigger_overlaps(
            Vec2::new(48.01, 0.0),
            collider,
            Vec2::ZERO,
            Vec2::splat(56.0)
        ));
    }

    #[test]
    fn terminal_activation_commits_checkpoint_before_dialogue_request() {
        let placement = PlacementId { x: 3, y: -2 };
        let mut terminal = Terminal {
            placement,
            dialogue_id: "terminal_intro".into(),
            activated: false,
        };
        let mut progress = RunProgress::default();
        progress
            .collected_pickups
            .insert(PlacementId { x: 1, y: 1 });
        let mut checkpoint = CheckpointSnapshot::default();

        commit_terminal_activation(&mut terminal, &mut progress, &mut checkpoint);

        assert!(terminal.activated);
        assert!(terminal_is_activated(&progress, placement));
        assert_eq!(checkpoint.respawn, placement);
        assert_eq!(checkpoint.progress, progress);
        assert!(
            checkpoint
                .progress
                .collected_pickups
                .contains(&PlacementId { x: 1, y: 1 })
        );
    }

    #[test]
    fn terminal_catalog_fallback_is_a_system_line_and_restored_terminals_stay_inactive() {
        let catalog = DialogueCatalog::default();
        let lines = terminal_dialogue_lines(&catalog, "missing_terminal");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].speaker, Speaker::System);
        assert!(lines[0].text.contains("missing_terminal"));

        let placement = PlacementId { x: 2, y: 4 };
        let mut progress = RunProgress::default();
        progress.activated_terminals.insert(placement);
        assert!(terminal_is_activated(&progress, placement));
        assert!(!terminal_is_activated(
            &progress,
            PlacementId { x: 2, y: 5 }
        ));
    }

    #[test]
    fn integration_terminal_checkpoint_precedes_dialogue_and_does_not_replay() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .init_state::<GameState>()
            .init_resource::<CombatConfig>()
            .init_resource::<RunProgress>()
            .init_resource::<CheckpointSnapshot>()
            .insert_resource(StoryDialogueCatalog(DialogueCatalog {
                conversations: std::collections::BTreeMap::from([(
                    "intro".into(),
                    vec![DialogueLine {
                        speaker: Speaker::NoOne,
                        text: "checkpoint first".into(),
                    }],
                )]),
            }))
            .add_systems(Update, activate_touched_terminal);
        let placement = PlacementId { x: 1, y: 0 };
        let terminal = app
            .world_mut()
            .spawn((
                Terminal {
                    placement,
                    dialogue_id: "intro".into(),
                    activated: false,
                },
                TerminalTrigger,
                Sprite::default(),
                Transform::from_translation(Vec3::new(20.0, 0.0, 1.0)),
            ))
            .id();
        app.world_mut().spawn((
            Player,
            PlayerCollider(Collider::new(Vec2::splat(20.0), Vec2::ZERO)),
            Transform::default(),
        ));

        app.update();
        assert!(
            app.world()
                .entity(terminal)
                .get::<Terminal>()
                .unwrap()
                .activated
        );
        assert!(!app.world().entity(terminal).contains::<TerminalTrigger>());
        assert_eq!(
            app.world().resource::<CheckpointSnapshot>().respawn,
            placement
        );
        assert!(
            app.world()
                .resource::<crate::game_dialogue::ActiveDialogue>()
                .lines
                .first()
                .is_some_and(|line| line.text == "checkpoint first")
        );

        app.update();
        assert_eq!(
            app.world()
                .resource::<RunProgress>()
                .activated_terminals
                .len(),
            1
        );
    }

    #[test]
    fn terminal_order_is_deterministic_by_tile() {
        let overlapping = [
            PlacementId { x: 4, y: 0 },
            PlacementId { x: -1, y: -1 },
            PlacementId { x: 0, y: -1 },
        ];
        assert_eq!(
            overlapping.into_iter().min_by_key(|id| (id.y, id.x)),
            Some(PlacementId { x: -1, y: -1 })
        );
    }

    #[test]
    fn integration_continue_rolls_back_post_checkpoint_progress_and_new_game_clears_it() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .init_state::<GameState>()
            .init_resource::<RestartRequest>()
            .init_resource::<RunProgress>()
            .init_resource::<CheckpointSnapshot>()
            .init_resource::<SelectedAttackMode>()
            .init_resource::<TeleportCooldown>()
            .init_resource::<AttackFeedback>()
            .init_resource::<CollisionDebug>()
            .init_resource::<CameraShakeConfig>()
            .init_resource::<EnemiesSpawned>()
            .init_resource::<TerminalsSpawned>()
            .init_resource::<PickupsSpawned>()
            .init_resource::<HealthDropSeed>()
            .init_resource::<HealthDropSeedSequence>()
            .init_resource::<PlayerSpawn>()
            .init_resource::<RunNeedsSpawn>()
            .add_systems(Update, restart_run);
        let old = app.world_mut().spawn(RunScoped).id();
        let checkpoint_id = PlacementId { x: 3, y: -2 };
        let checkpoint_pickup = PlacementId { x: 4, y: -2 };
        let post_checkpoint_pickup = PlacementId { x: 5, y: -2 };
        {
            let mut snapshot = app.world_mut().resource_mut::<CheckpointSnapshot>();
            snapshot.progress.defeated_enemies.insert(checkpoint_id);
            snapshot
                .progress
                .collected_pickups
                .insert(checkpoint_pickup);
            snapshot.progress.unlocked_skills.insert(Skill::Stun);
            snapshot.respawn = checkpoint_id;
        }
        // Simulate progress made after the checkpoint; Continue must discard it.
        {
            let mut current = app.world_mut().resource_mut::<RunProgress>();
            current.collected_pickups.insert(post_checkpoint_pickup);
            current.unlocked_skills.insert(Skill::Teleport);
        }
        app.world_mut().resource_mut::<RestartRequest>().0 = RestartIntent::ContinueCheckpoint;
        app.update();

        assert!(app.world().get_entity(old).is_err());
        assert!(
            app.world()
                .resource::<RunProgress>()
                .defeated_enemies
                .contains(&checkpoint_id)
        );
        assert_eq!(
            app.world().resource::<PlayerSpawn>().0,
            Vec2::new(3.0 * map::TILE_SIZE, -2.0 * map::TILE_SIZE)
        );
        assert!(app.world().resource::<RunNeedsSpawn>().0);
        assert!(
            app.world()
                .resource::<RunProgress>()
                .collected_pickups
                .contains(&checkpoint_pickup)
        );
        assert!(
            !app.world()
                .resource::<RunProgress>()
                .collected_pickups
                .contains(&post_checkpoint_pickup)
        );
        assert!(
            app.world()
                .resource::<RunProgress>()
                .unlocked_skills
                .contains(&Skill::Stun)
        );
        assert!(
            !app.world()
                .resource::<RunProgress>()
                .unlocked_skills
                .contains(&Skill::Teleport)
        );

        app.world_mut().resource_mut::<RestartRequest>().0 = RestartIntent::NewGame;
        app.update();
        assert!(
            app.world()
                .resource::<RunProgress>()
                .defeated_enemies
                .is_empty()
        );
        assert_eq!(
            *app.world().resource::<CheckpointSnapshot>(),
            CheckpointSnapshot::default()
        );
    }

    #[test]
    fn integration_progression_unlocks_dynamic_hud_controls() {
        let progress = RunProgress::default();
        assert!(progress.unlocked_skills.is_empty());
        assert_eq!(SelectedAttackMode::default().0, AttackMode::Lightning);
        let hud = format_attack_hud(
            AttackMode::Lightning,
            None,
            TeleportCooldown::default(),
            &progress,
        );
        assert!(!hud.contains("Projectile"));
        assert!(!hud.contains("Stun:"));
        assert!(!hud.contains("teleport"));

        let mut unlocked = progress.clone();
        unlocked.unlocked_skills.insert(Skill::Projectile);
        unlocked.unlocked_skills.insert(Skill::Stun);
        unlocked.unlocked_skills.insert(Skill::Teleport);
        let hud = format_attack_hud(
            AttackMode::Projectile,
            None,
            TeleportCooldown::default(),
            &unlocked,
        );
        assert!(hud.contains("2 Projectile, 3 Auto"));
        assert!(hud.contains("Stun: Shift/middle click"));
        assert!(hud.contains("Right click: teleport"));
        let rejected_teleport_hud = format_attack_hud(
            AttackMode::Lightning,
            None,
            TeleportCooldown {
                rejection: Some(TeleportRejection::InvalidTerrain),
                ..default()
            },
            &unlocked,
        );
        assert!(rejected_teleport_hud.contains("rejected: blocked terrain"));
    }

    #[test]
    fn integration_pickups_are_one_shot_with_exact_messages_and_armor_effect() {
        let config = CombatConfig::default();
        let mut progress = RunProgress::default();
        let mut health = Health {
            current: 40.0,
            max: 100.0,
        };
        assert!(apply_pickup(
            PickupKind::Skill(Skill::Projectile),
            &mut progress,
            &mut health,
            &config
        ));
        assert!(!apply_pickup(
            PickupKind::Skill(Skill::Projectile),
            &mut progress,
            &mut health,
            &config
        ));
        assert!(progress.unlocked_skills.contains(&Skill::Projectile));
        assert_eq!(
            pickup_message(PickupKind::Skill(Skill::Projectile), false),
            "You have unlocked the long-range attack. Press 2 to select it, or press 3 for automatic range selection. Fire with left click."
        );
        assert_eq!(
            pickup_message(PickupKind::Skill(Skill::Stun), false),
            "You have unlocked stun. Press either Shift key or middle click to stun nearby enemies."
        );
        assert_eq!(
            pickup_message(PickupKind::Skill(Skill::Teleport), false),
            "You have unlocked teleport. Right click valid floor to teleport."
        );
        assert_eq!(
            pickup_message(PickupKind::ReinforcedArmor, false),
            "You have unlocked Reinforced Armor. Maximum health increased by 50."
        );
        assert!(apply_pickup(
            PickupKind::ReinforcedArmor,
            &mut progress,
            &mut health,
            &config
        ));
        assert_eq!(
            health,
            Health {
                current: 90.0,
                max: 150.0
            }
        );
        assert_eq!(progress.maximum_health_bonus, 50);
        let mut full_health = Health {
            current: 100.0,
            max: 100.0,
        };
        let mut full_progress = RunProgress::default();
        apply_pickup(
            PickupKind::ReinforcedArmor,
            &mut full_progress,
            &mut full_health,
            &config,
        );
        assert_eq!(
            full_health,
            Health {
                current: 150.0,
                max: 150.0
            }
        );
    }

    #[test]
    fn integration_locked_abilities_are_silent_until_pickup_unlocks_them() {
        let mut mode_app = App::new();
        mode_app
            .add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<SelectedAttackMode>()
            .init_resource::<RunProgress>()
            .add_systems(Update, select_attack_mode);
        mode_app
            .world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Digit2);
        mode_app.update();
        assert_eq!(
            mode_app.world().resource::<SelectedAttackMode>().0,
            AttackMode::Lightning
        );
        mode_app
            .world_mut()
            .insert_resource(ButtonInput::<KeyCode>::default());
        mode_app
            .world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Digit3);
        mode_app.update();
        assert_eq!(
            mode_app.world().resource::<SelectedAttackMode>().0,
            AttackMode::Lightning
        );
        mode_app
            .world_mut()
            .insert_resource(ButtonInput::<KeyCode>::default());
        mode_app
            .world_mut()
            .resource_mut::<RunProgress>()
            .unlocked_skills
            .insert(Skill::Projectile);
        mode_app
            .world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Digit2);
        mode_app.update();
        assert_eq!(
            mode_app.world().resource::<SelectedAttackMode>().0,
            AttackMode::Projectile
        );
        mode_app
            .world_mut()
            .insert_resource(ButtonInput::<KeyCode>::default());
        mode_app
            .world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Digit3);
        mode_app.update();
        assert_eq!(
            mode_app.world().resource::<SelectedAttackMode>().0,
            AttackMode::Auto
        );

        let mut stun_app = App::new();
        stun_app
            .add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<CombatConfig>()
            .init_resource::<CameraShakeConfig>()
            .init_resource::<RunProgress>()
            .add_systems(Update, cast_stun);
        stun_app.world_mut().spawn((Player, Transform::default()));
        stun_app
            .world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ShiftLeft);
        stun_app.update();
        assert_eq!(stun_app.world().resource::<CameraShakeConfig>().trauma, 0.0);
        assert_eq!(
            stun_app
                .world_mut()
                .query::<&ShockwaveVisual>()
                .iter(stun_app.world())
                .count(),
            0
        );

        let mut teleport_app = App::new();
        teleport_app
            .add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .insert_resource(GameMap(flat_test_map()))
            .insert_resource(test_player_animations())
            .init_resource::<SelectedAttackMode>()
            .init_resource::<CombatConfig>()
            .insert_resource(CursorWorld(Some(Vec2::new(10.0, 0.0))))
            .init_resource::<AttackFeedback>()
            .init_resource::<TeleportCooldown>()
            .init_resource::<RunProgress>()
            .add_systems(Update, control_character);
        let player = teleport_app
            .world_mut()
            .spawn((
                Player,
                PlayerCollider(Collider::new(PLAYER_COLLIDER_SIZE, PLAYER_COLLIDER_OFFSET)),
                Facing::Right,
                SpritesheetAnimation::new(Handle::default()),
                Transform::from_xyz(0.0, 0.0, 2.0),
            ))
            .id();
        teleport_app
            .world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Right);
        teleport_app.update();
        assert_eq!(
            teleport_app
                .world()
                .entity(player)
                .get::<Transform>()
                .unwrap()
                .translation
                .xy(),
            Vec2::ZERO
        );
        assert!(teleport_app.world().resource::<TeleportCooldown>().ready());
        assert_eq!(
            teleport_app
                .world()
                .resource::<TeleportCooldown>()
                .rejection,
            None
        );
        assert!(
            !teleport_app
                .world()
                .entity(player)
                .contains::<PlayerAction>()
        );
    }

    #[test]
    fn integration_terminal_pickup_overlap_defers_pickup_to_later_frame() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .init_state::<GameState>()
            .init_resource::<CombatConfig>()
            .init_resource::<RunProgress>()
            .add_systems(Update, activate_touched_pickup);
        let pickup_id = PlacementId { x: 1, y: 1 };
        let pickup = app
            .world_mut()
            .spawn((
                Pickup {
                    placement: pickup_id,
                    kind: PickupKind::Skill(Skill::Stun),
                },
                PickupTrigger,
                Transform::default(),
            ))
            .id();
        let terminal = app
            .world_mut()
            .spawn((
                Terminal {
                    placement: PlacementId::default(),
                    dialogue_id: "intro".into(),
                    activated: false,
                },
                TerminalTrigger,
                Transform::default(),
            ))
            .id();
        app.world_mut().spawn((
            Player,
            PlayerCollider(Collider::new(Vec2::splat(20.0), Vec2::ZERO)),
            Health {
                current: 100.0,
                max: 100.0,
            },
            Transform::default(),
        ));
        app.update();
        assert!(app.world().get_entity(pickup).is_ok());
        assert!(
            !app.world()
                .resource::<RunProgress>()
                .collected_pickups
                .contains(&pickup_id)
        );

        app.world_mut().despawn(terminal);
        app.update();
        assert!(app.world().get_entity(pickup).is_err());
        assert!(
            app.world()
                .resource::<RunProgress>()
                .collected_pickups
                .contains(&pickup_id)
        );
        assert!(
            app.world()
                .resource::<RunProgress>()
                .unlocked_skills
                .contains(&Skill::Stun)
        );
        app.update();
        assert_eq!(
            app.world()
                .resource::<RunProgress>()
                .collected_pickups
                .len(),
            1
        );
    }

    #[test]
    fn integration_game_over_ui_uses_exact_actions_and_routes_restarting() {
        let mut ui_app = App::new();
        ui_app
            .add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .init_state::<GameState>()
            .add_systems(OnEnter(GameState::GameOver), setup_game_over_ui);
        ui_app
            .world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::GameOver);
        ui_app.update();
        let labels: Vec<_> = ui_app
            .world_mut()
            .query::<&Text>()
            .iter(ui_app.world())
            .map(|text| text.0.clone())
            .collect();
        assert!(labels.iter().any(|label| label == "Game Over"));
        assert!(
            labels
                .iter()
                .any(|label| label == "Continue from Last Checkpoint")
        );
        assert!(labels.iter().any(|label| label == "Main Menu"));
        assert!(!labels.iter().any(|label| label == "Retry"));

        for (action, expected) in [
            (GameOverAction::Continue, RestartIntent::ContinueCheckpoint),
            (GameOverAction::MainMenu, RestartIntent::MainMenu),
        ] {
            let mut app = App::new();
            app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
                .init_state::<GameState>()
                .init_resource::<RestartRequest>()
                .add_systems(Update, game_over_action);
            app.world_mut()
                .resource_mut::<NextState<GameState>>()
                .set(GameState::GameOver);
            app.update();
            app.world_mut()
                .spawn((Button, Interaction::Pressed, action));
            app.update();
            assert_eq!(app.world().resource::<RestartRequest>().0, expected);
            app.update();
            assert_eq!(
                app.world().resource::<State<GameState>>().get(),
                &GameState::Restarting
            );
        }
    }

    #[test]
    fn integration_new_run_uses_implicit_checkpoint_lightning_and_full_health() {
        let config = CombatConfig::default();
        let snapshot = CheckpointSnapshot::default();
        assert_eq!(snapshot.progress, RunProgress::default());
        assert_eq!(snapshot.respawn, PlacementId::default());
        assert_eq!(SelectedAttackMode::default().0, AttackMode::Lightning);
        assert_eq!(
            restored_player_health(&config, &snapshot.progress),
            Health {
                current: 100.0,
                max: 100.0,
            }
        );
    }

    #[test]
    fn health_drop_seed_sequence_refreshes_new_games_preserves_continue_and_cleans_drops() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .init_state::<GameState>()
            .init_resource::<RestartRequest>()
            .init_resource::<RunProgress>()
            .init_resource::<CheckpointSnapshot>()
            .init_resource::<SelectedAttackMode>()
            .init_resource::<TeleportCooldown>()
            .init_resource::<AttackFeedback>()
            .init_resource::<CollisionDebug>()
            .init_resource::<CameraShakeConfig>()
            .init_resource::<EnemiesSpawned>()
            .init_resource::<TerminalsSpawned>()
            .init_resource::<PickupsSpawned>()
            .init_resource::<HealthDropSeed>()
            .init_resource::<HealthDropSeedSequence>()
            .init_resource::<PlayerSpawn>()
            .init_resource::<RunNeedsSpawn>()
            .add_systems(Update, restart_run);

        app.world_mut().resource_mut::<RestartRequest>().0 = RestartIntent::NewGame;
        app.update();
        let first_seed = *app.world().resource::<HealthDropSeed>();
        let sequence_after_first = *app.world().resource::<HealthDropSeedSequence>();
        let drop = app
            .world_mut()
            .spawn((
                RunScoped,
                HealthDrop {
                    source: PlacementId { x: 1, y: 2 },
                },
                HealthDropTrigger,
            ))
            .id();

        app.world_mut().resource_mut::<RestartRequest>().0 = RestartIntent::ContinueCheckpoint;
        app.update();
        assert_eq!(*app.world().resource::<HealthDropSeed>(), first_seed);
        assert_eq!(
            *app.world().resource::<HealthDropSeedSequence>(),
            sequence_after_first
        );
        assert!(app.world().get_entity(drop).is_err());

        app.world_mut().resource_mut::<RestartRequest>().0 = RestartIntent::MainMenu;
        app.update();
        assert_eq!(
            *app.world().resource::<HealthDropSeedSequence>(),
            sequence_after_first
        );
        app.world_mut().resource_mut::<RestartRequest>().0 = RestartIntent::NewGame;
        app.update();
        let second_seed = *app.world().resource::<HealthDropSeed>();
        assert_ne!(first_seed, second_seed);
        assert_ne!(
            *app.world().resource::<HealthDropSeedSequence>(),
            sequence_after_first
        );
    }

    #[test]
    fn health_drop_roll_is_seeded_and_clamps_chance() {
        let placement = PlacementId { x: -3, y: 7 };
        assert!(!health_drop_roll(1, placement, 0.0));
        assert!(health_drop_roll(1, placement, 1.0));
        assert!(!health_drop_roll(1, placement, -5.0));
        assert!(health_drop_roll(1, placement, 5.0));
        assert_eq!(
            health_drop_roll(0x42, placement, 0.25),
            health_drop_roll(0x42, placement, 0.25)
        );
        assert_ne!(next_health_drop_seed(0x42), 0x42);
        let differs_for_some_placement = (-32..=32).any(|x| {
            (-32..=32).any(|y| {
                let placement = PlacementId { x, y };
                health_drop_roll(0x42, placement, 0.25)
                    != health_drop_roll(next_health_drop_seed(0x42), placement, 0.25)
            })
        });
        assert!(differs_for_some_placement);
    }

    #[test]
    fn placed_enemy_death_spawns_one_run_scoped_health_drop_at_death_position() {
        let placement = PlacementId { x: 2, y: -4 };
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(EnemyAnimations {
                idle: Handle::default(),
                movement: Handle::default(),
                stunned: Handle::default(),
                death: Handle::default(),
            })
            .insert_resource(HealthDropSeed(7))
            .insert_resource(CombatConfig {
                enemy_health_drop_chance: 1.0,
                ..default()
            })
            .insert_resource(RunProgress::default())
            .add_systems(Update, begin_enemy_deaths);
        let position = Vec3::new(31.0, -17.0, 2.0);
        let enemy = app
            .world_mut()
            .spawn((
                Enemy,
                Placement(placement),
                Health {
                    current: 0.0,
                    max: 100.0,
                },
                Transform::from_translation(position),
            ))
            .id();

        app.update();
        app.update();
        assert!(app.world().entity(enemy).contains::<Dying>());
        let world = app.world_mut();
        let drops: Vec<_> = world
            .query::<(&HealthDrop, &Transform, &RunScoped)>()
            .iter(world)
            .collect();
        assert_eq!(drops.len(), 1);
        assert_eq!(drops[0].0.source, placement);
        assert_eq!(drops[0].1.translation, position.with_z(3.0));
    }

    #[test]
    fn unplaced_enemy_death_never_spawns_health_drop() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(EnemyAnimations {
                idle: Handle::default(),
                movement: Handle::default(),
                stunned: Handle::default(),
                death: Handle::default(),
            })
            .insert_resource(HealthDropSeed::default())
            .insert_resource(CombatConfig {
                enemy_health_drop_chance: 1.0,
                ..default()
            })
            .add_systems(Update, begin_enemy_deaths);
        app.world_mut().spawn((
            Enemy,
            Health {
                current: 0.0,
                max: 100.0,
            },
            Transform::default(),
        ));
        app.update();
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<HealthDrop>>()
                .iter(app.world())
                .count(),
            0
        );
    }

    #[test]
    fn integration_checkpoint_restores_pickups_and_full_armor_health() {
        let config = CombatConfig::default();
        let pickup = PlacementId { x: 2, y: 3 };
        let mut checkpoint_progress = RunProgress::default();
        checkpoint_progress.collected_pickups.insert(pickup);
        checkpoint_progress.unlocked_skills.insert(Skill::Stun);
        checkpoint_progress.maximum_health_bonus = 50;
        let snapshot = CheckpointSnapshot {
            progress: checkpoint_progress.clone(),
            respawn: PlacementId::default(),
        };
        let restored = snapshot.progress.clone();
        assert!(restored.collected_pickups.contains(&pickup));
        assert!(restored.unlocked_skills.contains(&Skill::Stun));
        assert_eq!(
            restored_player_health(&config, &restored),
            Health {
                current: 150.0,
                max: 150.0
            }
        );
        assert_eq!(
            restored_player_health(&config, &RunProgress::default()),
            Health {
                current: 100.0,
                max: 100.0
            }
        );
    }
}
