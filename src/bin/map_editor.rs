use bevy::{
    input::mouse::{MouseMotion, MouseWheel},
    prelude::*,
};
use map_support::{
    map::{self, MapData, TILE_SIZE, TerrainTile},
    tilemap::{self, TerrainAtlas, TerrainMapTile, TerrainTilemapPlugin},
};

const NORMAL_BUTTON: Color = Color::srgb(0.16, 0.16, 0.16);
const SELECTED_BUTTON: Color = Color::srgb(0.15, 0.55, 0.3);
const HOVERED_BUTTON: Color = Color::srgb(0.3, 0.4, 0.55);

#[derive(Resource)]
struct EditorMap {
    name: String,
    map: MapData,
    selected: TerrainTile,
}

#[derive(Component)]
struct PaletteButton(TerrainTile);

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "initial".into());
    let map = map::load_map(&name).unwrap_or_else(|error| {
        eprintln!("Starting a new map '{name}': {error}");
        MapData::default()
    });

    App::new()
        .insert_resource(EditorMap {
            name,
            map,
            selected: TerrainTile::Floor,
        })
        .add_plugins((
            DefaultPlugins.set(ImagePlugin::default_nearest()),
            TerrainTilemapPlugin,
        ))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                palette_input,
                palette_button_interaction,
                paint_map,
                save_map,
                zoom_camera,
                pan_camera,
            ),
        )
        .run();
}

fn setup(mut commands: Commands, atlas: Res<TerrainAtlas>, editor_map: Res<EditorMap>) {
    commands.spawn(Camera2d);
    tilemap::spawn_map(&mut commands, &atlas, &editor_map.map);

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(16),
            left: px(16),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(px(12)),
            row_gap: px(8),
            ..default()
        },
        BackgroundColor(Color::srgb(0.08, 0.08, 0.1)),
        children![
            (
                Text::new("Terrain Palette\n1: Floor   2: Wall\nLeft click: paint\nRight click: delete\nCtrl+S: save"),
                TextFont { font_size: FontSize::Px(18.0), ..default() },
                TextColor(Color::WHITE),
            ),
            (
                Button,
                palette_button_node(),
                BackgroundColor(SELECTED_BUTTON),
                PaletteButton(TerrainTile::Floor),
                children![(Text::new("Floor (1)"), TextColor(Color::WHITE))]
            ),
            (
                Button,
                palette_button_node(),
                BackgroundColor(NORMAL_BUTTON),
                PaletteButton(TerrainTile::Wall),
                children![(Text::new("Wall (2)"), TextColor(Color::WHITE))]
            ),
        ],
    ));
}

fn palette_button_node() -> Node {
    Node {
        width: px(140),
        height: px(38),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    }
}

fn palette_input(keyboard: Res<ButtonInput<KeyCode>>, mut editor_map: ResMut<EditorMap>) {
    if keyboard.just_pressed(KeyCode::Digit1) {
        editor_map.selected = TerrainTile::Floor;
    }
    if keyboard.just_pressed(KeyCode::Digit2) {
        editor_map.selected = TerrainTile::Wall;
    }
}

fn palette_button_interaction(
    mut buttons: Query<(&Interaction, &PaletteButton, &mut BackgroundColor), With<Button>>,
    mut editor_map: ResMut<EditorMap>,
) {
    for (interaction, palette_button, mut color) in &mut buttons {
        if *interaction == Interaction::Pressed {
            editor_map.selected = palette_button.0;
        }
        *color = if editor_map.selected == palette_button.0 {
            SELECTED_BUTTON.into()
        } else if *interaction == Interaction::Hovered {
            HOVERED_BUTTON.into()
        } else {
            NORMAL_BUTTON.into()
        };
    }
}

fn paint_map(
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window>,
    camera: Single<(&Camera, &GlobalTransform), With<Camera2d>>,
    atlas: Res<TerrainAtlas>,
    mut commands: Commands,
    mut editor_map: ResMut<EditorMap>,
    tiles: Query<Entity, With<TerrainMapTile>>,
    palette_buttons: Query<&Interaction, With<PaletteButton>>,
) {
    if palette_buttons
        .iter()
        .any(|interaction| *interaction == Interaction::Pressed)
        || (!mouse.just_pressed(MouseButton::Left) && !mouse.just_pressed(MouseButton::Right))
    {
        return;
    }
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Ok(world_position) = camera.0.viewport_to_world_2d(camera.1, cursor) else {
        return;
    };
    let x = (world_position.x / TILE_SIZE).round() as i32;
    let y = (world_position.y / TILE_SIZE).round() as i32;

    if mouse.just_pressed(MouseButton::Left) {
        let selected = editor_map.selected;
        editor_map.map.set(x, y, selected);
    } else {
        editor_map.map.remove(x, y);
    }

    for entity in &tiles {
        commands.entity(entity).despawn();
    }
    tilemap::spawn_map(&mut commands, &atlas, &editor_map.map);
}

fn save_map(keyboard: Res<ButtonInput<KeyCode>>, editor_map: Res<EditorMap>) {
    if keyboard.pressed(KeyCode::ControlLeft) && keyboard.just_pressed(KeyCode::KeyS) {
        match map::save_map(&editor_map.name, &editor_map.map) {
            Ok(()) => info!("Saved map '{}'", editor_map.name),
            Err(error) => error!("Could not save map '{}': {error}", editor_map.name),
        }
    }
}

fn pan_camera(
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window>,
    mut mouse_motion_events: MessageReader<MouseMotion>,
    mut camera: Single<(&mut Transform, &Projection), With<Camera2d>>,
) {
    if !mouse.pressed(MouseButton::Middle) {
        mouse_motion_events.clear();
        return;
    }

    let scale = match camera.1 {
        Projection::Orthographic(projection) => projection.scale,
        _ => 1.0,
    };
    // MouseMotion uses physical pixels while the camera viewport uses logical pixels.
    // Convert between them so panning stays one-to-one at any display scale factor.
    let logical_scale = scale / window.scale_factor() as f32;
    for event in mouse_motion_events.read() {
        // Keep the map under the cursor: screen Y grows downward, world Y grows upward.
        let pan_delta = Vec2::new(-event.delta.x, event.delta.y) * logical_scale;
        camera.0.translation += pan_delta.extend(0.0);
    }
}

fn zoom_camera(
    mut mouse_wheel_events: MessageReader<MouseWheel>,
    mut camera: Single<&mut Projection, With<Camera2d>>,
) {
    for event in mouse_wheel_events.read() {
        if let Projection::Orthographic(projection) = &mut **camera {
            projection.scale = (projection.scale - event.y * 0.1).clamp(0.25, 4.0);
        }
    }
}
