//! There is no actual game, it will just display the current
//! settings for 5 seconds before going back to the menu.

use bevy::{app::AppExit, prelude::*, window::PrimaryWindow};
use bevy_spritesheet_animation::prelude::*;
use map_support::{
    collision::{self, Collider},
    map,
    tilemap::{self, TerrainAtlas, TerrainTilemapPlugin},
};

const TEXT_COLOR: Color = Color::srgb(0.9, 0.9, 0.9);
const PAUSE_BUTTON_NORMAL: Color = Color::srgb(0.18, 0.25, 0.38);
const PAUSE_BUTTON_HOVERED: Color = Color::srgb(0.25, 0.42, 0.65);
const PAUSE_BUTTON_PRESSED: Color = Color::srgb(0.12, 0.65, 0.35);

use crate::GameState;

// This plugin contains the game.
pub fn game_plugin(app: &mut App) {
    app.add_plugins(TerrainTilemapPlugin)
        .init_resource::<CollisionDebug>()
        .init_resource::<CombatConfig>()
        .init_resource::<SelectedAttackMode>()
        .init_resource::<CursorWorld>()
        .init_resource::<AttackFeedback>()
        .add_message::<AttackFired>()
        .add_message::<Damage>();
    app.add_systems(
        OnEnter(GameState::Game),
        (setup_scene, setup_instructions, spawn_character),
    )
    .add_systems(OnEnter(GameState::Paused), pause_menu_setup)
    .add_systems(
        Update,
        (
            update_cursor_world_position,
            select_attack_mode,
            control_character,
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
    .add_systems(
        Update,
        (pause_menu_action, pause_menu_button_system).run_if(in_state(GameState::Paused)),
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

#[derive(Resource)]
struct GameMap(map::MapData);

#[derive(Resource, Default)]
struct CollisionDebug {
    enabled: bool,
}

#[derive(Resource, Clone, Copy, Debug)]
struct CombatConfig {
    lightning_range: f32,
    lightning_damage: f32,
    lightning_visible_lifetime: f32,
    projectile_speed: f32,
    projectile_damage: f32,
    projectile_collision_radius: f32,
    projectile_maximum_lifetime: f32,
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
        }
    }
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
        Self(AttackMode::Auto)
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

#[derive(Component)]
struct AttackHud;

#[derive(Component, Clone, Copy, Debug)]
struct Hitbox(Collider);

#[allow(dead_code)]
#[derive(Component, Clone, Copy, Debug)]
struct Health {
    current: f32,
    max: f32,
}

#[derive(Message, Clone, Copy, Debug)]
struct Damage {
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

#[derive(Resource)]
struct GameInitialized;

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

fn setup_scene(
    mut commands: Commands,
    terrain_atlas: Res<TerrainAtlas>,
    initialized: Option<Res<GameInitialized>>,
) {
    if initialized.is_some() {
        return;
    }

    commands.insert_resource(GameInitialized);
    let map = map::load_map("initial").unwrap_or_else(|error| {
        warn!("Could not load initial map: {error}. Using the built-in map.");
        map::MapData::initial()
    });
    commands.insert_resource(GameMap(map.clone()));
    tilemap::spawn_map(&mut commands, &terrain_atlas, &map);
}

fn spawn_character(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut animations: ResMut<Assets<Animation>>,
    mut atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    initialized: Option<Res<GameInitialized>>,
) {
    if initialized.is_some() {
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
        Player,
        Faction::Player,
        PlayerCollider(Collider::new(PLAYER_COLLIDER_SIZE, PLAYER_COLLIDER_OFFSET)),
        Facing::Right,
        sprite,
        SpritesheetAnimation::new(idle[Facing::Right.animation_index()].clone()),
        Transform::from_xyz(0., 0., 2.),
    ));
}

fn setup_instructions(mut commands: Commands, initialized: Option<Res<GameInitialized>>) {
    if initialized.is_some() {
        return;
    }

    commands.spawn((
        AttackHud,
        Text::new(format_attack_hud(AttackMode::Auto, None)),
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
    player: Single<(&Transform, &PlayerCollider), With<Player>>,
    mut gizmos: Gizmos,
) {
    if collision_debug.enabled {
        let (transform, collider) = *player;
        gizmos.rect_2d(
            transform.translation.xy() + collider.0.offset,
            collider.0.size,
            Color::srgb(1.0, 0.0, 1.0),
        );
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

fn format_attack_hud(mode: AttackMode, rejection: Option<RejectionReason>) -> String {
    let status = match rejection {
        Some(RejectionReason::OutOfRange) => " | Lightning rejected: target out of range",
        Some(RejectionReason::Obstructed) => " | Lightning rejected: terrain blocks the path",
        None => "",
    };
    format!(
        "Move: WASD/arrows | Fire: left click | Modes: 1 Lightning, 2 Projectile, 3 Auto | Selected: {}{} | Pause: Esc | Debug: F3",
        mode.label(),
        status
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
) {
    if keyboard.just_pressed(KeyCode::Digit1) {
        selected_mode.0 = AttackMode::Lightning;
    } else if keyboard.just_pressed(KeyCode::Digit2) {
        selected_mode.0 = AttackMode::Projectile;
    } else if keyboard.just_pressed(KeyCode::Digit3) {
        selected_mode.0 = AttackMode::Auto;
    }
}

fn update_attack_hud(
    selected_mode: Res<SelectedAttackMode>,
    feedback: Res<AttackFeedback>,
    mut hud: Single<&mut Text, With<AttackHud>>,
) {
    if selected_mode.is_changed() || feedback.is_changed() {
        **hud = Text::new(format_attack_hud(
            selected_mode.0,
            feedback.rejection.map(|rejection| rejection.reason),
        ));
    }
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
    )>,
    game_map: Res<GameMap>,
    my_animations: Res<PlayerAnimations>,
    selected_mode: Res<SelectedAttackMode>,
    combat_config: Res<CombatConfig>,
    cursor_world: Res<CursorWorld>,
    mut feedback: ResMut<AttackFeedback>,
) {
    let (entity, mut animation, mut transform, mut facing, collider, action) =
        character.into_inner();

    if action.is_some() {
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
    character: Single<(Entity, &SpritesheetAnimation, Option<&mut PlayerAction>)>,
    mut attack_fired: MessageWriter<AttackFired>,
) {
    let (entity, sprite_animation, action) = character.into_inner();
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
    hitboxes: Query<(Entity, &Transform, &Hitbox), With<Health>>,
) {
    for fired in attack_fired.read() {
        if fired.request.kind != AttackKind::Lightning {
            continue;
        }

        commands.spawn((
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
) {
    for fired in attack_fired.read() {
        if fired.request.kind != AttackKind::Projectile {
            continue;
        }

        commands.spawn((
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

fn apply_damage(mut damage: MessageReader<Damage>, mut health: Query<&mut Health>) {
    for damage in damage.read() {
        if let Ok(mut health) = health.get_mut(damage.target) {
            health.current = (health.current - damage.amount).max(0.0);
        }
    }
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

fn tick_attack_feedback(time: Res<Time>, mut feedback: ResMut<AttackFeedback>) {
    if let Some(mut rejection) = feedback.rejection {
        rejection.remaining -= time.delta_secs();
        feedback.rejection = (rejection.remaining > 0.0).then_some(rejection);
    }
}

fn draw_lightning_range_feedback(
    selected_mode: Res<SelectedAttackMode>,
    combat_config: Res<CombatConfig>,
    feedback: Res<AttackFeedback>,
    player: Single<&Transform, With<Player>>,
    mut gizmos: Gizmos,
) {
    if matches!(selected_mode.0, AttackMode::Lightning | AttackMode::Auto) {
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
    character: Single<(
        Entity,
        &mut SpritesheetAnimation,
        &Facing,
        Option<&PlayerAction>,
    )>,
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

        let player = app.world_mut().spawn_empty().id();
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
    fn projectile_spawn_uses_normalized_direction_independent_of_click_distance() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<AttackFired>()
            .insert_resource(CombatConfig::default())
            .add_systems(Update, spawn_projectiles);

        let player = app.world_mut().spawn_empty().id();
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
}
