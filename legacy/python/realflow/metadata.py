"""Optional, locally maintained ICAO24 -> ICAO aircraft type enrichment.

OpenSky states alone do not identify the aircraft model, so optional CSV metadata
is required for accurate FSLTL model matching beyond airline-prefix heuristics.
"""
import csv
from dataclasses import replace
from pathlib import Path
from .live import Observation

class AircraftMetadata:
    def __init__(self, icao_to_type:dict[str,str]):
        self.icao_to_type={k.lower().strip():v.upper().strip() for k,v in icao_to_type.items() if k and v}

    @classmethod
    def from_csv(cls,path:str|Path):
        with open(path,encoding='utf-8-sig',newline='') as inp:
            reader=csv.DictReader(inp)
            if not {'icao24','icao_type'}.issubset(reader.fieldnames or []):
                raise ValueError('CSV must have icao24,icao_type columns')
            return cls({row['icao24']:row['icao_type'] for row in reader})

    def enrich(self, observations:list[Observation])->list[Observation]:
        return [replace(o,icao_type=self.icao_to_type.get(o.icao24,o.icao_type)) for o in observations]
