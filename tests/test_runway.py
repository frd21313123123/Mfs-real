from realflow.runway import RunwayController

def test_runway_exclusive():
    r=RunwayController()
    assert r.reserve('18','A','arrival',10)
    assert r.reserve('18','A','arrival',11)
    assert not r.reserve('18','B','departure',12)
    assert not r.release('18','B')
    assert r.release('18','A')
    assert r.reserve('18','B','departure',12)

def test_runway_independent():
    r=RunwayController()
    assert r.reserve('18','A','departure',1)
    assert r.reserve('27','B','arrival',1)
    r.free_aircraft('A')
    assert r.is_available('18') and not r.is_available('27')
