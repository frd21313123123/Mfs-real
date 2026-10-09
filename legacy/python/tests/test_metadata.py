from realflow.metadata import AircraftMetadata
from realflow.live import Observation
from realflow.geo import Position


def test_csv_enrich(tmp_path):
    path=tmp_path/'types.csv'
    path.write_text('icao24,icao_type\nABCDEF,B738\n')
    db=AircraftMetadata.from_csv(path)
    record=Observation('abcdef','UAL123',Position(0,0,1000),0,123)
    enriched=db.enrich([record])
    assert enriched[0].icao_type=='B738'
    assert record.icao_type==''
