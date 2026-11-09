//! Validated domain types and the public battle input schema for Drumfire.
//!
//! The model deliberately rejects invalid states at the boundary: force counts
//! are non-zero, ratings are 1..=10, morale is 0..=100, and a battle has at most
//! one home team. The simulation crate can then assume these invariants.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Maximum number of characters allowed in a team name.
pub const MAX_NAME_LEN: usize = 80;

/// Error returned when constructing a [`ForceCount`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ForceError {
    #[error("force count must be greater than zero")]
    Zero,
}

/// A validated, strictly positive troop count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct ForceCount(u32);

impl ForceCount {
    /// Construct a force count. Zero is rejected.
    pub fn new(value: u32) -> Result<Self, ForceError> {
        if value == 0 {
            Err(ForceError::Zero)
        } else {
            Ok(Self(value))
        }
    }

    /// Return the underlying count.
    pub fn get(self) -> u32 {
        self.0
    }
}

impl TryFrom<u32> for ForceCount {
    type Error = ForceError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ForceCount> for u32 {
    fn from(value: ForceCount) -> Self {
        value.0
    }
}

/// Error returned when constructing a [`Rating`].
#[derive(Debug, Error, PartialEq, Eq)]
#[error("rating must be between 1 and 10, got {0}")]
pub struct RatingError(pub u8);

/// A validated 1..=10 rating used for technology, training, experience,
/// logistics, and intel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct Rating(u8);

impl Rating {
    pub const MIN: u8 = 1;
    pub const MAX: u8 = 10;

    /// Construct a rating in `1..=10`.
    pub fn new(value: u8) -> Result<Self, RatingError> {
        if (Self::MIN..=Self::MAX).contains(&value) {
            Ok(Self(value))
        } else {
            Err(RatingError(value))
        }
    }

    /// Return the underlying value.
    pub fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for Rating {
    type Error = RatingError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Rating> for u8 {
    fn from(value: Rating) -> Self {
        value.0
    }
}

/// Error returned when constructing [`Morale`].
#[derive(Debug, Error, PartialEq, Eq)]
#[error("morale must be between 0 and 100, got {0}")]
pub struct MoraleError(pub u8);

/// A validated 0..=100 morale value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct Morale(u8);

impl Morale {
    pub const MAX: u8 = 100;

    /// Construct a morale value in `0..=100`.
    pub fn new(value: u8) -> Result<Self, MoraleError> {
        if value <= Self::MAX {
            Ok(Self(value))
        } else {
            Err(MoraleError(value))
        }
    }

    /// Return the underlying value.
    pub fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for Morale {
    type Error = MoraleError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Morale> for u8 {
    fn from(value: Morale) -> Self {
        value.0
    }
}

/// Error returned when parsing a [`Seed`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SeedError {
    #[error("seed must be 64 hexadecimal characters, got {0}")]
    Length(usize),
    #[error("seed contains a non-hexadecimal character")]
    InvalidHex,
}

/// A 32-byte RNG seed, serialized as 64 lowercase hex characters.
///
/// Seeds are part of the public battle record. Publishing a seed is safe: it
/// lets anyone reproduce the battle, and it is not a credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Seed([u8; 32]);

impl Seed {
    /// The all-zero seed. Useful in tests and examples, but prefer a random
    /// seed for real battles.
    pub const ZERO: Seed = Seed([0; 32]);

    /// Wrap raw bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Build a seed from a `u64` (useful for tests and short examples).
    pub fn from_u64(value: u64) -> Self {
        let mut bytes = [0u8; 32];
        bytes[..8].copy_from_slice(&value.to_be_bytes());
        Self(bytes)
    }

    /// Parse a 64-character hex seed.
    pub fn from_hex(input: &str) -> Result<Self, SeedError> {
        let input = input.trim();
        if input.len() != 64 {
            return Err(SeedError::Length(input.len()));
        }
        let decoded = hex::decode(input).map_err(|_| SeedError::InvalidHex)?;
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&decoded);
        Ok(Self(bytes))
    }

    /// Render the seed as 64 lowercase hex characters.
    pub fn to_hex(self) -> String {
        hex::encode(self.0)
    }

    /// Borrow the raw bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl TryFrom<String> for Seed {
    type Error = SeedError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_hex(&value)
    }
}

impl From<Seed> for String {
    fn from(value: Seed) -> Self {
        value.to_hex()
    }
}

/// One side's validated input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeamInput {
    pub name: String,
    pub forces: ForceCount,
    pub morale: Morale,
    pub tech: Rating,
    pub training: Rating,
    pub experience: Rating,
    pub logistics: Rating,
    pub intel: Rating,
    /// Whether this side is fighting on home soil.
    #[serde(default)]
    pub home: bool,
}

impl TeamInput {
    /// Validate the free-form parts of the team (the numeric types are already
    /// validated by construction and by serde).
    pub fn validate(&self) -> Result<(), ValidationError> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(ValidationError::EmptyName);
        }
        if name.chars().count() > MAX_NAME_LEN {
            return Err(ValidationError::NameTooLong { max: MAX_NAME_LEN });
        }
        Ok(())
    }
}

/// Tunable battle parameters. Every field has a default, so a minimal input
/// file only needs teams and a seed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BattleConfig {
    /// Hard cap on resolved intervals. Range: 1..=100.
    pub max_rounds: u32,
    /// Logistic scale for the strength difference. Range: 0.1..=20.0.
    pub scale: f64,
    /// Strength bonus for the home side. Range: -2.0..=2.0.
    pub home_bonus: f64,
    /// Base casualty intensity. Range: 0.0..=0.5.
    pub casualty_rate: f64,
    /// A side at or below this morale routs. Range: 0..=100.
    pub morale_rout_threshold: u8,
}

impl Default for BattleConfig {
    fn default() -> Self {
        Self {
            max_rounds: 12,
            scale: 2.5,
            home_bonus: 0.35,
            casualty_rate: 0.12,
            morale_rout_threshold: 20,
        }
    }
}

impl BattleConfig {
    /// Validate ranges and finiteness.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if !(1..=100).contains(&self.max_rounds) {
            return Err(ValidationError::InvalidConfig(
                "max_rounds must be between 1 and 100",
            ));
        }
        if !self.scale.is_finite() || !(0.1..=20.0).contains(&self.scale) {
            return Err(ValidationError::InvalidConfig(
                "scale must be finite and between 0.1 and 20.0",
            ));
        }
        if !self.home_bonus.is_finite() || !(-2.0..=2.0).contains(&self.home_bonus) {
            return Err(ValidationError::InvalidConfig(
                "home_bonus must be finite and between -2.0 and 2.0",
            ));
        }
        if !self.casualty_rate.is_finite() || !(0.0..=0.5).contains(&self.casualty_rate) {
            return Err(ValidationError::InvalidConfig(
                "casualty_rate must be finite and between 0.0 and 0.5",
            ));
        }
        if self.morale_rout_threshold > 100 {
            return Err(ValidationError::InvalidConfig(
                "morale_rout_threshold must be between 0 and 100",
            ));
        }
        Ok(())
    }
}

/// The simulation mode. Skirmish and Battle are implemented; the rest are
/// reserved so the schema and UI can grow without breaking existing logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Skirmish,
    #[default]
    Battle,
    Frontlines,
    Dogfight,
    Naval,
}

impl Mode {
    /// UI order: supported ground modes first, then planned modes.
    pub const ALL: [Mode; 5] = [
        Mode::Skirmish,
        Mode::Battle,
        Mode::Frontlines,
        Mode::Dogfight,
        Mode::Naval,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Mode::Skirmish => "Skirmish",
            Mode::Battle => "Battle",
            Mode::Frontlines => "Frontlines",
            Mode::Dogfight => "Dogfight",
            Mode::Naval => "Naval",
        }
    }

    /// What one resolved interval is called in this mode.
    pub fn phase_label(self) -> &'static str {
        match self {
            Mode::Skirmish => "Exchange",
            Mode::Battle => "Phase",
            Mode::Frontlines => "Day",
            Mode::Dogfight => "Pass",
            Mode::Naval => "Salvo",
        }
    }

    /// Plural form of [`Mode::phase_label`].
    pub fn phase_label_plural(self) -> &'static str {
        match self {
            Mode::Skirmish => "Exchanges",
            Mode::Battle => "Phases",
            Mode::Frontlines => "Days",
            Mode::Dogfight => "Passes",
            Mode::Naval => "Salvoes",
        }
    }

    /// What the force count represents in this mode.
    pub fn force_label(self) -> &'static str {
        match self {
            Mode::Skirmish => "Combatants",
            Mode::Battle => "Troops",
            Mode::Frontlines => "Sectors",
            Mode::Dogfight => "Aircraft",
            Mode::Naval => "Ships",
        }
    }

    /// Whether this build can simulate the mode.
    pub fn is_supported(self) -> bool {
        matches!(
            self,
            Mode::Skirmish | Mode::Battle | Mode::Dogfight | Mode::Naval | Mode::Frontlines
        )
    }

    /// Whether the mode uses the shared ground order of battle.
    pub fn is_ground(self) -> bool {
        matches!(self, Mode::Skirmish | Mode::Battle)
    }

    /// Battle is the default and is omitted from serialized Battle inputs to
    /// keep existing logs and hashes stable.
    pub fn is_battle(&self) -> bool {
        matches!(self, Mode::Battle)
    }
}

impl std::str::FromStr for Mode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "skirmish" => Ok(Mode::Skirmish),
            "battle" => Ok(Mode::Battle),
            "frontlines" => Ok(Mode::Frontlines),
            "dogfight" => Ok(Mode::Dogfight),
            "naval" => Ok(Mode::Naval),
            other => Err(format!(
                "unknown mode {other:?}; expected skirmish, battle, frontlines, dogfight, or naval"
            )),
        }
    }
}

/// The complete, serializable input to one battle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BattleInput {
    #[serde(default, skip_serializing_if = "Mode::is_battle")]
    pub mode: Mode,
    pub attacker: TeamInput,
    pub defender: TeamInput,
    #[serde(default)]
    pub config: BattleConfig,
    pub seed: Seed,
}

impl BattleInput {
    /// Validate names, mode support, home-side exclusivity, and configuration.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if !self.mode.is_ground() {
            return Err(ValidationError::UnsupportedMode(self.mode));
        }
        self.attacker.validate()?;
        self.defender.validate()?;
        if self.attacker.home && self.defender.home {
            return Err(ValidationError::BothHome);
        }
        self.config.validate()?;
        Ok(())
    }
}

/// Error returned when constructing an [`AircraftCount`].
#[derive(Debug, Error, PartialEq, Eq)]
#[error("aircraft count must be between 1 and 64, got {0}")]
pub struct AircraftCountError(pub u8);

/// A validated number of aircraft committed to a dogfight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct AircraftCount(u8);

impl AircraftCount {
    pub const MAX: u8 = 64;

    pub fn new(value: u8) -> Result<Self, AircraftCountError> {
        if (1..=Self::MAX).contains(&value) {
            Ok(Self(value))
        } else {
            Err(AircraftCountError(value))
        }
    }

    pub fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for AircraftCount {
    type Error = AircraftCountError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<AircraftCount> for u8 {
    fn from(value: AircraftCount) -> Self {
        value.0
    }
}

/// One side's air order of battle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AirForce {
    pub name: String,
    pub aircraft: AircraftCount,
    /// Pilot skill and training.
    pub pilot_skill: Rating,
    /// Aircraft performance: speed, climb, turn, sensors.
    pub aircraft_quality: Rating,
    /// Missiles, guns, and fire-control quality.
    pub weapons: Rating,
    /// Radar, electronic warfare, and detection.
    pub radar: Rating,
    /// Ground-controlled intercept, AWACS, tankers, and sustainment.
    pub support: Rating,
    /// Formation discipline and situational awareness.
    pub cohesion: Morale,
}

impl AirForce {
    pub fn validate(&self) -> Result<(), ValidationError> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(ValidationError::EmptyName);
        }
        if name.chars().count() > MAX_NAME_LEN {
            return Err(ValidationError::NameTooLong { max: MAX_NAME_LEN });
        }
        Ok(())
    }
}

/// Tunable dogfight parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DogfightConfig {
    /// Hard cap on passes. Range: 1..=30.
    pub max_passes: u32,
    /// Logistic scale for the strength difference. Range: 0.1..=20.0.
    pub scale: f64,
    /// Base kill intensity per pass. Range: 0.0..=0.5.
    pub kill_rate: f64,
    /// A side at or below this cohesion breaks off. Range: 0..=100.
    pub cohesion_break: u8,
}

impl Default for DogfightConfig {
    fn default() -> Self {
        Self {
            max_passes: 8,
            scale: 2.0,
            kill_rate: 0.12,
            cohesion_break: 20,
        }
    }
}

impl DogfightConfig {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if !(1..=30).contains(&self.max_passes) {
            return Err(ValidationError::InvalidConfig(
                "max_passes must be between 1 and 30",
            ));
        }
        if !self.scale.is_finite() || !(0.1..=20.0).contains(&self.scale) {
            return Err(ValidationError::InvalidConfig(
                "scale must be finite and between 0.1 and 20.0",
            ));
        }
        if !self.kill_rate.is_finite() || !(0.0..=0.5).contains(&self.kill_rate) {
            return Err(ValidationError::InvalidConfig(
                "kill_rate must be finite and between 0.0 and 0.5",
            ));
        }
        if self.cohesion_break > 100 {
            return Err(ValidationError::InvalidConfig(
                "cohesion_break must be between 0 and 100",
            ));
        }
        Ok(())
    }
}

fn dogfight_mode() -> Mode {
    Mode::Dogfight
}

/// The complete, serializable input to one dogfight.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DogfightInput {
    #[serde(default = "dogfight_mode")]
    pub mode: Mode,
    pub attacker: AirForce,
    pub defender: AirForce,
    #[serde(default)]
    pub config: DogfightConfig,
    pub seed: Seed,
}

impl DogfightInput {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.mode != Mode::Dogfight {
            return Err(ValidationError::UnsupportedMode(self.mode));
        }
        self.attacker.validate()?;
        self.defender.validate()?;
        self.config.validate()?;
        Ok(())
    }
}

/// Error returned when constructing a [`ShipCount`].
#[derive(Debug, Error, PartialEq, Eq)]
#[error("ship count must be between 1 and 32, got {0}")]
pub struct ShipCountError(pub u8);

/// A validated number of ships committed to a naval action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct ShipCount(u8);

impl ShipCount {
    pub const MAX: u8 = 32;

    pub fn new(value: u8) -> Result<Self, ShipCountError> {
        if (1..=Self::MAX).contains(&value) {
            Ok(Self(value))
        } else {
            Err(ShipCountError(value))
        }
    }

    pub fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for ShipCount {
    type Error = ShipCountError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ShipCount> for u8 {
    fn from(value: ShipCount) -> Self {
        value.0
    }
}

/// One side's naval order of battle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fleet {
    pub name: String,
    pub ships: ShipCount,
    /// Hull, armor, and damage resistance.
    pub hull_quality: Rating,
    /// Radar, sonar, electronic warfare, and fire control.
    pub sensors: Rating,
    /// Missiles, torpedoes, guns, and magazine depth.
    pub weapons: Rating,
    /// Carrier air cover, SAMs, and point defense.
    pub air_cover: Rating,
    /// Damage control, crew quality, and repair parties.
    pub damage_control: Rating,
    /// Readiness and formation cohesion.
    pub readiness: Morale,
}

impl Fleet {
    pub fn validate(&self) -> Result<(), ValidationError> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(ValidationError::EmptyName);
        }
        if name.chars().count() > MAX_NAME_LEN {
            return Err(ValidationError::NameTooLong { max: MAX_NAME_LEN });
        }
        Ok(())
    }
}

/// Tunable naval parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NavalConfig {
    /// Hard cap on salvos. Range: 1..=20.
    pub max_salvos: u32,
    /// Logistic scale for the strength difference. Range: 0.1..=20.0.
    pub scale: f64,
    /// Base hit/sink intensity per salvo. Range: 0.0..=0.5.
    pub hit_rate: f64,
    /// A side at or below this readiness withdraws. Range: 0..=100.
    pub readiness_break: u8,
}

impl Default for NavalConfig {
    fn default() -> Self {
        Self {
            max_salvos: 8,
            scale: 2.0,
            hit_rate: 0.14,
            readiness_break: 20,
        }
    }
}

impl NavalConfig {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if !(1..=20).contains(&self.max_salvos) {
            return Err(ValidationError::InvalidConfig(
                "max_salvos must be between 1 and 20",
            ));
        }
        if !self.scale.is_finite() || !(0.1..=20.0).contains(&self.scale) {
            return Err(ValidationError::InvalidConfig(
                "scale must be finite and between 0.1 and 20.0",
            ));
        }
        if !self.hit_rate.is_finite() || !(0.0..=0.5).contains(&self.hit_rate) {
            return Err(ValidationError::InvalidConfig(
                "hit_rate must be finite and between 0.0 and 0.5",
            ));
        }
        if self.readiness_break > 100 {
            return Err(ValidationError::InvalidConfig(
                "readiness_break must be between 0 and 100",
            ));
        }
        Ok(())
    }
}

fn naval_mode() -> Mode {
    Mode::Naval
}

/// The complete, serializable input to one naval action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NavalInput {
    #[serde(default = "naval_mode")]
    pub mode: Mode,
    pub attacker: Fleet,
    pub defender: Fleet,
    #[serde(default)]
    pub config: NavalConfig,
    pub seed: Seed,
}

impl NavalInput {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.mode != Mode::Naval {
            return Err(ValidationError::UnsupportedMode(self.mode));
        }
        self.attacker.validate()?;
        self.defender.validate()?;
        self.config.validate()?;
        Ok(())
    }
}

/// One side's strategic front: committed sector troops plus reserves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrontForce {
    pub name: String,
    /// Uncommitted replacements available to flow into sectors.
    pub reserves: u32,
    pub morale: Morale,
    pub tech: Rating,
    pub training: Rating,
    pub experience: Rating,
    pub logistics: Rating,
    pub intel: Rating,
}

impl FrontForce {
    pub fn validate(&self) -> Result<(), ValidationError> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(ValidationError::EmptyName);
        }
        if name.chars().count() > MAX_NAME_LEN {
            return Err(ValidationError::NameTooLong { max: MAX_NAME_LEN });
        }
        Ok(())
    }
}

/// One sector of the front.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectorInput {
    pub name: String,
    pub attacker_troops: ForceCount,
    pub defender_troops: ForceCount,
    /// Defender fortification, 1–10.
    pub fortification: Rating,
    /// Whether capturing this sector counts toward a breakthrough.
    pub objective: bool,
}

impl SectorInput {
    pub fn validate(&self) -> Result<(), ValidationError> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(ValidationError::EmptyName);
        }
        if name.chars().count() > MAX_NAME_LEN {
            return Err(ValidationError::NameTooLong { max: MAX_NAME_LEN });
        }
        Ok(())
    }
}

/// Tunable front-line parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FrontlineConfig {
    /// Hard cap on days. Range: 1..=30.
    pub max_days: u32,
    /// Logistic scale for a sector’s strength difference. Range: 0.1..=20.0.
    pub scale: f64,
    /// Base casualty intensity per sector per day. Range: 0.0..=0.5.
    pub casualty_rate: f64,
    /// A side at or below this morale breaks. Range: 0..=100.
    pub morale_rout_threshold: u8,
    /// Fraction of daily losses replaced from reserves. Range: 0.0..=1.0.
    pub replenish_rate: f64,
    /// A sector is captured when the defender falls to this fraction of its
    /// starting troops. Range: 0.0..=0.5.
    pub capture_threshold: f64,
}

impl Default for FrontlineConfig {
    fn default() -> Self {
        Self {
            max_days: 10,
            scale: 2.5,
            casualty_rate: 0.12,
            morale_rout_threshold: 20,
            replenish_rate: 0.25,
            capture_threshold: 0.10,
        }
    }
}

impl FrontlineConfig {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if !(1..=30).contains(&self.max_days) {
            return Err(ValidationError::InvalidConfig(
                "max_days must be between 1 and 30",
            ));
        }
        if !self.scale.is_finite() || !(0.1..=20.0).contains(&self.scale) {
            return Err(ValidationError::InvalidConfig(
                "scale must be finite and between 0.1 and 20.0",
            ));
        }
        if !self.casualty_rate.is_finite() || !(0.0..=0.5).contains(&self.casualty_rate) {
            return Err(ValidationError::InvalidConfig(
                "casualty_rate must be finite and between 0.0 and 0.5",
            ));
        }
        if self.morale_rout_threshold > 100 {
            return Err(ValidationError::InvalidConfig(
                "morale_rout_threshold must be between 0 and 100",
            ));
        }
        if !self.replenish_rate.is_finite() || !(0.0..=1.0).contains(&self.replenish_rate) {
            return Err(ValidationError::InvalidConfig(
                "replenish_rate must be finite and between 0.0 and 1.0",
            ));
        }
        if !self.capture_threshold.is_finite() || !(0.0..=0.5).contains(&self.capture_threshold) {
            return Err(ValidationError::InvalidConfig(
                "capture_threshold must be finite and between 0.0 and 0.5",
            ));
        }
        Ok(())
    }
}

fn frontlines_mode() -> Mode {
    Mode::Frontlines
}

/// The complete, serializable input to one front-line battle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrontlineInput {
    #[serde(default = "frontlines_mode")]
    pub mode: Mode,
    pub attacker: FrontForce,
    pub defender: FrontForce,
    /// Three to five sectors. The GUI uses three; the engine supports up to five.
    pub sectors: Vec<SectorInput>,
    #[serde(default)]
    pub config: FrontlineConfig,
    pub seed: Seed,
}

impl FrontlineInput {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.mode != Mode::Frontlines {
            return Err(ValidationError::UnsupportedMode(self.mode));
        }
        self.attacker.validate()?;
        self.defender.validate()?;
        if !(3..=5).contains(&self.sectors.len()) {
            return Err(ValidationError::InvalidConfig(
                "frontlines need 3 to 5 sectors",
            ));
        }
        if !self.sectors.iter().any(|sector| sector.objective) {
            return Err(ValidationError::InvalidConfig(
                "frontlines need at least one objective sector",
            ));
        }
        for (index, sector) in self.sectors.iter().enumerate() {
            sector.validate()?;
            if self.sectors[..index]
                .iter()
                .any(|other| other.name.eq_ignore_ascii_case(&sector.name))
            {
                return Err(ValidationError::InvalidConfig(
                    "sector names must be unique",
                ));
            }
        }
        self.config.validate()?;
        Ok(())
    }
}

/// Errors produced by model-level validation.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ValidationError {
    #[error("team name must not be empty")]
    EmptyName,
    #[error("team name is too long (maximum {max} characters)")]
    NameTooLong { max: usize },
    #[error("both teams cannot have home advantage")]
    BothHome,
    #[error("invalid configuration: {0}")]
    InvalidConfig(&'static str),
    #[error("mode {0:?} is not available for this input")]
    UnsupportedMode(Mode),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn team(name: &str) -> TeamInput {
        TeamInput {
            name: name.to_string(),
            forces: ForceCount::new(1000).unwrap(),
            morale: Morale::new(70).unwrap(),
            tech: Rating::new(5).unwrap(),
            training: Rating::new(5).unwrap(),
            experience: Rating::new(5).unwrap(),
            logistics: Rating::new(5).unwrap(),
            intel: Rating::new(5).unwrap(),
            home: false,
        }
    }

    fn input() -> BattleInput {
        BattleInput {
            mode: Mode::Battle,
            attacker: team("A"),
            defender: team("B"),
            config: BattleConfig::default(),
            seed: Seed::from_u64(1),
        }
    }

    #[test]
    fn force_count_rejects_zero() {
        assert!(ForceCount::new(0).is_err());
        assert_eq!(ForceCount::new(1).unwrap().get(), 1);
    }

    #[test]
    fn ratings_reject_out_of_range() {
        assert!(Rating::new(0).is_err());
        assert!(Rating::new(11).is_err());
        assert_eq!(Rating::new(1).unwrap().get(), 1);
        assert_eq!(Rating::new(10).unwrap().get(), 10);
    }

    #[test]
    fn morale_accepts_bounds() {
        assert_eq!(Morale::new(0).unwrap().get(), 0);
        assert_eq!(Morale::new(100).unwrap().get(), 100);
        assert!(Morale::new(101).is_err());
    }

    #[test]
    fn seed_round_trips_through_hex() {
        let seed = Seed::from_u64(0xdead_beef);
        let hex = seed.to_hex();
        assert_eq!(hex.len(), 64);
        assert_eq!(Seed::from_hex(&hex).unwrap(), seed);
    }

    #[test]
    fn seed_rejects_bad_input() {
        assert!(matches!(Seed::from_hex("abcd"), Err(SeedError::Length(4))));
        assert!(matches!(
            Seed::from_hex(&"z".repeat(64)),
            Err(SeedError::InvalidHex)
        ));
    }

    #[test]
    fn empty_name_is_invalid() {
        let mut i = input();
        i.attacker.name = "   ".into();
        assert_eq!(i.validate(), Err(ValidationError::EmptyName));
    }

    #[test]
    fn both_home_is_invalid() {
        let mut i = input();
        i.attacker.home = true;
        i.defender.home = true;
        assert_eq!(i.validate(), Err(ValidationError::BothHome));
    }

    #[test]
    fn config_ranges_are_enforced() {
        let mut i = input();
        i.config.max_rounds = 0;
        assert!(i.validate().is_err());

        let mut i = input();
        i.config.scale = 0.0;
        assert!(i.validate().is_err());

        let mut i = input();
        i.config.casualty_rate = 0.9;
        assert!(i.validate().is_err());

        let mut i = input();
        i.config.morale_rout_threshold = 101;
        assert!(i.validate().is_err());
    }

    #[test]
    fn serde_validates_newtypes() {
        let bad = r#"{
            "name": "A",
            "forces": 0,
            "morale": 50,
            "tech": 5,
            "training": 5,
            "experience": 5,
            "logistics": 5,
            "intel": 5
        }"#;
        assert!(serde_json::from_str::<TeamInput>(bad).is_err());

        let bad_rating = bad
            .replace("\"forces\": 0", "\"forces\": 100")
            .replace("\"tech\": 5", "\"tech\": 11");
        assert!(serde_json::from_str::<TeamInput>(&bad_rating).is_err());
    }

    #[test]
    fn battle_mode_is_default_and_omitted_from_json() {
        let input = input();
        assert_eq!(input.mode, Mode::Battle);
        let json = serde_json::to_string(&input).unwrap();
        assert!(!json.contains("\"mode\""));
        let parsed: BattleInput = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.mode, Mode::Battle);
    }

    #[test]
    fn skirmish_mode_serializes_and_round_trips() {
        let mut input = input();
        input.mode = Mode::Skirmish;
        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains("\"mode\":\"skirmish\""));
        let parsed: BattleInput = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.mode, Mode::Skirmish);
    }

    #[test]
    fn planned_modes_are_rejected_by_validation() {
        let mut input = input();
        input.mode = Mode::Frontlines;
        assert_eq!(
            input.validate(),
            Err(ValidationError::UnsupportedMode(Mode::Frontlines))
        );
    }

    fn air_force(name: &str, aircraft: u8) -> AirForce {
        AirForce {
            name: name.to_string(),
            aircraft: AircraftCount::new(aircraft).unwrap(),
            pilot_skill: Rating::new(6).unwrap(),
            aircraft_quality: Rating::new(6).unwrap(),
            weapons: Rating::new(6).unwrap(),
            radar: Rating::new(5).unwrap(),
            support: Rating::new(5).unwrap(),
            cohesion: Morale::new(70).unwrap(),
        }
    }

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

    fn front_force(name: &str) -> FrontForce {
        FrontForce {
            name: name.to_string(),
            reserves: 5000,
            morale: Morale::new(70).unwrap(),
            tech: Rating::new(6).unwrap(),
            training: Rating::new(6).unwrap(),
            experience: Rating::new(6).unwrap(),
            logistics: Rating::new(5).unwrap(),
            intel: Rating::new(5).unwrap(),
        }
    }

    fn sector(name: &str, objective: bool) -> SectorInput {
        SectorInput {
            name: name.to_string(),
            attacker_troops: ForceCount::new(1000).unwrap(),
            defender_troops: ForceCount::new(1000).unwrap(),
            fortification: Rating::new(6).unwrap(),
            objective,
        }
    }

    #[test]
    fn aircraft_count_rejects_zero_and_huge_counts() {
        assert!(AircraftCount::new(0).is_err());
        assert!(AircraftCount::new(65).is_err());
        assert_eq!(AircraftCount::new(1).unwrap().get(), 1);
        assert_eq!(AircraftCount::new(64).unwrap().get(), 64);
    }

    #[test]
    fn dogfight_input_round_trips() {
        let input = DogfightInput {
            mode: Mode::Dogfight,
            attacker: air_force("Red", 6),
            defender: air_force("Blue", 8),
            config: DogfightConfig::default(),
            seed: Seed::from_u64(9),
        };
        input.validate().unwrap();
        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains("\"mode\":\"dogfight\""));
        let parsed: DogfightInput = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, input);
    }

    #[test]
    fn ground_input_rejects_air_mode() {
        let mut input = input();
        input.mode = Mode::Dogfight;
        assert_eq!(
            input.validate(),
            Err(ValidationError::UnsupportedMode(Mode::Dogfight))
        );
    }

    #[test]
    fn ship_count_rejects_zero_and_huge_counts() {
        assert!(ShipCount::new(0).is_err());
        assert!(ShipCount::new(33).is_err());
        assert_eq!(ShipCount::new(1).unwrap().get(), 1);
        assert_eq!(ShipCount::new(32).unwrap().get(), 32);
    }

    #[test]
    fn naval_input_round_trips() {
        let input = NavalInput {
            mode: Mode::Naval,
            attacker: fleet("Red Fleet", 6),
            defender: fleet("Blue Fleet", 8),
            config: NavalConfig::default(),
            seed: Seed::from_u64(13),
        };
        input.validate().unwrap();
        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains("\"mode\":\"naval\""));
        let parsed: NavalInput = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, input);
    }

    #[test]
    fn frontline_input_round_trips() {
        let input = FrontlineInput {
            mode: Mode::Frontlines,
            attacker: front_force("Red Front"),
            defender: front_force("Blue Front"),
            sectors: vec![
                sector("North", false),
                sector("Center", true),
                sector("South", true),
            ],
            config: FrontlineConfig::default(),
            seed: Seed::from_u64(19),
        };
        input.validate().unwrap();
        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains("\"mode\":\"frontlines\""));
        let parsed: FrontlineInput = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, input);
    }

    #[test]
    fn mode_metadata_is_distinct() {
        assert_eq!(Mode::Skirmish.phase_label(), "Exchange");
        assert_eq!(Mode::Battle.phase_label(), "Phase");
        assert_eq!(Mode::Skirmish.force_label(), "Combatants");
        assert_eq!(Mode::Battle.force_label(), "Troops");
        assert!(Mode::Skirmish.is_supported());
        assert!(Mode::Battle.is_supported());
        assert!(Mode::Dogfight.is_supported());
        assert!(Mode::Naval.is_supported());
        assert!(Mode::Frontlines.is_supported());
    }

    #[test]
    fn mode_parses_from_cli_names() {
        assert_eq!("skirmish".parse::<Mode>().unwrap(), Mode::Skirmish);
        assert_eq!("BATTLE".parse::<Mode>().unwrap(), Mode::Battle);
        assert_eq!("dogfight".parse::<Mode>().unwrap(), Mode::Dogfight);
        assert!("starship".parse::<Mode>().is_err());
    }
}
