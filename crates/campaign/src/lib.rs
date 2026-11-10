//! Persistent campaign layer for Drumfire.
//!
//! The Halcyon Sweep: seven nodes across Kestrel Coast, five named formations,
//! seven operations using all five battle modes, persistent resources, enemy
//! intents, and exact campaign replay.

pub mod referee;

use std::collections::HashSet;

use drumfire_core::dogfight::{
    replay_dogfight, simulate_dogfight, DogfightError, DogfightLog, DogfightReplayError,
};
use drumfire_core::frontline::{
    replay_frontline, simulate_frontline, FrontlineError, FrontlineLog, FrontlineReplayError,
};
use drumfire_core::naval::{replay_naval, simulate_naval, NavalError, NavalLog, NavalReplayError};
use drumfire_core::{replay, simulate, BattleLog, ReplayError, SimError, Winner};
use drumfire_model::{
    AirForce, BattleConfig, BattleInput, DogfightConfig, DogfightInput, Fleet, ForceCount,
    FrontForce, FrontlineConfig, FrontlineInput, Mode, Morale, NavalConfig, NavalInput, Rating,
    SectorInput, Seed, ShipCount, TeamInput,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const MAX_TURNS: u32 = 8;

/// Why a node matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Ridge,
    Airbase,
    Bridge,
    Port,
    RailHub,
    Industrial,
    Capital,
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

/// The enemy's operational posture for the next turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnemyIntent {
    Hold,
    Reinforce,
    Counterattack,
    Interdict,
    Withdraw,
}

impl EnemyIntent {
    pub fn label(self) -> &'static str {
        match self {
            EnemyIntent::Hold => "Hold",
            EnemyIntent::Reinforce => "Reinforce",
            EnemyIntent::Counterattack => "Counterattack",
            EnemyIntent::Interdict => "Interdict",
            EnemyIntent::Withdraw => "Withdraw",
        }
    }
}

/// The battle mode used by an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Frontlines,
    Skirmish,
    Battle,
    Dogfight,
    Naval,
}

impl OperationKind {
    pub fn label(self) -> &'static str {
        match self {
            OperationKind::Frontlines => "Frontlines",
            OperationKind::Skirmish => "Skirmish",
            OperationKind::Battle => "Battle",
            OperationKind::Dogfight => "Dogfight",
            OperationKind::Naval => "Naval",
        }
    }

    pub fn code(self) -> u8 {
        match self {
            OperationKind::Frontlines => 1,
            OperationKind::Skirmish => 2,
            OperationKind::Battle => 3,
            OperationKind::Dogfight => 4,
            OperationKind::Naval => 5,
        }
    }
}

/// Which campaign systems are switched on for this war.
///
/// These are ruleset toggles, not just display flags. Modules gate operations
/// and change how results are applied. Modules without a full simulation yet
/// are marked reserved in the GUI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CampaignModules {
    pub advanced_rules: bool,
    pub intel: bool,
    pub logistics: bool,
    pub manpower: bool,
    pub reserve_manpower: bool,
    pub tanks: bool,
    pub planes: bool,
    pub ships: bool,
    pub satellites: bool,
    pub strategic_weapons: bool,
    pub rods_from_god: bool,
    pub nuclear_weapons: bool,
    pub population: bool,
    pub population_stability: bool,
    pub occupation: bool,
    pub liberation: bool,
    pub nationstates: bool,
    pub alliances: bool,
    pub diplomacy: bool,
}

impl Default for CampaignModules {
    fn default() -> Self {
        Self::preset(ModulePreset::CombinedArms)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModulePreset {
    Abstract,
    CombinedArms,
    FullSpectrum,
}

impl CampaignModules {
    pub fn preset(preset: ModulePreset) -> Self {
        match preset {
            ModulePreset::Abstract => Self {
                advanced_rules: false,
                intel: false,
                logistics: false,
                manpower: false,
                reserve_manpower: false,
                tanks: false,
                planes: true,
                ships: true,
                satellites: false,
                strategic_weapons: false,
                rods_from_god: false,
                nuclear_weapons: false,
                population: false,
                population_stability: false,
                occupation: false,
                liberation: false,
                nationstates: false,
                alliances: false,
                diplomacy: false,
            },
            ModulePreset::CombinedArms => Self {
                advanced_rules: true,
                intel: true,
                logistics: true,
                manpower: true,
                reserve_manpower: true,
                tanks: true,
                planes: true,
                ships: true,
                satellites: true,
                strategic_weapons: false,
                rods_from_god: false,
                nuclear_weapons: false,
                population: true,
                population_stability: true,
                occupation: true,
                liberation: false,
                nationstates: true,
                alliances: true,
                diplomacy: true,
            },
            ModulePreset::FullSpectrum => Self {
                advanced_rules: true,
                intel: true,
                logistics: true,
                manpower: true,
                reserve_manpower: true,
                tanks: true,
                planes: true,
                ships: true,
                satellites: true,
                strategic_weapons: true,
                rods_from_god: true,
                nuclear_weapons: true,
                population: true,
                population_stability: true,
                occupation: true,
                liberation: true,
                nationstates: true,
                alliances: true,
                diplomacy: true,
            },
        }
    }

    pub fn preset_name(&self) -> &'static str {
        if *self == Self::preset(ModulePreset::Abstract) {
            "Abstract"
        } else if *self == Self::preset(ModulePreset::CombinedArms) {
            "Combined Arms"
        } else if *self == Self::preset(ModulePreset::FullSpectrum) {
            "Full Spectrum"
        } else {
            "Custom"
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.reserve_manpower && !self.manpower {
            return Err("reserve manpower requires manpower".to_string());
        }
        if self.population_stability && !self.population {
            return Err("population stability requires population".to_string());
        }
        if (self.occupation || self.liberation) && !self.population {
            return Err("occupation and liberation require population".to_string());
        }
        if self.rods_from_god && (!self.satellites || !self.strategic_weapons) {
            return Err("rods from god require satellites and strategic weapons".to_string());
        }
        if self.nuclear_weapons && !self.strategic_weapons {
            return Err("nuclear weapons require strategic weapons".to_string());
        }
        if self.alliances && !self.diplomacy {
            return Err("alliances require diplomacy".to_string());
        }
        Ok(())
    }
}

/// A scenario is the initial state of a war. It is data, not a story.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CampaignScenario {
    pub name: String,
    pub player_name: String,
    pub enemy_name: String,
    pub max_turns: u32,
    pub starting_will: i32,
    pub starting_supply: i32,
    pub starting_intel: i32,
    pub modules: CampaignModules,
    pub nodes: Vec<Node>,
    pub formations: Vec<Formation>,
}

impl Default for CampaignScenario {
    fn default() -> Self {
        Self {
            name: "The Halcyon Sweep".to_string(),
            player_name: "Coalition".to_string(),
            enemy_name: "Concord".to_string(),
            max_turns: MAX_TURNS,
            starting_will: 70,
            starting_supply: 60,
            starting_intel: 40,
            modules: CampaignModules::default(),
            nodes: default_nodes(),
            formations: default_formations(),
        }
    }
}

impl CampaignScenario {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("scenario name must not be empty".to_string());
        }
        if self.player_name.trim().is_empty() || self.enemy_name.trim().is_empty() {
            return Err("both sides need names".to_string());
        }
        if !(1..=50).contains(&self.max_turns) {
            return Err("max_turns must be between 1 and 50".to_string());
        }
        for (label, value) in [
            ("starting_will", self.starting_will),
            ("starting_supply", self.starting_supply),
            ("starting_intel", self.starting_intel),
        ] {
            if !(0..=100).contains(&value) {
                return Err(format!("{label} must be between 0 and 100"));
            }
        }
        let mut node_ids = HashSet::new();
        for node in &self.nodes {
            if node.id.trim().is_empty() || node.name.trim().is_empty() {
                return Err("every node needs an id and a name".to_string());
            }
            if !node_ids.insert(node.id.as_str()) {
                return Err(format!("duplicate node id {}", node.id));
            }
            if !(1..=10).contains(&node.fortification) {
                return Err(format!(
                    "node {} fortification must be between 1 and 10",
                    node.id
                ));
            }
            if node.enemy_garrison == 0 {
                return Err(format!(
                    "node {} enemy_garrison must be at least 1",
                    node.id
                ));
            }
        }
        for node in &self.nodes {
            for connection in &node.connections {
                if connection == &node.id {
                    return Err(format!("node {} connects to itself", node.id));
                }
                if !node_ids.contains(connection.as_str()) {
                    return Err(format!(
                        "node {} connects to unknown node {}",
                        node.id, connection
                    ));
                }
            }
        }
        let mut formation_ids = HashSet::new();
        for formation in &self.formations {
            if formation.id.trim().is_empty() || formation.name.trim().is_empty() {
                return Err("every formation needs an id and a name".to_string());
            }
            if !formation_ids.insert(formation.id.as_str()) {
                return Err(format!("duplicate formation id {}", formation.id));
            }
            if !(1..=100).contains(&formation.strength) {
                return Err(format!(
                    "formation {} strength must be between 1 and 100",
                    formation.id
                ));
            }
            if formation.morale > 100 || formation.supply > 100 {
                return Err(format!(
                    "formation {} morale and supply must be between 0 and 100",
                    formation.id
                ));
            }
            if formation.experience > 10 {
                return Err(format!(
                    "formation {} experience must be between 0 and 10",
                    formation.id
                ));
            }
        }
        for required in [
            "vantage", "halcyon", "orne", "callas", "sable", "vesper", "meridian",
        ] {
            if !self.nodes.iter().any(|node| node.id == required) {
                return Err(format!("scenario is missing required node {required}"));
            }
        }
        for required in [
            "old-ironsides",
            "ridgewalkers",
            "nightjars",
            "salt-watch",
            "reserve-corps",
        ] {
            if !self
                .formations
                .iter()
                .any(|formation| formation.id == required)
            {
                return Err(format!("scenario is missing required formation {required}"));
            }
        }
        self.modules.validate()?;
        Ok(())
    }
}

fn default_nodes() -> Vec<Node> {
    vec![
        Node {
            id: "vantage".to_string(),
            name: "Vantage Ridge".to_string(),
            kind: NodeKind::Ridge,
            control: Control::Contested,
            objective: true,
            enemy_garrison: 6000,
            fortification: 6,
            connections: vec!["halcyon".to_string(), "orne".to_string()],
        },
        Node {
            id: "halcyon".to_string(),
            name: "Halcyon Airbase".to_string(),
            kind: NodeKind::Airbase,
            control: Control::Enemy,
            objective: true,
            enemy_garrison: 6000,
            fortification: 7,
            connections: vec!["vantage".to_string(), "sable".to_string()],
        },
        Node {
            id: "orne".to_string(),
            name: "Orne Bridge".to_string(),
            kind: NodeKind::Bridge,
            control: Control::Enemy,
            objective: true,
            enemy_garrison: 4500,
            fortification: 5,
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
            connections: vec!["orne".to_string(), "vesper".to_string()],
        },
        Node {
            id: "sable".to_string(),
            name: "Sable Junction".to_string(),
            kind: NodeKind::RailHub,
            control: Control::Enemy,
            objective: true,
            enemy_garrison: 6000,
            fortification: 7,
            connections: vec![
                "halcyon".to_string(),
                "vesper".to_string(),
                "meridian".to_string(),
            ],
        },
        Node {
            id: "vesper".to_string(),
            name: "Vesper Works".to_string(),
            kind: NodeKind::Industrial,
            control: Control::Enemy,
            objective: true,
            enemy_garrison: 6000,
            fortification: 8,
            connections: vec![
                "callas".to_string(),
                "sable".to_string(),
                "meridian".to_string(),
            ],
        },
        Node {
            id: "meridian".to_string(),
            name: "Meridian".to_string(),
            kind: NodeKind::Capital,
            control: Control::Enemy,
            objective: true,
            enemy_garrison: 500,
            fortification: 6,
            connections: vec!["sable".to_string(), "vesper".to_string()],
        },
    ]
}

fn default_formations() -> Vec<Formation> {
    vec![
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
            id: "ridgewalkers".to_string(),
            name: "7th Rifles — The Ridgewalkers".to_string(),
            branch: Branch::Ground,
            strength: 88,
            morale: 78,
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
        Formation {
            id: "reserve-corps".to_string(),
            name: "Reserve Corps".to_string(),
            branch: Branch::Reserve,
            strength: 95,
            morale: 72,
            supply: 75,
            experience: 1,
        },
    ]
}

/// A concrete operation in the theater.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationId {
    IronTide,
    SwiftBridge,
    BrightLance,
    CrimsonWake,
    SableHammer,
    Ashfall,
    Meridian,
}

impl OperationId {
    pub const ALL: [OperationId; 7] = [
        OperationId::IronTide,
        OperationId::SwiftBridge,
        OperationId::BrightLance,
        OperationId::CrimsonWake,
        OperationId::SableHammer,
        OperationId::Ashfall,
        OperationId::Meridian,
    ];

    pub fn kind(self) -> OperationKind {
        match self {
            OperationId::IronTide => OperationKind::Frontlines,
            OperationId::SwiftBridge => OperationKind::Skirmish,
            OperationId::BrightLance => OperationKind::Dogfight,
            OperationId::CrimsonWake => OperationKind::Naval,
            OperationId::SableHammer | OperationId::Ashfall | OperationId::Meridian => {
                OperationKind::Battle
            }
        }
    }

    pub fn node_id(self) -> &'static str {
        match self {
            OperationId::IronTide => "vantage",
            OperationId::SwiftBridge => "orne",
            OperationId::BrightLance => "halcyon",
            OperationId::CrimsonWake => "callas",
            OperationId::SableHammer => "sable",
            OperationId::Ashfall => "vesper",
            OperationId::Meridian => "meridian",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            OperationId::IronTide => "Operation Iron Tide",
            OperationId::SwiftBridge => "Operation Swift Bridge",
            OperationId::BrightLance => "Operation Bright Lance",
            OperationId::CrimsonWake => "Operation Crimson Wake",
            OperationId::SableHammer => "Operation Sable Hammer",
            OperationId::Ashfall => "Operation Ashfall",
            OperationId::Meridian => "Operation Meridian",
        }
    }

    pub fn formation_id(self) -> &'static str {
        match self {
            OperationId::IronTide | OperationId::SableHammer => "old-ironsides",
            OperationId::SwiftBridge => "ridgewalkers",
            OperationId::BrightLance => "nightjars",
            OperationId::CrimsonWake => "salt-watch",
            OperationId::Ashfall => "ridgewalkers",
            OperationId::Meridian => "reserve-corps",
        }
    }

    pub fn code(self) -> u8 {
        match self {
            OperationId::IronTide => 1,
            OperationId::SwiftBridge => 2,
            OperationId::BrightLance => 3,
            OperationId::CrimsonWake => 4,
            OperationId::SableHammer => 5,
            OperationId::Ashfall => 6,
            OperationId::Meridian => 7,
        }
    }
}

/// An operation the player can choose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationOption {
    pub id: OperationId,
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
    Ground(BattleLog),
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
    pub id: OperationId,
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
    pub scenario: CampaignScenario,
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
    #[error("invalid campaign scenario: {0}")]
    Scenario(String),
    #[error("the campaign is already finished")]
    Finished,
    #[error("operation {0:?} is not available")]
    Unavailable(OperationId),
    #[error("no formation is available for this operation")]
    MissingFormation,
    #[error(transparent)]
    Frontline(#[from] FrontlineError),
    #[error(transparent)]
    Dogfight(#[from] DogfightError),
    #[error(transparent)]
    Naval(#[from] NavalError),
    #[error(transparent)]
    Ground(#[from] SimError),
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
    Ground(#[from] ReplayError),
    #[error(transparent)]
    Campaign(#[from] CampaignError),
    #[error("campaign log is inconsistent at turn {0}")]
    Mismatch(u32),
    #[error("campaign log is inconsistent: {0}")]
    Inconsistent(String),
}

impl Campaign {
    /// Start the Halcyon Sweep.
    /// Start with the default Kestrel Coast scenario.
    pub fn new(seed: Seed) -> Self {
        Self::from_scenario(seed, CampaignScenario::default()).expect("default scenario is valid")
    }

    /// Start a war from an explicit scenario.
    pub fn from_scenario(seed: Seed, scenario: CampaignScenario) -> Result<Self, CampaignError> {
        scenario.validate().map_err(CampaignError::Scenario)?;
        Ok(Self {
            name: scenario.name.clone(),
            seed,
            turn: 1,
            max_turns: scenario.max_turns,
            political_will: scenario.starting_will.clamp(0, 100),
            supply: scenario.starting_supply.clamp(0, 100),
            intel: scenario.starting_intel.clamp(0, 100),
            momentum: 0,
            enemy_intent: EnemyIntent::Hold,
            nodes: scenario.nodes.clone(),
            formations: scenario.formations.clone(),
            operations: Vec::new(),
            chronicle: Vec::new(),
            finished: false,
            result: None,
            scenario,
        })
    }

    /// Run the canonical seven-operation demo.
    pub fn demo(seed: Seed) -> Result<Self, CampaignError> {
        let mut campaign = Self::new(seed);
        for id in OperationId::ALL {
            campaign.play_operation(id)?;
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

    pub fn formation_for(&self, id: OperationId) -> Option<&Formation> {
        self.formations
            .iter()
            .find(|formation| formation.id == id.formation_id())
    }

    fn formation_mut(&mut self, id: &str) -> Option<&mut Formation> {
        self.formations
            .iter_mut()
            .find(|formation| formation.id == id)
    }

    /// Every operation, with availability and a reason.
    pub fn options(&self) -> Vec<OperationOption> {
        OperationId::ALL
            .iter()
            .map(|id| {
                let kind = id.kind();
                let node_id = id.node_id();
                let node = self.node(node_id);
                let done = self
                    .operations
                    .iter()
                    .any(|op| op.id == *id && op.aftermath.victory);
                let (available, reason) = if self.finished {
                    (false, "campaign finished".to_string())
                } else if done {
                    (false, "already attempted".to_string())
                } else if node
                    .map(|node| node.control == Control::Player)
                    .unwrap_or(false)
                {
                    (false, "already controlled".to_string())
                } else {
                    match id {
                        OperationId::IronTide => (true, "available".to_string()),
                        OperationId::SwiftBridge => {
                            if self.is_player("vantage") {
                                (true, "available".to_string())
                            } else {
                                (false, "take Vantage Ridge first".to_string())
                            }
                        }
                        OperationId::BrightLance => {
                            if !self.scenario.modules.planes {
                                (false, "planes module is off".to_string())
                            } else if self.is_player("vantage") {
                                (true, "available".to_string())
                            } else {
                                (false, "take Vantage Ridge first".to_string())
                            }
                        }
                        OperationId::CrimsonWake => {
                            if !self.scenario.modules.ships {
                                (false, "ships module is off".to_string())
                            } else if self.is_player("vantage") {
                                (true, "available".to_string())
                            } else {
                                (false, "take Vantage Ridge first".to_string())
                            }
                        }
                        OperationId::SableHammer => {
                            if self.is_player("halcyon") {
                                (true, "available".to_string())
                            } else {
                                (false, "take Halcyon Airbase first".to_string())
                            }
                        }
                        OperationId::Ashfall => {
                            if self.is_player("sable") || self.is_player("callas") {
                                (true, "available".to_string())
                            } else {
                                (false, "take Sable Junction or Port Callas".to_string())
                            }
                        }
                        OperationId::Meridian => {
                            if self.combined_arms_ready() {
                                (true, "combined-arms gate met".to_string())
                            } else {
                                (false, "need Halcyon, Sable, Callas, and Vesper".to_string())
                            }
                        }
                    }
                };
                OperationOption {
                    id: *id,
                    kind,
                    node_id: node_id.to_string(),
                    title: id.title().to_string(),
                    available,
                    reason,
                }
            })
            .collect()
    }

    pub fn is_player(&self, node_id: &str) -> bool {
        self.node(node_id).map(|node| node.control) == Some(Control::Player)
    }

    pub fn combined_arms_ready(&self) -> bool {
        self.is_player("halcyon")
            && self.is_player("sable")
            && self.is_player("callas")
            && self.is_player("vesper")
    }

    /// Scenario values can be edited until the first operation is launched.
    pub fn scenario_locked(&self) -> bool {
        !self.operations.is_empty()
    }

    /// Copy editable state back into the scenario snapshot. No-op once the war
    /// has started, so replay still uses the original scenario.
    pub fn sync_scenario_from_state(&mut self) {
        if self.scenario_locked() {
            return;
        }
        self.scenario.name = self.name.clone();
        self.scenario.max_turns = self.max_turns;
        self.scenario.starting_will = self.political_will;
        self.scenario.starting_supply = self.supply;
        self.scenario.starting_intel = self.intel;
        for state in &self.nodes {
            if let Some(node) = self.scenario.nodes.iter_mut().find(|n| n.id == state.id) {
                node.name = state.name.clone();
                node.control = state.control;
                node.enemy_garrison = state.enemy_garrison;
                node.fortification = state.fortification;
                node.objective = state.objective;
                node.connections = state.connections.clone();
            }
        }
        for state in &self.formations {
            if let Some(formation) = self
                .scenario
                .formations
                .iter_mut()
                .find(|f| f.id == state.id)
            {
                formation.name = state.name.clone();
                formation.strength = state.strength;
                formation.morale = state.morale;
                formation.supply = state.supply;
                formation.experience = state.experience;
            }
        }
    }

    /// Build the front-line input without mutating the campaign.
    pub fn frontline_input(&self) -> Result<FrontlineInput, CampaignError> {
        let formation = self
            .formation_for(OperationId::IronTide)
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
            seed: self.operation_seed(OperationId::IronTide),
        })
    }

    /// Build a dogfight input without mutating the campaign.
    pub fn dogfight_input(&self) -> Result<DogfightInput, CampaignError> {
        let formation = self
            .formation_for(OperationId::BrightLance)
            .ok_or(CampaignError::MissingFormation)?;
        let aircraft = (formation.strength / 10).clamp(4, 10);
        let skill = (4 + formation.experience).min(10);
        let quality = (formation.supply / 16).clamp(2, 10);

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
                aircraft: drumfire_model::AircraftCount::new(6).unwrap(),
                pilot_skill: Rating::new(5).unwrap(),
                aircraft_quality: Rating::new(5).unwrap(),
                weapons: Rating::new(6).unwrap(),
                radar: Rating::new(5).unwrap(),
                support: Rating::new(5).unwrap(),
                cohesion: Morale::new(65).unwrap(),
            },
            config: DogfightConfig::default(),
            seed: self.operation_seed(OperationId::BrightLance),
        })
    }

    /// Build the naval input, including the air-superiority bonus when Halcyon
    /// is controlled.
    pub fn naval_input(&self) -> Result<NavalInput, CampaignError> {
        let formation = self
            .formation_for(OperationId::CrimsonWake)
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
            seed: self.operation_seed(OperationId::CrimsonWake),
        })
    }

    /// Build a ground battle or skirmish input.
    pub fn ground_input(&self, id: OperationId) -> Result<BattleInput, CampaignError> {
        let formation = self
            .formation_for(id)
            .ok_or(CampaignError::MissingFormation)?;
        let node = self
            .node(id.node_id())
            .ok_or(CampaignError::MissingFormation)?;
        let mode = match id.kind() {
            OperationKind::Skirmish => Mode::Skirmish,
            _ => Mode::Battle,
        };
        let force_scale = if id.kind() == OperationKind::Skirmish {
            45
        } else {
            100
        };
        let mut force = (formation.strength as u32 * force_scale).clamp(400, 1_000_000);
        if self.scenario.modules.tanks {
            force = (force + force / 10).min(1_000_000);
        }
        let air = self.air_superiority() >= 60;
        let intel = (5 + u8::from(air)).min(10);
        let attacker = TeamInput {
            name: formation.name.clone(),
            forces: ForceCount::new(force).unwrap(),
            morale: Morale::new(formation.morale).unwrap(),
            tech: Rating::new(6).unwrap(),
            training: Rating::new((4 + formation.experience / 2 + u8::from(air)).min(10)).unwrap(),
            experience: Rating::new((4 + formation.experience).min(10)).unwrap(),
            logistics: Rating::new((formation.supply / 20).clamp(1, 10)).unwrap(),
            intel: Rating::new(intel).unwrap(),
            home: false,
        };
        let defender = TeamInput {
            name: format!("{} Garrison", node.name),
            forces: ForceCount::new(node.enemy_garrison.max(500)).unwrap(),
            morale: Morale::new(65).unwrap(),
            tech: Rating::new(5).unwrap(),
            training: Rating::new(5).unwrap(),
            experience: Rating::new(5).unwrap(),
            logistics: Rating::new(5).unwrap(),
            intel: Rating::new(5).unwrap(),
            home: true,
        };
        Ok(BattleInput {
            mode,
            attacker,
            defender,
            config: BattleConfig {
                max_rounds: 24,
                casualty_rate: 0.18,
                morale_rout_threshold: 10,
                ..BattleConfig::default()
            },
            seed: self.operation_seed(id),
        })
    }

    fn operation_seed(&self, id: OperationId) -> Seed {
        let mut hasher = Sha256::new();
        hasher.update(self.seed.as_bytes());
        hasher.update(self.turn.to_le_bytes());
        hasher.update([id.code()]);
        let digest = hasher.finalize();
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&digest);
        Seed::from_bytes(bytes)
    }

    /// Play one operation and apply its aftermath.
    pub fn play_operation(&mut self, id: OperationId) -> Result<OperationRecord, CampaignError> {
        self.scenario
            .modules
            .validate()
            .map_err(CampaignError::Scenario)?;
        if self.finished {
            return Err(CampaignError::Finished);
        }
        let option = self
            .options()
            .into_iter()
            .find(|option| option.id == id)
            .ok_or(CampaignError::Unavailable(id))?;
        if !option.available {
            return Err(CampaignError::Unavailable(id));
        }

        let intent_note = self.resolve_enemy_intent();

        let record = match id.kind() {
            OperationKind::Frontlines => {
                let input = self.frontline_input()?;
                let log = simulate_frontline(&input)?;
                let aftermath = self.apply_frontline(&log)?;
                OperationRecord {
                    turn: self.turn,
                    id,
                    kind: id.kind(),
                    node_id: id.node_id().to_string(),
                    title: id.title().to_string(),
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
                    id,
                    kind: id.kind(),
                    node_id: id.node_id().to_string(),
                    title: id.title().to_string(),
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
                    id,
                    kind: id.kind(),
                    node_id: id.node_id().to_string(),
                    title: id.title().to_string(),
                    log: OperationLog::Naval(log),
                    aftermath,
                }
            }
            OperationKind::Battle | OperationKind::Skirmish => {
                let input = self.ground_input(id)?;
                let log = simulate(&input)?;
                let aftermath = self.apply_ground(&log, id)?;
                OperationRecord {
                    turn: self.turn,
                    id,
                    kind: id.kind(),
                    node_id: id.node_id().to_string(),
                    title: id.title().to_string(),
                    log: OperationLog::Ground(log),
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
        if let Some(note) = intent_note {
            self.chronicle.push(format!("Enemy intent — {note}"));
        }
        self.turn += 1;
        self.enemy_intent = self.roll_enemy_intent();
        self.check_end();
        Ok(record)
    }

    fn apply_frontline(&mut self, log: &FrontlineLog) -> Result<Aftermath, CampaignError> {
        let formation_id = OperationId::IronTide.formation_id().to_string();
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
        let formation_id = OperationId::BrightLance.formation_id().to_string();
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
        let formation_id = OperationId::CrimsonWake.formation_id().to_string();
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

    fn apply_ground(
        &mut self,
        log: &BattleLog,
        id: OperationId,
    ) -> Result<Aftermath, CampaignError> {
        let formation_id = id.formation_id().to_string();
        let initial = log.input.attacker.forces.get() as f64;
        let final_forces = log.result.final_forces.attacker as f64;
        let loss_pct = (((initial - final_forces) / initial.max(1.0)) * 100.0).round() as i32;
        let victory = log.result.winner == Winner::Attacker;
        let strength_delta = -(loss_pct / 2).min(30);
        let (will_delta, supply_delta, intel_delta, momentum_delta) = match id {
            OperationId::SwiftBridge => (4, 5, 8, 1),
            OperationId::SableHammer => (9, 7, 5, 2),
            OperationId::Ashfall => (12, 4, 4, 2),
            OperationId::Meridian => (20, 6, 8, 3),
            _ => (5, 4, 3, 1),
        };
        let will_delta = if victory { will_delta } else { -10 };
        let supply_delta = if victory { supply_delta } else { -8 };
        let intel_delta = if victory { intel_delta } else { -3 };
        let momentum_delta = if victory { momentum_delta } else { -2 };

        self.apply_formation(&formation_id, strength_delta, victory);
        self.apply_resources(will_delta, supply_delta, intel_delta, momentum_delta);
        let node_id = id.node_id();
        if let Some(node) = self.node_mut(node_id) {
            node.control = if victory {
                Control::Player
            } else {
                Control::Enemy
            };
        }

        Ok(Aftermath {
            victory,
            summary: format!(
                "{} · {}% losses · {}",
                if victory {
                    "objective taken"
                } else {
                    "assault repulsed"
                },
                loss_pct,
                match id {
                    OperationId::SwiftBridge => "the bridge is open",
                    OperationId::SableHammer => "the rail hub is ours",
                    OperationId::Ashfall => "the works are silent",
                    OperationId::Meridian => "the capital is taken",
                    _ => "the line moves",
                }
            ),
            formation_id,
            strength_delta,
            will_delta,
            supply_delta,
            intel_delta,
            momentum_delta,
            control: Some(node_id.to_string()),
        })
    }

    fn apply_formation(&mut self, id: &str, strength_delta: i32, victory: bool) {
        let strength_delta = if self.scenario.modules.manpower {
            strength_delta
        } else {
            0
        };
        let supply_cost = if self.scenario.modules.logistics {
            5
        } else {
            0
        };
        if let Some(formation) = self.formation_mut(id) {
            formation.strength = (formation.strength as i32 + strength_delta).clamp(10, 100) as u8;
            let morale_delta = if victory { 3 } else { -8 };
            formation.morale = (formation.morale as i32 + morale_delta).clamp(0, 100) as u8;
            formation.supply = (formation.supply as i32 - supply_cost).clamp(0, 100) as u8;
        }
    }

    fn apply_resources(&mut self, will: i32, supply: i32, intel: i32, momentum: i32) {
        let supply = if self.scenario.modules.logistics {
            supply
        } else {
            0
        };
        self.political_will = (self.political_will + will).clamp(0, 100);
        self.supply = (self.supply + supply).clamp(0, 100);
        self.intel = (self.intel + intel).clamp(0, 100);
        self.momentum = (self.momentum + momentum).clamp(-5, 5);
    }

    fn resolve_enemy_intent(&mut self) -> Option<String> {
        match self.enemy_intent {
            EnemyIntent::Hold => None,
            EnemyIntent::Reinforce => {
                let target = ["meridian", "sable", "vesper", "callas"]
                    .into_iter()
                    .find(|id| !self.is_player(id));
                if let Some(id) = target {
                    if let Some(node) = self.node_mut(id) {
                        node.enemy_garrison = node.enemy_garrison.saturating_add(1500);
                        return Some(format!("reinforces {}", node.name));
                    }
                }
                None
            }
            EnemyIntent::Interdict => {
                self.apply_resources(0, -6, -4, 0);
                Some("interdicts the supply road".to_string())
            }
            EnemyIntent::Counterattack => {
                self.apply_resources(-5, -4, 0, -1);
                Some("counterattacks the forward line".to_string())
            }
            EnemyIntent::Withdraw => {
                let target = ["vesper", "sable", "callas", "orne"]
                    .into_iter()
                    .find(|id| !self.is_player(id));
                if let Some(id) = target {
                    if let Some(node) = self.node_mut(id) {
                        node.enemy_garrison = (node.enemy_garrison / 2).max(1000);
                        node.control = Control::Contested;
                        return Some(format!("withdraws from {}", node.name));
                    }
                }
                None
            }
        }
    }

    fn roll_enemy_intent(&self) -> EnemyIntent {
        let mut hasher = Sha256::new();
        hasher.update(self.seed.as_bytes());
        hasher.update(self.turn.to_le_bytes());
        hasher.update([77u8]);
        let digest = hasher.finalize();
        let roll = digest[0] % 100;
        match roll {
            0..=34 => EnemyIntent::Hold,
            35..=59 => EnemyIntent::Reinforce,
            60..=74 => EnemyIntent::Interdict,
            75..=89 => EnemyIntent::Counterattack,
            _ => EnemyIntent::Withdraw,
        }
    }

    fn check_end(&mut self) {
        if self.political_will <= 0 {
            self.finish(CampaignResult::Defeat);
            return;
        }
        if self.is_player("meridian") {
            self.finish(CampaignResult::Victory);
            return;
        }
        if self.turn > self.max_turns {
            self.finish(CampaignResult::Defeat);
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
            OperationLog::Ground(log) => {
                replay(log)?;
            }
        }
    }

    let mut rebuilt = Campaign::from_scenario(campaign.seed, campaign.scenario.clone())?;
    for operation in &campaign.operations {
        let replay = rebuilt.play_operation(operation.id)?;
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
        Seed::from_u64(5)
    }

    #[test]
    fn campaign_starts_with_seven_options() {
        let campaign = Campaign::new(seed());
        let options = campaign.options();
        assert_eq!(options.len(), 7);
        assert!(options[0].available);
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
        assert_eq!(campaign.operations.len(), 7);
        assert!(campaign.finished);
        assert_eq!(campaign.result, Some(CampaignResult::Victory));
        replay_campaign(&campaign).unwrap();
    }

    #[test]
    fn operation_cannot_be_repeated() {
        let mut campaign = Campaign::new(seed());
        campaign.play_operation(OperationId::IronTide).unwrap();
        campaign.play_operation(OperationId::BrightLance).unwrap();
        assert!(matches!(
            campaign.play_operation(OperationId::BrightLance),
            Err(CampaignError::Unavailable(OperationId::BrightLance))
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
    fn combined_arms_gate_requires_four_nodes() {
        let mut campaign = Campaign::new(seed());
        assert!(!campaign.combined_arms_ready());
        for node in ["halcyon", "sable", "callas", "vesper"] {
            campaign.node_mut(node).unwrap().control = Control::Player;
        }
        assert!(campaign.combined_arms_ready());
    }

    #[test]
    fn committed_campaign_log_replays() {
        let campaign: Campaign =
            serde_json::from_str(include_str!("../../../fixtures/campaign.log.json"))
                .expect("committed campaign log must be valid");
        assert!(!campaign.operations.is_empty());
        replay_campaign(&campaign).unwrap();
    }

    #[test]
    fn custom_scenario_names_and_module_gating_work() {
        let mut scenario = CampaignScenario {
            name: "Custom War".to_string(),
            player_name: "Aurelia".to_string(),
            enemy_name: "Vermillion".to_string(),
            ..CampaignScenario::default()
        };
        scenario.nodes[0].name = "Alder Gate".to_string();
        scenario.modules.planes = false;
        let campaign = Campaign::from_scenario(seed(), scenario).unwrap();
        assert_eq!(campaign.name, "Custom War");
        assert_eq!(campaign.nodes[0].name, "Alder Gate");
        let bright_lance = campaign
            .options()
            .into_iter()
            .find(|option| option.id == OperationId::BrightLance)
            .unwrap();
        assert!(!bright_lance.available);
        assert_eq!(bright_lance.reason, "planes module is off");
    }

    #[test]
    fn module_dependencies_are_validated() {
        let modules = CampaignModules {
            manpower: false,
            reserve_manpower: true,
            ..CampaignModules::default()
        };
        assert!(modules.validate().is_err());
        assert_eq!(
            CampaignModules::preset(ModulePreset::CombinedArms).preset_name(),
            "Combined Arms"
        );
    }

    #[test]
    fn invalid_scenario_values_are_rejected() {
        let mut scenario = CampaignScenario::default();
        scenario.formations[0].morale = 101;
        assert!(Campaign::from_scenario(seed(), scenario).is_err());

        let mut scenario = CampaignScenario::default();
        scenario.nodes[0].fortification = 0;
        assert!(Campaign::from_scenario(seed(), scenario).is_err());

        let mut scenario = CampaignScenario::default();
        scenario.nodes[1].id = scenario.nodes[0].id.clone();
        assert!(Campaign::from_scenario(seed(), scenario).is_err());
    }
}
