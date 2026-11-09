//! Deterministic air-to-air combat.
//!
//! A dogfight is a small number of aircraft fighting over a series of passes.
//! Unlike the ground model, casualties are whole aircraft, cohesion replaces
//! morale, and energy/sensors/pilot skill are first-class inputs.

use drumfire_model::{AirForce, DogfightInput, Mode, ValidationError};
use rand::Rng;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{win_probability, OutcomeClass, Side, Winner};

/// Why the dogfight ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DogfightEnding {
    /// One side lost all aircraft.
    Splashed,
    /// One or both sides broke off due to cohesion loss.
    BreakOff,
    /// The configured pass cap was reached.
    MaxPasses,
    /// No winner.
    Draw,
}

/// One resolved pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PassLog {
    pub pass: u32,
    pub attacker_strength: f64,
    pub defender_strength: f64,
    pub attacker_win_probability: f64,
    pub roll: f64,
    pub pass_winner: Side,
    pub attacker_kills: u8,
    pub defender_kills: u8,
    pub attacker_aircraft_after: u8,
    pub defender_aircraft_after: u8,
    pub attacker_cohesion_after: u8,
    pub defender_cohesion_after: u8,
}

/// Final aircraft counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AircraftPair {
    pub attacker: u8,
    pub defender: u8,
}

/// Final cohesion values.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CohesionPair {
    pub attacker: u8,
    pub defender: u8,
}

/// The outcome fields of a dogfight.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DogfightResult {
    pub winner: Winner,
    pub ending: DogfightEnding,
    pub final_aircraft: AircraftPair,
    pub final_cohesion: CohesionPair,
}

/// A complete, self-contained dogfight record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DogfightLog {
    pub input: DogfightInput,
    pub input_hash: String,
    pub passes: Vec<PassLog>,
    pub result: DogfightResult,
}

/// Errors from dogfight simulation.
#[derive(Debug, Error)]
pub enum DogfightError {
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error("failed to serialize dogfight input: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// Errors from dogfight replay verification.
#[derive(Debug, Error)]
pub enum DogfightReplayError {
    #[error(transparent)]
    Sim(#[from] DogfightError),
    #[error("replayed dogfight does not match the stored log")]
    Mismatch,
}

fn strength(force: &AirForce, aircraft: u8, cohesion: u8) -> f64 {
    (aircraft as f64 + 1.0).ln()
        + 0.35 * (force.pilot_skill.get() as f64 - 5.0)
        + 0.30 * (force.aircraft_quality.get() as f64 - 5.0)
        + 0.20 * (force.weapons.get() as f64 - 5.0)
        + 0.15 * (force.radar.get() as f64 - 5.0)
        + 0.15 * (force.support.get() as f64 - 5.0)
        + 0.02 * (cohesion as f64 - 50.0)
}

fn clamp_cohesion(value: f64) -> u8 {
    value.round().clamp(0.0, 100.0) as u8
}

fn compare_air(a_aircraft: u8, d_aircraft: u8, a_cohesion: u8, d_cohesion: u8) -> Winner {
    if a_aircraft > d_aircraft {
        Winner::Attacker
    } else if d_aircraft > a_aircraft {
        Winner::Defender
    } else if a_cohesion > d_cohesion {
        Winner::Attacker
    } else if d_cohesion > a_cohesion {
        Winner::Defender
    } else {
        Winner::Draw
    }
}

/// Resolve a dogfight.
pub fn simulate_dogfight(input: &DogfightInput) -> Result<DogfightLog, DogfightError> {
    input.validate()?;

    let input_hash = hex::encode(Sha256::digest(serde_json::to_vec(input)?));
    let config = &input.config;
    let mut rng = ChaCha20Rng::from_seed(*input.seed.as_bytes());

    let mut a_aircraft = input.attacker.aircraft.get();
    let mut d_aircraft = input.defender.aircraft.get();
    let mut a_cohesion = input.attacker.cohesion.get();
    let mut d_cohesion = input.defender.cohesion.get();

    let mut passes = Vec::with_capacity(config.max_passes as usize);
    let mut winner = Winner::Draw;
    let mut ending = DogfightEnding::MaxPasses;

    for pass in 1..=config.max_passes {
        let a_before = a_aircraft;
        let d_before = d_aircraft;
        let a_strength = strength(&input.attacker, a_before, a_cohesion);
        let d_strength = strength(&input.defender, d_before, d_cohesion);
        let p = win_probability(a_strength - d_strength, config.scale);

        let roll: f64 = rng.gen();
        let intensity_roll: f64 = rng.gen();
        let winner_roll: f64 = rng.gen();

        let pass_winner = if roll < p {
            Side::Attacker
        } else {
            Side::Defender
        };
        let edge = (p - 0.5).abs() * 2.0;
        let intensity = config.kill_rate * (0.70 + 0.60 * intensity_roll) * (0.80 + 0.40 * edge);

        let (a_kills, d_kills) = match pass_winner {
            Side::Attacker => {
                let d_kills = if intensity > 0.0 {
                    ((d_before as f64 * intensity).round() as u8)
                        .max(1)
                        .min(d_before)
                } else {
                    0
                };
                let a_kills = ((a_before as f64 * intensity * 0.20 * winner_roll).round() as u8)
                    .min(a_before);
                (a_kills, d_kills)
            }
            Side::Defender => {
                let a_kills = if intensity > 0.0 {
                    ((a_before as f64 * intensity).round() as u8)
                        .max(1)
                        .min(a_before)
                } else {
                    0
                };
                let d_kills = ((d_before as f64 * intensity * 0.20 * winner_roll).round() as u8)
                    .min(d_before);
                (a_kills, d_kills)
            }
        };

        a_aircraft = a_aircraft.saturating_sub(a_kills);
        d_aircraft = d_aircraft.saturating_sub(d_kills);

        let a_loss_fraction = a_kills as f64 / a_before as f64;
        let d_loss_fraction = d_kills as f64 / d_before as f64;
        let winner_gain = 2.0 + 2.0 * (1.0 - edge);
        let (a_delta, d_delta) = match pass_winner {
            Side::Attacker => (winner_gain, -(4.0 + 15.0 * d_loss_fraction)),
            Side::Defender => (-(4.0 + 15.0 * a_loss_fraction), winner_gain),
        };
        a_cohesion = clamp_cohesion(a_cohesion as f64 + a_delta);
        d_cohesion = clamp_cohesion(d_cohesion as f64 + d_delta);

        passes.push(PassLog {
            pass,
            attacker_strength: a_strength,
            defender_strength: d_strength,
            attacker_win_probability: p,
            roll,
            pass_winner,
            attacker_kills: d_kills,
            defender_kills: a_kills,
            attacker_aircraft_after: a_aircraft,
            defender_aircraft_after: d_aircraft,
            attacker_cohesion_after: a_cohesion,
            defender_cohesion_after: d_cohesion,
        });

        if a_aircraft == 0 || d_aircraft == 0 {
            winner = if a_aircraft == 0 && d_aircraft == 0 {
                Winner::Draw
            } else if d_aircraft == 0 {
                Winner::Attacker
            } else {
                Winner::Defender
            };
            ending = if winner == Winner::Draw {
                DogfightEnding::Draw
            } else {
                DogfightEnding::Splashed
            };
            break;
        }

        let a_break = a_cohesion <= config.cohesion_break;
        let d_break = d_cohesion <= config.cohesion_break;
        if a_break || d_break {
            winner = match (a_break, d_break) {
                (true, true) => compare_air(a_aircraft, d_aircraft, a_cohesion, d_cohesion),
                (true, false) => Winner::Defender,
                (false, true) => Winner::Attacker,
                (false, false) => unreachable!("break condition checked above"),
            };
            ending = if winner == Winner::Draw {
                DogfightEnding::Draw
            } else {
                DogfightEnding::BreakOff
            };
            break;
        }
    }

    if ending == DogfightEnding::MaxPasses {
        winner = compare_air(a_aircraft, d_aircraft, a_cohesion, d_cohesion);
        if winner == Winner::Draw {
            ending = DogfightEnding::Draw;
        }
    }

    Ok(DogfightLog {
        input: input.clone(),
        input_hash,
        passes,
        result: DogfightResult {
            winner,
            ending,
            final_aircraft: AircraftPair {
                attacker: a_aircraft,
                defender: d_aircraft,
            },
            final_cohesion: CohesionPair {
                attacker: a_cohesion,
                defender: d_cohesion,
            },
        },
    })
}

/// Re-simulate a stored dogfight log and verify it matches exactly.
pub fn replay_dogfight(log: &DogfightLog) -> Result<DogfightLog, DogfightReplayError> {
    let replayed = simulate_dogfight(&log.input)?;
    if replayed != *log {
        return Err(DogfightReplayError::Mismatch);
    }
    Ok(replayed)
}

/// Classify a dogfight by margin and cost.
pub fn classify_dogfight(log: &DogfightLog) -> OutcomeClass {
    let initial_a = log.input.attacker.aircraft.get() as f64;
    let initial_d = log.input.defender.aircraft.get() as f64;
    let final_a = log.result.final_aircraft.attacker as f64;
    let final_d = log.result.final_aircraft.defender as f64;

    let (winner_loss, loser_loss) = match log.result.winner {
        Winner::Attacker => (1.0 - final_a / initial_a, 1.0 - final_d / initial_d),
        Winner::Defender => (1.0 - final_d / initial_d, 1.0 - final_a / initial_a),
        Winner::Draw => return OutcomeClass::Draw,
    };

    match log.result.ending {
        DogfightEnding::BreakOff => OutcomeClass::Rout,
        DogfightEnding::Splashed => {
            if winner_loss < 0.5 {
                OutcomeClass::DecisiveVictory
            } else {
                OutcomeClass::PyrrhicVictory
            }
        }
        DogfightEnding::MaxPasses => {
            if winner_loss >= 0.8 * loser_loss {
                OutcomeClass::PyrrhicVictory
            } else if winner_loss > 0.5 && loser_loss > 0.5 {
                OutcomeClass::Exhaustion
            } else {
                OutcomeClass::MarginalVictory
            }
        }
        DogfightEnding::Draw => OutcomeClass::Draw,
    }
}

/// The mode this module implements, for dispatch and metadata.
pub const MODE: Mode = Mode::Dogfight;

#[cfg(test)]
mod tests {
    use super::*;
    use drumfire_model::{AircraftCount, Morale, Rating, Seed};

    fn force(name: &str, aircraft: u8, skill: u8) -> AirForce {
        AirForce {
            name: name.to_string(),
            aircraft: AircraftCount::new(aircraft).unwrap(),
            pilot_skill: Rating::new(skill).unwrap(),
            aircraft_quality: Rating::new(6).unwrap(),
            weapons: Rating::new(6).unwrap(),
            radar: Rating::new(5).unwrap(),
            support: Rating::new(5).unwrap(),
            cohesion: Morale::new(70).unwrap(),
        }
    }

    fn input() -> DogfightInput {
        DogfightInput {
            mode: Mode::Dogfight,
            attacker: force("Red Flight", 6, 7),
            defender: force("Blue Flight", 8, 6),
            config: Default::default(),
            seed: Seed::from_u64(11),
        }
    }

    #[test]
    fn dogfight_is_deterministic() {
        let input = input();
        assert_eq!(
            simulate_dogfight(&input).unwrap(),
            simulate_dogfight(&input).unwrap()
        );
    }

    #[test]
    fn dogfight_replays_exactly() {
        let log = simulate_dogfight(&input()).unwrap();
        assert_eq!(replay_dogfight(&log).unwrap(), log);
    }

    #[test]
    fn committed_dogfight_log_replays() {
        let log: DogfightLog =
            serde_json::from_str(include_str!("../../../fixtures/dogfight.log.json"))
                .expect("committed dogfight log must be valid");
        assert_eq!(log.input.mode, Mode::Dogfight);
        assert_eq!(replay_dogfight(&log).unwrap(), log);
    }

    #[test]
    fn aircraft_never_increase_or_go_negative() {
        let mut input = input();
        input.config.max_passes = 20;
        let log = simulate_dogfight(&input).unwrap();
        for pass in &log.passes {
            assert!(pass.attacker_aircraft_after <= 6);
            assert!(pass.defender_aircraft_after <= 8);
        }
    }

    #[test]
    fn classification_has_a_label() {
        let log = simulate_dogfight(&input()).unwrap();
        assert!(!classify_dogfight(&log).label().is_empty());
    }

    #[test]
    fn wrong_mode_is_rejected() {
        let mut input = input();
        input.mode = Mode::Battle;
        assert!(matches!(
            simulate_dogfight(&input),
            Err(DogfightError::Validation(ValidationError::UnsupportedMode(
                Mode::Battle
            )))
        ));
    }
}
