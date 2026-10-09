"""Persisted configuration, with conservative defaults for real simulator safety."""
import json
from dataclasses import dataclass, asdict
from pathlib import Path

@dataclass
class Settings:
    mode: str = "hybrid"
    airborne_limit: int = 35
    ground_limit: int = 30
    realistic_taxi: bool = True
    runway_control: bool = True
    sync_real_flights: bool = True
    fetch_seconds: int = 120
    max_radius_km: float = 120.0
    fps_target: int = 35
    fsltl_path: str = ""
    airport_graph: str = ""
    # Deliberately disabled until native bridge validated in simulator.
    enable_simconnect_position_writes: bool = False

    def validate(self):
        if self.mode not in ("hybrid", "live", "simulation"):
            raise ValueError("mode must be hybrid, live or simulation")
        if not 0 <= self.airborne_limit <= 35 or not 0 <= self.ground_limit <= 30:
            raise ValueError("airborne_limit <=35; ground_limit <=30")
        if self.fetch_seconds < 60:
            raise ValueError("OpenSky polling interval must be >= 60 s")
        if not 2 <= self.max_radius_km <= 250:
            raise ValueError("max_radius_km must be 2..250")
        return self

    @classmethod
    def load(cls, path: str | Path):
        p=Path(path)
        if not p.exists():
            return cls()
        raw=json.loads(p.read_text(encoding='utf-8'))
        return cls(**raw).validate()

    def save(self, path: str | Path):
        self.validate()
        p=Path(path)
        p.parent.mkdir(parents=True,exist_ok=True)
        p.write_text(json.dumps(asdict(self),indent=2,ensure_ascii=False),encoding='utf-8')
