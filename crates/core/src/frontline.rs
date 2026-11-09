//! Multi-sector front-line combat.
//!
//! Each day resolves every contested sector. Reserves replace a fraction of the
//! day's losses, fortifications favor the defender, and objective sectors decide
//! whether the front has been broken.

use drumfire_model::{FrontForce, FrontlineInput, ValidationError};
use rand::Rng;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{win_probability, ForcePair, MoralePair, OutcomeClass, Side, Winner};

/// Why the front-line battle ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrontlineEnding {
    /// All objective sectors were captured.
    Breakthrough,
    /// One side ran out of troops and reserves.
    Exhaustion,
    /// One side's morale collapsed and it withdrew.
    Withdrawal,
    /// The day cap was reached.
    MaxDays,
    /// No winner.
    Draw,
}

/// One sector's result on one day.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectorDayLog {
    pub sector: String,
    pub attacker_strength: f64,
    pub defender_strength: f64,
    pub attacker_win_probability: f64,
    pub roll: f64,
    pub sector_winner: Side,
    pub attacker_losses: u32,
    pub defender_losses: u32,
    pub attacker_troops_after: u32,
    pub defender_troops_after: u32,
    pub captured: bool,
}

/// One day across the front.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DayLog {
    pub day: u32,
    pub sectors: Vec<SectorDayLog>,
    pub attacker_morale_after: u8,
    pub defender_morale_after: u8,
    pub attacker_reserves_after: u32,
    pub defender_reserves_after: u32,
}

/// Final state of one sector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectorResult {
    pub name: String,
    pub attacker_troops: u32,
    pub defender_troops: u32,
    pub fortification: u8,
    pub objective: bool,
    pub captured: bool,
}

/// The outcome of a front-line battle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrontlineResult {
    pub winner: Winner,
    pub ending: FrontlineEnding,
    pub sectors: Vec<SectorResult>,
    pub final_reserves: ForcePair,
    pub final_morale: MoralePair,
    pub objectives_captured: u32,
    pub objectives_total: u32,
}

/// A complete, self-contained front-line log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrontlineLog {
    pub input: FrontlineInput,
    pub input_hash: String,
    pub days: Vec<DayLog>,
    pub result: FrontlineResult,
}

/// Errors from front-line simulation.
#[derive(Debug, Error)]
pub enum FrontlineError {
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error("failed to serialize front-line input: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// Errors from front-line replay verification.
#[derive(Debug, Error)]
pub enum FrontlineReplayError {
    #[error(transparent)]
    Sim(#[from] FrontlineError),
    #[error("replayed front-line battle does not match the stored log")]
    Mismatch,
}

struct SectorState {
    name: String,
    attacker: u32,
    defender: u32,
    initial_defender: u32,
    fortification: u8,
    objective: bool,
    captured: bool,
}

fn strength(force: &FrontForce, troops: u32, fortification: u8, defender: bool) -> f64 {
    (troops as f64 + 1.0).ln()
        + 0.30 * (force.tech.get() as f64 - 5.0)
        + 0.25 * (force.training.get() as f64 - 5.0)
        + 0.20 * (force.experience.get() as f64 - 5.0)
        + 0.20 * (force.logistics.get() as f64 - 5.0)
        + 0.15 * (force.intel.get() as f64 - 5.0)
        + if defender {
            0.20 * (fortification as f64 - 5.0)
        } else {
            0.0
        }
}

fn clamp_morale(value: f64) -> u8 {
    value.round().clamp(0.0, 100.0) as u8
}

fn replenish(
    sectors: &mut [SectorState],
    reserves: &mut u32,
    losses: u32,
    rate: f64,
    side: Side,
) -> u32 {
    if losses == 0 || *reserves == 0 {
        return 0;
    }
    let floor = (losses as f64 * rate).round() as u32;
    let amount = floor.min(*reserves);
    if amount == 0 {
        return 0;
    }
    let targets: Vec<usize> = sectors
        .iter()
        .enumerate()
        .filter(|(_, sector)| !sector.captured)
        .map(|(index, _)| index)
        .collect();
    if targets.is_empty() {
        return 0;
    }
    let each = amount / targets.len() as u32;
    let mut remainder = amount % targets.len() as u32;
    for index in targets {
        let mut add = each;
        if remainder > 0 {
            add += 1;
            remainder -= 1;
        }
        match side {
            Side::Attacker => sectors[index].attacker += add,
            Side::Defender => sectors[index].defender += add,
        }
    }
    *reserves -= amount;
    amount
}

/// Resolve a front-line battle.
pub fn simulate_frontline(input: &FrontlineInput) -> Result<FrontlineLog, FrontlineError> {
    input.validate()?;

    let input_hash = hex::encode(Sha256::digest(serde_json::to_vec(input)?));
    let config = &input.config;
    let mut rng = ChaCha20Rng::from_seed(*input.seed.as_bytes());

    let mut sectors: Vec<SectorState> = input
        .sectors
        .iter()
        .map(|sector| SectorState {
            name: sector.name.clone(),
            attacker: sector.attacker_troops.get(),
            defender: sector.defender_troops.get(),
            initial_defender: sector.defender_troops.get(),
            fortification: sector.fortification.get(),
            objective: sector.objective,
            captured: false,
        })
        .collect();

    let mut a_morale = input.attacker.morale.get();
    let mut d_morale = input.defender.morale.get();
    let mut a_reserves = input.attacker.reserves;
    let mut d_reserves = input.defender.reserves;
    let mut days = Vec::with_capacity(config.max_days as usize);
    let mut winner = Winner::Draw;
    let mut ending = FrontlineEnding::MaxDays;

    for day in 1..=config.max_days {
        let mut day_sectors = Vec::new();
        let mut a_losses = 0u32;
        let mut d_losses = 0u32;
        let mut a_morale_delta = 0.0;
        let mut d_morale_delta = 0.0;

        for sector in &mut sectors {
            if sector.captured || sector.attacker == 0 || sector.defender == 0 {
                continue;
            }

            let a_before = sector.attacker;
            let d_before = sector.defender;
            let a_strength = strength(&input.attacker, a_before, sector.fortification, false);
            let d_strength = strength(&input.defender, d_before, sector.fortification, true);
            let p = win_probability(a_strength - d_strength, config.scale);

            let roll: f64 = rng.gen();
            let intensity_roll: f64 = rng.gen();
            let winner_roll: f64 = rng.gen();

            let sector_winner = if roll < p {
                Side::Attacker
            } else {
                Side::Defender
            };
            let edge = (p - 0.5).abs() * 2.0;
            let base = config.casualty_rate * (0.65 + 0.70 * intensity_roll);
            let loser_fraction = (base * (0.80 + 0.60 * edge)).clamp(0.0, 0.45);
            let winner_fraction = (base * 0.35 * (0.60 + 0.80 * winner_roll)).clamp(0.0, 0.25);

            let (a_loss, d_loss) = match sector_winner {
                Side::Attacker => (
                    ((a_before as f64 * winner_fraction).round() as u32).min(a_before),
                    ((d_before as f64 * loser_fraction).round() as u32)
                        .max(1)
                        .min(d_before),
                ),
                Side::Defender => (
                    ((a_before as f64 * loser_fraction).round() as u32)
                        .max(1)
                        .min(a_before),
                    ((d_before as f64 * winner_fraction).round() as u32).min(d_before),
                ),
            };

            sector.attacker = sector.attacker.saturating_sub(a_loss);
            sector.defender = sector.defender.saturating_sub(d_loss);
            a_losses += a_loss;
            d_losses += d_loss;

            let captured = sector.defender == 0
                || sector.defender as f64
                    <= sector.initial_defender as f64 * config.capture_threshold;
            if captured {
                sector.captured = true;
            }

            let (mut a_delta, mut d_delta) = match sector_winner {
                Side::Attacker => (1.0, -2.0),
                Side::Defender => (-2.0, 1.0),
            };
            if captured {
                a_delta += 5.0;
                d_delta -= 5.0;
            }
            a_morale_delta += a_delta;
            d_morale_delta += d_delta;

            day_sectors.push(SectorDayLog {
                sector: sector.name.clone(),
                attacker_strength: a_strength,
                defender_strength: d_strength,
                attacker_win_probability: p,
                roll,
                sector_winner,
                attacker_losses: a_loss,
                defender_losses: d_loss,
                attacker_troops_after: sector.attacker,
                defender_troops_after: sector.defender,
                captured,
            });
        }

        a_morale = clamp_morale(a_morale as f64 + a_morale_delta);
        d_morale = clamp_morale(d_morale as f64 + d_morale_delta);
        replenish(
            &mut sectors,
            &mut a_reserves,
            a_losses,
            config.replenish_rate,
            Side::Attacker,
        );
        replenish(
            &mut sectors,
            &mut d_reserves,
            d_losses,
            config.replenish_rate,
            Side::Defender,
        );

        days.push(DayLog {
            day,
            sectors: day_sectors,
            attacker_morale_after: a_morale,
            defender_morale_after: d_morale,
            attacker_reserves_after: a_reserves,
            defender_reserves_after: d_reserves,
        });

        let objectives_total = sectors.iter().filter(|sector| sector.objective).count();
        let objectives_captured = sectors
            .iter()
            .filter(|sector| sector.objective && sector.captured)
            .count();
        if objectives_captured == objectives_total {
            winner = Winner::Attacker;
            ending = FrontlineEnding::Breakthrough;
            break;
        }

        let a_total = a_reserves + sectors.iter().map(|sector| sector.attacker).sum::<u32>();
        let d_total = d_reserves + sectors.iter().map(|sector| sector.defender).sum::<u32>();
        if d_total == 0 {
            winner = Winner::Attacker;
            ending = FrontlineEnding::Breakthrough;
            break;
        }
        if a_total == 0 {
            winner = Winner::Defender;
            ending = FrontlineEnding::Exhaustion;
            break;
        }

        let a_break = a_morale <= config.morale_rout_threshold;
        let d_break = d_morale <= config.morale_rout_threshold;
        if a_break || d_break {
            winner = match (a_break, d_break) {
                (true, true) => {
                    if objectives_captured * 2 > objectives_total {
                        Winner::Attacker
                    } else if objectives_captured * 2 < objectives_total {
                        Winner::Defender
                    } else {
                        Winner::Draw
                    }
                }
                (true, false) => Winner::Defender,
                (false, true) => Winner::Attacker,
                (false, false) => unreachable!("break condition checked above"),
            };
            ending = if winner == Winner::Draw {
                FrontlineEnding::Draw
            } else if winner == Winner::Attacker {
                FrontlineEnding::Breakthrough
            } else {
                FrontlineEnding::Withdrawal
            };
            break;
        }
    }

    if ending == FrontlineEnding::MaxDays {
        let objectives_total = sectors.iter().filter(|sector| sector.objective).count() as u32;
        let objectives_captured = sectors
            .iter()
            .filter(|sector| sector.objective && sector.captured)
            .count() as u32;
        let a_total = a_reserves + sectors.iter().map(|sector| sector.attacker).sum::<u32>();
        let d_total = d_reserves + sectors.iter().map(|sector| sector.defender).sum::<u32>();
        winner = if objectives_captured * 2 > objectives_total {
            Winner::Attacker
        } else if objectives_captured * 2 < objectives_total {
            Winner::Defender
        } else if a_total > d_total {
            Winner::Attacker
        } else if d_total > a_total {
            Winner::Defender
        } else if a_morale > d_morale {
            Winner::Attacker
        } else if d_morale > a_morale {
            Winner::Defender
        } else {
            Winner::Draw
        };
        if winner == Winner::Draw {
            ending = FrontlineEnding::Draw;
        }
    }

    let objectives_total = sectors.iter().filter(|sector| sector.objective).count() as u32;
    let objectives_captured = sectors
        .iter()
        .filter(|sector| sector.objective && sector.captured)
        .count() as u32;

    Ok(FrontlineLog {
        input: input.clone(),
        input_hash,
        days,
        result: FrontlineResult {
            winner,
            ending,
            sectors: sectors
                .into_iter()
                .map(|sector| SectorResult {
                    name: sector.name,
                    attacker_troops: sector.attacker,
                    defender_troops: sector.defender,
                    fortification: sector.fortification,
                    objective: sector.objective,
                    captured: sector.captured,
                })
                .collect(),
            final_reserves: ForcePair {
                attacker: a_reserves,
                defender: d_reserves,
            },
            final_morale: MoralePair {
                attacker: a_morale,
                defender: d_morale,
            },
            objectives_captured,
            objectives_total,
        },
    })
}

/// Re-simulate a stored front-line log and verify it matches exactly.
pub fn replay_frontline(log: &FrontlineLog) -> Result<FrontlineLog, FrontlineReplayError> {
    let replayed = simulate_frontline(&log.input)?;
    if replayed != *log {
        return Err(FrontlineReplayError::Mismatch);
    }
    Ok(replayed)
}

/// Classify a front-line result by margin and cost.
pub fn classify_frontline(log: &FrontlineLog) -> OutcomeClass {
    let initial_a = (log.input.attacker.reserves
        + log
            .input
            .sectors
            .iter()
            .map(|sector| sector.attacker_troops.get())
            .sum::<u32>()) as f64;
    let initial_d = (log.input.defender.reserves
        + log
            .input
            .sectors
            .iter()
            .map(|sector| sector.defender_troops.get())
            .sum::<u32>()) as f64;
    let final_a = (log.result.final_reserves.attacker
        + log
            .result
            .sectors
            .iter()
            .map(|sector| sector.attacker_troops)
            .sum::<u32>()) as f64;
    let final_d = (log.result.final_reserves.defender
        + log
            .result
            .sectors
            .iter()
            .map(|sector| sector.defender_troops)
            .sum::<u32>()) as f64;

    let winner_loss = match log.result.winner {
        Winner::Attacker => 1.0 - final_a / initial_a.max(1.0),
        Winner::Defender => 1.0 - final_d / initial_d.max(1.0),
        Winner::Draw => return OutcomeClass::Draw,
    };

    match log.result.ending {
        FrontlineEnding::Withdrawal => OutcomeClass::Rout,
        FrontlineEnding::Breakthrough => {
            if winner_loss < 0.5 {
                OutcomeClass::DecisiveVictory
            } else {
                OutcomeClass::PyrrhicVictory
            }
        }
        FrontlineEnding::Exhaustion => OutcomeClass::Exhaustion,
        FrontlineEnding::MaxDays => {
            if winner_loss > 0.5 {
                OutcomeClass::Exhaustion
            } else {
                OutcomeClass::MarginalVictory
            }
        }
        FrontlineEnding::Draw => OutcomeClass::Draw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use drumfire_model::{ForceCount, Morale, Rating, SectorInput, Seed};

    fn force(name: &str, reserves: u32) -> FrontForce {
        FrontForce {
            name: name.to_string(),
            reserves,
            morale: Morale::new(70).unwrap(),
            tech: Rating::new(6).unwrap(),
            training: Rating::new(6).unwrap(),
            experience: Rating::new(6).unwrap(),
            logistics: Rating::new(5).unwrap(),
            intel: Rating::new(5).unwrap(),
        }
    }

    fn input() -> FrontlineInput {
        FrontlineInput {
            mode: drumfire_model::Mode::Frontlines,
            attacker: force("Red Front", 5000),
            defender: force("Blue Front", 5000),
            sectors: vec![
                SectorInput {
                    name: "North".to_string(),
                    attacker_troops: ForceCount::new(4000).unwrap(),
                    defender_troops: ForceCount::new(5000).unwrap(),
                    fortification: Rating::new(6).unwrap(),
                    objective: false,
                },
                SectorInput {
                    name: "Center".to_string(),
                    attacker_troops: ForceCount::new(8000).unwrap(),
                    defender_troops: ForceCount::new(6000).unwrap(),
                    fortification: Rating::new(7).unwrap(),
                    objective: true,
                },
                SectorInput {
                    name: "South".to_string(),
                    attacker_troops: ForceCount::new(5000).unwrap(),
                    defender_troops: ForceCount::new(5000).unwrap(),
                    fortification: Rating::new(5).unwrap(),
                    objective: true,
                },
            ],
            config: Default::default(),
            seed: Seed::from_u64(23),
        }
    }

    #[test]
    fn frontline_is_deterministic() {
        let input = input();
        assert_eq!(
            simulate_frontline(&input).unwrap(),
            simulate_frontline(&input).unwrap()
        );
    }

    #[test]
    fn frontline_replays_exactly() {
        let log = simulate_frontline(&input()).unwrap();
        assert_eq!(replay_frontline(&log).unwrap(), log);
    }

    #[test]
    fn objective_sectors_are_tracked() {
        let log = simulate_frontline(&input()).unwrap();
        assert_eq!(log.result.objectives_total, 2);
        assert!(log.result.objectives_captured <= 2);
    }

    #[test]
    fn committed_frontline_log_replays() {
        let log: FrontlineLog =
            serde_json::from_str(include_str!("../../../fixtures/frontline.log.json"))
                .expect("committed frontline log must be valid");
        assert_eq!(log.input.mode, drumfire_model::Mode::Frontlines);
        assert_eq!(replay_frontline(&log).unwrap(), log);
    }
}
