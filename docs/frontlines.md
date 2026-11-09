# Drumfire front-line rules — v1

Frontlines mode is a multi-sector battle. Each day resolves every contested
sector; fortifications favor the defender, reserves replace losses, and objective
sectors decide whether the front has been broken.

Status: implemented in the engine, CLI, and GUI.

---

## 1. Inputs

```json
{
  "mode": "frontlines",
  "attacker": {
    "name": "Red Front",
    "reserves": 5000,
    "morale": 70,
    "tech": 6,
    "training": 6,
    "experience": 6,
    "logistics": 5,
    "intel": 5
  },
  "defender": { "...": "same shape" },
  "sectors": [
    {
      "name": "North",
      "attacker_troops": 4000,
      "defender_troops": 5000,
      "fortification": 6,
      "objective": false
    },
    {
      "name": "Center",
      "attacker_troops": 8000,
      "defender_troops": 6000,
      "fortification": 7,
      "objective": true
    },
    {
      "name": "South",
      "attacker_troops": 5000,
      "defender_troops": 5000,
      "fortification": 5,
      "objective": true
    }
  ],
  "config": {
    "max_days": 10,
    "scale": 2.5,
    "casualty_rate": 0.12,
    "morale_rout_threshold": 20,
    "replenish_rate": 0.25
  },
  "seed": "64 hex characters"
}
```

| Field | Range | Meaning |
|---|---|---|
| `sectors` | 3–5 | Sectors of the front. The GUI uses three. |
| `attacker_troops` / `defender_troops` | > 0 | Troops initially committed to that sector. |
| `fortification` | 1–10 | Defender bonus in that sector. |
| `objective` | boolean | Whether capturing the sector counts toward a breakthrough. |
| `reserves` | integer ≥ 0 | Troops available to replace daily losses. |
| `morale` | 0–100 | Front-wide cohesion. |
| `tech`, `training`, `experience`, `logistics`, `intel` | 1–10 | Front-wide quality. |

At least one sector must be an objective, and sector names must be unique.

---

## 2. Sector strength

For one side in a sector with `T` troops:

```text
S = ln(1 + T)
  + 0.30 * (tech - 5)
  + 0.25 * (training - 5)
  + 0.20 * (experience - 5)
  + 0.20 * (logistics - 5)
  + 0.15 * (intel - 5)
  + defender_bonus
```

where `defender_bonus = 0.20 * (fortification - 5)` for the defender, and `0`
for the attacker.

The sector win probability is the shared logistic:

```text
p = 1 / (1 + exp(-(S_attacker - S_defender) / scale))
```

---

## 3. A day

For every contested sector, in input order:

1. Compute both sector strengths.
2. Compute `p`.
3. Draw `u1`, `u2`, `u3` from the seeded ChaCha20 stream.
4. The attacker wins the sector if `u1 < p`; otherwise the defender wins.
5. `edge = |2p - 1|`.
6. Base intensity:
   ```text
   base = casualty_rate * (0.65 + 0.70*u2)
   ```
7. Loser’s loss fraction:
   ```text
   loser_frac = clamp(base * (0.80 + 0.60*edge), 0.0, 0.45)
   ```
8. Winner’s loss fraction:
   ```text
   winner_frac = clamp(base * 0.35 * (0.60 + 0.80*u3), 0.0, 0.25)
   ```
9. Apply losses. If the defender reaches zero, the sector is **captured**.
10. Morale changes:
    - sector win: `+1` for the winner, `-2` for the loser;
    - capture: an additional `+5` for the attacker, `-5` for the defender.

After all sectors:

- Replace a share of the day’s losses from reserves:
  ```text
  replacements = min(reserves, round(total_losses * replenish_rate))
  ```
  Replacements are distributed evenly across uncaptured sectors.
- Clamp morale to `0..=100`.

---

## 4. Ending

After each day:

- **Breakthrough**: all objective sectors are captured, or the defender has no
  troops or reserves left.
- **Exhaustion**: the attacker has no troops or reserves left.
- **Withdrawal**: one side’s morale reaches `morale_rout_threshold`.
- **Day cap**: after `max_days`, compare captured objectives; then total troops
  plus reserves; then morale.

Endings are recorded as `breakthrough`, `exhaustion`, `withdrawal`, `max_days`,
or `draw`.

---

## 5. Outcome labels

- **Decisive victory** — breakthrough with most of the attacking force intact.
- **Pyrrhic victory** — breakthrough at half or greater losses.
- **Marginal victory** — day cap with a modest advantage.
- **Rout** — enemy withdrawal due to morale collapse.
- **Exhaustion** — one side runs out of troops and reserves.
- **Draw** — no winner.

---

## 6. Example

`fixtures/frontline.json`: Red Front attacks North, Center, and South against
Blue Front. Center and South are objectives. The committed
`fixtures/frontline.log.json` resolves in 10 days:

```text
Result: Blue Front — Marginal victory after 10 days.
Ending: day cap.
Objectives: 0 / 2
Final reserves: 2686 / 2805
Final morale: 55 / 55
```

The attacker ends with more troops in the field but never captures an objective,
so the defender wins on the day cap. The log is covered by a replay regression
test.

---

## 7. What this mode is not

- It is not a hex map or a unit-by-unit movement model.
- It does not model individual battalions, supply lines, or command decisions.
- Reserves are automatic replacements, not a player-controlled operational
  reserve.
- It does not include air or naval assets; those live in their own modes.

Those are deliberate limits. Frontlines is a sector-pressure model, not a
full operational wargame.
