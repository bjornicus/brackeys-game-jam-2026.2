//! There is no actual game, it will just display the current
//! settings for 5 seconds before going back to the menu.

use bevy::{app::AppExit, prelude::*};
use bevy_spritesheet_animation::prelude::*;
use map_support::{
    map::{self, TILE_SIZE, TerrainTile},
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
struct PlayerCollider {
    size: Vec2,
    offset: Vec2,
}

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

// Let's use a custom resource to store our animations and access them across systems
#[derive(Resource)]
struct PlayerAnimations {
    idle: Handle<Animation>,
    move_right: Handle<Animation>,
    move_left: Handle<Animation>,
    shoot: Handle<Animation>,
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

    let image = assets.load("sprites/character.png");

    let spritesheet = Spritesheet::new(&image, 8, 8);

    // Idle

    let idle_animation = spritesheet
        .create_animation()
        .add_horizontal_strip(0, 0, 5)
        .build();

    let idle_animation_handle = animations.add(idle_animation);

    // Movement animations. Row 2 faces left and row 3 faces right.

    let move_left_animation = spritesheet.create_animation().add_row(2).build();
    let move_left_animation_handle = animations.add(move_left_animation);

    let move_right_animation = spritesheet.create_animation().add_row(3).build();
    let move_right_animation_handle = animations.add(move_right_animation);

    // Shoot

    let shoot_animation = spritesheet
        .create_animation()
        .add_horizontal_strip(0, 5, 5)
        .build();

    let shoot_animation_handle = animations.add(shoot_animation);

    // Store the animations as a resource

    commands.insert_resource(PlayerAnimations {
        idle: idle_animation_handle.clone(),
        move_right: move_right_animation_handle,
        move_left: move_left_animation_handle,
        shoot: shoot_animation_handle,
    });

    // Spawn the character

    let sprite = spritesheet
        .with_size_hint(768, 768)
        .sprite(&mut atlas_layouts);

    commands.spawn((
        Player,
        PlayerCollider {
            size: PLAYER_COLLIDER_SIZE,
            offset: PLAYER_COLLIDER_OFFSET,
        },
        Facing::Right,
        sprite,
        SpritesheetAnimation::new(idle_animation_handle),
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
            transform.translation.xy() + collider.offset,
            collider.size,
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

// Records the last horizontal direction so vertical-only movement can use the
// corresponding movement animation.
#[derive(Component, Clone, Copy)]
enum Facing {
    Left,
    Right,
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
    let horizontal_movement = Vec2::new(movement.x, 0.0);
    if can_occupy(
        transform.translation.xy() + horizontal_movement,
        collider,
        map,
    ) {
        transform.translation += horizontal_movement.extend(0.0);
    }

    let vertical_movement = Vec2::new(0.0, movement.y);
    if can_occupy(
        transform.translation.xy() + vertical_movement,
        collider,
        map,
    ) {
        transform.translation += vertical_movement.extend(0.0);
    }
}

fn can_occupy(position: Vec2, collider: &PlayerCollider, map: &map::MapData) -> bool {
    let collider_center = position + collider.offset;
    let half_size = collider.size / 2.0;
    let min = collider_center - half_size;
    let max = collider_center + half_size;
    let half_tile = TILE_SIZE / 2.0;

    // Tiles are centered on integer grid coordinates, so shift by half a tile before
    // converting world positions to grid positions. The tiny epsilon keeps a collider
    // that exactly touches an edge from entering the neighboring tile.
    let min_x = ((min.x + half_tile) / TILE_SIZE).floor() as i32;
    let max_x = ((max.x + half_tile - f32::EPSILON) / TILE_SIZE).floor() as i32;
    let min_y = ((min.y + half_tile) / TILE_SIZE).floor() as i32;
    let max_y = ((max.y + half_tile - f32::EPSILON) / TILE_SIZE).floor() as i32;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            if map.tile_at(x, y) != Some(TerrainTile::Floor) {
                return false;
            }
        }
    }
    true
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

            animation.switch(my_animations.shoot.clone());

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
                // Horizontal input takes priority for diagonal movement. Vertical-only
                // movement uses the most recently selected horizontal facing direction.
                if direction.x < 0. {
                    *facing = Facing::Left;
                } else if direction.x > 0. {
                    *facing = Facing::Right;
                }

                let movement_animation = match *facing {
                    Facing::Left => &my_animations.move_left,
                    Facing::Right => &my_animations.move_right,
                };
                if animation.animation != *movement_animation {
                    animation.switch(movement_animation.clone());
                }

                let move_delta = direction.normalize() * PLAYER_SPEED * time.delta_secs();
                move_with_collision(&mut transform, collider, move_delta, &game_map.0);
            } else if animation.animation != my_animations.idle {
                animation.switch(my_animations.idle.clone());
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
            && animation == &my_animations.shoot
        {
            commands.entity(*entity).remove::<Shooting>();
        }
    }
}
