use crate::geo::{FT, KNOT, Position};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Model {
    pub title: String,
    pub aircraft_type: String,
    pub airline: String,
    pub path: String,
    pub variation: String,
}
impl Model {
    pub fn fixture() -> Self {
        Self {
            title: "MOCK A320 (OFFLINE ONLY)".into(),
            aircraft_type: "A320".into(),
            airline: "DEM".into(),
            path: String::new(),
            variation: String::new(),
        }
    }
}
pub fn known_roots() -> Vec<PathBuf> {
    let home = crate::config::user_dir().parent().unwrap().to_owned();
    let local = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or(home.join("AppData/Local"));
    let roaming = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or(home.join("AppData/Roaming"));
    let store = local.join("Packages/Microsoft.FlightSimulator_8wekyb3d8bbwe/LocalCache");
    let steam = roaming.join("Microsoft Flight Simulator");
    let mut roots = vec![];
    for var in ["FSLTL_BASE", "MSFS_COMMUNITY"] {
        if let Some(p) = std::env::var_os(var) {
            roots.push(p.into());
        }
    }
    for base in [&store, &steam] {
        if let Ok(cfg) = std::fs::read_to_string(base.join("UserCfg.opt")) {
            for line in cfg.lines() {
                if line.trim().starts_with("InstalledPackagesPath")
                    && let Some(path) = line.split('"').nth(1)
                {
                    roots.push(PathBuf::from(path).join("Community"));
                }
            }
        }
    }
    roots.extend([
        store.join("Packages/Community"),
        steam.join("Packages/Community"),
        home.join("AppData/Local/MSFSPackages/Community"),
    ]);
    roots
}
pub fn discover(explicit: &str) -> Option<PathBuf> {
    for base in if explicit.is_empty() {
        known_roots()
    } else {
        vec![PathBuf::from(explicit)]
    } {
        if base.join("fsltl-traffic-base").is_dir() {
            return Some(base.join("fsltl-traffic-base"));
        }
        if base.is_dir()
            && base.file_name().is_some_and(|s| {
                s.to_string_lossy()
                    .eq_ignore_ascii_case("fsltl-traffic-base")
            })
        {
            return Some(base);
        }
    }
    None
}
pub fn scan(root: &Path) -> Result<Vec<Model>> {
    ensure!(root.is_dir(), "FSLTL directory not found");
    let mut models = vec![];
    let mut files = walkdir::WalkDir::new(root)
        .into_iter()
        .collect::<std::result::Result<Vec<_>, _>>()?;
    files.sort_by(|a, b| a.path().cmp(b.path()));
    for entry in files {
        if !entry.file_type().is_file()
            || !entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case("aircraft.cfg")
        {
            continue;
        }
        let bytes = std::fs::read(entry.path())?;
        let data = String::from_utf8_lossy(&bytes);
        let mut attrs = BTreeMap::<String, String>::new();
        let mut active = false;
        let flush = |attrs: &BTreeMap<String, String>, models: &mut Vec<Model>| {
            if let Some(title) = attrs.get("title").filter(|s| !s.is_empty()) {
                let get = |keys: &[&str]| {
                    keys.iter()
                        .find_map(|k| attrs.get(*k).filter(|s| !s.is_empty()))
                        .cloned()
                        .unwrap_or_default()
                };
                models.push(Model {
                    title: title.clone(),
                    aircraft_type: get(&["icao_type_designator", "atc_model", "ui_type"])
                        .to_uppercase(),
                    airline: get(&["icao_airline", "atc_airline"]).to_uppercase(),
                    path: entry.path().to_string_lossy().into_owned(),
                    variation: get(&["ui_variation"]),
                });
            }
        };
        for line in data.trim_start_matches('\u{feff}').lines() {
            let line = line.trim();
            if line.starts_with('[') && line.ends_with(']') {
                if active {
                    flush(&attrs, &mut models);
                }
                attrs.clear();
                let section = line[1..line.len() - 1].replace(' ', "").to_lowercase();
                active = section
                    .strip_prefix("fltsim.")
                    .is_some_and(|n| n.parse::<u32>().is_ok());
                continue;
            }
            if !active || line.starts_with(';') || line.starts_with('#') || line.starts_with("//") {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                let v = v
                    .split(" ;")
                    .next()
                    .unwrap_or(v)
                    .trim()
                    .trim_matches(['"', '\'']);
                attrs.insert(k.trim().to_lowercase(), v.into());
            }
        }
        if active {
            flush(&attrs, &mut models);
        }
    }
    Ok(models)
}
pub fn choose<'a>(models: &'a [Model], typ: &str, airline: &str) -> Option<&'a Model> {
    let (t, a) = (typ.trim().to_uppercase(), airline.trim().to_uppercase());
    let score = |m: &Model| {
        let name = m.title.to_uppercase();
        let points = (if !t.is_empty() && m.aircraft_type == t {
            110
        } else if !t.is_empty() && name.contains(&t) {
            35
        } else {
            0
        }) + (if !a.is_empty() && m.airline == a {
            55
        } else if !a.is_empty() && name.contains(&a) {
            10
        } else {
            0
        }) + if t.is_empty() { 5 } else { 0 };
        (points, -(m.title.len() as i64))
    };
    let result = models.iter().max_by_key(|m| score(m))?;
    if !t.is_empty() && result.aircraft_type != t && !result.title.to_uppercase().contains(&t) {
        let family = t.get(..2)?;
        return models
            .iter()
            .filter(|m| m.aircraft_type.starts_with(family))
            .max_by_key(|m| score(m));
    }
    Some(result)
}
#[derive(Clone, Debug, Serialize)]
pub struct Observation {
    pub icao24: String,
    pub callsign: String,
    pub position: Position,
    pub vertical_mps: f64,
    pub observed_at: f64,
    pub icao_type: String,
}
pub fn parse_states(payload: &Value) -> Vec<Observation> {
    let mut results = vec![];
    if let Some(rows) = payload["states"].as_array() {
        for row in rows {
            let Some(row) = row.as_array().filter(|r| r.len() >= 17) else {
                continue;
            };
            let (Some(lon), Some(lat), Some(alt)) = (
                row[5].as_f64(),
                row[6].as_f64(),
                row[13].as_f64().or_else(|| row[7].as_f64()),
            ) else {
                continue;
            };
            let icao = row[0].as_str().unwrap_or("").trim().to_lowercase();
            if icao.is_empty() {
                continue;
            }
            let p = Position {
                heading: row[10].as_f64().unwrap_or(0.),
                speed_kt: row[9].as_f64().unwrap_or(0.) / KNOT,
                on_ground: row[8].as_bool().unwrap_or(false),
                ..Position::new(lat, lon, alt / FT)
            };
            if p.validate().is_err() {
                continue;
            }
            let observed_at = row[4]
                .as_f64()
                .filter(|x| *x > 0.)
                .or_else(|| row[3].as_f64())
                .or_else(|| payload["time"].as_f64())
                .unwrap_or(0.);
            results.push(Observation {
                icao24: icao,
                callsign: row[1].as_str().unwrap_or("").trim().into(),
                position: p,
                vertical_mps: row[11].as_f64().unwrap_or(0.),
                observed_at,
                icao_type: String::new(),
            });
        }
    }
    results
}
pub fn http_client() -> Result<reqwest::blocking::Client> {
    let mut builder = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .user_agent("RealFlowTraffic/0.3 Rust");
    // Respect the injected cloud CA without disabling TLS verification.
    if let Some(path) =
        std::env::var_os("SSL_CERT_FILE").or_else(|| std::env::var_os("REQUESTS_CA_BUNDLE"))
    {
        for cert in reqwest::Certificate::from_pem_bundle(&std::fs::read(path)?)? {
            builder = builder.add_root_certificate(cert);
        }
    }
    Ok(builder.build()?)
}
pub struct OpenSky {
    client: reqwest::blocking::Client,
    id: String,
    secret: String,
    token: String,
    expires: Instant,
    pub next_allowed: Instant,
    interval: Duration,
}
impl OpenSky {
    pub fn new(seconds: u64) -> Result<Self> {
        ensure!(seconds >= 60, "minimum interval 60 s");
        Ok(Self {
            client: http_client()?,
            id: std::env::var("OPENSKY_CLIENT_ID").unwrap_or_default(),
            secret: std::env::var("OPENSKY_CLIENT_SECRET").unwrap_or_default(),
            token: String::new(),
            expires: Instant::now(),
            next_allowed: Instant::now(),
            interval: Duration::from_secs(seconds),
        })
    }
    pub fn poll(&mut self, p: Position, radius: f64) -> Result<Vec<Observation>> {
        ensure!(
            (0. ..=250.).contains(&radius) && radius > 0.,
            "radius must be >0 and <=250"
        );
        let now = Instant::now();
        if now < self.next_allowed {
            return Ok(vec![]);
        }
        self.next_allowed = now + self.interval;
        if !self.id.is_empty()
            && !self.secret.is_empty()
            && (self.token.is_empty() || now >= self.expires)
        {
            let payload:Value=self.client.post("https://auth.opensky-network.org/auth/realms/opensky-network/protocol/openid-connect/token").form(&[("grant_type","client_credentials"),("client_id",&self.id),("client_secret",&self.secret)]).send()?.error_for_status()?.json()?;
            self.token = payload["access_token"]
                .as_str()
                .context("OAuth response missing token")?
                .into();
            self.expires = now
                + Duration::from_secs(
                    payload["expires_in"]
                        .as_u64()
                        .unwrap_or(1800)
                        .saturating_sub(30)
                        .max(1),
                );
        }
        let dy = radius / 111.2;
        let dx = (radius / (111.2 * p.lat.to_radians().cos().abs().max(0.1))).min(180.);
        let mut request = self
            .client
            .get("https://opensky-network.org/api/states/all")
            .query(&[
                ("lamin", (p.lat - dy).max(-90.)),
                ("lamax", (p.lat + dy).min(90.)),
                ("lomin", (p.lon - dx).max(-180.)),
                ("lomax", (p.lon + dx).min(180.)),
            ]);
        if !self.token.is_empty() {
            request = request.bearer_auth(&self.token);
        }
        let response = request.send()?;
        if [401, 403, 429].contains(&response.status().as_u16()) {
            let wait = response
                .headers()
                .get("X-Rate-Limit-Retry-After-Seconds")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(300);
            self.next_allowed = now + self.interval.max(Duration::from_secs(wait.min(86400)));
            if response.status().as_u16() == 401 {
                self.token.clear();
            }
        }
        Ok(parse_states(&response.error_for_status()?.json()?))
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Frequency {
    pub airport: String,
    pub station: String,
    pub mhz: f64,
    pub description: String,
}
pub fn station_alias(s: &str) -> Option<&'static str> {
    Some(match s.trim().to_uppercase().as_str() {
        "TWR" | "TOWER" => "tower",
        "GND" | "GROUND" => "ground",
        "CLD" | "CLR" | "DEL" | "CLEARANCE" => "delivery",
        "DEP" | "DEPARTURE" => "departure",
        "APP" | "APPR" | "ARR" => "approach",
        "CTR" | "CENTER" | "ACC" => "center",
        "ATIS" => "atis",
        "UNICOM" => "unicom",
        "CTAF" | "ATF" => "ctaf",
        _ => return None,
    })
}
pub fn frequencies(text: &str) -> Result<Vec<Frequency>> {
    let mut reader = csv::Reader::from_reader(text.trim_start_matches('\u{feff}').as_bytes());
    let headers = reader.headers()?.clone();
    let col = |s: &str| {
        headers
            .iter()
            .position(|h| h == s)
            .with_context(|| format!("missing CSV column {s}"))
    };
    let (airport, kind, mhz) = (col("airport_ident")?, col("type")?, col("frequency_mhz")?);
    let desc = headers.iter().position(|h| h == "description");
    let mut output = vec![];
    for row in reader.records() {
        let row = row?;
        let a = row.get(airport).unwrap_or("").trim().to_uppercase();
        if !(3..=5).contains(&a.len()) || !a.chars().all(|c| c.is_ascii_alphanumeric()) {
            continue;
        }
        let Some(station) = station_alias(row.get(kind).unwrap_or("")) else {
            continue;
        };
        let Ok(freq) = row.get(mhz).unwrap_or("").parse::<f64>() else {
            continue;
        };
        if !(118. ..=136.99).contains(&freq) {
            continue;
        }
        output.push(Frequency {
            airport: a,
            station: station.into(),
            mhz: (freq * 1000.).round() / 1000.,
            description: desc.and_then(|i| row.get(i)).unwrap_or("").into(),
        });
    }
    Ok(output)
}
pub fn primary(rows: &[Frequency], icao: &str) -> BTreeMap<String, f64> {
    let mut out = BTreeMap::new();
    for row in rows.iter().filter(|r| r.airport.eq_ignore_ascii_case(icao)) {
        out.entry(row.station.clone()).or_insert(row.mhz);
    }
    out
}
pub fn update_frequencies(path: &Path) -> Result<usize> {
    let response = http_client()?
        .get("https://davidmegginson.github.io/ourairports-data/airport-frequencies.csv")
        .send()?
        .error_for_status()?;
    let mut bytes = vec![];
    response.take(8_000_001).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 8_000_000, "frequency dataset too large");
    let text = String::from_utf8(bytes)?;
    let rows = frequencies(&text)?;
    ensure!(
        rows.len() >= 500,
        "dataset too small; existing cache preserved"
    );
    crate::config::write_atomic(path, text.as_bytes())?;
    Ok(rows.len())
}
pub fn metadata(path: &Path) -> Result<BTreeMap<String, String>> {
    let text = std::fs::read_to_string(path)?;
    let mut reader = csv::Reader::from_reader(text.trim_start_matches('\u{feff}').as_bytes());
    let headers = reader.headers()?.clone();
    let a = headers
        .iter()
        .position(|x| x == "icao24")
        .context("CSV requires icao24")?;
    let t = headers
        .iter()
        .position(|x| x == "icao_type")
        .context("CSV requires icao_type")?;
    let mut map = BTreeMap::new();
    for row in reader.records() {
        let row = row?;
        if let (Some(a), Some(t)) = (row.get(a), row.get(t))
            && !a.trim().is_empty()
            && !t.trim().is_empty()
        {
            map.insert(a.trim().to_lowercase(), t.trim().to_uppercase());
        }
    }
    Ok(map)
}
