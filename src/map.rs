use crate::progression::Skill;
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MapTile {
    pub x: i32,
    pub y: i32,
    pub tile: TerrainTile,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum MapEntityKind {
    Enemy,
    Terminal { dialogue_id: String },
    SkillPickup { skill: Skill },
    ReinforcedArmorPickup,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MapEntity {
    pub x: i32,
    pub y: i32,
    pub kind: MapEntityKind,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct MapData {
    pub tiles: Vec<MapTile>,
    #[serde(default)]
    pub entities: Vec<MapEntity>,
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
        Self {
            tiles,
            entities: Vec::new(),
        }
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

    pub fn entity_at(&self, x: i32, y: i32) -> Option<&MapEntity> {
        self.entities
            .iter()
            .find(|entry| entry.x == x && entry.y == y)
    }

    pub fn can_place_entity(&self, x: i32, y: i32) -> bool {
        self.tile_at(x, y) == Some(TerrainTile::Floor)
    }

    pub fn place_entity(&mut self, x: i32, y: i32, kind: MapEntityKind) -> Option<MapEntity> {
        if let Some(existing) = self
            .entities
            .iter_mut()
            .find(|entry| entry.x == x && entry.y == y)
        {
            let previous = existing.clone();
            *existing = MapEntity { x, y, kind };
            Some(previous)
        } else {
            self.entities.push(MapEntity { x, y, kind });
            None
        }
    }

    pub fn try_place_entity(&mut self, x: i32, y: i32, kind: MapEntityKind) -> bool {
        if !self.can_place_entity(x, y) {
            return false;
        }
        self.place_entity(x, y, kind);
        true
    }

    pub fn remove_entity(&mut self, x: i32, y: i32) -> bool {
        let original_len = self.entities.len();
        self.entities.retain(|entry| entry.x != x || entry.y != y);
        self.entities.len() != original_len
    }

    pub fn atlas_index_for(&self, tile: MapTile) -> usize {
        match tile.tile {
            TerrainTile::Floor => 0,
            TerrainTile::Wall => {
                let has_wall_south = self.tile_at(tile.x, tile.y - 1) == Some(TerrainTile::Wall);
                if has_wall_south { 2 } else { 1 }
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_ron_without_entities_loads_with_empty_entities() {
        let map: MapData = ron::de::from_str(
            r#"(
                tiles: [
                    (x: 0, y: 0, tile: Floor),
                ],
            )"#,
        )
        .unwrap();

        assert_eq!(map.tile_at(0, 0), Some(TerrainTile::Floor));
        assert!(map.entities.is_empty());
    }

    #[test]
    fn entity_ron_round_trips() {
        let mut map = MapData::default();
        map.set(1, 2, TerrainTile::Floor);
        map.place_entity(1, 2, MapEntityKind::Enemy);

        let serialized = ron::ser::to_string_pretty(&map, ron::ser::PrettyConfig::default())
            .expect("map serializes");
        let loaded: MapData = ron::de::from_str(&serialized).expect("map deserializes");

        assert_eq!(loaded.tile_at(1, 2), Some(TerrainTile::Floor));
        assert_eq!(
            loaded.entity_at(1, 2).cloned(),
            Some(MapEntity {
                x: 1,
                y: 2,
                kind: MapEntityKind::Enemy,
            })
        );
    }

    #[test]
    fn every_entity_variant_round_trips_and_terminal_ids_are_owned() {
        let kinds = [
            MapEntityKind::Enemy,
            MapEntityKind::Terminal {
                dialogue_id: "terminal_intro".into(),
            },
            MapEntityKind::SkillPickup {
                skill: Skill::Projectile,
            },
            MapEntityKind::SkillPickup { skill: Skill::Stun },
            MapEntityKind::SkillPickup {
                skill: Skill::Teleport,
            },
            MapEntityKind::ReinforcedArmorPickup,
        ];
        let mut map = MapData::default();
        for (x, kind) in kinds.into_iter().enumerate() {
            map.set(x as i32, 0, TerrainTile::Floor);
            map.place_entity(x as i32, 0, kind);
        }
        let encoded = ron::ser::to_string(&map).unwrap();
        let decoded: MapData = ron::de::from_str(&encoded).unwrap();
        assert_eq!(decoded.entities, map.entities);
        let terminal = decoded.entity_at(1, 0).unwrap().clone();
        assert_eq!(
            terminal.kind,
            MapEntityKind::Terminal {
                dialogue_id: "terminal_intro".into()
            }
        );
    }

    #[test]
    fn old_enemy_only_ron_remains_compatible() {
        let map: MapData = ron::de::from_str(
            "(tiles: [(x: 0, y: 0, tile: Floor)], entities: [(x: 0, y: 0, kind: Enemy)])",
        )
        .unwrap();
        assert_eq!(map.entity_at(0, 0).unwrap().kind, MapEntityKind::Enemy);
    }

    #[test]
    fn placing_entity_is_idempotent_and_replaces_same_tile() {
        let mut map = MapData::default();

        assert_eq!(map.place_entity(3, 4, MapEntityKind::Enemy), None);
        assert_eq!(map.entities.len(), 1);
        assert_eq!(
            map.place_entity(3, 4, MapEntityKind::Enemy),
            Some(MapEntity {
                x: 3,
                y: 4,
                kind: MapEntityKind::Enemy,
            })
        );
        assert_eq!(map.entities.len(), 1);
        assert_eq!(
            map.entity_at(3, 4).cloned(),
            Some(MapEntity {
                x: 3,
                y: 4,
                kind: MapEntityKind::Enemy,
            })
        );
    }

    #[test]
    fn terrain_and_entity_layers_are_independent() {
        let mut map = MapData::default();
        map.set(0, 0, TerrainTile::Floor);
        map.place_entity(0, 0, MapEntityKind::Enemy);

        assert!(map.remove(0, 0));
        assert_eq!(map.tile_at(0, 0), None);
        assert_eq!(
            map.entity_at(0, 0).cloned(),
            Some(MapEntity {
                x: 0,
                y: 0,
                kind: MapEntityKind::Enemy,
            })
        );

        map.set(0, 0, TerrainTile::Wall);
        assert!(map.remove_entity(0, 0));
        assert_eq!(map.tile_at(0, 0), Some(TerrainTile::Wall));
        assert_eq!(map.entity_at(0, 0), None);
    }

    #[test]
    fn removing_missing_entity_returns_false() {
        let mut map = MapData::default();

        assert!(!map.remove_entity(0, 0));
    }

    #[test]
    fn enemy_placement_requires_explicit_floor_tile() {
        let mut map = MapData::default();
        map.set(0, 0, TerrainTile::Floor);
        map.set(1, 0, TerrainTile::Wall);

        assert!(map.can_place_entity(0, 0));
        assert!(!map.can_place_entity(1, 0));
        assert!(!map.can_place_entity(2, 0));

        assert!(map.try_place_entity(0, 0, MapEntityKind::Enemy));
        assert!(!map.try_place_entity(1, 0, MapEntityKind::Enemy));
        assert!(!map.try_place_entity(2, 0, MapEntityKind::Enemy));
        assert_eq!(map.entities.len(), 1);
        assert_eq!(
            map.entity_at(0, 0).map(|entity| entity.kind.clone()),
            Some(MapEntityKind::Enemy)
        );
    }

    #[test]
    fn initial_map_entities_are_on_floor_tiles() {
        let map = load_map("initial").expect("initial map loads");

        assert!(!map.entities.is_empty());
        for entity in &map.entities {
            assert_eq!(
                map.tile_at(entity.x, entity.y),
                Some(TerrainTile::Floor),
                "entity should be on an explicit floor tile: {entity:?}"
            );
        }
    }
}
