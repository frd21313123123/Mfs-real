use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub mode: String,
    pub airborne_limit: usize,
    pub ground_limit: usize,
    pub realistic_taxi: bool,
    pub runway_control: bool,
    pub sync_real_flights: bool,
    pub fetch_seconds: u64,
    pub max_radius_km: f64,
    pub fps_target: u32,
    pub fsltl_path: String,
    pub airport_graph: String,
    pub enable_simconnect_position_writes: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: "hybrid".into(),
            airborne_limit: 35,
            ground_limit: 30,
            realistic_taxi: true,
            runway_control: true,
            sync_real_flights: true,
            fetch_seconds: 120,
            max_radius_km: 120.,
            fps_target: 35,
            fsltl_path: String::new(),
            airport_graph: String::new(),
            enable_simconnect_position_writes: false,
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            ["hybrid", "live", "simulation"].contains(&self.mode.as_str()),
            "mode must be hybrid, live or simulation"
        );
        ensure!(
            self.airborne_limit <= 35 && self.ground_limit <= 30,
            "airborne <=35; ground <=30"
        );
        ensure!(self.fetch_seconds >= 60, "OpenSky interval must be >=60 s");
        ensure!(
            self.max_radius_km.is_finite() && (2. ..=250.).contains(&self.max_radius_km),
            "radius must be 2..250 km"
        );
        ensure!(self.fps_target > 0, "FPS must be positive");
        Ok(())
    }
    pub fn load(path: &Path) -> Result<Self> {
        let value = match fs::read_to_string(path) {
            Ok(s) => serde_json::from_str(&s).context("Invalid settings JSON")?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => return Err(e.into()),
        };
        value.validate()?;
        Ok(value)
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        write_atomic(path, serde_json::to_string_pretty(self)?.as_bytes())
    }
    pub fn resolve_paths(&mut self, profile: &Path) {
        let base = profile.parent().unwrap_or(Path::new("."));
        for value in [&mut self.fsltl_path, &mut self.airport_graph] {
            if !value.is_empty() && Path::new(value).is_relative() {
                *value = base.join(&*value).to_string_lossy().into_owned();
            }
        }
    }
}
pub fn user_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".realflow")
}
pub fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".realflow-{}-{}.tmp",
        std::process::id(),
        rand::random::<u64>()
    ));
    let result = (|| {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        f.write_all(data)?;
        f.sync_all()?;
        drop(f);
        #[cfg(windows)]
        {
            // MoveFileExW atomically replaces an existing Windows profile.
            use std::os::windows::ffi::OsStrExt;
            #[link(name = "kernel32")]
            unsafe extern "system" {
                fn MoveFileExW(a: *const u16, b: *const u16, flags: u32) -> i32;
            }
            let a: Vec<_> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
            let b: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            if unsafe { MoveFileExW(a.as_ptr(), b.as_ptr(), 0x1 | 0x8) } == 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        #[cfg(not(windows))]
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if temporary.exists() {
        let _ = fs::remove_file(temporary);
    }
    result
}
