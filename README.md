# Brackeys Game Jam 2026.2

A Rust/Bevy 0.19 top-down combat prototype.

## Run and validate

```bash
cargo run
cargo run --bin map_editor -- initial
cargo fmt --check
cargo test
cargo check --all-targets
```

## Game controls

- **Move:** WASD or arrow keys. `F3` shows player/enemy collision and combat boxes.
- **Attack mode:** `1` lightning, `2` projectile, `3` automatic (default). Lightning and auto show its range circle.
- **Fire:** left click. Lightning must be in range with a clear terrain path; projectiles continue past the click and stop at their first wall or enemy.
- **Teleport:** right click on valid floor. It has a five-second cooldown and uses the same terrain occupancy rule as walking.
- **Stun:** either Shift key or middle click. It stuns enemies within the shockwave radius and shakes the camera.
- **Pause:** Escape.

When idle, simultaneous gameplay input resolves in this order: stun, teleport, fire, movement. While firing, gameplay input is ignored (only pause is accepted).

## Map editor

Run `cargo run --bin map_editor -- <map-name>`. Palette keys are `1` floor, `2` wall, and `3` enemy. In terrain modes, left click paints and right click removes terrain. In enemy mode, left click places an enemy only on an explicit floor tile and right click removes only the enemy marker. Use Ctrl+S to save. Middle-drag pans and the wheel zooms.

Maps are sparse RON files under `assets/maps/`. Enemy placements are independent of terrain; removing or painting terrain does not silently remove an entity.

## Story dialogue

Story conversations live in the human-editable RON catalog at
`assets/dialogue/story.ron`. Each ID maps to an ordered list of lines; a line
has a `speaker` (`NoFive`, `NoOne`, `NoTwo`, or `System`) and a `text` string:

```ron
(
    conversations: {
        "terminal_intro": [
            (speaker: NoOne, text: "No. Five, can you hear me?"),
            (speaker: NoFive, text: "Loud and clear."),
        ],
    },
)
```

Conversation IDs and conversations must be non-empty. Keep IDs unique; the
runtime and map editor use them to connect terminals to dialogue.

## Assets and tuning

Temporary checked-in sheets and their stable replacement layout are documented in [`assets/sprites/PLACEHOLDERS.md`](assets/sprites/PLACEHOLDERS.md). Regenerate them with `python3 tools/generate_placeholder_sprites.py`.

Gameplay values, including attack ranges/damage, enemy values, teleport cooldown, stun, and camera shake, are centralized in `src/config.rs` (`CombatConfig` and `CameraShakeConfig`).
