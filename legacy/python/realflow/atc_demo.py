"""A complete reproducible offline ATC/AI/ground integration smoke test.

Fictional airport, fictional COM data and placeholder plane titles are used
ONLY with MockBridge. No MSFS aircraft are created by this command.
"""
import argparse
import json
from pathlib import Path
from .airport import AirportGraph
from .atc import AIAirportController, PilotATC
from .bridge import MockBridge
from .config import Settings
from .flight_director import AIFlightDirector
from .fsltl import ModelMatcher, AircraftModel
from .geo import Position, bearing_deg
from .ground import GroundEngine
from .manager import TrafficManager
from .pilot_console import PilotRadioConsole


FIXTURE = Path(__file__).resolve().parent.parent / "examples/demo_airport.json"
FICTIONAL_RADIOS = {"delivery":121.6,"ground":121.9,"tower":118.7,
                    "departure":124.2,"center":125.7,"approach":120.4}


def demo(steps: int = 600) -> dict:
    if steps < 1 or steps > 7200:
        raise ValueError("steps must be 1..7200")
    graph = AirportGraph.from_json(FIXTURE)
    runway="TEST/18"
    field=graph.nodes["RUNWAY_EXIT"].position
    ground = GroundEngine(graph,max_ground=3)
    ground.add("DEMO101","MOCK A320","GATE_A","RUNWAY_EXIT")
    ai = AIAirportController(FICTIONAL_RADIOS,ground.runways)
    flight_atc = AIFlightDirector(FICTIONAL_RADIOS,ground.runways,runway,field.alt_ft,field)
    flight_atc.approach_heading=bearing_deg(graph.nodes["RUNWAY_ENTRY"].position,field)
    pilot = PilotATC("USR101","TEST","DEMO",FICTIONAL_RADIOS,"18",
                     runways=ground.runways,runway_key=runway)
    console = PilotRadioConsole(pilot, field.alt_ft, offline_com1=121.6)
    console.submit("request IFR clearance")
    console.poll(now=0)
    console.submit("climb 5000 squawk 4101")
    console.poll(now=1)
    bridge=MockBridge()
    manager=TrafficManager(Settings(mode="simulation",airborne_limit=8,ground_limit=3),
                           ModelMatcher([AircraftModel("MOCK A320","A320","DEM","MOCK")]),
                           bridge,ground,ai_atc=ai,flight_director=flight_atc)
    max_air=0
    max_ground=0
    seen=set()
    events=0
    try:
        for sec in range(steps):
            result=manager.tick(1,1000000+sec,
                        Position(field.lat,field.lon,field.alt_ft+2000),
                        synthetic_target=3)
            seen.update(ground.aircraft.keys())
            max_air=max(max_air,result["synthetic"])
            max_ground=max(max_ground,result["ground"])
            events+=len(ai.lines)+len(flight_atc.lines)
        return {
            "simulation":"FICTIONAL OFFLINE ONLY",
            "steps":steps,
            "pilot_stage":pilot.stage,
            "ai_radio_lines":len(ai.lines),
            "air_atc_lines":len(flight_atc.lines),
            "max_airborne":max_air,
            "max_ground":max_ground,
            "unique_ground_flights":len(seen),
            "active_objects_before_shutdown":len(bridge.objects),
            "success":(pilot.stage=="cleared" and max_air<=8 and max_ground<=3
                       and len(ai.lines)>0 and len(flight_atc.lines)>0),
        }
    finally:
        console.close()
        manager.close()


def main(argv=None) -> int:
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--steps",type=int,default=600)
    parser.add_argument("--output",default="atc-demo-results.json")
    args=parser.parse_args(argv)
    result=demo(args.steps)
    out=Path(args.output)
    out.parent.mkdir(parents=True,exist_ok=True)
    out.write_text(json.dumps(result,indent=2),encoding="utf-8")
    print(json.dumps(result,indent=2))
    return 0 if result["success"] else 1


if __name__=="__main__":
    raise SystemExit(main())
