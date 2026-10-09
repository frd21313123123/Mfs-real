"""Deterministic ATC flight guidance for RealFlow-owned SYNTHETIC flights.

Not used for real ADS-B tracks or third-party AI. All motion remains an
experimental SimConnect position-write path until tested in MSFS 2020.
"""
from __future__ import annotations

import math
from dataclasses import dataclass
from .geo import Position, bearing_deg, distance_m, forward, KNOT_TO_MPS
from .atc import RadioLine
from .runway import RunwayController


def heading_error(target: float, actual: float) -> float:
    return (target - actual + 180) % 360 - 180


def clamp(value: float, low: float, high: float) -> float:
    return max(low, min(high, value))


@dataclass(frozen=True)
class FlightClearance:
    heading_deg: float
    altitude_ft: float
    speed_kt: float
    phase: str = "enroute"
    runway: str = ""


@dataclass
class ControlledFlight:
    id: str
    clearance: FlightClearance
    last_phase: str = "enroute"
    last_instruction_s: float = -1e6
    owns_runway: bool = False
    hold_until_s: float = 0.0
    completed: bool = False
    go_arounds: int = 0
    go_around_heading: float | None = None
    go_around_altitude_ft: float = 0.0


class AIFlightDirector:
    """Closed-loop kinematics, terminal control and AI radio protocol.

    Departure/arrival geometry is deliberately approximate. The current MVP
    does not integrate SID/STAR procedures, performance tables or actual ATC
    sector boundaries. No commands are sent to the user's aircraft.
    """
    def __init__(self, frequencies: dict[str, float] | None = None,
                 runways: RunwayController | None = None,
                 runway_id: str = "", airport_elevation_ft: float = 0.0,
                 airport_position: Position | None = None,
                 separation_m: float = 5500.0,
                 min_vertical_sep_ft: float = 1000.0):
        self.frequencies = dict(frequencies or {})
        self.runways = runways if runways is not None else RunwayController()
        self.runway_id = runway_id
        self.airport_position = airport_position
        self.airport_elevation_ft = airport_elevation_ft
        self.separation_m = separation_m
        self.min_vertical_sep_ft = min_vertical_sep_ft
        self.tracks: dict[str, ControlledFlight] = {}
        self.lines: list[RadioLine] = []
        self.arrivals: list = []
        self.clock = 0.0
        self.player_position: Position | None = None

    def _radio(self, flight_id: str, station: str, instruction: str,
               readback: str | None = None):
        """Only produce station dialogue where a published frequency exists."""
        mhz = self.frequencies.get(station)
        if mhz is None:
            return
        self.lines.append(RadioLine(station, station.upper(),
                                    f"{flight_id}, {instruction}.", mhz, "atc"))
        if readback:
            self.lines.append(RadioLine(station, flight_id,
                                        f"{readback}, {flight_id}.", mhz, "readback"))

    def _set_clearance(self, f, new: FlightClearance, station: str, reason: str = ""):
        t = self.tracks[f.id]
        old = t.clearance
        if (old.phase == new.phase and
                abs(heading_error(new.heading_deg, old.heading_deg)) < 8 and
                abs(new.altitude_ft - old.altitude_ft) < 750 and
                abs(new.speed_kt - old.speed_kt) < 20):
            t.clearance = new
            return
        t.clearance = new
        # Avoid repeated radio orders when glideslope target changes continuously.
        if self.clock - t.last_instruction_s < 12 and old.phase == new.phase:
            return
        t.last_instruction_s = self.clock
        desc = f"{reason} turn heading {round(new.heading_deg)%360:03d}, " \
               f"maintain {round(new.altitude_ft/100)*100:.0f} feet, " \
               f"speed {round(new.speed_kt):.0f} knots".strip()
        self._radio(f.id, station, desc,
                    f"heading {round(new.heading_deg)%360:03d}, " \
                    f"{round(new.altitude_ft/100)*100:.0f} feet, " \
                    f"{round(new.speed_kt):.0f} knots")

    def register(self, f):
        if f.id in self.tracks:
            return
        clearance = FlightClearance(f.position.heading, f.position.alt_ft,
                                     max(130.0, f.position.speed_kt))
        self.tracks[f.id] = ControlledFlight(f.id, clearance)
        self._radio(f.id, "center", "radar contact, maintain present altitude",
                    "maintaining present altitude")

    def forget(self, id: str, keep_runway: bool = False):
        t = self.tracks.pop(id, None)
        if t is not None and t.owns_runway and not keep_runway:
            self.runways.release(self.runway_id, id)

    def _guidance(self, f, others) -> FlightClearance:
        t = self.tracks[f.id]
        current = f.position
        destination = f.destination
        dist = distance_m(current, destination)
        route_heading = bearing_deg(current, destination)
        speed = clamp(current.speed_kt, 180, 420)
        phase = "enroute"
        altitude = max(current.alt_ft, 1500.0)
        station = "center"

        if self.airport_position is not None and self.runway_id and getattr(f, 'arrival', False):
            dist = distance_m(current, self.airport_position)
            route_heading = bearing_deg(current, self.airport_position)
            if dist <= 45000:
                phase, station = "approach", "approach"
                # Progressive 3-degree glideslope with sensible terminal envelope.
                glide = self.airport_elevation_ft + clamp(dist * 0.0524 / 0.3048, 70, 7500)
                altitude = glide
                speed = 210 if dist > 14000 else 165 if dist > 7000 else 140
            if dist <= 9000:
                if not t.owns_runway and self.clock >= t.hold_until_s:
                    if self.runways.reserve(self.runway_id, f.id, "arrival", self.clock):
                        t.owns_runway = True
                        t.go_around_heading = None
                        self._radio(f.id, "tower",
                                    f"runway {self.runway_id.rsplit(chr(47), 1)[-1]}, cleared to land",
                                    f"runway {self.runway_id.rsplit(chr(47), 1)[-1]}, cleared to land")
                    else:
                        t.go_arounds += 1
                        t.hold_until_s = self.clock + 90
                        t.go_around_heading = (current.heading + 45) % 360
                        t.go_around_altitude_ft = max(current.alt_ft + 1500,
                                                      self.airport_elevation_ft + 4000)
                        self._radio(f.id, "tower", "go around, runway occupied, climb 4000 feet",
                                    "going around, 4000 feet")
                if not t.owns_runway:
                    phase, station = "go-around", "approach"
                    altitude = t.go_around_altitude_ft or (self.airport_elevation_ft + 4000)
                    route_heading = t.go_around_heading if t.go_around_heading is not None else current.heading
                    speed = 195
            if t.owns_runway and dist < 100 and current.alt_ft <= self.airport_elevation_ft + 180:
                t.completed = True
                phase = "landed"
                altitude = self.airport_elevation_ft
                speed = 120

        # Give the real player priority: only our synthetic aircraft maneuvers.
        player = self.player_position
        if (player is not None and phase != 'landed' and
                distance_m(current, player) < self.separation_m and
                abs(current.alt_ft - player.alt_ft) < self.min_vertical_sep_ft):
            route_heading = (current.heading + 40) % 360
            altitude = max(altitude, current.alt_ft + 1400)
            speed = min(speed, max(140, current.speed_kt - 35))
            phase, station = 'vector', 'approach' if dist < 45000 else 'center'

        # Basic strategic conflict management among owned, synthetic aircraft.
        # Higher lexical ID yields to prevent symmetrical conflicting orders.
        if phase != "landed":
            for other in others:
                if other.id == f.id or other.id > f.id:
                    continue
                if (distance_m(current, other.position) < self.separation_m and
                        abs(current.alt_ft - other.position.alt_ft) < self.min_vertical_sep_ft):
                    route_heading = (current.heading + 35) % 360
                    altitude = max(altitude, current.alt_ft + 1100)
                    speed = min(speed, max(140, current.speed_kt - 25))
                    phase, station = "vector", "approach" if dist < 45000 else "center"
                    break

        new = FlightClearance(route_heading, altitude, speed, phase, self.runway_id)
        self._set_clearance(f, new, station)
        return new

    @staticmethod
    def _move(f, clearance: FlightClearance, dt: float):
        p = f.position
        turn = clamp(heading_error(clearance.heading_deg, p.heading),
                     -3.0 * dt, 3.0 * dt)
        heading = (p.heading + turn) % 360
        speed = clamp(clearance.speed_kt, max(0, p.speed_kt - dt * 3.0),
                      p.speed_kt + dt * 3.0)
        speed = max(115.0, speed)
        altitude_delta = clamp(clearance.altitude_ft - p.alt_ft,
                               -1900 * dt / 60.0, 1700 * dt / 60.0)
        altitude = max(0, p.alt_ft + altitude_delta)
        moved = forward(p, heading, speed * KNOT_TO_MPS * dt)
        bank = clamp(turn/max(dt, 1e-6) * 6, -25, 25)
        pitch = clamp(altitude_delta/max(dt, 1e-6)/10, -6, 6)
        f.position = Position(moved.lat, moved.lon, altitude, heading,
                              speed, False, pitch, bank)

    def tick(self, aircraft: dict, dt: float):
        if not 0 < dt <= 10:
            raise ValueError("AI flight tick must be 0 < dt <= 10 seconds")
        self.clock += dt
        for flight in list(aircraft.values()):
            self.register(flight)
        for id in list(self.tracks):
            if id not in aircraft:
                self.forget(id)
        # Deterministic order and a snapshot before moving any aircraft.
        snapshot = sorted(aircraft.values(), key=lambda a: a.id)
        for f in snapshot:
            if f.id not in aircraft:
                continue
            clearance = self._guidance(f, snapshot)
            self._move(f, clearance, dt)
            if self.tracks[f.id].completed:
                # Defer releasing runway until GroundEngine takes ownership.
                self.arrivals.append(f)
                aircraft.pop(f.id, None)

    def take_arrivals(self) -> list:
        arrivals = self.arrivals
        self.arrivals = []
        return arrivals

    def close(self):
        for id in list(self.tracks):
            self.forget(id)
        self.lines.clear()
