from pathlib import Path
from realflow.airport_xml import convert,airport_xml_to_graph
from realflow.airport import AirportGraph

def test_convert_source_xml(tmp_path):
    source=Path(__file__).parent.parent/'examples'/'sample_airport_source.xml'
    result=convert(source,tmp_path/'airfield.json')
    assert result['icao']=='TSTX'
    graph=AirportGraph.from_json(tmp_path/'airfield.json')
    assert len(graph.nodes)==5
    assert len(graph.route('N4','N3'))==5
    assert graph.nodes['N4'].kind=='gate'
    assert graph.nodes['N1'].kind=='hold'
    assert any(edge.runway=='TSTX/18' for edge in graph.edges.values())
