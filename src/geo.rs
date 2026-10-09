use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
pub const EARTH: f64 = 6_371_000.;
pub const KNOT: f64 = 0.514444;
pub const FT: f64 = 0.3048;
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Position {
    pub lat: f64,
    pub lon: f64,
    pub alt_ft: f64,
    pub heading: f64,
    pub speed_kt: f64,
    pub on_ground: bool,
    pub pitch: f64,
    pub bank: f64,
}
impl Position {
    pub fn new(lat: f64, lon: f64, alt_ft: f64) -> Self {
        Self {
            lat,
            lon,
            alt_ft,
            ..Self::default()
        }
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (-90. ..=90.).contains(&self.lat) && (-180. ..=180.).contains(&self.lon),
            "Invalid coordinates"
        );
        ensure!(
            [
                self.alt_ft,
                self.heading,
                self.speed_kt,
                self.pitch,
                self.bank
            ]
            .iter()
            .all(|x| x.is_finite()),
            "Nonfinite position"
        );
        Ok(())
    }
}
pub fn distance(a: Position, b: Position) -> f64 {
    let x = ((b.lat - a.lat).to_radians() / 2.).sin().powi(2)
        + a.lat.to_radians().cos()
            * b.lat.to_radians().cos()
            * ((b.lon - a.lon).to_radians() / 2.).sin().powi(2);
    EARTH * 2. * x.clamp(0., 1.).sqrt().asin()
}
pub fn bearing(a: Position, b: Position) -> f64 {
    let (a, b, dl) = (
        a.lat.to_radians(),
        b.lat.to_radians(),
        (b.lon - a.lon).to_radians(),
    );
    (dl.sin() * b.cos())
        .atan2(a.cos() * b.sin() - a.sin() * b.cos() * dl.cos())
        .to_degrees()
        .rem_euclid(360.)
}
pub fn forward(p: Position, heading: f64, metres: f64) -> Position {
    let (a, lat, lon, brg) = (
        metres / EARTH,
        p.lat.to_radians(),
        p.lon.to_radians(),
        heading.to_radians(),
    );
    let dest = (lat.sin() * a.cos() + lat.cos() * a.sin() * brg.cos())
        .clamp(-1., 1.)
        .asin();
    let dl = lon + (brg.sin() * a.sin() * lat.cos()).atan2(a.cos() - lat.sin() * dest.sin());
    Position {
        lat: dest.to_degrees(),
        lon: (dl.to_degrees() + 180.).rem_euclid(360.) - 180.,
        heading: heading.rem_euclid(360.),
        ..p
    }
}
pub fn heading_error(target: f64, actual: f64) -> f64 {
    (target - actual + 180.).rem_euclid(360.) - 180.
}
pub fn blend(a: Position, b: Position, t: f64) -> Position {
    let t = t.clamp(0., 1.);
    Position {
        lat: a.lat + (b.lat - a.lat) * t,
        lon: (a.lon + heading_error(b.lon, a.lon) * t + 180.).rem_euclid(360.) - 180.,
        alt_ft: a.alt_ft + (b.alt_ft - a.alt_ft) * t,
        heading: (a.heading + heading_error(b.heading, a.heading) * t).rem_euclid(360.),
        speed_kt: a.speed_kt + (b.speed_kt - a.speed_kt) * t,
        on_ground: if t >= 1. { b.on_ground } else { a.on_ground },
        ..a
    }
}
