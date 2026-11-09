# Drumfire scenario and ruleset schema

Campaign mode is data-driven. A scenario defines the war; a ruleset defines which
systems are simulated. The built-in campaign is one scenario, not the engine.

Current file: [`scenarios/kestrel-coast.json`](../scenarios/kestrel-coast.json).

---

## 1. What can be edited today

| Value | Where | Notes |
|---|---|---|
| Scenario name | `name` | Free text |
| Side names | `player_name`, `enemy_name` | Free text |
| City names | `nodes[].name` | Free text; no API required |
| City owner | `nodes[].control` | `player`, `enemy`, or `contested` |
| Fortification | `nodes[].fortification` | 1–10 |
| Garrison | `nodes[].enemy_garrison` | Integer |
| Objective flag | `nodes[].objective` | Boolean |
| Formation names | `formations[].name` | Free text |
| Formation strength/morale/supply | `formations[]` | 0–100 |
| Starting will/supply/intel | `starting_*` | 0–100 |
| Turn limit | `max_turns` | 1–50 |
| Ruleset | `modules` | See below |

The GUI exposes the same values under **Scenario setup** and **Ruleset** as text
fields, checkboxes, and sliders. Values are locked after the first operation so
the campaign log stays replayable.

The theater currently has seven fixed role IDs (`vantage`, `halcyon`, `orne`,
`callas`, `sable`, `vesper`, `meridian`). Names, owners, forts, garrisons,
objectives, and starting forces are editable; arbitrary node topology is the next
schema step.

---

## 2. NationStates capitals

NationStates exposes **`capital`** and **`customcapital`** as public nation
shards. See the [official API documentation](https://www.nationstates.net/pages/api.html).

The client requests:

```text
?nation=Testlandia&q=name+fullname+region+population+category+capital+customcapital+motto+currency+flag
```

Rules:

- Use `customcapital` when set.
- Otherwise use `capital`.
- If both are empty, use `"{nation} Capital"` and treat it as generated, not canon.
- One request per nation, all shards combined.
- Public shards only; no authentication.
- Respect `RateLimit-*` and `Retry-After`; the existing rate gate does this.
- Never scrape an entire region through per-nation API calls.

The NS API does **not** provide forts, garrisons, tank counts, ship counts, or
alliances. Those are scenario values or campaign-defined relationships.

In the GUI, use **Scenario setup** to type a capital name directly. The CLI prints
the imported capital with:

```sh
cargo run -p drumfire-cli -- nation Testlandia
```

---

## 3. Ruleset modules

```json
{
  "modules": {
    "advanced_rules": true,
    "intel": true,
    "logistics": true,
    "manpower": true,
    "reserve_manpower": true,
    "tanks": true,
    "planes": true,
    "ships": true,
    "satellites": true,
    "strategic_weapons": false,
    "rods_from_god": false,
    "nuclear_weapons": false,
    "population": true,
    "population_stability": true,
    "occupation": true,
    "liberation": false,
    "nationstates": true,
    "alliances": true,
    "diplomacy": true
  }
}
```

### Dependencies

- `reserve_manpower` requires `manpower`.
- `population_stability` requires `population`.
- `occupation` and `liberation` require `population`.
- `rods_from_god` requires `satellites` and `strategic_weapons`.
- `nuclear_weapons` requires `strategic_weapons`.
- `alliances` requires `diplomacy`.

Invalid combinations are rejected by scenario validation and by campaign play.

### Active effects

| Module | Effect today |
|---|---|
| `manpower` | Formation strength losses persist; off means abstract strength |
| `logistics` | Supply costs apply; off means no supply drain |
| `tanks` | Ground attacker force gets an armored bonus |
| `planes` | Dogfight operations are available |
| `ships` | Naval operations are available |
| `advanced_rules` | Reserved rules-layer flag |
| `intel` | Reserved fog/intent flag; intent is currently shown |
| `satellites` | Reserved strategic-recon flag |
| `strategic_weapons`, `rods_from_god`, `nuclear_weapons` | Reserved strategic-strike layer |
| `population`, `population_stability` | Reserved society layer |
| `occupation`, `liberation` | Reserved occupation layer |
| `nationstates` | Marks scenarios intended for NS-derived data |
| `alliances`, `diplomacy` | Reserved coalition layer |

The reserved modules are serialized and validated now so scenarios and campaign
logs stay stable as their simulation hooks are added.

### Presets

| Preset | Contents |
|---|---|
| `Abstract` | Minimal rules; no manpower/logistics; planes and ships remain available |
| `Combined Arms` | Default; foundations, manpower, equipment, satellites, society, alliances |
| `Full Spectrum` | Everything, including strategic weapons and liberation |

Custom combinations are allowed if dependencies pass.

---

## 4. Scenario JSON

```json
{
  "name": "War for the Amber Coast",
  "player_name": "Aurelia",
  "enemy_name": "Vermillion",
  "max_turns": 8,
  "starting_will": 70,
  "starting_supply": 60,
  "starting_intel": 40,
  "modules": {
    "planes": true,
    "ships": true,
    "manpower": true,
    "logistics": true
  },
  "nodes": [
    {
      "id": "vantage",
      "name": "Alder Gate",
      "kind": "ridge",
      "control": "contested",
      "objective": true,
      "enemy_garrison": 6000,
      "fortification": 6,
      "connections": ["halcyon", "orne"]
    }
  ],
  "formations": [
    {
      "id": "old-ironsides",
      "name": "1st Armored",
      "branch": "ground",
      "strength": 95,
      "morale": 75,
      "supply": 80,
      "experience": 2
    }
  ]
}
```

A written scenario is a full initial state, not a patch on top of the built-in
one. `campaign scenario --out` writes the built-in template; edit the names and
values, then run `campaign demo --scenario path`.

---

## 5. Campaign log

The campaign log embeds the scenario snapshot and every operation log. Replay
rebuilds from that snapshot, so renaming a city or changing a fort after the war
does not rewrite history.

```sh
cargo run -p drumfire-cli -- campaign demo --scenario scenarios/my-war.json --out my-war.log.json
cargo run -p drumfire-cli -- campaign replay my-war.log.json
```

---

## 6. Moddability

The module list is serialized by name. Unknown module keys should be preserved by
future loaders rather than dropped. New modules can be added without changing the
campaign state machine: define the module, its dependencies, and the fields it
reads or writes.

The next schema step is arbitrary node topology: add/remove cities and routes
instead of the seven fixed role IDs. The current editor covers names, ownership,
forts, garrisons, objectives, formations, starting resources, turn limits, and
the ruleset.
