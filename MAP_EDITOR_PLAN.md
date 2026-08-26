# Tile Map and Editor Plan

## Shared map system

- Keep `TILE_SIZE` (currently `96.0`) in one shared map module.
- Use a two-cell horizontal terrain atlas at `assets/sprites/terrain.png`: Floor and Wall.
- Store maps as sparse, human-editable RON files in `assets/maps/<name>.ron`.
- Each tile stores an integer grid coordinate and a terrain type. Empty coordinates have no tile.
- Derive map bounds from placed tiles. Painting outside the bounds expands a map; deleting edge tiles shrinks it. Deleting interior tiles leaves transparent gaps.

## Initial map

`assets/maps/initial.ron` is a 10x10 map with Wall tiles around the edge and Floor tiles in the 8x8 interior.

## Source organization

- `src/lib.rs`: shared map modules for both executables.
- `src/map.rs`: map data, RON load/save, terrain definitions, and tile-size constant.
- `src/tilemap.rs`: shared terrain-atlas and tile-spawning code.
- `src/main.rs`: game executable.
- `src/bin/map_editor.rs`: standalone editor executable.

## Running

```bash
cargo run                         # game
cargo run --bin map_editor -- initial # edit assets/maps/initial.ron
```

## Editor controls

- `1`: select Floor.
- `2`: select Wall.
- Clickable Floor/Wall palette buttons provide the same selection.
- Left click: paint the selected tile.
- Right click: delete a tile.
- `Ctrl+S`: save the current map.
- Mouse wheel: zoom.
- Middle mouse drag: pan the camera.

## Collision and passability

- Only explicit `Floor` tiles are passable. Wall and empty sparse-map cells block movement.
- Player movement resolves horizontal and vertical axes separately so it slides along walls.
- The player uses a manually tunable feet collider rather than its full padded 96×96 sprite: `PLAYER_COLLIDER_SIZE` and `PLAYER_COLLIDER_OFFSET` in `src/game.rs`. The initial values are 40×24 with an offset of `(0, -32)`.

Collision now exists; Floor and Wall remain otherwise visual-only.
