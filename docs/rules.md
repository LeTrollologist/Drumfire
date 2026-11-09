# Drumfire combat rules — v1

This document is the authoritative description of the Phase 1 engine. If the
code and this document disagree, the code is wrong.

The model is intentionally simple and legible. It is not a physics simulation,
and it does not claim to predict real warfare. It exists to give roleplayers a
consistent, explainable answer to “who wins this battle?”.

This document covers the **ground** modes (Skirmish and Battle). Dogfight and
Naval use separate models documented in [`dogfight.md`](dogfight.md) and
[`naval.md`](naval.md).

---

## 1. Inputs

The input carries a `mode`:

| `mode` | Status | What it represents |
|---|---|---|
| `battle` (default) | implemented | A large single engagement between two aggregate forces |
| `skirmish` | implemented | A small-unit fight; same ground family, different profile |
| `frontlines`, `dogfight`, `naval` | reserved | Planned; validation rejects them for now |

Each side supplies:

| Symbol | Field | Range | Meaning |
|---|---|---|---|
| `F` | `forces` | integer > 0 | Troops committed to this battle. |
| `M` | `morale` | 0–100 | Will to fight. |
| `T` | `tech` | 1–10 | Technology level. |
| `R` | `training` | 1–10 | Training quality. |
| `E` | `experience` | 1–10 | Combat experience. |
| `L` | `logistics` | 1–10 | Supply and sustainment. |
| `I` | `intel` | 1–10 | Information advantage. |
| `H` | `home` | boolean | Home-soil advantage. |

At most one side may have `home: true`. A neutral battle has both `false`.

The configuration is:

| Field | Default | Range |
|---|---:|---:|
| `max_rounds` | 12 | 1–100 |
| `scale` | 2.5 | 0.1–20.0 |
| `home_bonus` | 0.35 | -2.0–2.0 |
| `casualty_rate` | 0.12 | 0.0–0.5 |
| `morale_rout_threshold` | 20 | 0–100 |

`max_rounds` is retained for schema compatibility; the UI calls these intervals
“phases” in Battle and “exchanges” in Skirmish.

---

## 2. Strength score

For a side with current forces `F` and current morale `M`:

```text
S = w_F * ln(1 + F)
  + w_M * (M - 50)
  + w_T * (T - 5)
  + w_R * (R - 5)
  + w_E * (E - 5)
  + w_L * (L - 5)
  + w_I * (I - 5)
  + H
```

where `H = home_bonus` if the side is the home team, otherwise `0`.

The weights are mode-specific:

| Term | Battle | Skirmish |
|---|---:|---:|
| `w_F` forces | 1.00 | 0.85 |
| `w_M` morale | 0.025 | 0.035 |
| `w_T` tech | 0.30 | 0.25 |
| `w_R` training | 0.25 | 0.35 |
| `w_E` experience | 0.20 | 0.30 |
| `w_L` logistics | 0.20 | 0.10 |
| `w_I` intel | 0.15 | 0.15 |

Battle keeps the original v1 coefficients exactly. Skirmish makes training,
experience, and morale matter more, and material/logistics matter less, because a
small-unit fight is decided more by cohesion and skill than by stockpiles.

Design rationale:

- **Forces are logarithmic.** Doubling from 10,000 to 20,000 is meaningful, but
  not an automatic win; `w_F * ln(1 + F)` prevents a 100:1 stack from producing
  an infinite score.
- **Ratings are centered at 5.** A rating of 5 contributes zero; 10 contributes
  `+5 × coefficient`, 1 contributes `−4 × coefficient`. This makes “average”
  the neutral baseline and keeps the coefficients interpretable.
- **Morale is centered at 50.** Battle keeps the coefficient small; Skirmish
  raises it because cohesion breaks faster at small-unit scale.
- **Everything is additive.** There are no ratios, no divisions by ratings, and
  no integer truncation. A zero-valued input cannot produce a division error.

Initial strengths use the starting forces and morale. Later intervals recompute
`S` from the current values, so casualties and morale shocks feed back into the
next phase.

---

## 3. Interval win probability

The attacker wins an interval with probability

```text
p = 1 / (1 + exp(-(S_attacker - S_defender) / scale))
```

`p` is clamped to `[0.000001, 0.999999]` so that no interval is a foregone
conclusion. A larger `scale` flattens the curve (strength matters less); a
smaller `scale` makes it steeper.

Examples at the default `scale = 2.5`:

| Strength difference | Attacker `p` |
|---:|---:|
| -5.0 | 11.9% |
| -2.5 | 26.9% |
| 0.0 | 50.0% |
| +2.5 | 73.1% |
| +5.0 | 88.1% |

---

## 4. Resolving one interval

For each phase, in order:

1. Compute `S_a` and `S_d` from the **current** forces and morale.
2. Compute `p` as above.
3. Draw three `f64` values from the battle’s ChaCha20 stream, in this exact
   order:
   - `u1` — the interval roll;
   - `u2` — casualty intensity;
   - `u3` — winner’s casualty variation.
4. The attacker wins the interval if `u1 < p`; otherwise the defender wins.
5. Let `edge = |2p − 1|`, a number in `[0, 1]` describing how lopsided the
   interval was.
6. Apply the mode’s volatility multipliers:

   | Multiplier | Battle | Skirmish |
   |---|---:|---:|
   | `casualty_multiplier` | 1.00 | 1.25 |
   | `morale_swing_multiplier` | 1.00 | 1.30 |

7. Base intensity:
   ```text
   base = casualty_rate * casualty_multiplier * (0.65 + 0.70 * u2)
   ```
8. Loser’s casualty fraction:
   ```text
   loser_frac = clamp(base * (0.80 + 0.60 * edge), 0.0, 0.45)
   ```
9. Winner’s casualty fraction:
   ```text
   winner_frac = clamp(base * 0.35 * (0.60 + 0.80 * u3), 0.0, 0.25)
   ```
10. Convert fractions to integer losses:
    ```text
    loss = round(forces * fraction)
    ```
    The loser loses at least 1 troop when `loser_frac > 0` (and loses no more
    than it has). The winner may lose 0. Forces never increase and never go below
    zero.
11. Morale changes:
    ```text
    winner_delta = +(2.0 + 3.0 * (1.0 - edge)) * morale_swing_multiplier
    loser_delta  = -(3.0 + 40.0 * loss_fraction) * morale_swing_multiplier
    ```
    where `loss_fraction = losses / forces_before`. Clamp morale to `0..=100`.
12. Append a `RoundLog` with strengths, `p`, `u1`, winner, losses, remaining
    forces, and remaining morale.

The winner’s morale gain is larger in close intervals (`+5`) than in lopsided
ones (`+2`): a hard-fought interval is worth more psychologically than a
walkover. The loser’s morale loss scales with the fraction of troops lost.
Skirmish multiplies both effects by 1.30, so cohesion breaks faster.

---

## 5. Ending the battle

After each interval, check in this order:

1. **Annihilation.** If either side has zero forces:
   - both zero → draw;
   - defender zero → attacker wins;
   - attacker zero → defender wins.
2. **Rout.** If either side is at or below `morale_rout_threshold`:
   - only one side routs → the other wins;
   - both rout → higher morale wins; if equal, higher remaining forces wins; if
     still equal, draw.

If `max_rounds` is reached:

- compare final strengths;
- if the strengths are equal within `1e-9`, compare remaining forces;
- if still equal, the battle is a draw.

The ending is recorded as `annihilation`, `rout`, `max_rounds`, or `draw`.

---

## 6. Determinism and replay

- The RNG is `ChaCha20Rng` seeded with the battle’s 32-byte `seed`.
- The draw order in §4 is part of the format. Changing it changes every battle
  and requires a rules-version bump.
- A battle log embeds the input and every round. `replay` re-simulates the input
  and requires exact equality, including `f64` fields.
- JSON parsing uses exact float round-tripping. A log written on one machine
  replays on another; no platform-specific floating-point behaviour is relied
  upon beyond IEEE-754 `f64` arithmetic.
- `input_hash` is SHA-256 over the JSON encoding of the input. It is a
  convenience for confirming two people hold the same battle, not a security
  primitive.
- A `battle` input omits the `mode` field when serialized so old logs and hashes
  remain stable. A `skirmish` input includes `"mode": "skirmish"` and therefore
  has a distinct hash.

**Compatibility rule:** any change to the coefficients, draw order, casualty
formula, morale formula, or ending rules must bump the rules version, update
this document, and regenerate the committed fixtures. Old logs are expected to
remain replayable only under the rules version that produced them.

---

## 7. Worked example — Battle

`fixtures/example.json`:

- Arcadia: 12,000 troops, morale 72, tech 7, training 6, experience 5,
  logistics 6, intel 5, not home.
- Borealis: 15,000 troops, morale 65, tech 6, training 7, experience 6,
  logistics 5, intel 6, home.

Initial strength:

```text
Arcadia  = ln(12001) + 0.025*22 + 0.30*2 + 0.25*1 + 0 + 0.20*1 + 0
         = 9.393 + 0.550 + 0.600 + 0.250 + 0.200
         = 10.993

Borealis = ln(15001) + 0.025*15 + 0.30*1 + 0.25*2 + 0.20*1 + 0.15*1 + 0.35
         = 9.616 + 0.375 + 0.300 + 0.500 + 0.200 + 0.150 + 0.350
         = 11.491
```

Opening probability for Arcadia:

```text
p = 1 / (1 + exp(-(10.993 - 11.491) / 2.5))
  = 0.45035...
```

Phase 1 from the committed log:

| Quantity | Value |
|---|---:|
| `p` (attacker) | 0.450351 |
| `u1` | 0.414456 |
| Interval winner | attacker |
| Attacker losses | 531 |
| Defender losses | 1,316 |
| Forces after | 11,469 / 13,684 |
| Morale after | 77 / 58 |

The committed `fixtures/example.log.json` is the full 12-phase record and is
covered by a replay regression test.

### Skirmish example

`fixtures/skirmish.json` is a small-unit fight: Red Section (120 combatants)
against Blue Section (150, home). Skirmish uses the weights above, a 1.25×
casualty multiplier, and a 1.30× morale-swing multiplier.

The committed `fixtures/skirmish.log.json` resolves in **5 exchanges**:

```text
Result: Red Section — Rout after 5 exchanges.
Ending: rout.
Final forces: 78 / 64
Final morale: 87 / 20
```

Blue Section’s morale collapses even though it still has 64 combatants. That is
the intended Skirmish behaviour: cohesion breaks before the force is destroyed.
The log is also covered by a replay regression test.

---

## 8. Tuning

The coefficients in §2 and the casualty constants in §4 are initial choices, not
natural laws. They should be tuned against recorded roleplay battles and
community feedback, not changed ad hoc. When tuning:

1. change one family of coefficients at a time;
2. run the invariant tests and a batch of representative battles;
3. compare win rates and casualty distributions to the previous rules version;
4. bump the rules version and regenerate fixtures;
5. document the change in the changelog.

The invariant tests require that:

- forces never increase and never go below zero;
- every `p` is strictly between 0 and 1;
- every roll is in `[0, 1)`;
- a battle never exceeds `max_rounds`;
- replay is exact.
