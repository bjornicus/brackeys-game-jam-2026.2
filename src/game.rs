//! There is no actual game, it will just display the current
//! settings for 5 seconds before going back to the menu.

use bevy::prelude::*;
use bevy_spritesheet_animation::prelude::*;

const TEXT_COLOR: Color = Color::srgb(0.9, 0.9, 0.9);


use crate::GameState;

// This plugin will contain the game. In this case, it's just be a screen that will
// display the current settings for 5 seconds before returning to the menu
pub fn game_plugin(app: &mut App) {
    app
        .add_systems(OnEnter(GameState::Game), (
            setup_scene,
            setup_instructions,
            spawn_character,
        ))
        .add_systems(Update, (
            control_character,
            control_player,
            update_camera,
        ).chain().run_if(in_state(GameState::Game)));
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

    // Player
    commands.spawn((
        Player,
        Mesh2d(meshes.add(Circle::new(25.))),
        MeshMaterial2d(materials.add(Color::srgb(6.25, 9.4, 9.1))), // RGB values exceed 1 to achieve a bright color for the bloom effect
        Transform::from_xyz(0., 0., 2.),
    ));
}

fn spawn_character(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut animations: ResMut<Assets<Animation>>,
    mut atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    commands.spawn(Camera2d);

    // Create the animations

    let image = assets.load("sprites/character.png");

    let spritesheet = Spritesheet::new(&image, 8, 8);

    // Idle

    let idle_animation = spritesheet
        .create_animation()
        .add_horizontal_strip(0, 0, 5)
        .build();

    let idle_animation_handle = animations.add(idle_animation);

    // Run

    let run_animation = spritesheet.create_animation().add_row(3).build();

    let run_animation_handle = animations.add(run_animation);

    // Shoot

    let shoot_animation = spritesheet
        .create_animation()
        .add_horizontal_strip(0, 5, 5)
        .build();

    let shoot_animation_handle = animations.add(shoot_animation);

    // Store the animations as a resource

    commands.insert_resource(PlayerAnimations {
        idle: idle_animation_handle.clone(),
        move_right: run_animation_handle.clone(),
        move_left: run_animation_handle,
        shoot: shoot_animation_handle,
    });

    // Spawn the character

    let sprite = spritesheet
        .with_size_hint(768, 768)
        .sprite(&mut atlas_layouts);

    commands.spawn((sprite, SpritesheetAnimation::new(idle_animation_handle)));
}

fn setup_instructions(mut commands: Commands) {
    commands.spawn((
        Text::new("Move the light with WASD.\nThe camera will smoothly track the light."),
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

/// Update the player position with keyboard inputs.
/// Note that the approach used here is for demonstration purposes only,
/// as the point of this example is to showcase the camera tracking feature.
///
/// A more robust solution for player movement can be found in `examples/movement/physics_in_fixed_timestep.rs`.
fn control_player(
    mut player: Single<&mut Transform, With<Player>>,
    time: Res<Time>,
    kb_input: Res<ButtonInput<KeyCode>>,
) {
    let mut direction = Vec2::ZERO;

    if kb_input.pressed(KeyCode::KeyW) {
        direction.y += 1.;
    }

    if kb_input.pressed(KeyCode::KeyS) {
        direction.y -= 1.;
    }

    if kb_input.pressed(KeyCode::KeyA) {
        direction.x -= 1.;
    }

    if kb_input.pressed(KeyCode::KeyD) {
        direction.x += 1.;
    }

    // Progressively update the player's position over time. Normalize the
    // direction vector to prevent it from exceeding a magnitude of 1 when
    // moving diagonally.
    let move_delta = direction.normalize_or_zero() * PLAYER_SPEED * time.delta_secs();
    player.translation += move_delta.extend(0.);
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
        &mut Sprite,
        &mut SpritesheetAnimation,
        &mut Transform,
        Option<&Shooting>,
    )>,
    my_animations: Res<PlayerAnimations>,
    mut messages: MessageReader<AnimationEvent>,
) {
    // Control the character with the keyboard

    const CHARACTER_SPEED: f32 = 150.0;

    let (entity, mut sprite, mut animation, mut transform, shooting) = character.into_inner();

    // If they're shooting, do nothing and wait for the animation to end

    if shooting.is_none() {
        // Shoot with the spacebar
        if keyboard.pressed(KeyCode::Space) {
            // Set the animation

            animation.switch(my_animations.shoot.clone());

            // Add a Shooting component

            commands.entity(entity).insert(Shooting);
        }
        // Run with the arrows
        else if keyboard.pressed(KeyCode::ArrowLeft) || keyboard.pressed(KeyCode::ArrowRight) {
            // Set the animation
            //
            // Only if not already running as we don't want to reset the animation in that case

            if animation.animation != my_animations.move_right {
                animation.switch(my_animations.move_right.clone());
            }

            // Move the entity and flip it horizontally depending on the direction

            let translation = Vec3::X * time.delta_secs() * CHARACTER_SPEED;

            if keyboard.pressed(KeyCode::ArrowLeft) {
                transform.translation -= translation;
                sprite.flip_x = true;
            } else {
                transform.translation += translation;
                sprite.flip_x = false;
            }
        }
        // Idle
        else {
            // Set the animation
            //
            // Only if not already idle as we don't want to reset the animation in that case

            if animation.animation != my_animations.idle {
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