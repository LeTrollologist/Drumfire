//! Referee-mode conflict simulation.
//!
//! This is deliberately not a game loop: no player side, no victory screen, no
//! fixed operation cards. A referee supplies a scenario and order sheets, gets a
//! multi-day report, and uses it as a shared record for roleplay.

use std::collections::{HashMap, HashSet, VecDeque};

use drumfire_core::{simulate, BattleLog, SimError};
use drumfire_model::{
    BattleConfig, BattleInput, ForceCount, Mode, Morale, Rating, Seed, TeamInput,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{Branch, CampaignModules};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefereeScenario {
    pub name: String,
    pub start_date: String,
    pub max_days: u32,
    #[serde(default)]
    pub modules: CampaignModules,
    pub sides: Vec<Side>,
    pub zones: Vec<Zone>,
    pub units: Vec<Unit>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Side {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub nation: Option<String>,
    pub capital_zone: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Zone {
    pub id: String,
    pub name: String,
    pub kind: ZoneKind,
    pub owner: String,
    pub connections: Vec<String>,
    pub terrain: Terrain,
    pub fortification: u8,
    pub supply_hub: bool,
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoneKind {
    Capital,
    City,
    Port,
    Airbase,
    Rail,
    Countryside,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Terrain {
    Plains,
    Hills,
    Urban,
    River,
    Coast,
}

impl Terrain {
    fn defender_multiplier(self) -> f64 {
        match self {
            Terrain::Plains => 1.0,
            Terrain::Hills => 1.15,
            Terrain::Urban => 1.25,
            Terrain::River => 1.10,
            Terrain::Coast => 1.05,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Unit {
    pub id: String,
    pub name: String,
    pub side: String,
    pub branch: Branch,
    pub zone: String,
    pub strength: u32,
    pub morale: u8,
    pub supply: u8,
    pub experience: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderSheet {
    pub side: String,
    pub orders: Vec<Order>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Order {
    pub day: u32,
    pub unit: String,
    pub action: OrderAction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OrderAction {
    Move { to: String },
    Attack { target: String },
    Defend,
    Recon,
    Resupply,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ZoneState {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub fortification: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnitState {
    pub id: String,
    pub name: String,
    pub side: String,
    pub branch: Branch,
    pub zone: String,
    pub strength: u32,
    pub morale: u8,
    pub supply: u8,
    pub experience: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Casualty {
    pub side: String,
    pub unit: String,
    pub losses: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contact {
    pub side: String,
    pub unit: String,
    pub zone: String,
    pub estimated_strength: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DayReport {
    pub day: u32,
    pub events: Vec<String>,
    pub casualties: Vec<Casualty>,
    pub control_changes: Vec<String>,
    pub contacts: Vec<Contact>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConflictLog {
    pub scenario: RefereeScenario,
    pub days: Vec<DayReport>,
    pub final_zones: Vec<ZoneState>,
    pub final_units: Vec<UnitState>,
    pub summary: String,
}

#[derive(Debug, Error)]
pub enum RefereeError {
    #[error("invalid referee scenario: {0}")]
    Scenario(String),
    #[error("invalid order sheet: {0}")]
    Orders(String),
    #[error(transparent)]
    Simulation(#[from] SimError),
}

#[derive(Debug, Clone)]
pub struct RefereeSimulation {
    scenario: RefereeScenario,
    zones: Vec<ZoneState>,
    units: Vec<UnitState>,
    day: u32,
}

impl RefereeSimulation {
    pub fn new(scenario: RefereeScenario) -> Result<Self, RefereeError> {
        scenario.validate()?;
        Ok(Self {
            zones: scenario
                .zones
                .iter()
                .map(|zone| ZoneState {
                    id: zone.id.clone(),
                    name: zone.name.clone(),
                    owner: zone.owner.clone(),
                    fortification: zone.fortification,
                })
                .collect(),
            units: scenario
                .units
                .iter()
                .map(|unit| UnitState {
                    id: unit.id.clone(),
                    name: unit.name.clone(),
                    side: unit.side.clone(),
                    branch: unit.branch,
                    zone: unit.zone.clone(),
                    strength: unit.strength,
                    morale: unit.morale,
                    supply: unit.supply,
                    experience: unit.experience,
                })
                .collect(),
            scenario,
            day: 1,
        })
    }

    pub fn scenario(&self) -> &RefereeScenario {
        &self.scenario
    }

    pub fn day(&self) -> u32 {
        self.day
    }

    pub fn zones(&self) -> &[ZoneState] {
        &self.zones
    }

    pub fn units(&self) -> &[UnitState] {
        &self.units
    }

    pub fn log(&self, days: &[DayReport]) -> ConflictLog {
        ConflictLog {
            scenario: self.scenario.clone(),
            days: days.to_vec(),
            final_zones: self.zones.clone(),
            final_units: self.units.clone(),
            summary: self.summary(),
        }
    }

    /// Resolve the current day and advance the simulation clock.
    pub fn advance_day(&mut self, sheets: &[OrderSheet]) -> Result<DayReport, RefereeError> {
        let day = self.day.min(self.scenario.max_days);
        let report = self.resolve_day(day, sheets)?;
        if self.day < self.scenario.max_days {
            self.day += 1;
        }
        Ok(report)
    }

    fn resolve(mut self, sheets: &[OrderSheet]) -> Result<ConflictLog, RefereeError> {
        let mut days = Vec::new();
        for day in 1..=self.scenario.max_days {
            self.day = day;
            days.push(self.resolve_day(day, sheets)?);
        }
        let summary = self.summary();
        Ok(ConflictLog {
            scenario: self.scenario,
            days,
            final_zones: self.zones,
            final_units: self.units,
            summary,
        })
    }

    fn resolve_day(&mut self, day: u32, sheets: &[OrderSheet]) -> Result<DayReport, RefereeError> {
        let orders = collect_orders(day, sheets)?;
        let mut events = Vec::new();
        let mut casualties = Vec::new();
        let mut control_changes = Vec::new();
        let mut notes = Vec::new();
        let mut contacts = Vec::new();

        for unit in self.scenario.units.clone() {
            let Some((order_side, action)) = orders.get(&unit.id) else {
                continue;
            };
            if *order_side != unit.side {
                return Err(RefereeError::Orders(format!(
                    "order for {} names side {}, but the unit belongs to {}",
                    unit.id, order_side, unit.side
                )));
            }
            let Some(index) = self.units.iter().position(|state| state.id == unit.id) else {
                continue;
            };
            if self.units[index].strength == 0 {
                continue;
            }
            let side = unit.side.as_str();
            match action {
                OrderAction::Recon => {
                    let zone = self.units[index].zone.clone();
                    let found = self.detect_adjacent(&zone, side);
                    for contact in found {
                        contacts.push(contact);
                    }
                    self.units[index].supply = self.units[index].supply.saturating_add(5);
                    events.push(format!(
                        "{} reconnoiters from {}",
                        self.units[index].name,
                        self.zone_name(&zone)
                    ));
                }
                OrderAction::Resupply => {
                    self.units[index].supply = self.units[index].supply.saturating_add(20).min(100);
                    events.push(format!("{} resupplies", self.units[index].name));
                }
                OrderAction::Defend => {
                    events.push(format!("{} holds position", self.units[index].name));
                }
                OrderAction::Move { to } | OrderAction::Attack { target: to } => {
                    let from = self.units[index].zone.clone();
                    if from == *to {
                        events.push(format!(
                            "{} is already at {}",
                            self.units[index].name,
                            self.zone_name(to)
                        ));
                        continue;
                    }
                    if !self.are_connected(&from, to) {
                        return Err(RefereeError::Orders(format!(
                            "{} is not connected to {}",
                            from, to
                        )));
                    }
                    if self.has_enemy_units(to, side) || self.zone_owner(to) != side {
                        self.engage(
                            index,
                            to,
                            day,
                            &mut events,
                            &mut casualties,
                            &mut control_changes,
                        )?;
                    } else {
                        self.units[index].zone = to.clone();
                        events.push(format!(
                            "{} advances from {} to {}",
                            self.units[index].name,
                            self.zone_name(&from),
                            self.zone_name(to)
                        ));
                    }
                }
            }
        }

        if self.scenario.modules.logistics {
            self.apply_supply(&mut notes);
        }
        self.apply_morale();

        Ok(DayReport {
            day,
            events,
            casualties,
            control_changes,
            contacts,
            notes,
        })
    }

    fn engage(
        &mut self,
        attacker_index: usize,
        target_zone: &str,
        _day: u32,
        events: &mut Vec<String>,
        casualties: &mut Vec<Casualty>,
        control_changes: &mut Vec<String>,
    ) -> Result<(), RefereeError> {
        let attacker = self.units[attacker_index].clone();
        let defender_indices: Vec<usize> = self
            .units
            .iter()
            .enumerate()
            .filter(|(_, unit)| {
                unit.zone == target_zone
                    && unit.side != attacker.side
                    && unit.strength > 0
                    && unit.branch == Branch::Ground
            })
            .map(|(index, _)| index)
            .collect();
        if defender_indices.is_empty() {
            self.set_owner(target_zone, &attacker.side);
            self.units[attacker_index].zone = target_zone.to_string();
            let name = self.zone_name(target_zone);
            control_changes.push(format!(
                "{} occupied {}",
                self.side_name(&attacker.side),
                name
            ));
            events.push(format!("{} occupies {}", attacker.name, name));
            return Ok(());
        }

        let zone = self
            .scenario
            .zones
            .iter()
            .find(|zone| zone.id == target_zone)
            .cloned()
            .ok_or_else(|| RefereeError::Orders(format!("unknown zone {target_zone}")))?;
        let defender_strength: u32 = defender_indices
            .iter()
            .map(|index| self.units[*index].strength)
            .sum();
        let support = self.support_strength(&attacker.side, target_zone);
        let attacker_strength = (attacker.strength as f64 + support as f64).round() as u32;
        let defender_forces = ((defender_strength as f64)
            * zone.terrain.defender_multiplier()
            * (1.0 + zone.fortification as f64 * 0.03))
            .round() as u32;
        let attacker_force = attacker_strength.max(1);
        let input = BattleInput {
            mode: Mode::Battle,
            attacker: team_from_unit(&attacker, attacker_force, false),
            defender: team_from_defenders(
                &self.units,
                &defender_indices,
                defender_forces.max(1),
                &zone,
            ),
            config: BattleConfig {
                max_rounds: 16,
                casualty_rate: 0.16,
                morale_rout_threshold: 8,
                ..BattleConfig::default()
            },
            seed: seed_for(&self.scenario.name, attacker_index, target_zone),
        };
        let log: BattleLog = simulate(&input)?;
        let a_initial = input.attacker.forces.get();
        let d_initial = input.defender.forces.get();
        let a_loss = a_initial.saturating_sub(log.result.final_forces.attacker);
        let d_loss = d_initial.saturating_sub(log.result.final_forces.defender);

        let a_share = if a_initial == 0 {
            0
        } else {
            ((a_loss as f64 / a_initial as f64) * attacker.strength as f64).round() as u32
        };
        self.units[attacker_index].strength = self.units[attacker_index]
            .strength
            .saturating_sub(a_share.max(1));
        self.units[attacker_index].supply = self.units[attacker_index].supply.saturating_sub(12);
        casualties.push(Casualty {
            side: attacker.side.clone(),
            unit: attacker.name.clone(),
            losses: a_share,
        });

        let d_total: u32 = defender_indices
            .iter()
            .map(|index| self.units[*index].strength)
            .sum();
        for index in &defender_indices {
            let share = if d_total == 0 {
                0
            } else {
                ((d_loss as f64 / d_initial.max(1) as f64) * self.units[*index].strength as f64)
                    .round() as u32
            };
            self.units[*index].strength = self.units[*index].strength.saturating_sub(share.max(1));
            self.units[*index].supply = self.units[*index].supply.saturating_sub(10);
            casualties.push(Casualty {
                side: self.units[*index].side.clone(),
                unit: self.units[*index].name.clone(),
                losses: share,
            });
        }

        if log.result.winner == drumfire_core::Winner::Attacker {
            self.set_owner(target_zone, &attacker.side);
            self.units[attacker_index].zone = target_zone.to_string();
            control_changes.push(format!(
                "{} captured {}",
                self.side_name(&attacker.side),
                self.zone_name(target_zone)
            ));
            events.push(format!(
                "{} takes {} after {} rounds",
                attacker.name,
                zone.name,
                log.rounds.len()
            ));
        } else {
            events.push(format!(
                "{} holds {} after {} rounds",
                self.side_name(&self.zone_owner(target_zone)),
                zone.name,
                log.rounds.len()
            ));
        }
        Ok(())
    }

    fn support_strength(&self, side: &str, target: &str) -> u32 {
        self.units
            .iter()
            .filter(|unit| unit.side == side && unit.strength > 0)
            .filter(|unit| unit.zone == target || self.are_connected(&unit.zone, target))
            .filter(|unit| unit.branch != Branch::Ground)
            .map(|unit| match unit.branch {
                Branch::Air => unit.strength / 5,
                Branch::Naval => unit.strength / 8,
                _ => 0,
            })
            .sum()
    }

    fn detect_adjacent(&self, zone: &str, side: &str) -> Vec<Contact> {
        let mut contacts = Vec::new();
        for unit in &self.units {
            if unit.side == side || unit.strength == 0 {
                continue;
            }
            if unit.zone == zone || self.are_connected(zone, &unit.zone) {
                contacts.push(Contact {
                    side: side.to_string(),
                    unit: unit.name.clone(),
                    zone: unit.zone.clone(),
                    estimated_strength: estimate(unit.strength),
                });
            }
        }
        contacts
    }

    fn apply_supply(&mut self, notes: &mut Vec<String>) {
        for side in &self.scenario.sides {
            let mut reachable = HashSet::new();
            let mut queue = VecDeque::new();
            for zone in &self.zones {
                if zone.owner == side.id
                    && self
                        .scenario
                        .zones
                        .iter()
                        .any(|z| z.id == zone.id && z.supply_hub)
                {
                    reachable.insert(zone.id.clone());
                    queue.push_back(zone.id.clone());
                }
            }
            while let Some(zone) = queue.pop_front() {
                if let Some(def) = self.scenario.zones.iter().find(|z| z.id == zone) {
                    for connection in &def.connections {
                        if self.zone_owner(connection) == side.id
                            && reachable.insert(connection.clone())
                        {
                            queue.push_back(connection.clone());
                        }
                    }
                }
            }
            for unit in self
                .units
                .iter_mut()
                .filter(|unit| unit.side == side.id && unit.strength > 0)
            {
                if reachable.contains(&unit.zone) {
                    unit.supply = unit.supply.saturating_add(8).min(100);
                } else {
                    unit.supply = unit.supply.saturating_sub(14);
                    if unit.supply == 0 {
                        unit.morale = unit.morale.saturating_sub(4);
                        unit.strength = (unit.strength as f64 * 0.98) as u32;
                        notes.push(format!("{} is out of supply", unit.name));
                    }
                }
            }
        }
    }

    fn apply_morale(&mut self) {
        for unit in &mut self.units {
            if unit.strength == 0 {
                unit.morale = 0;
            } else if unit.supply >= 50 {
                unit.morale = unit.morale.saturating_add(1).min(100);
            }
        }
    }

    fn summary(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!("Conflict: {}", self.scenario.name));
        for side in &self.scenario.sides {
            let zones = self
                .zones
                .iter()
                .filter(|zone| zone.owner == side.id)
                .count();
            let units: Vec<String> = self
                .units
                .iter()
                .filter(|unit| unit.side == side.id)
                .map(|unit| format!("{} ({})", unit.name, unit.strength))
                .collect();
            lines.push(format!(
                "{}: {} zones; units {}",
                side.name,
                zones,
                units.join(", ")
            ));
        }
        lines.join("\n")
    }

    fn zone_owner(&self, zone: &str) -> String {
        self.zones
            .iter()
            .find(|state| state.id == zone)
            .map(|state| state.owner.clone())
            .unwrap_or_else(|| "neutral".to_string())
    }

    fn zone_name(&self, zone: &str) -> String {
        self.zones
            .iter()
            .find(|state| state.id == zone)
            .map(|state| state.name.clone())
            .unwrap_or_else(|| zone.to_string())
    }

    fn side_name(&self, side: &str) -> String {
        self.scenario
            .sides
            .iter()
            .find(|candidate| candidate.id == side)
            .map(|candidate| candidate.name.clone())
            .unwrap_or_else(|| side.to_string())
    }

    fn set_owner(&mut self, zone: &str, owner: &str) {
        if let Some(state) = self.zones.iter_mut().find(|state| state.id == zone) {
            state.owner = owner.to_string();
        }
    }

    fn are_connected(&self, a: &str, b: &str) -> bool {
        self.scenario
            .zones
            .iter()
            .find(|zone| zone.id == a)
            .map(|zone| zone.connections.iter().any(|id| id == b))
            .unwrap_or(false)
    }

    fn has_enemy_units(&self, zone: &str, side: &str) -> bool {
        self.units
            .iter()
            .any(|unit| unit.zone == zone && unit.side != side && unit.strength > 0)
    }
}

fn collect_orders(
    day: u32,
    sheets: &[OrderSheet],
) -> Result<HashMap<String, (String, OrderAction)>, RefereeError> {
    let mut output: HashMap<String, (String, OrderAction)> = HashMap::new();
    for sheet in sheets {
        for order in sheet.orders.iter().filter(|order| order.day == day) {
            if output
                .insert(
                    order.unit.clone(),
                    (sheet.side.clone(), order.action.clone()),
                )
                .is_some()
            {
                return Err(RefereeError::Orders(format!(
                    "duplicate order for {} on day {day}",
                    order.unit
                )));
            }
        }
    }
    Ok(output)
}

fn team_from_unit(unit: &UnitState, forces: u32, home: bool) -> TeamInput {
    TeamInput {
        name: unit.name.clone(),
        forces: ForceCount::new(forces.max(1)).unwrap(),
        morale: Morale::new(unit.morale).unwrap(),
        tech: Rating::new(6).unwrap(),
        training: Rating::new((4 + unit.experience / 2).min(10)).unwrap(),
        experience: Rating::new((4 + unit.experience).min(10)).unwrap(),
        logistics: Rating::new((unit.supply / 20).clamp(1, 10)).unwrap(),
        intel: Rating::new(5).unwrap(),
        home,
    }
}

fn team_from_defenders(
    units: &[UnitState],
    indices: &[usize],
    forces: u32,
    zone: &Zone,
) -> TeamInput {
    let morale = indices
        .iter()
        .map(|index| units[*index].morale as u32)
        .sum::<u32>()
        / indices.len().max(1) as u32;
    let supply = indices
        .iter()
        .map(|index| units[*index].supply as u32)
        .sum::<u32>()
        / indices.len().max(1) as u32;
    TeamInput {
        name: format!("{} Garrison", zone.name),
        forces: ForceCount::new(forces.max(1)).unwrap(),
        morale: Morale::new(morale.min(100) as u8).unwrap(),
        tech: Rating::new(6).unwrap(),
        training: Rating::new(5).unwrap(),
        experience: Rating::new(5).unwrap(),
        logistics: Rating::new((supply / 20).clamp(1, 10) as u8).unwrap(),
        intel: Rating::new(5).unwrap(),
        home: true,
    }
}

fn seed_for(name: &str, index: usize, target: &str) -> Seed {
    let mut bytes = [0u8; 32];
    let mut hasher = Sha256::new();
    hasher.update(name.as_bytes());
    hasher.update((index as u64).to_le_bytes());
    hasher.update(target.as_bytes());
    let digest = hasher.finalize();
    bytes.copy_from_slice(&digest);
    Seed::from_bytes(bytes)
}

fn estimate(strength: u32) -> String {
    match strength {
        0 => "destroyed".to_string(),
        1..=3_000 => "under 3,000".to_string(),
        3_001..=7_000 => "3,000–7,000".to_string(),
        7_001..=12_000 => "7,000–12,000".to_string(),
        _ => "over 12,000".to_string(),
    }
}

impl RefereeScenario {
    pub fn validate(&self) -> Result<(), RefereeError> {
        if self.name.trim().is_empty() {
            return Err(RefereeError::Scenario("scenario name is empty".to_string()));
        }
        if !(1..=365).contains(&self.max_days) {
            return Err(RefereeError::Scenario(
                "max_days must be between 1 and 365".to_string(),
            ));
        }
        self.modules.validate().map_err(RefereeError::Scenario)?;
        if self.sides.len() < 2 {
            return Err(RefereeError::Scenario(
                "a conflict needs at least two sides".to_string(),
            ));
        }
        let mut side_ids = HashSet::new();
        for side in &self.sides {
            if side.id.trim().is_empty() || side.name.trim().is_empty() {
                return Err(RefereeError::Scenario(
                    "every side needs an id and a name".to_string(),
                ));
            }
            if side.id == "neutral" {
                return Err(RefereeError::Scenario(
                    "side id 'neutral' is reserved".to_string(),
                ));
            }
            if !side_ids.insert(side.id.as_str()) {
                return Err(RefereeError::Scenario(format!(
                    "duplicate side id {}",
                    side.id
                )));
            }
        }
        let zone_ids: HashSet<&str> = self.zones.iter().map(|zone| zone.id.as_str()).collect();
        if self.zones.len() < 2 {
            return Err(RefereeError::Scenario(
                "a conflict needs at least two zones".to_string(),
            ));
        }
        if zone_ids.len() != self.zones.len() {
            return Err(RefereeError::Scenario(
                "zone ids must be unique".to_string(),
            ));
        }
        for zone in &self.zones {
            if zone.id.trim().is_empty() || zone.name.trim().is_empty() {
                return Err(RefereeError::Scenario(
                    "every zone needs an id and a name".to_string(),
                ));
            }
            if !(1..=10).contains(&zone.fortification) {
                return Err(RefereeError::Scenario(format!(
                    "zone {} fortification must be between 1 and 10",
                    zone.id
                )));
            }
            if !zone.x.is_finite()
                || !zone.y.is_finite()
                || !(0.0..=1.0).contains(&zone.x)
                || !(0.0..=1.0).contains(&zone.y)
            {
                return Err(RefereeError::Scenario(format!(
                    "zone {} coordinates must be finite and between 0 and 1",
                    zone.id
                )));
            }
            if !side_ids.contains(zone.owner.as_str()) && zone.owner != "neutral" {
                return Err(RefereeError::Scenario(format!(
                    "zone {} has unknown owner {}",
                    zone.id, zone.owner
                )));
            }
            for connection in &zone.connections {
                if connection == &zone.id {
                    return Err(RefereeError::Scenario(format!(
                        "zone {} connects to itself",
                        zone.id
                    )));
                }
                if !zone_ids.contains(connection.as_str()) {
                    return Err(RefereeError::Scenario(format!(
                        "zone {} connects to unknown zone {}",
                        zone.id, connection
                    )));
                }
            }
        }
        for side in &self.sides {
            if !zone_ids.contains(side.capital_zone.as_str()) {
                return Err(RefereeError::Scenario(format!(
                    "side {} capital zone {} does not exist",
                    side.id, side.capital_zone
                )));
            }
        }
        let mut unit_ids = HashSet::new();
        for unit in &self.units {
            if unit.id.trim().is_empty() || unit.name.trim().is_empty() {
                return Err(RefereeError::Scenario(
                    "every unit needs an id and a name".to_string(),
                ));
            }
            if !unit_ids.insert(unit.id.as_str()) {
                return Err(RefereeError::Scenario(format!(
                    "duplicate unit id {}",
                    unit.id
                )));
            }
            if !side_ids.contains(unit.side.as_str()) {
                return Err(RefereeError::Scenario(format!(
                    "unit {} has unknown side {}",
                    unit.id, unit.side
                )));
            }
            if !zone_ids.contains(unit.zone.as_str()) {
                return Err(RefereeError::Scenario(format!(
                    "unit {} starts in unknown zone {}",
                    unit.id, unit.zone
                )));
            }
            if unit.strength == 0 {
                return Err(RefereeError::Scenario(format!(
                    "unit {} strength must be at least 1",
                    unit.id
                )));
            }
            if unit.morale > 100 || unit.supply > 100 || unit.experience > 10 {
                return Err(RefereeError::Scenario(format!(
                    "unit {} morale/supply or experience is out of range",
                    unit.id
                )));
            }
        }
        Ok(())
    }

    pub fn demo() -> Self {
        Self {
            name: "The Alder Corridor".to_string(),
            start_date: "2026-11-10".to_string(),
            max_days: 5,
            modules: CampaignModules::default(),
            sides: vec![
                Side {
                    id: "aurelia".to_string(),
                    name: "Aurelia".to_string(),
                    nation: Some("Aurelia".to_string()),
                    capital_zone: "aurelia_capital".to_string(),
                },
                Side {
                    id: "vermillion".to_string(),
                    name: "Vermillion".to_string(),
                    nation: Some("Vermillion".to_string()),
                    capital_zone: "vermillion_capital".to_string(),
                },
            ],
            zones: vec![
                Zone {
                    id: "aurelia_capital".to_string(),
                    name: "Aurelia Capital".to_string(),
                    kind: ZoneKind::Capital,
                    owner: "aurelia".to_string(),
                    connections: vec!["alder_gate".to_string()],
                    terrain: Terrain::Urban,
                    fortification: 7,
                    supply_hub: true,
                    x: 0.12,
                    y: 0.55,
                },
                Zone {
                    id: "alder_gate".to_string(),
                    name: "Alder Gate".to_string(),
                    kind: ZoneKind::City,
                    owner: "aurelia".to_string(),
                    connections: vec![
                        "aurelia_capital".to_string(),
                        "rail_junction".to_string(),
                        "airbase".to_string(),
                    ],
                    terrain: Terrain::Plains,
                    fortification: 4,
                    supply_hub: false,
                    x: 0.32,
                    y: 0.45,
                },
                Zone {
                    id: "airbase".to_string(),
                    name: "Kestrel Airbase".to_string(),
                    kind: ZoneKind::Airbase,
                    owner: "neutral".to_string(),
                    connections: vec!["alder_gate".to_string(), "rail_junction".to_string()],
                    terrain: Terrain::Plains,
                    fortification: 3,
                    supply_hub: false,
                    x: 0.48,
                    y: 0.2,
                },
                Zone {
                    id: "rail_junction".to_string(),
                    name: "Rail Junction".to_string(),
                    kind: ZoneKind::Rail,
                    owner: "neutral".to_string(),
                    connections: vec![
                        "alder_gate".to_string(),
                        "airbase".to_string(),
                        "vesper".to_string(),
                        "coast".to_string(),
                    ],
                    terrain: Terrain::Plains,
                    fortification: 5,
                    supply_hub: true,
                    x: 0.5,
                    y: 0.55,
                },
                Zone {
                    id: "coast".to_string(),
                    name: "Callas Coast".to_string(),
                    kind: ZoneKind::Port,
                    owner: "neutral".to_string(),
                    connections: vec!["rail_junction".to_string(), "vesper".to_string()],
                    terrain: Terrain::Coast,
                    fortification: 3,
                    supply_hub: true,
                    x: 0.5,
                    y: 0.82,
                },
                Zone {
                    id: "vesper".to_string(),
                    name: "Vesper".to_string(),
                    kind: ZoneKind::City,
                    owner: "vermillion".to_string(),
                    connections: vec![
                        "rail_junction".to_string(),
                        "coast".to_string(),
                        "vermillion_capital".to_string(),
                    ],
                    terrain: Terrain::Urban,
                    fortification: 6,
                    supply_hub: false,
                    x: 0.72,
                    y: 0.6,
                },
                Zone {
                    id: "vermillion_capital".to_string(),
                    name: "Vermillion Capital".to_string(),
                    kind: ZoneKind::Capital,
                    owner: "vermillion".to_string(),
                    connections: vec!["vesper".to_string()],
                    terrain: Terrain::Hills,
                    fortification: 7,
                    supply_hub: true,
                    x: 0.9,
                    y: 0.45,
                },
            ],
            units: vec![
                Unit {
                    id: "aurelia_1".to_string(),
                    name: "Aurelian 1st Corps".to_string(),
                    side: "aurelia".to_string(),
                    branch: Branch::Ground,
                    zone: "alder_gate".to_string(),
                    strength: 12_000,
                    morale: 76,
                    supply: 82,
                    experience: 3,
                },
                Unit {
                    id: "aurelia_2".to_string(),
                    name: "Aurelian 2nd Corps".to_string(),
                    side: "aurelia".to_string(),
                    branch: Branch::Ground,
                    zone: "aurelia_capital".to_string(),
                    strength: 9_000,
                    morale: 72,
                    supply: 86,
                    experience: 2,
                },
                Unit {
                    id: "aurelia_air".to_string(),
                    name: "Aurelian 1st Air Wing".to_string(),
                    side: "aurelia".to_string(),
                    branch: Branch::Air,
                    zone: "aurelia_capital".to_string(),
                    strength: 2_400,
                    morale: 80,
                    supply: 88,
                    experience: 3,
                },
                Unit {
                    id: "vermillion_7".to_string(),
                    name: "Vermillion 7th Corps".to_string(),
                    side: "vermillion".to_string(),
                    branch: Branch::Ground,
                    zone: "vesper".to_string(),
                    strength: 11_000,
                    morale: 74,
                    supply: 80,
                    experience: 3,
                },
                Unit {
                    id: "vermillion_8".to_string(),
                    name: "Vermillion 8th Corps".to_string(),
                    side: "vermillion".to_string(),
                    branch: Branch::Ground,
                    zone: "vermillion_capital".to_string(),
                    strength: 8_000,
                    morale: 70,
                    supply: 84,
                    experience: 2,
                },
                Unit {
                    id: "vermillion_air".to_string(),
                    name: "Vermillion 2nd Air Wing".to_string(),
                    side: "vermillion".to_string(),
                    branch: Branch::Air,
                    zone: "vermillion_capital".to_string(),
                    strength: 2_200,
                    morale: 78,
                    supply: 84,
                    experience: 2,
                },
            ],
        }
    }

    pub fn demo_orders() -> Vec<OrderSheet> {
        vec![
            OrderSheet {
                side: "aurelia".to_string(),
                orders: vec![
                    Order {
                        day: 1,
                        unit: "aurelia_1".to_string(),
                        action: OrderAction::Move {
                            to: "rail_junction".to_string(),
                        },
                    },
                    Order {
                        day: 1,
                        unit: "aurelia_air".to_string(),
                        action: OrderAction::Recon,
                    },
                    Order {
                        day: 2,
                        unit: "aurelia_2".to_string(),
                        action: OrderAction::Move {
                            to: "alder_gate".to_string(),
                        },
                    },
                    Order {
                        day: 2,
                        unit: "aurelia_air".to_string(),
                        action: OrderAction::Recon,
                    },
                    Order {
                        day: 3,
                        unit: "aurelia_1".to_string(),
                        action: OrderAction::Attack {
                            target: "vesper".to_string(),
                        },
                    },
                    Order {
                        day: 3,
                        unit: "aurelia_2".to_string(),
                        action: OrderAction::Move {
                            to: "rail_junction".to_string(),
                        },
                    },
                    Order {
                        day: 4,
                        unit: "aurelia_1".to_string(),
                        action: OrderAction::Defend,
                    },
                    Order {
                        day: 5,
                        unit: "aurelia_1".to_string(),
                        action: OrderAction::Resupply,
                    },
                ],
            },
            OrderSheet {
                side: "vermillion".to_string(),
                orders: vec![
                    Order {
                        day: 1,
                        unit: "vermillion_7".to_string(),
                        action: OrderAction::Move {
                            to: "rail_junction".to_string(),
                        },
                    },
                    Order {
                        day: 1,
                        unit: "vermillion_air".to_string(),
                        action: OrderAction::Recon,
                    },
                    Order {
                        day: 2,
                        unit: "vermillion_8".to_string(),
                        action: OrderAction::Move {
                            to: "vesper".to_string(),
                        },
                    },
                    Order {
                        day: 3,
                        unit: "vermillion_7".to_string(),
                        action: OrderAction::Defend,
                    },
                    Order {
                        day: 4,
                        unit: "vermillion_7".to_string(),
                        action: OrderAction::Attack {
                            target: "rail_junction".to_string(),
                        },
                    },
                    Order {
                        day: 4,
                        unit: "vermillion_8".to_string(),
                        action: OrderAction::Move {
                            to: "vesper".to_string(),
                        },
                    },
                    Order {
                        day: 5,
                        unit: "vermillion_7".to_string(),
                        action: OrderAction::Resupply,
                    },
                ],
            },
        ]
    }
}

pub fn resolve_conflict(
    scenario: &RefereeScenario,
    sheets: &[OrderSheet],
) -> Result<ConflictLog, RefereeError> {
    RefereeSimulation::new(scenario.clone())?.resolve(sheets)
}

pub fn conflict_markdown(log: &ConflictLog) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", log.scenario.name));
    out.push_str(&format!(
        "**Start:** {} · **Duration:** {} days\n\n",
        log.scenario.start_date,
        log.days.len()
    ));
    out.push_str("## Sides\n\n");
    for side in &log.scenario.sides {
        let capital = log
            .scenario
            .zones
            .iter()
            .find(|zone| zone.id == side.capital_zone)
            .map(|zone| zone.name.as_str())
            .unwrap_or(&side.capital_zone);
        out.push_str(&format!("- **{}** — capital {}\n", side.name, capital));
    }
    out.push_str("\n## Timeline\n\n");
    for day in &log.days {
        out.push_str(&format!("### Day {}\n\n", day.day));
        for event in &day.events {
            out.push_str(&format!("- {}\n", event));
        }
        if !day.contacts.is_empty() {
            out.push_str("\n**Recon contacts:**\n\n");
            for contact in &day.contacts {
                let side = log
                    .scenario
                    .sides
                    .iter()
                    .find(|side| side.id == contact.side)
                    .map(|side| side.name.as_str())
                    .unwrap_or(&contact.side);
                let zone = log
                    .scenario
                    .zones
                    .iter()
                    .find(|zone| zone.id == contact.zone)
                    .map(|zone| zone.name.as_str())
                    .unwrap_or(&contact.zone);
                out.push_str(&format!(
                    "- {} spots **{}** at {} ({})\n",
                    side, contact.unit, zone, contact.estimated_strength
                ));
            }
        }
        if !day.notes.is_empty() {
            out.push_str("\n**Logistics:**\n\n");
            for note in &day.notes {
                out.push_str(&format!("- {}\n", note));
            }
        }
        if !day.casualties.is_empty() {
            out.push_str("\n**Casualties:**\n\n");
            for casualty in &day.casualties {
                let side = log
                    .scenario
                    .sides
                    .iter()
                    .find(|side| side.id == casualty.side)
                    .map(|side| side.name.as_str())
                    .unwrap_or(&casualty.side);
                out.push_str(&format!(
                    "- {} · {}: {}\n",
                    side, casualty.unit, casualty.losses
                ));
            }
        }
        if !day.control_changes.is_empty() {
            out.push_str("\n**Control:**\n\n");
            for change in &day.control_changes {
                out.push_str(&format!("- {}\n", change));
            }
        }
        out.push('\n');
    }
    out.push_str("## Final state\n\n");
    out.push_str(&log.summary);
    out.push('\n');
    out.push_str("\n### Control\n\n");
    out.push_str("| Zone | Controller |\n|---|---|\n");
    for zone in &log.final_zones {
        let controller = log
            .scenario
            .sides
            .iter()
            .find(|side| side.id == zone.owner)
            .map(|side| side.name.as_str())
            .unwrap_or("Neutral");
        out.push_str(&format!("| {} | {} |\n", zone.name, controller));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_conflict_resolves() {
        let scenario = RefereeScenario::demo();
        let log = resolve_conflict(&scenario, &RefereeScenario::demo_orders()).unwrap();
        assert_eq!(log.days.len(), 5);
        assert!(!log.summary.is_empty());
    }

    #[test]
    fn unknown_zone_is_rejected() {
        let mut scenario = RefereeScenario::demo();
        scenario.zones[0].connections.push("missing".to_string());
        assert!(scenario.validate().is_err());
    }

    #[test]
    fn invalid_unit_values_are_rejected() {
        let mut scenario = RefereeScenario::demo();
        scenario.units[0].morale = 101;
        assert!(scenario.validate().is_err());

        let mut scenario = RefereeScenario::demo();
        scenario.max_days = 0;
        assert!(scenario.validate().is_err());
    }

    #[test]
    fn report_contains_days_and_sides() {
        let scenario = RefereeScenario::demo();
        let log = resolve_conflict(&scenario, &RefereeScenario::demo_orders()).unwrap();
        let report = conflict_markdown(&log);
        assert!(report.contains("Day 1"));
        assert!(report.contains("Aurelia"));
    }
}
