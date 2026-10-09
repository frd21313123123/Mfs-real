use crate::{
    atc::{RadioLine, Runways},
    engine::Flight,
    geo::{KNOT, Position, bearing, distance, forward, heading_error},
};
use anyhow::{Result, ensure};
use std::collections::BTreeMap;
#[derive(Clone, Debug)]
pub struct FlightClearance {
    pub heading: f64,
    pub altitude: f64,
    pub speed: f64,
    pub phase: String,
}
#[derive(Clone, Debug)]
pub struct Controlled {
    pub clearance: FlightClearance,
    pub last_instruction: f64,
    pub owns_runway: bool,
    pub hold_until: f64,
    pub completed: bool,
    pub go_arounds: usize,
    pub go_heading: Option<f64>,
    pub go_altitude: f64,
}
pub struct Director {
    pub frequencies: BTreeMap<String, f64>,
    pub runway_id: String,
    pub airport_position: Option<Position>,
    pub elevation: f64,
    pub separation: f64,
    pub vertical_sep: f64,
    pub tracks: BTreeMap<String, Controlled>,
    pub lines: Vec<RadioLine>,
    pub arrivals: Vec<Flight>,
    pub clock: f64,
    pub player_position: Option<Position>,
    pub approach_heading: Option<f64>,
}
impl Director {
    pub fn new(
        frequencies: BTreeMap<String, f64>,
        runway_id: String,
        airport_position: Option<Position>,
    ) -> Self {
        Self {
            frequencies,
            runway_id,
            elevation: airport_position.map_or(0., |p| p.alt_ft),
            airport_position,
            separation: 5500.,
            vertical_sep: 1000.,
            tracks: BTreeMap::new(),
            lines: vec![],
            arrivals: vec![],
            clock: 0.,
            player_position: None,
            approach_heading: None,
        }
    }
    fn radio(&mut self, id: &str, station: &str, instruction: &str, readback: &str) {
        let Some(mhz) = self.frequencies.get(station).copied() else {
            return;
        };
        self.lines.push(RadioLine {
            station: station.into(),
            speaker: station.to_uppercase(),
            message: format!("{id}, {instruction}."),
            mhz: Some(mhz),
            kind: "atc".into(),
        });
        if !readback.is_empty() {
            self.lines.push(RadioLine {
                station: station.into(),
                speaker: id.into(),
                message: format!("{readback}, {id}."),
                mhz: Some(mhz),
                kind: "readback".into(),
            });
        }
    }
    pub fn register(&mut self, f: &Flight) {
        if self.tracks.contains_key(&f.id) {
            return;
        }
        self.tracks.insert(
            f.id.clone(),
            Controlled {
                clearance: FlightClearance {
                    heading: f.position.heading,
                    altitude: f.position.alt_ft,
                    speed: f.position.speed_kt.max(130.),
                    phase: "enroute".into(),
                },
                last_instruction: -1e6,
                owns_runway: false,
                hold_until: 0.,
                completed: false,
                go_arounds: 0,
                go_heading: None,
                go_altitude: 0.,
            },
        );
        self.radio(
            &f.id,
            if f.arrival && self.frequencies.contains_key("approach") {
                "approach"
            } else {
                "center"
            },
            "radar contact, maintain present altitude",
            "maintaining present altitude",
        );
    }
    pub fn forget(&mut self, id: &str, keep_runway: bool, runways: &mut Runways) {
        if let Some(t) = self.tracks.remove(id)
            && t.owns_runway
            && !keep_runway
        {
            runways.release(&self.runway_id, id);
        }
    }
    pub fn close(&mut self, runways: &mut Runways) {
        let ids: Vec<_> = self.tracks.keys().cloned().collect();
        for id in ids {
            self.forget(&id, false, runways);
        }
        self.lines.clear();
    }
    fn guidance(
        &mut self,
        f: &Flight,
        others: &[Flight],
        runways: &mut Runways,
    ) -> Result<FlightClearance> {
        let current = f.position;
        let mut dist = distance(current, f.destination);
        let mut heading = bearing(current, f.destination);
        let mut speed = current.speed_kt.clamp(180., 420.);
        let mut phase = "enroute";
        let mut altitude = current.alt_ft.max(1500.);
        let mut station = "center";
        let mut track = self.tracks[&f.id].clone();
        if let Some(apt) = self.airport_position.filter(|_| !self.runway_id.is_empty()) {
            dist = distance(current, apt);
            if !f.arrival && dist < 40000. {
                phase = "departure";
                station = if self.frequencies.contains_key("departure") {
                    "departure"
                } else {
                    "center"
                };
                altitude = current.alt_ft.max(self.elevation + 8000.);
                speed = if dist < 10000. {
                    210.
                } else if dist < 30000. {
                    270.
                } else {
                    310.
                };
            }
            if f.arrival {
                heading = bearing(current, apt);
                if dist <= 45000. {
                    phase = "approach";
                    station = "approach";
                    altitude = self.elevation + (dist * 0.0524 / 0.3048).clamp(70., 7500.);
                    speed = if dist > 14000. {
                        210.
                    } else if dist > 7000. {
                        165.
                    } else {
                        140.
                    };
                }
                if dist <= 9000. {
                    if !track.owns_runway && self.clock >= track.hold_until {
                        if runways.reserve(&self.runway_id, &f.id, "arrival", self.clock)? {
                            track.owns_runway = true;
                            track.go_heading = None;
                            let message = format!(
                                "runway {}, cleared to land",
                                self.runway_id.rsplit('/').next().unwrap()
                            );
                            self.radio(&f.id, "tower", &message, &message);
                        } else {
                            track.go_arounds += 1;
                            track.hold_until = self.clock + 90.;
                            track.go_heading = Some((current.heading + 45.).rem_euclid(360.));
                            track.go_altitude =
                                (current.alt_ft + 1500.).max(self.elevation + 4000.);
                            self.radio(
                                &f.id,
                                "tower",
                                "go around, runway occupied, climb 4000 feet",
                                "going around, 4000 feet",
                            );
                        }
                    }
                    if !track.owns_runway {
                        phase = "go-around";
                        station = "approach";
                        altitude = if track.go_altitude > 0. {
                            track.go_altitude
                        } else {
                            self.elevation + 4000.
                        };
                        heading = track.go_heading.unwrap_or(current.heading);
                        speed = 195.;
                    }
                }
                if track.owns_runway && dist < 100. && current.alt_ft <= self.elevation + 180. {
                    track.completed = true;
                    phase = "landed";
                    altitude = self.elevation;
                    speed = 120.;
                }
            }
        }
        if !track.owns_runway && self.clock < track.hold_until && track.go_heading.is_some() {
            phase = "go-around";
            station = "approach";
            heading = track.go_heading.unwrap();
            altitude = track.go_altitude;
            speed = 195.;
        }
        if self.player_position.is_some_and(|p| {
            phase != "landed"
                && distance(current, p) < self.separation
                && (current.alt_ft - p.alt_ft).abs() < self.vertical_sep
        }) {
            heading = (current.heading + 40.).rem_euclid(360.);
            altitude = altitude.max(current.alt_ft + 1400.);
            speed = speed.min((current.speed_kt - 35.).max(140.));
            phase = "vector";
            station = if dist < 45000. { "approach" } else { "center" };
        }
        if phase != "landed" {
            for other in others {
                if other.id >= f.id {
                    continue;
                }
                if distance(current, other.position) < self.separation
                    && (current.alt_ft - other.position.alt_ft).abs() < self.vertical_sep
                {
                    heading = (current.heading + 35.).rem_euclid(360.);
                    altitude = altitude.max(current.alt_ft + 1100.);
                    speed = speed.min((current.speed_kt - 25.).max(140.));
                    phase = "vector";
                    station = if dist < 45000. { "approach" } else { "center" };
                    break;
                }
            }
        }
        let clearance = FlightClearance {
            heading,
            altitude,
            speed,
            phase: phase.into(),
        };
        let old = &track.clearance;
        let changed = old.phase != phase
            || heading_error(heading, old.heading).abs() >= 8.
            || (altitude - old.altitude).abs() >= 750.
            || (speed - old.speed).abs() >= 20.;
        if changed && (self.clock - track.last_instruction >= 12. || old.phase != phase) {
            track.last_instruction = self.clock;
            let desc = format!(
                "turn heading {:03}, maintain {:.0} feet, speed {:.0} knots",
                heading.round() as u32 % 360,
                (altitude / 100.).round() * 100.,
                speed.round()
            );
            self.radio(&f.id, station, &desc, &desc);
        }
        track.clearance = clearance.clone();
        self.tracks.insert(f.id.clone(), track);
        Ok(clearance)
    }
    pub fn move_flight(f: &mut Flight, c: &FlightClearance, dt: f64) {
        let p = f.position;
        let turn = heading_error(c.heading, p.heading).clamp(-3. * dt, 3. * dt);
        let heading = (p.heading + turn).rem_euclid(360.);
        let speed = c
            .speed
            .clamp((p.speed_kt - 3. * dt).max(0.), p.speed_kt + 3. * dt)
            .max(115.);
        let delta = (c.altitude - p.alt_ft).clamp(-1900. * dt / 60., 1700. * dt / 60.);
        let moved = forward(p, heading, speed * KNOT * dt);
        f.position = Position {
            alt_ft: (p.alt_ft + delta).max(0.),
            heading,
            speed_kt: speed,
            on_ground: false,
            bank: (turn / dt.max(1e-6) * 6.).clamp(-25., 25.),
            pitch: (delta / dt.max(1e-6) / 10.).clamp(-6., 6.),
            ..moved
        };
    }
    pub fn tick(
        &mut self,
        aircraft: &mut BTreeMap<String, Flight>,
        dt: f64,
        runways: &mut Runways,
    ) -> Result<()> {
        ensure!(dt > 0. && dt <= 10., "air tick dt must be 0..10");
        self.clock += dt;
        for f in aircraft.values() {
            self.register(f);
        }
        let missing: Vec<_> = self
            .tracks
            .keys()
            .filter(|id| !aircraft.contains_key(*id))
            .cloned()
            .collect();
        for id in missing {
            self.forget(&id, false, runways);
        }
        let snapshot: Vec<_> = aircraft.values().cloned().collect();
        for f in &snapshot {
            let clearance = self.guidance(f, &snapshot, runways)?;
            let actual = aircraft.get_mut(&f.id).unwrap();
            Self::move_flight(actual, &clearance, dt);
            if self.tracks[&f.id].completed {
                self.arrivals.push(aircraft.remove(&f.id).unwrap());
            }
        }
        Ok(())
    }
}
