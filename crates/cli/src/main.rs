//! `drumfire` command-line interface.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use drumfire_core::{
    classify, initial_win_probability, replay, simulate, strength_breakdown, BattleLog, Ending,
    Side, Winner,
};
use drumfire_model::{BattleInput, Mode, Seed};
use serde::de::DeserializeOwned;

#[derive(Debug, Parser)]
#[command(
    name = "drumfire",
    version,
    about = "Deterministic battle resolution for NationStates roleplay",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Resolve a battle from a JSON input file.
    Run {
        /// Path to the battle input JSON.
        input: PathBuf,
        /// Override the input seed with 64 hex characters, or `random`.
        #[arg(long)]
        seed: Option<String>,
        /// Override the mode: skirmish or battle.
        #[arg(long)]
        mode: Option<Mode>,
        /// Print the full battle log as JSON instead of a narrative.
        #[arg(long)]
        json: bool,
        /// Also write the full battle log to this path.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Verify a saved battle log by re-simulating it.
    Replay {
        /// Path to a battle log JSON produced by `run --out` or `run --json`.
        log: PathBuf,
        /// Print the verified log as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Show the strength breakdown and opening win probability for an input.
    Explain {
        /// Path to the battle input JSON.
        input: PathBuf,
        /// Override the mode: skirmish or battle.
        #[arg(long)]
        mode: Option<Mode>,
    },
    /// Fetch one nation's public data (read-only, no credentials).
    Nation {
        /// Nation name, e.g. "Testlandia" or "The Grendels".
        name: String,
        /// Print the result as JSON.
        #[arg(long)]
        json: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Run {
            input,
            seed,
            mode,
            json,
            out,
        } => {
            let mut input: BattleInput = read_json(&input)?;
            if let Some(mode) = mode {
                input.mode = mode;
            }
            if let Some(seed) = seed {
                input.seed = parse_seed(&seed)?;
            }
            let log = simulate(&input)?;

            if let Some(path) = out {
                write_json(&path, &log)?;
            }

            if json {
                println!("{}", serde_json::to_string_pretty(&log)?);
            } else {
                print_narrative(&log);
            }
        }
        Command::Replay { log, json } => {
            let log: BattleLog = read_json(&log)?;
            let verified = replay(&log)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&verified)?);
            } else {
                println!("Replay verified.");
                println!();
                print_narrative(&verified);
            }
        }
        Command::Explain { input, mode } => {
            let mut input: BattleInput = read_json(&input)?;
            if let Some(mode) = mode {
                input.mode = mode;
            }
            explain(&input)?;
        }
        Command::Nation { name, json } => {
            let nation = drumfire_ns::fetch_nation(&name)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&nation)?);
            } else {
                print_nation(&nation);
            }
        }
    }
    Ok(())
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("invalid JSON in {}", path.display()))
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    let text = serde_json::to_string_pretty(value)?;
    std::fs::write(path, text).with_context(|| format!("failed to write {}", path.display()))
}

fn parse_seed(value: &str) -> Result<Seed> {
    if value.eq_ignore_ascii_case("random") {
        let mut bytes = [0u8; 32];
        getrandom::getrandom(&mut bytes).context("failed to obtain a random seed")?;
        Ok(Seed::from_bytes(bytes))
    } else {
        Seed::from_hex(value).map_err(Into::into)
    }
}

fn side_name(side: Side) -> &'static str {
    match side {
        Side::Attacker => "attacker",
        Side::Defender => "defender",
    }
}

fn ending_name(ending: Ending, mode: Mode) -> String {
    match ending {
        Ending::Annihilation => "annihilation".to_string(),
        Ending::Rout => "rout".to_string(),
        Ending::MaxRounds => format!("{} cap", mode.phase_label().to_lowercase()),
        Ending::Draw => "draw".to_string(),
    }
}

fn winner_name(winner: Winner, input: &BattleInput) -> &str {
    match winner {
        Winner::Attacker => &input.attacker.name,
        Winner::Defender => &input.defender.name,
        Winner::Draw => "Draw",
    }
}

fn print_narrative(log: &BattleLog) {
    let a = &log.input.attacker.name;
    let d = &log.input.defender.name;
    let phase = log.input.mode.phase_label();
    let phases = log.input.mode.phase_label_plural();
    let outcome = classify(log);

    println!(
        "{}: {a} (attacker) vs {d} (defender)",
        log.input.mode.label()
    );
    println!("Seed: {}", log.input.seed.to_hex());
    println!("Input hash: {}", log.input_hash);
    println!();

    for r in &log.rounds {
        println!(
            "{phase} {:>2}: {:<8} p={:5.1}% roll={:.3}  losses {:>6}/{:<6}  forces {:>7}/{:<7}  morale {:>3}/{:<3}",
            r.round,
            side_name(r.round_winner),
            r.attacker_win_probability * 100.0,
            r.roll,
            r.attacker_casualties,
            r.defender_casualties,
            r.attacker_forces_after,
            r.defender_forces_after,
            r.attacker_morale_after,
            r.defender_morale_after
        );
    }

    println!();
    match log.result.winner {
        Winner::Draw => println!(
            "Result: draw — {} after {} {}.",
            outcome.label(),
            log.rounds.len(),
            phases.to_lowercase()
        ),
        winner => println!(
            "Result: {} — {} after {} {}.",
            winner_name(winner, &log.input),
            outcome.label(),
            log.rounds.len(),
            phases.to_lowercase()
        ),
    }
    println!(
        "Ending: {}.",
        ending_name(log.result.ending, log.input.mode)
    );
    println!(
        "Final forces: {} / {}",
        log.result.final_forces.attacker, log.result.final_forces.defender
    );
    println!(
        "Final morale: {} / {}",
        log.result.final_morale.attacker, log.result.final_morale.defender
    );
}

fn print_nation(nation: &drumfire_ns::NationSummary) {
    if nation.fullname.is_empty() || nation.fullname == nation.name {
        println!("{}", nation.name);
    } else {
        println!("{}", nation.fullname);
        println!("Short name: {}", nation.name);
    }
    println!("Region: {}", nation.region);
    println!("Population: {}", nation.population);
    println!("Category: {}", nation.category);
    if !nation.motto.is_empty() {
        println!("Motto: \"{}\"", nation.motto);
    }
    if !nation.currency.is_empty() {
        println!("Currency: {}", nation.currency);
    }
    if !nation.flag.is_empty() {
        println!("Flag: {}", nation.flag);
    }
}

fn explain(input: &BattleInput) -> Result<()> {
    input.validate()?;
    println!(
        "{} profile · {} vs {}",
        input.mode.label(),
        input.attacker.name,
        input.defender.name
    );
    println!();

    for (label, team) in [("Attacker", &input.attacker), ("Defender", &input.defender)] {
        let b = strength_breakdown(
            team,
            team.forces.get(),
            team.morale.get(),
            &input.config,
            input.mode,
        );
        println!("{}: {}", label, team.name);
        println!("  forces      {:+.3}", b.forces);
        println!("  morale      {:+.3}", b.morale);
        println!("  tech        {:+.3}", b.tech);
        println!("  training    {:+.3}", b.training);
        println!("  experience  {:+.3}", b.experience);
        println!("  logistics   {:+.3}", b.logistics);
        println!("  intel       {:+.3}", b.intel);
        println!("  home        {:+.3}", b.home);
        println!("  total       {:+.3}", b.total);
        println!();
    }

    let p = initial_win_probability(input)?;
    println!(
        "Opening interval win probability for {}: {:.1}%",
        input.attacker.name,
        p * 100.0
    );
    Ok(())
}
