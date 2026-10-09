use crate::{
    airport::Airport,
    atc::{GroundAtc, Runways},
    bridge::Bridge,
    config::Settings,
    director::Director,
    geo::{FT, KNOT, Position, bearing, blend, distance, forward},
    sources::{Model, Observation, choose},
};
use anyhow::{Result, ensure};
use rand::{Rng, SeedableRng, rngs::StdRng};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Phase {
    Parked,
    Pushback,
    TaxiOut,
    HoldShort,
    Takeoff,
    Airborne,
    Landing,
    Rollout,
    TaxiIn,
    Complete,
}
#[derive(Clone, Debug)]
pub struct GroundAircraft {
    pub id: String,
    pub model_title: String,
    pub route: Vec<String>,
    pub position: Position,
    pub arrival: bool,
    pub cursor: usize,
    pub speed_mps: f64,
    pub phase: Phase,
    pub locked_edge: Option<(String, String)>,
    pub owns_runway: String,
}
pub struct Ground {
    pub airport: Airport,
    pub max_ground: usize,
    pub aircraft: BTreeMap<String, GroundAircraft>,
    pub edge_owners: BTreeMap<(String, String), String>,
    pub node_owners: BTreeMap<String, String>,
    pub time: f64,
}
fn edge_key(a: &str, b: &str) -> (String, String) {
    if a < b {
        (a.into(), b.into())
    } else {
        (b.into(), a.into())
    }
}
impl Ground {
    pub fn new(airport: Airport, max_ground: usize) -> Self {
        Self {
            airport,
            max_ground,
            aircraft: BTreeMap::new(),
            edge_owners: BTreeMap::new(),
            node_owners: BTreeMap::new(),
            time: 0.,
        }
    }
    pub fn add(
        &mut self,
        id: &str,
        title: &str,
        start: &str,
        end: &str,
        arrival: bool,
    ) -> Result<()> {
        ensure!(
            self.aircraft.len() < self.max_ground,
            "ground capacity exhausted"
        );
        ensure!(!self.aircraft.contains_key(id), "duplicate aircraft");
        let route = self.airport.route(start, end, &BTreeSet::new())?;
        ensure!(route.len() >= 2, "no route {start} -> {end}");
        self.aircraft.insert(
            id.into(),
            GroundAircraft {
                id: id.into(),
                model_title: title.into(),
                position: self.airport.nodes[start].position(),
                route,
                arrival,
                cursor: 0,
                speed_mps: 0.,
                phase: if arrival {
                    Phase::Landing
                } else {
                    Phase::Parked
                },
                locked_edge: None,
                owns_runway: String::new(),
            },
        );
        Ok(())
    }
    fn free_edge(&mut self, p: &mut GroundAircraft) {
        if let Some((src, dst)) = p.locked_edge.take() {
            let key = edge_key(&src, &dst);
            if self.edge_owners.get(&key) == Some(&p.id) {
                self.edge_owners.remove(&key);
            }
            if self.node_owners.get(&dst) == Some(&p.id) {
                self.node_owners.remove(&dst);
            }
        }
    }
    pub fn remove(&mut self, id: &str, runways: &mut Runways) {
        if let Some(mut p) = self.aircraft.remove(id) {
            self.free_edge(&mut p);
            runways.free(id);
        }
    }
    pub fn tick(
        &mut self,
        dt: f64,
        runways: &mut Runways,
        mut atc: Option<&mut GroundAtc>,
    ) -> Result<()> {
        ensure!(dt > 0. && dt <= 10., "ground tick dt must be 0..10");
        self.time += dt;
        let ids: Vec<_> = self.aircraft.keys().cloned().collect();
        for id in ids {
            let mut plane = self.aircraft.remove(&id).unwrap();
            let result = if ![Phase::Complete, Phase::Airborne].contains(&plane.phase) {
                self.advance(&mut plane, dt, runways, atc.as_deref_mut())
            } else {
                Ok(())
            };
            self.aircraft.insert(id, plane);
            result?;
        }
        Ok(())
    }
    fn advance(
        &mut self,
        p: &mut GroundAircraft,
        dt: f64,
        runways: &mut Runways,
        atc: Option<&mut GroundAtc>,
    ) -> Result<()> {
        if p.cursor >= p.route.len() - 1 {
            p.phase = if p.arrival {
                Phase::Complete
            } else {
                Phase::Airborne
            };
            return Ok(());
        }
        let (src, dst) = (p.route[p.cursor].clone(), p.route[p.cursor + 1].clone());
        let edge = self.airport.edges[&(src.clone(), dst.clone())].clone();
        if atc.is_some_and(|a| !a.allow(p, &edge, runways)) {
            p.speed_mps = (p.speed_mps - 2. * dt).max(0.);
            p.phase = Phase::HoldShort;
            return Ok(());
        }
        let dest = self.airport.nodes[&dst].position();
        let key = edge_key(&src, &dst);
        let blocked = self.edge_owners.get(&key).is_some_and(|s| s != &p.id)
            || self.node_owners.get(&dst).is_some_and(|s| s != &p.id)
            || (p.locked_edge.is_none()
                && self.aircraft.values().any(|o| {
                    ![Phase::Airborne, Phase::Complete].contains(&o.phase)
                        && distance(o.position, dest) < 18.
                }));
        if blocked {
            p.speed_mps = (p.speed_mps - 2. * dt).max(0.);
            if self.airport.nodes[&src].kind == "hold" {
                p.phase = Phase::HoldShort;
            }
            return Ok(());
        }
        if !edge.runway.is_empty() {
            if !runways.reserve(
                &edge.runway,
                &p.id,
                if p.arrival { "arrival" } else { "departure" },
                self.time,
            )? {
                p.speed_mps = (p.speed_mps - 2. * dt).max(0.);
                p.phase = Phase::HoldShort;
                return Ok(());
            }
            p.owns_runway = edge.runway.clone();
        }
        if p.locked_edge.is_none() {
            p.locked_edge = Some((src.clone(), dst.clone()));
            self.edge_owners.insert(key, p.id.clone());
            self.node_owners.insert(dst.clone(), p.id.clone());
        }
        p.phase = if edge.kind == "pushback" {
            Phase::Pushback
        } else if !edge.runway.is_empty() {
            if p.arrival {
                Phase::Rollout
            } else {
                Phase::Takeoff
            }
        } else if p.arrival {
            Phase::TaxiIn
        } else {
            Phase::TaxiOut
        };
        let dist = distance(p.position, dest);
        let safe = (edge.speed_kt * KNOT).min((3. * dist).sqrt()).max(0.8);
        p.speed_mps = safe.min(p.speed_mps + dt);
        let mut step = dist.min(p.speed_mps * dt);
        let heading = bearing(p.position, dest);
        for other in self
            .aircraft
            .values()
            .filter(|o| ![Phase::Airborne, Phase::Complete].contains(&o.phase))
        {
            let current = distance(p.position, other.position);
            if current >= 20. && distance(forward(p.position, heading, step), other.position) < 20.
            {
                let (mut low, mut high) = (0., step);
                for _ in 0..16 {
                    let mid = (low + high) / 2.;
                    if distance(forward(p.position, heading, mid), other.position) >= 20. {
                        low = mid
                    } else {
                        high = mid
                    }
                }
                step = low;
            } else if current < 20.
                && distance(forward(p.position, heading, step), other.position) <= current
            {
                step = 0.;
            }
        }
        if step < 0.02 {
            p.speed_mps = 0.;
            return Ok(());
        }
        let heading = if edge.kind == "pushback" {
            (heading + 180.).rem_euclid(360.)
        } else {
            heading
        };
        if step >= dist - 0.15 {
            p.position = Position {
                heading,
                speed_kt: p.speed_mps / KNOT,
                on_ground: true,
                ..dest
            };
            p.cursor += 1;
            p.speed_mps = 0.;
            self.free_edge(p);
            if !p.owns_runway.is_empty()
                && (p.cursor >= p.route.len() - 1
                    || self.airport.edges
                        [&(p.route[p.cursor].clone(), p.route[p.cursor + 1].clone())]
                        .runway
                        .is_empty())
            {
                runways.release(&p.owns_runway, &p.id);
                p.owns_runway.clear();
            }
            if p.cursor >= p.route.len() - 1 {
                p.phase = if p.arrival {
                    Phase::Complete
                } else {
                    Phase::Airborne
                };
            }
        } else {
            let moved = forward(p.position, bearing(p.position, dest), step);
            p.position = Position {
                alt_ft: p.position.alt_ft
                    + (dest.alt_ft - p.position.alt_ft) * step / dist.max(0.1),
                heading,
                speed_kt: p.speed_mps / KNOT,
                on_ground: true,
                ..moved
            };
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct LiveAircraft {
    pub id: String,
    pub callsign: String,
    pub model_title: String,
    pub position: Position,
    pub target: Position,
    pub vertical_mps: f64,
    pub seen_at: f64,
    pub predicted: bool,
}
#[derive(Clone, Debug)]
pub struct Flight {
    pub id: String,
    pub model_title: String,
    pub position: Position,
    pub destination: Position,
    pub remaining_s: f64,
    pub arrival: bool,
}
#[derive(Default, Clone, Copy, Debug, Serialize)]
pub struct Stats {
    pub live: usize,
    pub synthetic: usize,
    pub ground: usize,
    pub objects: usize,
}
pub struct Manager<B: Bridge> {
    pub settings: Settings,
    pub models: Vec<Model>,
    pub bridge: B,
    pub fleet: BTreeMap<String, LiveAircraft>,
    pub synthetic: BTreeMap<String, Flight>,
    pub ground: Option<Ground>,
    pub ground_atc: Option<GroundAtc>,
    pub director: Option<Director>,
    pub runways: Runways,
    pub spawned: BTreeSet<String>,
    rng: StdRng,
    index: u64,
}
impl<B: Bridge> Manager<B> {
    pub fn new(
        settings: Settings,
        models: Vec<Model>,
        bridge: B,
        ground: Option<Ground>,
    ) -> Result<Self> {
        settings.validate()?;
        Ok(Self {
            settings,
            models,
            bridge,
            ground,
            fleet: BTreeMap::new(),
            synthetic: BTreeMap::new(),
            ground_atc: None,
            director: None,
            runways: Runways::default(),
            spawned: BTreeSet::new(),
            rng: StdRng::seed_from_u64(2026),
            index: 0,
        })
    }
    pub fn feed(&mut self, observations: Vec<Observation>, origin: Position, now: f64) {
        if self.settings.mode == "simulation" || !self.settings.sync_real_flights {
            return;
        }
        let mut candidates: Vec<_> = observations
            .into_iter()
            .filter(|o| {
                !o.position.on_ground
                    && o.position.validate().is_ok()
                    && o.vertical_mps.is_finite()
                    && distance(origin, o.position) <= self.settings.max_radius_km * 1000.
                    && o.observed_at > 0.
                    && (-30. ..300.).contains(&(now - o.observed_at))
            })
            .collect();
        candidates
            .sort_by(|a, b| distance(origin, a.position).total_cmp(&distance(origin, b.position)));
        for o in candidates {
            if let Some(record) = self.fleet.get_mut(&o.icao24) {
                if o.observed_at <= record.seen_at || distance(record.position, o.position) > 80000.
                {
                    continue;
                }
                record.target = o.position;
                record.seen_at = o.observed_at;
                record.vertical_mps = o.vertical_mps;
                if !o.callsign.is_empty() {
                    record.callsign = o.callsign;
                }
                record.predicted = false;
                continue;
            }
            if self.fleet.len() >= self.settings.airborne_limit {
                break;
            }
            let airline = o
                .callsign
                .chars()
                .take(3)
                .filter(|c| c.is_ascii_alphabetic())
                .collect::<String>()
                .to_uppercase();
            let Some(model) = choose(&self.models, &o.icao_type, &airline) else {
                continue;
            };
            self.fleet.insert(
                o.icao24.clone(),
                LiveAircraft {
                    id: o.icao24,
                    callsign: o.callsign,
                    model_title: model.title.clone(),
                    position: o.position,
                    target: o.position,
                    vertical_mps: o.vertical_mps,
                    seen_at: o.observed_at,
                    predicted: false,
                },
            );
        }
    }
    fn generate(&mut self, origin: Position, limit: usize, target: usize) {
        if self.models.is_empty() {
            return;
        }
        for _ in self.synthetic.len()..limit.min(target) {
            let heading = self.rng.gen_range(0. ..360.);
            let airport = self.director.as_ref().and_then(|d| d.airport_position);
            let (start, dest, speed, alt) = if let Some(apt) = airport {
                let radial = self
                    .director
                    .as_ref()
                    .and_then(|d| d.approach_heading)
                    .map_or(heading, |h| (h + 180.).rem_euclid(360.));
                let start = forward(apt, radial, self.rng.gen_range(38000. ..65000.));
                (
                    forward(start, radial + 90., self.rng.gen_range(-2500. ..2500.)),
                    apt,
                    self.rng.gen_range(190. ..245.),
                    apt.alt_ft + self.rng.gen_range(6500. ..9500.),
                )
            } else {
                (
                    forward(origin, heading, self.rng.gen_range(30000. ..75000.)),
                    forward(origin, heading + 180., self.rng.gen_range(25000. ..75000.)),
                    self.rng.gen_range(230. ..440.),
                    self.rng.gen_range(10000. ..30000.),
                )
            };
            self.index += 1;
            let id = format!("SYN-{:05}", self.index);
            let model = &self.models[self.rng.gen_range(0..self.models.len())];
            self.synthetic.insert(
                id.clone(),
                Flight {
                    id,
                    model_title: model.title.clone(),
                    position: Position {
                        alt_ft: alt,
                        heading: bearing(start, dest),
                        speed_kt: speed,
                        on_ground: false,
                        ..start
                    },
                    destination: Position {
                        alt_ft: if airport.is_some() { dest.alt_ft } else { alt },
                        ..dest
                    },
                    remaining_s: 2700.,
                    arrival: airport.is_some(),
                },
            );
        }
    }
    pub fn tick(&mut self, dt: f64, now: f64, origin: Position, target: usize) -> Result<Stats> {
        ensure!(
            dt > 0. && dt <= 10. && dt.is_finite(),
            "tick must be 0 < dt <=10"
        );
        self.fleet.retain(|_, p| now - p.seen_at <= 600.);
        for p in self.fleet.values_mut() {
            let age = now - p.seen_at;
            p.predicted = age > 150.;
            let mut moved = forward(
                p.position,
                p.position.heading,
                p.position.speed_kt.max(0.) * KNOT * dt,
            );
            moved.alt_ft += p.vertical_mps * dt / FT;
            if age < 30. && distance(moved, p.target) < 4000. {
                moved = blend(moved, p.target, (dt * 0.2).min(0.35));
            }
            p.position = moved;
        }
        if self.settings.mode != "live" {
            let available = self
                .settings
                .airborne_limit
                .saturating_sub(self.fleet.len());
            while self.synthetic.len() > available {
                let id = self.synthetic.keys().next_back().unwrap().clone();
                self.synthetic.remove(&id);
                if let Some(d) = &mut self.director {
                    d.forget(&id, false, &mut self.runways);
                }
            }
            self.generate(origin, available, target);
            if let Some(d) = &mut self.director {
                d.tick(&mut self.synthetic, dt, &mut self.runways)?;
            }
            let mut expired = vec![];
            for (id, p) in &mut self.synthetic {
                p.remaining_s -= dt;
                if p.remaining_s <= 0. {
                    expired.push(id.clone());
                    continue;
                }
                if self.director.is_none() {
                    let step = dt * p.position.speed_kt * KNOT;
                    if distance(p.position, p.destination) <= step + 500. {
                        expired.push(id.clone());
                    } else {
                        p.position = forward(p.position, bearing(p.position, p.destination), step);
                    }
                }
            }
            for id in expired {
                self.synthetic.remove(&id);
                if let Some(d) = &mut self.director {
                    d.forget(&id, false, &mut self.runways);
                }
            }
        } else {
            self.synthetic.clear();
        }
        if let Some(g) = &mut self.ground {
            g.tick(dt, &mut self.runways, self.ground_atc.as_mut())?;
            if let Some(d) = &mut self.director {
                for arrived in std::mem::take(&mut d.arrivals) {
                    let mut transferred = false;
                    if g.aircraft.len() < self.settings.ground_limit {
                        let runway_nodes: Vec<_> = g
                            .airport
                            .nodes
                            .values()
                            .filter(|n| {
                                n.kind == "runway"
                                    && distance(n.position(), arrived.position) < 110.
                            })
                            .map(|n| n.id.clone())
                            .collect();
                        let occupied: BTreeSet<_> = g
                            .aircraft
                            .values()
                            .filter(|p| p.arrival)
                            .map(|p| p.route.last().unwrap().clone())
                            .collect();
                        let gates: Vec<_> = g
                            .airport
                            .nodes
                            .values()
                            .filter(|n| n.kind == "gate" && !occupied.contains(&n.id))
                            .map(|n| n.id.clone())
                            .collect();
                        'outer: for node in runway_nodes {
                            for gate in &gates {
                                if g.add(&arrived.id, &arrived.model_title, &node, gate, true)
                                    .is_ok()
                                {
                                    g.aircraft.get_mut(&arrived.id).unwrap().owns_runway =
                                        d.runway_id.clone();
                                    transferred = true;
                                    break 'outer;
                                }
                            }
                        }
                    }
                    d.forget(&arrived.id, transferred, &mut self.runways);
                }
            }
            let completed: Vec<_> = g
                .aircraft
                .values()
                .filter(|p| [Phase::Airborne, Phase::Complete].contains(&p.phase))
                .cloned()
                .collect();
            for p in completed {
                if p.phase == Phase::Airborne
                    && self.settings.mode != "live"
                    && self.synthetic.len() + self.fleet.len() < self.settings.airborne_limit
                {
                    let id = if p.id.starts_with("SYN-") {
                        p.id.clone()
                    } else {
                        format!("GROUND-{}", p.id)
                    };
                    self.synthetic.insert(
                        id.clone(),
                        Flight {
                            id,
                            model_title: p.model_title.clone(),
                            position: Position {
                                on_ground: false,
                                speed_kt: p.position.speed_kt.max(125.),
                                ..p.position
                            },
                            destination: forward(p.position, p.position.heading, 85000.),
                            remaining_s: 2700.,
                            arrival: false,
                        },
                    );
                }
                g.remove(&p.id, &mut self.runways);
                if let Some(a) = &mut self.ground_atc {
                    a.forget(&p.id);
                }
            }
        } else if let Some(d) = &mut self.director {
            for a in std::mem::take(&mut d.arrivals) {
                d.forget(&a.id, false, &mut self.runways);
            }
        }
        let mut desired = BTreeMap::new();
        if self.settings.mode != "simulation" {
            for p in self.fleet.values() {
                desired.insert(
                    format!("LIVE-{}", p.id),
                    (p.model_title.clone(), p.position),
                );
            }
        }
        if self.settings.mode != "live" {
            for p in self.synthetic.values() {
                desired.insert(p.id.clone(), (p.model_title.clone(), p.position));
            }
        }
        if let Some(g) = &self.ground {
            for p in g.aircraft.values() {
                if ![Phase::Airborne, Phase::Complete].contains(&p.phase) {
                    let key = if p.id.starts_with("SYN-") {
                        p.id.clone()
                    } else {
                        format!("GROUND-{}", p.id)
                    };
                    desired.insert(key, (p.model_title.clone(), p.position));
                }
            }
        }
        let removed: Vec<_> = self
            .spawned
            .iter()
            .filter(|id| !desired.contains_key(*id))
            .cloned()
            .collect();
        for id in removed {
            self.bridge.remove(&id)?;
            self.spawned.remove(&id);
        }
        for (id, (title, pos)) in &desired {
            if self.spawned.contains(id) {
                self.bridge.update(id, *pos)?;
            } else {
                self.bridge.create(id, title, *pos)?;
                self.spawned.insert(id.clone());
            }
        }
        Ok(Stats {
            live: self.fleet.len(),
            synthetic: self.synthetic.len(),
            ground: self.ground.as_ref().map_or(0, |g| {
                g.aircraft
                    .values()
                    .filter(|p| ![Phase::Airborne, Phase::Complete].contains(&p.phase))
                    .count()
            }),
            objects: self.spawned.len(),
        })
    }
    pub fn close(&mut self) -> Result<()> {
        if let Some(d) = &mut self.director {
            d.close(&mut self.runways);
        }
        let mut error = None;
        for id in std::mem::take(&mut self.spawned) {
            if let Err(e) = self.bridge.remove(&id) {
                error.get_or_insert(e);
            }
        }
        if let Err(e) = self.bridge.close() {
            error.get_or_insert(e);
        }
        if let Some(e) = error {
            return Err(e);
        }
        Ok(())
    }
}
impl<B: Bridge> Drop for Manager<B> {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
