from realflow.fsltl import scan_models,ModelMatcher,AircraftModel,discover_root

def test_fltsim_parsing(tmp_path):
    root=tmp_path/'fsltl-traffic-base'/'SimObjects'/'Airplanes'/'A320'
    root.mkdir(parents=True)
    (root/'aircraft.cfg').write_text('''[VERSION]
major=1
[FLTSIM.0]
title = "FSLTL_A320_AFL" ; note
icao_type_designator=A320
icao_airline=AFL
ui_variation = Aeroflot
[FLTSIM.1]
title=FSLTL_A320_DLH
icao_type_designator = A320
icao_airline = DLH
[GENERAL]
icao_airline=XXX
''')
    models=scan_models(tmp_path/'fsltl-traffic-base')
    assert len(models)==2
    assert models[0].title=='FSLTL_A320_AFL'
    assert models[0].airline=='AFL'
    assert models[1].airline=='DLH'
    assert discover_root(str(tmp_path))==tmp_path/'fsltl-traffic-base'

def test_matcher_type_and_airline(matcher):
    assert 'DLH' in matcher.choose('A320','DLH').title
    assert matcher.choose('B738','AFL').aircraft_type=='B738'
    assert matcher.choose('C172','UAL') is None
    assert matcher.choose('','UAL').airline=='UAL'
