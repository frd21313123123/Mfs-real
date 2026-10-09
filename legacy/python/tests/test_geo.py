from realflow.geo import Position,forward,distance_m,bearing_deg,blend

def test_forward_is_invertible_short_legs():
    a=Position(57.91,56.0)
    b=forward(a,270,2500)
    assert abs(distance_m(a,b)-2500)<1
    assert 260 < bearing_deg(a,b) < 280

def test_antimeridian_blend():
    a=Position(0,179.9,1000,heading=350)
    b=Position(0,-179.9,2000,heading=10)
    m=blend(a,b,0.5)
    assert abs(abs(m.lon)-180)<0.01
    assert abs(m.heading)<0.001
    assert m.alt_ft==1500

def test_invalid_latitude():
    import pytest
    with pytest.raises(ValueError): Position(100,0)
