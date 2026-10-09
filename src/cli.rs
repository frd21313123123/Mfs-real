use crate::{
    airport::{Airport, convert_xml},
    atc::{GroundAtc, Pilot, PlayerMonitor},
    bridge::{Bridge, MockBridge, native_bridge},
    config::{Settings, user_dir, write_atomic},
    director::Director,
    engine::{Ground, Manager, Stats},
    geo::{Position, bearing},
    sources,
};
use anyhow::{Context, Result, ensure};
use clap::{Args, Parser, Subcommand};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, BufRead, Write},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
#[derive(Parser, Debug)]
#[command(version, about = "RealFlow native traffic and ATC prototype")]
pub struct Cli {
    #[arg(long, default_value = "config.json", global = true)]
    pub config: PathBuf,
    #[command(subcommand)]
    pub command: Command,
}
#[derive(Subcommand, Debug)]
pub enum Command {
    Doctor {
        #[arg(long, default_value = "")]
        fsltl: String,
    },
    Scan {
        #[arg(long, default_value = "")]
        fsltl: String,
        #[arg(long, default_value_t = 30)]
        limit: usize,
    },
    Demo {
        #[arg(long, default_value_t = 240)]
        steps: u32,
        #[arg(long, default_value = "demo-results.json")]
        output: PathBuf,
    },
    AtcDemo {
        #[arg(long, default_value_t = 600)]
        steps: u32,
        #[arg(long, default_value = "atc-demo-results.json")]
        output: PathBuf,
    },
    ConvertAirport {
        #[arg(long)]
        xml: PathBuf,
        #[arg(long)]
        icao: Option<String>,
        #[arg(long)]
        output: PathBuf,
    },
    Run(RunOptions),
    Atc {
        #[arg(long,default_value_os_t=user_dir().join("airport-frequencies.csv"))]
        csv: PathBuf,
        #[command(subcommand)]
        command: AtcCommand,
    },
}
#[derive(Args, Debug, Clone)]
pub struct RunOptions {
    #[arg(long,default_value="mock",value_parser=["mock","simconnect"])]
    pub bridge: String,
    #[arg(long, default_value = "")]
    pub fsltl: String,
    #[arg(long)]
    pub allow_motion: bool,
    #[arg(long, default_value_t = 57.914)]
    pub lat: f64,
    #[arg(long, default_value_t = 56.02)]
    pub lon: f64,
    #[arg(long, default_value_t = 10000.)]
    pub alt_ft: f64,
    #[arg(long)]
    pub airport: Option<PathBuf>,
    #[arg(long)]
    pub metadata: Option<PathBuf>,
    #[arg(long)]
    pub atc_csv: Option<PathBuf>,
    #[arg(long)]
    pub atc_voice: bool,
    #[arg(long)]
    pub atc_airborne: bool,
    #[arg(long, default_value = "")]
    pub pilot_callsign: String,
    #[arg(long, default_value = "")]
    pub pilot_destination: String,
    #[arg(long, default_value = "")]
    pub pilot_runway: String,
    #[arg(long)]
    pub pilot_voice_model: Option<PathBuf>,
    #[arg(long)]
    pub atc_monitor_frequency: Option<f64>,
    #[arg(long, default_value_t = 0.)]
    pub duration: f64,
    #[arg(long)]
    pub stop_file: Option<PathBuf>,
}
#[derive(Subcommand, Debug)]
pub enum AtcCommand {
    Update,
    Frequencies {
        #[arg(long)]
        icao: String,
    },
    Session(SessionOptions),
}
#[derive(Args, Debug)]
pub struct SessionOptions {
    #[arg(long)]
    pub icao: String,
    #[arg(long)]
    pub destination: String,
    #[arg(long)]
    pub callsign: String,
    #[arg(long)]
    pub runway: String,
    #[arg(long)]
    pub com1: Option<f64>,
    #[arg(long)]
    pub cockpit: bool,
    #[arg(long)]
    pub voice: bool,
    #[arg(long)]
    pub vosk_model: Option<PathBuf>,
    #[arg(long)]
    pub ai_demo: bool,
}
pub fn fictional_radios() -> BTreeMap<String, f64> {
    [
        ("delivery", 121.6),
        ("ground", 121.9),
        ("tower", 118.7),
        ("departure", 124.2),
        ("center", 125.7),
        ("approach", 120.4),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v))
    .collect()
}
pub fn demo(settings: Settings, steps: u32) -> Result<Value> {
    ensure!((1..=7200).contains(&steps), "steps must be 1..7200");
    let mut ground = Ground::new(Airport::demo(), settings.ground_limit);
    let title = sources::Model::fixture().title;
    if settings.ground_limit > 0 {
        ground.add("TEST101", &title, "GATE_A", "RUNWAY_EXIT", false)?;
    }
    if settings.ground_limit > 1 {
        ground.add("TEST102", &title, "GATE_B", "RUNWAY_EXIT", false)?;
    }
    let mut manager = Manager::new(
        settings.clone(),
        vec![sources::Model::fixture()],
        MockBridge::new(),
        Some(ground),
    )?;
    let origin = Position::new(47.452, -122.3095, 5000.);
    let mut stats = Stats::default();
    let mut phases = BTreeMap::new();
    for sec in 0..steps {
        let now = 1e6 + sec as f64;
        if sec % 120 == 0 && settings.mode != "simulation" {
            manager.feed(
                vec![sources::Observation {
                    icao24: "demo-live-001".into(),
                    callsign: "DEM112".into(),
                    position: Position {
                        heading: 75.,
                        speed_kt: 210.,
                        ..Position::new(47.5, -122.30 + 0.01 * (sec / 120) as f64, 7000.)
                    },
                    vertical_mps: 0.,
                    observed_at: now - 2.,
                    icao_type: "A320".into(),
                }],
                origin,
                now,
            );
        }
        stats = manager.tick(1., now, origin, 8.min(settings.airborne_limit))?;
        ensure!(
            stats.live + stats.synthetic <= settings.airborne_limit
                && stats.ground <= settings.ground_limit,
            "traffic capacity violated"
        );
        if sec % 10 == 0 {
            phases.insert(
                sec.to_string(),
                manager
                    .ground
                    .as_ref()
                    .unwrap()
                    .aircraft
                    .iter()
                    .map(|(id, p)| (id.clone(), p.phase))
                    .collect::<BTreeMap<_, _>>(),
            );
        }
    }
    let active = manager.bridge.objects.len();
    let creates = manager.bridge.creates;
    let updates = manager.bridge.updates;
    manager.close()?;
    Ok(
        json!({"version":crate::VERSION,"mode":settings.mode,"model_origin":"embedded fixture (OFFLINE ONLY)","duration_seconds":steps,"limits":{"air":settings.airborne_limit,"ground":settings.ground_limit},"traffic":stats,"events":{"creates":creates,"updates":updates},"active_before_shutdown":active,"clean_shutdown":manager.bridge.objects.is_empty(),"phases":phases}),
    )
}
pub fn atc_demo(steps: u32) -> Result<Value> {
    ensure!((1..=7200).contains(&steps), "steps must be 1..7200");
    let graph = Airport::demo();
    let field = graph.nodes["RUNWAY_EXIT"].position();
    let heading = bearing(graph.nodes["RUNWAY_ENTRY"].position(), field);
    let mut ground = Ground::new(graph, 3);
    ground.add(
        "DEMO101",
        &sources::Model::fixture().title,
        "GATE_A",
        "RUNWAY_EXIT",
        false,
    )?;
    let settings = Settings {
        mode: "simulation".into(),
        airborne_limit: 8,
        ground_limit: 3,
        ..Settings::default()
    };
    let mut manager = Manager::new(
        settings,
        vec![sources::Model::fixture()],
        MockBridge::new(),
        Some(ground),
    )?;
    manager.ground_atc = Some(GroundAtc::new(fictional_radios()));
    let mut director = Director::new(fictional_radios(), "TEST/18".into(), Some(field));
    director.approach_heading = Some(heading);
    manager.director = Some(director);
    let mut pilot = Pilot::new("USR101", "TEST", "DEMO", fictional_radios(), "18")?;
    pilot.com1 = Some(121.6);
    pilot.transmit("request IFR clearance", &mut manager.runways)?;
    pilot.transmit("climb 5000 squawk 4101", &mut manager.runways)?;
    let (mut max_air, mut max_ground) = (0, 0);
    let mut seen = BTreeSet::new();
    for sec in 0..steps {
        let stats = manager.tick(
            1.,
            1e6 + sec as f64,
            Position {
                alt_ft: field.alt_ft + 2000.,
                ..field
            },
            3,
        )?;
        max_air = max_air.max(stats.synthetic);
        max_ground = max_ground.max(stats.ground);
        seen.extend(manager.ground.as_ref().unwrap().aircraft.keys().cloned());
    }
    let ai = manager.ground_atc.as_ref().unwrap().lines.len();
    let air = manager.director.as_ref().unwrap().lines.len();
    let active = manager.bridge.objects.len();
    let success = pilot.stage == "cleared" && max_air <= 8 && max_ground <= 3 && ai > 0 && air > 0;
    pilot.release(&mut manager.runways);
    manager.close()?;
    ensure!(success, "ATC scenario failed");
    Ok(
        json!({"simulation":"FICTIONAL OFFLINE ONLY","steps":steps,"pilot_stage":pilot.stage,"ai_radio_lines":ai,"air_atc_lines":air,"max_airborne":max_air,"max_ground":max_ground,"unique_ground_flights":seen.len(),"active_objects_before_shutdown":active,"clean_shutdown":manager.bridge.objects.is_empty(),"success":success}),
    )
}
fn write_report(path: &std::path::Path, value: Value) -> Result<()> {
    write_atomic(path, serde_json::to_string_pretty(&value)?.as_bytes())?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    println!("Report: {}", path.display());
    Ok(())
}
pub fn execute(cli: Cli) -> Result<()> {
    let mut settings = Settings::load(&cli.config)?;
    settings.resolve_paths(&cli.config);
    match cli.command {
        Command::Doctor { fsltl } => {
            let path = if fsltl.is_empty() {
                &settings.fsltl_path
            } else {
                &fsltl
            };
            let root = sources::discover(path);
            let count = root
                .as_ref()
                .map(|r| sources::scan(r).map(|m| m.len()))
                .transpose()?
                .unwrap_or(0);
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &json!({"version":crate::VERSION,"runtime":"native Rust; Python not required","platform":std::env::consts::OS,"fsltl_found":root.is_some(),"fsltl_path":root,"models":count,"simconnect_supported_os":cfg!(windows),"opensky_oauth2_configured":std::env::var_os("OPENSKY_CLIENT_ID").is_some()&&std::env::var_os("OPENSKY_CLIENT_SECRET").is_some()})
                )?
            );
        }
        Command::Scan { fsltl, limit } => {
            let path = if fsltl.is_empty() {
                &settings.fsltl_path
            } else {
                &fsltl
            };
            let root = sources::discover(path).context("FSLTL not found; specify --fsltl")?;
            let models = sources::scan(&root)?;
            println!(
                "Found {} aircraft titles in {}",
                models.len(),
                root.display()
            );
            for m in models.iter().take(limit) {
                println!("[{}] {}: {}", m.aircraft_type, m.airline, m.title);
            }
        }
        Command::Demo { steps, output } => write_report(&output, demo(settings, steps)?)?,
        Command::AtcDemo { steps, output } => write_report(&output, atc_demo(steps)?)?,
        Command::ConvertAirport { xml, icao, output } => {
            let data = convert_xml(&std::fs::read_to_string(xml)?, icao.as_deref())?;
            write_atomic(&output, serde_json::to_string_pretty(&data)?.as_bytes())?;
            println!(
                "{} nodes, {} edges. Verify geometry against MSFS before use.",
                data.nodes.len(),
                data.edges.len()
            );
        }
        Command::Run(args) => run(settings, args)?,
        Command::Atc { csv, command } => match command {
            AtcCommand::Update => println!(
                "Saved {} community frequency records",
                sources::update_frequencies(&csv)?
            ),
            AtcCommand::Frequencies { icao } => {
                let rows = sources::frequencies(&std::fs::read_to_string(csv)?)?;
                let rows: Vec<_> = rows
                    .iter()
                    .filter(|r| r.airport.eq_ignore_ascii_case(&icao))
                    .collect();
                ensure!(
                    !rows.is_empty(),
                    "No published frequencies; no fallback invented"
                );
                for f in rows {
                    println!(
                        "{}: {} {:.3} {}",
                        f.airport, f.station, f.mhz, f.description
                    );
                }
            }
            AtcCommand::Session(args) => {
                session(args, sources::frequencies(&std::fs::read_to_string(csv)?)?)?
            }
        },
    }
    Ok(())
}
fn input_queue(model: Option<PathBuf>) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::sync_channel(64);
    std::thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            let Ok(mut text) = line else {
                break;
            };
            if text.trim().is_empty()
                && let Some(model) = &model
            {
                match crate::voice::listen(model) {
                    Ok(t) => text = t,
                    Err(e) => {
                        eprintln!("Voice: {e}");
                        continue;
                    }
                }
            }
            if !text.trim().is_empty() && tx.try_send(text).is_err() {
                eprintln!("Radio queue busy; message dropped");
            }
        }
    });
    rx
}
fn stopping(stop_file: Option<&PathBuf>) -> Result<Arc<AtomicBool>> {
    if let Some(path) = stop_file {
        ensure!(
            !path.exists(),
            "Stop file already exists; use a new session path"
        );
    }
    let stop = Arc::new(AtomicBool::new(false));
    let s = stop.clone();
    ctrlc::set_handler(move || s.store(true, Ordering::Relaxed))?;
    Ok(stop)
}
fn handle_message(
    text: &str,
    pilot: &mut Pilot,
    runways: &mut crate::atc::Runways,
    cockpit: bool,
) -> Result<()> {
    if text == "/status" {
        println!(
            "Player: {} COM {:?} squawk {:?}",
            pilot.stage,
            pilot.tuned(),
            pilot.actual_squawk
        );
        return Ok(());
    }
    if let Some(mhz) = text.strip_prefix("/tune ") {
        ensure!(!cockpit, "Use cockpit COM panel to change frequency");
        let mhz = mhz.trim().parse()?;
        ensure!(
            crate::bridge::valid_mhz(mhz).is_some(),
            "Frequency must be 118..136.99 MHz"
        );
        pilot.com1 = Some(mhz);
        pilot.transmitting = 1;
        return Ok(());
    }
    ensure!(!text.starts_with('/'), "Unknown command");
    let reply = pilot.transmit(text, runways)?;
    println!("{}: {}", reply.speaker, reply.message);
    Ok(())
}
pub fn run(settings: Settings, args: RunOptions) -> Result<()> {
    ensure!(
        args.duration.is_finite() && args.duration >= 0.,
        "duration must be >=0"
    );
    let mut origin = Position::new(args.lat, args.lon, args.alt_ft);
    origin.validate()?;
    if args.bridge == "simconnect" {
        ensure!(cfg!(windows), "SimConnect requires Windows");
    }
    let fsltl = if args.fsltl.is_empty() {
        &settings.fsltl_path
    } else {
        &args.fsltl
    };
    let root = sources::discover(fsltl)
        .context("Installed FSLTL models required; no mock models injected")?;
    let models = sources::scan(&root)?;
    ensure!(!models.is_empty(), "FSLTL contains no models");
    let airport = args.airport.clone().or_else(|| {
        (!settings.airport_graph.is_empty()).then(|| PathBuf::from(&settings.airport_graph))
    });
    let graph = airport.as_ref().map(|p| Airport::load(p)).transpose()?;
    if args.bridge == "simconnect"
        && let Some(g) = &graph
    {
        ensure!(
            !g.fictional,
            "Fictional airport geometry refused in native mode"
        );
        ensure!(
            args.allow_motion,
            "Ground movement requires --allow-motion and a verified graph"
        );
    }
    let frequencies = args
        .atc_csv
        .as_ref()
        .map(|p| sources::frequencies(&std::fs::read_to_string(p)?))
        .transpose()?;
    let primary = if let Some(rows) = &frequencies {
        let graph = graph.as_ref().context("--atc-csv requires --airport")?;
        let primary = sources::primary(rows, &graph.icao);
        ensure!(
            primary.contains_key("ground") && primary.contains_key("tower"),
            "Published ground/tower frequencies required"
        );
        primary
    } else {
        BTreeMap::new()
    };
    ensure!(
        !args.atc_airborne || (primary.contains_key("approach") && graph.is_some()),
        "Airborne ATC requires airport and approach/tower CSV"
    );
    ensure!(
        (!args.atc_voice && args.pilot_voice_model.is_none()) || !primary.is_empty(),
        "Voice requires --atc-csv and airport"
    );
    let speaker = if args.atc_voice || args.pilot_voice_model.is_some() {
        Some(crate::voice::Speaker::new()?)
    } else {
        None
    };
    let stop = stopping(args.stop_file.as_ref())?;
    let metadata = args
        .metadata
        .as_ref()
        .map(|p| sources::metadata(p))
        .transpose()?
        .unwrap_or_default();
    let live = if settings.mode != "simulation" && settings.sync_real_flights {
        Some(sources::OpenSky::new(settings.fetch_seconds)?)
    } else {
        None
    };
    // HTTPS polling happens on a separate bounded worker, outside the movement loop.
    let (request_tx, request_rx) = mpsc::sync_channel::<Position>(1);
    let (observation_tx, observation_rx) = mpsc::sync_channel(1);
    let radius = settings.max_radius_km;
    if let Some(mut live) = live {
        std::thread::spawn(move || {
            while let Ok(pos) = request_rx.recv() {
                let result = live.poll(pos, radius).map(|mut rows| {
                    for o in &mut rows {
                        if let Some(t) = metadata.get(&o.icao24) {
                            o.icao_type = t.clone();
                        }
                    }
                    rows
                });
                if observation_tx.send(result).is_err() {
                    break;
                }
            }
        });
    }
    let bridge: Box<dyn Bridge> = if args.bridge == "simconnect" {
        native_bridge(args.allow_motion)?
    } else {
        Box::new(MockBridge::new())
    };
    let ground = graph.clone().map(|g| Ground::new(g, settings.ground_limit));
    let mut manager = Manager::new(settings.clone(), models, bridge, ground)?;
    if !primary.is_empty() {
        manager.ground_atc = Some(GroundAtc::new(primary.clone()));
    }
    if args.atc_airborne {
        let g = graph.as_ref().unwrap();
        let edge = g
            .edges
            .values()
            .find(|e| {
                !e.runway.is_empty()
                    && g.nodes[&e.src].kind == "runway"
                    && g.nodes[&e.dst].kind == "runway"
            })
            .context("Airport must contain a tagged runway segment")?;
        let field = g.nodes[&edge.dst].position();
        let mut d = Director::new(primary.clone(), edge.runway.clone(), Some(field));
        d.approach_heading = Some(bearing(g.nodes[&edge.src].position(), field));
        manager.director = Some(d);
    }
    let mut pilot = if !args.pilot_callsign.is_empty() {
        ensure!(
            !primary.is_empty(),
            "Pilot session requires ATC CSV and airport"
        );
        let graph = graph.as_ref().unwrap();
        let runways: BTreeSet<_> = graph
            .edges
            .values()
            .filter(|e| {
                !e.runway.is_empty()
                    && (e.runway.eq_ignore_ascii_case(&args.pilot_runway)
                        || e.runway
                            .rsplit('/')
                            .next()
                            .unwrap()
                            .eq_ignore_ascii_case(&args.pilot_runway))
            })
            .map(|e| e.runway.clone())
            .collect();
        ensure!(
            runways.len() == 1,
            "Pilot runway must map uniquely to graph"
        );
        let mut p = Pilot::new(
            &args.pilot_callsign,
            &graph.icao,
            &args.pilot_destination,
            primary.clone(),
            &args.pilot_runway,
        )?;
        p.runway_key = runways.into_iter().next().unwrap();
        p.com1 = args.atc_monitor_frequency;
        Some(p)
    } else {
        None
    };
    let input = input_queue(args.pilot_voice_model.clone());
    let mut monitor = PlayerMonitor::new(
        graph
            .as_ref()
            .and_then(|g| g.nodes.values().map(|n| n.alt_ft).min_by(f64::total_cmp))
            .unwrap_or(0.),
    );
    let started = Instant::now();
    let mut last = started;
    let mut next_fetch = 0.;
    let mut next_ground = 0.;
    let mut ground_counter = 0;
    let (mut ai_cursor, mut air_cursor) = (0, 0);
    println!("Started native traffic. SimConnect is experimental. Ctrl+C or /quit to stop.");
    let result = (|| -> Result<()> {
        while (args.duration == 0. || started.elapsed().as_secs_f64() < args.duration)
            && !stop.load(Ordering::Relaxed)
            && !args.stop_file.as_ref().is_some_and(|p| p.exists())
        {
            manager.bridge.poll()?;
            let now = Instant::now();
            let elapsed = started.elapsed().as_secs_f64();
            let dt = (now - last).as_secs_f64().clamp(0.05, 1.);
            last = now;
            let player = manager.bridge.player_position();
            if let Some(p) = player {
                origin = p;
            }
            if let Some(d) = &mut manager.director {
                d.player_position = player;
            }
            if let Some(p) = &mut pilot {
                if let Some(r) = manager.bridge.player_radio() {
                    p.com1 = r.com1;
                    p.com2 = r.com2;
                    p.transmitting = r.transmitting;
                    p.actual_squawk = r.squawk;
                }
                if let Some(line) = monitor.tick(p, player, elapsed, &mut manager.runways) {
                    println!("{}: {}", line.speaker, line.message);
                }
            }
            for text in input.try_iter().take(16) {
                if text == "/quit" {
                    stop.store(true, Ordering::Relaxed);
                    break;
                }
                if let Some(p) = &mut pilot {
                    let before = p.lines.len();
                    if let Err(e) =
                        handle_message(&text, p, &mut manager.runways, args.bridge == "simconnect")
                    {
                        eprintln!("Radio: {e}");
                    }
                    if let Some(speaker) = &speaker {
                        for line in &p.lines[before..] {
                            if line.kind != "pilot"
                                && line
                                    .mhz
                                    .zip(p.tuned())
                                    .is_some_and(|(a, b)| (a - b).abs() <= 0.001)
                            {
                                speaker.speak(&line.message);
                            }
                        }
                    }
                }
            }
            if settings.mode != "simulation" && settings.sync_real_flights && elapsed >= next_fetch
            {
                let _ = request_tx.try_send(origin);
                next_fetch = elapsed + settings.fetch_seconds as f64;
            }
            for result in observation_rx.try_iter() {
                match result {
                    Ok(rows) => {
                        println!("OpenSky: {} observations", rows.len());
                        manager.feed(
                            rows,
                            origin,
                            SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64(),
                        );
                    }
                    Err(e) => eprintln!("OpenSky: {e}"),
                }
            }
            if elapsed >= next_ground && settings.mode != "live" {
                next_ground = elapsed + 180.;
                if let Some(g) = &mut manager.ground {
                    let runway_exit = g
                        .airport
                        .edges
                        .values()
                        .find(|e| {
                            e.kind == "runway"
                                && g.airport.nodes[&e.src].kind == "runway"
                                && g.airport.nodes[&e.dst].kind == "runway"
                        })
                        .map(|e| e.dst.clone());
                    if let Some(end) = runway_exit {
                        let gates: Vec<_> = g
                            .airport
                            .nodes
                            .values()
                            .filter(|n| n.kind == "gate")
                            .map(|n| n.id.clone())
                            .collect();
                        for gate in gates {
                            if g.aircraft
                                .values()
                                .any(|p| !p.arrival && p.route[0] == gate)
                            {
                                continue;
                            }
                            if g.aircraft.len() >= settings.ground_limit {
                                break;
                            }
                            ground_counter += 1;
                            let model = &manager.models[ground_counter % manager.models.len()];
                            let _ = g.add(
                                &format!("AUTO-{ground_counter}"),
                                &model.title,
                                &gate,
                                &end,
                                false,
                            );
                        }
                    }
                }
            }
            manager.tick(
                dt,
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64(),
                origin,
                8,
            )?;
            let tuned = manager
                .bridge
                .player_radio()
                .and_then(|r| if r.transmitting == 2 { r.com2 } else { r.com1 })
                .or(args.atc_monitor_frequency);
            let emit = |lines: &[crate::atc::RadioLine], cursor: &mut usize| {
                for line in &lines[*cursor..] {
                    if line
                        .mhz
                        .zip(tuned)
                        .is_some_and(|(a, b)| (a - b).abs() <= 0.001)
                    {
                        println!("ATC {}: {}", line.speaker, line.message);
                        if let Some(s) = &speaker {
                            s.speak(&line.message);
                        }
                    }
                }
                *cursor = lines.len();
            };
            if let Some(a) = &manager.ground_atc {
                emit(&a.lines, &mut ai_cursor);
            }
            if let Some(d) = &manager.director {
                emit(&d.lines, &mut air_cursor);
            }
            io::stdout().flush()?;
            std::thread::sleep(Duration::from_millis(250));
        }
        Ok(())
    })();
    if let Some(p) = &mut pilot {
        p.release(&mut manager.runways);
    }
    let cleanup = manager.close();
    result?;
    cleanup?;
    println!("Clean shutdown complete.");
    Ok(())
}
fn session(args: SessionOptions, rows: Vec<sources::Frequency>) -> Result<()> {
    let frequencies = sources::primary(&rows, &args.icao);
    ensure!(!frequencies.is_empty(), "No published frequencies");
    let mut pilot = Pilot::new(
        &args.callsign,
        &args.icao,
        &args.destination,
        frequencies.clone(),
        &args.runway,
    )?;
    pilot.com1 = args.com1.or(frequencies.values().next().copied());
    let mut runways = crate::atc::Runways::default();
    let mut bridge = if args.cockpit {
        Some(native_bridge(false)?)
    } else {
        None
    };
    let mut ground = if args.ai_demo {
        let mut g = Ground::new(Airport::demo(), 2);
        g.add("SIM101", "OFFLINE ONLY", "GATE_A", "RUNWAY_EXIT", false)?;
        g.add("SIM102", "OFFLINE ONLY", "GATE_B", "RUNWAY_EXIT", false)?;
        Some(g)
    } else {
        None
    };
    let mut ai = GroundAtc::new(frequencies);
    let mut cursor = 0;
    let speaker = if args.voice {
        ensure!(args.vosk_model.is_some(), "Voice requires --vosk-model");
        Some(crate::voice::Speaker::new()?)
    } else {
        None
    };
    let input = input_queue(if args.voice {
        args.vosk_model.clone()
    } else {
        None
    });
    let stop = stopping(None)?;
    println!(
        "LOCAL simulated ATC. /status, /tune 121.900, /tick 10, /quit. Press empty Enter for microphone if enabled."
    );
    let result = (|| -> Result<()> {
        while !stop.load(Ordering::Relaxed) {
            if let Some(b) = &mut bridge {
                b.poll()?;
                if let Some(r) = b.player_radio() {
                    pilot.com1 = r.com1;
                    pilot.com2 = r.com2;
                    pilot.transmitting = r.transmitting;
                    pilot.actual_squawk = r.squawk;
                }
            }
            match input.recv_timeout(Duration::from_millis(100)) {
                Ok(text) => {
                    let text = text.trim();
                    if ["/quit", "/exit"].contains(&text) {
                        break;
                    }
                    if let Some(seconds) = text.strip_prefix("/tick ") {
                        if let Some(g) = &mut ground {
                            let seconds = seconds.parse::<u32>()?.min(300);
                            for _ in 0..seconds {
                                g.tick(1., &mut runways, Some(&mut ai))?;
                            }
                            for line in &ai.lines[cursor..] {
                                if line
                                    .mhz
                                    .zip(pilot.tuned())
                                    .is_some_and(|(a, b)| (a - b).abs() <= 0.001)
                                {
                                    println!("{}: {}", line.speaker, line.message);
                                }
                            }
                            cursor = ai.lines.len();
                        }
                        continue;
                    }
                    let before = pilot.lines.len();
                    if let Err(e) = handle_message(text, &mut pilot, &mut runways, args.cockpit) {
                        eprintln!("Radio: {e}");
                    }
                    if let Some(s) = &speaker {
                        for line in &pilot.lines[before..] {
                            if line.kind != "pilot"
                                && line
                                    .mhz
                                    .zip(pilot.tuned())
                                    .is_some_and(|(a, b)| (a - b).abs() <= 0.001)
                            {
                                s.speak(&line.message);
                            }
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
        Ok(())
    })();
    pilot.release(&mut runways);
    if let Some(b) = &mut bridge {
        b.close()?;
    }
    result
}
