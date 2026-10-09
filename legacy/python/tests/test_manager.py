from realflow.bridge import MockBridge
from realflow.config import Settings
from realflow.geo import Position
from realflow.manager import TrafficManager
from realflow.live import Observation
from realflow.ground import GroundEngine

def test_caps_and_shutdown(matcher,graph):
    s=Settings(airborne_limit=6,ground_limit=2)
    b=MockBridge()
    ground=GroundEngine(graph,max_ground=2)
    ground.add('A','FSLTL Airbus A320 AFL','GATE_A','RUNWAY_EXIT')
    m=TrafficManager(s,matcher,b,ground)
    stats=m.tick(1,1010,Position(57.91,56),synthetic_target=6)
    assert stats['synthetic']==6 and stats['ground']==1 and stats['objects']==7
    assert len(b.objects)==7
    m.close()
    assert not b.objects
    assert not b.connected

def test_live_trims_synthetic_to_capacity(matcher):
    s=Settings(airborne_limit=2)
    b=MockBridge();m=TrafficManager(s,matcher,b)
    origin=Position(57.91,56)
    m.feed([Observation('a','AFL123',Position(57.9,56,5000),0,1000,'A320')],origin,1010)
    m.tick(1,1010,origin,synthetic_target=10)
    assert len(m.fleet.aircraft)==1
    assert len(m.synthetic.aircraft)==1
    m.feed([Observation('b','DLH123',Position(57.92,56,5000),0,1012,'A320')],origin,1013)
    m.tick(1,1013,origin,synthetic_target=10)
    # Requirement: all spawned objects stay within hard cap even after live uptake.
    assert len(m.fleet.aircraft)+len(m.synthetic.aircraft)<=2
