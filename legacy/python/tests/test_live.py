import pytest
import urllib.error
from realflow.live import parse_states,OpenSkyClient,Observation
from realflow.geo import Position
from realflow.fleet import AirFleet

def sample_row(icao='abc123',on_ground=False):
    return [icao,'AFL123','RU',990,995,56.02,57.91,2000,on_ground,120,90,0.5,None,2100,'7000',False,0]

def test_parse_states_correct_columns():
    parsed=parse_states({'time':1000,'states':[sample_row(),sample_row('invalid',True),[None]]})
    assert len(parsed)==2
    assert parsed[0].callsign=='AFL123'
    assert abs(parsed[0].position.alt_ft-6889.76)<1
    assert parsed[0].position.speed_kt>200
    assert parsed[1].position.on_ground

def test_feed_distance_and_expiry(matcher):
    fleet=AirFleet(matcher,limit=1,expiry_seconds=600)
    obs=Observation('aabbcc','AFL123',Position(57.91,56.02,5000,90,240,False),0.0,1000,'A320')
    fleet.ingest([obs],Position(57.9,56),1010)
    assert len(fleet.aircraft)==1
    assert fleet.aircraft['aabbcc'].model_title.endswith('AFL')
    fleet.tick(1,1020)
    assert len(fleet.aircraft)==1
    fleet.tick(1,1700)
    assert not fleet.aircraft

def test_no_ground_or_stale_aircraft(matcher):
    fleet=AirFleet(matcher)
    a=Observation('a','AFL123',Position(57.91,56,1000,0,0,True),0,1000,'A320')
    b=Observation('b','AFL123',Position(57.91,56,1000,0,0,False),0,100,'A320')
    fleet.ingest([a,b],Position(57.91,56),1010)
    assert not fleet.aircraft

def test_rate_limit_before_network(monkeypatch):
    c=OpenSkyClient(minimum_interval=120)
    c.next_allowed=float('inf')
    assert c.poll(57.91,56.02)==[]
    with pytest.raises(ValueError):OpenSkyClient(minimum_interval=1)
