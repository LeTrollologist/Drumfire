//! Persistent campaign layer for Drumfire.
//!
//! The Halcyon Sweep vertical slice: three nodes, three formations, three
//! operations (Frontlines, Dogfight, Naval), and one enemy intent. The campaign
//! is a pure state machine; every battle is delegated to the existing mode
//! engines and its result is applied back to the campaign.

use drumfire_core::dogfight::{
    replay_dogfight, simulate_dogfight, DogfightError, DogfightLog, DogfightReplayError,
};
use drumfire_core::frontline::{
    replay_frontline, simulate_frontline, FrontlineError, FrontlineLog, FrontlineReplayError,
};
use drumfire_core::naval::{replay_naval, simulate_naval, NavalError, NavalLog, NavalReplayError};
use drumfire_core::Winner;
use drumfire_model::{
    AirForce, DogfightConfig, DogfightInput, Fleet, ForceCount, FrontForce, FrontlineConfig,
    FrontlineInput, Mode, Morale, NavalConfig, NavalInput, Rating, SectorInput, Seed, ShipCount,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const MAX_TURNS: u32 = 3;

/// Why a node matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Ridge,
    Airbase,
    Port,
}

/// Who controls a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Control {
    Player,
    Enemy,
    Contested,
}

/// A named formation that persists across operations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Formation {
    pub id: String,
    pub name: String,
    pub branch: Branch,
    pub strength: u8,
    pub morale: u8,
    pub supply: u8,
    pub experience: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Branch {
    Ground,
    Air,
    Naval,
    Reserve,
}

/// A theater node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub name: String,
    pub kind: NodeKind,
    pub control: Control,
    pub objective: bool,
    pub enemy_garrison: u32,
    pub fortification: u8,
    pub connections: Vec<String>,
}

/// The single enemy intent used by the vertical slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnemyIntent {
    Hold,
}

/// One of the three operations supported by the slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Frontlines,
    Dogfight,
    Naval,
}

impl OperationKind {
    pub fn label(self) -> &'static str {
        match self {
            OperationKind::Frontlines => "Frontlines",
            OperationKind::Dogfight => "Dogfight",
            OperationKind::Naval => "Naval",
        }
    }

    pub fn code(self) -> u8 {
        match self {
            OperationKind::Frontlines => 1,
            OperationKind::Dogfight => 2,
            OperationKind::Naval => 3,
        }
    }
}

/// An operation the player can choose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationOption {
    pub kind: OperationKind,
    pub node_id: String,
    pub title: String,
    pub available: bool,
    pub reason: String,
}

/// The full log of the chosen mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", content = "log", rename_all = "snake_case")]
pub enum OperationLog {
    Frontlines(FrontlineLog),
    Dogfight(DogfightLog),
    Naval(NavalLog),
}

/// What changed after an operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Aftermath {
    pub victory: bool,
    pub summary: String,
    pub formation_id: String,
    pub strength_delta: i32,
    pub will_delta: i32,
    pub supply_delta: i32,
    pub intel_delta: i32,
    pub momentum_delta: i32,
    pub control: Option<String>,
}

/// One operation in the campaign log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationRecord {
    pub turn: u32,
    pub kind: OperationKind,
    pub node_id: String,
    pub title: String,
    pub log: OperationLog,
    pub aftermath: Aftermath,
}

/// Final campaign result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CampaignResult {
    Victory,
    Defeat,
}

/// The campaign state and log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Campaign {
    pub name: String,
    pub seed: Seed,
    pub turn: u32,
    pub max_turns: u32,
    pub political_will: i32,
    pub supply: i32,
    pub intel: i32,
    pub momentum: i32,
    pub enemy_intent: EnemyIntent,
    pub nodes: Vec<Node>,
    pub formations: Vec<Formation>,
    pub operations: Vec<OperationRecord>,
    pub chronicle: Vec<String>,
    pub finished: bool,
    pub result: Option<CampaignResult>,
}

/// Errors from playing a campaign.
#[derive(Debug, Error)]
pub enum CampaignError {
    #[error("the campaign is already finished")]
    Finished,
    #[error("operation {0:?} is not available")]
    Unavailable(OperationKind),
    #[error("no formation is available for this operation")]
    MissingFormation,
    #[error(transparent)]
    Frontline(#[from] FrontlineError),
    #[error(transparent)]
    Dogfight(#[from] DogfightError),
    #[error(transparent)]
    Naval(#[from] NavalError),
    #[error("failed to serialize campaign: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// Errors from campaign replay verification.
#[derive(Debug, Error)]
pub enum CampaignReplayError {
    #[error(transparent)]
    Frontline(#[from] FrontlineReplayError),
    #[error(transparent)]
    Dogfight(#[from] DogfightReplayError),
    #[error(transparent)]
    Naval(#[from] NavalReplayError),
    #[error(transparent)]
    Campaign(#[from] CampaignError),
    #[error("campaign log is inconsistent at turn {0}")]
    Mismatch(u32),
    #[error("campaign log is inconsistent: {0}")]
    Inconsistent(String),
}

impl Campaign {
    /// Start the Halcyon Sweep.
    pub fn new(seed: Seed) -> Self {
        Self {
            name: "The Halcyon Sweep".to_string(),
            seed,
            turn: 1,
            max_turns: MAX_TURNS,
            political_will: 70,
            supply: 60,
            intel: 40,
            momentum: 0,
            enemy_intent: EnemyIntent::Hold,
            nodes: vec![
                Node {
                    id: "vantage".to_string(),
                    name: "Vantage Ridge".to_string(),
                    kind: NodeKind::Ridge,
                    control: Control::Contested,
                    objective: true,
                    enemy_garrison: 6000,
                    fortification: 6,
                    connections: vec!["halcyon".to_string(), "callas".to_string()],
                },
                Node {
                    id: "halcyon".to_string(),
                    name: "Halcyon Airbase".to_string(),
                    kind: NodeKind::Airbase,
                    control: Control::Enemy,
                    objective: true,
                    enemy_garrison: 6000,
                    fortification: 7,
                    connections: vec!["vantage".to_string(), "callas".to_string()],
                },
                Node {
                    id: "callas".to_string(),
                    name: "Port Callas".to_string(),
                    kind: NodeKind::Port,
                    control: Control::Enemy,
                    objective: true,
                    enemy_garrison: 10000,
                    fortification: 6,
                    connections: vec!["vantage".to_string(), "halcyon".to_string()],
                },
            ],
            formations: vec![
                Formation {
                    id: "old-ironsides".to_string(),
                    name: "1st Armored — Old Ironsides".to_string(),
                    branch: Branch::Ground,
                    strength: 95,
                    morale: 75,
                    supply: 80,
                    experience: 2,
                },
                Formation {
                    id: "nightjars".to_string(),
                    name: "4th Air Wing — Nightjars".to_string(),
                    branch: Branch::Air,
                    strength: 85,
                    morale: 80,
                    supply: 80,
                    experience: 2,
                },
                Formation {
                    id: "salt-watch".to_string(),
                    name: "2nd Naval Group — Salt Watch".to_string(),
                    branch: Branch::Naval,
                    strength: 90,
                    morale: 75,
                    supply: 80,
                    experience: 2,
                },
            ],
            operations: Vec::new(),
            chronicle: Vec::new(),
            finished: false,
            result: None,
        }
    }

    /// Run the canonical three-operation demo.
    pub fn demo(seed: Seed) -> Result<Self, CampaignError> {
        let mut campaign = Self::new(seed);
        for kind in [
            OperationKind::Frontlines,
            OperationKind::Dogfight,
            OperationKind::Naval,
        ] {
            campaign.play_operation(kind)?;
        }
        Ok(campaign)
    }

    /// The current air-superiority value derived from node control.
    pub fn air_superiority(&self) -> u8 {
        if self.node("halcyon").map(|node| node.control) == Some(Control::Player) {
            75
        } else {
            40
        }
    }

    /// The current naval-control value derived from node control.
    pub fn naval_control(&self) -> u8 {
        if self.node("callas").map(|node| node.control) == Some(Control::Player) {
            75
        } else {
            40
        }
    }

    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|node| node.id == id)
    }

    fn node_mut(&mut self, id: &str) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|node| node.id == id)
    }

    fn formation(&self, branch: Branch) -> Option<&Formation> {
        self.formations.iter().find(|f| f.branch == branch)
    }

    fn formation_mut(&mut self, id: &str) -> Option<&mut Formation> {
        self.formations.iter_mut().find(|f| f.id == id)
    }

    /// The three operation choices for this slice.
    pub fn options(&self) -> Vec<OperationOption> {
        let done = |kind| self.operations.iter().any(|op| op.kind == kind);
        let make = |kind: OperationKind, node_id: &str, title: &str| {
            let node = self.node(node_id);
            let available = !self.finished
                && !done(kind)
                && node
                    .map(|node| node.control != Control::Player)
                    .unwrap_or(false);
            let reason = if self.finished {
                "campaign finished".to_string()
            } else if done(kind) {
                "already attempted".to_string()
            } else if node
                .map(|node| node.control == Control::Player)
                .unwrap_or(false)
            {
                "already controlled".to_string()
            } else {
                "available".to_string()
            };
            OperationOption {
                kind,
                node_id: node_id.to_string(),
                title: title.to_string(),
                available,
                reason,
            }
        };

        vec![
            make(OperationKind::Frontlines, "vantage", "Operation Iron Tide"),
            make(OperationKind::Dogfight, "halcyon", "Operation Bright Lance"),
            make(OperationKind::Naval, "callas", "Operation Crimson Wake"),
        ]
    }

    /// Build the front-line input without mutating the campaign.
    pub fn frontline_input(&self) -> Result<FrontlineInput, CampaignError> {
        let formation = self
            .formation(Branch::Ground)
            .ok_or(CampaignError::MissingFormation)?;
        let node = self
            .node("vantage")
            .ok_or(CampaignError::MissingFormation)?;

        let total = (formation.strength as u32 * 100).max(100);
        let split = [total * 30 / 100, total * 45 / 100, total * 25 / 100];
        let enemy = node.enemy_garrison;
        let enemy_split = [enemy * 30 / 100, enemy * 45 / 100, enemy * 25 / 100];
        let air = self.air_superiority() >= 60;
        let intel = (5 + u8::from(air)).min(10);
        let training = (4 + formation.experience / 2 + u8::from(air)).min(10);
        let experience = (4 + formation.experience).min(10);
        let logistics = (formation.supply / 20).clamp(1, 10);

        Ok(FrontlineInput {
            mode: Mode::Frontlines,
            attacker: FrontForce {
                name: formation.name.clone(),
                reserves: (formation.strength as u32 * 20).max(500),
                morale: Morale::new(formation.morale).unwrap_or(Morale::new(50).unwrap()),
                tech: Rating::new(6).unwrap(),
                training: Rating::new(training).unwrap(),
                experience: Rating::new(experience).unwrap(),
                logistics: Rating::new(logistics).unwrap(),
                intel: Rating::new(intel).unwrap(),
            },
            defender: FrontForce {
                name: "Halcyon Garrison".to_string(),
                reserves: 3000,
                morale: Morale::new(65).unwrap(),
                tech: Rating::new(5).unwrap(),
                training: Rating::new(5).unwrap(),
                experience: Rating::new(5).unwrap(),
                logistics: Rating::new(5).unwrap(),
                intel: Rating::new(5).unwrap(),
            },
            sectors: vec![
                SectorInput {
                    name: "North".to_string(),
                    attacker_troops: ForceCount::new(split[0].max(1)).unwrap(),
                    defender_troops: ForceCount::new(enemy_split[0].max(1)).unwrap(),
                    fortification: Rating::new(node.fortification.saturating_sub(1).max(1))
                        .unwrap(),
                    objective: false,
                },
                SectorInput {
                    name: "Center".to_string(),
                    attacker_troops: ForceCount::new(split[1].max(1)).unwrap(),
                    defender_troops: ForceCount::new(enemy_split[1].max(1)).unwrap(),
                    fortification: Rating::new(node.fortification).unwrap(),
                    objective: true,
                },
                SectorInput {
                    name: "South".to_string(),
                    attacker_troops: ForceCount::new(split[2].max(1)).unwrap(),
                    defender_troops: ForceCount::new(enemy_split[2].max(1)).unwrap(),
                    fortification: Rating::new(node.fortification.saturating_sub(1).max(1))
                        .unwrap(),
                    objective: false,
                },
            ],
            config: FrontlineConfig {
                max_days: 12,
                casualty_rate: 0.20,
                capture_threshold: 0.25,
                ..FrontlineConfig::default()
            },
            seed: self.operation_seed(OperationKind::Frontlines),
        })
    }

    /// Build the dogfight input without mutating the campaign.
    pub fn dogfight_input(&self) -> Result<DogfightInput, CampaignError> {
        let formation = self
            .formation(Branch::Air)
            .ok_or(CampaignError::MissingFormation)?;
        let aircraft = (formation.strength / 12).clamp(3, 10);
        let skill = (4 + formation.experience).min(10);
        let quality = (formation.supply / 20).clamp(1, 10);

        Ok(DogfightInput {
            mode: Mode::Dogfight,
            attacker: AirForce {
                name: formation.name.clone(),
                aircraft: drumfire_model::AircraftCount::new(aircraft).unwrap(),
                pilot_skill: Rating::new(skill).unwrap(),
                aircraft_quality: Rating::new(quality).unwrap(),
                weapons: Rating::new(6).unwrap(),
                radar: Rating::new(6).unwrap(),
                support: Rating::new(5).unwrap(),
                cohesion: Morale::new(formation.morale).unwrap(),
            },
            defender: AirForce {
                name: "Halcyon Interceptors".to_string(),
                aircraft: drumfire_model::AircraftCount::new(8).unwrap(),
                pilot_skill: Rating::new(6).unwrap(),
                aircraft_quality: Rating::new(6).unwrap(),
                weapons: Rating::new(6).unwrap(),
                radar: Rating::new(5).unwrap(),
                support: Rating::new(5).unwrap(),
                cohesion: Morale::new(70).unwrap(),
            },
            config: DogfightConfig::default(),
            seed: self.operation_seed(OperationKind::Dogfight),
        })
    }

    /// Build the naval input, including the air-superiority bonus when Halcyon
    /// is controlled.
    pub fn naval_input(&self) -> Result<NavalInput, CampaignError> {
        let formation = self
            .formation(Branch::Naval)
            .ok_or(CampaignError::MissingFormation)?;
        let ships = (formation.strength / 15).clamp(2, 8);
        let air_cover = 5 + if self.air_superiority() >= 60 { 2 } else { 0 };
        let damage_control = (formation.supply / 20).clamp(1, 10);

        Ok(NavalInput {
            mode: Mode::Naval,
            attacker: Fleet {
                name: formation.name.clone(),
                ships: ShipCount::new(ships).unwrap(),
                hull_quality: Rating::new(6).unwrap(),
                sensors: Rating::new(6).unwrap(),
                weapons: Rating::new(6).unwrap(),
                air_cover: Rating::new(air_cover).unwrap(),
                damage_control: Rating::new(damage_control).unwrap(),
                readiness: Morale::new(formation.morale).unwrap(),
            },
            defender: Fleet {
                name: "Callas Defense Flotilla".to_string(),
                ships: ShipCount::new(4).unwrap(),
                hull_quality: Rating::new(6).unwrap(),
                sensors: Rating::new(6).unwrap(),
                weapons: Rating::new(6).unwrap(),
                air_cover: Rating::new(5).unwrap(),
                damage_control: Rating::new(5).unwrap(),
                readiness: Morale::new(65).unwrap(),
            },
            config: NavalConfig::default(),
            seed: self.operation_seed(OperationKind::Naval),
        })
    }

    fn operation_seed(&self, kind: OperationKind) -> Seed {
        let mut hasher = Sha256::new();
        hasher.update(self.seed.as_bytes());
        hasher.update(self.turn.to_le_bytes());
        hasher.update([kind.code()]);
        let digest = hasher.finalize();
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&digest);
        Seed::from_bytes(bytes)
    }

    /// Play one operation and apply its aftermath.
    pub fn play_operation(
        &mut self,
        kind: OperationKind,
    ) -> Result<OperationRecord, CampaignError> {
        if self.finished {
            return Err(CampaignError::Finished);
        }
        let option = self
            .options()
            .into_iter()
            .find(|option| option.kind == kind)
            .ok_or(CampaignError::Unavailable(kind))?;
        if !option.available {
            return Err(CampaignError::Unavailable(kind));
        }

        let record = match kind {
            OperationKind::Frontlines => {
                let input = self.frontline_input()?;
                let log = simulate_frontline(&input)?;
                let aftermath = self.apply_frontline(&log)?;
                OperationRecord {
                    turn: self.turn,
                    kind,
                    node_id: "vantage".to_string(),
                    title: "Operation Iron Tide".to_string(),
                    log: OperationLog::Frontlines(log),
                    aftermath,
                }
            }
            OperationKind::Dogfight => {
                let input = self.dogfight_input()?;
                let log = simulate_dogfight(&input)?;
                let aftermath = self.apply_dogfight(&log)?;
                OperationRecord {
                    turn: self.turn,
                    kind,
                    node_id: "halcyon".to_string(),
                    title: "Operation Bright Lance".to_string(),
                    log: OperationLog::Dogfight(log),
                    aftermath,
                }
            }
            OperationKind::Naval => {
                let input = self.naval_input()?;
                let log = simulate_naval(&input)?;
                let aftermath = self.apply_naval(&log)?;
                OperationRecord {
                    turn: self.turn,
                    kind,
                    node_id: "callas".to_string(),
                    title: "Operation Crimson Wake".to_string(),
                    log: OperationLog::Naval(log),
                    aftermath,
                }
            }
        };

        self.operations.push(record.clone());
        self.chronicle.push(format!(
            "Turn {} · {} · {} — {}",
            record.turn,
            record.title,
            self.node(&record.node_id)
                .map(|node| node.name.as_str())
                .unwrap_or("unknown"),
            record.aftermath.summary
        ));
        self.turn += 1;
        self.check_end();
        Ok(record)
    }

    fn apply_frontline(&mut self, log: &FrontlineLog) -> Result<Aftermath, CampaignError> {
        let formation_id = self
            .formation(Branch::Ground)
            .map(|f| f.id.clone())
            .ok_or(CampaignError::MissingFormation)?;
        let initial = log
            .input
            .sectors
            .iter()
            .map(|s| s.attacker_troops.get())
            .sum::<u32>()
            + log.input.attacker.reserves;
        let final_troops = log
            .result
            .sectors
            .iter()
            .map(|s| s.attacker_troops)
            .sum::<u32>()
            + log.result.final_reserves.attacker;
        let loss_pct = if initial > 0 {
            (((initial.saturating_sub(final_troops)) as f64 / initial as f64) * 100.0).round()
                as i32
        } else {
            0
        };
        let victory = log.result.winner == Winner::Attacker;
        let strength_delta = -(loss_pct / 2).min(30);
        let will_delta = if victory { 8 } else { -12 };
        let supply_delta = if victory { 5 } else { -10 };
        let intel_delta = if victory { 3 } else { -3 };
        let momentum_delta = if victory { 1 } else { -2 };

        self.apply_formation(&formation_id, strength_delta, victory);
        self.apply_resources(will_delta, supply_delta, intel_delta, momentum_delta);
        if victory {
            if let Some(node) = self.node_mut("vantage") {
                node.control = Control::Player;
            }
        } else if let Some(node) = self.node_mut("vantage") {
            node.control = Control::Enemy;
        }

        Ok(Aftermath {
            victory,
            summary: format!(
                "{} · {}% losses · {}",
                if victory {
                    "ridge taken"
                } else {
                    "attack stalled"
                },
                loss_pct,
                if victory {
                    "the road inland is open"
                } else {
                    "the front falls back"
                }
            ),
            formation_id,
            strength_delta,
            will_delta,
            supply_delta,
            intel_delta,
            momentum_delta,
            control: Some("vantage".to_string()),
        })
    }

    fn apply_dogfight(&mut self, log: &DogfightLog) -> Result<Aftermath, CampaignError> {
        let formation_id = self
            .formation(Branch::Air)
            .map(|f| f.id.clone())
            .ok_or(CampaignError::MissingFormation)?;
        let initial = log.input.attacker.aircraft.get() as f64;
        let final_air = log.result.final_aircraft.attacker as f64;
        let loss_pct = (((initial - final_air) / initial.max(1.0)) * 100.0).round() as i32;
        let victory = log.result.winner == Winner::Attacker;
        let strength_delta = -(loss_pct / 2).min(30);
        let will_delta = if victory { 3 } else { -5 };
        let intel_delta = if victory { 12 } else { -4 };
        let momentum_delta = if victory { 1 } else { -1 };

        self.apply_formation(&formation_id, strength_delta, victory);
        self.apply_resources(will_delta, 0, intel_delta, momentum_delta);
        if victory {
            if let Some(node) = self.node_mut("halcyon") {
                node.control = Control::Player;
            }
        } else if let Some(node) = self.node_mut("halcyon") {
            node.control = Control::Enemy;
        }

        Ok(Aftermath {
            victory,
            summary: format!(
                "{} · {}% aircraft lost · air superiority {}",
                if victory { "sweep won" } else { "sweep lost" },
                loss_pct,
                self.air_superiority()
            ),
            formation_id,
            strength_delta,
            will_delta,
            supply_delta: 0,
            intel_delta,
            momentum_delta,
            control: Some("halcyon".to_string()),
        })
    }

    fn apply_naval(&mut self, log: &NavalLog) -> Result<Aftermath, CampaignError> {
        let formation_id = self
            .formation(Branch::Naval)
            .map(|f| f.id.clone())
            .ok_or(CampaignError::MissingFormation)?;
        let initial = log.input.attacker.ships.get() as f64;
        let final_ships = log.result.final_ships.attacker as f64;
        let loss_pct = (((initial - final_ships) / initial.max(1.0)) * 100.0).round() as i32;
        let victory = log.result.winner == Winner::Attacker;
        let strength_delta = -(loss_pct / 2).min(30);
        let will_delta = if victory { 4 } else { -6 };
        let supply_delta = if victory { 12 } else { -10 };
        let momentum_delta = if victory { 1 } else { -1 };

        self.apply_formation(&formation_id, strength_delta, victory);
        self.apply_resources(will_delta, supply_delta, 0, momentum_delta);
        if victory {
            if let Some(node) = self.node_mut("callas") {
                node.control = Control::Player;
            }
        } else if let Some(node) = self.node_mut("callas") {
            node.control = Control::Enemy;
        }

        Ok(Aftermath {
            victory,
            summary: format!(
                "{} · {}% ships lost · supply {}",
                if victory {
                    "port opened"
                } else {
                    "convoy turned back"
                },
                loss_pct,
                self.supply
            ),
            formation_id,
            strength_delta,
            will_delta,
            supply_delta,
            intel_delta: 0,
            momentum_delta,
            control: Some("callas".to_string()),
        })
    }

    fn apply_formation(&mut self, id: &str, strength_delta: i32, victory: bool) {
        if let Some(formation) = self.formation_mut(id) {
            formation.strength = (formation.strength as i32 + strength_delta).clamp(10, 100) as u8;
            let morale_delta = if victory { 3 } else { -8 };
            formation.morale = (formation.morale as i32 + morale_delta).clamp(0, 100) as u8;
            formation.supply = (formation.supply as i32 - 5).clamp(0, 100) as u8;
        }
    }

    fn apply_resources(&mut self, will: i32, supply: i32, intel: i32, momentum: i32) {
        self.political_will = (self.political_will + will).clamp(0, 100);
        self.supply = (self.supply + supply).clamp(0, 100);
        self.intel = (self.intel + intel).clamp(0, 100);
        self.momentum = (self.momentum + momentum).clamp(-5, 5);
    }

    fn check_end(&mut self) {
        if self.political_will <= 0 {
            self.finish(CampaignResult::Defeat);
            return;
        }
        if self
            .nodes
            .iter()
            .all(|node| node.control == Control::Player)
        {
            self.finish(CampaignResult::Victory);
            return;
        }
        if self.turn > self.max_turns {
            let held = self
                .nodes
                .iter()
                .filter(|node| node.control == Control::Player)
                .count();
            self.finish(if held == self.nodes.len() {
                CampaignResult::Victory
            } else {
                CampaignResult::Defeat
            });
        }
    }

    fn finish(&mut self, result: CampaignResult) {
        self.finished = true;
        self.result = Some(result);
        let verdict = match result {
            CampaignResult::Victory => "The Halcyon Sweep succeeds.",
            CampaignResult::Defeat => "The Halcyon Sweep stalls.",
        };
        self.chronicle.push(verdict.to_string());
    }
}

/// Re-verify every operation log and rebuild the campaign from its seed.
pub fn replay_campaign(campaign: &Campaign) -> Result<(), CampaignReplayError> {
    if campaign.operations.is_empty() {
        return Err(CampaignReplayError::Inconsistent(
            "campaign has no operations".to_string(),
        ));
    }

    for operation in &campaign.operations {
        match &operation.log {
            OperationLog::Frontlines(log) => {
                replay_frontline(log)?;
            }
            OperationLog::Dogfight(log) => {
                replay_dogfight(log)?;
            }
            OperationLog::Naval(log) => {
                replay_naval(log)?;
            }
        }
    }

    let mut rebuilt = Campaign::new(campaign.seed);
    for operation in &campaign.operations {
        let replay = rebuilt.play_operation(operation.kind)?;
        if replay != *operation {
            return Err(CampaignReplayError::Mismatch(operation.turn));
        }
    }
    if rebuilt != *campaign {
        return Err(CampaignReplayError::Inconsistent(
            "final campaign state differs from the log".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed() -> Seed {
        Seed::from_u64(31)
    }

    #[test]
    fn campaign_starts_with_three_options() {
        let campaign = Campaign::new(seed());
        let options = campaign.options();
        assert_eq!(options.len(), 3);
        assert!(options.iter().all(|option| option.available));
    }

    #[test]
    fn air_superiority_changes_naval_air_cover() {
        let mut campaign = Campaign::new(seed());
        let before = campaign.naval_input().unwrap().attacker.air_cover.get();
        campaign.node_mut("halcyon").unwrap().control = Control::Player;
        let after = campaign.naval_input().unwrap().attacker.air_cover.get();
        assert!(after > before);
    }

    #[test]
    fn demo_campaign_completes_and_replays() {
        let campaign = Campaign::demo(seed()).unwrap();
        assert_eq!(campaign.operations.len(), 3);
        assert!(campaign.finished);
        replay_campaign(&campaign).unwrap();
    }

    #[test]
    fn operation_cannot_be_repeated() {
        let mut campaign = Campaign::new(seed());
        campaign.play_operation(OperationKind::Dogfight).unwrap();
        assert!(matches!(
            campaign.play_operation(OperationKind::Dogfight),
            Err(CampaignError::Unavailable(OperationKind::Dogfight))
        ));
    }

    #[test]
    fn resources_stay_in_bounds() {
        let campaign = Campaign::demo(seed()).unwrap();
        assert!((0..=100).contains(&campaign.political_will));
        assert!((0..=100).contains(&campaign.supply));
        assert!((0..=100).contains(&campaign.intel));
        assert!((-5..=5).contains(&campaign.momentum));
    }

    #[test]
    fn demo_seed_five_wins() {
        let campaign = Campaign::demo(Seed::from_u64(5)).unwrap();
        assert_eq!(campaign.result, Some(CampaignResult::Victory));
    }

    #[test]
    fn committed_campaign_log_replays() {
        let campaign: Campaign =
            serde_json::from_str(include_str!("../../../fixtures/campaign.log.json"))
                .expect("committed campaign log must be valid");
        assert_eq!(campaign.operations.len(), 3);
        replay_campaign(&campaign).unwrap();
    }
}
