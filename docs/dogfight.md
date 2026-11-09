# Drumfire dogfight rules — v1

Dogfight mode is an air-to-air engagement between small flights of aircraft. It
is deliberately not the ground model with different labels: aircraft counts are
whole airframes, cohesion replaces morale, and pilot/airframe/weapons/sensors are
the driving ratings.

Status: implemented in the engine, CLI, and GUI.

---

## 1. Inputs

```json
{
  "mode": "dogfight",
  "attacker": {
    "name": "Red Flight",
    "aircraft": 6,
    "pilot_skill": 7,
    "aircraft_quality": 6,
    "weapons": 6,
    "radar": 5,
    "support": 5,
    "cohesion": 70
  },
  "defender": { "...": "same shape" },
  "config": {
    "max_passes": 8,
    "scale": 2.0,
    "kill_rate": 0.12,
    "cohesion_break": 20
  },
  "seed": "64 hex characters"
}
```

| Field | Range | Meaning |
|---|---|---|
| `aircraft` | 1–64 | Aircraft committed to this engagement. |
| `pilot_skill` | 1–10 | Training, experience, and situational awareness. |
| `aircraft_quality` | 1–10 | Speed, climb, turn, sensors, and endurance. |
| `weapons` | 1–10 | Missiles, guns, and fire-control quality. |
| `radar` | 1–10 | Detection, tracking, and electronic warfare. |
| `support` | 1–10 | GCI, AWACS, tankers, and sustainment. |
| `cohesion` | 0–100 | Formation discipline and willingness to stay engaged. |

`max_passes` is the hard cap (1–30). `scale` controls how sharply strength
differences translate into a pass advantage. `kill_rate` is the base lethality;
`cohesion_break` is the break-off threshold.

---

## 2. Strength

For one flight with `A` aircraft and `C` cohesion:

```text
S = ln(1 + A)
  + 0.35 * (pilot_skill - 5)
  + 0.30 * (aircraft_quality - 5)
  + 0.20 * (weapons - 5)
  + 0.15 * (radar - 5)
  + 0.15 * (support - 5)
  + 0.02 * (cohesion - 50)
```

The pass win probability is the same logistic form as the ground engine:

```text
p = 1 / (1 + exp(-(S_attacker - S_defender) / scale))
```

clamped to `[0.000001, 0.999999]`.

---

## 3. A pass

1. Compute both strengths from current aircraft and cohesion.
2. Compute `p`.
3. Draw `u1`, `u2`, `u3` from the seeded ChaCha20 stream in that order.
4. The attacker wins the pass if `u1 < p`; otherwise the defender wins.
5. `edge = |2p - 1|`.
6. Intensity:
   ```text
   intensity = kill_rate * (0.70 + 0.60*u2) * (0.80 + 0.40*edge)
   ```
7. **Kills by the winner** against the loser:
   ```text
   kills = max(1, round(loser_aircraft * intensity))
   ```
   capped at the loser’s remaining aircraft. If `intensity == 0`, no kills occur.
8. **Kills by the loser** against the winner:
   ```text
   kills = round(winner_aircraft * intensity * 0.20 * u3)
   ```
   capped at the winner’s remaining aircraft; may be zero.
9. Cohesion:
   ```text
   winner_gain = +(2.0 + 2.0 * (1.0 - edge))
   loser_loss  = -(4.0 + 15.0 * loss_fraction)
   ```
   clamped to `0..=100`.
10. Record the pass.

Aircraft counts never increase. A pass is an abstract few minutes of maneuvering,
not a literal missile-by-missile simulation.

---

## 4. Ending

After each pass:

- **Splashed**: one side reaches zero aircraft. Both zero is a draw.
- **Break-off**: one or both sides reach `cohesion_break`. If both break, the side
  with more aircraft wins; ties go to cohesion; then draw.
- **Pass cap**: after `max_passes`, compare aircraft; then cohesion; then draw.

Endings are recorded as `splashed`, `break_off`, `max_passes`, or `draw`.

---

## 5. Outcome labels

Dogfights use the shared outcome vocabulary:

- **Decisive victory** — one side is splashed with most of its aircraft intact.
- **Pyrrhic victory** — the winner loses half or more of its aircraft.
- **Marginal victory** — pass cap reached with a modest advantage.
- **Rout** — the enemy breaks off due to cohesion.
- **Exhaustion** — both sides are badly depleted at the pass cap.
- **Draw** — no winner.

---

## 6. Example

`fixtures/dogfight.json`: Red Flight (6 aircraft, pilot 7) versus Blue Flight
(8 aircraft, home-quality airframes). The committed `fixtures/dogfight.log.json`
resolves in 8 passes:

```text
Result: Blue Flight — Marginal victory after 8 passes.
Ending: pass cap.
Final aircraft: 2 / 4
Final cohesion: 52 / 61
```

The log is covered by a replay regression test.

---

## 7. What this mode is not

- It is not a missile-by-missile or energy-maneuver physics model.
- It does not model individual pilot names, fuel states, or terrain.
- It does not use ground morale or troop counts.
- It does not yet include bombers, escorts, or mission packages; those belong in
  a scenario layer once the core dogfight is tuned.

Those are deliberate limits. The goal is a transparent, replayable air combat
result that a roleplay can narrate.
