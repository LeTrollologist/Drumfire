# Drumfire Campaign — design and implementation plan

**Status:** Halcyon Sweep vertical slice implemented (CLI, three operations,
campaign replay). The full seven-node war-room campaign remains the target plan
below.

**Goal:** make Campaign the capstone mode: a short, replayable, combined-arms
campaign that uses Skirmish, Battle, Frontlines, Dogfight, and Naval as
interlocking parts of one war story — not a menu of unrelated minigames.

---

## 0. What shipped first

The **Halcyon Sweep** slice is playable now:

- 3 nodes: Vantage Ridge, Halcyon Airbase, Port Callas.
- 3 formations: Old Ironsides (ground), Nightjars (air), Salt Watch (naval).
- 3 operations: Frontlines, Dogfight, Naval.
- 4 resources: political will, supply, intel, momentum.
- 1 enemy intent: Hold.
- Cross-mode effects: winning the air battle raises naval air cover; losing
  ground strength reduces the next ground operation.
- Campaign log with every mode log embedded, plus exact campaign replay.
- CLI: `campaign play`, `campaign demo`, `campaign replay`.

The first playable path is deliberately three operations. If the handoffs are
not fun at this scale, adding four more nodes will not fix it.

```sh
cargo run -p drumfire-cli -- campaign demo
cargo run -p drumfire-cli -- campaign play
cargo run -p drumfire-cli -- campaign replay fixtures/campaign.log.json
```

---

## 1. The one-sentence pitch

**Command a coalition through a 10-operation theater: choose where to push, spend
scarce reserves, win the air and sea before the ground, and watch named formations
carry the scars into the next battle — then export the whole war as a chronicle.**

---

## 2. Design pillars

1. **Every battle changes the next one.** Losses, control, supply, intel, and
   morale persist. A pyrrhic victory is still a victory, but it costs the next
   operation.
2. **Combined arms is mandatory.** Ground offensives need air cover; amphibious
   options need naval control; airbases and bridges need raids or breakthroughs.
   The player cannot win by spamming one mode.
3. **Command is choice under fog.** The player allocates formations and chooses
   an operation; the enemy has an intent that is only partly visible. Recon and
   air superiority buy information.
4. **The campaign writes a war story.** Named formations, operation names,
   generated dispatches, and a final chronicle make the result feel authored.
5. **Short, seeded, replayable, auditable.** 8–12 operations, 45–90 minutes, one
   seed, exact campaign replay. No save-scumming; the log is the save.

---

## 3. The theater: Kestrel Coast

One theater, seven nodes, three acts. Each node is a card on a paper map, not a
hex grid. Connections define what can be attacked next.

| Node | Kind | Why it matters | Typical modes |
|---|---|---|---|
| **Vantage Ridge** | Border ridge | Gateway to the coast; start line | Frontlines, Skirmish |
| **Halcyon Airbase** | Airbase | Air superiority over the theater | Dogfight, Skirmish |
| **Orne Bridge** | River crossing | Supply route and rail access | Skirmish, Battle |
| **Port Callas** | Deep-water port | Naval access, amphibious flank, supply | Naval, Battle |
| **Sable Junction** | Rail hub | Enemy reinforcement; shortens the war | Frontlines, Battle |
| **Vesper Works** | Industrial city | Enemy political will; heavy garrison | Battle, Frontlines |
| **Meridian** | Capital | Final objective; requires a coalition of advantages | Frontlines, Battle |

Connections:

```text
Vantage Ridge ── Halcyon Airbase ── Sable Junction ── Meridian
      │                 │                  │
      └── Orne Bridge ── Port Callas ── Vesper Works ── Meridian
```

The player starts at Vantage Ridge. Meridian is only assaultable when at least
two of these are true:

- Sable Junction is friendly;
- Port Callas is friendly;
- Halcyon Airbase is friendly and air superiority is at least 60.

This is the core “combined arms gate.” It forces the player to use at least three
modes before the finale.

---

## 4. The campaign loop

Each turn is one **operation**. A campaign runs for a maximum of 12 turns.

```text
SITUATION
  ├─ enemy intent (revealed by intel)
  ├─ supply / political will / intel
  └─ formation status
      ↓
PLANNING
  ├─ choose node + operation type
  ├─ commit formations
  ├─ choose stance (bold / measured / cautious)
  └─ allocate support (air / naval / reserves)
      ↓
BATTLE
  └─ hand off to the existing mode with campaign-modified inputs
      ↓
AFTERMATH
  ├─ casualties, morale, experience, traits
  ├─ node control and objectives
  ├─ supply / will / intel changes
  └─ chronicle entry
      ↓
ENEMY TURN
  ├─ intent resolves (counterattack, reinforce, interdict, withdraw)
  └─ next situation
```

The player never enters a mode “cold.” Every battle has a briefing that states
what is at stake and what support is available.

---

## 5. Resources and state

Keep four strategic resources. More than four becomes a spreadsheet.

| Resource | Range | Gained by | Spent / lost by | Effect |
|---|---:|---|---|---|
| **Political Will** | 0–100 | Objectives, decisive wins, low casualties | Losses, pyrrhic wins, enemy counterattacks | At 0, the campaign collapses or negotiates |
| **Supply** | 0–100 | Ports, rail hubs, quiet turns | Operations, enemy interdiction, bad weather | Low supply reduces formation strength and morale recovery |
| **Intel** | 0–100 | Recon skirmishes, air patrols, signals | Enemy counterintel, time | Reveals enemy intent, garrison range, and hidden reserves |
| **Momentum** | -5..+5 | Fast wins, breakthroughs | Stalled attacks, retreats | Modifies initiative and enemy intent |

Momentum is a small modifier, not a fourth economy. It answers “who has the
initiative?”.

### Formation state

Each formation has:

```text
name, branch, strength (0–100), morale (0–100), supply (0–100),
experience (0–5), traits, location, fatigue
```

- **Strength** is the percentage of authorized personnel/aircraft/hulls.
- **Morale** is the formation’s willingness to keep fighting.
- **Supply** is local, separate from the theater supply level.
- **Experience** rises with operations and grants a small rating bonus.
- **Traits** are earned at milestones (“Bridgeheads”, “Ace Flight”, “Convoy
  Raiders”) and change how the formation maps into its mode.

### Starting order of battle

| Formation | Branch | Role |
|---|---|---|
| **1st Armored — “Old Ironsides”** | Ground | Frontline breakthrough, Battle |
| **7th Rifles — “The Ridgewalkers”** | Ground | Skirmish, holding actions |
| **4th Air Wing — “Nightjars”** | Air | Dogfight, escort, air superiority |
| **2nd Naval Group — “Salt Watch”** | Naval | Naval control, convoy, amphibious support |
| **Reserve Corps** | Ground | Replacements, emergency defense |

Five formations are enough to force choices without overwhelming the player.

---

## 6. How each mode is used

The campaign does not ask “which minigame do you want?” It asks “what does this
objective require?”

| Mode | Campaign role | Example operation | Strategic effect |
|---|---|---|---|
| **Skirmish** | Raid, recon, bridge seizure, holding action | “Take the Orne Bridge before dawn” | Opens a route, raises intel, cuts enemy supply, low casualties |
| **Battle** | Set-piece assault or defense of a node | “Break the Vesper Works garrison” | Captures a key node, high casualties, political will swing |
| **Frontlines** | Multi-day advance across a sector | “Roll up the Vantage Ridge line” | Shifts control, unlocks adjacent nodes, drains supply |
| **Dogfight** | Air superiority, escort, interception | “Sweep Halcyon’s skies” | Air cover bonus for ground/naval, reveals enemy movement |
| **Naval** | Sea control, convoy, amphibious flank | “Run the Callas convoy” | Supply, naval gunfire support, amphibious option |

### Cross-mode modifiers

These are the connections that make the campaign a system, not a playlist.

| If the player has… | Then… |
|---|---|
| Air superiority ≥ 60 | Ground and naval operations get +1 air-cover rating; enemy reserves are easier to spot |
| Naval control around Callas | Ground operations near the coast get naval gunfire support (+1 firepower); amphibious assault unlocks |
| Intel ≥ 60 | Enemy intent is revealed; garrison estimates become exact |
| Sable Junction | Enemy reinforcements are slower; enemy counterattack strength drops |
| Low supply | Formation strength and morale recovery are reduced; dogfight sortie rate falls |
| High momentum | Enemy is more likely to withdraw or reinforce; player gets first choice of operation |

This table is the heart of the design. Every mode feeds at least one other mode.

---

## 7. Operation design

An operation is a small package:

```text
Operation {
  id, name, node, mode,
  committed formations,
  stance: Bold | Measured | Cautious,
  support: Air | Naval | Reserves | None,
  seed,
  briefing,
  stakes
}
```

### Stances

| Stance | Bonus | Cost |
|---|---|---|
| **Bold** | +10% strength, +1 momentum on win | Higher casualties; worse on loss |
| **Measured** | No modifier | — |
| **Cautious** | −10% casualties, +supply recovery | −10% strength; may stall |

Stances give the player a meaningful risk dial without a hundred sliders.

### Operation generation

Each node offers 2–4 operation options, gated by control and prerequisites:

- **Vantage Ridge**: Frontlines (main advance) or Skirmish (night raid).
- **Halcyon Airbase**: Dogfight (sweep) or Skirmish (airfield raid).
- **Orne Bridge**: Skirmish (coup de main) or Battle (set-piece crossing).
- **Port Callas**: Naval (convoy/sea control) or Battle (overland assault) or
  Naval + ground (amphibious).
- **Sable Junction**: Frontlines (broad advance) or Battle (rail-hub assault).
- **Vesper Works**: Battle (urban assault) or Frontlines (encirclement).
- **Meridian**: Frontlines or Battle, with the combined-arms gate.

The player can always choose a suboptimal mode, but the briefing tells them what
they are giving up.

---

## 8. Enemy design

The enemy is an **intent deck**, not a full AI. Each turn it draws one intent
based on the campaign state:

| Intent | Trigger | Effect |
|---|---|---|
| **Hold** | Default; enemy strong at the node | Fortifies; no counterattack |
| **Counterattack** | Player overextended or low supply | Attacks a recently taken node with a generated force |
| **Reinforce** | Player threatens a key node | Adds garrison to that node; raises its fortification |
| **Interdict** | Player supply high, naval route open | Raids supply; reduces supply and intel |
| **Withdraw** | Enemy morale low or Capital threatened | Falls back; player gains momentum but not a decisive battle |

Intel reveals the intent at ≥60; below that, the player sees a **rumor** (“enemy
movement on the coast road”) with a confidence level.

The intent deck is deterministic and seeded, so the campaign can be replayed.
It is legible enough that the player can plan around it.

---

## 9. Victory, defeat, and endings

### Victory

- **Military victory:** capture Meridian and hold it for one turn.
- **Political victory:** reduce enemy political will to 0.
- **Negotiated victory:** both sides below 20 will; the player may accept a
  ceasefire with the captured nodes they hold.

### Defeat

- Player political will reaches 0.
- The enemy captures Vantage Ridge and cuts the player’s supply for two turns.
- The player runs out of ground formations and reserves.

### Endings

| Ending | Condition | Tone |
|---|---|---|
| **Decisive victory** | Meridian taken, will ≥ 60 | The coalition dictates terms |
| **Pyrrhic victory** | Meridian taken, will < 30 | A hollow triumph |
| **Negotiated peace** | Both wills < 20 | A bitter compromise |
| **Strategic collapse** | Will 0 | The home front breaks |
| **Military collapse** | No ground formations | The front unravels |

The ending feeds the final chronicle and a campaign score.

---

## 10. The chronicle

Every operation appends a **dispatch**:

```text
OPERATION BRIGHT LANCE
Halcyon Airbase · Day 3

The Nightjars went in low over the ridge at first light. Two flights
did not come home, but the enemy’s radar coverage is broken. The road
to Sable Junction is open.
```

Dispatches are generated from deterministic templates keyed by:

- mode and node;
- winner and ending;
- committed formations and their traits;
- casualties, momentum, and objectives.

No AI generation is required. The result is a readable war story that can be
exported as Markdown or copied into a NationStates dispatch.

The final screen is a **campaign chronicle**: chapters by act, a map of control,
the order of battle, losses, and the ending. This is the shareable artifact.

---

## 11. UI concept

### War Room

- Paper map of Kestrel Coast with seven node cards and connection lines.
- Node cards show control, objective, garrison estimate, and available
  operation types.
- Left rail: political will, supply, intel, momentum.
- Bottom rail: formation roster with strength/morale/supply bars.
- Right rail: enemy intent (revealed or rumored) and the “Plan operation” button.
- A single **Next operation** call to action; no menu maze.

### Briefing

- Operation name and node.
- “What is at stake” paragraph.
- Mode recommendation and why.
- Formation commitment (with fatigue and supply warnings).
- Stance selector.
- Support allocation.
- Predicted odds as a **range**, not a false precise number; the range tightens
  with intel.

### Battle handoff

- The chosen mode opens with a campaign banner:
  “Operation Bright Lance · Halcyon Airbase · Air superiority at stake.”
- On completion, the result returns to the campaign, not a dead end.

### Aftermath

- Result, casualties, control changes.
- “What changed” list: air superiority, supply, intel, will, traits.
- Chronicle entry.
- Enemy intent for the next turn.

### Visual language

- Same Paper theme, but map-first: pins, dashed connection lines, control tints,
  stamped operation names, handwritten-style annotations.
- No spreadsheet as the primary view. The roster is a rail, not the center.

---

## 12. Architecture

Add a new crate:

```text
crates/campaign/
  src/
    lib.rs          # public API
    state.rs        # CampaignState, resources
    theater.rs      # nodes, connections, control
    formation.rs    # formations, traits, losses
    operation.rs    # operation request/result, adapters
    enemy.rs        # intent deck, counterattacks
    chronicle.rs    # dispatch generation
    adapter/        # maps campaign state to each mode's input
```

Dependencies: `drumfire-model`, `drumfire-core` (for all five modes). The campaign
is pure: no I/O, no network, no UI.

### Campaign log

```json
{
  "campaign": "Kestrel Coast",
  "seed": "64 hex",
  "turn": 4,
  "state": { "political_will": 68, "supply": 54, "intel": 61, "momentum": 2 },
  "formations": [ ... ],
  "nodes": [ ... ],
  "operations": [
    {
      "request": { "mode": "dogfight", "node": "halcyon", "stance": "bold" },
      "log": { "...": "the existing mode log" },
      "aftermath": { "losses": {}, "control": {}, "chronicle": "..." }
    }
  ],
  "chronicle": [ ... ]
}
```

`replay_campaign` re-runs every operation from its seed and verifies the whole
campaign. The campaign log is the save file and the audit trail.

---

## 13. What the MVP includes

Do not build the full dream first. The first playable campaign should be:

- One theater: Kestrel Coast, 7 nodes.
- 5 formations: 2 ground, 1 air, 1 naval, 1 reserve.
- 8 operations maximum (a shorter campaign), 3 acts.
- 4 resources: will, supply, intel, momentum.
- 3 stances.
- 5 enemy intents.
- All five battle modes wired through adapters.
- 3 endings: decisive, pyrrhic, collapse.
- CLI campaign playthrough + campaign replay.
- A basic war-room GUI with node cards, resource rail, formation rail, briefing,
  aftermath, and chronicle.
- No traits, weather, economy, diplomacy, or multiple theaters in MVP.

### Explicit non-goals

- No hex map or unit-by-unit movement.
- No production/economy simulation.
- No tactical battle inside the campaign; battles hand off to the existing modes.
- No AI-generated prose; templates only.
- No multiplayer.
- No save-scumming; one seeded timeline.

---

## 14. Implementation plan

### Phase 0 — Design fixtures (2–3 days)

- Write `docs/campaign.md` (done).
- Define the Kestrel Coast node graph and starting order of battle in JSON.
- Write a “paper playthrough” of 8 operations on paper, checking that all five
  modes are used and the resource economy closes.

**Exit:** a fixture campaign can be described turn by turn without code.

### Phase 1 — Campaign state and theater (3–5 days)

- `CampaignState`, resources, node control, connections.
- Formations, strength/morale/supply, losses, refit.
- Validation and invariants.
- Campaign log skeleton.

**Exit:** can load a campaign fixture, apply a fake operation result, and advance
a turn deterministically.

### Phase 2 — Operation adapters (5–7 days)

- Map campaign formations + node + support into each mode’s input:
  - Frontlines: sector troops, fortification, reserves.
  - Battle: forces, ratings, home/fortification.
  - Skirmish: small force, surprise/raid modifiers.
  - Dogfight: aircraft, pilot/airframe/weapons.
  - Naval: ships, sensors/weapons/air cover.
- Map each mode’s result back into losses, control, will, supply, intel,
  momentum.
- Tests: every adapter round-trips and applies exactly once.

**Exit:** a campaign operation can invoke any mode and apply its aftermath.

### Phase 3 — Strategic loop and enemy (5–7 days)

- Operation selection, stances, support allocation.
- Enemy intent deck and counterattacks.
- Victory/defeat/endings.
- Campaign replay.

**Exit:** an 8-operation campaign can be played start to finish in tests/CLI.

### Phase 4 — CLI (3–5 days)

- `drumfire campaign new`
- `drumfire campaign status`
- `drumfire campaign plan`
- `drumfire campaign fight`
- `drumfire campaign replay`
- `drumfire campaign chronicle --markdown`

**Exit:** a full campaign is playable without the GUI.

### Phase 5 — War-room GUI (2–3 weeks)

- Map/node cards and control lines.
- Resource rail, formation rail, enemy intent.
- Briefing screen with stance and support.
- Handoff banner to each mode.
- Aftermath and chronicle.
- Export chronicle as Markdown/PNG.

**Exit:** the campaign is understandable and playable without reading the docs.

### Phase 6 — Balance and polish (1–2 weeks)

- Playtest 10+ campaigns.
- Tune resource flows, casualties, enemy intents.
- Ensure every mode is used in a winning line.
- Add traits, operation names, and dispatch variety if time allows.
- Write tests for invariants and replay.

**Estimated total:** 6–8 weeks part-time for the MVP, not a single sprint.

---

## 15. Acceptance criteria

The MVP is done when:

- A full 8-operation campaign is winnable and losable.
- At least four of the five modes are required in a typical winning run.
- Every operation result changes at least one strategic resource or node.
- A campaign log replays exactly, including every mode log.
- No formation strength, morale, supply, will, or intel goes outside its bounds.
- The GUI can go Situation → Briefing → Battle → Aftermath → Next turn without
  leaving the app.
- The final chronicle is readable and exportable.
- A new player can understand the next action from the war-room screen alone.

---

## 16. Risks and mitigations

| Risk | Why it matters | Mitigation |
|---|---|---|
| **Mode handoff complexity** | Five schemas, five logs, one campaign | Build one adapter at a time; test each in isolation |
| **Campaign becomes a menu** | Player picks a mode with no consequences | Enforce combined-arms gates and cross-mode modifiers |
| **Resource spreadsheet** | Too many numbers, not enough decisions | Four resources maximum; one screen, one next action |
| **Balance drift** | One mode or formation dominates | Paper playthrough, then 10 scripted campaigns |
| **UI scope creep** | Map becomes a tactical game | Node cards and connections only; no hexes |
| **Narrative repetition** | Dispatches feel generated | Template variants keyed by formation traits and outcomes |
| **Save-scumming** | Undermines tension | Seeded campaign log; replay is the save; no rerolls |

---

## 17. The first vertical slice

**Status:** shipped as the CLI slice described in section 0.

Build this before anything else:

**“The Halcyon Sweep”** — 3 turns, 3 operations, CLI only:

1. **Frontlines** at Vantage Ridge (ground advance).
2. **Dogfight** over Halcyon Airbase (air superiority).
3. **Naval** at Port Callas (sea control), with the air-superiority bonus applied.

Persistent state:

- 1 ground formation, 1 air wing, 1 naval group.
- Political will, supply, intel, momentum.
- 3 nodes, 3 connections.
- One enemy intent.

Success criteria:

- The three operations use three different mode engines.
- Air superiority changes the naval operation’s input.
- Ground losses reduce the next operation’s strength.
- The campaign log replays exactly.
- A short chronicle is produced.

This slice proves the core idea — **modes feeding each other** — before building
the full seven-node theater. If it is not fun with three operations, adding four
more nodes will not save it.

---

## 18. Open decisions

1. **Campaign length:** 8 operations (tight, 45–60 min) or 12 (fuller, 90 min)?
   Recommend 8 for MVP.
2. **Map style:** node cards only, or a stylized paper map with pins? Recommend
   pins on a paper map, no hexes.
3. **Traits:** MVP or phase 2? Recommend phase 2; they are flavor, not core.
4. **Formation naming:** fixed named formations or player-created? Recommend
   fixed for character, with rename later.
5. **Failure:** is defeat a restart, or a “lost war” chronicle? Recommend a lost
   war chronicle; failure should still produce a story.
6. **Campaign export:** Markdown only, or also a PNG map? Recommend Markdown
   first, PNG later.
