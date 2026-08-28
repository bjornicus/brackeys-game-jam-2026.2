//! There is no actual game, it will just display the current
//! settings for 5 seconds before going back to the menu.

use bevy::{app::AppExit, prelude::*};
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
        .init_resource::<CollisionDebug>();
    app.add_systems(
        OnEnter(GameState::Game),
        (setup_scene, setup_instructions, spawn_character),
    )
    .add_systems(OnEnter(GameState::Paused), pause_menu_setup)
    .add_systems(
        Update,
        (
            control_character,
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

    fn is_shoot_animation(&self, handle: &Handle<Animation>) -> bool {
        self.shoot.iter().any(|shoot| shoot == handle)
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
        make_strip_animation(
            &mut animations,
            player_animation_row(facing, PLAYER_SHOOT_STATE_ROW_OFFSET),
            PLAYER_SHOOT_FRAMES,
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
        Text::new("Move with WASD or arrow keys. Fire with Space"),
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

// Component to mark that a character is currently shooting
#[derive(Component)]
struct Shooting;

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
    mut commands: Commands,
    character: Single<(
        Entity,
        &mut SpritesheetAnimation,
        &mut Transform,
        &mut Facing,
        &PlayerCollider,
        Option<&Shooting>,
    )>,
    game_map: Res<GameMap>,
    my_animations: Res<PlayerAnimations>,
    mut messages: MessageReader<AnimationEvent>,
) {
    // Control the character with the keyboard

    let (entity, mut animation, mut transform, mut facing, collider, shooting) =
        character.into_inner();

    // If they're shooting, do nothing and wait for the animation to end

    if shooting.is_none() {
        // Shoot with the spacebar
        if keyboard.pressed(KeyCode::Space) {
            // Set the animation

            animation.switch(my_animations.shoot_for(*facing).clone());

            // Add a Shooting component

            commands.entity(entity).insert(Shooting);
        } else {
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
    }

    // Remove the Shooting component when the shooting animation ends
    //
    // We use animation events to detect when this happens.
    // Check out the `events` examples for more details.

    for event in messages.read() {
        if let AnimationEvent::AnimationRepetitionEnd {
            entity, animation, ..
        } = event
            && my_animations.is_shoot_animation(animation)
        {
            commands.entity(*entity).remove::<Shooting>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
