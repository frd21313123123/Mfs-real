from realflow.ground import GroundEngine,Phase
from realflow.geo import distance_m

def test_ground_pushback_taxi_and_departure(graph):
    g=GroundEngine(graph)
    ac=g.add('A','FSLTL_A320','GATE_A','RUNWAY_EXIT')
    observed={ac.phase}
    for i in range(1000):
        g.tick(1.0)
        observed.add(ac.phase)
        if ac.phase==Phase.AIRBORNE: break
    assert ac.phase==Phase.AIRBORNE
    assert Phase.PUSHBACK in observed
    assert Phase.TAXI_OUT in observed
    assert Phase.TAKEOFF in observed
    assert not g.runways.reservations
    assert not g.edge_owners

def test_two_planes_no_shared_edge(graph):
    g=GroundEngine(graph)
    a=g.add('A','x','GATE_A','RUNWAY_EXIT')
    b=g.add('B','x','GATE_B','RUNWAY_EXIT')
    for i in range(1300):
        g.tick(1.0)
        if a.locked_edge and b.locked_edge:
            assert frozenset(a.locked_edge)!=frozenset(b.locked_edge)
        # Once a flight is airborne the ground-engine object's final coordinate
        # remains at the runway end; it is no longer a physical surface object.
        if a.phase!=Phase.AIRBORNE and b.phase!=Phase.AIRBORNE:
            assert distance_m(a.position,b.position)>14
    assert a.phase==Phase.AIRBORNE and b.phase==Phase.AIRBORNE

def test_capacity_and_removal(graph):
    g=GroundEngine(graph,max_ground=1)
    g.add('A','x','GATE_A','RUNWAY_EXIT')
    import pytest
    with pytest.raises(ValueError):g.add('B','x','GATE_B','RUNWAY_EXIT')
    g.remove('A')
    assert not g.aircraft and not g.edge_owners

def test_arrival_ground_to_gate(graph):
    g=GroundEngine(graph)
    ac=g.add('ARR1','x','RUNWAY_EXIT','GATE_B',arrival=True)
    for i in range(1800):
        g.tick(1)
        if ac.phase==Phase.COMPLETE: break
    assert ac.phase==Phase.COMPLETE
