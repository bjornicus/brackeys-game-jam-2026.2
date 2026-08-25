//! There is no actual game, it will just display the current
//! settings for 5 seconds before going back to the menu.

use bevy::prelude::*;
use bevy_spritesheet_animation::prelude::*;

const TEXT_COLOR: Color = Color::srgb(0.9, 0.9, 0.9);

use crate::GameState;

// This plugin will contain the game. In this case, it's just be a screen that will
// display the current settings for 5 seconds before returning to the menu
pub fn game_plugin(app: &mut App) {
    app.add_systems(
        OnEnter(GameState::Game),
        (setup_scene, setup_instructions, spawn_character),
    )
    .add_systems(
        Update,
        (control_character, update_camera)
            .chain()
            .run_if(in_state(GameState::Game)),
    );
}

/// Player movement speed factor.
const PLAYER_SPEED: f32 = 100.;

/// How quickly should the camera snap to the desired location.
const CAMERA_DECAY_RATE: f32 = 2.;

#[derive(Component)]
struct Player;

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
) {
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
) {
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

fn setup_instructions(mut commands: Commands) {
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
