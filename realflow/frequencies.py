"""Real-world *published* airport radio frequencies.

OurAirports is community-maintained, not an authoritative source of current ATC
services. Only a station actually present in the CSV will be exposed: no
invented sector frequencies or fallback frequency masquerading as real.
"""
from __future__ import annotations

import csv
import io
from dataclasses import dataclass
from pathlib import Path
from urllib.request import Request, urlopen

DATA_URL = "https://davidmegginson.github.io/ourairports-data/airport-frequencies.csv"
STATION_ALIASES = {
    "TWR": "tower", "TOWER": "tower",
    "GND": "ground", "GROUND": "ground",
    "CLD": "delivery", "CLR": "delivery", "DEL": "delivery", "CLEARANCE": "delivery",
    "DEP": "departure", "DEPARTURE": "departure",
    "APP": "approach", "APPR": "approach", "ARR": "approach",
    "CTR": "center", "CENTER": "center", "ACC": "center",
    "ATIS": "atis", "UNICOM": "unicom", "CTAF": "ctaf", "ATF": "ctaf",
}


@dataclass(frozen=True)
class Frequency:
    airport: str
    station: str
    mhz: float
    description: str
    source: str = "OurAirports (community data, verify before use)"


class FrequencyDirectory:
    def __init__(self, frequencies: list[Frequency]):
        self.by_airport: dict[str, list[Frequency]] = {}
        for frequency in frequencies:
            self.by_airport.setdefault(frequency.airport, []).append(frequency)

    @classmethod
    def from_csv_text(cls, content: str) -> "FrequencyDirectory":
        result = []
        reader = csv.DictReader(io.StringIO(content))
        required = {"airport_ident", "type", "frequency_mhz"}
        if not required.issubset(reader.fieldnames or []):
            raise ValueError("Expected OurAirports airport-frequencies.csv columns")
        for row in reader:
            airport = (row.get("airport_ident") or "").strip().upper()
            station = STATION_ALIASES.get((row.get("type") or "").strip().upper())
            if not station or not (3 <= len(airport) <= 5 and airport.isalnum()):
                continue
            try:
                mhz = round(float(row["frequency_mhz"]), 3)
            except (ValueError, TypeError):
                continue
            if not 118.0 <= mhz <= 136.99:
                continue
            result.append(Frequency(airport, station, mhz, (row.get("description") or "").strip()))
        return cls(result)

    @classmethod
    def load(cls, path: str | Path) -> "FrequencyDirectory":
        return cls.from_csv_text(Path(path).read_text(encoding="utf-8-sig"))

    def lookup(self, airport: str, station: str | None = None) -> list[Frequency]:
        results = self.by_airport.get(airport.upper().strip(), [])
        if station:
            return [f for f in results if f.station == station.lower().strip()]
        return list(results)

    def resolve(self, airport: str, mhz: float, tolerance_khz: float = 1.0) -> list[Frequency]:
        return [f for f in self.lookup(airport) if abs(f.mhz - mhz) * 1000 <= tolerance_khz]

    def primary(self, airport: str) -> dict[str, float]:
        # Multiple stations of a type may exist, especially at large airports.
        # Do not imply we have selected the operational sector for the pilot.
        found = {}
        for f in self.lookup(airport):
            found.setdefault(f.station, f.mhz)
        return found


def download_ourairports(path: str | Path, timeout: int = 20) -> int:
    """Explicit download only; never fetch from a real-time simulation tick."""
    if not 1 <= timeout <= 120:
        raise ValueError("Invalid timeout")
    req = Request(DATA_URL, headers={"User-Agent": "RealFlowTraffic/0.2 ATC research"})
    with urlopen(req, timeout=timeout) as response:
        data = response.read(8_000_001)
    if len(data) > 8_000_000:
        raise ValueError("Frequency CSV unexpectedly large")
    text = data.decode("utf-8-sig")
    directory = FrequencyDirectory.from_csv_text(text)
    count = sum(map(len, directory.by_airport.values()))
    if count < 500:
        raise ValueError("Unexpectedly small frequency dataset; refusing to overwrite cache")
    target = Path(path)
    target.parent.mkdir(parents=True, exist_ok=True)
    tmp = target.with_suffix(target.suffix + ".tmp")
    tmp.write_text(text, encoding="utf-8")
    tmp.replace(target)
    return count
