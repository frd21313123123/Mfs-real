"""Geodetic utilities in WGS84-like spherical approximation for short steps."""
import math
from dataclasses import dataclass, replace

EARTH_RADIUS_M = 6_371_000.0
KNOT_TO_MPS = 0.514444
FT_TO_M = 0.3048

@dataclass(frozen=True)
class Position:
    lat: float
    lon: float
    alt_ft: float = 0.0
    heading: float = 0.0
    speed_kt: float = 0.0
    on_ground: bool = False
    pitch: float = 0.0
    bank: float = 0.0

    def __post_init__(self):
        if not (-90 <= self.lat <= 90 and -180 <= self.lon <= 180):
            raise ValueError("Invalid latitude or longitude")


def distance_m(a: Position, b: Position) -> float:
    lat1, lat2 = math.radians(a.lat), math.radians(b.lat)
    dlat, dlon = lat2-lat1, math.radians(b.lon-a.lon)
    x = math.sin(dlat / 2)**2 + math.cos(lat1)*math.cos(lat2)*math.sin(dlon/2)**2
    return EARTH_RADIUS_M*2*math.asin(min(1.0, math.sqrt(x)))


def bearing_deg(a: Position, b: Position) -> float:
    lat1, lat2 = math.radians(a.lat), math.radians(b.lat)
    dl = math.radians(b.lon-a.lon)
    y = math.sin(dl)*math.cos(lat2)
    x = math.cos(lat1)*math.sin(lat2)-math.sin(lat1)*math.cos(lat2)*math.cos(dl)
    return (math.degrees(math.atan2(y, x)) + 360) % 360


def forward(p: Position, bearing: float, metres: float) -> Position:
    a = metres / EARTH_RADIUS_M
    lat, lon, brg = math.radians(p.lat), math.radians(p.lon), math.radians(bearing)
    dest_lat = math.asin(max(-1,min(1,math.sin(lat)*math.cos(a)+math.cos(lat)*math.sin(a)*math.cos(brg))))
    dest_lon = lon + math.atan2(math.sin(brg)*math.sin(a)*math.cos(lat), math.cos(a)-math.sin(lat)*math.sin(dest_lat))
    return replace(p, lat=math.degrees(dest_lat), lon=((math.degrees(dest_lon)+180)%360)-180, heading=bearing%360)


def blend(a: Position, b: Position, alpha: float) -> Position:
    """Short-distance interpolation with longitude anti-meridian handling."""
    t = max(0.0, min(1.0, alpha))
    delta = ((b.lon-a.lon+180)%360)-180
    dh = ((b.heading-a.heading+180)%360)-180
    return replace(a, lat=a.lat+(b.lat-a.lat)*t, lon=((a.lon+delta*t+180)%360)-180,
                   alt_ft=a.alt_ft+(b.alt_ft-a.alt_ft)*t, heading=(a.heading+dh*t)%360,
                   speed_kt=a.speed_kt+(b.speed_kt-a.speed_kt)*t,
                   on_ground=b.on_ground if t>=1 else a.on_ground)
