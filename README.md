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

## Game controls and progression

- **Move:** WASD or arrow keys. `F3` shows player/enemy collision and combat boxes.
- **Lightning:** `1` selects the starting short-range lightning attack; fire with left click. It needs a clear terrain path.
- **Projectile:** collect the yellow **SHOT** pickup to unlock `2` (projectile) and `3` (automatic range selection). Projectiles stop at their first wall or enemy.
- **Stun:** collect the purple **STUN** pickup to unlock either Shift key or middle click.
- **Teleport:** collect the teal **WARP** pickup to unlock right click on valid floor. It has a five-second cooldown and uses the walking terrain rule.
- **Reinforced Armor:** the green **ARMOR** pickup increases maximum health and heals by 50.
- **Pause:** Escape. Dialogue cannot be dismissed with Escape; use Space, Enter, or left click to advance one line.

Locked controls are omitted from the HUD and silently do nothing. In gameplay, simultaneous input resolves as terminal trigger, pickup trigger, stun, teleport, fire, then movement. Dialogue and game-over input are isolated by state.

## Health, death, and checkpoints

A new run starts at **100 / 100** health with lightning selected. The fixed health bar and text show current health at all times. Enemy melee attacks wind up before hitting; an accepted hit grants brief invulnerability. Defeated map enemies have a 25% chance to leave a green health drop. Touch one while injured to restore 25 health (up to maximum); drops stay available at full health. At zero health, no. five has a short death presentation, followed by **Game Over**.

Every terminal is an immediate checkpoint before its dialogue opens. **Continue from Last Checkpoint** rebuilds the original map at that terminal with full checkpoint-appropriate health: progress made before it (activated terminals, pickups/unlocks/armor, and defeated enemies) remains, while progress made after it is rolled back. Before a terminal, Continue uses the initial spawn/default state. **Main Menu**, followed by **New Game**, discards all in-memory checkpoint/run progress.

## Map editor

Run `cargo run --bin map_editor -- <map-name>`. Palette keys are `1` floor, `2` wall, `3` enemy, `4` terminal, `5` projectile pickup, `6` stun pickup, `7` teleport pickup, and `8` Reinforced Armor. In terminal mode, `[` and `]` cycle the sorted story dialogue IDs shown at the top of the editor. In terrain modes, left click paints and right click removes terrain. In every entity mode, left click places/replaces an entity only on an explicit floor tile and right click removes only the entity marker. Use Ctrl+S to save. The editor rejects missing terminal dialogue IDs when loading or saving. Middle-drag pans and the wheel zooms.

Markers are colored and labeled: red **ENEMY**, blue **TERM**, yellow **SHOT**, purple **STUN**, teal **WARP**, and green **ARMOR**. Maps are sparse RON files under `assets/maps/`; entity placements are independent of terrain, so removing or painting terrain does not silently remove an entity.

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

Gameplay values, including player health/hitbox/invulnerability/death timing, enemy melee ranges/damage/timers, the 25% health-drop chance and 25-health healing amount, terminal and pickup trigger sizes, armor values, teleport cooldown, stun, and camera shake, are centralized in `src/config.rs` (`CombatConfig` and `CameraShakeConfig`).
