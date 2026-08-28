use bevy::math::Vec2;

use crate::map::{MapData, TILE_SIZE, TerrainTile};

const TILE_EDGE_EPSILON: f32 = 0.001;

/// A local-space collider that can be placed in world space by adding its
/// `offset` to an entity position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Collider {
    pub size: Vec2,
    pub offset: Vec2,
}

impl Collider {
    pub const fn new(size: Vec2, offset: Vec2) -> Self {
        Self { size, offset }
    }

    pub fn aabb_at(self, position: Vec2) -> Aabb {
        Aabb::from_center_size(position + self.offset, self.size)
    }
}

/// World-space axis-aligned bounding box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec2,
    pub max: Vec2,
}

impl Aabb {
    pub fn from_center_size(center: Vec2, size: Vec2) -> Self {
        let half_size = size / 2.0;
        Self {
            min: center - half_size,
            max: center + half_size,
        }
    }

    pub fn intersects(self, other: Self) -> bool {
        self.min.x < other.max.x
            && self.max.x > other.min.x
            && self.min.y < other.max.y
            && self.max.y > other.min.y
    }
}

/// Shared occupancy query for movement and future teleport/enemy collision.
///
/// Terrain is the only blocker currently used by gameplay. `dynamic_blockers` is
/// an explicit extension point for future movement-blocking entities without
/// duplicating terrain/collider rules at each call site.
pub struct Occupancy<'a> {
    pub map: &'a MapData,
    pub dynamic_blockers: &'a [Aabb],
}

impl<'a> Occupancy<'a> {
    pub fn terrain_only(map: &'a MapData) -> Self {
        Self {
            map,
            dynamic_blockers: &[],
        }
    }
}

pub fn can_occupy(position: Vec2, collider: Collider, occupancy: &Occupancy<'_>) -> bool {
    let aabb = collider.aabb_at(position);

    if !terrain_allows(aabb, occupancy.map) {
        return false;
    }

    !occupancy
        .dynamic_blockers
        .iter()
        .any(|blocker| aabb.intersects(*blocker))
}

pub fn move_axis_separated(
    position: Vec2,
    movement: Vec2,
    collider: Collider,
    occupancy: &Occupancy<'_>,
) -> Vec2 {
    let mut next = position;

    let horizontal = Vec2::new(movement.x, 0.0);
    if can_occupy(next + horizontal, collider, occupancy) {
        next += horizontal;
    }

    let vertical = Vec2::new(0.0, movement.y);
    if can_occupy(next + vertical, collider, occupancy) {
        next += vertical;
    }

    next
}

fn terrain_allows(aabb: Aabb, map: &MapData) -> bool {
    let half_tile = TILE_SIZE / 2.0;

    // Tiles are centered on integer grid coordinates, so shift by half a tile before
    // converting world positions to grid positions. The tiny epsilon keeps a collider
    // that exactly touches an edge from entering the neighboring tile.
    let min_x = ((aabb.min.x + half_tile) / TILE_SIZE).floor() as i32;
    let max_x = ((aabb.max.x + half_tile - TILE_EDGE_EPSILON) / TILE_SIZE).floor() as i32;
    let min_y = ((aabb.min.y + half_tile) / TILE_SIZE).floor() as i32;
    let max_y = ((aabb.max.y + half_tile - TILE_EDGE_EPSILON) / TILE_SIZE).floor() as i32;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            if map.tile_at(x, y) != Some(TerrainTile::Floor) {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::MapTile;

    fn map_with(tiles: impl IntoIterator<Item = (i32, i32, TerrainTile)>) -> MapData {
        MapData {
            tiles: tiles
                .into_iter()
                .map(|(x, y, tile)| MapTile { x, y, tile })
                .collect(),
        }
    }

    fn small_collider() -> Collider {
        Collider::new(Vec2::new(40.0, 20.0), Vec2::ZERO)
    }

    #[test]
    fn floor_accepts_fitting_collider() {
        let map = map_with([(0, 0, TerrainTile::Floor)]);
        let occupancy = Occupancy::terrain_only(&map);

        assert!(can_occupy(Vec2::ZERO, small_collider(), &occupancy));
    }

    #[test]
    fn wall_rejects_collider() {
        let map = map_with([(0, 0, TerrainTile::Wall)]);
        let occupancy = Occupancy::terrain_only(&map);

        assert!(!can_occupy(Vec2::ZERO, small_collider(), &occupancy));
    }

    #[test]
    fn missing_sparse_tile_rejects_collider() {
        let map = map_with([]);
        let occupancy = Occupancy::terrain_only(&map);

        assert!(!can_occupy(Vec2::ZERO, small_collider(), &occupancy));
    }

    #[test]
    fn collider_spanning_a_wall_rejects() {
        let map = map_with([(0, 0, TerrainTile::Floor), (1, 0, TerrainTile::Wall)]);
        let occupancy = Occupancy::terrain_only(&map);
        let wide_collider = Collider::new(Vec2::new(TILE_SIZE + 8.0, 20.0), Vec2::ZERO);

        assert!(!can_occupy(Vec2::ZERO, wide_collider, &occupancy));
    }

    #[test]
    fn exact_tile_edge_contact_does_not_enter_neighbor() {
        let map = map_with([(0, 0, TerrainTile::Floor)]);
        let occupancy = Occupancy::terrain_only(&map);
        let tile_sized_collider = Collider::new(Vec2::splat(TILE_SIZE), Vec2::ZERO);

        assert!(can_occupy(Vec2::ZERO, tile_sized_collider, &occupancy));
    }

    #[test]
    fn collider_crossing_tile_edge_requires_neighbor_floor() {
        let map = map_with([(0, 0, TerrainTile::Floor)]);
        let occupancy = Occupancy::terrain_only(&map);
        let tile_sized_collider = Collider::new(Vec2::splat(TILE_SIZE), Vec2::ZERO);

        assert!(!can_occupy(
            Vec2::new(TILE_EDGE_EPSILON * 2.0, 0.0),
            tile_sized_collider,
            &occupancy
        ));
    }

    #[test]
    fn axis_separated_movement_slides_along_blocked_axis() {
        let map = map_with([
            (0, 0, TerrainTile::Floor),
            (0, 1, TerrainTile::Floor),
            (1, 0, TerrainTile::Wall),
            (1, 1, TerrainTile::Wall),
        ]);
        let occupancy = Occupancy::terrain_only(&map);
        let collider = small_collider();

        let moved = move_axis_separated(
            Vec2::ZERO,
            Vec2::new(TILE_SIZE, TILE_SIZE),
            collider,
            &occupancy,
        );

        assert_eq!(moved, Vec2::new(0.0, TILE_SIZE));
    }
}
