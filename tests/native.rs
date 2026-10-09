use realflow::{
    airport::{Airport, convert_xml},
    atc::{GroundAtc, Pilot, PlayerMonitor, Runways, readback_contains},
    bridge::{Assigned, InitPosition, MockBridge, Recv, decode_squawk, valid_mhz},
    config::Settings,
    director::{Director, FlightClearance},
    engine::{Flight, Ground, Manager, Phase},
    geo::{Position, bearing, blend, distance, forward, heading_error},
    sources::{self, Model, Observation},
};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
fn radios() -> BTreeMap<String, f64> {
    realflow::cli::fictional_radios()
}
fn manager(settings: Settings, ground: bool) -> Manager<MockBridge> {
    let g = ground.then(|| Ground::new(Airport::demo(), settings.ground_limit));
    Manager::new(settings, vec![Model::fixture()], MockBridge::new(), g).unwrap()
}
fn flight(id: &str, p: Position, dest: Position, arrival: bool) -> Flight {
    Flight {
        id: id.into(),
        model_title: Model::fixture().title,
        position: p,
        destination: dest,
        remaining_s: 1800.,
        arrival,
    }
}
fn observation(id: &str, now: f64) -> Observation {
    Observation {
        icao24: id.into(),
        callsign: "DEM123".into(),
        position: Position {
            speed_kt: 240.,
            heading: 90.,
            ..Position::new(57.91, 56.02, 5000.)
        },
        vertical_mps: 0.,
        observed_at: now,
        icao_type: "A320".into(),
    }
}
#[test]
fn compatible_json_config_and_validation() {
    let settings = Settings::load(Path::new("config.json")).unwrap();
    assert_eq!(settings, Settings::default());
    for value in [36, usize::MAX] {
        assert!(
            Settings {
                airborne_limit: value,
                ..settings.clone()
            }
            .validate()
            .is_err()
        );
    }
    assert!(
        Settings {
            max_radius_km: f64::NAN,
            ..settings
        }
        .validate()
        .is_err()
    );
}
#[test]
fn profile_roundtrip_and_invalid_save_preserves_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("profile with spaces.json");
    let s = Settings {
        mode: "simulation".into(),
        airborne_limit: 3,
        ground_limit: 2,
        ..Settings::default()
    };
    s.save(&path).unwrap();
    assert_eq!(Settings::load(&path).unwrap(), s);
    let before = std::fs::read(&path).unwrap();
    assert!(
        Settings {
            airborne_limit: 99,
            ..s
        }
        .save(&path)
        .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}
#[test]
fn partial_config_uses_defaults_unknown_key_rejected() {
    let s: Settings = serde_json::from_str(r#"{"mode":"simulation"}"#).unwrap();
    assert_eq!(s.airborne_limit, 35);
    assert!(serde_json::from_str::<Settings>(r#"{"wrong_setting":true}"#).is_err());
}
#[test]
fn missing_config_only_not_found_defaults() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        Settings::load(&dir.path().join("missing.json")).unwrap(),
        Settings::default()
    );
    assert!(Settings::load(dir.path()).is_err());
}
#[test]
fn relative_paths_resolved_against_profile() {
    let directory = tempfile::tempdir().unwrap();
    let mut s = Settings {
        fsltl_path: "models".into(),
        airport_graph: "airport.json".into(),
        ..Settings::default()
    };
    s.resolve_paths(&directory.path().join("config.json"));
    assert_eq!(
        s.fsltl_path,
        directory.path().join("models").to_string_lossy()
    );
    assert_eq!(
        s.airport_graph,
        directory.path().join("airport.json").to_string_lossy()
    );
}
#[test]
fn geo_distance_bearing_forward() {
    let a = Position::new(47., -122., 0.);
    let b = forward(a, 90., 1000.);
    assert!((distance(a, b) - 1000.).abs() < 0.1);
    assert!((bearing(a, b) - 90.).abs() < 0.1);
    assert!(Position::new(91., 0., 0.).validate().is_err());
}
#[test]
fn antimeridian_blend_shortest_heading() {
    let a = Position {
        heading: 350.,
        ..Position::new(0., 179.9, 0.)
    };
    let b = Position {
        heading: 10.,
        ..Position::new(0., -179.9, 1000.)
    };
    let mid = blend(a, b, 0.5);
    assert!((mid.lon.abs() - 180.).abs() < 0.001);
    assert!(mid.heading < 0.001);
    assert_eq!(mid.alt_ft, 500.);
}
#[test]
fn graph_route_closed_and_forbidden() {
    let graph = Airport::demo();
    let route = graph
        .route("GATE_A", "RUNWAY_EXIT", &BTreeSet::new())
        .unwrap();
    assert!(route.contains(&"HOLD_R".into()));
    let blocked = BTreeSet::from([("JUNCTION_A".into(), "JUNCTION_B".into())]);
    assert!(
        graph
            .route("GATE_A", "RUNWAY_EXIT", &blocked)
            .unwrap()
            .is_empty()
    );
    assert!(
        graph
            .route("UNKNOWN", "RUNWAY_EXIT", &BTreeSet::new())
            .is_err()
    );
}
#[test]
fn graph_rejects_duplicate_unknown_nonpositive() {
    for graph in [
        r#"{"icao":"TEST","nodes":[{"id":"A","lat":0,"lon":0},{"id":"A","lat":1,"lon":1}],"edges":[]}"#,
        r#"{"icao":"TEST","nodes":[{"id":"A","lat":0,"lon":0}],"edges":[{"src":"A","dst":"B"}]}"#,
    ] {
        assert!(Airport::from_json(graph).is_err());
    }
}
#[test]
fn airport_xml_conversion() {
    let data = convert_xml(include_str!("../examples/sample_airport_source.xml"), None).unwrap();
    assert_eq!(data.icao, "TSTX");
    let graph = Airport::new(data).unwrap();
    assert_eq!(graph.nodes.len(), 5);
    assert_eq!(graph.route("N4", "N3", &BTreeSet::new()).unwrap().len(), 5);
    assert_eq!(graph.nodes["N1"].kind, "hold");
    assert!(graph.edges.values().any(|e| e.runway == "TSTX/18"));
    assert!(convert_xml("<FSData/>", None).is_err());
}
#[test]
fn runway_exclusion_owner_release() {
    let mut r = Runways::default();
    assert!(r.reserve("18", "A", "arrival", 0.).unwrap());
    assert!(!r.reserve("18", "B", "departure", 0.).unwrap());
    assert!(!r.release("18", "B"));
    assert!(r.release("18", "A"));
    assert!(r.reserve("18", "B", "departure", 0.).unwrap());
    r.free("B");
    assert!(r.reservations.is_empty());
    assert!(r.reserve("", "A", "arrival", 0.).is_err());
}
#[test]
fn ground_pushback_taxi_takeoff_cleanup() {
    let mut g = Ground::new(Airport::demo(), 1);
    let mut r = Runways::default();
    g.add("A", "MOCK", "GATE_A", "RUNWAY_EXIT", false).unwrap();
    let mut seen = BTreeSet::new();
    for _ in 0..1200 {
        g.tick(1., &mut r, None).unwrap();
        seen.insert(g.aircraft["A"].phase);
    }
    assert_eq!(g.aircraft["A"].phase, Phase::Airborne);
    for phase in [Phase::Pushback, Phase::TaxiOut, Phase::Takeoff] {
        assert!(seen.contains(&phase));
    }
    assert!(r.reservations.is_empty());
    assert!(g.edge_owners.is_empty());
}
#[test]
fn converging_ground_aircraft_separation() {
    let mut g = Ground::new(Airport::demo(), 2);
    let mut r = Runways::default();
    g.add("A", "x", "GATE_A", "RUNWAY_EXIT", false).unwrap();
    g.add("B", "x", "GATE_B", "RUNWAY_EXIT", false).unwrap();
    for _ in 0..1600 {
        g.tick(1., &mut r, None).unwrap();
        let (a, b) = (&g.aircraft["A"], &g.aircraft["B"]);
        if let (Some(a), Some(b)) = (&a.locked_edge, &b.locked_edge) {
            assert_ne!(BTreeSet::from([&a.0, &a.1]), BTreeSet::from([&b.0, &b.1]));
        }
        if a.phase != Phase::Airborne && b.phase != Phase::Airborne {
            assert!(distance(a.position, b.position) > 14.);
        }
    }
    assert_eq!(g.aircraft["A"].phase, Phase::Airborne);
    assert_eq!(g.aircraft["B"].phase, Phase::Airborne);
}
#[test]
fn ground_arrival_to_gate() {
    let mut g = Ground::new(Airport::demo(), 1);
    let mut r = Runways::default();
    g.add("ARR", "x", "RUNWAY_EXIT", "GATE_B", true).unwrap();
    for _ in 0..1800 {
        g.tick(1., &mut r, None).unwrap();
    }
    assert_eq!(g.aircraft["ARR"].phase, Phase::Complete);
}
#[test]
fn ground_capacity_and_removal() {
    let mut g = Ground::new(Airport::demo(), 1);
    let mut r = Runways::default();
    g.add("A", "x", "GATE_A", "RUNWAY_EXIT", false).unwrap();
    assert!(g.add("B", "x", "GATE_B", "RUNWAY_EXIT", false).is_err());
    g.tick(1., &mut r, None).unwrap();
    g.remove("A", &mut r);
    assert!(g.aircraft.is_empty() && g.edge_owners.is_empty() && g.node_owners.is_empty());
    assert!(g.tick(0., &mut r, None).is_err());
}
#[test]
fn fsltl_parser_section_boundary_and_matching() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("fsltl-traffic-base");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("aircraft.cfg"),"[FLTSIM.0]\ntitle=\"FSLTL A320 AFL\" ; comment\nicao_type_designator=A320\nicao_airline=AFL\n[FLTSIM.1]\ntitle=FSLTL A320 DLH\nicao_type_designator=A320\nicao_airline=DLH\n[GENERAL]\nicao_airline=XXX\n").unwrap();
    let models = sources::scan(&root).unwrap();
    assert_eq!(models.len(), 2);
    assert_eq!(models[1].airline, "DLH");
    assert_eq!(
        sources::choose(&models, "A320", "DLH").unwrap().airline,
        "DLH"
    );
    assert!(sources::choose(&models, "C172", "UAL").is_none());
    assert_eq!(sources::discover(dir.path().to_str().unwrap()), Some(root));
}
#[test]
fn opensky_columns_and_malformed_rows() {
    let states = json!({"time":1000,"states":[["abc123","AFL123 ","RU",990,995,56.02,57.91,2000,false,120,90,0.5,null,2100,"7000",false,0],[null],["x","","",0,0,0,999,100,false,0,0,0,null,null,"",false,0]]});
    let parsed = sources::parse_states(&states);
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].callsign, "AFL123");
    assert!((parsed[0].position.alt_ft - 6889.76).abs() < 1.);
    assert!(parsed[0].position.speed_kt > 200.);
}
#[test]
fn live_distance_ground_age_and_expiry() {
    let mut m = manager(
        Settings {
            airborne_limit: 1,
            ..Settings::default()
        },
        false,
    );
    let origin = Position::new(57.9, 56., 0.);
    let mut ground = observation("ground", 1000.);
    ground.position.on_ground = true;
    m.feed(
        vec![
            ground,
            observation("stale", 100.),
            observation("valid", 1000.),
        ],
        origin,
        1010.,
    );
    assert_eq!(m.fleet.len(), 1);
    m.tick(1., 1200., origin, 0).unwrap();
    assert!(m.fleet["valid"].predicted);
    m.tick(1., 1700., origin, 0).unwrap();
    assert!(m.fleet.is_empty());
}
#[test]
fn live_rejects_jump_and_out_of_order() {
    let mut m = manager(Settings::default(), false);
    let origin = Position::new(57.91, 56.02, 5000.);
    m.feed(vec![observation("A", 1000.)], origin, 1001.);
    let mut old = observation("A", 900.);
    old.position.alt_ft = 9000.;
    m.feed(vec![old], origin, 1002.);
    assert_eq!(m.fleet["A"].seen_at, 1000.);
    let mut jump = observation("A", 1010.);
    jump.position = forward(origin, 90., 100000.);
    m.feed(vec![jump], origin, 1011.);
    assert_eq!(m.fleet["A"].seen_at, 1000.);
}
#[test]
fn live_has_priority_with_hard_air_cap() {
    let mut m = manager(
        Settings {
            airborne_limit: 2,
            ..Settings::default()
        },
        false,
    );
    let origin = Position::new(57.91, 56.02, 5000.);
    m.tick(1., 1000., origin, 10).unwrap();
    assert_eq!(m.synthetic.len(), 2);
    m.feed(
        vec![observation("A", 1001.), observation("B", 1001.)],
        origin,
        1002.,
    );
    let stats = m.tick(1., 1002., origin, 10).unwrap();
    assert_eq!(stats.live, 2);
    assert_eq!(stats.synthetic, 0);
    assert_eq!(m.bridge.objects.len(), 2);
    m.close().unwrap();
    assert!(m.bridge.objects.is_empty());
    assert!(!m.bridge.connected);
}
#[test]
fn simulation_ignores_live_and_live_mode_no_synthetic() {
    let origin = Position::new(57.91, 56.02, 5000.);
    let mut sim = manager(
        Settings {
            mode: "simulation".into(),
            ..Settings::default()
        },
        false,
    );
    sim.feed(vec![observation("A", 1000.)], origin, 1001.);
    assert!(sim.fleet.is_empty());
    let mut live = manager(
        Settings {
            mode: "live".into(),
            ..Settings::default()
        },
        false,
    );
    assert_eq!(live.tick(1., 1001., origin, 10).unwrap().synthetic, 0);
}
#[test]
fn frequencies_no_fabricated_services() {
    let text = "airport_ident,type,frequency_mhz,description\nTEST,TWR,118.700,tower\nTEST,GND,121.900,ground\nTEST,FOO,119.5,unknown\nTEST,TWR,99.0,invalid\n";
    let rows = sources::frequencies(text).unwrap();
    let map = sources::primary(&rows, "TEST");
    assert_eq!(map.len(), 2);
    assert_eq!(map["tower"], 118.7);
    assert!(!map.contains_key("center"));
    assert!(sources::frequencies("airport,frequency\nTEST,118.7\n").is_err());
}
#[test]
fn pilot_clearance_requires_full_readback() {
    let mut p = Pilot::new("AFL101", "TEST", "UUEE", radios(), "18").unwrap();
    let mut r = Runways::default();
    p.com1 = Some(121.6);
    assert_eq!(
        p.transmit("request IFR clearance", &mut r).unwrap().kind,
        "clearance"
    );
    assert_eq!(
        p.transmit("cleared to destination", &mut r).unwrap().kind,
        "correction"
    );
    assert_eq!(p.stage, "filed");
    assert_eq!(
        p.transmit("climb 5000 squawk 4101", &mut r).unwrap().kind,
        "ack"
    );
    assert_eq!(p.stage, "cleared");
}
#[test]
fn spoken_readback_and_digit_boundaries() {
    assert!(readback_contains(
        "climb five thousand squawk four one zero one",
        "5000"
    ));
    assert!(readback_contains("runway two four left hold short", "24L"));
    assert!(!readback_contains("runway 124L", "24L"));
    assert!(!readback_contains("15000", "5000"));
}
#[test]
fn pilot_full_departure_and_runway_release() {
    let mut p = Pilot::new("AFL101", "TEST", "UUEE", radios(), "18").unwrap();
    let mut r = Runways::default();
    for (freq, text) in [
        (121.6, "request clearance"),
        (121.6, "5000 4101"),
        (121.9, "request pushback"),
        (121.9, "pushback approved"),
        (121.9, "request taxi"),
        (121.9, "runway 18 hold short"),
        (118.7, "ready for departure"),
        (118.7, "runway 18 cleared for takeoff"),
    ] {
        p.com1 = Some(freq);
        p.transmit(text, &mut r).unwrap();
    }
    assert_eq!(p.stage, "departure");
    assert!(!r.available("18", "OTHER"));
    p.transmit("airborne", &mut r).unwrap();
    assert_eq!(p.stage, "airborne");
    assert!(r.available("18", "OTHER"));
}
#[test]
fn pilot_and_ai_share_runway_and_squawk_checks() {
    let mut p = Pilot::new("AFL101", "TEST", "UUEE", radios(), "18").unwrap();
    p.stage = "taxi".into();
    p.com1 = Some(118.7);
    let mut r = Runways::default();
    r.reserve("18", "SIM101", "departure", 0.).unwrap();
    assert_eq!(
        p.transmit("ready for departure", &mut r).unwrap().kind,
        "hold"
    );
    r.free("SIM101");
    p.actual_squawk = Some("7000".into());
    assert_eq!(
        p.transmit("ready for departure", &mut r).unwrap().kind,
        "correction"
    );
    p.actual_squawk = Some("4101".into());
    assert_eq!(
        p.transmit("ready for departure", &mut r).unwrap().kind,
        "clearance"
    );
    p.release(&mut r);
    assert!(r.reservations.is_empty());
}
#[test]
fn pilot_wrong_frequency_no_advance() {
    let mut p = Pilot::new("AFL101", "TEST", "UUEE", radios(), "18").unwrap();
    let mut r = Runways::default();
    p.com1 = Some(118.3);
    assert_eq!(
        p.transmit("request clearance", &mut r).unwrap().kind,
        "unavailable"
    );
    p.com1 = Some(118.7);
    assert_eq!(
        p.transmit("request clearance", &mut r).unwrap().kind,
        "handoff"
    );
    assert_eq!(p.stage, "filed");
}
#[test]
fn ground_atc_blocks_runway_and_emits_clearance_after_release() {
    let mut g = Ground::new(Airport::demo(), 1);
    let mut r = Runways::default();
    let mut atc = GroundAtc::new(radios());
    g.add("A", "x", "GATE_A", "RUNWAY_EXIT", false).unwrap();
    r.reserve("TEST/18", "OTHER", "arrival", 0.).unwrap();
    for _ in 0..500 {
        g.tick(1., &mut r, Some(&mut atc)).unwrap();
    }
    assert_eq!(g.aircraft["A"].phase, Phase::HoldShort);
    assert!(atc.lines.iter().any(|l| l.kind == "hold"));
    assert!(
        !atc.lines
            .iter()
            .any(|l| l.message.contains("cleared for takeoff"))
    );
    r.free("OTHER");
    for _ in 0..600 {
        g.tick(1., &mut r, Some(&mut atc)).unwrap();
    }
    assert_eq!(g.aircraft["A"].phase, Phase::Airborne);
    assert!(
        atc.lines
            .iter()
            .any(|l| l.kind == "readback" && l.message.contains("takeoff"))
    );
}
#[test]
fn flight_heading_altitude_speed_limited() {
    let start = Position {
        heading: 270.,
        speed_kt: 205.,
        ..Position::new(47., -122., 9000.)
    };
    let mut f = flight("A", start, forward(start, 70., 50000.), false);
    let c = FlightClearance {
        heading: 70.,
        altitude: 15000.,
        speed: 310.,
        phase: "enroute".into(),
    };
    for _ in 0..10 {
        let before = f.position;
        Director::move_flight(&mut f, &c, 1.);
        assert!(heading_error(f.position.heading, before.heading).abs() <= 3.001);
        assert!((f.position.alt_ft - before.alt_ft).abs() <= 1900. / 60. + 0.01);
        assert!((f.position.speed_kt - before.speed_kt).abs() <= 3.001);
        assert!(f.position.bank.abs() <= 25.);
    }
}
#[test]
fn occupied_runway_goaround_heading_stable() {
    let airport = Position::new(47., -122., 200.);
    let mut r = Runways::default();
    r.reserve("18", "PLAYER", "departure", 0.).unwrap();
    let start = Position {
        alt_ft: 1400.,
        heading: 0.,
        speed_kt: 180.,
        ..forward(airport, 180., 7000.)
    };
    let mut fleet = BTreeMap::from([("A".into(), flight("A", start, airport, true))]);
    let mut d = Director::new(radios(), "18".into(), Some(airport));
    d.tick(&mut fleet, 1., &mut r).unwrap();
    assert_eq!(d.tracks["A"].clearance.phase, "go-around");
    let heading = d.tracks["A"].go_heading.unwrap();
    fleet.get_mut("A").unwrap().position = Position {
        alt_ft: 3000.,
        speed_kt: 180.,
        ..forward(airport, 180., 12000.)
    };
    d.tick(&mut fleet, 1., &mut r).unwrap();
    assert_eq!(d.tracks["A"].clearance.heading, heading);
    assert_eq!(d.tracks["A"].clearance.phase, "go-around");
    d.close(&mut r);
    assert_eq!(r.reservations["18"].aircraft_id, "PLAYER");
}
#[test]
fn ai_arrival_reserves_and_releases_runway() {
    let apt = Position::new(47., -122., 200.);
    let start = Position {
        alt_ft: 250.,
        speed_kt: 140.,
        ..forward(apt, 180., 800.)
    };
    let mut fleet = BTreeMap::from([("A".into(), flight("A", start, apt, true))]);
    let mut d = Director::new(radios(), "18".into(), Some(apt));
    let mut r = Runways::default();
    d.tick(&mut fleet, 1., &mut r).unwrap();
    assert_eq!(r.reservations["18"].aircraft_id, "A");
    d.forget("A", false, &mut r);
    assert!(r.reservations.is_empty());
}
#[test]
fn player_has_priority_over_synthetic() {
    let start = Position {
        heading: 80.,
        speed_kt: 200.,
        ..Position::new(47., -122., 5000.)
    };
    let mut fleet = BTreeMap::from([(
        "A".into(),
        flight("A", start, forward(start, 85., 75000.), false),
    )]);
    let mut d = Director::new(radios(), String::new(), None);
    d.player_position = Some(Position::new(47., -122.001, 5050.));
    d.tick(&mut fleet, 1., &mut Runways::default()).unwrap();
    assert_eq!(d.tracks["A"].clearance.phase, "vector");
    assert!(d.tracks["A"].clearance.altitude >= 6100.);
}
#[test]
fn departure_climbs_and_preserves_bridge_identity() {
    let mut m = manager(
        Settings {
            mode: "simulation".into(),
            airborne_limit: 2,
            ground_limit: 2,
            ..Settings::default()
        },
        true,
    );
    let apt = m.ground.as_ref().unwrap().airport.nodes["RUNWAY_EXIT"].position();
    m.ground
        .as_mut()
        .unwrap()
        .add("A", "MOCK", "GATE_A", "RUNWAY_EXIT", false)
        .unwrap();
    m.director = Some(Director::new(radios(), "TEST/18".into(), Some(apt)));
    for n in 0..1200 {
        m.tick(1., 1000. + n as f64, apt, 0).unwrap();
        if m.synthetic.contains_key("GROUND-A") {
            break;
        }
    }
    assert!(m.synthetic.contains_key("GROUND-A"));
    assert!(!m.ground.as_ref().unwrap().aircraft.contains_key("A"));
    assert_eq!(
        m.bridge
            .created_keys
            .iter()
            .filter(|k| k.as_str() == "GROUND-A")
            .count(),
        1
    );
    let before = m.synthetic["GROUND-A"].position.alt_ft;
    m.tick(1., 2400., apt, 0).unwrap();
    assert!(m.synthetic["GROUND-A"].position.alt_ft > before);
}
#[test]
fn arrival_handoff_to_near_runway_preserves_identity() {
    let mut m = manager(
        Settings {
            mode: "simulation".into(),
            airborne_limit: 2,
            ground_limit: 2,
            ..Settings::default()
        },
        true,
    );
    let apt = m.ground.as_ref().unwrap().airport.nodes["RUNWAY_EXIT"].position();
    m.director = Some(Director::new(radios(), "TEST/18".into(), Some(apt)));
    let p = Position {
        alt_ft: apt.alt_ft + 50.,
        heading: 0.,
        speed_kt: 140.,
        ..forward(apt, 180., 65.)
    };
    m.synthetic
        .insert("SYN-ARR".into(), flight("SYN-ARR", p, apt, true));
    for n in 0..30 {
        m.tick(0.25, 1000. + n as f64 * 0.25, apt, 0).unwrap();
        if m.ground.as_ref().unwrap().aircraft.contains_key("SYN-ARR") {
            break;
        }
    }
    assert!(m.ground.as_ref().unwrap().aircraft.contains_key("SYN-ARR"));
    assert_eq!(
        m.bridge
            .created_keys
            .iter()
            .filter(|k| k.as_str() == "SYN-ARR")
            .count(),
        1
    );
}
#[test]
fn player_monitor_detects_departure_then_sustained_deviation() {
    let mut p = Pilot::new("AFL101", "TEST", "UUEE", radios(), "18").unwrap();
    let mut r = Runways::default();
    p.stage = "departure".into();
    p.owns_runway = true;
    r.reserve("18", "AFL101", "departure", 0.).unwrap();
    let mut monitor = PlayerMonitor::new(450.);
    assert!(
        monitor
            .tick(&mut p, Some(Position::new(47., -122., 900.)), 0., &mut r)
            .is_some()
    );
    assert_eq!(p.stage, "airborne");
    assert!(r.reservations.is_empty());
    p.stage = "cruise".into();
    monitor.tick(&mut p, Some(Position::new(47., -122., 30000.)), 10., &mut r);
    assert!(
        monitor
            .tick(&mut p, Some(Position::new(47., -122., 31000.)), 20., &mut r)
            .is_none()
    );
    assert_eq!(
        monitor
            .tick(&mut p, Some(Position::new(47., -122., 31000.)), 41., &mut r)
            .unwrap()
            .kind,
        "warning"
    );
}
#[test]
fn abi_sizes_and_radio_decoding() {
    assert_eq!(std::mem::size_of::<InitPosition>(), 56);
    assert_eq!(std::mem::size_of::<Recv>(), 12);
    assert_eq!(std::mem::size_of::<Assigned>(), 20);
    assert_eq!(valid_mhz(118.7), Some(118.7));
    assert_eq!(valid_mhz(70.), None);
    assert_eq!(decode_squawk(4101.), Some("4101".into()));
    assert_eq!(decode_squawk(8888.), None);
}
#[test]
fn demo_and_atc_scenario_functional() {
    let demo = realflow::cli::demo(Settings::default(), 240).unwrap();
    assert_eq!(demo["clean_shutdown"], true);
    assert!(demo["events"]["creates"].as_u64().unwrap() > 0);
    let atc = realflow::cli::atc_demo(600).unwrap();
    assert_eq!(atc["success"], true);
    assert_eq!(atc["clean_shutdown"], true);
    assert!(atc["air_atc_lines"].as_u64().unwrap() > 0);
    assert!(realflow::cli::atc_demo(0).is_err());
}
#[test]
fn native_binary_runs_outside_repository_no_python() {
    let dir = tempfile::tempdir().unwrap();
    let profile = dir.path().join("profile with spaces.json");
    Settings {
        mode: "simulation".into(),
        airborne_limit: 2,
        ground_limit: 1,
        ..Settings::default()
    }
    .save(&profile)
    .unwrap();
    let report = dir.path().join("report.json");
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_realflow"))
        .current_dir(dir.path())
        .args([
            "--config",
            profile.to_str().unwrap(),
            "demo",
            "--steps",
            "240",
            "--output",
            report.to_str().unwrap(),
        ])
        .env("PATH", dir.path())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let data: serde_json::Value = serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
    assert_eq!(data["mode"], "simulation");
    assert_eq!(data["limits"], json!({"air":2,"ground":1}));
    assert_eq!(data["clean_shutdown"], true);
}
#[cfg(feature = "gui")]
#[test]
fn launcher_arguments_match_native_cli() {
    let dir = tempfile::tempdir().unwrap();
    let profile = dir.path().join("profile with spaces.json");
    Settings::default().save(&profile).unwrap();
    let args = realflow::launcher::command_args(
        &Settings::default(),
        &profile,
        &realflow::launcher::LaunchOptions::default(),
        "demo",
        &dir.path().join("stop"),
    )
    .unwrap();
    use clap::Parser;
    let cli =
        realflow::cli::Cli::try_parse_from(std::iter::once("realflow".to_owned()).chain(args))
            .unwrap();
    assert!(matches!(
        cli.command,
        realflow::cli::Command::Demo { steps: 600, .. }
    ));
}

#[test]
fn mock_run_stop_file_performs_graceful_shutdown() {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("fsltl-traffic-base");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(
        root.join("aircraft.cfg"),
        "[FLTSIM.0]\ntitle=OFFLINE TEST A320\nicao_type_designator=A320\n",
    )
    .unwrap();
    let profile = dir.path().join("config.json");
    Settings {
        mode: "simulation".into(),
        fsltl_path: root.to_string_lossy().into_owned(),
        ..Settings::default()
    }
    .save(&profile)
    .unwrap();
    let stop = dir.path().join("stop");
    let mut child = Command::new(env!("CARGO_BIN_EXE_realflow"))
        .args([
            "--config",
            profile.to_str().unwrap(),
            "run",
            "--bridge",
            "mock",
            "--stop-file",
            stop.to_str().unwrap(),
            "--duration",
            "10",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut started = String::new();
    reader.read_line(&mut started).unwrap();
    assert!(started.contains("Started native traffic"), "{started}");
    std::fs::write(stop, []).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if std::time::Instant::now() > deadline {
            child.kill().unwrap();
            panic!("Mock session did not stop");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    assert!(status.success());
    let mut output = String::new();
    std::io::Read::read_to_string(&mut reader, &mut output).unwrap();
    assert!(output.contains("Clean shutdown complete."));
}

#[test]
fn metadata_type_enrichment_csv() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("types.csv");
    std::fs::write(&path, "icao24,icao_type\n ABCDEF , a320 \n").unwrap();
    assert_eq!(sources::metadata(&path).unwrap()["abcdef"], "A320");
    std::fs::write(&path, "wrong,columns\n").unwrap();
    assert!(sources::metadata(&path).is_err());
}
