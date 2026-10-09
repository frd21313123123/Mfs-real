import pytest
from realflow.airport import AirportGraph,Node,Edge
from realflow.geo import Position

def test_route_is_connected(graph):
    route=graph.route('GATE_A','RUNWAY_EXIT')
    assert route[0]=='GATE_A' and route[-1]=='RUNWAY_EXIT'
    assert 'HOLD_R' in route
    assert all((a,b) in graph.edges for a,b in zip(route,route[1:]))

def test_forbidden_edge(graph):
    assert graph.route('GATE_A','RUNWAY_EXIT',{('HOLD_R','RUNWAY_ENTRY')})==[]

def test_blocked_edge(graph):
    assert graph.route('GATE_A','RUNWAY_EXIT',{('PUSH_A','JUNCTION_A')})==[]

def test_duplicate_nodes_rejected():
    a=Node('a',Position(1,1))
    with pytest.raises(ValueError):AirportGraph([a,a],[])

def test_closed_edges_ignored():
    nodes=[Node('a',Position(0,0)),Node('b',Position(0,.01))]
    g=AirportGraph(nodes,[Edge('a','b',kind='closed')])
    assert g.route('a','b')==[]
