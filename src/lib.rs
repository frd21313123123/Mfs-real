pub mod airport;
pub mod atc;
pub mod bridge;
pub mod cli;
pub mod config;
pub mod director;
pub mod engine;
pub mod geo;
#[cfg(feature = "gui")]
pub mod launcher;
pub mod sources;
#[cfg(feature = "gui")]
pub mod traffic_download;
pub mod voice;
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DEMO_GRAPH: &str = include_str!("../examples/demo_airport.json");
