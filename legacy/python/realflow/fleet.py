"""Owned traffic with tracked/predicted motion and hard airborne capacity."""
from __future__ import annotations
from dataclasses import dataclass
from .geo import Position, forward, blend, distance_m, KNOT_TO_MPS, FT_TO_M
from .live import Observation
from .fsltl import ModelMatcher

@dataclass
class LiveAircraft:
    id: str
    callsign: str
    model_title: str
    position: Position
    target: Position
    vertical_mps: float
    seen_at: float
    source: str = 'LIVE'

class AirFleet:
    def __init__(self,matcher:ModelMatcher,limit:int=35,expiry_seconds:float=600):
        if limit<0 or limit>35: raise ValueError('air limit must be 0..35')
        self.matcher=matcher
        self.limit=limit
        self.expiry_seconds=expiry_seconds
        self.aircraft:dict[str,LiveAircraft]={}

    def ingest(self,observations:list[Observation],origin:Position,now:float,radius_km:float=120):
        # Avoid the 0/garbage timestamps and faraway airplanes.
        candidates=[o for o in observations if not o.position.on_ground and
                    distance_m(origin,o.position) <= radius_km*1000 and
                    o.observed_at>0 and now-o.observed_at < 300 and now-o.observed_at>=-30]
        candidates.sort(key=lambda o:distance_m(origin,o.position))
        for o in candidates:
            record=self.aircraft.get(o.icao24)
            if record:
                if o.observed_at<=record.seen_at: continue
                if distance_m(record.position,o.position)>80_000:
                    # An unexpected jump is not turned into a visual teleport.
                    continue
                record.target=o.position
                record.seen_at=o.observed_at
                record.vertical_mps=o.vertical_mps
                record.callsign=o.callsign or record.callsign
                record.source='LIVE'
                continue
            if len(self.aircraft)>=self.limit: break
            airline=''.join(c for c in o.callsign[:3] if c.isalpha()).upper()
            model=self.matcher.choose(o.icao_type,airline)
            if not model: continue
            self.aircraft[o.icao24]=LiveAircraft(o.icao24,o.callsign or o.icao24,
                model.title,o.position,o.position,o.vertical_mps,o.observed_at)

    def tick(self,dt:float,now:float)->list[str]:
        expired=[]
        for key,plane in list(self.aircraft.items()):
            age=now-plane.seen_at
            if age>self.expiry_seconds:
                expired.append(key);del self.aircraft[key];continue
            plane.source='PREDICTED' if age>150 else 'LIVE'
            p=plane.position
            # Dead-reckon along ground track; limit extrapolation to stale TTL.
            moved=forward(p,p.heading,max(0,p.speed_kt)*KNOT_TO_MPS*dt)
            drift_alt=p.alt_ft+plane.vertical_mps*dt/FT_TO_M
            moved=Position(moved.lat,moved.lon,drift_alt,p.heading,p.speed_kt,False)
            # Converge toward the last observation, but avoid snapping back to old locations.
            if age<30 and distance_m(moved,plane.target)<4000:
                moved=blend(moved,plane.target,min(0.35,dt*0.2))
            plane.position=moved
        return expired
