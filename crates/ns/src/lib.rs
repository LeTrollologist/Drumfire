//! Read-only NationStates API client.
//!
//! This crate only touches public shards. It never sends credentials, never
//! automates a game action, and never scrapes HTML. The client sets an
//! informative `User-Agent`, gates requests with the documented API rate
//! limit, and observes the server's `RateLimit-*` headers.
//!
//! The only request currently exposed is a batched nation lookup used by the
//! GUI's "Import nation" button.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::Event;
use quick_xml::Reader;
use reqwest::blocking::Client;
use reqwest::header::HeaderMap;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// API version pinned by this client. NationStates supports the two most recent
/// versions; bump this deliberately after reading the changelog.
pub const API_VERSION: &str = "13";

const API_BASE: &str = "https://www.nationstates.net/cgi-bin/api.cgi";
const DEFAULT_CONTACT: &str = "https://github.com/LeTrollologist/NationStatesBattleSim";
const DEFAULT_LIMIT: u32 = 50;
const DEFAULT_WINDOW_SECS: u64 = 30;

/// A small, display-oriented subset of a nation's public data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NationSummary {
    pub name: String,
    pub fullname: String,
    pub region: String,
    pub population: u64,
    pub category: String,
    pub motto: String,
    pub currency: String,
    pub flag: String,
}

/// Errors from the read-only NationStates client.
#[derive(Debug, Error)]
pub enum NsError {
    #[error("nation name must not be empty")]
    EmptyNation,
    #[error("nation name is too long (maximum 80 characters)")]
    NationTooLong,
    #[error("nation name contains a control character")]
    InvalidNation,
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("NationStates returned HTTP {status}")]
    HttpStatus { status: u16 },
    #[error("rate limited by NationStates; retry after {retry_after} seconds")]
    RateLimited { retry_after: u64 },
    #[error("could not parse NationStates response: {0}")]
    Parse(String),
    #[error("NationStates response did not include {0}")]
    Missing(String),
}

/// The `User-Agent` sent with every request. Set `DRUMFIRE_CONTACT` to a URL or
/// email address the NationStates staff can use to reach you.
pub fn user_agent() -> String {
    let contact = std::env::var("DRUMFIRE_CONTACT").unwrap_or_else(|_| DEFAULT_CONTACT.to_string());
    format!(
        "Drumfire/{} (NationStates roleplay tool; contact: {})",
        env!("CARGO_PKG_VERSION"),
        contact
    )
}

/// A conservative, single-process gate for the documented 50 requests / 30 s
/// per-IP limit. It is advisory: the server's headers always win when present.
struct Gate {
    limit: u32,
    remaining: u32,
    window: Duration,
    reset_at: Instant,
}

impl Gate {
    fn new() -> Self {
        let window = Duration::from_secs(DEFAULT_WINDOW_SECS);
        Self {
            limit: DEFAULT_LIMIT,
            remaining: DEFAULT_LIMIT,
            window,
            reset_at: Instant::now() + window,
        }
    }

    fn acquire(&mut self) -> Result<(), NsError> {
        let now = Instant::now();
        if now >= self.reset_at {
            self.reset_at = now + self.window;
            self.remaining = self.limit;
        }
        if self.remaining == 0 {
            let wait = self
                .reset_at
                .saturating_duration_since(now)
                .as_secs()
                .max(1);
            return Err(NsError::RateLimited { retry_after: wait });
        }
        self.remaining -= 1;
        Ok(())
    }

    fn observe(&mut self, headers: &HeaderMap) {
        let now = Instant::now();

        if let Some(policy) = header_str(headers, "ratelimit-policy") {
            // e.g. "50;w=30"
            let mut parts = policy.split(';');
            if let Some(limit) = parts.next().and_then(|v| v.trim().parse::<u32>().ok()) {
                self.limit = limit;
            }
            for part in parts {
                if let Some(window) = part.trim().strip_prefix("w=") {
                    if let Ok(secs) = window.parse::<u64>() {
                        self.window = Duration::from_secs(secs.max(1));
                    }
                }
            }
        }
        if let Some(limit) = header_u64(headers, "ratelimit-limit") {
            self.limit = limit.min(u32::MAX as u64) as u32;
        }
        if let Some(remaining) = header_u64(headers, "ratelimit-remaining") {
            self.remaining = remaining.min(self.limit as u64) as u32;
        }
        if let Some(reset) = header_u64(headers, "ratelimit-reset") {
            self.reset_at = now + Duration::from_secs(reset.max(1));
        }
        self.remaining = self.remaining.min(self.limit);
    }
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

fn header_u64(headers: &HeaderMap, name: &str) -> Option<u64> {
    header_str(headers, name).and_then(|value| value.trim().parse::<u64>().ok())
}

fn gate() -> &'static Mutex<Gate> {
    static GATE: OnceLock<Mutex<Gate>> = OnceLock::new();
    GATE.get_or_init(|| Mutex::new(Gate::new()))
}

fn normalize_nation(nation: &str) -> Result<String, NsError> {
    let trimmed = nation.trim();
    if trimmed.is_empty() {
        return Err(NsError::EmptyNation);
    }
    if trimmed.chars().count() > 80 {
        return Err(NsError::NationTooLong);
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return Err(NsError::InvalidNation);
    }
    Ok(trimmed.replace(' ', "_"))
}

/// Fetch one nation with a single batched public-shard request.
///
/// The request is `q=name+fullname+region+population+category+motto+currency+flag`
/// pinned to [`API_VERSION`]. No credentials are sent.
pub fn fetch_nation(nation: &str) -> Result<NationSummary, NsError> {
    let nation = normalize_nation(nation)?;

    {
        let mut gate = gate()
            .lock()
            .map_err(|_| NsError::Parse("rate gate was poisoned".to_string()))?;
        gate.acquire()?;
    }

    let client = Client::builder()
        .user_agent(user_agent())
        .timeout(Duration::from_secs(15))
        .build()?;

    let url = format!(
        "{API_BASE}?nation={}&q=name+fullname+region+population+category+motto+currency+flag&v={API_VERSION}",
        urlencoding::encode(&nation)
    );

    let response = client.get(url).send()?;
    let status = response.status();
    let headers = response.headers().clone();
    if let Ok(mut gate) = gate().lock() {
        gate.observe(&headers);
    }

    if status == StatusCode::TOO_MANY_REQUESTS {
        let retry_after = header_u64(&headers, "retry-after").unwrap_or(30);
        return Err(NsError::RateLimited { retry_after });
    }
    if !status.is_success() {
        return Err(NsError::HttpStatus {
            status: status.as_u16(),
        });
    }

    if let Some(length) = response.content_length() {
        if length > 1_000_000 {
            return Err(NsError::Parse(
                "response was unexpectedly large".to_string(),
            ));
        }
    }

    let body = response.text()?;
    parse_nation_xml(&body)
}

/// Parse the XML returned by the public nation shard. Kept public for tests and
/// for callers that already have a response body.
pub fn parse_nation_xml(xml: &str) -> Result<NationSummary, NsError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);

    let mut current: Option<String> = None;
    let mut values: HashMap<String, String> = HashMap::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                current = Some(String::from_utf8_lossy(event.name().as_ref()).into_owned());
            }
            Ok(Event::Text(event)) => {
                if let Some(tag) = &current {
                    let text = event
                        .xml10_content()
                        .map_err(|err| NsError::Parse(err.to_string()))?;
                    values
                        .entry(tag.clone())
                        .or_default()
                        .push_str(text.as_ref());
                }
            }
            Ok(Event::GeneralRef(event)) => {
                if let Some(tag) = &current {
                    let resolved = match event.resolve_char_ref() {
                        Ok(Some(ch)) => ch.to_string(),
                        Ok(None) => {
                            let name = event
                                .decode()
                                .map_err(|err| NsError::Parse(err.to_string()))?;
                            resolve_predefined_entity(&name)
                                .map(str::to_string)
                                .unwrap_or_else(|| format!("&{name};"))
                        }
                        Err(err) => return Err(NsError::Parse(err.to_string())),
                    };
                    values.entry(tag.clone()).or_default().push_str(&resolved);
                }
            }
            Ok(Event::End(_)) => current = None,
            Ok(Event::Eof) => break,
            Err(err) => return Err(NsError::Parse(err.to_string())),
            _ => {}
        }
    }

    let required = |key: &str| -> Result<String, NsError> {
        values
            .get(key)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| NsError::Missing(key.to_string()))
    };

    let name = required("NAME")?;
    let population = required("POPULATION")?
        .parse::<u64>()
        .map_err(|err| NsError::Parse(format!("invalid POPULATION: {err}")))?;

    Ok(NationSummary {
        name,
        fullname: required("FULLNAME").unwrap_or_default(),
        region: required("REGION").unwrap_or_default(),
        population,
        category: required("CATEGORY").unwrap_or_default(),
        motto: required("MOTTO").unwrap_or_default(),
        currency: required("CURRENCY").unwrap_or_default(),
        flag: required("FLAG").unwrap_or_default(),
    })
}

/// A transparent, editable starting point for a nation's committed force.
///
/// This is a roleplay aid, not a NationStates military statistic: one troop per
/// 1,000 residents, clamped to a sane range. The user can and should override it.
pub fn suggested_forces(population: u64) -> u32 {
    (population / 1_000).clamp(1, 10_000_000) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<NATION id="testlandia">
  <NAME>Testlandia</NAME>
  <FULLNAME>The Hive Mind of Testlandia</FULLNAME>
  <REGION>The Pacific</REGION>
  <POPULATION>1250000</POPULATION>
  <CATEGORY>Corporate Police State</CATEGORY>
  <MOTTO>We are watching.</MOTTO>
  <CURRENCY>testudo</CURRENCY>
  <FLAG>https://www.nationstates.net/images/flags/Testlandia.svg</FLAG>
</NATION>"#;

    #[test]
    fn parses_a_nation_response() {
        let nation = parse_nation_xml(SAMPLE).unwrap();
        assert_eq!(nation.name, "Testlandia");
        assert_eq!(nation.fullname, "The Hive Mind of Testlandia");
        assert_eq!(nation.region, "The Pacific");
        assert_eq!(nation.population, 1_250_000);
        assert_eq!(nation.category, "Corporate Police State");
        assert_eq!(nation.motto, "We are watching.");
        assert_eq!(nation.currency, "testudo");
        assert!(nation.flag.ends_with("Testlandia.svg"));
    }

    #[test]
    fn missing_required_fields_are_errors() {
        let xml = "<NATION><NAME>Testlandia</NAME></NATION>";
        assert!(matches!(
            parse_nation_xml(xml),
            Err(NsError::Missing(field)) if field == "POPULATION"
        ));
    }

    #[test]
    fn nation_names_are_normalized_and_checked() {
        assert_eq!(normalize_nation("  Testlandia  ").unwrap(), "Testlandia");
        assert_eq!(normalize_nation("The Grendels").unwrap(), "The_Grendels");
        assert!(matches!(normalize_nation("   "), Err(NsError::EmptyNation)));
        assert!(matches!(
            normalize_nation("bad\nname"),
            Err(NsError::InvalidNation)
        ));
    }

    #[test]
    fn force_suggestion_is_bounded_and_monotonic() {
        assert_eq!(suggested_forces(0), 1);
        assert_eq!(suggested_forces(999), 1);
        assert_eq!(suggested_forces(1_000), 1);
        assert_eq!(suggested_forces(1_250_000), 1_250);
        assert!(suggested_forces(2_000_000) > suggested_forces(1_000_000));
    }

    #[test]
    fn user_agent_identifies_the_tool() {
        let ua = user_agent();
        assert!(ua.starts_with("Drumfire/"));
        assert!(ua.contains("contact:"));
    }

    #[test]
    fn xml_entities_are_unescaped() {
        let xml = SAMPLE.replace("We are watching.", "We &amp; watch");
        let nation = parse_nation_xml(&xml).unwrap();
        assert_eq!(nation.motto, "We & watch");
    }

    #[test]
    fn gate_blocks_after_the_limit_and_resets() {
        let mut gate = Gate::new();
        for _ in 0..DEFAULT_LIMIT {
            gate.acquire().unwrap();
        }
        assert!(matches!(gate.acquire(), Err(NsError::RateLimited { .. })));

        gate.reset_at = Instant::now() - Duration::from_millis(1);
        assert!(gate.acquire().is_ok());
    }

    #[test]
    fn gate_observes_server_headers() {
        let mut gate = Gate::new();
        let mut headers = HeaderMap::new();
        headers.insert("ratelimit-policy", HeaderValue::from_static("10;w=5"));
        headers.insert("ratelimit-limit", HeaderValue::from_static("10"));
        headers.insert("ratelimit-remaining", HeaderValue::from_static("3"));
        headers.insert("ratelimit-reset", HeaderValue::from_static("4"));
        gate.observe(&headers);

        assert_eq!(gate.limit, 10);
        assert_eq!(gate.remaining, 3);
        assert_eq!(gate.window, Duration::from_secs(5));
    }
}
