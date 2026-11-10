# Changelog

All notable changes to Drumfire are documented here. The combat rules have
their own version in [`docs/rules.md`](docs/rules.md).

## Unreleased

### Added

- Referee mode: a roleplay conflict packet with a scenario, two order sheets,
  multi-day resolution, a conflict log, and an exported Markdown report.
- Referee GUI console: map, unit chips, side views, per-unit order entry,
  day-by-day resolution, and clipboard report export.
- Connected-zone movement, ground engagements through the Battle engine, air and
  naval support, supply tracing, recon contacts, and casualty reports.
- CLI `referee scenario`, `referee orders`, `referee resolve`, `referee report`,
  `referee demo`, and `referee packet`.
- [`docs/referee.md`](docs/referee.md) and the `referee-packet/` example packet.

## 0.2.0 — 2026-11-09

The combined-arms release: Frontlines, Dogfight, Naval, and the seven-node
Halcyon Sweep campaign, plus an editable scenario ruleset and NationStates capital
import.

### Added

- Mode framework with a `mode` field on `BattleInput`:
  - **Skirmish**, **Battle**, **Dogfight**, **Naval**, and **Frontlines**
    implemented.
  - Campaign implemented as a meta-layer with seven nodes and all five modes.
- Mode-aware ground profiles: Skirmish changes strength weights, casualty
  intensity, and morale volatility; Battle keeps the original v1 coefficients.
- Mode-aware terminology: Battle uses “Phase”/“Troops”; Skirmish uses
  “Exchange”/“Combatants.”
- Outcome classification derived from the log: decisive, pyrrhic, marginal,
  rout, exhaustion, stalemate, draw.
- CLI `--mode` override for `run` and `explain`.
- GUI mode picker for Skirmish, Battle, Dogfight, Naval, and Frontlines.
- Visual redesign: warm Paper theme by default (Night toggle), report hierarchy,
  momentum bar, probability sparkline, clickable phase cards, selected-phase
  detail, factor bars, and a compact stat grid instead of a wall of sliders.
- Dogfight mode: individual aircraft, pilot/airframe/weapons/radar/support
  ratings, cohesion, passes, kills, break-off, `air run/replay` CLI commands,
  GUI screen, and a committed fixture/replay test.
- Naval mode: individual ships, hull/sensors/weapons/air-cover/damage-control
  ratings, readiness, salvos, `naval run/replay` CLI commands, GUI screen, and a
  committed fixture/replay test.
- Frontline mode: sectors, reserves, fortifications, objectives, daily logs,
  `frontline run/replay` CLI commands, GUI sector board, and a committed
  fixture/replay test.
- Campaign war-room GUI: paper map, resource and formation rails, operation
  briefings, after-action reports, chronicle export, and a seven-operation demo.
- Campaign: **The Halcyon Sweep** with seven nodes, five named formations,
  persistent will/supply/intel/momentum, all five mode adapters, five enemy
  intents, the four-node combined-arms gate, campaign log, and exact replay.
- Campaign scenario/ruleset layer: editable city and formation names, owners,
  forts, garrisons, starting resources, turn limit, and module toggles.
- Campaign modules: abstract/combined-arms/full-spectrum presets, dependency
  validation, manpower/logistics/tanks/planes/ships effects, and reserved
  strategic/society flags.
- NationStates `capital` and `customcapital` shards; CLI prints the display
  capital and the nation summary carries it.
- CLI `campaign scenario --out`, `campaign demo --scenario`, and
  `campaign play --scenario`.
- [`docs/scenario.md`](docs/scenario.md) documenting the scenario and ruleset
  schema.
- CLI `campaign play`, `campaign demo`, and `campaign replay`.
- [`docs/campaign.md`](docs/campaign.md) with the full campaign design.
- Front-line `capture_threshold`: a sector falls when the defender drops to a
  fraction of its starting troops instead of requiring annihilation.
- [`docs/frontlines.md`](docs/frontlines.md) documenting the sector model.
- [`docs/naval.md`](docs/naval.md) documenting the fleet model.
- [`docs/dogfight.md`](docs/dogfight.md) documenting the air model.
- Skirmish fixture and committed replay log; Battle logs and hashes remain
  backward-compatible.
- [`docs/modes.md`](docs/modes.md) describing all planned modes and the campaign
  wrapper.

### Changed

- UI and CLI now say “phase” (or the mode’s noun) instead of “round.” The JSON
  key `rounds` remains for schema stability.
- Battle inputs omit `mode` when serialized; Skirmish, Dogfight, and Naval inputs
  include it.

### Known limitations

- The seven city role IDs are fixed; names, owners, forts, garrisons, formations,
  and ruleset are editable, but arbitrary city topology is not yet supported.
- Strategic and society module toggles are serialized and validated; their deeper
  simulation hooks are reserved for the next release.
- Windows is the tested binary target. Linux is supported in source and CI-style
  builds but has not been exercised on this machine.
- No installer, code signing, or authenticated NationStates features.

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
