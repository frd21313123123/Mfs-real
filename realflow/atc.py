"""Offline ATC radio/clearance engine for player and RealFlow-controlled aircraft.

Training simulation only. Not connected to VATSIM or real aviation radio.
Real ADS-B flights are observations and must not be treated as controllable AI.
"""
from __future__ import annotations

import re
from dataclasses import dataclass
from .runway import RunwayController

STAGES = ("filed", "cleared", "pushed", "taxi", "departure", "airborne", "cruise",
          "inbound", "approach", "landed")
_ROLES = {
    "filed": "delivery", "cleared": "ground", "pushed": "ground", "taxi": "tower",
    "departure": "tower", "airborne": "departure", "cruise": "center",
    "inbound": "approach", "approach": "tower", "landed": "ground",
}
_WORDS = {"takeoff": ("takeoff", "take off"), "taxi": ("taxi",),
          "pushback": ("pushback", "push back"), "landing": ("land", "landing"),
          "clearance": ("clearance", "clearance delivery", "cleared")}


@dataclass(frozen=True)
class RadioLine:
    station: str
    speaker: str
    message: str
    mhz: float | None
    kind: str = "traffic"


@dataclass
class Clearance:
    action: str
    mandatory: tuple[str, ...]
    next_stage: str


def _normalized(message: str) -> str:
    return re.sub(r"\s+", " ", re.sub(r"[^a-z0-9 ]", " ", message.lower())).strip()


def _said(message: str, *phrases: str) -> bool:
    clean = f" {_normalized(message)} "
    return any(f" {phrase} " in clean for phrase in phrases)


def _contains_number(message: str, value: str | int) -> bool:
    return re.search(r"(?<!\d)" + re.escape(str(value)) + r"(?!\d)", message) is not None


_DIGIT_NAMES = ("zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine")
_RUNWAY_SUFFIXES = {"l": "left", "r": "right", "c": "center"}


def _readback_contains(message: str, expected: str) -> bool:
    """Allow digit-by-digit and simple ICAO-style spoken numbers from Vosk."""
    expected = expected.lower()
    if _contains_number(message, expected) or _said(message, expected):
        return True
    match = re.fullmatch(r"(\d{1,4})([lrc])?", expected)
    if not match:
        return False
    digits, suffix = match.groups()
    aliases = [" ".join(_DIGIT_NAMES[int(digit)] for digit in digits)]
    if len(digits) == 4 and digits.endswith("000"):
        aliases.append(_DIGIT_NAMES[int(digits[0])] + " thousand")
    if len(digits) == 2:
        aliases.append({ "18": "eighteen", "24": "twenty four"}.get(digits, ""))
    if suffix:
        aliases = [a + " " + _RUNWAY_SUFFIXES[suffix] for a in aliases]
    return any(a and _said(message, a) for a in aliases)


class PilotATC:
    """IFR phraseology training; state advances only after required readbacks.

    Does NOT claim the real world controller is online. Runway occupancy is
    coordinated with RealFlow's existing RunwayController.
    """
    def __init__(self, callsign: str, origin: str, destination: str,
                 frequencies: dict[str, float], runway: str,
                 runways: RunwayController | None = None, altitude_ft: int = 5000,
                 runway_key: str | None = None):
        self.callsign = callsign.upper().strip()
        self.origin = origin.upper().strip()
        self.destination = destination.upper().strip()
        if not re.fullmatch(r"[A-Z0-9-]{3,12}", self.callsign):
            raise ValueError("Invalid callsign")
        if not re.fullmatch(r"[A-Z0-9]{4}", self.origin) or not re.fullmatch(r"[A-Z0-9]{4}", self.destination):
            raise ValueError("Airport must be ICAO code")
        if not runway.strip():
            raise ValueError("Runway must be specified")
        self.runway = runway.upper()
        self.runway_key = runway_key or self.runway
        self.frequencies = {s.lower(): round(float(v), 3) for s, v in frequencies.items()
                            if 118 <= float(v) <= 136.99}
        self.runways = runways if runways is not None else RunwayController()
        self.altitude_ft = altitude_ft
        self.squawk = "4101"   # simulation-only placeholder, not a real-world assignment
        self.stage = "filed"
        self.pending: Clearance | None = None
        self.com1 = None
        self.com2 = None
        self.transmitting = 1
        self.actual_squawk = None
        self.lines: list[RadioLine] = []
        self._runway_owned = False

    @property
    def tuned(self) -> float | None:
        return self.com1 if self.transmitting == 1 else self.com2

    @property
    def station(self) -> str | None:
        if self.tuned is None:
            return None
        for role, mhz in self.frequencies.items():
            if abs(mhz - self.tuned) <= 0.001:
                return role
        return None

    def update_cockpit(self, com1: float | None, com2: float | None = None,
                       transmitting: int = 1, squawk: str | None = None):
        """Use native COM panels as read-only input; never overwrite cockpit."""
        self.com1 = round(com1, 3) if com1 is not None else None
        self.com2 = round(com2, 3) if com2 is not None else None
        self.transmitting = 2 if transmitting == 2 else 1
        if squawk is not None:
            self.actual_squawk = str(squawk)

    def _say(self, station: str, text: str, kind: str = "atc") -> RadioLine:
        line = RadioLine(station, f"{self.origin} {station.upper()}",
                         f"{self.callsign}, {text}", self.frequencies.get(station), kind)
        self.lines.append(line)
        return line

    def _clear(self, station: str, text: str, action: str,
               mandatory: tuple[str, ...], next_stage: str):
        self.pending = Clearance(action, mandatory, next_stage)
        return self._say(station, text, "clearance")

    def _handoff(self, source: str, target: str) -> RadioLine:
        if target not in self.frequencies:
            return self._say(source, "remain on this frequency, next sector unavailable in local data.", "info")
        return self._say(source, f"contact {target} {self.frequencies[target]:.3f}.", "handoff")

    def _expected(self) -> str | None:
        expected = _ROLES[self.stage]
        if expected in self.frequencies:
            return expected
        # Real airports may not have Clearance Delivery or Departure published.
        for substitute in (("delivery", "ground"), ("departure", "approach"),
                           ("center", "approach"), ("ground", "tower")):
            if expected == substitute[0] and substitute[1] in self.frequencies:
                return substitute[1]
        return None

    def _readback(self, station: str, message: str) -> RadioLine:
        pending = self.pending
        assert pending is not None
        normalized = _normalized(message)
        missing = [term for term in pending.mandatory
                   if not _readback_contains(normalized, term)]
        if missing:
            return self._say(station, "readback incorrect; missing " + ", ".join(missing) + ". Say again.", "correction")
        self.pending = None
        self.stage = pending.next_stage
        return self._say(station, "readback correct.", "ack")

    def transmit(self, text: str) -> RadioLine:
        if not text.strip():
            raise ValueError("Empty radio transmission")
        active = self.station
        self.lines.append(RadioLine(active or "unknown", self.callsign, text, self.tuned, "pilot"))
        if not active:
            return self._say("radio", "no simulated controller on this frequency.", "unavailable")
        if self.pending:
            if _said(text, "say again", "repeat"):
                return self._say(active, f"repeat clearance: {self.pending.action}, read back {', '.join(self.pending.mandatory)}.", "repeat")
            return self._readback(active, text)

        if self.stage == "landed" and active in ("tower", "ground") and _said(text, "vacated", "clear of runway"):
            self._release_runway()
            return self._handoff(active, "ground")
        expected = self._expected()
        if expected and active != expected and not (self.stage == "taxi" and active == "tower"):
            return self._handoff(active, expected)
        if self.stage == "filed" and _said(text, *_WORDS["clearance"]):
            if active not in ("delivery", "ground"):
                return self._say(active, "contact clearance delivery or ground.", "unavailable")
            return self._clear(active,
                f"IFR clearance to {self.destination}, climb initially {self.altitude_ft} feet, "
                f"squawk {self.squawk}.", "IFR",
                (str(self.altitude_ft), self.squawk), "cleared")
        if self.stage == "cleared" and _said(text, *_WORDS["pushback"]):
            return self._clear(active, "pushback approved, report ready to taxi.",
                               "pushback", ("approved",), "pushed")
        if self.stage in ("cleared", "pushed") and _said(text, *_WORDS["taxi"]):
            return self._clear(active, f"taxi to runway {self.runway}, hold short.",
                               "taxi", (self.runway.lower(), "hold"), "taxi")
        if self.stage == "taxi" and active == "tower" and _said(text, "ready", "departure", *_WORDS["takeoff"]):
            if self.actual_squawk is not None and self.actual_squawk != self.squawk:
                return self._say(active, f"check transponder, squawk {self.squawk}.", "correction")
            if not self.runways.reserve(self.runway_key, self.callsign, "departure"):
                return self._say(active, f"hold short runway {self.runway}, traffic on runway.", "hold")
            self._runway_owned = True
            return self._clear(active, f"runway {self.runway}, cleared for takeoff.",
                               "takeoff", (self.runway.lower(), "takeoff"), "departure")
        if self.stage == "departure" and _said(text, "airborne", "positive climb"):
            self._release_runway()
            self.stage = "airborne"
            return self._handoff(active, "departure")
        if self.stage == "airborne" and _said(text, "passing", "climb", "with you"):
            return self._clear(active, "radar contact, climb and maintain flight level 300.",
                               "climb", ("300",), "cruise")
        if self.stage == "cruise" and _said(text, "descent", "approach", "inbound"):
            self.stage = "inbound"
            return self._handoff(active, "approach")
        if self.stage == "inbound" and _said(text, "inbound", "approach", "vectors"):
            return self._clear(active, f"descend {self.altitude_ft} feet, cleared approach runway {self.runway}.",
                               "approach", (str(self.altitude_ft), self.runway.lower()), "approach")
        if self.stage == "approach" and active == "tower" and _said(text, *_WORDS["landing"], "final"):
            if not self.runways.reserve(self.runway_key, self.callsign, "arrival"):
                return self._say(active, "go around, runway occupied. Maintain runway heading.", "go-around")
            self._runway_owned = True
            return self._clear(active, f"runway {self.runway}, cleared to land.",
                               "land", (self.runway.lower(), "land"), "landed")
        if self.stage == "landed" and _said(text, "vacated", "clear of runway"):
            self._release_runway()
            return self._handoff(active, "ground")
        if _said(text, "say again", "repeat"):
            return self._say(active, "say request again.", "repeat")
        return self._say(active, f"unable, check request for phase {self.stage}.", "unable")

    def _release_runway(self):
        if self._runway_owned:
            self.runways.release(self.runway_key, self.callsign)
            self._runway_owned = False

    def close(self):
        self._release_runway()


class AIAirportController:
    """Automatic ATC dialogue and movement gating for OWNED ground aircraft.

    An AI aircraft receives/readbacks taxi and takeoff clearances before a
    segment is traversed. The GroundEngine remains the collision/runway arbiter.
    """
    def __init__(self, frequencies: dict[str, float], runways: RunwayController):
        self.frequencies = frequencies
        self.runways = runways
        self.grants: dict[str, set[str]] = {}
        self.lines: list[RadioLine] = []
        self.waiting: set[str] = set()

    def _exchange(self, pilot: str, role: str, request: str, response: str, readback: str):
        freq = self.frequencies.get(role)
        self.lines.extend([
            RadioLine(role, pilot, f"{role}, {pilot}, {request}", freq, "ai"),
            RadioLine(role, role.upper(), f"{pilot}, {response}", freq, "atc"),
            RadioLine(role, pilot, f"{readback}, {pilot}", freq, "readback"),
        ])

    def allow_edge(self, plane, edge) -> bool:
        grants = self.grants.setdefault(plane.id, set())
        role = "tower" if edge.runway else "ground"
        # No invented controller when the field has no station in the source.
        if role not in self.frequencies:
            return False
        if edge.kind == "pushback" and "pushback" not in grants:
            self._exchange(plane.id, "ground", "request pushback", "pushback approved",
                           "pushback approved")
            grants.add("pushback")
        if not edge.runway:
            if edge.kind != "pushback" and "taxi" not in grants:
                self._exchange(plane.id, "ground", "request taxi",
                               "taxi approved, hold short of active runway",
                               "taxi approved, hold short")
                grants.add("taxi")
            return True
        if not self.runways.is_available(edge.runway, plane.id):
            if plane.id not in self.waiting:
                self.lines.append(RadioLine("tower", "TOWER",
                    f"{plane.id}, hold short runway {edge.runway}, landing or departing traffic.",
                    self.frequencies.get("tower"), "hold"))
                self.waiting.add(plane.id)
            return False
        self.waiting.discard(plane.id)
        operation = "landing" if plane.arrival else "takeoff"
        if operation not in grants:
            if plane.arrival:
                runway_label = edge.runway.rsplit("/", 1)[-1]
                self._exchange(plane.id, "tower", "established on final",
                               f"runway {runway_label}, cleared to land",
                               f"runway {runway_label}, cleared to land")
            else:
                runway_label = edge.runway.rsplit("/", 1)[-1]
                self._exchange(plane.id, "tower", "ready for departure",
                               f"runway {runway_label}, cleared for takeoff",
                               f"runway {runway_label}, cleared for takeoff")
            grants.add(operation)
        return True

    def forget(self, aircraft_id: str):
        self.grants.pop(aircraft_id, None)
        self.waiting.discard(aircraft_id)

    def heard_on(self, mhz: float | None) -> list[RadioLine]:
        if mhz is None:
            return []
        return [line for line in self.lines if line.mhz is not None and abs(line.mhz - mhz) <= 0.001]
