"""Single manager coordinating LIVE, synthetic and ground-owned traffic."""
from .config import Settings
from .geo import Position, forward, distance_m
from .fleet import AirFleet
from .ground import GroundEngine
from .fsltl import ModelMatcher
from .synthetic import SyntheticTraffic

class TrafficManager:
    def __init__(self,settings:Settings,matcher:ModelMatcher,bridge,ground:GroundEngine|None=None,ai_atc=None,flight_director=None):
        self.settings=settings.validate()
        self.bridge=bridge
        self.fleet=AirFleet(matcher,limit=settings.airborne_limit)
        self.synthetic=SyntheticTraffic(matcher)
        self.ground=ground
        self.ai_atc=ai_atc
        self.flight_director=flight_director
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
            approach=self.flight_director.airport_position if self.flight_director else None
            runway_heading=getattr(self.flight_director,'approach_heading',None)
            self.synthetic.generate(origin,available,synthetic_target,
                                    arrival_target=approach,approach_heading=runway_heading)
            self.synthetic.tick(dt,self.flight_director)
        else:
            self.synthetic.aircraft.clear()
        if self.ground:
            self.ground.tick(min(10,dt))
            # Arrival: transfer near validated runway node, reserving ownership.
            if self.flight_director is not None:
                for arrived in self.flight_director.take_arrivals():
                    transferred=False
                    if len(self.ground.aircraft)<self.settings.ground_limit:
                        runway_nodes=[
                            node.id for node in self.ground.airport.nodes.values()
                            if node.kind=='runway' and
                               distance_m(node.position,arrived.position)<110
                        ]
                        occupied={p.route[-1] for p in self.ground.aircraft.values() if p.arrival}
                        gates=[n.id for n in self.ground.airport.nodes.values()
                               if n.kind=='gate' and n.id not in occupied]
                        for runway_node in runway_nodes:
                            for gate in gates:
                                try:
                                    ground_plane=self.ground.add(arrived.id,arrived.model_title,
                                                                  runway_node,gate,arrival=True)
                                    # Same ID in both engines: no simultaneous runway grants.
                                    if self.flight_director.runway_id:
                                        ground_plane.owns_runway=self.flight_director.runway_id
                                    transferred=True
                                    break
                                except ValueError:
                                    continue
                            if transferred: break
                    self.flight_director.forget(arrived.id,keep_runway=transferred)
            for key,ac in list(self.ground.aircraft.items()):
                if ac.phase.value=='AIRBORNE':
                    # Preserve the same SimConnect object key on ground -> air.
                    if self.settings.mode!='live':
                        available=max(0,self.settings.airborne_limit-len(self.fleet.aircraft))
                        destination=forward(ac.position,ac.position.heading,85_000)
                        self.synthetic.accept_departure('GROUND-'+key,ac.model_title,
                                                         ac.position,destination,available)
                    self.ground.remove(key)
                    if self.ai_atc is not None: self.ai_atc.forget(key)
                elif ac.phase.value=='COMPLETE':
                    self.ground.remove(key)
                    if self.ai_atc is not None: self.ai_atc.forget(key)
        elif self.flight_director is not None:
            for arrived in self.flight_director.take_arrivals():
                self.flight_director.forget(arrived.id)
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
                    key=ac.id if ac.id.startswith('SYN-') else 'GROUND-'+ac.id
                    desired[key]=(ac.model_title,ac.position)
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
        return {'live':len(self.fleet.aircraft),'synthetic':len(self.synthetic.aircraft),
                'ground':sum(1 for a in self.ground.aircraft.values()
                             if a.phase.value not in ('AIRBORNE','COMPLETE')) if self.ground else 0,
                'objects':len(self._spawned)}

    def close(self):
        if self.flight_director is not None: self.flight_director.close()
        for key in list(self._spawned): self.bridge.remove(key)
        self._spawned.clear()
        self.bridge.close()
