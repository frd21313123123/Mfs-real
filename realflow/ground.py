"""Deterministic ground controller; one-aircraft-at-a-time edge locks."""
from dataclasses import dataclass
from enum import Enum
from .geo import Position, distance_m, bearing_deg, forward, KNOT_TO_MPS
from .airport import AirportGraph, Edge
from .runway import RunwayController

class Phase(str,Enum):
    PARKED='PARKED'
    PUSHBACK='PUSHBACK'
    TAXI_OUT='TAXI_OUT'
    HOLD_SHORT='HOLD_SHORT'
    LINE_UP='LINE_UP'
    TAKEOFF='TAKEOFF'
    AIRBORNE='AIRBORNE'
    LANDING='LANDING'
    ROLLOUT='ROLLOUT'
    TAXI_IN='TAXI_IN'
    COMPLETE='COMPLETE'

@dataclass
class GroundAircraft:
    id: str
    model_title: str
    route: list[str]
    position: Position
    arrival: bool = False
    cursor: int = 0
    speed_mps: float = 0.0
    phase: Phase = Phase.PARKED
    locked_edge: tuple[str,str] | None = None
    owns_runway: str = ''

class GroundEngine:
    def __init__(self, airport: AirportGraph, runways: RunwayController | None = None, max_ground:int=30):
        self.airport=airport
        self.runways=runways or RunwayController()
        self.max_ground=max_ground
        self.aircraft:dict[str,GroundAircraft]={}
        self.edge_owners:dict[frozenset[str],str]={}
        self.node_owners:dict[str,str]={}
        self.time=0.0

    def add(self, aircraft_id:str, model_title:str, start:str, end:str, arrival=False) -> GroundAircraft:
        if len(self.aircraft)>=self.max_ground: raise ValueError('ground capacity exhausted')
        if aircraft_id in self.aircraft: raise ValueError('aircraft already exists')
        route=self.airport.route(start,end)
        if len(route)<2: raise ValueError(f'no route {start} -> {end}')
        start_pos=self.airport.nodes[start].position
        plane=GroundAircraft(aircraft_id,model_title,route,start_pos,arrival=arrival,
                             phase=Phase.LANDING if arrival else Phase.PARKED)
        self.aircraft[aircraft_id]=plane
        return plane

    def remove(self, aircraft_id:str):
        plane=self.aircraft.pop(aircraft_id,None)
        if not plane: return
        self._free_edge(plane)
        self.runways.free_aircraft(aircraft_id)

    def _free_edge(self, plane:GroundAircraft):
        if plane.locked_edge:
            self.edge_owners.pop(frozenset(plane.locked_edge),None)
            dest=plane.locked_edge[1]
            if self.node_owners.get(dest)==plane.id:
                del self.node_owners[dest]
            plane.locked_edge=None

    def _destination_blocked(self, plane:GroundAircraft, dest:Position)->bool:
        for other in self.aircraft.values():
            if other.id==plane.id or other.phase in (Phase.AIRBORNE,Phase.COMPLETE): continue
            # If another plane is already at endpoint, do not enter a collision radius.
            if distance_m(other.position,dest)<18:
                return True
        return False

    def tick(self,dt:float)->dict[str,Phase]:
        if dt<=0 or dt>10: raise ValueError('tick dt 0..10 seconds')
        self.time+=dt
        transitions={}
        for plane in list(self.aircraft.values()):
            if plane.phase in (Phase.COMPLETE,Phase.AIRBORNE): continue
            self._advance(plane,dt)
            transitions[plane.id]=plane.phase
        return transitions

    def _advance(self, plane:GroundAircraft,dt:float):
        if plane.cursor>=len(plane.route)-1:
            plane.phase=Phase.COMPLETE if plane.arrival else Phase.AIRBORNE
            return
        src,dst=plane.route[plane.cursor:plane.cursor+2]
        edge=self.airport.edges[(src,dst)]
        dest=self.airport.nodes[dst].position
        owner=self.edge_owners.get(frozenset((src,dst)))
        # At most one plane per bidirectional edge. Unlike optimistic reservations,
        # a lock is held for the entire physical traversal.
        node_owner=self.node_owners.get(dst)
        if (owner and owner!=plane.id or
                node_owner and node_owner!=plane.id or
                (plane.locked_edge is None and self._destination_blocked(plane,dest))):
            plane.speed_mps=max(0,plane.speed_mps-2*dt)
            if self.airport.nodes[src].kind=='hold': plane.phase=Phase.HOLD_SHORT
            return
        if edge.runway and not self.runways.reserve(edge.runway,plane.id,
                                                     'arrival' if plane.arrival else 'departure',self.time):
            plane.speed_mps=max(0,plane.speed_mps-2*dt)
            plane.phase=Phase.HOLD_SHORT
            return
        if edge.runway: plane.owns_runway=edge.runway
        if plane.locked_edge is None:
            plane.locked_edge=(src,dst)
            self.edge_owners[frozenset((src,dst))]=plane.id
            self.node_owners[dst]=plane.id
        if edge.kind=='pushback': plane.phase=Phase.PUSHBACK
        elif edge.runway: plane.phase=Phase.ROLLOUT if plane.arrival else Phase.TAKEOFF
        else: plane.phase=Phase.TAXI_IN if plane.arrival else Phase.TAXI_OUT
        dist=distance_m(plane.position,dest)
        max_speed=edge.speed_kt*KNOT_TO_MPS
        # 1.0 m/s² acceleration, 1.5 m/s² braking; speed approaches zero at node.
        safe_speed=max(0.8,min(max_speed,(2*1.5*dist)**0.5))
        plane.speed_mps=min(safe_speed,plane.speed_mps+dt*1.0)
        step=min(dist,plane.speed_mps*dt)
        heading=bearing_deg(plane.position,dest)
        # Prevent conflicts at *converging* edges too, not only on shared edges.
        # Limit movement against all nearby traffic, even if the graph differs.
        for other in self.aircraft.values():
            if other.id==plane.id or other.phase in (Phase.AIRBORNE,Phase.COMPLETE):
                continue
            current=distance_m(plane.position,other.position)
            if current>=20 and distance_m(forward(plane.position,heading,step),other.position)<20:
                low,high=0.0,step
                for _ in range(16):
                    mid=(low+high)/2
                    if distance_m(forward(plane.position,heading,mid),other.position)>=20:
                        low=mid
                    else:
                        high=mid
                step=low
            elif current<20:
                # Already close: permit motion only if it increases separation.
                if distance_m(forward(plane.position,heading,step),other.position)<=current:
                    step=0.0
        if step<0.02:
            plane.speed_mps=0.0
            return
        if step>=dist-0.15:
            plane.position=Position(dest.lat,dest.lon,dest.alt_ft,
                   (heading+180)%360 if edge.kind=='pushback' else heading,
                   plane.speed_mps/KNOT_TO_MPS,True)
            plane.cursor+=1
            plane.speed_mps=0
            self._free_edge(plane)
            if plane.owns_runway and (plane.cursor >= len(plane.route)-1 or
                    not self.airport.edges[(plane.route[plane.cursor],plane.route[plane.cursor+1])].runway):
                self.runways.release(plane.owns_runway,plane.id)
                plane.owns_runway=''
            if plane.cursor>=len(plane.route)-1:
                plane.phase=Phase.COMPLETE if plane.arrival else Phase.AIRBORNE
        else:
            moved=forward(plane.position,heading,step)
            plane.position=Position(moved.lat,moved.lon,
                    plane.position.alt_ft+(dest.alt_ft-plane.position.alt_ft)*step/max(dist,0.1),
                    (heading+180)%360 if edge.kind=='pushback' else heading,
                    plane.speed_mps/KNOT_TO_MPS,True)
