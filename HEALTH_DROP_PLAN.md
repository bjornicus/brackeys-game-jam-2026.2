# Player Health Bar and Enemy Health-Drop Plan

This plan is divided into three sequential implementation steps. A forked implementation agent completes one step per session, validates it, updates this checklist/log, and leaves the repository compiling before the parent reviews and commits.

## Confirmed implementation decisions

### Player health bar

- Replace the current text-only block bar with a fixed-screen, visually filled health bar plus `current / maximum` text.
- The fill ratio is `current / max`, clamped to `0..=1`, and safely becomes zero if maximum health is non-positive.
- The bar updates immediately after enemy damage, Reinforced Armor, health regeneration, checkpoint restoration, and New Game.
- The HUD remains `RunScoped`, appears exactly once per run, and may update safely while a dialogue modal is visible.
- Use procedural Bevy UI nodes and colors; no new art is required.

### Enemy health regeneration drops

- A map-spawned enemy has an initial `25%` chance to drop one health regeneration item when it first enters `Dying`.
- A drop heals `25` health, capped at the player's current maximum health.
- Drop chance and healing are centralized in `CombatConfig` in `src/config.rs`.
- Drop eligibility is deterministic for a stable enemy `PlacementId` and a per-run seed. Continue preserves the seed, so enemies restored after a checkpoint keep the same drop outcome; New Game creates a new seed.
- Only map-spawned enemies with a stable placement identity can drop an item. Test/debug enemies without a placement do not.
- The item appears at the enemy's death position as a distinct procedural green health marker and trigger; it is `RunScoped`.
- Touching a health item automatically heals and despawns it. At full health it remains available and is not consumed.
- Trigger contact includes exact AABB boundary contact and uses the configured pickup trigger size.
- At most one health item is consumed per gameplay frame.
- Trigger priority is terminal, progression/armor pickup, then health regeneration drop. A health drop never opens a modal.
- Health drops are transient consumables, not checkpoint progression. Continue/New Game despawns outstanding drops. Continue already restores the player to full checkpoint-appropriate health; post-checkpoint enemies respawn and can deterministically drop again, while enemies defeated before the checkpoint remain absent.
- No save persistence is required.

## Source organization

- Keep centralized values in `src/config.rs`.
- Keep stable progression/run seed data in `src/progression.rs` if it must be shared or checkpointed.
- Runtime health HUD, enemy death integration, drop spawning, pickup detection, and tests may remain in `src/game.rs`; use clearly named components/helpers and do not duplicate `Health`, `Hitbox`, `PlacementId`, map-coordinate, trigger-overlap, or restart rules.
- Update `README.md` with the health bar and health-drop behavior.

## Rules for every implementation session

1. Read this entire file, `PLAN.MD`, `ATTACK_PLAN.md`, `AGENTS.md`, current `src/game.rs`, and files changed by earlier steps.
2. Check `git status` and preserve unrelated work.
3. Run a quick baseline before editing.
4. Implement only the current numbered step and its tests.
5. Prefer pure helpers and headless Bevy tests; never open a window in automated tests.
6. Run `cargo fmt --check`, targeted tests, `cargo test`, and `cargo check --all-targets` before handoff.
7. Mark completed boxes and append an implementation-log entry. Record manual checks not run.
8. Do not create a git commit; the parent reviews and commits each step.

## Step 0 — Replace the text block with a real player health bar

**Goal:** Provide a clear, fixed-screen player health bar without changing damage or drop behavior.

- [x] Record green baseline `cargo fmt --check`, `cargo test`, and `cargo check --all-targets`.
- [x] Refactor player health HUD into a run-scoped root with a background/border, fill node, and `current / maximum` text.
- [x] Size the fill from the existing clamped health-ratio helper; never write negative, NaN, or over-100% UI width.
- [x] Update text and fill immediately when `Health` changes, including safe updates during `GameState::Dialogue`.
- [x] Ensure restart/resume/modal transitions never duplicate the HUD and cleanup remains under `RunScoped` ownership.
- [x] Add headless tests for `100%`, partial, zero, over-max, and invalid/non-positive maximum ratios plus HUD text/fill updates and one-HUD ownership.

**Manual acceptance:** Run the game, take damage, collect Reinforced Armor, and verify the bar/text update without moving with the world. If not run, record it.

## Step 1 — Generate visible health drops from defeated enemies

**Goal:** Create at most one deterministic, run-scoped health item when an eligible enemy dies.

- [ ] Add `enemy_health_drop_chance = 0.25` and `health_drop_healing = 25.0` to `CombatConfig`.
- [ ] Add a per-run drop seed resource/state. Continue preserves it; New Game refreshes it; Main Menu discards it before the next run.
- [ ] Add a pure deterministic drop-roll helper using run seed plus enemy `PlacementId`, with chance clamped to `0..=1`.
- [ ] Add a `HealthDrop` runtime component tied to the source enemy placement and a distinct procedural green visual/trigger at the death position.
- [ ] Integrate drop creation exactly once when a placed enemy first enters `Dying`; dying/stunned/despawn completion must not duplicate it.
- [ ] Mark every drop `RunScoped`; restart cleanup removes outstanding drops.
- [ ] Ensure enemies filtered as defeated by checkpoint progress do not independently create drops during rebuild.
- [ ] Add headless tests for 0%/100% chance, stable same-seed outcomes, seed variation, placed-vs-unplaced enemies, one drop per death, death-position placement, and run cleanup/no duplicate drops.

**Manual acceptance:** Defeat several enemies and verify only some create one clearly visible green health item at the death location. If not run, record it.

## Step 2 — Heal on touch and harden lifecycle/priority/documentation

**Goal:** Make health drops useful and verify they integrate with damage, modals, and restarts.

- [ ] Detect boundary-inclusive overlap after terminal and progression-pickup checks; consume at most one health drop per gameplay frame.
- [ ] On touch while below maximum health, heal exactly `25`, cap at `Health.max`, despawn the drop, and refresh the visual health bar in the same frame.
- [ ] At full health, leave the drop untouched and available; zero-health/`PlayerDying` players cannot consume it.
- [ ] Enforce terminal > progression pickup > health drop priority, including deferred trigger removal/modal transitions.
- [ ] Ensure health drops and their timers/triggers do not advance outside `GameState::Game`.
- [ ] Verify Continue/New Game remove outstanding drops, Continue restores full checkpoint-appropriate health, and restored post-checkpoint enemies retain deterministic eligibility through the preserved seed.
- [ ] Add compact headless integration tests for partial heal, capped heal, full-health non-consumption, one-per-frame selection, exact boundary, modal priority, death guard, HUD refresh, Continue cleanup/determinism, and New Game seed refresh.
- [ ] Update `README.md` controls/gameplay and tuning documentation.
- [ ] Run and record the complete final gate.

**Final automated gate:**

```bash
cargo fmt --check
cargo test
cargo check --all-targets
```

**Manual acceptance:** Take enemy damage, collect a health drop, verify health text/bar healing, verify a full-health player leaves it, then test Continue and New Game cleanup. If not run, record it.

## Implementation log

Append one entry per completed step:

- `Step N — YYYY-MM-DD`: summary; files changed; commands/results; manual checks; known follow-up.
- `Step 0 — 2026-08-29`: Replaced the text-only health indicator with a run-scoped fixed UI root containing a labelled `current / maximum` text, bordered background, and green proportional fill. The fill is driven by a clamped finite-only ratio helper and is also refreshed while dialogue is active; invalid health displays safely as `0 / 0`. Added headless ratio/fill (including negative, NaN, and infinity), dialogue refresh, and one-HUD run-ownership tests. Files changed: `src/game.rs`, `HEALTH_DROP_PLAN.md`. Baseline and final `cargo fmt --check`, `cargo test` (87 tests), and `cargo check --all-targets` passed; targeted `cargo test player_health_bar` passed. Manual game/armor HUD smoke check not run. Follow-up: enemy health drops are Step 1.
