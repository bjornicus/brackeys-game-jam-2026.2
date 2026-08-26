//! There is no actual game, it will just display the current
//! settings for 5 seconds before going back to the menu.

use bevy::{app::AppExit, prelude::*};
use bevy_spritesheet_animation::prelude::*;

const TEXT_COLOR: Color = Color::srgb(0.9, 0.9, 0.9);
const PAUSE_BUTTON_NORMAL: Color = Color::srgb(0.18, 0.25, 0.38);
const PAUSE_BUTTON_HOVERED: Color = Color::srgb(0.25, 0.42, 0.65);
const PAUSE_BUTTON_PRESSED: Color = Color::srgb(0.12, 0.65, 0.35);

use crate::GameState;

// This plugin will contain the game. In this case, it's just be a screen that will
// display the current settings for 5 seconds before returning to the menu
pub fn game_plugin(app: &mut App) {
    app.add_systems(
        OnEnter(GameState::Game),
        (setup_scene, setup_instructions, spawn_character),
    )
    .add_systems(OnEnter(GameState::Paused), pause_menu_setup)
    .add_systems(
        Update,
        (control_character, update_camera, pause_game)
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

#[derive(Component)]
struct Player;

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
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    initialized: Option<Res<GameInitialized>>,
) {
    if initialized.is_some() {
        return;
    }

    commands.insert_resource(GameInitialized);
    // World where we move the player
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(1000., 700.))),
        MeshMaterial2d(materials.add(Color::srgb(0.2, 0.2, 0.3))),
    ));
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
        Text::new("Move with WASD or arrow keys.\nThe camera will smoothly track the player."),
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

fn control_character(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    character: Single<(
        Entity,
        &mut SpritesheetAnimation,
        &mut Transform,
        &mut Facing,
        Option<&Shooting>,
    )>,
    my_animations: Res<PlayerAnimations>,
    mut messages: MessageReader<AnimationEvent>,
) {
    // Control the character with the keyboard

    let (entity, mut animation, mut transform, mut facing, shooting) = character.into_inner();

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
                transform.translation += move_delta.extend(0.);
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
