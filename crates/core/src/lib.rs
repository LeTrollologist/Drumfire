//! Deterministic battle resolution for Drumfire.
//!
//! `core` is a pure library: it performs no I/O and no network access. Given a
//! [`BattleInput`] it returns a [`BattleLog`] that can be serialized, replayed,
//! and audited. All randomness comes from a seeded ChaCha20 stream.

use drumfire_model::{BattleConfig, BattleInput, Mode, TeamInput, ValidationError};
use rand::Rng;
use rand_chacha::rand_core::SeedableRng;
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub mod dogfight;
pub mod naval;

/// Weight of the morale term in the strength score.
pub const W_MORALE: f64 = 0.025;
/// Weight of the technology term.
pub const W_TECH: f64 = 0.30;
/// Weight of the training term.
pub const W_TRAINING: f64 = 0.25;
/// Weight of the combat-experience term.
pub const W_EXPERIENCE: f64 = 0.20;
/// Weight of the logistics term.
pub const W_LOGISTICS: f64 = 0.20;
/// Weight of the intel term.
pub const W_INTEL: f64 = 0.15;

/// Coefficients and volatility for the ground family. Battle is the original
/// v1 profile; Skirmish is a smaller, more morale- and training-sensitive fight.
#[derive(Debug, Clone, Copy)]
struct GroundProfile {
    force_weight: f64,
    morale_weight: f64,
    tech_weight: f64,
    training_weight: f64,
    experience_weight: f64,
    logistics_weight: f64,
    intel_weight: f64,
    casualty_multiplier: f64,
    morale_swing_multiplier: f64,
}

const BATTLE_PROFILE: GroundProfile = GroundProfile {
    force_weight: 1.0,
    morale_weight: W_MORALE,
    tech_weight: W_TECH,
    training_weight: W_TRAINING,
    experience_weight: W_EXPERIENCE,
    logistics_weight: W_LOGISTICS,
    intel_weight: W_INTEL,
    casualty_multiplier: 1.0,
    morale_swing_multiplier: 1.0,
};

const SKIRMISH_PROFILE: GroundProfile = GroundProfile {
    force_weight: 0.85,
    morale_weight: 0.035,
    tech_weight: 0.25,
    training_weight: 0.35,
    experience_weight: 0.30,
    logistics_weight: 0.10,
    intel_weight: 0.15,
    casualty_multiplier: 1.25,
    morale_swing_multiplier: 1.30,
};

fn ground_profile(mode: Mode) -> Option<GroundProfile> {
    match mode {
        Mode::Skirmish => Some(SKIRMISH_PROFILE),
        Mode::Battle => Some(BATTLE_PROFILE),
        Mode::Frontlines | Mode::Dogfight | Mode::Naval => None,
    }
}

/// Which side of the battle is being described.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Attacker,
    Defender,
}

/// The final winner, including the draw case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Winner {
    Attacker,
    Defender,
    Draw,
}

/// Why the battle ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ending {
    /// One side reached zero troops.
    Annihilation,
    /// One or both sides fell to the morale rout threshold.
    Rout,
    /// The configured round cap was reached.
    MaxRounds,
    /// The battle ended without a winner.
    Draw,
}

/// A component-by-component view of one side's strength score.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StrengthBreakdown {
    pub forces: f64,
    pub morale: f64,
    pub tech: f64,
    pub training: f64,
    pub experience: f64,
    pub logistics: f64,
    pub intel: f64,
    pub home: f64,
    pub total: f64,
}

/// One resolved interval of the battle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoundLog {
    pub round: u32,
    pub attacker_strength: f64,
    pub defender_strength: f64,
    pub attacker_win_probability: f64,
    pub roll: f64,
    pub round_winner: Side,
    pub attacker_casualties: u32,
    pub defender_casualties: u32,
    pub attacker_forces_after: u32,
    pub defender_forces_after: u32,
    pub attacker_morale_after: u8,
    pub defender_morale_after: u8,
}

/// Final force counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForcePair {
    pub attacker: u32,
    pub defender: u32,
}

/// Final morale values.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoralePair {
    pub attacker: u8,
    pub defender: u8,
}

/// The outcome fields of a battle, separate from the round-by-round trace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BattleResult {
    pub winner: Winner,
    pub ending: Ending,
    pub final_forces: ForcePair,
    pub final_morale: MoralePair,
}

/// A derived, human-facing classification of the result. This is intentionally
/// not stored in the log: older logs can still be classified, and the rules for
/// the label can evolve without breaking replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutcomeClass {
    DecisiveVictory,
    PyrrhicVictory,
    MarginalVictory,
    Stalemate,
    Rout,
    Exhaustion,
    Draw,
}

impl OutcomeClass {
    pub fn label(self) -> &'static str {
        match self {
            OutcomeClass::DecisiveVictory => "Decisive victory",
            OutcomeClass::PyrrhicVictory => "Pyrrhic victory",
            OutcomeClass::MarginalVictory => "Marginal victory",
            OutcomeClass::Stalemate => "Stalemate",
            OutcomeClass::Rout => "Rout",
            OutcomeClass::Exhaustion => "Exhaustion",
            OutcomeClass::Draw => "Draw",
        }
    }
}

/// Classify a battle by margin and cost, not just by winner.
pub fn classify(log: &BattleLog) -> OutcomeClass {
    let initial_a = log.input.attacker.forces.get() as f64;
    let initial_d = log.input.defender.forces.get() as f64;
    let final_a = log.result.final_forces.attacker as f64;
    let final_d = log.result.final_forces.defender as f64;

    let (winner_loss, loser_loss) = match log.result.winner {
        Winner::Attacker => (1.0 - final_a / initial_a, 1.0 - final_d / initial_d),
        Winner::Defender => (1.0 - final_d / initial_d, 1.0 - final_a / initial_a),
        Winner::Draw => return OutcomeClass::Draw,
    };

    match log.result.ending {
        Ending::Rout => OutcomeClass::Rout,
        Ending::Annihilation => {
            if winner_loss < 0.5 {
                OutcomeClass::DecisiveVictory
            } else {
                OutcomeClass::PyrrhicVictory
            }
        }
        Ending::MaxRounds => {
            if winner_loss >= 0.8 * loser_loss {
                OutcomeClass::PyrrhicVictory
            } else if winner_loss > 0.5 && loser_loss > 0.5 {
                OutcomeClass::Exhaustion
            } else if loser_loss < 0.35 {
                OutcomeClass::Stalemate
            } else {
                OutcomeClass::MarginalVictory
            }
        }
        Ending::Draw => OutcomeClass::Draw,
    }
}

/// A complete, self-contained battle record.
///
/// The log embeds the input, so a saved log can be replayed without any other
/// file. `input_hash` is the SHA-256 of the canonical JSON encoding of
/// `input`; it lets two parties confirm they are discussing the same battle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BattleLog {
    pub input: BattleInput,
    pub input_hash: String,
    pub rounds: Vec<RoundLog>,
    pub result: BattleResult,
}

/// Errors from simulation.
#[derive(Debug, Error)]
pub enum SimError {
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error("mode {0:?} is not implemented yet")]
    UnsupportedMode(Mode),
    #[error("failed to serialize battle input: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// Errors from replay verification.
#[derive(Debug, Error)]
pub enum ReplayError {
    #[error(transparent)]
    Sim(#[from] SimError),
    #[error("replayed battle does not match the stored log")]
    Mismatch,
}

/// Compute the strength score for one side at a given force and morale level.
/// Unsupported modes fall back to the Battle profile; callers normally validate
/// the mode first.
pub fn strength_breakdown(
    team: &TeamInput,
    forces: u32,
    morale: u8,
    config: &BattleConfig,
    mode: Mode,
) -> StrengthBreakdown {
    let profile = ground_profile(mode).unwrap_or(BATTLE_PROFILE);
    let forces_term = profile.force_weight * (forces as f64 + 1.0).ln();
    let morale_term = profile.morale_weight * (morale as f64 - 50.0);
    let tech_term = profile.tech_weight * (team.tech.get() as f64 - 5.0);
    let training_term = profile.training_weight * (team.training.get() as f64 - 5.0);
    let experience_term = profile.experience_weight * (team.experience.get() as f64 - 5.0);
    let logistics_term = profile.logistics_weight * (team.logistics.get() as f64 - 5.0);
    let intel_term = profile.intel_weight * (team.intel.get() as f64 - 5.0);
    let home_term = if team.home { config.home_bonus } else { 0.0 };

    StrengthBreakdown {
        forces: forces_term,
        morale: morale_term,
        tech: tech_term,
        training: training_term,
        experience: experience_term,
        logistics: logistics_term,
        intel: intel_term,
        home: home_term,
        total: forces_term
            + morale_term
            + tech_term
            + training_term
            + experience_term
            + logistics_term
            + intel_term
            + home_term,
    }
}

fn team_strength(
    team: &TeamInput,
    forces: u32,
    morale: u8,
    config: &BattleConfig,
    mode: Mode,
) -> f64 {
    strength_breakdown(team, forces, morale, config, mode).total
}

/// Logistic win probability for a given strength difference.
///
/// The tiny clamp keeps probabilities away from exactly 0 or 1 so that every
/// interval remains a genuine roll of the dice, even for lopsided matchups.
pub fn win_probability(strength_difference: f64, scale: f64) -> f64 {
    let x = strength_difference / scale;
    let p = if x >= 40.0 {
        1.0
    } else if x <= -40.0 {
        0.0
    } else {
        1.0 / (1.0 + (-x).exp())
    };
    p.clamp(1.0e-6, 1.0 - 1.0e-6)
}

/// The probability that the attacker wins the opening interval, before any
/// casualties or morale changes.
pub fn initial_win_probability(input: &BattleInput) -> Result<f64, ValidationError> {
    input.validate()?;
    let a = team_strength(
        &input.attacker,
        input.attacker.forces.get(),
        input.attacker.morale.get(),
        &input.config,
        input.mode,
    );
    let d = team_strength(
        &input.defender,
        input.defender.forces.get(),
        input.defender.morale.get(),
        &input.config,
        input.mode,
    );
    Ok(win_probability(a - d, input.config.scale))
}

fn casualties(forces: u32, fraction: f64, at_least_one: bool) -> u32 {
    if forces == 0 || fraction <= 0.0 {
        return 0;
    }
    let raw = (forces as f64 * fraction).round() as u32;
    let raw = if at_least_one { raw.max(1) } else { raw };
    raw.min(forces)
}

fn clamp_morale(value: f64) -> u8 {
    value.round().clamp(0.0, 100.0) as u8
}

fn compare_strength(a: f64, d: f64, a_forces: u32, d_forces: u32) -> Winner {
    let diff = a - d;
    if diff.abs() > 1.0e-9 {
        if diff > 0.0 {
            Winner::Attacker
        } else {
            Winner::Defender
        }
    } else if a_forces > d_forces {
        Winner::Attacker
    } else if d_forces > a_forces {
        Winner::Defender
    } else {
        Winner::Draw
    }
}

fn compare_morale(a_morale: u8, d_morale: u8, a_forces: u32, d_forces: u32) -> Winner {
    if a_morale > d_morale {
        Winner::Attacker
    } else if d_morale > a_morale {
        Winner::Defender
    } else if a_forces > d_forces {
        Winner::Attacker
    } else if d_forces > a_forces {
        Winner::Defender
    } else {
        Winner::Draw
    }
}

/// Resolve a battle.
pub fn simulate(input: &BattleInput) -> Result<BattleLog, SimError> {
    input.validate()?;
    let profile = ground_profile(input.mode).ok_or(SimError::UnsupportedMode(input.mode))?;
    simulate_ground(input, profile)
}

fn simulate_ground(input: &BattleInput, profile: GroundProfile) -> Result<BattleLog, SimError> {
    let input_hash = hex::encode(Sha256::digest(serde_json::to_vec(input)?));
    let config = &input.config;
    let attacker = &input.attacker;
    let defender = &input.defender;

    let mut rng = ChaCha20Rng::from_seed(*input.seed.as_bytes());
    let mut a_forces = attacker.forces.get();
    let mut d_forces = defender.forces.get();
    let mut a_morale = attacker.morale.get();
    let mut d_morale = defender.morale.get();

    let mut rounds = Vec::with_capacity(config.max_rounds as usize);
    let mut winner = Winner::Draw;
    let mut ending = Ending::MaxRounds;

    for round in 1..=config.max_rounds {
        let a_before = a_forces;
        let d_before = d_forces;

        let a_strength = team_strength(attacker, a_before, a_morale, config, input.mode);
        let d_strength = team_strength(defender, d_before, d_morale, config, input.mode);
        let p = win_probability(a_strength - d_strength, config.scale);

        // Fixed draw order is part of the replay contract: probability roll,
        // casualty-intensity roll, winner-casualty roll. Do not reorder these.
        let roll: f64 = rng.gen();
        let intensity_roll: f64 = rng.gen();
        let winner_roll: f64 = rng.gen();

        let round_winner = if roll < p {
            Side::Attacker
        } else {
            Side::Defender
        };

        // The stronger the pre-round edge, the more decisive the casualties.
        let edge = (p - 0.5).abs() * 2.0;
        let base =
            config.casualty_rate * (0.65 + 0.70 * intensity_roll) * profile.casualty_multiplier;
        let loser_fraction = (base * (0.80 + 0.60 * edge)).clamp(0.0, 0.45);
        let winner_fraction = (base * 0.35 * (0.60 + 0.80 * winner_roll)).clamp(0.0, 0.25);

        let (a_loss, d_loss) = match round_winner {
            Side::Attacker => (
                casualties(a_before, winner_fraction, false),
                casualties(d_before, loser_fraction, true),
            ),
            Side::Defender => (
                casualties(a_before, loser_fraction, true),
                casualties(d_before, winner_fraction, false),
            ),
        };

        a_forces = a_forces.saturating_sub(a_loss);
        d_forces = d_forces.saturating_sub(d_loss);

        let a_loss_fraction = a_loss as f64 / a_before as f64;
        let d_loss_fraction = d_loss as f64 / d_before as f64;

        let winner_gain = (2.0 + 3.0 * (1.0 - edge)) * profile.morale_swing_multiplier;
        let (a_delta, d_delta) = match round_winner {
            Side::Attacker => (
                winner_gain,
                -(3.0 + 40.0 * d_loss_fraction) * profile.morale_swing_multiplier,
            ),
            Side::Defender => (
                -(3.0 + 40.0 * a_loss_fraction) * profile.morale_swing_multiplier,
                winner_gain,
            ),
        };

        a_morale = clamp_morale(a_morale as f64 + a_delta);
        d_morale = clamp_morale(d_morale as f64 + d_delta);

        rounds.push(RoundLog {
            round,
            attacker_strength: a_strength,
            defender_strength: d_strength,
            attacker_win_probability: p,
            roll,
            round_winner,
            attacker_casualties: a_loss,
            defender_casualties: d_loss,
            attacker_forces_after: a_forces,
            defender_forces_after: d_forces,
            attacker_morale_after: a_morale,
            defender_morale_after: d_morale,
        });

        if a_forces == 0 || d_forces == 0 {
            if a_forces == 0 && d_forces == 0 {
                winner = Winner::Draw;
                ending = Ending::Draw;
            } else if d_forces == 0 {
                winner = Winner::Attacker;
                ending = Ending::Annihilation;
            } else {
                winner = Winner::Defender;
                ending = Ending::Annihilation;
            }
            break;
        }

        let a_rout = a_morale <= config.morale_rout_threshold;
        let d_rout = d_morale <= config.morale_rout_threshold;
        if a_rout || d_rout {
            winner = match (a_rout, d_rout) {
                (true, true) => compare_morale(a_morale, d_morale, a_forces, d_forces),
                (true, false) => Winner::Defender,
                (false, true) => Winner::Attacker,
                (false, false) => unreachable!("rout condition checked above"),
            };
            ending = if winner == Winner::Draw {
                Ending::Draw
            } else {
                Ending::Rout
            };
            break;
        }
    }

    if ending == Ending::MaxRounds {
        let a_strength = team_strength(attacker, a_forces, a_morale, config, input.mode);
        let d_strength = team_strength(defender, d_forces, d_morale, config, input.mode);
        winner = compare_strength(a_strength, d_strength, a_forces, d_forces);
        if winner == Winner::Draw {
            ending = Ending::Draw;
        }
    }

    Ok(BattleLog {
        input: input.clone(),
        input_hash,
        rounds,
        result: BattleResult {
            winner,
            ending,
            final_forces: ForcePair {
                attacker: a_forces,
                defender: d_forces,
            },
            final_morale: MoralePair {
                attacker: a_morale,
                defender: d_morale,
            },
        },
    })
}

/// Re-simulate a stored log and verify that it matches exactly.
pub fn replay(log: &BattleLog) -> Result<BattleLog, ReplayError> {
    let replayed = simulate(&log.input)?;
    if replayed != *log {
        return Err(ReplayError::Mismatch);
    }
    Ok(replayed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use drumfire_model::{ForceCount, Morale, Rating, Seed};
    use rand::Rng;

    fn example_input() -> BattleInput {
        serde_json::from_str(include_str!("../../../fixtures/example.json"))
            .expect("fixture must be valid")
    }

    fn team(name: &str, forces: u32, morale: u8, rating: u8) -> TeamInput {
        TeamInput {
            name: name.to_string(),
            forces: ForceCount::new(forces).unwrap(),
            morale: Morale::new(morale).unwrap(),
            tech: Rating::new(rating).unwrap(),
            training: Rating::new(rating).unwrap(),
            experience: Rating::new(rating).unwrap(),
            logistics: Rating::new(rating).unwrap(),
            intel: Rating::new(rating).unwrap(),
            home: false,
        }
    }

    #[test]
    fn strength_breakdown_sums_to_total() {
        let input = example_input();
        let b = strength_breakdown(
            &input.attacker,
            input.attacker.forces.get(),
            input.attacker.morale.get(),
            &input.config,
            Mode::Battle,
        );
        let sum = b.forces
            + b.morale
            + b.tech
            + b.training
            + b.experience
            + b.logistics
            + b.intel
            + b.home;
        assert!((sum - b.total).abs() < 1.0e-12);
    }

    #[test]
    fn simulation_is_deterministic() {
        let input = example_input();
        let a = simulate(&input).unwrap();
        let b = simulate(&input).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn different_seeds_change_the_rolls() {
        let input = example_input();
        let mut other = input.clone();
        other.seed = Seed::from_u64(0x1234_5678_9abc_def0);
        let a = simulate(&input).unwrap();
        let b = simulate(&other).unwrap();
        assert_ne!(a.rounds[0].roll, b.rounds[0].roll);
    }

    #[test]
    fn replay_round_trips() {
        let input = example_input();
        let log = simulate(&input).unwrap();
        assert_eq!(replay(&log).unwrap(), log);
    }

    #[test]
    fn committed_log_fixture_replays() {
        // This catches regressions in float serialization as well as simulation
        // changes: the committed log must remain reproducible byte-for-byte.
        let log: BattleLog =
            serde_json::from_str(include_str!("../../../fixtures/example.log.json"))
                .expect("committed log fixture must be valid");
        assert_eq!(replay(&log).unwrap(), log);
    }

    #[test]
    fn committed_skirmish_log_replays() {
        let log: BattleLog =
            serde_json::from_str(include_str!("../../../fixtures/skirmish.log.json"))
                .expect("committed skirmish log must be valid");
        assert_eq!(log.input.mode, Mode::Skirmish);
        assert_eq!(replay(&log).unwrap(), log);
    }

    #[test]
    fn replay_detects_tampering() {
        let input = example_input();
        let mut log = simulate(&input).unwrap();
        log.result.final_forces.attacker += 1;
        assert!(matches!(replay(&log), Err(ReplayError::Mismatch)));
    }

    #[test]
    fn more_forces_increases_opening_probability() {
        let mut weak = example_input();
        weak.attacker.forces = ForceCount::new(1_000).unwrap();
        let mut strong = weak.clone();
        strong.attacker.forces = ForceCount::new(2_000).unwrap();
        assert!(
            initial_win_probability(&strong).unwrap() > initial_win_probability(&weak).unwrap()
        );
    }

    #[test]
    fn zero_casualty_rate_produces_no_losses() {
        let mut input = example_input();
        input.config.casualty_rate = 0.0;
        input.config.max_rounds = 5;
        let log = simulate(&input).unwrap();
        assert!(log
            .rounds
            .iter()
            .all(|r| r.attacker_casualties == 0 && r.defender_casualties == 0));
        assert_eq!(
            log.result.final_forces.attacker,
            input.attacker.forces.get()
        );
        assert_eq!(
            log.result.final_forces.defender,
            input.defender.forces.get()
        );
    }

    #[test]
    fn annihilation_ends_the_battle() {
        let mut input = example_input();
        input.attacker.forces = ForceCount::new(1_000_000).unwrap();
        input.defender.forces = ForceCount::new(1).unwrap();
        input.config.casualty_rate = 0.5;
        input.config.max_rounds = 20;
        let log = simulate(&input).unwrap();
        assert_eq!(log.result.ending, Ending::Annihilation);
        assert_ne!(log.result.winner, Winner::Draw);
    }

    #[test]
    fn low_morale_can_rout() {
        let mut input = example_input();
        input.attacker.forces = ForceCount::new(1_000_000).unwrap();
        input.defender.morale = Morale::new(0).unwrap();
        input.config.morale_rout_threshold = 20;
        input.config.max_rounds = 20;
        let log = simulate(&input).unwrap();
        assert_eq!(log.result.ending, Ending::Rout);
        assert_eq!(log.result.winner, Winner::Attacker);
    }

    #[test]
    fn round_cap_is_respected() {
        let input = BattleInput {
            mode: Mode::Battle,
            attacker: team("A", 5000, 70, 5),
            defender: team("B", 5000, 70, 5),
            config: BattleConfig {
                max_rounds: 3,
                casualty_rate: 0.0,
                morale_rout_threshold: 0,
                ..BattleConfig::default()
            },
            seed: Seed::from_u64(7),
        };
        let log = simulate(&input).unwrap();
        assert_eq!(log.rounds.len(), 3);
        assert_eq!(log.result.ending, Ending::MaxRounds);
    }

    #[test]
    fn simulation_invariants_across_generated_inputs() {
        let mut rng = ChaCha20Rng::from_seed([0x42; 32]);

        for _ in 0..250 {
            let a_forces = rng.gen_range(1u32..200_000);
            let d_forces = rng.gen_range(1u32..200_000);
            let a_morale = rng.gen_range(0u8..=100);
            let d_morale = rng.gen_range(0u8..=100);
            let a_rating = rng.gen_range(1u8..=10);
            let d_rating = rng.gen_range(1u8..=10);
            let mut seed = [0u8; 32];
            rng.fill(&mut seed);

            let input = BattleInput {
                mode: Mode::Battle,
                attacker: team("A", a_forces, a_morale, a_rating),
                defender: team("B", d_forces, d_morale, d_rating),
                config: BattleConfig::default(),
                seed: Seed::from_bytes(seed),
            };
            let log = simulate(&input).unwrap();

            assert!(!log.rounds.is_empty());
            assert!(log.rounds.len() <= input.config.max_rounds as usize);
            assert!(log.result.final_forces.attacker <= a_forces);
            assert!(log.result.final_forces.defender <= d_forces);
            assert!(log.result.final_morale.attacker <= 100);
            assert!(log.result.final_morale.defender <= 100);

            for r in &log.rounds {
                assert!(r.attacker_win_probability > 0.0 && r.attacker_win_probability < 1.0);
                assert!((0.0..1.0).contains(&r.roll));
                assert!(r.attacker_forces_after <= a_forces);
                assert!(r.defender_forces_after <= d_forces);
            }

            let replayed = replay(&log).unwrap();
            assert_eq!(replayed, log);
        }
    }

    #[test]
    fn mode_defaults_to_battle_for_old_inputs() {
        let input = example_input();
        assert_eq!(input.mode, Mode::Battle);
    }

    #[test]
    fn skirmish_uses_a_distinct_profile_and_replays() {
        let mut input = example_input();
        input.mode = Mode::Skirmish;

        let skirmish = simulate(&input).unwrap();
        let battle = simulate(&example_input()).unwrap();

        assert_eq!(skirmish.input.mode, Mode::Skirmish);
        assert_ne!(
            skirmish.rounds[0].attacker_strength,
            battle.rounds[0].attacker_strength
        );
        assert_eq!(replay(&skirmish).unwrap(), skirmish);
    }

    #[test]
    fn unsupported_mode_is_rejected() {
        let mut input = example_input();
        input.mode = Mode::Dogfight;
        assert!(matches!(
            simulate(&input),
            Err(SimError::Validation(ValidationError::UnsupportedMode(
                Mode::Dogfight
            )))
        ));
    }

    #[test]
    fn outcome_labels_are_stable() {
        assert_eq!(OutcomeClass::DecisiveVictory.label(), "Decisive victory");
        assert_eq!(OutcomeClass::PyrrhicVictory.label(), "Pyrrhic victory");
        assert_eq!(OutcomeClass::Rout.label(), "Rout");
    }
}
