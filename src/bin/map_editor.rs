use bevy::{
    input::mouse::{MouseMotion, MouseWheel},
    prelude::*,
};
use map_support::{
    dialogue::{DialogueCatalog, load_dialogue_catalog},
    map::{self, MapData, MapEntityKind, TILE_SIZE, TerrainTile},
    progression::Skill,
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
    dialogue_ids: Vec<String>,
    selected_dialogue: usize,
    catalogue_error: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PaletteSelection {
    Terrain(TerrainTile),
    PlayerSpawn,
    Entity(EntityPalette),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EntityPalette {
    Enemy,
    Terminal,
    Projectile,
    Stun,
    Teleport,
    ReinforcedArmor,
}

impl EntityPalette {
    const ALL: [(Self, &'static str, KeyCode); 6] = [
        (Self::Enemy, "Enemy", KeyCode::Digit3),
        (Self::Terminal, "Terminal", KeyCode::Digit4),
        (Self::Projectile, "Projectile", KeyCode::Digit5),
        (Self::Stun, "Stun", KeyCode::Digit6),
        (Self::Teleport, "Teleport", KeyCode::Digit7),
        (Self::ReinforcedArmor, "Armor", KeyCode::Digit8),
    ];

    fn kind(self, dialogue_id: Option<&str>) -> Option<MapEntityKind> {
        Some(match self {
            Self::Enemy => MapEntityKind::Enemy,
            Self::Terminal => MapEntityKind::Terminal {
                dialogue_id: dialogue_id?.to_owned(),
            },
            Self::Projectile => MapEntityKind::SkillPickup {
                skill: Skill::Projectile,
            },
            Self::Stun => MapEntityKind::SkillPickup { skill: Skill::Stun },
            Self::Teleport => MapEntityKind::SkillPickup {
                skill: Skill::Teleport,
            },
            Self::ReinforcedArmor => MapEntityKind::ReinforcedArmorPickup,
        })
    }
}

#[derive(Component)]
struct PaletteButton(PaletteSelection);
#[derive(Component)]
struct EntityMarker;
#[derive(Component)]
struct PaletteHelp;

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "initial".into());
    let map = map::load_map(&name).unwrap_or_else(|error| {
        eprintln!("Starting a new map '{name}': {error}");
        MapData::default()
    });
    let (dialogue_ids, catalogue_error) = match load_dialogue_catalog("story") {
        Ok(catalog) => {
            let ids = catalogue_ids(&catalog);
            let error = validate_terminal_references(&map, &ids).err();
            (ids, error)
        }
        Err(error) => (
            Vec::new(),
            Some(format!("Could not load assets/dialogue/story.ron: {error}")),
        ),
    };

    App::new()
        .insert_resource(EditorMap {
            name,
            map,
            selected: PaletteSelection::Terrain(TerrainTile::Floor),
            dialogue_ids,
            selected_dialogue: 0,
            catalogue_error,
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
                update_palette_help,
                zoom_camera,
                pan_camera,
            ),
        )
        .run();
}

fn catalogue_ids(catalog: &DialogueCatalog) -> Vec<String> {
    catalog.conversations.keys().cloned().collect()
}

fn setup(mut commands: Commands, atlas: Res<TerrainAtlas>, editor_map: Res<EditorMap>) {
    commands.spawn(Camera2d);
    tilemap::spawn_map(&mut commands, &atlas, &editor_map.map);
    spawn_entity_markers(&mut commands, &editor_map.map);

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            bottom: px(12),
            left: px(0),
            width: percent(100),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            flex_wrap: FlexWrap::Wrap,
            column_gap: px(6),
            row_gap: px(6),
            ..default()
        },
        children![
            palette_button(PaletteSelection::Terrain(TerrainTile::Floor), "1 Floor"),
            palette_button(PaletteSelection::Terrain(TerrainTile::Wall), "2 Wall"),
            palette_button(PaletteSelection::Entity(EntityPalette::Enemy), "3 Enemy"),
            palette_button(
                PaletteSelection::Entity(EntityPalette::Terminal),
                "4 Terminal"
            ),
            palette_button(
                PaletteSelection::Entity(EntityPalette::Projectile),
                "5 Projectile"
            ),
            palette_button(PaletteSelection::Entity(EntityPalette::Stun), "6 Stun"),
            palette_button(
                PaletteSelection::Entity(EntityPalette::Teleport),
                "7 Teleport"
            ),
            palette_button(
                PaletteSelection::Entity(EntityPalette::ReinforcedArmor),
                "8 Armor"
            ),
            palette_button(PaletteSelection::PlayerSpawn, "9 Spawn"),
            (
                Text::new("Ctrl+S save"),
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextColor(Color::WHITE),
            ),
        ],
    ));
    commands.spawn((
        PaletteHelp,
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(16.0),
            ..default()
        },
        TextColor(Color::WHITE),
        Node {
            position_type: PositionType::Absolute,
            top: px(12),
            left: px(12),
            ..default()
        },
    ));
}

fn palette_button(selection: PaletteSelection, label: &'static str) -> impl Bundle {
    (
        Button,
        Node {
            width: px(90),
            height: px(36),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(NORMAL_BUTTON),
        PaletteButton(selection),
        children![(
            Text::new(label),
            TextFont {
                font_size: FontSize::Px(13.0),
                ..default()
            },
            TextColor(Color::WHITE),
        )],
    )
}

fn redraw_map(
    commands: &mut Commands,
    atlas: &TerrainAtlas,
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
    spawn_entity_markers(commands, map);
}

fn spawn_entity_markers(commands: &mut Commands, map: &MapData) {
    if let Some(spawn) = map.player_spawn {
        spawn_marker(
            commands,
            spawn.x,
            spawn.y,
            Color::srgba(0.2, 0.9, 0.35, 0.8),
            "SPAWN",
            TILE_SIZE * 0.7,
            0.9,
        );
    }
    for entity in &map.entities {
        let (color, label) = marker_style(&entity.kind);
        spawn_marker(
            commands,
            entity.x,
            entity.y,
            color,
            label,
            TILE_SIZE * 0.52,
            1.0,
        );
    }
}

fn spawn_marker(
    commands: &mut Commands,
    x: i32,
    y: i32,
    color: Color,
    label: &'static str,
    size: f32,
    z: f32,
) {
    commands.spawn((
        EntityMarker,
        Sprite {
            color,
            custom_size: Some(Vec2::splat(size)),
            ..default()
        },
        Transform::from_xyz(x as f32 * TILE_SIZE, y as f32 * TILE_SIZE, z),
        children![(
            Text2d::new(label),
            TextFont {
                font_size: FontSize::Px(15.0),
                ..default()
            },
            TextColor(Color::WHITE),
            Transform::from_xyz(0.0, 0.0, 1.0),
        )],
    ));
}

fn marker_style(kind: &MapEntityKind) -> (Color, &'static str) {
    match kind {
        MapEntityKind::Enemy => (Color::srgb(0.75, 0.12, 0.12), "ENEMY"),
        MapEntityKind::Terminal { .. } => (Color::srgb(0.15, 0.55, 0.9), "TERM"),
        MapEntityKind::SkillPickup {
            skill: Skill::Projectile,
        } => (Color::srgb(0.9, 0.7, 0.1), "SHOT"),
        MapEntityKind::SkillPickup { skill: Skill::Stun } => (Color::srgb(0.65, 0.25, 0.9), "STUN"),
        MapEntityKind::SkillPickup {
            skill: Skill::Teleport,
        } => (Color::srgb(0.1, 0.8, 0.7), "WARP"),
        MapEntityKind::ReinforcedArmorPickup => (Color::srgb(0.45, 0.75, 0.45), "ARMOR"),
    }
}

fn palette_input(keyboard: Res<ButtonInput<KeyCode>>, mut editor_map: ResMut<EditorMap>) {
    if keyboard.just_pressed(KeyCode::Digit1) {
        editor_map.selected = PaletteSelection::Terrain(TerrainTile::Floor);
    }
    if keyboard.just_pressed(KeyCode::Digit2) {
        editor_map.selected = PaletteSelection::Terrain(TerrainTile::Wall);
    }
    if keyboard.just_pressed(KeyCode::Digit9) {
        editor_map.selected = PaletteSelection::PlayerSpawn;
    }
    for (entity, _, key) in EntityPalette::ALL {
        if keyboard.just_pressed(key) {
            editor_map.selected = PaletteSelection::Entity(entity);
        }
    }
    if editor_map.selected == PaletteSelection::Entity(EntityPalette::Terminal)
        && !editor_map.dialogue_ids.is_empty()
    {
        if keyboard.just_pressed(KeyCode::BracketLeft) {
            editor_map.selected_dialogue =
                (editor_map.selected_dialogue + editor_map.dialogue_ids.len() - 1)
                    % editor_map.dialogue_ids.len();
        }
        if keyboard.just_pressed(KeyCode::BracketRight) {
            editor_map.selected_dialogue =
                (editor_map.selected_dialogue + 1) % editor_map.dialogue_ids.len();
        }
    }
}

fn palette_button_interaction(
    mut buttons: Query<(&Interaction, &PaletteButton, &mut BackgroundColor), With<Button>>,
    mut editor_map: ResMut<EditorMap>,
) {
    for (interaction, button, mut color) in &mut buttons {
        if *interaction == Interaction::Pressed {
            editor_map.selected = button.0.clone();
        }
        *color = if editor_map.selected == button.0 {
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
    markers: Query<Entity, With<EntityMarker>>,
    palette_buttons: Query<&Interaction, With<PaletteButton>>,
) {
    if palette_buttons.iter().any(|i| *i == Interaction::Pressed)
        || (!mouse.just_pressed(MouseButton::Left) && !mouse.just_pressed(MouseButton::Right))
    {
        return;
    }
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Ok(position) = camera.0.viewport_to_world_2d(camera.1, cursor) else {
        return;
    };
    let (x, y) = (
        (position.x / TILE_SIZE).round() as i32,
        (position.y / TILE_SIZE).round() as i32,
    );
    let selected = editor_map.selected.clone();
    match (&selected, mouse.just_pressed(MouseButton::Left)) {
        (PaletteSelection::Terrain(tile), true) => editor_map.map.set(x, y, *tile),
        (PaletteSelection::Terrain(_), false) => {
            editor_map.map.remove(x, y);
        }
        (PaletteSelection::PlayerSpawn, true) => {
            editor_map.map.try_set_player_spawn(x, y);
        }
        (PaletteSelection::PlayerSpawn, false) => {
            editor_map.map.remove_player_spawn(x, y);
        }
        (PaletteSelection::Entity(entity), true) => {
            let dialogue_id = editor_map
                .dialogue_ids
                .get(editor_map.selected_dialogue)
                .map(String::as_str);
            if let Some(kind) = entity.kind(dialogue_id) {
                editor_map.map.try_place_entity(x, y, kind);
            }
        }
        (PaletteSelection::Entity(_), false) => {
            editor_map.map.remove_entity(x, y);
        }
    }
    redraw_map(&mut commands, &atlas, &editor_map.map, &tiles, &markers);
}

fn validate_terminal_references(map: &MapData, dialogue_ids: &[String]) -> Result<(), String> {
    for entity in &map.entities {
        if let MapEntityKind::Terminal { dialogue_id } = &entity.kind {
            if !dialogue_ids.iter().any(|id| id == dialogue_id) {
                return Err(format!(
                    "terminal at ({}, {}) references missing dialogue ID '{dialogue_id}'",
                    entity.x, entity.y
                ));
            }
        }
    }
    Ok(())
}

fn save_map(keyboard: Res<ButtonInput<KeyCode>>, editor_map: Res<EditorMap>) {
    if keyboard.pressed(KeyCode::ControlLeft) && keyboard.just_pressed(KeyCode::KeyS) {
        if let Some(error) = &editor_map.catalogue_error {
            error!("Cannot save: {error}");
            return;
        }
        if let Err(error) = validate_terminal_references(&editor_map.map, &editor_map.dialogue_ids)
        {
            error!("Cannot save: {error}");
            return;
        }
        match map::save_map(&editor_map.name, &editor_map.map) {
            Ok(()) => info!("Saved map '{}'", editor_map.name),
            Err(error) => error!("Could not save map '{}': {error}", editor_map.name),
        }
    }
}

fn update_palette_help(editor_map: Res<EditorMap>, mut text: Single<&mut Text, With<PaletteHelp>>) {
    text.0 = match &editor_map.catalogue_error {
        Some(error) => error.clone(),
        None if editor_map.selected == PaletteSelection::Entity(EntityPalette::Terminal) => format!(
            "Terminal dialogue: {}  ([ / ] to cycle)",
            editor_map.dialogue_ids.get(editor_map.selected_dialogue).map(String::as_str).unwrap_or("no valid dialogue IDs"),
        ),
        None if editor_map.selected == PaletteSelection::PlayerSpawn => {
            "Player spawn requires an explicit floor tile. Left click moves it; right click removes it.".into()
        }
        None => "Entity placement requires an explicit floor tile. Right click removes only entities in entity modes.".into(),
    };
}

fn pan_camera(
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window>,
    mut events: MessageReader<MouseMotion>,
    mut camera: Single<(&mut Transform, &Projection), With<Camera2d>>,
) {
    if !mouse.pressed(MouseButton::Middle) {
        events.clear();
        return;
    }
    let scale = match camera.1 {
        Projection::Orthographic(projection) => projection.scale,
        _ => 1.0,
    };
    for event in events.read() {
        camera.0.translation += Vec2::new(-event.delta.x, event.delta.y).extend(0.0) * scale
            / window.scale_factor() as f32;
    }
}

fn zoom_camera(
    mut events: MessageReader<MouseWheel>,
    mut camera: Single<&mut Projection, With<Camera2d>>,
) {
    for event in events.read() {
        if let Projection::Orthographic(projection) = &mut **camera {
            projection.scale = (projection.scale - event.y * 0.1).clamp(0.25, 4.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_references_require_catalogue_id() {
        let map = MapData {
            tiles: vec![],
            entities: vec![map_support::map::MapEntity {
                x: 1,
                y: 2,
                kind: MapEntityKind::Terminal {
                    dialogue_id: "missing".into(),
                },
            }],
            player_spawn: None,
        };
        assert!(
            validate_terminal_references(&map, &["known".into()])
                .unwrap_err()
                .contains("missing dialogue ID 'missing'")
        );
        assert!(validate_terminal_references(&map, &["missing".into()]).is_ok());
    }

    #[test]
    fn terminal_palette_uses_selected_human_readable_id() {
        assert_eq!(
            EntityPalette::Terminal.kind(Some("terminal_intro")),
            Some(MapEntityKind::Terminal {
                dialogue_id: "terminal_intro".into()
            })
        );
    }
}
