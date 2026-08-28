use bevy::{
    input::mouse::{MouseMotion, MouseWheel},
    prelude::*,
};
use map_support::{
    map::{self, MapData, MapEntityKind, TILE_SIZE, TerrainTile},
    tilemap::{self, TerrainAtlas, TerrainMapTile, TerrainTilemapPlugin},
};

const NORMAL_BUTTON: Color = Color::srgb(0.16, 0.16, 0.16);
const SELECTED_BUTTON: Color = Color::srgb(0.15, 0.55, 0.3);
const HOVERED_BUTTON: Color = Color::srgb(0.3, 0.4, 0.55);

#[derive(Resource)]
struct EditorMap {
    name: String,
    map: MapData,
    selected: PaletteSelection,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PaletteSelection {
    Terrain(TerrainTile),
    Enemy,
}

#[derive(Component)]
struct PaletteButton(PaletteSelection);

#[derive(Component)]
struct EntityMarker;

#[derive(Resource)]
struct EnemyPlaceholderAtlas {
    image: Handle<Image>,
    layout: Handle<TextureAtlasLayout>,
}

const ENEMY_PLACEHOLDER_CELL_SIZE: u32 = 64;
const ENEMY_PLACEHOLDER_COLUMNS: u32 = 4;
const ENEMY_PLACEHOLDER_ROWS: u32 = 4;

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
            selected: PaletteSelection::Terrain(TerrainTile::Floor),
        })
        .add_plugins((
            DefaultPlugins.set(ImagePlugin::default_nearest()),
            TerrainTilemapPlugin,
        ))
        .add_systems(PreStartup, initialize_enemy_placeholder_atlas)
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

fn initialize_enemy_placeholder_atlas(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    commands.insert_resource(EnemyPlaceholderAtlas {
        image: asset_server.load("sprites/enemy_placeholder.png"),
        layout: layouts.add(TextureAtlasLayout::from_grid(
            UVec2::splat(ENEMY_PLACEHOLDER_CELL_SIZE),
            ENEMY_PLACEHOLDER_COLUMNS,
            ENEMY_PLACEHOLDER_ROWS,
            None,
            None,
        )),
    });
}

fn setup(
    mut commands: Commands,
    atlas: Res<TerrainAtlas>,
    enemy_atlas: Res<EnemyPlaceholderAtlas>,
    editor_map: Res<EditorMap>,
) {
    commands.spawn(Camera2d);
    tilemap::spawn_map(&mut commands, &atlas, &editor_map.map);
    spawn_entity_markers(&mut commands, &editor_map.map, &enemy_atlas);

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            bottom: px(12),
            left: px(0),
            width: percent(100),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            column_gap: px(8),
            ..default()
        },
        children![
            (
                Button,
                palette_button_node(),
                BackgroundColor(SELECTED_BUTTON),
                PaletteButton(PaletteSelection::Terrain(TerrainTile::Floor)),
                children![
                    (
                        ImageNode::from_atlas_image(
                            atlas.image(),
                            TextureAtlas {
                                layout: atlas.layout(),
                                index: 0
                            }
                        ),
                        palette_icon_node(),
                    ),
                    palette_hotkey_label("1"),
                ]
            ),
            (
                Button,
                palette_button_node(),
                BackgroundColor(NORMAL_BUTTON),
                PaletteButton(PaletteSelection::Terrain(TerrainTile::Wall)),
                children![
                    (
                        ImageNode::from_atlas_image(
                            atlas.image(),
                            TextureAtlas {
                                layout: atlas.layout(),
                                index: 1
                            }
                        ),
                        palette_icon_node(),
                    ),
                    palette_hotkey_label("2"),
                ]
            ),
            (
                Button,
                palette_button_node(),
                BackgroundColor(NORMAL_BUTTON),
                PaletteButton(PaletteSelection::Enemy),
                children![
                    (
                        ImageNode::from_atlas_image(
                            enemy_atlas.image.clone(),
                            TextureAtlas {
                                layout: enemy_atlas.layout.clone(),
                                index: 0,
                            },
                        ),
                        enemy_icon_node(),
                    ),
                    palette_hotkey_label("3"),
                ]
            ),
            (
                Text::new("Ctrl+S save"),
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextColor(Color::srgba(1.0, 1.0, 1.0, 0.75)),
            ),
        ],
    ));
}

fn redraw_map(
    commands: &mut Commands,
    atlas: &TerrainAtlas,
    enemy_atlas: &EnemyPlaceholderAtlas,
    map: &MapData,
    tiles: &Query<Entity, With<TerrainMapTile>>,
    markers: &Query<Entity, With<EntityMarker>>,
) {
    for entity in tiles {
        commands.entity(entity).despawn();
    }
    for entity in markers {
        commands.entity(entity).despawn();
    }
    tilemap::spawn_map(commands, atlas, map);
    spawn_entity_markers(commands, map, enemy_atlas);
}

fn spawn_entity_markers(
    commands: &mut Commands,
    map: &MapData,
    enemy_atlas: &EnemyPlaceholderAtlas,
) {
    for entity in &map.entities {
        match entity.kind {
            MapEntityKind::Enemy => {
                commands.spawn((
                    EntityMarker,
                    Sprite {
                        image: enemy_atlas.image.clone(),
                        texture_atlas: Some(TextureAtlas {
                            layout: enemy_atlas.layout.clone(),
                            index: 0,
                        }),
                        custom_size: Some(Vec2::splat(TILE_SIZE * 0.45)),
                        ..default()
                    },
                    Transform::from_xyz(
                        entity.x as f32 * TILE_SIZE,
                        entity.y as f32 * TILE_SIZE,
                        1.0,
                    ),
                ));
            }
        }
    }
}

fn palette_button_node() -> Node {
    Node {
        width: px(56),
        height: px(56),
        padding: UiRect::all(px(4)),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        overflow: Overflow::clip(),
        ..default()
    }
}

fn palette_icon_node() -> Node {
    Node {
        width: percent(100),
        height: percent(100),
        ..default()
    }
}

fn enemy_icon_node() -> Node {
    Node {
        width: percent(70),
        height: percent(70),
        ..default()
    }
}

fn palette_hotkey_label(label: &'static str) -> impl Bundle {
    (
        Text::new(label),
        TextFont {
            font_size: FontSize::Px(18.0),
            ..default()
        },
        TextColor(Color::WHITE),
        TextShadow::default(),
        Node {
            position_type: PositionType::Absolute,
            right: px(5),
            bottom: px(1),
            ..default()
        },
    )
}

fn palette_input(keyboard: Res<ButtonInput<KeyCode>>, mut editor_map: ResMut<EditorMap>) {
    if keyboard.just_pressed(KeyCode::Digit1) {
        editor_map.selected = PaletteSelection::Terrain(TerrainTile::Floor);
    }
    if keyboard.just_pressed(KeyCode::Digit2) {
        editor_map.selected = PaletteSelection::Terrain(TerrainTile::Wall);
    }
    if keyboard.just_pressed(KeyCode::Digit3) {
        editor_map.selected = PaletteSelection::Enemy;
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
    enemy_atlas: Res<EnemyPlaceholderAtlas>,
    mut commands: Commands,
    mut editor_map: ResMut<EditorMap>,
    tiles: Query<Entity, With<TerrainMapTile>>,
    markers: Query<Entity, With<EntityMarker>>,
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

    match (editor_map.selected, mouse.just_pressed(MouseButton::Left)) {
        (PaletteSelection::Terrain(selected), true) => editor_map.map.set(x, y, selected),
        (PaletteSelection::Terrain(_), false) => {
            editor_map.map.remove(x, y);
        }
        (PaletteSelection::Enemy, true) => {
            editor_map.map.try_place_entity(x, y, MapEntityKind::Enemy);
        }
        (PaletteSelection::Enemy, false) => {
            editor_map.map.remove_entity(x, y);
        }
    }

    redraw_map(
        &mut commands,
        &atlas,
        &enemy_atlas,
        &editor_map.map,
        &tiles,
        &markers,
    );
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
