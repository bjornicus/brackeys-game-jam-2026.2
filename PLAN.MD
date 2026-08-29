# Health, Story, and Progression Implementation Plan

This plan is intentionally divided into small, sequential implementation sessions. A smaller implementation model should complete **one numbered step per session**, run that step's checks, update the checklist and implementation log, and leave the repository compiling before handoff.

## Confirmed design decisions

### Player health and enemy attacks

- The player starts each new run with `100` current and maximum health.
- Enemies use a melee attack when the player is within their existing `attack_distance`.
- An enemy melee attack has a visible `0.35 s` wind-up, deals `20` damage, and then has a `1.0 s` cooldown.
- The hit is evaluated at the end of the wind-up. Moving out of range before that point avoids damage.
- A successful hit gives the player `0.5 s` of invulnerability. Additional hits during that interval do no damage.
- Stun cancels an enemy's current wind-up. Dying enemies never attack.
- At zero health, player control and enemy attacks stop, a short death presentation plays, and the game enters a game-over screen.
- Every terminal is also a checkpoint. Touching it activates the checkpoint immediately, before its dialogue opens.
- Game over offers **Continue from Last Checkpoint** and **Main Menu**; there is no Retry button.
- Continuing restores the player at the latest checkpoint with full health for the maximum health recorded by that checkpoint.
- Progress made before the checkpoint is preserved: skills/items collected, terminals activated, and enemies defeated before that checkpoint remain consumed/activated/defeated.
- Progress made after the checkpoint is rolled back: later pickups return, later unlocks/armor are removed, and enemies defeated after the checkpoint respawn.
- Before any terminal is reached, the implicit checkpoint is the initial player spawn with default health/progression and the original map state.

### Dialogue and terminals

- Terminals are map entities placed on floor tiles.
- Touching a terminal activates it; there is no interaction key or proximity prompt.
- Activating a terminal immediately marks that terminal used for the current run. It cannot activate again, including after its dialogue closes.
- Dialogue pauses all gameplay simulation. The map may remain visible behind a modal overlay.
- Space, Enter, or left click advances exactly one line. Advancing from the final line closes the box and resumes gameplay.
- Escape does not close dialogue and does not open the pause menu while dialogue is active.
- Story dialogue is loaded from a human-editable RON file, not hard-coded in systems.
- Dialogue supports the displayed speakers `no. five`, `no. one`, `no. two`, and `system`.
- Placeholder conversations will be supplied. At least one conversation should involve no. one, one should involve no. two, and one should involve both NPCs.
- Terminal activation and checkpoint state last only for the current run; no save-game persistence is required.
- The latest terminal replaces the previous checkpoint. Continuing does not replay that terminal because its activated state is part of the checkpoint snapshot.

### Skill and item progression

- A new run starts with only short-range lightning unlocked and selected.
- Long-range projectile, automatic attack mode, stun, and teleport begin locked.
- The projectile pickup unlocks both forced projectile mode (`2`) and automatic mode (`3`).
- Stun retains Shift/middle-click controls after it is unlocked.
- Teleport retains right-click control after it is unlocked.
- Inputs for locked abilities are silently ignored. Do not show an “ability not unlocked” message.
- Locked controls and modes are omitted from the on-screen control instructions. The HUD updates immediately after an unlock.
- Pickups activate automatically when the player touches them, disappear for the current run, and open a paused modal unlock message.
- Unlock messages say “You have unlocked …” and explain the exact controls.
- The passive item is **Reinforced Armor**. It raises maximum health by `50` and heals `50`, capped at the new maximum. It does not provide separate percentage damage reduction.
- Unlocks and collected items roll back to the latest checkpoint on Continue and reset completely on New Game; there is no cross-run persistence.

### Map editor

- The editor must place enemies, terminals, projectile pickups, stun pickups, teleport pickups, and Reinforced Armor pickups.
- Terminal placements store a dialogue ID. The editor loads the dialogue catalog, displays the selected ID, and allows cycling through valid IDs rather than requiring a complex free-text widget.
- A tile still holds at most one map entity. Every entity placement requires an explicit floor tile.

## Initial tuning values

Add these to centralized configuration resources in `src/config.rs`; do not scatter them through systems.

| Setting | Initial value |
| --- | ---: |
| Player maximum health | `100.0` |
| Player combat hitbox | `48 x 72`, centered on the visible body |
| Post-hit invulnerability | `0.5 s` |
| Enemy melee damage | `20.0` |
| Enemy melee wind-up | `0.35 s` |
| Enemy melee cooldown | `1.0 s` |
| Player death presentation | `0.75 s` |
| Reinforced Armor maximum-health increase | `50.0` |
| Reinforced Armor healing | `50.0` |
| Terminal/pickup trigger size | `56 x 56` |

Use the existing `enemy_attack_distance` as melee range. Preserve all attack, teleport, and stun tuning already in `CombatConfig` unless a step explicitly changes it.

## Planned source and data organization

The existing `src/game.rs` is already large. New behavior should be split rather than adding every type and system to that file.

- `src/progression.rs`: shared serializable `Skill`/pickup identifiers, stable map-placement identities, checkpoint progress snapshots, and pure unlock/health-upgrade helpers. Export shared types from `src/lib.rs` where the editor needs them.
- `src/dialogue.rs`: shared serializable dialogue catalog, speaker/line types, validation, and RON loading. Export it from `src/lib.rs` so game and editor use the same parser.
- `src/game_dialogue.rs`: runtime active-dialogue resource, dialogue UI, advancement, terminal/checkpoint activation, and unlock popup requests.
- `src/player_health.rs` or a clearly delimited module under `src/game/`: player health HUD, invulnerability, death transition, and game-over UI.
- `src/enemy_attack.rs` or a clearly delimited module under `src/game/`: enemy melee state machine and telegraph.
- `src/map.rs`: serializable placement variants only; do not put runtime activation state into map data.
- `src/bin/map_editor.rs`: palette, terminal dialogue-ID selection, markers, and save/load behavior.
- `assets/dialogue/story.ron`: human-editable placeholder conversations.
- `assets/maps/initial.ron`: test placements for terminals and all pickup kinds.

If moving existing private combat types out of `src/game.rs` would cause a large unrelated refactor, leave those types in place and expose narrow helper functions. New modules must not duplicate `Health`, `Hitbox`, collision, map-coordinate, or HUD rules.

### Intended map representation

Replace the currently copyable one-variant entity enum with an extensible serializable shape equivalent to:

```rust
pub enum MapEntityKind {
    Enemy,
    Terminal { dialogue_id: String },
    SkillPickup { skill: Skill },
    ReinforcedArmorPickup,
}

pub enum Skill {
    Projectile,
    Stun,
    Teleport,
}
```

`MapEntity` and `MapEntityKind` will become `Clone` rather than `Copy` because terminal IDs own strings. Update helpers and callers deliberately; do not work around this by leaking strings or assigning meaningless numeric IDs.

### Intended dialogue file shape

Use a stable, human-readable RON schema equivalent to:

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

A catalog is a map from unique string ID to a non-empty ordered list of lines. Unknown IDs, empty IDs, and empty conversations must produce clear errors. Runtime loading may fall back to a tiny built-in error conversation so a malformed story file does not panic the game, but the editor should visibly report the error and refuse to place an invalid terminal ID.

## State and scheduling rules

Add explicit application states for modal and reset behavior rather than sprinkling pause booleans through gameplay systems:

- `GameState::Game`: normal gameplay systems run.
- `GameState::Paused`: existing pause menu; unchanged except for new-run cleanup integration.
- `GameState::Dialogue`: only dialogue input/UI and non-gameplay-safe rendering systems run.
- `GameState::GameOver`: only game-over UI input runs.
- `GameState::Restarting`: one-frame cleanup/rebuild state, then transition to `Game` for Continue/New Game or `Menu` for Main Menu.

All time-driven gameplay—movement, enemy AI, melee timers, invulnerability, projectiles, effects, cooldowns, camera-shake decay, and death timers—must stop outside `GameState::Game`. Camera shake must restore the camera before leaving gameplay so dialogue and game-over screens cannot inherit a render offset.

A **run** must have explicit ownership. Add a `RunScoped` marker to every top-level runtime entity (terrain tiles, player, enemies, terminal/pickup visuals and triggers, HUD, projectiles, and world effects), and recursively despawn those entities during `Restarting`. Reset transient run resources in the same path. Do not rely on the current permanent `GameInitialized`/`EnemiesSpawned` flags without resetting or replacing them.

Checkpoint restoration requires stable placement identity. Because map data allows only one entity per tile, use map tile coordinates (for example a small `PlacementId { x, y }`) as the identity for enemies, terminals, and pickups. Attach that identity to runtime entities. Track current run progress separately from the immutable latest checkpoint snapshot:

- `RunProgress`: activated terminal IDs/tiles, collected pickup IDs/tiles, defeated enemy IDs/tiles, unlocked skills, armor/max-health progression, and any other permanent-through-checkpoint flags.
- `CheckpointSnapshot`: a clone of `RunProgress` plus the checkpoint terminal's world/tile position.
- Initial/default snapshot: empty progress plus the initial player spawn.

Record a defeated enemy when it first enters `Dying`, and record a pickup before opening its modal. When a terminal is touched, mark it activated, then clone current progress into the new checkpoint before opening dialogue. Continue rebuilds the original map and omits entities recorded consumed/defeated in the snapshot. Enemies that were alive at the checkpoint respawn in their original positions with full health; enemies defeated before it remain absent. Transient state—enemy damage/positions, projectiles, cooldowns, invulnerability, active attacks, feedback, and VFX—is not checkpointed.

## Rules for every implementation session

1. Read this entire file, `ATTACK_PLAN.md`, current `src/game.rs`, and files changed by prior completed steps.
2. Check `git status` before editing and preserve unrelated work.
3. Run the quickest relevant tests before editing. If the baseline is failing, record and resolve it before adding features.
4. Implement only the current numbered step and its tests. Do not pull later gameplay into an earlier step.
5. Prefer pure helpers and minimal headless Bevy `App` tests. Never open a window or require GPU assets in automated tests.
6. Run `cargo fmt --check`, targeted tests, `cargo test`, and `cargo check --all-targets` before handoff.
7. Perform the listed manual smoke check when practical. If it is not run, state that explicitly in the implementation log.
8. Mark completed boxes and append a concise entry under **Implementation log** with files changed, commands run, and remaining/manual validation.

---

## Step 0 — Establish a green baseline and shared data seams

**Goal:** Add progression/dialogue data types and parsers without changing gameplay.

- [x] Run and record baseline `cargo fmt --check`, `cargo test`, and `cargo check --all-targets`.
- [x] Add `src/progression.rs` with serializable `Skill::{Projectile, Stun, Teleport}` and export it from `src/lib.rs`.
- [x] Add `src/dialogue.rs` with serializable `Speaker`, `DialogueLine`, and `DialogueCatalog` types.
- [x] Give `Speaker` one display-label helper returning exactly `no. five`, `no. one`, `no. two`, or `system`; rendering code must use this helper.
- [x] Add dialogue catalog parsing/loading and validation helpers. Keep filesystem path construction protected from traversal in the same spirit as `map_path`.
- [x] Add `assets/dialogue/story.ron` with at least three short placeholder conversations: one with no. one, one with no. two, and one involving both NPCs. Include no. five in each exchange.
- [x] Document the dialogue schema and file location in `README.md`.

**Automated acceptance:** Tests parse and preserve line order, reject an empty ID/conversation, report malformed RON, map every speaker to the required display label, and reject invalid dialogue filenames. Existing tests remain green.

**Manual acceptance:** Open `assets/dialogue/story.ron` and confirm a non-programmer can understand and edit IDs, speakers, and line text.

## Step 1 — Extend map data and editor placement types

**Goal:** Persist terminals and all pickup types and make them editable, without runtime activation yet.

- [x] Extend `MapEntityKind` to the intended variants above. Derive `Clone`, serialization, equality, and debugging traits; remove `Copy` where the terminal string prevents it.
- [x] Update `MapData::entity_at`, placement/replacement helpers, enemy spawning, tests, and editor matching to clone or borrow explicitly. Do not introduce duplicate entity records on one tile.
- [x] Keep old RON compatibility: maps containing only `Enemy` entities and maps lacking `entities` must still load.
- [x] In the editor add palette choices/hotkeys: `1` floor, `2` wall, `3` enemy, `4` terminal, `5` projectile pickup, `6` stun pickup, `7` teleport pickup, `8` Reinforced Armor.
- [x] Load `assets/dialogue/story.ron` in the editor. While terminal mode is selected, use `[` and `]` to cycle through sorted dialogue IDs and display the current ID in the palette/help text.
- [x] Give every entity kind a visually distinct marker. Colored geometric sprites plus short text labels are sufficient; production art is out of scope.
- [x] Preserve current layer behavior: entity left-click places/replaces only on explicit floor, entity right-click removes only the entity, and terrain editing remains independent.
- [x] Add at least three terminals and one of every pickup type to valid, reachable, non-player-start floor tiles in `assets/maps/initial.ron`. Avoid replacing existing enemies unless necessary.
- [x] Validate terminal references when loading/saving in the editor. Show an actionable error containing the missing dialogue ID rather than silently saving a broken reference.
- [x] Update `README.md` editor controls and marker meanings.

**Automated acceptance:** Tests cover old map compatibility, round-trip of every new variant, same-tile replacement, floor-only placement, terminal string cloning, and valid/invalid terminal catalog references.

**Manual acceptance:** Run `cargo run --bin map_editor -- initial`; place, replace, save, reload, and remove each new kind. Cycle terminal IDs and verify the saved RON contains the selected human-readable ID.

## Step 2 — Make run lifecycle and modal states restart-safe

**Goal:** Establish state transitions and reliable cleanup before adding modal UI and game over.

- [ ] Add `Dialogue`, `GameOver`, and `Restarting` to `GameState`.
- [ ] Add a restart intent resource/enum with distinct `NewGame`, `ContinueCheckpoint`, and `MainMenu` paths. `NewGame` discards checkpoint progress; `ContinueCheckpoint` preserves the latest snapshot while rebuilding; `MainMenu` discards the run/checkpoint before entering the menu.
- [ ] Route the main-menu **New Game** button through `Restarting` with `NewGame`, ensuring a fresh run even after a previous run.
- [ ] Add `RunScoped` and mark all currently spawned top-level gameplay entities: terrain, player, enemies, enemy bars through their parent, combat HUD, projectiles, lightning/shockwave effects, and any other persistent runtime visuals.
- [ ] If needed, change `tilemap::spawn_map` to return spawned entity IDs so the game can mark terrain without changing editor behavior.
- [ ] Replace or reset `GameInitialized` and `EnemiesSpawned` so entering `Game` from `Paused`/`Dialogue` resumes the existing run, while entering after `Restarting` creates exactly one fresh run.
- [ ] Add stable `PlacementId`/tile identity plus initial `RunProgress` and `CheckpointSnapshot` resources. The initial snapshot has empty progress and the player's initial spawn position.
- [ ] Centralize cleanup of run entities/resources: `GameMap`, selected mode, teleport state, feedback, collision debug as appropriate, camera shake trauma/transform, and pending modal/death state. Preserve or discard checkpoint resources according to restart intent rather than unconditionally resetting them.
- [ ] Add one rebuild helper that starts from the immutable map plus either empty progress (`NewGame`) or cloned checkpoint progress (`ContinueCheckpoint`). It will be extended by later steps to filter enemies/pickups/terminals.
- [ ] Ensure every camera-shake application/decay system is gated to gameplay, while safe transform restoration still runs before modal states.
- [ ] Add temporary state-transition tests; do not add dialogue/game-over UI yet.

**Automated acceptance:** A headless state test proves Game→Dialogue→Game and Game→Paused→Game preserve one player/map, while NewGame and ContinueCheckpoint rebuild paths remove old run entities and create one new run. Tests distinguish preserved checkpoint data from discarded new-game data. No duplicate enemies, HUD, terrain, or resources appear.

**Manual acceptance:** Existing new-game, pause/resume, combat, and camera behavior remain unchanged. A temporary developer-triggered restart, if retained during this step, recreates the initial run once.

## Step 3 — Implement the generic modal dialogue box

**Goal:** Display and advance arbitrary story or system lines while gameplay is paused.

- [ ] Add an `ActiveDialogue` resource containing owned lines, current line index, and a source/kind (`Terminal` or `Unlock`). Do not store references into an asset that may be replaced.
- [ ] Add one request/helper that validates non-empty lines, installs `ActiveDialogue`, and transitions from `Game` to `Dialogue`.
- [ ] On entering `Dialogue`, spawn a modal UI tagged for cleanup on exit: dim translucent backdrop, anchored dialogue panel, speaker label, body text, and a small “Space / Enter / Click” continuation hint.
- [ ] Render one line at a time using `Speaker::display_label()`; support multi-line and long text with wrapping.
- [ ] Treat Space, Enter/Numpad Enter, or left mouse `just_pressed` as one advance action even if several occur in the same frame.
- [ ] For a non-final line, increment once and refresh text. For the final line, clear the active resource and return to `Game`.
- [ ] Ignore Escape in `Dialogue`; the pause system must not run in this state.
- [ ] Prevent the click that opened an unlock/terminal modal from also skipping its first line. State transition on the next schedule normally achieves this; add an input guard if tests expose bleed-through.
- [ ] Ensure no gameplay timers or systems advance while several seconds are spent in dialogue.
- [ ] Add a development-only test hook or focused unit setup to open a sample conversation; remove any permanent debug key before completing the step.

**Automated acceptance:** Headless tests prove line order, speaker labels, coalesced input, final-line close, Escape ignored, modal cleanup, and frozen representative gameplay timers while in `Dialogue`.

**Manual acceptance:** Open a sample three-line conversation, advance with each supported input, confirm one line per press, and verify enemies/projectiles/cooldowns remain frozen until it closes.

## Step 4 — Add player health, health HUD, and invulnerability

**Goal:** Give the player generic damageable health before enemies begin attacking.

- [ ] Extend `CombatConfig` with player maximum health and invulnerability duration.
- [ ] Spawn the player with the existing generic `Health`, a body `Hitbox`, and a player-specific `Invulnerable` timer only after a hit is accepted.
- [ ] Refine the generic damage pipeline so enemy and player damage still share `Damage`; do not mutate player health directly from future enemy attack systems.
- [ ] Add `DamageApplied` (or an equivalent explicit result seam) emitted only when health actually changes. Use it for player feedback and later death handling.
- [ ] While the player is invulnerable, ignore incoming damage. If several damage messages target the player in one frame, accept at most one before deferred component insertion; use a local set/aggregation rather than accidentally applying all messages.
- [ ] Tick and remove invulnerability only in `GameState::Game`.
- [ ] Add a fixed-screen player health display with current/max text and a clamped bar. Keep it distinct from enemy world-space bars.
- [ ] Add brief visible hit feedback on accepted damage, such as a sprite flash. Restore the normal appearance when feedback/invulnerability ends.
- [ ] Health reaching zero should remain clamped to zero in this step; Step 8 owns the death/game-over transition.
- [ ] Add player hitbox to F3 debug rendering with a color distinct from the feet movement collider.

**Automated acceptance:** Tests cover starting health, ordinary damage, zero clamping, one accepted hit from simultaneous messages, ignored invulnerable hits, timer expiry, damage accepted afterward, `DamageApplied` only on real damage, and health-bar ratio clamping.

**Manual acceptance:** Use a temporary test-only damage trigger or headless test; verify HUD reduction, flash, and F3 body hitbox. Remove any gameplay debug damage key before handoff.

## Step 5 — Spawn and activate one-shot terminals

**Goal:** Connect map terminal placements to the story dialogue engine.

- [ ] Add a runtime terminal component containing its placement identity, dialogue ID, and an `activated` flag/state.
- [ ] Spawn a visible terminal and trigger for every terminal map placement. Reuse one shared map-entity-to-world-center helper rather than duplicating enemy coordinate math.
- [ ] Use an AABB trigger based on the configured terminal size and the player's collider/body position. “Touching” includes exact boundary contact.
- [ ] After player movement and before pickup checks, find overlapping unused terminals deterministically (tile order or entity index) and activate at most one per frame.
- [ ] On touch, perform this order atomically in one system: mark the terminal activated; add its placement ID to `RunProgress.activated_terminals`; clone all current run progress into `CheckpointSnapshot`; record this terminal's world/tile position as the respawn point; then request dialogue.
- [ ] This checkpoint is active before the first dialogue line appears. If the dialogue catalog entry is unexpectedly missing, retain the checkpoint, show a short `system` error conversation, and log the ID instead of panicking.
- [ ] Keep the terminal visual after use but visibly change it to an inactive state (dim color/unlit screen). Remove or disable its trigger.
- [ ] On Continue, recreate terminals activated in the snapshot as visibly inactive with disabled triggers. On New Game, recreate every terminal as unused. No disk persistence is required.
- [ ] Define overlap priority: terminals activate before pickups when both would trigger in the same frame; only one modal opens.

**Automated acceptance:** Tests cover no activation outside the trigger, boundary activation, one activation on entry, checkpoint capture before dialogue, no reactivation after dialogue closes or Continue, deterministic choice among overlaps, missing-ID fallback that retains the checkpoint, restoration at checkpoint position, and full terminal reset on New Game.

**Manual acceptance:** Walk into every initial-map terminal, read its complete dialogue, and walk away/back. Confirm it stays visibly inactive and never reopens. After Continue it remains inactive; after New Game it becomes available again.

## Step 6 — Add ability and Reinforced Armor pickups

**Goal:** Unlock skills through one-shot world pickups and update instructions immediately.

- [ ] Add current `PlayerProgression`/unlock state defaulting to projectile/stun/teleport locked. Lightning is always available and need not be stored as an unlock bit. Keep its checkpoint-relevant fields mirrored in or derived from `RunProgress` so there is only one authoritative restoration rule.
- [ ] Default `SelectedAttackMode` to `Lightning`, replacing the current `Auto` default.
- [ ] Gate `select_attack_mode`: `1` always selects lightning; `2` only works with projectile; `3` only works with projectile. Locked selections silently leave the current mode unchanged.
- [ ] Gate stun and teleport at their earliest request checks. Locked input must do nothing: no cooldown, animation, VFX, camera shake, rejection status, or “locked” feedback.
- [ ] Spawn distinct visible/trigger entities for every pickup placement, attach stable placement identity, and detect overlap after terminals. Do not spawn pickups listed as collected in the progress being restored. Activate at most one pickup per frame.
- [ ] On a new skill pickup, add its placement ID to current `RunProgress`, set the unlock, despawn/disable the pickup, then open a one-line or short `system` modal with exact wording and controls:
  - Projectile: “You have unlocked the long-range attack. Press 2 to select it, or press 3 for automatic range selection. Fire with left click.”
  - Stun: “You have unlocked stun. Press either Shift key or middle click to stun nearby enemies.”
  - Teleport: “You have unlocked teleport. Right click valid floor to teleport.”
- [ ] On Reinforced Armor pickup, record its placement in current progress, increase `Health.max` by `50` and `Health.current` by `50`, capped at the new max, record the resulting maximum-health progression, then show: “You have unlocked Reinforced Armor. Maximum health increased by 50.”
- [ ] If a duplicate skill pickup exists, consume it without stacking and show a short already-acquired system message, or prevent duplicate placement. Choose one policy and test it; preferred runtime safety is consume-without-stacking.
- [ ] Refactor `format_attack_hud` to accept progression and include only unlocked controls/modes. Lightning and movement always appear. Add stun, teleport status/cooldown, projectile, and auto text only when unlocked.
- [ ] Ensure HUD updates immediately when progression or health changes, even while the modal obscures it.
- [ ] On Continue, restore unlocks, collected placements, and armor/max health exactly from `CheckpointSnapshot`, but fill current health to that restored maximum. Pickups collected after the checkpoint must return and their unlock/armor effects must be rolled back. Pickups collected before it remain absent.
- [ ] On New Game, discard all collected placements and start at `100/100` with optional skills locked.
- [ ] Only draw statuses relevant to unlocked abilities. The lightning range circle remains because lightning starts unlocked.

**Automated acceptance:** Pure/headless tests cover default locks, silent ignored inputs, each unlock, mode gating, dynamic HUD inclusion/exclusion, pickup one-shot behavior, exact popup control text, duplicate handling, armor at full and damaged health, checkpoint capture/rollback of pickups and unlocks, full-health restoration using checkpoint maximum health, and full New Game reset.

**Manual acceptance:** Start a new game and confirm only lightning instructions/actions exist. Collect each item, read its popup, verify its control appears and works afterward, and confirm pickups do not return after closing dialogue.

## Step 7 — Implement enemy melee wind-up, damage, and cooldown

**Goal:** Let living, unstunned enemies visibly attack the player through the generic damage pipeline.

- [ ] Extend `CombatConfig` with melee damage, wind-up, and cooldown values.
- [ ] Add an explicit enemy attack state machine, for example `Ready`, `WindingUp { remaining }`, and `Cooldown { remaining }`. Avoid separate booleans/timers that can enter contradictory states.
- [ ] Enemies continue existing chase behavior while out of range. A ready enemy entering `attack_distance` stops and starts wind-up.
- [ ] Show wind-up clearly without requiring final art: tint/flash the enemy and draw or spawn a shrinking red warning ring at melee range. Source duration/radius from config/state.
- [ ] At wind-up completion, recheck that the player is alive, damageable, and within `attack_distance`. If so, emit one `Damage` message for `20`; otherwise miss without damage. Enter cooldown in both cases.
- [ ] Tick cooldown in gameplay. During cooldown, an enemy may resume chasing if the player moves away but cannot start another wind-up until ready.
- [ ] Applying stun cancels wind-up and resets the enemy to `Ready` when stun expires; it must not release a delayed hit. Dying removes all attack state/telegraph.
- [ ] Attach stable placement identity to every map-spawned enemy. When an enemy first enters `Dying`, immediately add that identity to current `RunProgress.defeated_enemies`; do not wait for animation despawn.
- [ ] Enforce state/animation priority: `Dying` > `Stunned` > `WindingUp` > moving/idle. Preserve the existing death and stunned visuals.
- [ ] Stop enemy attacks once the player has zero health or enters `PlayerDying` in Step 8.
- [ ] Keep faction and hitbox rules explicit so enemies cannot damage one another.

**Automated acceptance:** Tests cover entering wind-up at range boundary, no early damage, one hit after wind-up, miss after player retreat, cooldown enforcement, player invulnerability, stun cancellation, no attacks while stunned/dying, deterministic damage from multiple enemies, defeat identity recording, and rebuild filtering from checkpoint progress.

**Manual acceptance:** Approach one and multiple enemies. Observe warning, retreat to avoid a hit, remain to take damage, inspect health HUD/invulnerability, and stun during wind-up to cancel it.

## Step 8 — Add player death, game-over UI, and checkpoint continuation

**Goal:** Turn zero player health into a clear terminal run state and rebuild the world from the latest checkpoint.

- [ ] Add a one-way `PlayerDying` component/timer when player health first reaches zero. Never enter it more than once.
- [ ] On death start, cancel/remove `PlayerAction`, disable player hitbox and controls, stop enemies from chasing/attacking, clear pending attack/teleport feedback, and restore camera shake safely.
- [ ] Provide a `0.75 s` visible death presentation without requiring new final art: tint/fade the player and optionally show a centered “no. five is down” message. Do not instantly despawn the player.
- [ ] Tick death only in `Game`. When complete, transition once to `GameOver`.
- [ ] On entering `GameOver`, spawn a modal UI showing “Game Over” with **Continue from Last Checkpoint** and **Main Menu** buttons. Gameplay remains frozen; do not label the action Retry.
- [ ] Continue sets restart intent to `ContinueCheckpoint` and enters `Restarting`; Main Menu uses `MainMenu` and enters `Restarting`.
- [ ] For Continue, recursively remove all `RunScoped` entities and transient resources, but preserve the immutable map/dialogue data and latest `CheckpointSnapshot`. Rebuild current `RunProgress` from that snapshot.
- [ ] Spawn the player at the snapshot's checkpoint position. Restore health to the snapshot's maximum (full health), restore its unlocks/armor, reset selected attack mode to a valid mode (prefer lightning), and reset all cooldowns/invulnerability/actions/feedback.
- [ ] Rebuild enemies from original placements, omitting those defeated in the snapshot. This restores enemies defeated after the checkpoint and keeps enemies defeated before it absent. Spawn all restored living enemies at full health and original map positions.
- [ ] Rebuild pickups from original placements, omitting those collected in the snapshot. This restores pickups collected after the checkpoint and keeps earlier pickups absent.
- [ ] Rebuild terminals from original placements; terminals activated in the snapshot remain visibly inactive and cannot replay. The checkpoint terminal itself is included in that activated set.
- [ ] If no terminal has been reached, Continue uses the implicit initial snapshot: initial player spawn, full `100/100` health, lightning only, all original enemies/pickups/terminals.
- [ ] A menu New Game uses `NewGame`, discards checkpoint/current progress completely, and creates the same initial state.
- [ ] Ensure Continue/New Game never duplicate terrain, player, HUD, enemies, markers, effects, or animation resources with unique-query assumptions.

**Automated acceptance:** Tests cover one-way death entry, disabled controls/attacks, delayed game-over transition, exact button actions/labels, implicit initial checkpoint, full-health checkpoint respawn, preserved pre-checkpoint unlocks/armor/consumption/defeats, rolled-back post-checkpoint progress, inactive non-replaying checkpoint terminal, transient reset, and Main Menu followed by New Game producing one clean run.

**Manual acceptance:** Activate a terminal; collect one skill and defeat one enemy; activate a later terminal; then collect another item, defeat another enemy, and die. Continue and verify the player appears at the later terminal with full checkpoint-appropriate health, pre-checkpoint progress remains, and only post-checkpoint enemy/item progress is restored. Then choose Main Menu/New Game and verify a complete reset.

## Step 9 — Integration hardening, documentation, and regression pass

**Goal:** Verify all features interact correctly and leave implementation guidance current.

- [ ] Add compact headless integration scenarios with manually advanced time:
  1. New run starts at `100/100`, lightning selected, all optional skills locked.
  2. Enemy wind-up → damage → invulnerability → later damage → zero health → delayed game over.
  3. Retreat or stun during wind-up prevents damage.
  4. Terminal touch → checkpoint captured before story modal → frozen combat → final advance → no terminal replay.
  5. Projectile pickup → unlock modal → modes `2`/`3` and HUD become available.
  6. Stun and teleport remain silent/no-op before pickup and work after pickup.
  7. Armor at partial health adds `50` max and `50` current, then updates HUD.
  8. Terminal/pickup overlap chooses and checkpoints the terminal first, then allows the pickup on a later gameplay frame.
  9. Collect/defeat before a terminal, collect/defeat after it, then Continue: only pre-checkpoint progress remains consumed/defeated, the player has full checkpoint-appropriate health, and restoration occurs exactly once.
  10. Before any terminal, Continue uses the implicit initial-spawn snapshot.
  11. Main Menu → New Game discards all checkpoint progress.
- [ ] Audit all gameplay systems and ensure they run only in `Game`, except intentional dialogue/game-over/restart UI and camera restoration.
- [ ] Audit every spawned runtime entity for `RunScoped` ownership or parent ownership under a run-scoped entity.
- [ ] Check simultaneous input priority after progression changes: dialogue/game-over UI first by state; in gameplay terminal trigger, then pickup trigger, then existing stun/teleport/fire/movement ordering.
- [ ] Update `README.md` with dynamic controls, health/game-over/checkpoint flow, terminal/pickup editor keys, dialogue schema/path, tuning locations, and precise Continue versus New Game semantics.
- [ ] Update any placeholder asset documentation if new marker art was added.
- [ ] Run the complete final automated gate and record results.
- [ ] Run a release build only if time permits.

**Final automated gate:**

```bash
cargo fmt --check
cargo test
cargo check --all-targets
```

**Final manual regression script:**

1. Start from splash/menu. Confirm one player, one HUD, and expected initial enemies/items/terminals.
2. Confirm lightning works; modes `2`/`3`, stun, and teleport silently do nothing and are absent from instructions.
3. Let an enemy wind up. Move away to avoid one strike, then remain in range to take one strike. Verify health, flash, and invulnerability.
4. Touch each terminal, advance with Space/Enter/click, and confirm combat freezes, the checkpoint activates before dialogue, and the terminal cannot replay.
5. Collect projectile, stun, and teleport items. Verify each modal text, updated controls, and newly working input.
6. Collect Reinforced Armor while damaged and verify both current and maximum health increase correctly.
7. Test stun during enemy wind-up and teleport during combat. Confirm existing attack/collision/cooldown behavior remains intact.
8. Establish a checkpoint, make additional enemy/item progress, then die. Continue and verify full health, checkpoint spawn position, preserved pre-checkpoint progress, and rolled-back post-checkpoint progress.
9. In a separate run, die before a terminal (or use a test map without an early terminal) and verify Continue uses the initial spawn/default snapshot.
10. Die again, return to Main Menu, choose New Game, and verify all checkpoint progress is discarded.
11. Open the editor, cycle terminal dialogue IDs, place every entity kind, save/reload, and launch the edited map without errors.

## Explicit non-goals

- Save files or checkpoint persistence between application launches; checkpoints are in-memory for the current run only.
- Branching dialogue choices, quests, localization, typewriter effects, portraits, voice acting, or audio.
- Replaying activated terminals during the same run.
- A free-text dialogue editor inside the map editor; story text remains in human-editable RON.
- Additional active abilities, ammunition/mana, skill trees, or upgrade levels.
- Percentage armor mitigation; Reinforced Armor is a one-time max-health/heal pickup.
- Enemy ranged attacks, navigation/pathfinding, or physical actor collision.
- Production-quality terminal, pickup, hit, death, or attack art.

## Implementation log

Append one entry per completed step using this format:

- `Step N — YYYY-MM-DD`: summary; files changed; baseline/final commands and results; manual checks run or explicitly not run; known follow-up.
- `Step 0 — 2026-08-29`: Added shared serializable progression skills and a validated, traversal-safe dialogue catalog loader with ordered-line, validation, malformed-RON, label, and filename tests. Added three editable placeholder conversations and documented the RON schema. Files changed: `src/progression.rs`, `src/dialogue.rs`, `src/lib.rs`, `assets/dialogue/story.ron`, `README.md`, `PLAN.MD`. Baseline and final `cargo fmt --check`, `cargo test`, and `cargo check --all-targets` passed; targeted `cargo test dialogue` passed. Manual story-file readability check not run.
- `Step 1 — 2026-08-29`: Extended map entities with terminal dialogue IDs, skill/armor pickups, and clone-safe placement helpers; updated gameplay enemy filtering and rebuilt the editor palette, labels, terminal dialogue-ID cycling, and load/save validation. Added initial-map placements and editor/map tests; updated README. Files changed: `src/map.rs`, `src/game.rs`, `src/bin/map_editor.rs`, `assets/maps/initial.ron`, `README.md`, `PLAN.MD`. Baseline/final `cargo fmt --check`, `cargo test`, and `cargo check --all-targets` passed. Manual editor smoke check not run. Follow-up: runtime terminal/pickup activation is intentionally deferred to Steps 5–6.
