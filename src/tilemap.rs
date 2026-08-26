use bevy::prelude::*;

use crate::map::{MapData, MapTile, TILE_SIZE};

#[derive(Component)]
pub struct TerrainMapTile;

#[derive(Resource)]
pub struct TerrainAtlas {
    image: Handle<Image>,
    layout: Handle<TextureAtlasLayout>,
}

pub struct TerrainTilemapPlugin;

impl Plugin for TerrainTilemapPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, initialize_terrain_atlas);
    }
}

fn initialize_terrain_atlas(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    commands.insert_resource(TerrainAtlas {
        image: asset_server.load("sprites/terrain.png"),
        layout: layouts.add(TextureAtlasLayout::from_grid(
            UVec2::splat(TILE_SIZE as u32),
            6,
            1,
            None,
            None,
        )),
    });
}

pub fn spawn_map(commands: &mut Commands, atlas: &TerrainAtlas, map: &MapData) {
    for tile in &map.tiles {
        spawn_tile(commands, atlas, *tile, map.atlas_index_for(*tile));
    }
}

pub fn spawn_tile(
    commands: &mut Commands,
    atlas: &TerrainAtlas,
    tile: MapTile,
    atlas_index: usize,
) {
    commands.spawn((
        TerrainMapTile,
        Sprite {
            image: atlas.image.clone(),
            texture_atlas: Some(TextureAtlas {
                layout: atlas.layout.clone(),
                index: atlas_index,
            }),
            ..default()
        },
        Transform::from_xyz(tile.x as f32 * TILE_SIZE, tile.y as f32 * TILE_SIZE, 0.0),
    ));
}
