use crate::geo::Position;
use anyhow::{Result, bail};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Default)]
pub struct CockpitRadio {
    pub com1: Option<f64>,
    pub com2: Option<f64>,
    pub transmitting: u8,
    pub squawk: Option<String>,
}
pub fn valid_mhz(raw: f64) -> Option<f64> {
    (118. ..=136.99)
        .contains(&raw)
        .then(|| (raw * 1000.).round() / 1000.)
}
pub fn decode_squawk(raw: f64) -> Option<String> {
    if !raw.is_finite() || !(0. ..=7777.).contains(&raw) {
        return None;
    }
    let code = format!("{:04}", raw as u32);
    code.chars()
        .all(|c| ('0'..='7').contains(&c))
        .then_some(code)
}
pub trait Bridge {
    fn poll(&mut self) -> Result<()> {
        Ok(())
    }
    fn create(&mut self, key: &str, title: &str, p: Position) -> Result<()>;
    fn update(&mut self, key: &str, p: Position) -> Result<()>;
    fn remove(&mut self, key: &str) -> Result<()>;
    fn close(&mut self) -> Result<()>;
    fn player_position(&self) -> Option<Position> {
        None
    }
    fn player_radio(&self) -> Option<CockpitRadio> {
        None
    }
}
#[derive(Default)]
pub struct MockBridge {
    pub objects: BTreeMap<String, (String, Position)>,
    pub creates: usize,
    pub updates: usize,
    pub removals: usize,
    pub connected: bool,
    pub created_keys: Vec<String>,
}
impl MockBridge {
    pub fn new() -> Self {
        Self {
            connected: true,
            ..Self::default()
        }
    }
}
impl Bridge for MockBridge {
    fn create(&mut self, key: &str, title: &str, p: Position) -> Result<()> {
        p.validate()?;
        self.objects.insert(key.into(), (title.into(), p));
        self.creates += 1;
        self.created_keys.push(key.into());
        Ok(())
    }
    fn update(&mut self, key: &str, p: Position) -> Result<()> {
        p.validate()?;
        if let Some(o) = self.objects.get_mut(key) {
            o.1 = p;
            self.updates += 1;
        } else {
            bail!("missing object {key}");
        }
        Ok(())
    }
    fn remove(&mut self, key: &str) -> Result<()> {
        if self.objects.remove(key).is_some() {
            self.removals += 1;
        }
        Ok(())
    }
    fn close(&mut self) -> Result<()> {
        self.objects.clear();
        self.connected = false;
        Ok(())
    }
}
impl Bridge for Box<dyn Bridge> {
    fn poll(&mut self) -> Result<()> {
        (**self).poll()
    }
    fn create(&mut self, k: &str, t: &str, p: Position) -> Result<()> {
        (**self).create(k, t, p)
    }
    fn update(&mut self, k: &str, p: Position) -> Result<()> {
        (**self).update(k, p)
    }
    fn remove(&mut self, k: &str) -> Result<()> {
        (**self).remove(k)
    }
    fn close(&mut self) -> Result<()> {
        (**self).close()
    }
    fn player_position(&self) -> Option<Position> {
        (**self).player_position()
    }
    fn player_radio(&self) -> Option<CockpitRadio> {
        (**self).player_radio()
    }
}
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct InitPosition {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude: f64,
    pub pitch: f64,
    pub bank: f64,
    pub heading: f64,
    pub on_ground: u32,
    pub airspeed: u32,
}
impl From<Position> for InitPosition {
    fn from(p: Position) -> Self {
        Self {
            latitude: p.lat,
            longitude: p.lon,
            altitude: p.alt_ft.max(-1000.),
            pitch: p.pitch,
            bank: p.bank,
            heading: p.heading,
            on_ground: u32::from(p.on_ground),
            airspeed: p.speed_kt.max(0.) as u32,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Recv {
    pub size: u32,
    pub version: u32,
    pub id: u32,
}
#[repr(C)]
pub struct Assigned {
    pub header: Recv,
    pub request: u32,
    pub object: u32,
}
#[cfg(windows)]
#[path = "simconnect.rs"]
mod native;
#[cfg(windows)]
pub use native::SimConnect;
pub fn native_bridge(allow_motion: bool) -> Result<Box<dyn Bridge>> {
    #[cfg(windows)]
    {
        Ok(Box::new(SimConnect::connect(allow_motion)?))
    }
    #[cfg(not(windows))]
    {
        let _ = allow_motion;
        bail!("SimConnect requires Windows and a running MSFS 2020")
    }
}
