//! `drumfire` command-line interface.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use drumfire_campaign::{
    replay_campaign, Campaign, CampaignScenario, OperationId, OperationRecord,
};
use drumfire_core::dogfight::{
    classify_dogfight, replay_dogfight, simulate_dogfight, DogfightEnding, DogfightLog,
};
use drumfire_core::frontline::{
    classify_frontline, replay_frontline, simulate_frontline, FrontlineEnding, FrontlineLog,
};
use drumfire_core::naval::{classify_naval, replay_naval, simulate_naval, NavalEnding, NavalLog};
use drumfire_core::{
    classify, initial_win_probability, replay, simulate, strength_breakdown, BattleLog, Ending,
    Side, Winner,
};
use drumfire_model::{BattleInput, DogfightInput, FrontlineInput, Mode, NavalInput, Seed};
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
    /// Run or verify a dogfight.
    Air {
        #[command(subcommand)]
        command: AirCommand,
    },
    /// Run or verify a naval action.
    Naval {
        #[command(subcommand)]
        command: NavalCommand,
    },
    /// Run or verify a front-line battle.
    Frontline {
        #[command(subcommand)]
        command: FrontlineCommand,
    },
    /// Play, demo, or verify the Halcyon Sweep campaign.
    Campaign {
        #[command(subcommand)]
        command: CampaignCommand,
    },
}

#[derive(Debug, Subcommand)]
enum AirCommand {
    /// Resolve a dogfight from a JSON input file.
    Run {
        /// Path to the dogfight input JSON.
        input: PathBuf,
        /// Override the input seed with 64 hex characters, or `random`.
        #[arg(long)]
        seed: Option<String>,
        /// Print the full dogfight log as JSON.
        #[arg(long)]
        json: bool,
        /// Also write the full dogfight log to this path.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Verify a saved dogfight log by re-simulating it.
    Replay {
        /// Path to a dogfight log JSON produced by `air run --out`.
        log: PathBuf,
        /// Print the verified log as JSON.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum NavalCommand {
    /// Resolve a naval action from a JSON input file.
    Run {
        /// Path to the naval input JSON.
        input: PathBuf,
        /// Override the input seed with 64 hex characters, or `random`.
        #[arg(long)]
        seed: Option<String>,
        /// Print the full naval log as JSON.
        #[arg(long)]
        json: bool,
        /// Also write the full naval log to this path.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Verify a saved naval log by re-simulating it.
    Replay {
        /// Path to a naval log JSON produced by `naval run --out`.
        log: PathBuf,
        /// Print the verified log as JSON.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum FrontlineCommand {
    /// Resolve a front-line battle from a JSON input file.
    Run {
        /// Path to the front-line input JSON.
        input: PathBuf,
        /// Override the input seed with 64 hex characters, or `random`.
        #[arg(long)]
        seed: Option<String>,
        /// Print the full front-line log as JSON.
        #[arg(long)]
        json: bool,
        /// Also write the full front-line log to this path.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Verify a saved front-line log by re-simulating it.
    Replay {
        /// Path to a front-line log JSON produced by `frontline run --out`.
        log: PathBuf,
        /// Print the verified log as JSON.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum CampaignCommand {
    /// Play the campaign interactively in the terminal.
    Play {
        /// Campaign seed, or `random`.
        #[arg(long)]
        seed: Option<String>,
        /// Scenario JSON. Uses the built-in Kestrel Coast scenario when omitted.
        #[arg(long)]
        scenario: Option<PathBuf>,
    },
    /// Run the canonical seven-operation demo.
    Demo {
        /// Campaign seed, or `random`.
        #[arg(long)]
        seed: Option<String>,
        /// Scenario JSON. Uses the built-in Kestrel Coast scenario when omitted.
        #[arg(long)]
        scenario: Option<PathBuf>,
        /// Write the campaign log to this path.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Print the campaign log as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Print or write the built-in scenario template.
    Scenario {
        /// Write the scenario JSON to this path.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Verify every operation in a saved campaign log.
    Replay {
        /// Path to a campaign log JSON.
        log: PathBuf,
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
        Command::Air { command } => match command {
            AirCommand::Run {
                input,
                seed,
                json,
                out,
            } => {
                let mut input: DogfightInput = read_json(&input)?;
                if let Some(seed) = seed {
                    input.seed = parse_seed(&seed)?;
                }
                let log = simulate_dogfight(&input)?;
                if let Some(path) = out {
                    write_json(&path, &log)?;
                }
                if json {
                    println!("{}", serde_json::to_string_pretty(&log)?);
                } else {
                    print_dogfight(&log);
                }
            }
            AirCommand::Replay { log, json } => {
                let log: DogfightLog = read_json(&log)?;
                let verified = replay_dogfight(&log)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&verified)?);
                } else {
                    println!("Dogfight replay verified.");
                    println!();
                    print_dogfight(&verified);
                }
            }
        },
        Command::Naval { command } => match command {
            NavalCommand::Run {
                input,
                seed,
                json,
                out,
            } => {
                let mut input: NavalInput = read_json(&input)?;
                if let Some(seed) = seed {
                    input.seed = parse_seed(&seed)?;
                }
                let log = simulate_naval(&input)?;
                if let Some(path) = out {
                    write_json(&path, &log)?;
                }
                if json {
                    println!("{}", serde_json::to_string_pretty(&log)?);
                } else {
                    print_naval(&log);
                }
            }
            NavalCommand::Replay { log, json } => {
                let log: NavalLog = read_json(&log)?;
                let verified = replay_naval(&log)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&verified)?);
                } else {
                    println!("Naval replay verified.");
                    println!();
                    print_naval(&verified);
                }
            }
        },
        Command::Frontline { command } => match command {
            FrontlineCommand::Run {
                input,
                seed,
                json,
                out,
            } => {
                let mut input: FrontlineInput = read_json(&input)?;
                if let Some(seed) = seed {
                    input.seed = parse_seed(&seed)?;
                }
                let log = simulate_frontline(&input)?;
                if let Some(path) = out {
                    write_json(&path, &log)?;
                }
                if json {
                    println!("{}", serde_json::to_string_pretty(&log)?);
                } else {
                    print_frontline(&log);
                }
            }
            FrontlineCommand::Replay { log, json } => {
                let log: FrontlineLog = read_json(&log)?;
                let verified = replay_frontline(&log)?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&verified)?);
                } else {
                    println!("Frontline replay verified.");
                    println!();
                    print_frontline(&verified);
                }
            }
        },
        Command::Campaign { command } => match command {
            CampaignCommand::Play { seed, scenario } => {
                let scenario = load_scenario(scenario)?;
                play_campaign(campaign_seed(seed)?, scenario)?;
            }
            CampaignCommand::Demo {
                seed,
                scenario,
                out,
                json,
            } => {
                let scenario = load_scenario(scenario)?;
                let mut campaign = Campaign::from_scenario(campaign_seed(seed)?, scenario)?;
                for id in OperationId::ALL {
                    campaign.play_operation(id)?;
                }
                if let Some(path) = out {
                    write_json(&path, &campaign)?;
                }
                if json {
                    println!("{}", serde_json::to_string_pretty(&campaign)?);
                } else {
                    print_campaign(&campaign);
                }
            }
            CampaignCommand::Scenario { out } => {
                let scenario = CampaignScenario::default();
                if let Some(path) = out {
                    write_json(&path, &scenario)?;
                } else {
                    println!("{}", serde_json::to_string_pretty(&scenario)?);
                }
            }
            CampaignCommand::Replay { log } => {
                let campaign: Campaign = read_json(&log)?;
                replay_campaign(&campaign)?;
                println!("Campaign replay verified.");
                println!();
                print_campaign(&campaign);
            }
        },
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

fn campaign_seed(value: Option<String>) -> Result<Seed> {
    match value {
        Some(value) => parse_seed(&value),
        None => Ok(Seed::from_u64(5)),
    }
}

fn load_scenario(path: Option<PathBuf>) -> Result<CampaignScenario> {
    match path {
        Some(path) => read_json(&path),
        None => Ok(CampaignScenario::default()),
    }
}

fn play_campaign(seed: Seed, scenario: CampaignScenario) -> Result<()> {
    let mut campaign = Campaign::from_scenario(seed, scenario)?;
    while !campaign.finished {
        print_campaign_status(&campaign);
        let options = campaign.options();
        for (index, option) in options.iter().enumerate() {
            println!(
                "  {}. {} — {} [{}]",
                index + 1,
                option.title,
                option.kind.label(),
                option.reason
            );
        }
        print!("Choose an operation: ");
        io::stdout().flush()?;
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        let choice = match line.trim().parse::<usize>() {
            Ok(value) if (1..=options.len()).contains(&value) => value - 1,
            _ => {
                println!("Enter a number from 1 to {}.\n", options.len());
                continue;
            }
        };
        let option = &options[choice];
        if !option.available {
            println!("That operation is not available.\n");
            continue;
        }
        match campaign.play_operation(option.id) {
            Ok(record) => print_operation(&record),
            Err(err) => println!("Operation failed: {err}\n"),
        }
    }
    print_campaign(&campaign);
    Ok(())
}

fn print_campaign_status(campaign: &Campaign) {
    println!(
        "\n=== {} · Turn {}/{} ===",
        campaign.name, campaign.turn, campaign.max_turns
    );
    println!(
        "Will {}  ·  Supply {}  ·  Intel {}  ·  Momentum {:+}  ·  Enemy: {}",
        campaign.political_will,
        campaign.supply,
        campaign.intel,
        campaign.momentum,
        campaign.enemy_intent.label()
    );
    println!(
        "Ruleset: {} · scenario: {}",
        campaign.scenario.modules.preset_name(),
        campaign.scenario.name
    );
    for node in &campaign.nodes {
        println!(
            "  {:<16} {:<10} {}",
            node.name,
            format!("{:?}", node.control).to_lowercase(),
            if node.objective { "objective" } else { "" }
        );
    }
    for formation in &campaign.formations {
        println!(
            "  {:<34} str {:>3}  morale {:>3}  supply {:>3}",
            formation.name, formation.strength, formation.morale, formation.supply
        );
    }
}

fn print_operation(record: &OperationRecord) {
    println!(
        "\n{} · {} — {}",
        record.title,
        record.kind.label(),
        record.aftermath.summary
    );
    println!(
        "  formation {} · strength {:+} · will {:+} · supply {:+} · intel {:+} · momentum {:+}",
        record.aftermath.formation_id,
        record.aftermath.strength_delta,
        record.aftermath.will_delta,
        record.aftermath.supply_delta,
        record.aftermath.intel_delta,
        record.aftermath.momentum_delta
    );
}

fn print_campaign(campaign: &Campaign) {
    println!("\n=== {} ===", campaign.name);
    if let Some(result) = campaign.result {
        println!("Result: {:?}", result);
    } else {
        println!("Result: incomplete");
    }
    println!(
        "Will {} · Supply {} · Intel {} · Momentum {:+}",
        campaign.political_will, campaign.supply, campaign.intel, campaign.momentum
    );
    println!("\nOperations:");
    for operation in &campaign.operations {
        println!(
            "  Turn {} · {} · {} — {}",
            operation.turn,
            operation.title,
            operation.kind.label(),
            operation.aftermath.summary
        );
    }
    println!("\nChronicle:");
    for line in &campaign.chronicle {
        println!("  {line}");
    }
}

fn print_frontline(log: &FrontlineLog) {
    let a = &log.input.attacker.name;
    let d = &log.input.defender.name;

    println!("Frontline: {a} (attacker) vs {d} (defender)");
    println!("Seed: {}", log.input.seed.to_hex());
    println!("Input hash: {}", log.input_hash);
    println!();

    for day in &log.days {
        println!("Day {}:", day.day);
        for sector in &day.sectors {
            println!(
                "  {:<10} {:<8} p={:5.1}% roll={:.3}  losses {:>5}/{:<5}  troops {:>5}/{:<5}{}",
                sector.sector,
                side_name(sector.sector_winner),
                sector.attacker_win_probability * 100.0,
                sector.roll,
                sector.attacker_losses,
                sector.defender_losses,
                sector.attacker_troops_after,
                sector.defender_troops_after,
                if sector.captured { "  CAPTURED" } else { "" }
            );
        }
    }

    println!();
    let outcome = classify_frontline(log);
    match log.result.winner {
        Winner::Draw => println!(
            "Result: draw — {} after {} days.",
            outcome.label(),
            log.days.len()
        ),
        winner => println!(
            "Result: {} — {} after {} days.",
            frontline_winner(winner, log),
            outcome.label(),
            log.days.len()
        ),
    }
    println!("Ending: {}.", frontline_ending(log.result.ending));
    println!(
        "Objectives: {} / {}",
        log.result.objectives_captured, log.result.objectives_total
    );
    println!(
        "Final reserves: {} / {}",
        log.result.final_reserves.attacker, log.result.final_reserves.defender
    );
    println!(
        "Final morale: {} / {}",
        log.result.final_morale.attacker, log.result.final_morale.defender
    );
    for sector in &log.result.sectors {
        println!(
            "  {:<10} troops {:>5}/{:<5}  {}{}",
            sector.name,
            sector.attacker_troops,
            sector.defender_troops,
            if sector.objective { "objective " } else { "" },
            if sector.captured {
                "captured"
            } else {
                "contested"
            }
        );
    }
}

fn frontline_winner(winner: Winner, log: &FrontlineLog) -> &str {
    match winner {
        Winner::Attacker => &log.input.attacker.name,
        Winner::Defender => &log.input.defender.name,
        Winner::Draw => "Draw",
    }
}

fn frontline_ending(ending: FrontlineEnding) -> &'static str {
    match ending {
        FrontlineEnding::Breakthrough => "breakthrough",
        FrontlineEnding::Exhaustion => "exhaustion",
        FrontlineEnding::Withdrawal => "withdrawal",
        FrontlineEnding::MaxDays => "day cap",
        FrontlineEnding::Draw => "draw",
    }
}

fn print_naval(log: &NavalLog) {
    let a = &log.input.attacker.name;
    let d = &log.input.defender.name;

    println!("Naval action: {a} (attacker) vs {d} (defender)");
    println!("Seed: {}", log.input.seed.to_hex());
    println!("Input hash: {}", log.input_hash);
    println!();

    for s in &log.salvos {
        println!(
            "Salvo {:>2}: {:<8} p={:5.1}% roll={:.3}  sunk {:>2}/{:<2}  ships {:>2}/{:<2}  readiness {:>3}/{:<3}",
            s.salvo,
            side_name(s.salvo_winner),
            s.attacker_win_probability * 100.0,
            s.roll,
            s.attacker_sunk,
            s.defender_sunk,
            s.attacker_ships_after,
            s.defender_ships_after,
            s.attacker_readiness_after,
            s.defender_readiness_after
        );
    }

    println!();
    let outcome = classify_naval(log);
    match log.result.winner {
        Winner::Draw => println!(
            "Result: draw — {} after {} salvos.",
            outcome.label(),
            log.salvos.len()
        ),
        winner => println!(
            "Result: {} — {} after {} salvos.",
            naval_winner(winner, log),
            outcome.label(),
            log.salvos.len()
        ),
    }
    println!("Ending: {}.", naval_ending(log.result.ending));
    println!(
        "Final ships: {} / {}",
        log.result.final_ships.attacker, log.result.final_ships.defender
    );
    println!(
        "Final readiness: {} / {}",
        log.result.final_readiness.attacker, log.result.final_readiness.defender
    );
}

fn naval_winner(winner: Winner, log: &NavalLog) -> &str {
    match winner {
        Winner::Attacker => &log.input.attacker.name,
        Winner::Defender => &log.input.defender.name,
        Winner::Draw => "Draw",
    }
}

fn naval_ending(ending: NavalEnding) -> &'static str {
    match ending {
        NavalEnding::Sunk => "fleet sunk",
        NavalEnding::Withdrawal => "withdrawal",
        NavalEnding::MaxSalvos => "salvo cap",
        NavalEnding::Draw => "draw",
    }
}

fn print_dogfight(log: &DogfightLog) {
    let a = &log.input.attacker.name;
    let d = &log.input.defender.name;

    println!("Dogfight: {a} (attacker) vs {d} (defender)");
    println!("Seed: {}", log.input.seed.to_hex());
    println!("Input hash: {}", log.input_hash);
    println!();

    for p in &log.passes {
        println!(
            "Pass {:>2}: {:<8} p={:5.1}% roll={:.3}  kills {:>2}/{:<2}  aircraft {:>2}/{:<2}  cohesion {:>3}/{:<3}",
            p.pass,
            side_name(p.pass_winner),
            p.attacker_win_probability * 100.0,
            p.roll,
            p.attacker_kills,
            p.defender_kills,
            p.attacker_aircraft_after,
            p.defender_aircraft_after,
            p.attacker_cohesion_after,
            p.defender_cohesion_after
        );
    }

    println!();
    let outcome = classify_dogfight(log);
    match log.result.winner {
        Winner::Draw => println!(
            "Result: draw — {} after {} passes.",
            outcome.label(),
            log.passes.len()
        ),
        winner => println!(
            "Result: {} — {} after {} passes.",
            dogfight_winner(winner, log),
            outcome.label(),
            log.passes.len()
        ),
    }
    println!("Ending: {}.", dogfight_ending(log.result.ending));
    println!(
        "Final aircraft: {} / {}",
        log.result.final_aircraft.attacker, log.result.final_aircraft.defender
    );
    println!(
        "Final cohesion: {} / {}",
        log.result.final_cohesion.attacker, log.result.final_cohesion.defender
    );
}

fn dogfight_winner(winner: Winner, log: &DogfightLog) -> &str {
    match winner {
        Winner::Attacker => &log.input.attacker.name,
        Winner::Defender => &log.input.defender.name,
        Winner::Draw => "Draw",
    }
}

fn dogfight_ending(ending: DogfightEnding) -> &'static str {
    match ending {
        DogfightEnding::Splashed => "splashed",
        DogfightEnding::BreakOff => "break-off",
        DogfightEnding::MaxPasses => "pass cap",
        DogfightEnding::Draw => "draw",
    }
}

fn print_nation(nation: &drumfire_ns::NationSummary) {
    if nation.fullname.is_empty() || nation.fullname == nation.name {
        println!("{}", nation.name);
    } else {
        println!("{}", nation.fullname);
        println!("Short name: {}", nation.name);
    }
    println!("Region: {}", nation.region);
    println!("Capital: {}", nation.display_capital());
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
