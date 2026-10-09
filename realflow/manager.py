"""Single manager coordinating LIVE, synthetic and ground-owned traffic."""
from .config import Settings
from .geo import Position
from .fleet import AirFleet
from .ground import GroundEngine
from .fsltl import ModelMatcher
from .synthetic import SyntheticTraffic

class TrafficManager:
    def __init__(self,settings:Settings,matcher:ModelMatcher,bridge,ground:GroundEngine|None=None,ai_atc=None):
        self.settings=settings.validate()
        self.bridge=bridge
        self.fleet=AirFleet(matcher,limit=settings.airborne_limit)
        self.synthetic=SyntheticTraffic(matcher)
        self.ground=ground
        self.ai_atc=ai_atc
        if self.ground is not None and self.ai_atc is not None:
            self.ground.clearance_provider=self.ai_atc.allow_edge
        self._spawned=set()
        self._clock=0.0

    def feed(self,observations:list,origin:Position,now:float):
        if self.settings.mode!='simulation' and self.settings.sync_real_flights:
            self.fleet.ingest(observations,origin,now,self.settings.max_radius_km)

    def tick(self,dt:float,now:float,origin:Position,synthetic_target:int=8):
        if dt<=0: raise ValueError('dt must be >0')
        self._clock+=dt
        if self.settings.mode!='simulation':
            self.fleet.tick(dt,now)
        if self.settings.mode!='live':
            available=max(0,self.settings.airborne_limit-len(self.fleet.aircraft))
            # LIVE always outranks synthetic. Discard surplus before new generation.
            while len(self.synthetic.aircraft)>available:
                victim=next(reversed(self.synthetic.aircraft))
                del self.synthetic.aircraft[victim]
            self.synthetic.generate(origin,available,synthetic_target)
            self.synthetic.tick(dt)
        else:
            self.synthetic.aircraft.clear()
        if self.ground:
            self.ground.tick(min(10,dt))
        desired={}
        if self.settings.mode!='simulation':
            for ac in self.fleet.aircraft.values():
                desired['LIVE-'+ac.id]=(ac.model_title,ac.position)
        if self.settings.mode!='live':
            for ac in self.synthetic.aircraft.values():
                desired[ac.id]=(ac.model_title,ac.position)
        if self.ground:
            for ac in self.ground.aircraft.values():
                if ac.phase.value not in ('AIRBORNE','COMPLETE'):
                    desired['GROUND-'+ac.id]=(ac.model_title,ac.position)
        for key in list(self._spawned):
            if key not in desired:
                self.bridge.remove(key)
                self._spawned.remove(key)
        for key,(title,pos) in desired.items():
            if key not in self._spawned:
                self.bridge.create(key,title,pos)
                self._spawned.add(key)
            else:
                self.bridge.update(key,pos)
        # Finished ground records must not leak and block future arrivals/departures.
        if self.ground:
            for key,ac in list(self.ground.aircraft.items()):
                if ac.phase.value in ('AIRBORNE','COMPLETE'):
                    self.ground.remove(key)
                    if self.ai_atc is not None: self.ai_atc.forget(key)
        return {'live':len(self.fleet.aircraft),'synthetic':len(self.synthetic.aircraft),
                'ground':sum(1 for a in self.ground.aircraft.values()
                             if a.phase.value not in ('AIRBORNE','COMPLETE')) if self.ground else 0,
                'objects':len(self._spawned)}

    def close(self):
        for key in list(self._spawned): self.bridge.remove(key)
        self._spawned.clear()
        self.bridge.close()
