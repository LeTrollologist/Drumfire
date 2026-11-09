# Changelog

All notable changes to Drumfire are documented here. The combat rules have
their own version in [`docs/rules.md`](docs/rules.md).

## Unreleased

### Added

- Mode framework with a `mode` field on `BattleInput`:
  - **Skirmish** and **Battle** implemented.
  - Frontlines, Dogfight, Naval, and Campaign reserved and rejected by
    validation rather than silently simulated.
- Mode-aware ground profiles: Skirmish changes strength weights, casualty
  intensity, and morale volatility; Battle keeps the original v1 coefficients.
- Mode-aware terminology: Battle uses “Phase”/“Troops”; Skirmish uses
  “Exchange”/“Combatants.”
- Outcome classification derived from the log: decisive, pyrrhic, marginal,
  rout, exhaustion, stalemate, draw.
- CLI `--mode` override for `run` and `explain`.
- GUI mode picker for Skirmish/Battle, with planned modes listed.
- Visual redesign: warm Paper theme by default (Night toggle), report hierarchy,
  momentum bar, probability sparkline, clickable phase cards, selected-phase
  detail, factor bars, and a compact stat grid instead of a wall of sliders.
- Skirmish fixture and committed replay log; Battle logs and hashes remain
  backward-compatible.
- [`docs/modes.md`](docs/modes.md) describing all planned modes and the campaign
  wrapper.

### Changed

- UI and CLI now say “phase” (or the mode’s noun) instead of “round.” The JSON
  key `rounds` remains for schema stability.
- Battle inputs omit `mode` when serialized; Skirmish inputs include it.

## 0.1.0 — 2026-11-09

### Added

- Rust workspace with `drumfire-model`, `drumfire-core`, `drumfire-ns`,
  `drumfire-cli`, and `drumfire-gui`.
- Validated domain types: non-zero forces, 1–10 ratings, 0–100 morale, hex seeds,
  and a `BattleInput` schema with a checked configuration.
- Deterministic ChaCha20 battle engine with per-round strength, win probability,
  casualties, morale, rout, annihilation, and round-cap endings.
- Self-contained battle logs with SHA-256 input hashes and exact replay
  verification, including exact float round-tripping.
- CLI commands: `run`, `replay`, `explain`, and `nation`, with `--seed`,
  `--json`, and `--out`.
- egui desktop UI: team forms, sliders, home-side selector, advanced rules,
  round-by-round report, strength breakdown, clipboard export, and a `--demo`
  startup mode.
- Read-only NationStates client: one batched public-shard request, descriptive
  User-Agent with `DRUMFIRE_CONTACT`, typed XML parsing, and a header-aware
  rate gate.
- Read-only “Import nation” flow with a transparent population-based force
  suggestion; every imported value remains editable.
- Combat specification in [`docs/rules.md`](docs/rules.md) and API/security notes
  in [`docs/nationstates.md`](docs/nationstates.md).
- Example battle, committed replay fixture, and GUI screenshot.
- GPL-2.0-only license and provenance notice.

### Known limitations

- No authenticated features, write actions, or Telegram support.
- No daily-dump ingestion or region-wide analysis.
- Rules v1 coefficients are initial choices, not tuned against a large corpus.
- Windows is the tested build target; Linux support is intended but not yet
  exercised on this machine.
- No installer or signed release artifacts yet.
