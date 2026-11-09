//! Deterministic naval surface action.
//!
//! Ships are individual hulls. Detection, weapons, air cover, damage control,
//! and readiness decide a sequence of salvos; the result is whole ships sunk or
//! a fleet that breaks off.

use drumfire_model::{Fleet, NavalInput, ValidationError};
use rand::Rng;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{win_probability, OutcomeClass, Side, Winner};

/// Why the naval action ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NavalEnding {
    /// One fleet lost all ships.
    Sunk,
    /// One or both fleets withdrew after readiness collapsed.
    Withdrawal,
    /// The configured salvo cap was reached.
    MaxSalvos,
    /// No winner.
    Draw,
}

/// One resolved salvo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SalvoLog {
    pub salvo: u32,
    pub attacker_strength: f64,
    pub defender_strength: f64,
    pub attacker_win_probability: f64,
    pub roll: f64,
    pub salvo_winner: Side,
    /// Ships sunk by the attacker.
    pub attacker_sunk: u8,
    /// Ships sunk by the defender.
    pub defender_sunk: u8,
    pub attacker_ships_after: u8,
    pub defender_ships_after: u8,
    pub attacker_readiness_after: u8,
    pub defender_readiness_after: u8,
}

/// Final ship counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShipPair {
    pub attacker: u8,
    pub defender: u8,
}

/// Final readiness values.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadinessPair {
    pub attacker: u8,
    pub defender: u8,
}

/// The outcome fields of a naval action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NavalResult {
    pub winner: Winner,
    pub ending: NavalEnding,
    pub final_ships: ShipPair,
    pub final_readiness: ReadinessPair,
}

/// A complete, self-contained naval log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NavalLog {
    pub input: NavalInput,
    pub input_hash: String,
    pub salvos: Vec<SalvoLog>,
    pub result: NavalResult,
}

/// Errors from naval simulation.
#[derive(Debug, Error)]
pub enum NavalError {
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error("failed to serialize naval input: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// Errors from naval replay verification.
#[derive(Debug, Error)]
pub enum NavalReplayError {
    #[error(transparent)]
    Sim(#[from] NavalError),
    #[error("replayed naval action does not match the stored log")]
    Mismatch,
}

fn strength(fleet: &Fleet, ships: u8, readiness: u8) -> f64 {
    (ships as f64 + 1.0).ln()
        + 0.30 * (fleet.hull_quality.get() as f64 - 5.0)
        + 0.30 * (fleet.sensors.get() as f64 - 5.0)
        + 0.25 * (fleet.weapons.get() as f64 - 5.0)
        + 0.15 * (fleet.air_cover.get() as f64 - 5.0)
        + 0.15 * (fleet.damage_control.get() as f64 - 5.0)
        + 0.02 * (readiness as f64 - 50.0)
}

fn clamp_readiness(value: f64) -> u8 {
    value.round().clamp(0.0, 100.0) as u8
}

fn compare_fleet(a_ships: u8, d_ships: u8, a_readiness: u8, d_readiness: u8) -> Winner {
    if a_ships > d_ships {
        Winner::Attacker
    } else if d_ships > a_ships {
        Winner::Defender
    } else if a_readiness > d_readiness {
        Winner::Attacker
    } else if d_readiness > a_readiness {
        Winner::Defender
    } else {
        Winner::Draw
    }
}

/// Resolve a naval action.
pub fn simulate_naval(input: &NavalInput) -> Result<NavalLog, NavalError> {
    input.validate()?;

    let input_hash = hex::encode(Sha256::digest(serde_json::to_vec(input)?));
    let config = &input.config;
    let mut rng = ChaCha20Rng::from_seed(*input.seed.as_bytes());

    let mut a_ships = input.attacker.ships.get();
    let mut d_ships = input.defender.ships.get();
    let mut a_readiness = input.attacker.readiness.get();
    let mut d_readiness = input.defender.readiness.get();

    let mut salvos = Vec::with_capacity(config.max_salvos as usize);
    let mut winner = Winner::Draw;
    let mut ending = NavalEnding::MaxSalvos;

    for salvo in 1..=config.max_salvos {
        let a_before = a_ships;
        let d_before = d_ships;
        let a_strength = strength(&input.attacker, a_before, a_readiness);
        let d_strength = strength(&input.defender, d_before, d_readiness);
        let p = win_probability(a_strength - d_strength, config.scale);

        let roll: f64 = rng.gen();
        let intensity_roll: f64 = rng.gen();
        let winner_roll: f64 = rng.gen();

        let salvo_winner = if roll < p {
            Side::Attacker
        } else {
            Side::Defender
        };
        let edge = (p - 0.5).abs() * 2.0;
        let damage = config.hit_rate * (0.70 + 0.60 * intensity_roll) * (0.80 + 0.40 * edge);

        let (a_sunk, d_sunk) = match salvo_winner {
            Side::Attacker => {
                let d_sunk = if damage > 0.0 {
                    ((d_before as f64 * damage).round() as u8)
                        .max(1)
                        .min(d_before)
                } else {
                    0
                };
                let a_sunk =
                    ((a_before as f64 * damage * 0.20 * winner_roll).round() as u8).min(a_before);
                (a_sunk, d_sunk)
            }
            Side::Defender => {
                let a_sunk = if damage > 0.0 {
                    ((a_before as f64 * damage).round() as u8)
                        .max(1)
                        .min(a_before)
                } else {
                    0
                };
                let d_sunk =
                    ((d_before as f64 * damage * 0.20 * winner_roll).round() as u8).min(d_before);
                (a_sunk, d_sunk)
            }
        };

        a_ships = a_ships.saturating_sub(a_sunk);
        d_ships = d_ships.saturating_sub(d_sunk);

        let a_loss_fraction = a_sunk as f64 / a_before as f64;
        let d_loss_fraction = d_sunk as f64 / d_before as f64;
        let winner_gain = 2.0 + 2.0 * (1.0 - edge);
        let (a_delta, d_delta) = match salvo_winner {
            Side::Attacker => (winner_gain, -(5.0 + 20.0 * d_loss_fraction)),
            Side::Defender => (-(5.0 + 20.0 * a_loss_fraction), winner_gain),
        };
        a_readiness = clamp_readiness(a_readiness as f64 + a_delta);
        d_readiness = clamp_readiness(d_readiness as f64 + d_delta);

        salvos.push(SalvoLog {
            salvo,
            attacker_strength: a_strength,
            defender_strength: d_strength,
            attacker_win_probability: p,
            roll,
            salvo_winner,
            attacker_sunk: d_sunk,
            defender_sunk: a_sunk,
            attacker_ships_after: a_ships,
            defender_ships_after: d_ships,
            attacker_readiness_after: a_readiness,
            defender_readiness_after: d_readiness,
        });

        if a_ships == 0 || d_ships == 0 {
            winner = if a_ships == 0 && d_ships == 0 {
                Winner::Draw
            } else if d_ships == 0 {
                Winner::Attacker
            } else {
                Winner::Defender
            };
            ending = if winner == Winner::Draw {
                NavalEnding::Draw
            } else {
                NavalEnding::Sunk
            };
            break;
        }

        let a_break = a_readiness <= config.readiness_break;
        let d_break = d_readiness <= config.readiness_break;
        if a_break || d_break {
            winner = match (a_break, d_break) {
                (true, true) => compare_fleet(a_ships, d_ships, a_readiness, d_readiness),
                (true, false) => Winner::Defender,
                (false, true) => Winner::Attacker,
                (false, false) => unreachable!("break condition checked above"),
            };
            ending = if winner == Winner::Draw {
                NavalEnding::Draw
            } else {
                NavalEnding::Withdrawal
            };
            break;
        }
    }

    if ending == NavalEnding::MaxSalvos {
        winner = compare_fleet(a_ships, d_ships, a_readiness, d_readiness);
        if winner == Winner::Draw {
            ending = NavalEnding::Draw;
        }
    }

    Ok(NavalLog {
        input: input.clone(),
        input_hash,
        salvos,
        result: NavalResult {
            winner,
            ending,
            final_ships: ShipPair {
                attacker: a_ships,
                defender: d_ships,
            },
            final_readiness: ReadinessPair {
                attacker: a_readiness,
                defender: d_readiness,
            },
        },
    })
}

/// Re-simulate a stored naval log and verify it matches exactly.
pub fn replay_naval(log: &NavalLog) -> Result<NavalLog, NavalReplayError> {
    let replayed = simulate_naval(&log.input)?;
    if replayed != *log {
        return Err(NavalReplayError::Mismatch);
    }
    Ok(replayed)
}

/// Classify a naval action by margin and cost.
pub fn classify_naval(log: &NavalLog) -> OutcomeClass {
    let initial_a = log.input.attacker.ships.get() as f64;
    let initial_d = log.input.defender.ships.get() as f64;
    let final_a = log.result.final_ships.attacker as f64;
    let final_d = log.result.final_ships.defender as f64;

    let (winner_loss, loser_loss) = match log.result.winner {
        Winner::Attacker => (1.0 - final_a / initial_a, 1.0 - final_d / initial_d),
        Winner::Defender => (1.0 - final_d / initial_d, 1.0 - final_a / initial_a),
        Winner::Draw => return OutcomeClass::Draw,
    };

    match log.result.ending {
        NavalEnding::Withdrawal => OutcomeClass::Rout,
        NavalEnding::Sunk => {
            if winner_loss < 0.5 {
                OutcomeClass::DecisiveVictory
            } else {
                OutcomeClass::PyrrhicVictory
            }
        }
        NavalEnding::MaxSalvos => {
            if winner_loss >= 0.8 * loser_loss {
                OutcomeClass::PyrrhicVictory
            } else if winner_loss > 0.5 && loser_loss > 0.5 {
                OutcomeClass::Exhaustion
            } else {
                OutcomeClass::MarginalVictory
            }
        }
        NavalEnding::Draw => OutcomeClass::Draw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use drumfire_model::{Morale, Rating, Seed, ShipCount};

    fn fleet(name: &str, ships: u8) -> Fleet {
        Fleet {
            name: name.to_string(),
            ships: ShipCount::new(ships).unwrap(),
            hull_quality: Rating::new(6).unwrap(),
            sensors: Rating::new(6).unwrap(),
            weapons: Rating::new(6).unwrap(),
            air_cover: Rating::new(5).unwrap(),
            damage_control: Rating::new(5).unwrap(),
            readiness: Morale::new(70).unwrap(),
        }
    }

    fn input() -> NavalInput {
        NavalInput {
            mode: drumfire_model::Mode::Naval,
            attacker: fleet("Red Fleet", 6),
            defender: fleet("Blue Fleet", 8),
            config: Default::default(),
            seed: Seed::from_u64(17),
        }
    }

    #[test]
    fn naval_action_is_deterministic() {
        let input = input();
        assert_eq!(
            simulate_naval(&input).unwrap(),
            simulate_naval(&input).unwrap()
        );
    }

    #[test]
    fn naval_replays_exactly() {
        let log = simulate_naval(&input()).unwrap();
        assert_eq!(replay_naval(&log).unwrap(), log);
    }

    #[test]
    fn committed_naval_log_replays() {
        let log: NavalLog = serde_json::from_str(include_str!("../../../fixtures/naval.log.json"))
            .expect("committed naval log must be valid");
        assert_eq!(log.input.mode, drumfire_model::Mode::Naval);
        assert_eq!(replay_naval(&log).unwrap(), log);
    }

    #[test]
    fn ships_never_increase_or_go_negative() {
        let mut input = input();
        input.config.max_salvos = 20;
        let log = simulate_naval(&input).unwrap();
        for salvo in &log.salvos {
            assert!(salvo.attacker_ships_after <= 6);
            assert!(salvo.defender_ships_after <= 8);
        }
    }

    #[test]
    fn classification_has_a_label() {
        let log = simulate_naval(&input()).unwrap();
        assert!(!classify_naval(&log).label().is_empty());
    }
}
