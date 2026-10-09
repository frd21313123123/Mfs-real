"""Additional radio phraseology/regression tests."""
from realflow.atc import PilotATC, AIAirportController, _readback_contains
from realflow.runway import RunwayController
from realflow.airport import Edge
from types import SimpleNamespace


def test_spoken_numbers_and_runway_suffix():
    assert _readback_contains("five thousand feet", "5000")
    assert _readback_contains("squawk four one zero one", "4101")
    assert _readback_contains("runway two four left", "24L")
    assert _readback_contains("runway eighteen", "18")
    assert not _readback_contains("runway two four right", "24L")


def test_readback_speech_works_without_numeric_digits():
    pilot = PilotATC("AFL104", "UUEE", "ULLI", {
        "delivery": 121.6, "ground": 121.9, "tower": 118.7,
    }, "24L")
    pilot.update_cockpit(121.6)
    pilot.transmit("request clearance")
    assert pilot.transmit("climb five thousand squawk four one zero one").kind == "ack"
    pilot.update_cockpit(121.9)
    pilot.transmit("request pushback")
    pilot.transmit("pushback approved")
    pilot.transmit("request taxi")
    assert pilot.transmit("runway two four left hold short").kind == "ack"


def test_wrong_transponder_blocks_departure_clearance():
    pilot = PilotATC("AFL106", "UUEE", "ULLI", {"tower": 118.7}, "24L")
    pilot.stage = "taxi"
    pilot.update_cockpit(118.7, squawk="1200")
    assert pilot.transmit("ready departure").kind == "correction"
    assert pilot.runways.is_available("24L", "OTHER")
    pilot.update_cockpit(118.7, squawk="4101")
    assert pilot.transmit("ready departure").kind == "clearance"
    pilot.close()


def test_no_invented_traffic_controller_for_missing_frequency():
    tower_only = AIAirportController({"tower": 118.7}, RunwayController())
    plane = SimpleNamespace(id="TEST201", arrival=False)
    assert not tower_only.allow_edge(plane, Edge("A", "B", 5, "taxi"))
    assert tower_only.lines == []


def test_ai_arrival_receives_landing_not_takeoff_clearance():
    atc = AIAirportController({"tower": 118.7, "ground": 121.9}, RunwayController())
    plane = SimpleNamespace(id="TEST202", arrival=True)
    edge = Edge("RWY", "EXIT", 10, "runway", "24L")
    assert atc.allow_edge(plane, edge)
    assert any("cleared to land" in line.message for line in atc.lines)
    assert not any("cleared for takeoff" in line.message for line in atc.lines)
