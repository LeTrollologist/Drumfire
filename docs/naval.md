# Drumfire naval rules — v1

Naval mode is a surface action between small fleets. Ships are individual hulls;
detection, weapons, air cover, damage control, and readiness decide a sequence of
salvos.

Status: implemented in the engine, CLI, and GUI.

---

## 1. Inputs

```json
{
  "mode": "naval",
  "attacker": {
    "name": "Red Fleet",
    "ships": 6,
    "hull_quality": 6,
    "sensors": 6,
    "weapons": 7,
    "air_cover": 5,
    "damage_control": 5,
    "readiness": 72
  },
  "defender": { "...": "same shape" },
  "config": {
    "max_salvos": 8,
    "scale": 2.0,
    "hit_rate": 0.14,
    "readiness_break": 20
  },
  "seed": "64 hex characters"
}
```

| Field | Range | Meaning |
|---|---|---|
| `ships` | 1–32 | Hulls committed to the action. |
| `hull_quality` | 1–10 | Armor, compartmentation, and damage resistance. |
| `sensors` | 1–10 | Radar, sonar, EW, and fire control. |
| `weapons` | 1–10 | Missiles, torpedoes, guns, and magazine depth. |
| `air_cover` | 1–10 | Carrier air, SAMs, and point defense. |
| `damage_control` | 1–10 | Crew quality, repair parties, and flooding/fire control. |
| `readiness` | 0–100 | Formation cohesion and willingness to stay engaged. |

`max_salvos` is the hard cap (1–20). `scale` controls how sharply strength
differences translate into a salvo advantage. `hit_rate` is base lethality;
`readiness_break` is the withdrawal threshold.

---

## 2. Strength

For one fleet with `S` ships and `R` readiness:

```text
strength = ln(1 + S)
         + 0.30 * (hull_quality - 5)
         + 0.30 * (sensors - 5)
         + 0.25 * (weapons - 5)
         + 0.15 * (air_cover - 5)
         + 0.15 * (damage_control - 5)
         + 0.02 * (readiness - 50)
```

The salvo win probability is the shared logistic:

```text
p = 1 / (1 + exp(-(strength_attacker - strength_defender) / scale))
```

clamped to `[0.000001, 0.999999]`.

---

## 3. A salvo

1. Compute both strengths from current ships and readiness.
2. Compute `p`.
3. Draw `u1`, `u2`, `u3` from the seeded ChaCha20 stream in that order.
4. The attacker wins the salvo if `u1 < p`; otherwise the defender wins.
5. `edge = |2p - 1|`.
6. Damage:
   ```text
   damage = hit_rate * (0.70 + 0.60*u2) * (0.80 + 0.40*edge)
   ```
7. **Ships sunk by the winner**:
   ```text
   sunk = max(1, round(loser_ships * damage))
   ```
   capped at the loser’s remaining ships. If `damage == 0`, no ships sink.
8. **Ships sunk by the loser**:
   ```text
   sunk = round(winner_ships * damage * 0.20 * u3)
   ```
   capped at the winner’s remaining ships; may be zero.
9. Readiness:
   ```text
   winner_gain = +(2.0 + 2.0 * (1.0 - edge))
   loser_loss  = -(5.0 + 20.0 * loss_fraction)
   ```
   clamped to `0..=100`.
10. Record the salvo.

Ships never increase. A salvo is an abstract detection-and-fire cycle, not a
missile-by-missile simulation.

---

## 4. Ending

After each salvo:

- **Fleet sunk**: one side reaches zero ships. Both zero is a draw.
- **Withdrawal**: one or both sides reach `readiness_break`. If both withdraw,
  the side with more ships wins; ties go to readiness; then draw.
- **Salvo cap**: after `max_salvos`, compare ships; then readiness; then draw.

Endings are recorded as `sunk`, `withdrawal`, `max_salvos`, or `draw`.

---

## 5. Outcome labels

- **Decisive victory** — enemy fleet sunk with most of your ships intact.
- **Pyrrhic victory** — the winner loses half or more of its ships.
- **Marginal victory** — salvo cap reached with a modest advantage.
- **Rout** — the enemy withdraws due to readiness collapse.
- **Exhaustion** — both fleets are badly depleted at the salvo cap.
- **Draw** — no winner.

---

## 6. Example

`fixtures/naval.json`: Red Fleet (6 ships, weapons 7) versus Blue Fleet (8 ships,
better hulls and air cover). The committed `fixtures/naval.log.json` resolves in
7 salvos:

```text
Result: Blue Fleet — Decisive victory after 7 salvos.
Ending: fleet sunk.
Final ships: 0 / 7
Final readiness: 0 / 81
```

Red loses all six ships; Blue loses one. The log is covered by a replay
regression test.

---

## 7. What this mode is not

- It is not a missile-by-missile or damage-model physics simulation.
- It does not track individual ship classes, ammunition, or flooding.
- It does not yet model submarines, carriers as separate air wings, or amphibious
  operations.
- It does not use ground troops or dogfight aircraft.

Those are deliberate limits. The goal is a transparent, replayable fleet action
that a roleplay can narrate.
