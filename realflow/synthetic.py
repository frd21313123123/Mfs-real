"""Offline simulated traffic, with bounded lifetime and deterministic seed."""
import random
from dataclasses import dataclass
from .geo import Position, forward, distance_m, bearing_deg, KNOT_TO_MPS
from .fsltl import ModelMatcher

@dataclass
class SimulatedFlight:
    id:str
    model_title:str
    position:Position
    destination:Position
    remaining_s:float

class SyntheticTraffic:
    def __init__(self,matcher:ModelMatcher, seed:int=2026):
        self.matcher=matcher
        self.random=random.Random(seed)
        self.flight_index=0
        self.aircraft:dict[str,SimulatedFlight]={}

    def generate(self,origin:Position,limit:int,target:int=8):
        if limit<0: raise ValueError('limit must be nonnegative')
        to_create=max(0,min(limit,target)-len(self.aircraft))
        if not self.matcher.models: return
        for _ in range(to_create):
            heading=self.random.uniform(0,360)
            start=forward(origin,heading, self.random.uniform(30_000,75_000))
            dest=forward(origin,(heading+180)%360,self.random.uniform(25_000,75_000))
            speed=self.random.uniform(230,440)
            altitude=self.random.uniform(10_000,30_000)
            model=self.random.choice(self.matcher.models)
            start=Position(start.lat,start.lon,altitude,bearing_deg(start,dest),speed,False)
            dest=Position(dest.lat,dest.lon,altitude)
            self.flight_index+=1
            key=f'SYN-{self.flight_index:05d}'
            self.aircraft[key]=SimulatedFlight(key,model.title,start,dest,1800)

    def tick(self,dt:float):
        expired=[]
        for key,f in list(self.aircraft.items()):
            metres=dt*f.position.speed_kt*KNOT_TO_MPS
            distance=distance_m(f.position,f.destination)
            f.remaining_s-=dt
            if distance<=metres+500 or f.remaining_s<=0:
                expired.append(key)
                del self.aircraft[key]
                continue
            heading=bearing_deg(f.position,f.destination)
            move=forward(f.position,heading,metres)
            f.position=Position(move.lat,move.lon,f.position.alt_ft,heading,f.position.speed_kt,False)
        return expired
