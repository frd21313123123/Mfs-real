"""Airborne ATC / ground-to-air transfer: reproducible offline checks."""
from realflow.geo import Position, forward, distance_m
from realflow.synthetic import SimulatedFlight
from realflow.flight_director import AIFlightDirector, FlightClearance, heading_error
from realflow.runway import RunwayController
from realflow.config import Settings
from realflow.bridge import MockBridge
from realflow.manager import TrafficManager


def flight(id, position, dest, arrival=False):
    return SimulatedFlight(id, "TEST MODEL", position, dest, 1800, arrival)


def test_heading_altitude_speed_limited_per_tick():
    start = Position(47.0, -122.0, 9000, 270, 205, False)
    target = forward(start, 70, 50000)
    target = Position(target.lat, target.lon, 10000)
    ac = flight("SYN-100", start, target)
    atc = AIFlightDirector({"center": 125.7})
    fleet = {ac.id: ac}
    for _ in range(10):
        last = ac.position
        atc.tick(fleet, 1.0)
        assert abs(heading_error(ac.position.heading, last.heading)) <= 3.0001
        assert abs(ac.position.alt_ft-last.alt_ft) <= 1900/60+0.01
        assert abs(ac.position.speed_kt-last.speed_kt) <= 3.0001
        assert abs(ac.position.bank) <= 25
        assert ac.position.on_ground is False
    assert any(line.kind == "readback" for line in atc.lines)


def test_busy_runway_triggers_goaround_with_lock_intact():
    airport = Position(47.0, -122.0, 200, 180)
    start = forward(airport, 180, 7000)
    ac = flight("SYN-202", Position(start.lat,start.lon,1400,0,180),
                airport,arrival=True)
    runways = RunwayController()
    assert runways.reserve("18", "PLAYER", "departure")
    atc = AIFlightDirector({"tower": 118.7,"approach": 121.1},
                           runways, "18", 200, airport)
    atc.tick({ac.id:ac}, 1.0)
    assert atc.tracks[ac.id].clearance.phase == "go-around"
    assert atc.tracks[ac.id].go_arounds == 1
    assert runways.reservations["18"].aircraft_id == "PLAYER"
    assert not atc.tracks[ac.id].owns_runway
    assert any("go around" in line.message for line in atc.lines)
    atc.close()
    assert runways.reservations["18"].aircraft_id == "PLAYER"


def test_runway_reservation_and_release_for_ai_arrival():
    airport = Position(47.0,-122.0,200)
    near = forward(airport,180,800)
    ac = flight("SYN-203", Position(near.lat,near.lon,250,0,140),
                airport,arrival=True)
    runways = RunwayController()
    atc = AIFlightDirector({"tower":118.7,"approach":121.1},
                            runways,"18",200,airport)
    atc.tick({ac.id: ac}, 1.0)
    assert runways.reservations["18"].aircraft_id == ac.id
    atc.forget(ac.id)
    assert not runways.reservations


def test_player_has_precedence_over_synthetic_ai():
    start = Position(47.0,-122.0,5000,80,200,False)
    dest = forward(start,85,75000)
    ac = flight("SYN-204",start,dest)
    atc = AIFlightDirector({"center":125.7})
    atc.player_position = Position(47.0,-122.001,5050,90,210,False)
    atc.tick({ac.id: ac}, 1)
    assert atc.tracks[ac.id].clearance.phase=="vector"
    assert atc.tracks[ac.id].clearance.altitude_ft >= 6100
    assert ac.position.lat != start.lat or ac.position.lon != start.lon


def test_departing_ground_ai_keeps_same_bridge_key(matcher,graph):
    settings = Settings(mode="simulation",airborne_limit=2,ground_limit=2)
    bridge = MockBridge()
    from realflow.ground import GroundEngine
    ground=GroundEngine(graph,max_ground=2)
    ground.add("A","FSLTL Airbus A320 AFL","GATE_A","RUNWAY_EXIT")
    atc=AIFlightDirector({"center":125.7})
    manager=TrafficManager(settings,matcher,bridge,ground,flight_director=atc)
    try:
        for n in range(1200):
            manager.tick(1,1000+n,Position(47.452,-122.309,5000),synthetic_target=0)
            if "GROUND-A" in manager.synthetic.aircraft:
                break
        assert "GROUND-A" in manager.synthetic.aircraft
        assert "A" not in ground.aircraft
        assert "GROUND-A" in bridge.objects
        assert sum(event[0]=="create" and event[1]=="GROUND-A" for event in bridge.events)==1
        assert not ground.runways.reservations
    finally:
        manager.close()
    assert bridge.objects=={}


def test_arrival_handoff_only_with_validated_near_runway(matcher,graph):
    settings=Settings(mode="simulation",airborne_limit=2,ground_limit=2)
    bridge=MockBridge()
    from realflow.ground import GroundEngine
    airport=graph.nodes["RUNWAY_EXIT"].position
    ground=GroundEngine(graph,max_ground=2)
    atc=AIFlightDirector({"tower":118.7,"approach":120.4},
                          ground.runways,"RWY",airport.alt_ft,airport)
    manager=TrafficManager(settings,matcher,bridge,ground,flight_director=atc)
    near=forward(airport,180,65)
    approach=flight("SYN-ARR",Position(near.lat,near.lon,airport.alt_ft+50,0,140,False),
                   airport,True)
    manager.synthetic.aircraft[approach.id]=approach
    try:
        for n in range(30):
            manager.tick(0.25,1000+n*0.25,airport,synthetic_target=0)
            if approach.id in ground.aircraft:
                break
        assert approach.id in ground.aircraft
        assert "SYN-ARR" in bridge.objects
        assert sum(e[0]=="create" and e[1]=="SYN-ARR" for e in bridge.events)==1
    finally:
        manager.close()



def test_go_around_heading_remains_stable_outside_final_radius():
    airport=Position(47,-122,450)
    start=forward(airport,180,8500)
    ac=flight("SYN-204GO",Position(start.lat,start.lon,2500,0,180),airport,True)
    runways=RunwayController()
    runways.reserve("RWY18","PLAYER","departure")
    director=AIFlightDirector({"approach":120.4,"tower":118.7},
                               runways,"RWY18",450,airport)
    director.tick({ac.id:ac},1)
    heading=director.tracks[ac.id].go_around_heading
    assert heading is not None
    # Move the plane outside the trigger radius while the clearance remains in force.
    far=forward(airport,180,12000)
    ac.position=Position(far.lat,far.lon,3000,0,180,False)
    director.tick({ac.id:ac},1)
    clearance=director.tracks[ac.id].clearance
    assert clearance.phase=="go-around"
    assert clearance.heading_deg==heading
    director.close()



def test_departing_ai_climbs_on_assigned_departure_clearance():
    airport=Position(47,-122,450)
    destination=forward(airport,90,85000)
    flight_out=flight("GROUND-TEST1",
                      Position(airport.lat,airport.lon,450,90,125,False),
                      destination,False)
    atc=AIFlightDirector({"departure":124.2,"center":127.8},
                          RunwayController(),"18",450,airport)
    atc.tick({flight_out.id:flight_out},1)
    assert atc.tracks[flight_out.id].clearance.phase=="departure"
    assert atc.tracks[flight_out.id].clearance.altitude_ft>=8450
    assert flight_out.position.alt_ft>450
    assert any(e.station=="departure" and e.kind=="atc" for e in atc.lines)
    atc.close()
