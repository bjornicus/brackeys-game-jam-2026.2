use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub const TILE_SIZE: f32 = 96.0;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum TerrainTile {
    #[default]
    Floor,
    Wall,
}

impl TerrainTile {
    pub const fn atlas_index(self) -> usize {
        match self {
            Self::Floor => 0,
            Self::Wall => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MapTile {
    pub x: i32,
    pub y: i32,
    pub tile: TerrainTile,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct MapData {
    pub tiles: Vec<MapTile>,
}

impl MapData {
    pub fn initial() -> Self {
        let mut tiles = Vec::with_capacity(100);
        for y in -5..5 {
            for x in -5..5 {
                let tile = if x == -5 || x == 4 || y == -5 || y == 4 {
                    TerrainTile::Wall
                } else {
                    TerrainTile::Floor
                };
                tiles.push(MapTile { x, y, tile });
            }
        }
        Self { tiles }
    }

    pub fn tile_at(&self, x: i32, y: i32) -> Option<TerrainTile> {
        self.tiles
            .iter()
            .find(|entry| entry.x == x && entry.y == y)
            .map(|entry| entry.tile)
    }

    pub fn set(&mut self, x: i32, y: i32, tile: TerrainTile) {
        if let Some(existing) = self
            .tiles
            .iter_mut()
            .find(|entry| entry.x == x && entry.y == y)
        {
            existing.tile = tile;
        } else {
            self.tiles.push(MapTile { x, y, tile });
        }
    }

    pub fn remove(&mut self, x: i32, y: i32) -> bool {
        let original_len = self.tiles.len();
        self.tiles.retain(|entry| entry.x != x || entry.y != y);
        self.tiles.len() != original_len
    }
}

pub fn map_path(name: &str) -> io::Result<PathBuf> {
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid map name",
        ));
    }
    Ok(Path::new("assets/maps").join(format!("{name}.ron")))
}

pub fn load_map(name: &str) -> Result<MapData, Box<dyn std::error::Error>> {
    let contents = fs::read_to_string(map_path(name)?)?;
    Ok(ron::de::from_str(&contents)?)
}

pub fn save_map(name: &str, map: &MapData) -> Result<(), Box<dyn std::error::Error>> {
    let path = map_path(name)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        path,
        ron::ser::to_string_pretty(map, ron::ser::PrettyConfig::default())?,
    )?;
    Ok(())
}
