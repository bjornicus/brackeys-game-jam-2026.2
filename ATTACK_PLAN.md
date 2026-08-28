# Attacks, Abilities, Enemies, and Validation Plan

This plan is intentionally split into small, sequential implementation sessions. A new agent should complete only one numbered step per session, run that step's checks, update the checklist, and leave the repository compiling before handing off.

## Confirmed design decisions

- Aim uses the mouse cursor converted from viewport coordinates to world coordinates.
- Four facings are used everywhere: `Right`, `Left`, `Up`, and `Down`. The dominant aim or movement axis selects the facing; horizontal wins exact ties.
- Movement changes facing and uses the matching movement animation. Starting an attack turns the player toward the click and locks movement until the firing animation ends.
- Left click fires. Attack modes are:
  - `1`: force short-range lightning.
  - `2`: force long-range projectile.
  - `3`: automatic mode (default), choosing lightning at or inside lightning range and projectile outside it.
- Forced lightning does nothing if the target is out of range. It also does nothing if terrain blocks the complete path to the clicked point.
- Automatic mode chooses strictly by range. Therefore, an in-range but obstructed click chooses lightning and fails rather than silently changing attack type.
- A visible world-space circle shows lightning range while lightning or automatic mode is selected.
- A projectile travels in the aim direction beyond the clicked point. It stops at the first wall or damageable entity hit.
- Right click teleports to the clicked point only when the player's normal movement collider can occupy that position. The cooldown is five seconds by default.
- Movement and teleport use the same occupancy API. Initially only terrain blocks occupancy. The API should accept/query future movement blockers so teleport automatically follows future collision rules.
- Left or right Shift, or middle click, casts stun. Stun emits several expanding concentric rings, shakes the screen, and stuns enemies in the shockwave's maximum range.
- Entities do not physically block one another.
- Enemies chase the player while outside `attack_distance`, then stop. They do not attack or damage the player yet.
- Enemy map placements are snapped to tile centers.
- Temporary player and enemy sprite sheets may be generated and checked in. They must be clearly documented as placeholders with stable frame layouts an artist can replace later.
- Combat and collision rules should be factored into fast pure-function and headless ECS tests. Rendering itself remains a manual smoke test.

## Initial tuning values

Put these in one `CombatConfig` resource (or a small number of clearly named config resources), not throughout systems. These are starting values, not permanent balance decisions:

| Setting | Initial value |
| --- | ---: |
| Lightning range | `220.0` world units |
| Lightning damage | `40.0` |
| Lightning visible lifetime | `0.18 s` |
| Projectile speed | `500.0` world units/s |
| Projectile damage | `30.0` |
| Projectile collision radius | `6.0` |
| Projectile maximum lifetime | `3.0 s` |
| Teleport cooldown | `5.0 s` |
| Shockwave radius | `240.0` world units |
| Shockwave animation duration | `0.4 s` |
| Stun duration | `2.0 s` |
| Enemy maximum health | `100.0` |
| Enemy speed | `55.0` world units/s |
| Enemy attack distance | `64.0` world units |
| Enemy feet collider | `40 x 20`, offset downward to match art |
| Enemy full-sprite hitbox | `64 x 80` |

The firing animation is the initial fire-rate limiter. Add an explicit attack cooldown to `CombatConfig` later only if animation duration is not sufficient.

## Planned source organization

Keep `src/game.rs` primarily as plugin/system registration and split behavior before it becomes too large:

- `src/collision.rs`: shared AABB, terrain occupancy, swept segment/circle tests, and movement helpers.
- `src/combat.rs`: combat configuration, attack selection, damage events, health, hitboxes, projectiles, lightning, and shockwave data/systems.
- `src/actors.rs` (or separate `player.rs` and `enemy.rs` if clearer): facing, player control/state, enemy AI/state, and animation selection.
- `src/camera_shake.rs`: trauma-based screen shake adapted from Bevy's example.
- `src/map.rs`: serializable terrain and entity-placement data.
- `src/tilemap.rs`: terrain rendering only unless a more general map spawning name becomes useful.
- `src/bin/map_editor.rs`: terrain/entity palette and placement.

`map_support` is already the library crate shared by game and editor. Export pure collision/combat modules from `src/lib.rs` when both binaries or integration tests need them. Do not duplicate collision or map-coordinate calculations between game and editor.

## Rules for every implementation session

1. Read this entire file, the current `src/game.rs`, and files changed by earlier completed steps.
2. Check `git status` before editing. Preserve unrelated work.
3. Run the quickest relevant tests before editing so pre-existing failures are known. If any test are failing, do not continue, all tests must be passing before starting a new implementation phase.
4. Implement only the current step and its tests. Avoid speculative features from later steps.
5. Run `cargo fmt --check`, the targeted tests, `cargo test`, and `cargo check --all-targets` before handoff. After the initial compile, tests should finish in seconds.
6. Perform the listed manual smoke check when the step changes visible/runtime behavior.
7. Mark the step complete and add a short note under **Implementation log** with files changed, commands run, and any remaining issue. include listing exactly which manual validation was not run so the human operator knows what to test before moving to the next step.

For headless ECS tests, construct a minimal `App`, add only required plugins/resources/systems, and advance `Time` manually. Do not start a window, load GPU assets, sleep, or depend on frame rate. Refer to Bevy's own crate tests and the local `bevy_spritesheet_animation` headless/events examples rather than the remote-control integration example.

---

## Step 0 — Establish a green baseline and test seams

**Goal:** Capture current behavior and make future logic testable without changing gameplay.

- [x] Run and record baseline `cargo fmt --check`, `cargo test`, and `cargo check --all-targets` results.
- [x] Add unit tests for existing map passability: floor accepts a fitting collider; wall, missing sparse tile, and a collider spanning a wall reject it.
- [x] Move `PlayerCollider`'s geometry into a reusable AABB/collider type in `src/collision.rs` while retaining the current player dimensions and behavior.
- [x] Move `can_occupy` and axis-separated terrain movement into reusable functions. Keep a thin game-specific wrapper if Bevy query data would otherwise leak into pure code.
- [x] Design occupancy as one shared operation used later by player movement, enemy movement, and teleport. Terrain is the only blocker now, but leave an explicit extension point for dynamic movement blockers.
- [x] Preserve F3 collision debug drawing.

**Automated acceptance:** Existing map behavior tests pass, including exact tile-edge contact and sparse-map rejection. `cargo check --all-targets` passes.

**Manual acceptance:** Player movement and wall sliding feel unchanged; F3 still displays the player feet collider in the correct place.

## Step 1 — Extend map data with enemy placements

**Goal:** Make enemy spawn positions persistent without yet implementing enemies.

- [x] Add a serializable map entity representation, initially an `Enemy` kind plus integer tile coordinates. Prefer an enum that can be extended over parallel ad-hoc vectors.
- [x] Add `entities` to `MapData` with `#[serde(default)]`, so existing RON files that only contain `tiles` still load.
- [x] Add focused helpers such as `entity_at`, `place_entity`, and `remove_entity`; enforce at most one map entity per tile for now.
- [x] Keep terrain and entity layers independent: adding/removing an enemy must not modify terrain.
- [x] Add one or more enemy placements on valid floor tiles in `assets/maps/initial.ron` for testing.

**Automated acceptance:** Tests cover old RON compatibility, new RON round-trip, replacement/idempotent placement, and layer-independent removal.

**Manual acceptance:** The game and editor still open the updated `initial` map without panic, even though enemy records are not rendered yet.

## Step 2 — Add enemy placement to the map editor

**Goal:** Place and remove enemies at tile centers.

- [x] Add an editor palette mode for `Enemy` (suggested editor key `3`; editor key conflicts do not affect game controls).
- [x] Render map entity markers using a simple colored placeholder sprite/shape until the generated enemy sheet is introduced.
- [x] In enemy mode, left click places an enemy only on an explicit floor tile; right click removes an enemy only. Terrain mode retains current paint/delete behavior.
- [x] Update palette text so selected layer and mouse behavior are unambiguous.
- [x] Ensure repainting terrain redraws entity markers rather than losing or duplicating them.
- [x] Save placements through the existing Ctrl+S flow.

**Automated acceptance:** Keep placement validity in pure helpers and test floor, wall, and missing-tile cases. Map serialization tests remain green.

**Manual acceptance:** Run `cargo run --bin map_editor -- initial`; place, save, restart, and remove an enemy. Verify terrain beneath it is unchanged.

## Step 3 — Generate and document temporary animation assets

**Goal:** Provide deterministic art sufficient to verify every direction and state.

- [x] Add a small reproducible generator under `tools/` and check generated PNG files into `assets/sprites/`. Avoid a runtime dependency. If the generator needs a development-only tool, document its exact command.
- [x] Prefer a new fixed-layout player sheet containing, for each of four directions: idle, move, and shoot frames. Make directions visually unmistakable (for example, weapon/arrow orientation and direction colors).
- [x] Generate an enemy sheet with move/idle, stunned, and death frames. A simple colored creature silhouette is enough.
- [x] Document sheet dimensions, cell size, row/frame ranges, origin convention, and replacement expectations in `assets/sprites/PLACEHOLDERS.md`.
- [x] Replace hard-coded row numbers with named animation-layout constants.
- [x] Add four-way `Facing` and player animation handles for idle/move/shoot. Movement input selects facing by dominant axis and uses `move_up`/`move_down` as well as left/right.
- [x] Preserve normalized diagonal speed.

**Automated acceptance:** Test dominant-axis facing selection, tie behavior, and sprite-sheet layout constants/dimensions where practical.

**Manual acceptance:** Move in all four cardinal directions and diagonally. Confirm the visual direction agrees with movement and existing collision behavior is unchanged.

## Step 4 — Add mouse aim, attack modes, and an attack state machine

**Goal:** Turn toward world-space clicks and play exactly one directional firing animation while movement is locked.

- [x] Add one cursor-to-world helper/system using `Camera::viewport_to_world_2d`. Return `None` safely when the cursor is absent or conversion fails.
- [x] Add `AttackMode::{Lightning, Projectile, Auto}` and game bindings `1`, `2`, `3`; default to `Auto`.
- [x] Add a small HUD line showing selected mode and controls.
- [x] On `MouseButton::Left.just_pressed`, capture the world target once. Do not continuously retarget while the button is held.
- [x] Resolve auto mode from player-to-target distance and configured lightning range.
- [x] For a valid request, set four-way facing from the aim vector, switch to the matching shoot animation, and enter an explicit player action/state component that locks movement.
- [x] Do not overload a bare `Shooting` marker with all behavior. Store attack kind and immutable origin/target/direction needed by the later payload system.
- [x] Complete the action on the matching animation-end event and return to idle or current movement on the next frame. Ignore additional left clicks while busy.
- [x] Define zero-length aim deterministically (retain current facing and use that facing's unit vector).
- [x] Add an `AttackFired` message/event seam. Initially it may only record that the configured firing frame/event was reached; later steps consume it to create lightning/projectiles. Trigger payload once per click, not once per animation loop.

**Automated acceptance:** Pure tests cover all four aim facings, auto selection at just below/equal/above range, zero-length aim, and manual mode selection. A minimal ECS test proves one click/request creates one fire event and movement remains locked until completion.

**Manual acceptance:** With a moving camera, click in all four screen directions. The player turns correctly, plays the matching shoot animation once, cannot move during it, then resumes movement.

## Step 5 — Implement manual lightning validation and range feedback

**Goal:** Make short-range lightning visible, damaging, and terrain-blocked.

- [ ] Draw a world-space lightning-range circle centered on the player while mode is `Lightning` or `Auto`. Use gizmos initially; keep its radius sourced from `CombatConfig`.
- [ ] Add a pure grid traversal/segment test that reports whether a segment from attack origin to click crosses wall or missing terrain. Exact boundary behavior must be tested.
- [ ] Forced lightning rejects out-of-range targets without entering the firing animation. Provide visible feedback (brief red range circle/target marker or HUD status), not only a log message.
- [ ] Lightning and in-range automatic attacks reject terrain-obstructed targets before entering the firing animation. This is intentionally different from a projectile, which may launch and then hit a wall.
- [ ] On the firing event, spawn a short-lived lightning visual from the player/weapon origin to the exact clicked point. A jagged line made from several segments is sufficient placeholder VFX.
- [ ] Introduce `Hitbox`, `Health`, `Damage`, and a damage message/API without coupling it specifically to enemies.
- [ ] Lightning damages the first damageable hitbox intersected along the segment. Wall validation occurs first. Keep hit ordering deterministic by segment distance rather than query iteration order.
- [ ] Ensure one cast cannot damage the same entity repeatedly over the visual's lifetime.

**Automated acceptance:** Tests cover range equality, out-of-range rejection, unobstructed floor, wall/missing-tile obstruction, nearest-hit selection, and exactly-once damage.

**Manual acceptance:** Lightning range follows the player. Valid casts show a bolt; out-of-range and through-wall casts do not animate/fire and show rejection feedback.

## Step 6 — Implement swept projectile movement and collision

**Goal:** Fire a projectile beyond the click and reliably hit the nearest wall or entity.

- [ ] Consume projectile fire events to spawn a visible placeholder projectile with normalized direction, speed, radius, damage, owner/faction, and lifetime.
- [ ] Move using `delta_secs`, but use a swept segment/circle test from old to new position to prevent tunneling at low frame rates.
- [ ] Find the earliest collision among terrain and eligible hitboxes. Resolve by distance/time of impact, not ECS iteration order.
- [ ] On entity impact, emit damage once and despawn the projectile. On terrain impact, despawn without damage. Ignore the owner and same-faction entities.
- [ ] Despawn after maximum lifetime as a safety net.
- [ ] Preserve the rule that projectile direction continues beyond the clicked point.

**Automated acceptance:** Tests cover direction independence from click distance, large-delta anti-tunneling, wall-before-enemy, enemy-before-wall, nearest of multiple enemies, owner exclusion, damage-once, and lifetime cleanup.

**Manual acceptance:** Fire near and far in all directions. Confirm projectiles pass the clicked point, hit walls, hit enemies once, and never persist forever.

## Step 7 — Spawn functional enemies from map data

**Goal:** Add test enemies that chase, stop at attack distance, collide with terrain, and can be stunned/damaged.

- [ ] Spawn an enemy for every enemy placement after map setup, at tile-center world coordinates.
- [ ] Attach the generated placeholder sprite/animations, feet movement collider, full-sprite combat hitbox, health, movement speed, attack distance, and an enemy/faction marker.
- [ ] Chase the player only while distance is greater than `attack_distance`. Stop based on distance, not player/entity collision.
- [ ] Resolve enemy movement against terrain with the same shared axis-separated occupancy helper used by the player. Do not make players or enemies dynamic movement blockers.
- [ ] Use a deterministic fallback when direct diagonal pursuit is terrain-blocked (axis-separated sliding is sufficient; pathfinding is out of scope).
- [ ] Add enemy collider/hitbox visualization to F3 debug drawing using distinct colors.
- [ ] Keep health mutation in the generic damage pipeline rather than directly in lightning/projectile systems.

**Automated acceptance:** Headless tests prove spawn count/positions match map data, enemies approach outside attack distance, stop at/inside it, do not cross walls, and lose the expected health from damage events.

**Manual acceptance:** Editor-placed enemies appear, face/move toward the player, slide/stop at walls, stop near the player, and react to both attacks.

## Step 8 — Add health bars, stun state, and enemy death

**Goal:** Make combat outcomes clearly visible and state transitions robust.

- [ ] Spawn a world-space health bar as enemy child entities with a fixed background and foreground fill.
- [ ] Update fill from clamped `current / max`, hide or remove it during death, and ensure parent scaling/animation does not accidentally distort layout beyond intent.
- [ ] Add a timed `Stunned` state. While stunned, enemy AI/movement is disabled and the stunned animation/visual is active. Reapplying stun should use one documented policy; default to resetting duration to the full configured duration.
- [ ] Define animation/state priority: `Dying` > `Stunned` > moving/idle.
- [ ] When health reaches zero or below, enter `Dying` once, disable movement/hitbox/damage reception, play the placeholder death animation once, then recursively despawn the enemy and health bar.
- [ ] Avoid despawning immediately on damage so the death animation is observable.

**Automated acceptance:** Tests cover health-bar ratio clamping, stun duration/reset, no movement while stunned, one-way transition to dying, no post-mortem damage, and despawn only after death completion.

**Manual acceptance:** Damage visibly shrinks health bars. A stunned enemy pauses and resumes. Lethal damage plays death animation before clean despawn.

## Step 9 — Implement teleport with shared occupancy and cooldown

**Goal:** Right-click teleport safely and make its five-second cooldown understandable.

- [ ] Add `TeleportCooldown` state driven by `CombatConfig.teleport_cooldown`.
- [ ] On right-button `just_pressed`, convert cursor to world coordinates and call the exact same occupancy operation and player collider used for normal movement.
- [ ] A valid ready teleport sets player position directly to the clicked point and starts cooldown.
- [ ] Invalid terrain, missing terrain, or active cooldown leaves position unchanged and provides brief visual/HUD feedback.
- [ ] Entities do not block teleport now. Keep the shared occupancy API ready for future movement-blocking entities rather than writing a teleport-only terrain check.
- [ ] Decide action interaction explicitly: default behavior is to reject teleport while firing; teleport itself does not play the firing animation.
- [ ] Show ready state or remaining cooldown in the HUD.

**Automated acceptance:** Tests cover valid floor, wall, sparse gap, near-wall collider overlap, exact edge contact, entity overlap allowed, cooldown start/tick/readiness, and rejection while busy.

**Manual acceptance:** Teleport around open floor and near every wall edge. Confirm invalid clicks do nothing, enemy overlap is allowed, and another teleport becomes available after five seconds.

## Step 10 — Implement shockwave stun and screen shake

**Goal:** Cast stun from either binding with synchronized VFX and safe camera shake.

- [ ] Trigger once on left Shift, right Shift, or middle-button `just_pressed`. Coalesce simultaneous inputs so one frame produces one cast.
- [ ] Spawn one short-lived shockwave effect represented by several concentric circles. Expand them quickly from the player to exactly `CombatConfig.shockwave_radius`, fade them, then despawn.
- [ ] Apply stun once per cast to all living enemies whose hitboxes are within the configured maximum radius. Use a circle-vs-AABB helper so range corresponds to hitboxes, not only transform origins.
- [ ] Adapt `../bevy/examples/camera/2d_screen_shake.rs`: restore the unshaken transform in `PreUpdate`, track the player normally in `Update`, and apply trauma/noise in `PostUpdate` before `TransformSystems::Propagate`.
- [ ] Keep cursor-to-world conversion and gameplay logic based on the restored/unshaken camera transform.
- [ ] Put shake strength/decay/exponent/max rotation/max translation/noise speed in a tunable camera-shake config. The stun cast adds a configured amount of trauma.
- [ ] Ensure pausing cannot leave the camera permanently offset: restoration must still occur when gameplay update systems are paused.
- [ ] Stun does not damage enemies and may be cast while enemies are already stunned (resetting duration per Step 8 policy).

**Automated acceptance:** Tests cover both keys and middle click, simultaneous-input coalescing, circle/AABB boundary inclusion, out-of-range exclusion, exactly-once stun application, effect cleanup, trauma clamp/decay, and camera transform restoration.

**Manual acceptance:** Each binding creates expanding rings to the same radius used for stun. In-range enemies stop; out-of-range enemies continue. Camera shakes briefly and still follows the player without drift afterward or after pause/resume.

## Step 11 — Integration hardening and regression pass

**Goal:** Verify systems interact correctly and leave maintainable documentation.

- [ ] Add compact headless integration scenarios using manually advanced time:
  1. Auto in range -> lightning -> enemy health decreases.
  2. Auto out of range -> projectile -> swept hit -> enemy health decreases.
  3. Wall blocks lightning before animation/damage.
  4. Projectile hits wall before enemy.
  5. Stun pauses chase, expires, and chase resumes.
  6. Lethal damage enters death, waits for completion, and despawns.
  7. Teleport rejects a wall and cooldown, accepts floor after cooldown.
- [ ] Ensure tests avoid rendering plugins/assets and complete in seconds after compilation.
- [ ] Update in-game instructions for movement, attack modes, fire, teleport, stun, pause, and F3.
- [ ] Update project documentation with editor entity controls, placeholder-asset replacement, tuning locations, and test commands.
- [ ] Check pause/resume does not duplicate maps, players, enemies, health bars, effects, or resources.
- [ ] Check simultaneous/near-simultaneous inputs have documented deterministic priority. Suggested priority while idle: stun, teleport, fire, movement; while firing: only pause is accepted.
- [ ] Run a release-mode smoke build only once at the end if time permits; routine validation remains debug-mode for speed.

**Final automated gate:**

```bash
cargo fmt --check
cargo test
cargo check --all-targets
```

All tests must pass without opening windows and should run in seconds after compilation.

**Final manual regression script:**

1. Start game from splash/menu; move and collide with walls in all directions.
2. Select modes 1, 2, and 3 and verify HUD/range indication.
3. Fire each direction while moving; verify facing lock and movement resume.
4. Test lightning at inside/equal/outside range and through a wall.
5. Test projectile continuation, wall collision, and enemy collision.
6. Teleport onto floor, wall, sparse space, near wall edges, and over an enemy; verify cooldown.
7. Stun with both Shift keys and middle click; compare ring radius to affected enemies and inspect shake recovery.
8. Damage, stun, and kill multiple enemies; inspect health bars and death cleanup.
9. Pause during movement, firing, projectile flight, stun VFX, shake, and teleport cooldown; resume and check for duplicates/drift.
10. Open editor, add/remove an enemy, save/reload, then verify it appears in game.

## Explicit non-goals

- Enemy attacks or player health/damage.
- Player/enemy or enemy/enemy physical collision.
- Navigation/pathfinding around complex walls.
- Attack mana/ammunition systems.
- Production-quality art, particles, audio, or balance.
- Letting lightning terminate on and affect walls; current behavior rejects an obstructed cast but is isolated so this can change later.

## Implementation log

Add one short entry per completed step, for example:

- `Step N — YYYY-MM-DD`: summary; files changed; validation commands; known follow-up.
- `Step 0 — 2026-08-27`: Added reusable collision AABB/collider, shared terrain occupancy and axis-separated movement helper with dynamic blocker extension point; updated player movement/debug drawing to use the shared helpers. Files changed: `src/collision.rs`, `src/game.rs`, `src/lib.rs`, `ATTACK_PLAN.md`. Baseline before editing: `cargo fmt --check`, `cargo test`, and `cargo check --all-targets` passed. Final validation: `cargo fmt --check`, `cargo test`, and `cargo check --all-targets` passed. Manual smoke not run.
- `Step 1 — 2026-08-27`: Added serializable map entity placements with an extensible `MapEntityKind::Enemy`, default-compatible `entities`, entity placement/removal helpers, tests for compatibility/round-trip/idempotency/layer independence, and two floor-tile enemy records in `assets/maps/initial.ron`. Files changed: `src/map.rs`, `src/collision.rs`, `assets/maps/initial.ron`, `ATTACK_PLAN.md`. Baseline before editing and final validation: `cargo fmt --check`, `cargo test`, and `cargo check --all-targets` passed. Targeted map tests passed. Manual game/editor smoke not run.
- `Step 2 — 2026-08-28`: Added map editor enemy entity palette mode on key `3`, placeholder enemy markers, enemy-only placement/removal behavior with explicit-floor validation, marker redraw on map repaint, and pure helper/tests for entity placement validity. Files changed: `src/bin/map_editor.rs`, `src/map.rs`, `ATTACK_PLAN.md`. Baseline before editing: `cargo fmt --check`, `cargo test`, and `cargo check --all-targets` passed. Final validation: `cargo fmt --check`, `cargo test`, and `cargo check --all-targets` passed. Manual editor smoke not run.
- `Step 3 — 2026-08-28`: Added a stdlib-only placeholder sprite generator, checked in fixed-layout player/enemy PNG sheets, documented replacement/layout details, and switched player animation setup to named four-way idle/move/shoot layout constants with dominant-axis facing tests. Files changed: `tools/generate_placeholder_sprites.py`, `assets/sprites/character_placeholder.png`, `assets/sprites/enemy_placeholder.png`, `assets/sprites/PLACEHOLDERS.md`, `src/game.rs`, `ATTACK_PLAN.md`. Baseline before editing and final validation: `cargo fmt --check`, `cargo test`, and `cargo check --all-targets` passed. Manual movement smoke not run.
- `Step 4 — 2026-08-28`: Added cursor-to-world aim capture, attack mode selection/HUD, `CombatConfig` lightning range, explicit player attack action state with immutable request data, firing-frame `AttackFired` message seam, and animation-end action completion with movement lock tests. Files changed: `src/game.rs`, `ATTACK_PLAN.md`. Baseline before editing and final validation: `cargo fmt --check`, `cargo test`, and `cargo check --all-targets` passed. Manual click/facing/movement-lock smoke not run.
