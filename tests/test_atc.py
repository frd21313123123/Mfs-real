"""ATC tests never require a microphone, internet or MSFS."""
from pathlib import Path
from realflow.frequencies import FrequencyDirectory
from realflow.atc import PilotATC, AIAirportController
from realflow.runway import RunwayController
from realflow.airport import AirportGraph
from realflow.ground import GroundEngine, Phase
from realflow.cockpit_radio import _decode_squawk, _valid_mhz

FREQUENCIES = {
    "delivery": 121.6, "ground": 121.9, "tower": 118.7,
    "departure": 124.2, "center": 126.4, "approach": 119.1,
}
CSV = """id,airport_ref,airport_ident,type,description,frequency_mhz
1,3,TEST,TWR,fictional test tower,118.700
2,3,TEST,GND,fictional test ground,121.900
3,3,TEST,CLR,fictional delivery,121.600
4,4,UUEE,APP,published approach,119.100
5,3,TEST,FOO,unknown service,119.500
6,3,TEST,TWR,invalid,99.999
"""


def test_frequency_import_does_not_invent_services():
    directory = FrequencyDirectory.from_csv_text(CSV)
    assert directory.primary("TEST") == {
        "tower": 118.7, "ground": 121.9, "delivery": 121.6
    }
    assert directory.primary("TEST").get("center") is None
    assert len(directory.resolve("TEST", 118.7)) == 1
    assert directory.lookup("UUEE", "approach")[0].mhz == 119.1
    assert not directory.lookup("NOTF")


def test_frequency_import_rejects_incompatible_file():
    try:
        FrequencyDirectory.from_csv_text("airport,frequency\nTEST,118.7\n")
    except ValueError:
        pass
    else:
        assert False, "missing columns must be rejected"


def test_radio_changes_from_cockpit_com1_com2():
    pilot = PilotATC("AFL101", "TEST", "UUEE", FREQUENCIES, "18")
    pilot.update_cockpit(121.6, 121.9, 1, "4101")
    assert pilot.station == "delivery"
    pilot.update_cockpit(121.6, 121.9, 2, "4101")
    assert pilot.station == "ground"
    pilot.update_cockpit(118.3, 118.4, 1)
    assert pilot.station is None
    pilot.close()


def test_readback_must_be_correct_before_progress():
    pilot = PilotATC("AFL101", "TEST", "UUEE", FREQUENCIES, "18")
    pilot.update_cockpit(121.6)
    assert pilot.transmit("request IFR clearance").kind == "clearance"
    assert pilot.stage == "filed"
    assert pilot.transmit("cleared to destination").kind == "correction"
    assert pilot.stage == "filed"
    assert pilot.transmit("climb 5000 squawk 4101").kind == "ack"
    assert pilot.stage == "cleared"
    pilot.update_cockpit(121.9)
    assert pilot.transmit("request pushback").kind == "clearance"
    assert pilot.transmit("pushback approved").kind == "ack"
    assert pilot.transmit("request taxi").kind == "clearance"
    assert pilot.transmit("runway 18 hold short").kind == "ack"
    pilot.update_cockpit(118.7)
    assert pilot.transmit("ready for departure").kind == "clearance"
    assert pilot.transmit("runway 18 cleared for takeoff").kind == "ack"
    assert pilot.stage == "departure"
    assert pilot.runways.is_available("18", "OTHER") is False
    pilot.transmit("airborne")
    assert pilot.runways.is_available("18", "OTHER") is True
    pilot.close()


def test_player_and_ai_share_runway_lock():
    runways = RunwayController()
    pilot = PilotATC("AFL101", "TEST", "UUEE", FREQUENCIES, "18", runways)
    pilot.stage = "taxi"
    pilot.update_cockpit(118.7)
    assert runways.reserve("18", "SIM101", "departure")
    assert pilot.transmit("ready for departure").kind == "hold"
    assert pilot.stage == "taxi"
    runways.release("18", "SIM101")
    assert pilot.transmit("ready for departure").kind == "clearance"
    pilot.close()
    assert runways.is_available("18", "SIM101")


def test_ai_cannot_enter_runway_without_conflict_resolution():
    from realflow.airport import Node, Edge
    from realflow.geo import Position
    points = [
        Node("G", Position(47.0, -122.0, 100, on_ground=True), "gate"),
        Node("P", Position(47.0002, -122.0, 100, on_ground=True)),
        Node("H", Position(47.0004, -122.0, 100, on_ground=True), "hold"),
        Node("R", Position(47.0006, -122.0, 100, on_ground=True), "runway"),
    ]
    graph = AirportGraph(points, [Edge("G", "P", 3, "pushback"),
                                  Edge("P", "H", 10, "taxi"),
                                  Edge("H", "R", 20, "runway", "18")], "TEST")
    runways = RunwayController()
    ground = GroundEngine(graph, runways=runways)
    atc = AIAirportController(FREQUENCIES, runways)
    ground.clearance_provider = atc.allow_edge
    plane = ground.add("SIM101", "MOCK", "G", "R")
    assert runways.reserve("18", "OTHER", "arrival")
    for _ in range(160):
        ground.tick(1)
    assert plane.cursor <= 2
    assert plane.phase == Phase.HOLD_SHORT
    assert any(line.kind == "hold" for line in atc.lines)
    assert not any("cleared for takeoff" in line.message for line in atc.lines)
    runways.release("18", "OTHER")
    for _ in range(90):
        ground.tick(1)
    assert any("cleared for takeoff" in line.message for line in atc.lines)
    assert any(line.kind == "readback" for line in atc.lines)


def test_simconnect_radio_helpers():
    assert _valid_mhz(118.7) == 118.7
    assert _valid_mhz(70.0) is None
    assert _decode_squawk(4101) == "4101"
    assert _decode_squawk(7777) == "7777"
    assert _decode_squawk(8888) is None
