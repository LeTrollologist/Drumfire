# Drumfire modes — design proposal

**Status:** framework implemented; **Skirmish** and **Battle** are playable.
**Frontlines**, **Dogfight**, **Naval**, and **Campaign** are reserved/planned.
This document defines the mode model before the engine grows incompatible input
schemas.

## 1. The core distinction

Drumfire currently has one combat model: an aggregate, single-engagement ground
battle. The requested modes are not variations of that model:

| Mode | Domain | Unit of account | What a phase is | What “victory” means | Status |
|---|---|---|---|---|---|
| **Skirmish** | Ground, small | Fireteams / individual combatants | Minutes; a short exchange | Take/hold the objective, or rout the other side | **Implemented** |
| **Battle** | Ground, large | Troops | Hours; a general engagement | Annihilation, rout, objective, or exhaustion | **Implemented** |
| **Frontlines** | Ground, spatial | Battalions/sectors | Days; sector pressure | Break the line, take objectives, collapse the front | Planned |
| **Dogfight** | Air | Aircraft/sorties | Minutes; a pass or missile exchange | Kills, drive-off, or protect the mission package | Planned |
| **Naval** | Sea | Ships/tonnage | Hours; detection and salvos | Sink/cripple, drive off, or protect the convoy | Planned |
| **Campaign** | Meta | Battles, formations, supply | Weeks/months | Strategic objectives across multiple battles | Planned |

A single `forces: int` field cannot represent troops, aircraft, ships, and
sectors. The input schema has to branch by mode.

## 2. Shared kernel vs mode-specific rules

Keep the parts that are genuinely common:

- seeded RNG and replay contract;
- side identity and phase log;
- casualty and morale primitives;
- outcome classification;
- battle log envelope and input hash;
- validation/error model.

Make the parts that differ mode-specific:

- order of battle;
- phase resolution;
- terrain/environment;
- objective and victory conditions;
- display/board;
- labels and units.

Current ground slice:

```rust
pub enum Mode {
    Skirmish,
    Battle,
    Frontlines,
    Dogfight,
    Naval,
}

pub struct BattleInput {
    pub mode: Mode,          // Skirmish | Battle | planned modes
    pub attacker: TeamInput, // shared ground order of battle
    pub defender: TeamInput,
    pub config: BattleConfig,
    pub seed: Seed,
}
```

When non-ground modes land, replace the flat ground fields with a tagged
scenario enum:

```rust
pub enum Scenario {
    Ground(GroundScenario),   // Skirmish + Battle share the ground family
    Front(FrontScenario),
    Air(AirScenario),
    Naval(NavalScenario),
}

pub struct BattleInput {
    pub scenario: Scenario,
    pub config: CommonConfig,
    pub seed: Seed,
}
```

Campaign is separate:

```rust
pub struct Campaign {
    pub name: String,
    pub side: Side,
    pub state: CampaignState,
    pub battles: Vec<CampaignBattle>,
}
```

Do **not** make one giant struct with every field optional. Use a tagged enum so
invalid combinations cannot be represented.

## 3. Mode definitions

### Skirmish

- Scale: roughly 10–2,000 combatants.
- Primitive: fireteams or aggregated individuals.
- Decisive factors: numbers, training, leadership, terrain, surprise, morale,
  weapons, fatigue.
- Resolution: short exchanges; suppression and initiative matter; rout is
  common; casualties are a larger fraction of the committed force.
- Victory: objective, rout, or withdrawal. Annihilation is rare.
- Display: a firefight timeline and a small-unit strength board; no front line.
- Relationship to Battle: same ground family, different scale/config and
  casualty/morale curves. It should not be a separate engine if it can share the
  ground resolver.

### Battle

- Scale: roughly 2,000–100,000 troops.
- Primitive: troops/formations.
- Decisive factors: the current seven factors plus home advantage; optionally
  reserves and objectives.
- Resolution: hours per phase; attrition and morale; 6–12 phases typical.
- Victory: annihilation, rout, objective, or exhaustion.
- Display: momentum bar, phase timeline, selected-phase inspector.
- This is the current engine. It is the baseline, not the whole product.

### Frontlines

- Scale: 100,000+; days/weeks.
- Primitive: sectors (3–5) with committed forces and reserves.
- Decisive factors: sector strength, reserves, supply, fortifications, air
  superiority, morale, command.
- Resolution: each phase, allocate reserves, resolve each sector, shift the line,
  exploit breakthroughs, encircle.
- Victory: capture objectives, collapse the front, or exhaust the enemy.
- Display: sector map/front line with pressure and reserves. This is the mode
  that justifies a map.
- This is a different model from Battle: space and reserves are the point.

### Dogfight

- Scale: 2–24 aircraft per side.
- Primitive: individual aircraft/sorties.
- Decisive factors: aircraft performance (speed, climb, turn), pilot skill,
  missiles/guns, radar/EW, altitude/energy, numbers, surprise.
- Resolution: passes and missile exchanges; energy state changes; kills/damage;
  cohesion/break-off.
- Victory: kills, drive-off, or mission success (escort, intercept, recon).
- Display: altitude/energy plot and engagement timeline, not a ground table.
- Do not reuse the ground morale model; use pilot cohesion/situational awareness.

### Naval

- Scale: 2–30 ships.
- Primitive: individual ships or small formations.
- Decisive factors: sensors, missile/torpedo loadout, air cover, EW, damage
  control, formation, sea state.
- Resolution: detection → salvo → damage → damage control → withdraw.
- Victory: sink/cripple, drive off, or protect the convoy/mission.
- Display: formation plot and damage board, with a salvo timeline.
- Do not reuse “troops” or “morale” labels; use hulls, readiness, and cohesion.

### Campaign

Campaign is **not a combat mode**. It is a meta-layer that strings battles of any
mode together:

- persistent forces, morale, supply, and objectives;
- a sequence of battles with branching outcomes;
- replacements, repairs, and fatigue between battles;
- strategic victory conditions;
- a strategic map/timeline.

“All of them” means a campaign can contain skirmishes, battles, frontlines,
dogfights, and naval actions. It should be built **after** the battle modes it
needs to string together.

## 4. What changes in the UI

The current single form cannot serve all modes. The first screen becomes a mode
chooser; the form, board, and result card change with the mode:

| Mode | Form | Primary visual | Result card |
|---|---|---|---|
| Skirmish | Small-unit factors | Firefight timeline | “Objective taken / routed” |
| Battle | Current team form | Momentum + phase cards | “Pyrrhic victory” etc. |
| Frontlines | Sectors + reserves | Front-line map | “Breakthrough / line collapsed” |
| Dogfight | Aircraft + pilots | Altitude/energy plot | “3 kills, 1 loss, driven off” |
| Naval | Ships + doctrine | Formation/damage board | “2 hulls sunk, convoy saved” |
| Campaign | Strategic state | Map/timeline | “Campaign won / stalled” |

The shared shell stays: header, seed, rules version, playback, export. The center
of the screen is mode-specific.

## 5. Build order

Do not build six shallow modes at once. Build vertical slices:

1. **Mode framework + Skirmish/Battle** — ✅ done. Tagged `Mode`, ground
   profiles, mode picker, CLI `--mode`, mode-aware labels, replay tests.
2. **Dogfight** — individual aircraft, energy, missiles; highest visual payoff.
3. **Naval** — individual ships, detection, salvos, damage.
4. **Frontlines** — sectors, reserves, breakthroughs; the biggest model change.
5. **Campaign** — persistent state over the modes that exist.

Rough effort, part-time:

| Slice | Effort | Status |
|---|---|---|
| Framework + ground refactor | 2–3 days | done |
| Skirmish + Battle polish | ~1 week | in progress |
| Dogfight | 1–2 weeks | planned |
| Naval | 1–2 weeks | planned |
| Frontlines | 2–3 weeks | planned |
| Campaign | 2–3 weeks | planned |

Implemented in this slice:

- `mode` on `BattleInput`; `battle` is the default and omitted from serialized
  Battle inputs to preserve existing hashes.
- Ground profiles for Battle and Skirmish (different strength weights, casualty
  multiplier, morale-swing multiplier).
- Mode-aware phase labels, force labels, outcome classification, and CLI/GUI
  selection.
- Committed Battle and Skirmish logs with replay tests.
- Planned modes are rejected by validation with a clear error rather than being
  silently simulated as Battle.

This is a multi-month product, not a single sprint. The shared kernel keeps it
from becoming six codebases, but each mode still needs its own validation, tests,
UI, and balance pass.

## 6. Outcome taxonomy

All modes can share a small result vocabulary, with mode-specific labels:

- **Decisive** — objective taken, enemy routed/annihilated, own force intact.
- **Pyrrhic** — objective taken, but at near-equal cost.
- **Marginal** — objective taken or held, but both sides exhausted.
- **Stalemate** — no objective, comparable remaining strength.
- **Rout** — cohesion collapsed before the force was destroyed.
- **Withdrawal** — one side breaks contact and preserves force.
- **Mission failure** — the objective was lost even if casualties were low.

The mode-specific card supplies the nouns: troops, hulls, aircraft, sectors.

## 7. Open decisions

1. Is “Skirmish” one mode or two (e.g. raid vs pitch battle)? The current list
   has it twice; assume one.
2. Is Campaign a wrapper over battles, or a full strategic layer with its own
   map/economy? These are very different scopes.
3. Do Dogfight and Naval use individual aircraft/ships (recommended) or
   aggregated squadrons/fleets? Individual units are more legible and justify the
   mode-specific display.
4. Do Frontlines use fixed 3–5 sectors or a real map? Start fixed sectors; a map
   can come later.
5. How much realism vs playability? The engine should be transparent, not
   pretend to be a physics simulation.

## 8. Recommendation

Build the mode framework and the **Skirmish + Battle** vertical slice first. It
reuses the existing engine, proves the mode picker and schema, and gives the UI a
real shape. Then add Dogfight and Naval as genuinely different boards. Frontlines
and Campaign come last because they need the most new machinery.

The alternative — six modes with one generic form and one table — would look like
six skins and satisfy none of them.
