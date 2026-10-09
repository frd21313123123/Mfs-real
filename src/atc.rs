use crate::{airport::Edge, engine::GroundAircraft, geo::Position};
use anyhow::{Result, ensure};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, Serialize)]
pub struct Reservation {
    pub aircraft_id: String,
    pub operation: String,
    pub since: f64,
}
#[derive(Clone, Debug, Default)]
pub struct Runways {
    pub reservations: BTreeMap<String, Reservation>,
}
impl Runways {
    pub fn reserve(&mut self, id: &str, aircraft: &str, operation: &str, now: f64) -> Result<bool> {
        ensure!(!id.is_empty(), "empty runway ID");
        ensure!(
            ["arrival", "departure", "crossing"].contains(&operation),
            "unknown runway operation"
        );
        if let Some(owner) = self.reservations.get(id) {
            return Ok(owner.aircraft_id == aircraft);
        }
        self.reservations.insert(
            id.into(),
            Reservation {
                aircraft_id: aircraft.into(),
                operation: operation.into(),
                since: now,
            },
        );
        Ok(true)
    }
    pub fn release(&mut self, id: &str, aircraft: &str) -> bool {
        if self
            .reservations
            .get(id)
            .is_some_and(|r| r.aircraft_id == aircraft)
        {
            self.reservations.remove(id);
            true
        } else {
            false
        }
    }
    pub fn free(&mut self, id: &str) {
        self.reservations.retain(|_, r| r.aircraft_id != id);
    }
    pub fn available(&self, id: &str, aircraft: &str) -> bool {
        self.reservations
            .get(id)
            .is_none_or(|r| r.aircraft_id == aircraft)
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct RadioLine {
    pub station: String,
    pub speaker: String,
    pub message: String,
    pub mhz: Option<f64>,
    pub kind: String,
}
#[derive(Clone, Debug)]
pub struct Clearance {
    pub action: String,
    pub mandatory: Vec<String>,
    pub next_stage: String,
}
pub fn normalize(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
pub fn said(s: &str, phrases: &[&str]) -> bool {
    let clean = format!(" {} ", normalize(s));
    phrases.iter().any(|p| clean.contains(&format!(" {p} ")))
}
pub fn readback_contains(s: &str, expected: &str) -> bool {
    let expected = expected.to_lowercase();
    if said(s, &[&expected]) {
        return true;
    }
    let words = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
    ];
    let (digits, suffix) = match expected.chars().last() {
        Some('l') => (&expected[..expected.len() - 1], " left"),
        Some('r') => (&expected[..expected.len() - 1], " right"),
        Some('c') => (&expected[..expected.len() - 1], " center"),
        _ => (&expected[..], ""),
    };
    if digits.is_empty() || digits.len() > 4 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let mut aliases = vec![
        digits
            .chars()
            .map(|c| words[c.to_digit(10).unwrap() as usize])
            .collect::<Vec<_>>()
            .join(" "),
    ];
    if digits.len() == 4 && digits.ends_with("000") {
        aliases.push(format!(
            "{} thousand",
            words[digits.as_bytes()[0] as usize - b'0' as usize]
        ));
    }
    match digits {
        "18" => aliases.push("eighteen".into()),
        "24" => aliases.push("twenty four".into()),
        _ => {}
    }
    aliases.iter().any(|a| said(s, &[&format!("{a}{suffix}")]))
}
#[derive(Clone, Debug)]
pub struct Pilot {
    pub callsign: String,
    pub origin: String,
    pub destination: String,
    pub runway: String,
    pub runway_key: String,
    pub frequencies: BTreeMap<String, f64>,
    pub altitude_ft: u32,
    pub squawk: String,
    pub stage: String,
    pub pending: Option<Clearance>,
    pub com1: Option<f64>,
    pub com2: Option<f64>,
    pub transmitting: u8,
    pub actual_squawk: Option<String>,
    pub lines: Vec<RadioLine>,
    pub owns_runway: bool,
}
impl Pilot {
    pub fn new(
        callsign: &str,
        origin: &str,
        destination: &str,
        frequencies: BTreeMap<String, f64>,
        runway: &str,
    ) -> Result<Self> {
        let (callsign, origin, destination) = (
            callsign.trim().to_uppercase(),
            origin.trim().to_uppercase(),
            destination.trim().to_uppercase(),
        );
        ensure!(
            (3..=12).contains(&callsign.len())
                && callsign
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-'),
            "Invalid callsign"
        );
        for id in [&origin, &destination] {
            ensure!(
                id.len() == 4 && id.chars().all(|c| c.is_ascii_alphanumeric()),
                "Airport must be ICAO code"
            );
        }
        ensure!(!runway.trim().is_empty(), "Runway required");
        let runway = runway.trim().to_uppercase();
        Ok(Self {
            callsign,
            origin,
            destination,
            runway_key: runway.clone(),
            runway,
            frequencies: frequencies
                .into_iter()
                .filter(|(_, v)| (118. ..=136.99).contains(v))
                .collect(),
            altitude_ft: 5000,
            squawk: "4101".into(),
            stage: "filed".into(),
            pending: None,
            com1: None,
            com2: None,
            transmitting: 1,
            actual_squawk: None,
            lines: vec![],
            owns_runway: false,
        })
    }
    pub fn tuned(&self) -> Option<f64> {
        if self.transmitting == 2 {
            self.com2
        } else {
            self.com1
        }
    }
    pub fn station(&self) -> Option<String> {
        let tuned = self.tuned()?;
        self.frequencies
            .iter()
            .find(|(_, f)| (**f - tuned).abs() <= 0.001)
            .map(|(s, _)| s.clone())
    }
    pub fn say(&mut self, station: &str, text: &str, kind: &str) -> RadioLine {
        let line = RadioLine {
            station: station.into(),
            speaker: format!("{} {}", self.origin, station.to_uppercase()),
            message: format!("{}, {}", self.callsign, text),
            mhz: self.frequencies.get(station).copied(),
            kind: kind.into(),
        };
        self.lines.push(line.clone());
        line
    }
    fn clear(
        &mut self,
        station: &str,
        text: String,
        action: &str,
        mandatory: Vec<String>,
        next: &str,
    ) -> RadioLine {
        self.pending = Some(Clearance {
            action: action.into(),
            mandatory,
            next_stage: next.into(),
        });
        self.say(station, &text, "clearance")
    }
    pub fn handoff(&mut self, source: &str, target: &str) -> RadioLine {
        if let Some(freq) = self.frequencies.get(target) {
            self.say(source, &format!("contact {target} {freq:.3}."), "handoff")
        } else {
            self.say(
                source,
                "remain on this frequency, next sector unavailable in local data.",
                "info",
            )
        }
    }
    fn expected(&self) -> Option<&str> {
        let role = match self.stage.as_str() {
            "filed" => "delivery",
            "cleared" | "pushed" | "landed" => "ground",
            "taxi" | "departure" | "approach" => "tower",
            "airborne" => "departure",
            "cruise" => "center",
            _ => "approach",
        };
        if self.frequencies.contains_key(role) {
            return Some(role);
        }
        let substitute = match role {
            "delivery" => "ground",
            "departure" | "center" => "approach",
            "ground" => "tower",
            _ => return None,
        };
        self.frequencies
            .contains_key(substitute)
            .then_some(substitute)
    }
    pub fn release(&mut self, runways: &mut Runways) {
        if self.owns_runway {
            runways.release(&self.runway_key, &self.callsign);
            self.owns_runway = false;
        }
    }
    pub fn transmit(&mut self, text: &str, runways: &mut Runways) -> Result<RadioLine> {
        ensure!(!text.trim().is_empty(), "Empty radio transmission");
        let active = self.station();
        self.lines.push(RadioLine {
            station: active.clone().unwrap_or("unknown".into()),
            speaker: self.callsign.clone(),
            message: text.into(),
            mhz: self.tuned(),
            kind: "pilot".into(),
        });
        let Some(active) = active else {
            return Ok(self.say(
                "radio",
                "no simulated controller on this frequency.",
                "unavailable",
            ));
        };
        if let Some(pending) = self.pending.clone() {
            if said(text, &["say again", "repeat"]) {
                return Ok(self.say(
                    &active,
                    &format!(
                        "repeat clearance: {}, read back {}.",
                        pending.action,
                        pending.mandatory.join(", ")
                    ),
                    "repeat",
                ));
            }
            let missing: Vec<_> = pending
                .mandatory
                .iter()
                .filter(|t| !readback_contains(text, t))
                .cloned()
                .collect();
            if !missing.is_empty() {
                return Ok(self.say(
                    &active,
                    &format!(
                        "readback incorrect; missing {}. Say again.",
                        missing.join(", ")
                    ),
                    "correction",
                ));
            }
            self.pending = None;
            self.stage = pending.next_stage;
            return Ok(self.say(&active, "readback correct.", "ack"));
        }
        if self.stage == "landed"
            && ["tower", "ground"].contains(&active.as_str())
            && said(text, &["vacated", "clear of runway"])
        {
            self.release(runways);
            return Ok(self.handoff(&active, "ground"));
        }
        if let Some(expected) = self.expected().map(str::to_owned)
            && expected != active
        {
            return Ok(self.handoff(&active, &expected));
        }
        let line = match self.stage.as_str() {
            "filed" if said(text, &["clearance", "clearance delivery", "cleared"]) => {
                if !["delivery", "ground"].contains(&active.as_str()) {
                    self.say(
                        &active,
                        "contact clearance delivery or ground.",
                        "unavailable",
                    )
                } else {
                    self.clear(
                        &active,
                        format!(
                            "IFR clearance to {}, climb initially {} feet, squawk {}.",
                            self.destination, self.altitude_ft, self.squawk
                        ),
                        "IFR",
                        vec![self.altitude_ft.to_string(), self.squawk.clone()],
                        "cleared",
                    )
                }
            }
            "cleared" if said(text, &["pushback", "push back"]) => self.clear(
                &active,
                "pushback approved, report ready to taxi.".into(),
                "pushback",
                vec!["approved".into()],
                "pushed",
            ),
            "cleared" | "pushed" if said(text, &["taxi"]) => self.clear(
                &active,
                format!("taxi to runway {}, hold short.", self.runway),
                "taxi",
                vec![self.runway.to_lowercase(), "hold".into()],
                "taxi",
            ),
            "taxi"
                if active == "tower"
                    && said(text, &["ready", "departure", "takeoff", "take off"]) =>
            {
                if self
                    .actual_squawk
                    .as_ref()
                    .is_some_and(|s| s != &self.squawk)
                {
                    self.say(
                        &active,
                        &format!("check transponder, squawk {}.", self.squawk),
                        "correction",
                    )
                } else if !runways.reserve(&self.runway_key, &self.callsign, "departure", 0.)? {
                    self.say(
                        &active,
                        &format!("hold short runway {}, traffic on runway.", self.runway),
                        "hold",
                    )
                } else {
                    self.owns_runway = true;
                    self.clear(
                        &active,
                        format!("runway {}, cleared for takeoff.", self.runway),
                        "takeoff",
                        vec![self.runway.to_lowercase(), "takeoff".into()],
                        "departure",
                    )
                }
            }
            "departure" if said(text, &["airborne", "positive climb"]) => {
                self.release(runways);
                self.stage = "airborne".into();
                self.handoff(&active, "departure")
            }
            "airborne" if said(text, &["passing", "climb", "with you"]) => self.clear(
                &active,
                "radar contact, climb and maintain flight level 300.".into(),
                "climb",
                vec!["300".into()],
                "cruise",
            ),
            "cruise" if said(text, &["descent", "approach", "inbound"]) => {
                self.stage = "inbound".into();
                self.handoff(&active, "approach")
            }
            "inbound" if said(text, &["inbound", "approach", "vectors"]) => self.clear(
                &active,
                format!(
                    "descend {} feet, cleared approach runway {}.",
                    self.altitude_ft, self.runway
                ),
                "approach",
                vec![self.altitude_ft.to_string(), self.runway.to_lowercase()],
                "approach",
            ),
            "approach" if active == "tower" && said(text, &["land", "landing", "final"]) => {
                if !runways.reserve(&self.runway_key, &self.callsign, "arrival", 0.)? {
                    self.say(
                        &active,
                        "go around, runway occupied. Maintain runway heading.",
                        "go-around",
                    )
                } else {
                    self.owns_runway = true;
                    self.clear(
                        &active,
                        format!("runway {}, cleared to land.", self.runway),
                        "land",
                        vec![self.runway.to_lowercase(), "land".into()],
                        "landed",
                    )
                }
            }
            _ if said(text, &["say again", "repeat"]) => {
                self.say(&active, "say request again.", "repeat")
            }
            _ => self.say(
                &active,
                &format!("unable, check request for phase {}.", self.stage),
                "unable",
            ),
        };
        Ok(line)
    }
}
#[derive(Clone, Debug, Default)]
pub struct GroundAtc {
    pub frequencies: BTreeMap<String, f64>,
    pub grants: BTreeMap<String, BTreeSet<String>>,
    pub waiting: BTreeSet<String>,
    pub lines: Vec<RadioLine>,
}
impl GroundAtc {
    pub fn new(frequencies: BTreeMap<String, f64>) -> Self {
        Self {
            frequencies,
            ..Self::default()
        }
    }
    fn exchange(&mut self, id: &str, role: &str, request: &str, response: &str) {
        let mhz = self.frequencies.get(role).copied();
        for (speaker, message, kind) in [
            (id.to_string(), format!("{role}, {id}, {request}"), "ai"),
            (role.to_uppercase(), format!("{id}, {response}"), "atc"),
            (id.to_string(), format!("{response}, {id}"), "readback"),
        ] {
            self.lines.push(RadioLine {
                station: role.into(),
                speaker,
                message,
                mhz,
                kind: kind.into(),
            });
        }
    }
    pub fn allow(&mut self, plane: &GroundAircraft, edge: &Edge, runways: &Runways) -> bool {
        let role = if edge.runway.is_empty() {
            "ground"
        } else {
            "tower"
        };
        if !self.frequencies.contains_key(role) {
            return false;
        }
        if edge.kind == "pushback"
            && !self
                .grants
                .get(&plane.id)
                .is_some_and(|g| g.contains("pushback"))
        {
            self.exchange(&plane.id, "ground", "request pushback", "pushback approved");
            self.grants
                .entry(plane.id.clone())
                .or_default()
                .insert("pushback".into());
        }
        if edge.runway.is_empty() {
            if edge.kind != "pushback"
                && !self
                    .grants
                    .get(&plane.id)
                    .is_some_and(|g| g.contains("taxi"))
            {
                self.exchange(
                    &plane.id,
                    "ground",
                    "request taxi",
                    "taxi approved, hold short of active runway",
                );
                self.grants
                    .entry(plane.id.clone())
                    .or_default()
                    .insert("taxi".into());
            }
            return true;
        }
        if !runways.available(&edge.runway, &plane.id) {
            if self.waiting.insert(plane.id.clone()) {
                self.lines.push(RadioLine {
                    station: "tower".into(),
                    speaker: "TOWER".into(),
                    message: format!(
                        "{}, hold short runway {}, landing or departing traffic.",
                        plane.id,
                        edge.runway.rsplit('/').next().unwrap()
                    ),
                    mhz: self.frequencies.get("tower").copied(),
                    kind: "hold".into(),
                });
            }
            return false;
        }
        self.waiting.remove(&plane.id);
        let op = if plane.arrival { "landing" } else { "takeoff" };
        if !self.grants.get(&plane.id).is_some_and(|g| g.contains(op)) {
            let label = edge.runway.rsplit('/').next().unwrap();
            self.exchange(
                &plane.id,
                "tower",
                if plane.arrival {
                    "established on final"
                } else {
                    "ready for departure"
                },
                &format!(
                    "runway {label}, cleared {}",
                    if plane.arrival {
                        "to land"
                    } else {
                        "for takeoff"
                    }
                ),
            );
            self.grants
                .entry(plane.id.clone())
                .or_default()
                .insert(op.into());
        }
        true
    }
    pub fn forget(&mut self, id: &str) {
        self.grants.remove(id);
        self.waiting.remove(id);
    }
}
#[derive(Default)]
pub struct PlayerMonitor {
    pub field_alt_ft: f64,
    captured: Option<f64>,
    deviation: Option<f64>,
    last_warning: Option<f64>,
}
impl PlayerMonitor {
    pub fn new(field_alt_ft: f64) -> Self {
        Self {
            field_alt_ft,
            ..Self::default()
        }
    }
    pub fn tick(
        &mut self,
        pilot: &mut Pilot,
        position: Option<Position>,
        now: f64,
        runways: &mut Runways,
    ) -> Option<RadioLine> {
        let pos = position?;
        if pilot.stage == "departure" && !pos.on_ground && pos.alt_ft > self.field_alt_ft + 350. {
            pilot.release(runways);
            pilot.stage = "airborne".into();
            return Some(pilot.handoff("tower", "departure"));
        }
        let target = match pilot.stage.as_str() {
            "cruise" => 30000.,
            "approach" => pilot.altitude_ft as f64,
            _ => {
                self.captured = None;
                self.deviation = None;
                return None;
            }
        };
        let diff = (pos.alt_ft - target).abs();
        if diff < 300. {
            self.captured = Some(target);
            self.deviation = None;
            return None;
        }
        if self.captured != Some(target) {
            return None;
        }
        if diff < 550. {
            self.deviation = None;
            return None;
        }
        let started = *self.deviation.get_or_insert(now);
        if now - started >= 20. && self.last_warning.is_none_or(|t| now - t >= 90.) {
            self.last_warning = Some(now);
            let station = pilot.station().unwrap_or("center".into());
            if pilot.frequencies.contains_key(&station) {
                return Some(pilot.say(
                    &station,
                    &format!("check altitude, maintain {target:.0} feet."),
                    "warning",
                ));
            }
        }
        None
    }
}
