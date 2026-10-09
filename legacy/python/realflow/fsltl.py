"""Local-only FSLTL aircraft.cfg catalog: no redistribution of aircraft assets."""
import re
import os
from pathlib import Path
from dataclasses import dataclass

@dataclass(frozen=True)
class AircraftModel:
    title: str
    aircraft_type: str
    airline: str
    path: str
    variation: str = ""


def known_roots() -> list[Path]:
    """Handle conventional Steam/MS Store MSFS 2020 package paths."""
    roots=[]
    home=Path.home()
    local=Path(os.environ.get("LOCALAPPDATA", str(home / 'AppData' / 'Local')))
    roaming=Path(os.environ.get("APPDATA",str(home / 'AppData' / 'Roaming')))
    storecfg=local/'Packages'/'Microsoft.FlightSimulator_8wekyb3d8bbwe'/'LocalCache'/'UserCfg.opt'
    steamcfg=roaming/'Microsoft Flight Simulator'/'UserCfg.opt'
    for config_path in (storecfg,steamcfg):
        if config_path.is_file():
            try:
                cfg=config_path.read_text(encoding='utf-8-sig',errors='replace')
                match=re.search(r'(?im)^InstalledPackagesPath\s+"([^"]+)"',cfg)
                if match:
                    roots.append(Path(match.group(1))/'Community')
            except OSError: pass
    roots.extend([
        local/'Packages'/'Microsoft.FlightSimulator_8wekyb3d8bbwe'/'LocalCache'/'Packages'/'Community',
        roaming/'Microsoft Flight Simulator'/'Packages'/'Community',
        home/'AppData'/'Local'/'MSFSPackages'/'Community',
    ])
    for var in ('MSFS_COMMUNITY', 'FSLTL_BASE'):
        if os.environ.get(var): roots.insert(0, Path(os.environ[var]))
    return roots


def discover_root(explicit: str = "") -> Path | None:
    for base in ([Path(explicit)] if explicit else known_roots()):
        if (base/'fsltl-traffic-base').is_dir():
            return base/'fsltl-traffic-base'
        if base.name.lower() == 'fsltl-traffic-base' and base.is_dir():
            return base
    return None


def _strip(value: str) -> str:
    return value.strip().strip('"').strip("'")


def scan_models(root: Path) -> list[AircraftModel]:
    """Parse [FLTSIM.N] blocks; accepts duplicate fields and nonstandard cfg formatting."""
    if not root.is_dir():
        raise FileNotFoundError(root)
    models=[]
    for cfg in root.rglob('aircraft.cfg'):
        try:
            data=cfg.read_text(encoding='utf-8-sig', errors='replace')
        except OSError:
            continue
        for block in re.split(r'(?im)^\s*\[\s*fltsim\s*\.\s*\d+\s*\]\s*$',data)[1:]:
            # Stop at a different non-FLTSIM section to avoid keys leaking in.
            block=re.split(r'(?m)^\s*\[[^\]]+\]',block,maxsplit=1)[0]
            attrs={}
            for line in block.splitlines():
                if '=' not in line or line.lstrip().startswith((';', '#', '//')):
                    continue
                k,v=line.split('=',1)
                attrs[k.strip().lower()]=_strip(re.split(r'\s+;',v,maxsplit=1)[0])
            title=attrs.get('title','')
            if not title: continue
            models.append(AircraftModel(title=title,
                aircraft_type=(attrs.get('icao_type_designator') or attrs.get('atc_model') or attrs.get('ui_type') or '').upper(),
                airline=(attrs.get('icao_airline') or attrs.get('atc_airline') or '').upper(),
                path=str(cfg),variation=attrs.get('ui_variation','')))
    return models


class ModelMatcher:
    def __init__(self, models: list[AircraftModel]):
        self.models=sorted(models,key=lambda m:m.title)

    def choose(self, icao_type: str = '', airline: str = '') -> AircraftModel | None:
        if not self.models: return None
        a=airline.strip().upper()
        t=icao_type.strip().upper()
        def score(m: AircraftModel):
            name=m.title.upper()
            typ=m.aircraft_type
            air=m.airline
            return (
                (110 if t and typ == t else 35 if t and t in name else 0) +
                (55 if a and air == a else 10 if a and a in name else 0) +
                (5 if not t else 0),
                -len(m.title)
            )
        result=max(self.models,key=score)
        # No unrestricted arbitrary-aircraft fallback for live traffic if the type cannot be identified.
        if t and not (result.aircraft_type == t or t in result.title.upper()):
            family=t[:2]
            compatible=[m for m in self.models if m.aircraft_type.startswith(family) and family]
            if compatible: return max(compatible,key=score)
            return None
        return result
