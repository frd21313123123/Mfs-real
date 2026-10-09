"""Human ATC console is an event pump; it never touches simulator controls."""
from realflow.atc import PilotATC
from realflow.pilot_console import PilotRadioConsole
from realflow.geo import Position
from realflow.runway import RunwayController
from realflow.cockpit_radio import CockpitRadio

FREQ = {"delivery":121.6,"ground":121.9,"tower":118.7,"departure":124.2,
        "approach":120.7,"center":125.4}


def test_pilot_console_nonblocking_clearance_readback():
    pilot=PilotATC("AFL101","UUEE","ULLI",FREQ,"24L")
    console=PilotRadioConsole(pilot,650,offline_com1=121.6)
    assert console.submit("request IFR clearance")
    out=console.poll(now=100)
    assert len(out)==1 and out[0].kind=="clearance"
    assert pilot.stage=="filed"
    assert console.submit("climb five thousand squawk four one zero one")
    assert console.poll(now=101)[0].kind=="ack"
    assert pilot.stage=="cleared"
    assert console.poll(now=102)==[]
    console.close()


def test_pilot_real_com_selection_without_mutation():
    pilot=PilotATC("AFL101","UUEE","ULLI",FREQ,"24L")
    console=PilotRadioConsole(pilot,650)
    console.poll(CockpitRadio(121.6,121.9,2,"4101"),now=100)
    assert pilot.station=="ground"
    assert pilot.actual_squawk=="4101"
    console.poll(CockpitRadio(121.6,121.9,1,"4101"),now=101)
    assert pilot.station=="delivery"
    console.close()


def test_observed_player_takeoff_releases_shared_runway():
    shared=RunwayController()
    pilot=PilotATC("AFL101","UUEE","ULLI",FREQ,"24L",runways=shared)
    assert shared.reserve("24L","AFL101","departure")
    pilot._runway_owned=True
    pilot.stage="departure"
    console=PilotRadioConsole(pilot,1000,offline_com1=118.7)
    event=console.poll(player_position=Position(47,-122,1650,90,165,False),now=10)
    assert len(event)==1 and event[0].kind=="handoff"
    assert pilot.stage=="airborne"
    assert shared.is_available("24L","OTHER")
    console.close()


def test_no_false_altitude_warning_while_climbing():
    pilot=PilotATC("AFL101","UUEE","ULLI",FREQ,"24L")
    pilot.stage="cruise"
    console=PilotRadioConsole(pilot,1000,offline_com1=125.4)
    assert not console.poll(player_position=Position(47,-122,15000),now=0)
    assert not console.poll(player_position=Position(47,-122,31000),now=5)
    assert not console.poll(player_position=Position(47,-122,30050),now=10)
    assert not console.poll(player_position=Position(47,-122,29300),now=20)
    assert not console.poll(player_position=Position(47,-122,29200),now=32)
    result=console.poll(player_position=Position(47,-122,29100),now=43)
    assert len(result)==1 and result[0].kind=="warning"
    assert console.poll(player_position=Position(47,-122,29100),now=44)==[]
    console.close()
