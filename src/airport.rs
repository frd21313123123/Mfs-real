use crate::geo::{Position, distance};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
fn taxi() -> String {
    "taxi".into()
}
fn speed() -> f64 {
    12.
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub lat: f64,
    pub lon: f64,
    #[serde(default)]
    pub alt_ft: f64,
    #[serde(default = "taxi")]
    pub kind: String,
}
impl Node {
    pub fn position(&self) -> Position {
        Position {
            on_ground: true,
            ..Position::new(self.lat, self.lon, self.alt_ft)
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Edge {
    pub src: String,
    pub dst: String,
    #[serde(default = "speed")]
    pub speed_kt: f64,
    #[serde(default = "taxi")]
    pub kind: String,
    #[serde(default)]
    pub runway: String,
    #[serde(default)]
    pub bidirectional: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphData {
    pub icao: String,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    #[serde(default, rename = "_NOTICE")]
    pub notice: String,
    #[serde(default, rename = "_warnings")]
    pub warnings: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct Airport {
    pub icao: String,
    pub nodes: BTreeMap<String, Node>,
    pub edges: BTreeMap<(String, String), Edge>,
    pub fictional: bool,
}
impl Airport {
    pub fn from_json(s: &str) -> Result<Self> {
        Self::new(serde_json::from_str(s)?)
    }
    pub fn load(path: &Path) -> Result<Self> {
        Self::from_json(&std::fs::read_to_string(path)?)
    }
    pub fn demo() -> Self {
        Self::from_json(crate::DEMO_GRAPH).expect("embedded airport validated by tests")
    }
    pub fn new(data: GraphData) -> Result<Self> {
        let mut nodes = BTreeMap::new();
        for n in data.nodes {
            n.position().validate()?;
            ensure!(nodes.insert(n.id.clone(), n).is_none(), "duplicate node");
        }
        ensure!(!nodes.is_empty(), "empty airport");
        let mut edges = BTreeMap::new();
        for e in data.edges {
            ensure!(
                nodes.contains_key(&e.src) && nodes.contains_key(&e.dst) && e.src != e.dst,
                "invalid edge endpoint"
            );
            ensure!(
                e.speed_kt.is_finite() && e.speed_kt > 0.,
                "invalid edge speed"
            );
            if e.bidirectional {
                let reverse = Edge {
                    src: e.dst.clone(),
                    dst: e.src.clone(),
                    bidirectional: false,
                    ..e.clone()
                };
                edges.insert((reverse.src.clone(), reverse.dst.clone()), reverse);
            }
            edges.insert((e.src.clone(), e.dst.clone()), e);
        }
        Ok(Self {
            icao: data.icao,
            nodes,
            edges,
            fictional: data.notice.to_uppercase().contains("FICTIONAL"),
        })
    }
    pub fn route(
        &self,
        start: &str,
        end: &str,
        forbidden: &BTreeSet<(String, String)>,
    ) -> Result<Vec<String>> {
        ensure!(
            self.nodes.contains_key(start) && self.nodes.contains_key(end),
            "node does not exist"
        );
        if start == end {
            return Ok(vec![start.into()]);
        }
        let mut costs: BTreeMap<String, f64> = BTreeMap::from([(start.to_owned(), 0.)]);
        let mut prev = BTreeMap::new();
        let mut open = BTreeSet::from([start.to_owned()]);
        while !open.is_empty() {
            let id = open
                .iter()
                .min_by(|a, b| costs[*a].total_cmp(&costs[*b]))
                .unwrap()
                .clone();
            open.remove(&id);
            if id == end {
                break;
            }
            for ((src, dst), edge) in &self.edges {
                if src != &id
                    || edge.kind == "closed"
                    || forbidden.contains(&(src.clone(), dst.clone()))
                {
                    continue;
                }
                let cost = costs[&id]
                    + distance(self.nodes[src].position(), self.nodes[dst].position())
                        / edge.speed_kt
                    + if edge.kind == "runway" { 80. } else { 0. };
                if cost < *costs.get(dst).unwrap_or(&f64::INFINITY) {
                    costs.insert(dst.clone(), cost);
                    prev.insert(dst.clone(), src.clone());
                    open.insert(dst.clone());
                }
            }
        }
        if !prev.contains_key(end) {
            return Ok(vec![]);
        }
        let mut route = vec![end.to_owned()];
        while route.last().unwrap() != start {
            route.push(prev[route.last().unwrap()].clone());
        }
        route.reverse();
        Ok(route)
    }
}
pub fn convert_xml(source: &str, icao: Option<&str>) -> Result<GraphData> {
    let document = roxmltree::Document::parse(source)?;
    let apt = document
        .descendants()
        .find(|n| {
            n.has_tag_name("Airport")
                && icao.is_none_or(|id| n.attribute("ident").unwrap_or("").eq_ignore_ascii_case(id))
        })
        .context("Airport not found")?;
    let value = |n: roxmltree::Node, key: &str, default: &str| -> Result<f64> {
        Ok(n.attribute(key).unwrap_or(default).parse()?)
    };
    let lat = value(apt, "lat", "0")?;
    let lon = value(apt, "lon", "0")?;
    let alt = apt.attribute("alt").unwrap_or("0").to_uppercase();
    let alt_ft = if alt.ends_with('F') {
        alt.trim_end_matches('F').parse()?
    } else {
        alt.trim_end_matches('M').parse::<f64>()? / crate::geo::FT
    };
    let ident = apt
        .attribute("ident")
        .or(apt.attribute("icao"))
        .unwrap_or("UNKNOWN")
        .to_uppercase();
    let mut nodes = BTreeMap::new();
    let mut warnings = vec![];
    let mut edges = vec![];
    for n in apt
        .children()
        .filter(|n| n.has_tag_name("TaxiwayPoint") || n.has_tag_name("TaxiwayParking"))
    {
        let Some(index) = n.attribute("index") else {
            warnings.push("point without index".into());
            continue;
        };
        let id = format!("N{}", index.parse::<u32>()?);
        let node = Node {
            id: id.clone(),
            lat: if n.attribute("lat").is_some() {
                value(n, "lat", "0")?
            } else {
                lat + value(n, "biasZ", "0")? / 111195.
            },
            lon: if n.attribute("lon").is_some() {
                value(n, "lon", "0")?
            } else {
                lon + value(n, "biasX", "0")? / (111195. * lat.to_radians().cos().max(0.01))
            },
            alt_ft,
            kind: if n.has_tag_name("TaxiwayParking") {
                "gate"
            } else if n.attribute("type").unwrap_or("").contains("HOLD_SHORT") {
                "hold"
            } else {
                "taxi"
            }
            .into(),
        };
        ensure!(nodes.insert(id, node).is_none(), "duplicate taxi index");
    }
    for n in apt.children().filter(|n| n.has_tag_name("TaxiwayPath")) {
        let typ = n.attribute("type").unwrap_or("TAXI").to_uppercase();
        if ["CLOSED", "VEHICLE", "ROAD"].contains(&typ.as_str()) {
            continue;
        }
        let (Some(start), Some(end)) = (n.attribute("start"), n.attribute("end")) else {
            continue;
        };
        let (start, end) = (format!("N{start}"), format!("N{end}"));
        if !nodes.contains_key(&start) || !nodes.contains_key(&end) {
            warnings.push(format!("missing path {start}->{end}"));
            continue;
        }
        if typ == "PARKING" {
            let (gate, taxi) = if nodes[&start].kind == "gate" {
                (start, end)
            } else if nodes[&end].kind == "gate" {
                (end, start)
            } else {
                warnings.push("parking path has no gate".into());
                continue;
            };
            edges.push(Edge {
                src: gate.clone(),
                dst: taxi.clone(),
                speed_kt: 3.,
                kind: "pushback".into(),
                runway: String::new(),
                bidirectional: false,
            });
            edges.push(Edge {
                src: taxi,
                dst: gate,
                speed_kt: 4.,
                kind: "taxi".into(),
                runway: String::new(),
                bidirectional: false,
            });
        } else {
            let runway = if typ == "RUNWAY" {
                format!(
                    "{}/{}{}",
                    ident,
                    n.attribute("number").unwrap_or("R"),
                    n.attribute("designator").unwrap_or("")
                )
            } else {
                String::new()
            };
            if !runway.is_empty() {
                for id in [&start, &end] {
                    let node = nodes.get_mut(id).unwrap();
                    if node.kind != "hold" {
                        node.kind = "runway".into();
                    }
                }
            }
            edges.push(Edge {
                src: start,
                dst: end,
                speed_kt: if runway.is_empty() { 12. } else { 100. },
                kind: if runway.is_empty() { "taxi" } else { "runway" }.into(),
                runway,
                bidirectional: true,
            });
        }
    }
    let data = GraphData {
        icao: ident,
        nodes: nodes.into_values().collect(),
        edges,
        notice: "Generated from source XML; verify against MSFS before use".into(),
        warnings,
    };
    Airport::new(data.clone())?;
    Ok(data)
}
