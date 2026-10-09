"""Exclusive runway reservation to avoid conflicting *owned* traffic operations."""
from dataclasses import dataclass

@dataclass(frozen=True)
class Reservation:
    aircraft_id: str
    operation: str
    since: float

class RunwayController:
    def __init__(self):
        self.reservations:dict[str, Reservation]={}

    def reserve(self, runway_id: str, aircraft_id: str, operation: str, now: float=0) -> bool:
        if operation not in ('departure','arrival','crossing'):
            raise ValueError('unknown runway operation')
        if not runway_id: raise ValueError('empty runway id')
        owner=self.reservations.get(runway_id)
        if owner and owner.aircraft_id != aircraft_id:
            return False
        if not owner:
            self.reservations[runway_id]=Reservation(aircraft_id, operation, now)
        return True

    def release(self, runway_id: str, aircraft_id: str) -> bool:
        r=self.reservations.get(runway_id)
        if r is None or r.aircraft_id != aircraft_id: return False
        del self.reservations[runway_id]
        return True

    def free_aircraft(self, aircraft_id: str):
        for runway_id, r in list(self.reservations.items()):
            if r.aircraft_id == aircraft_id: del self.reservations[runway_id]

    def is_available(self, runway_id: str, aircraft_id: str='') -> bool:
        holder=self.reservations.get(runway_id)
        return holder is None or holder.aircraft_id==aircraft_id
