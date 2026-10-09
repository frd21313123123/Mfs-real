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
    arrival:bool=False

class SyntheticTraffic:
    def __init__(self,matcher:ModelMatcher, seed:int=2026):
        self.matcher=matcher
        self.random=random.Random(seed)
        self.flight_index=0
        self.aircraft:dict[str,SimulatedFlight]={}

    def generate(self,origin:Position,limit:int,target:int=8,arrival_target:Position|None=None,approach_heading:float|None=None):
        if limit<0: raise ValueError('limit must be nonnegative')
        to_create=max(0,min(limit,target)-len(self.aircraft))
        if not self.matcher.models: return
        for _ in range(to_create):
            heading=self.random.uniform(0,360)
            if arrival_target is not None:
                # Validated airport node, approximate extended approach corridor.
                radial=(approach_heading+180)%360 if approach_heading is not None else heading
                start=forward(arrival_target,radial,self.random.uniform(38_000,65_000))
                start=forward(start,(radial+90)%360,self.random.uniform(-2500,2500))
                dest=arrival_target
                speed=self.random.uniform(190,245)
                altitude=arrival_target.alt_ft+self.random.uniform(6500,9500)
            else:
                start=forward(origin,heading,self.random.uniform(30_000,75_000))
                dest=forward(origin,(heading+180)%360,self.random.uniform(25_000,75_000))
                speed=self.random.uniform(230,440)
                altitude=self.random.uniform(10_000,30_000)
            model=self.random.choice(self.matcher.models)
            start=Position(start.lat,start.lon,altitude,bearing_deg(start,dest),speed,False)
            dest=Position(dest.lat,dest.lon,arrival_target.alt_ft if arrival_target is not None else altitude)
            self.flight_index+=1
            key=f'SYN-{self.flight_index:05d}'
            self.aircraft[key]=SimulatedFlight(key,model.title,start,dest,2700,arrival_target is not None)

    def accept_departure(self, aircraft_id:str, model_title:str, position:Position,
                         destination:Position, limit:int) -> bool:
        """Transfer runway departures without changing SimConnect object IDs."""
        if len(self.aircraft)>=limit or aircraft_id in self.aircraft:
            return False
        self.aircraft[aircraft_id]=SimulatedFlight(aircraft_id,model_title,
            Position(position.lat,position.lon,position.alt_ft,position.heading,
                     max(125,position.speed_kt),False),
            destination,2700,False)
        return True

    def tick(self,dt:float,director=None):
        expired=[]
        if director is not None:
            before=set(self.aircraft)
            director.tick(self.aircraft,dt)
            expired.extend(sorted(before-set(self.aircraft)))
            for key,flight in list(self.aircraft.items()):
                flight.remaining_s-=dt
                if flight.remaining_s<=0:
                    del self.aircraft[key]
                    director.forget(key)
                    expired.append(key)
            return expired
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
