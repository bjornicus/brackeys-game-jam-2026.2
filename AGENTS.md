# Project Guide

Rust/Bevy 0.19 2D game. Run the game with `cargo run`; run the map editor with `cargo run --bin map_editor -- <map-name>`.

## Maps

- Maps are sparse RON files in `assets/maps/<name>.ron`.
- Terrain art is `assets/sprites/terrain.png`; every tile is `TILE_SIZE` from `src/map.rs`.
- Shared map data and rendering live in `src/map.rs` and `src/tilemap.rs` (`map_support` library crate). Keep game and editor behavior shared through these modules.

## Gameplay

- `src/menu.rs` owns the initial menu shown when the game starts.
- `src/game.rs` owns Player spawning, animation setup, controls, gameplay and pause UI.
- The player uses `assets/sprites/character.png`, an 8×8 sprite sheet animated through `bevy_spritesheet_animation`.

## Validation

There are no automated tests yet. Use `cargo check` after code changes; use `cargo fmt` to format.
