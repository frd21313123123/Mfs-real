"""CLI: offline diagnostics/demo and optional Windows real SimConnect bridge."""
import argparse
import json
import os
import platform
import sys
import time
from dataclasses import asdict
from pathlib import Path
from .__init__ import __version__
from .geo import Position
from .airport import AirportGraph
from .airport_xml import convert
from .bridge import BridgeError,MockBridge
from .config import Settings
from .fsltl import discover_root,scan_models,ModelMatcher,AircraftModel
from .ground import GroundEngine
from .manager import TrafficManager
from .live import OpenSkyClient, Observation
from .metadata import AircraftMetadata

EXAMPLE_GRAPH=Path(__file__).resolve().parent.parent/'examples'/'demo_airport.json'

def find_models(path:str='', allow_fixtures:bool=False):
    root=discover_root(path)
    if root:
        models=scan_models(root)
        if models: return models,root
    if allow_fixtures:
        return [AircraftModel('DEMO FSLTL A320 (NOT AN INSTALLED MODEL)','A320','AFL','DEMO')],None
    return [],root

def doctor(args):
    root=discover_root(args.fsltl)
    models=scan_models(root) if root else []
    state={
        'platform':platform.platform(),'python':sys.version.split()[0],
        'fsltl_found':bool(root),'fsltl_path':str(root) if root else None,
        'models':len(models),'simconnect_supported_os':os.name=='nt',
        'simconnect_dll_available':None,'opensky_oauth2_configured':bool(os.getenv('OPENSKY_CLIENT_ID') and os.getenv('OPENSKY_CLIENT_SECRET'))}
    if os.name=='nt':
        import ctypes
        try: ctypes.WinDLL('SimConnect.dll');state['simconnect_dll_available']=True
        except OSError: state['simconnect_dll_available']=False
    print(json.dumps(state,indent=2,ensure_ascii=False))

def scan(args):
    models,root=find_models(args.fsltl)
    if not root:
        print('FSLTL Base Models not found. Specify --fsltl directory.');return 2
    print(f'Found {len(models)} aircraft titles in {root}')
    for model in models[:args.limit]:
        print(f'  [{model.aircraft_type or "?"}] {model.airline or "?"}: {model.title}')
    return 0

def offline_demo(args):
    settings=Settings.load(args.config)
    models,root=find_models(args.fsltl,allow_fixtures=True)
    origin=Position(47.452,-122.3095,5000)
    ground=GroundEngine(AirportGraph.from_json(EXAMPLE_GRAPH),max_ground=settings.ground_limit)
    if settings.ground_limit:
        ground.add('TEST101',models[0].title,'GATE_A','RUNWAY_EXIT')
        if settings.ground_limit>1:
            ground.add('TEST102',models[0].title,'GATE_B','RUNWAY_EXIT')
    bridge=MockBridge()
    manager=TrafficManager(settings,ModelMatcher(models),bridge,ground)
    phases={}
    try:
        for sec in range(args.steps):
            now=1000000.0+sec
            if sec%120==0 and settings.mode!='simulation':
                # Synthetic offline observation that exercises the *LIVE path*.
                # This is explicitly test data, not a claim of a real observed flight.
                ob=Observation('demo-live-001','AFL112',
                    Position(47.50,-122.30+0.01*(sec/120),7000,75,210,False),0,now-2,'A320')
                manager.feed([ob],origin,now)
            totals=manager.tick(1.0,now,origin,synthetic_target=min(8,settings.airborne_limit))
            if sec%10==0:
                phases[str(sec)]={key:ac.phase.value for key,ac in ground.aircraft.items()}
    finally:
        # Capture object statistics before cleaning up.
        active_before_close=len(bridge.objects)
        summary={'version':__version__, 'mode':settings.mode,'model_origin':str(root) if root else 'demo fixture (not MSFS-usable)',
                 'duration_seconds':args.steps, 'limits':{'air':settings.airborne_limit,'ground':settings.ground_limit},
                 'traffic':totals,'events':{'creates':sum(e[0]=='create' for e in bridge.events),
                  'updates':sum(e[0]=='update' for e in bridge.events)},
                 'active_before_shutdown':active_before_close,'phases':phases,
                 'runway_reservations':{k:asdict(v) for k,v in ground.runways.reservations.items()}}
        manager.close()
        summary['clean_shutdown']=len(bridge.objects)==0
    path=Path(args.output)
    path.parent.mkdir(parents=True,exist_ok=True)
    path.write_text(json.dumps(summary,indent=2,ensure_ascii=False),encoding='utf-8')
    print(json.dumps({k:v for k,v in summary.items() if k!='phases'},indent=2,ensure_ascii=False))
    print(f'Demo report: {path.resolve()}')
    return 0

def run(args):
    settings=Settings.load(args.config)
    if args.bridge=='simconnect' and os.name!='nt':
        raise BridgeError('SimConnect requires Windows and a running MSFS 2020')
    models,root=find_models(args.fsltl or settings.fsltl_path)
    if not models:
        raise RuntimeError('FSLTL models not found. Run doctor or set --fsltl path. No fallback sample models are injected.')
    if args.bridge=='simconnect':
        if args.atc_csv:
            from .cockpit_radio import CockpitSimConnectBridge
            bridge=CockpitSimConnectBridge(enable_position_writes=args.allow_motion)
        else:
            from .simconnect import SimConnectBridge
            bridge=SimConnectBridge(enable_position_writes=args.allow_motion)
    else:
        bridge=MockBridge()
    bridge.connect()
    airport=None
    if args.airport:
        if args.bridge=='simconnect' and Path(args.airport).resolve()==EXAMPLE_GRAPH.resolve():
            raise RuntimeError('Bundled airport graph is fictional. Provide a VERIFIED real airport JSON.')
        if args.bridge=='simconnect' and not args.allow_motion:
            raise RuntimeError('Ground movement requires --allow-motion; airport JSON must be VERIFIED against in-sim airport')
        airport=GroundEngine(AirportGraph.from_json(args.airport),max_ground=settings.ground_limit)
    ai_atc=None
    if args.atc_csv:
        if not airport:
            raise ValueError('--atc-csv requires --airport with validated taxi graph')
        from .frequencies import FrequencyDirectory
        from .atc import AIAirportController
        directory=FrequencyDirectory.load(args.atc_csv)
        stations=directory.primary(airport.airport.icao)
        if not stations.get('ground') or not stations.get('tower'):
            raise ValueError('No published GROUND/TOWER frequencies for this airport; AI ATC cannot start')
        ai_atc=AIAirportController(stations,airport.runways)
        print('AI ATC enabled using community frequency records; verify against AIP.')
    manager=TrafficManager(settings,ModelMatcher(models),bridge,airport,ai_atc=ai_atc)
    live=OpenSkyClient(minimum_interval=settings.fetch_seconds) if settings.sync_real_flights and settings.mode!='simulation' else None
    metadata=AircraftMetadata.from_csv(args.metadata) if args.metadata else None
    radio_speaker=None
    last_ai_message=0
    if args.atc_voice:
        if ai_atc is None:
            raise ValueError('--atc-voice requires --atc-csv with a verified airport graph')
        from .voice import RadioSpeaker
        radio_speaker=RadioSpeaker()
    last=time.monotonic();started=last;next_fetch=0.0
    next_ground=0.0
    ground_counter=0
    origin=Position(args.lat,args.lon,args.alt_ft)
    print('Started. Native SimConnect bridge is EXPERIMENTAL. Press Ctrl+C to stop.')
    try:
        while args.duration<=0 or time.monotonic()-started<args.duration:
            bridge.poll()
            now=time.monotonic()
            dt=max(0.05,min(1.0,now-last));last=now
            if bridge.player_position is not None and args.follow_user:
                origin=bridge.player_position
            if live and now>=next_fetch:
                try:
                    # Independent lower-frequency OpenSky polling, never per simulation tick.
                    observations=live.poll(origin.lat,origin.lon,settings.max_radius_km)
                    if metadata: observations=metadata.enrich(observations)
                    manager.feed(observations,origin,time.time())
                    print(f'OpenSky: {len(observations)} observations, {len(manager.fleet.aircraft)} selected')
                except Exception as exc:
                    print(f'OpenSky issue: {exc}',file=sys.stderr)
                next_fetch=max(now+settings.fetch_seconds,live.next_allowed)
            if airport and settings.mode!='live' and now>=next_ground:
                # Use only validated user-supplied taxi geometry. Do not invent taxi
                # paths from geographic positions alone.
                gates=[n.id for n in airport.airport.nodes.values() if n.kind=='gate']
                runway_legs=[edge for edge in airport.airport.edges.values()
                             if edge.kind=='runway' and
                             airport.airport.nodes[edge.src].kind=='runway' and
                             airport.airport.nodes[edge.dst].kind=='runway']
                if gates and runway_legs:
                    runway_exit=runway_legs[0].dst
                    occupied_gates={a.route[0] for a in airport.aircraft.values() if not a.arrival}
                    for gate in gates:
                        if gate not in occupied_gates and len(airport.aircraft)<settings.ground_limit:
                            match=manager.fleet.matcher.models
                            if not match: break
                            ground_counter+=1
                            try:airport.add(f'AUTO-{ground_counter}',match[ground_counter%len(match)].title,gate,runway_exit)
                            except ValueError: pass
                next_ground=now+180
            stats=manager.tick(dt,time.time(),origin)
            if ai_atc:
                lines=ai_atc.lines[last_ai_message:]
                last_ai_message=len(ai_atc.lines)
                radio=getattr(bridge,'player_radio',None)
                frequency=(radio.com2_mhz if radio.transmitting==2 else radio.com1_mhz) if radio else args.atc_monitor_frequency
                for line in lines:
                    if line.mhz is not None and frequency is not None and abs(line.mhz-frequency)<=0.001:
                        print(f'ATC [{line.mhz:.3f}] {line.speaker}: {line.message}')
                        if radio_speaker: radio_speaker.speak(line.message)
            if int(now-started)%30==0 and now-last<0.3:
                pass  # No console log spam while running.
            time.sleep(0.25)
    except KeyboardInterrupt:
        print('Shutting down...')
    finally:
        manager.close()
        if radio_speaker: radio_speaker.close()
    return 0

def convert_airport(args):
    stats=convert(args.xml,args.output,args.icao)
    print(json.dumps(stats,indent=2,ensure_ascii=False))
    print('VALIDATION REQUIRED: review the taxi graph against MSFS airport scenery.')
    return 0

def main(argv=None):
    parser=argparse.ArgumentParser(description='RealFlow Traffic 0.1: MSFS 2020 traffic prototype')
    parser.add_argument('--config',default='config.json')
    sub=parser.add_subparsers(dest='command',required=True)
    p=sub.add_parser('doctor',help='Report dependencies and installed FSLTL')
    p.add_argument('--fsltl',default='')
    p.set_defaults(handler=doctor)
    p=sub.add_parser('scan',help='List available local FSLTL model titles')
    p.add_argument('--fsltl',default='');p.add_argument('--limit',type=int,default=30)
    p.set_defaults(handler=scan)
    p=sub.add_parser('demo',help='Offline airport/runway/live-source synthetic test with mock simulator')
    p.add_argument('--fsltl',default='');p.add_argument('--steps',type=int,default=240)
    p.add_argument('--output',default='demo-results.json')
    p.set_defaults(handler=offline_demo)
    p=sub.add_parser('convert-airport',help='Convert MSFS Scenery Editor source XML to a taxi graph')
    p.add_argument('--xml',required=True);p.add_argument('--icao',default=None)
    p.add_argument('--output',required=True);p.set_defaults(handler=convert_airport)
    p=sub.add_parser('run',help='Live OpenSky polling + mock or experimental SimConnect')
    p.add_argument('--fsltl',default='');p.add_argument('--bridge',choices=['mock','simconnect'],default='mock')
    p.add_argument('--allow-motion',action='store_true',help='Experimental AI position writes to SimConnect')
    p.add_argument('--lat',type=float,default=57.914);p.add_argument('--lon',type=float,default=56.02)
    p.add_argument('--alt-ft',type=float,default=10000)
    p.add_argument('--airport',help='Verified airport graph JSON file, no automatic extraction yet')
    p.add_argument('--metadata',help='Optional CSV with icao24,icao_type model metadata')
    p.add_argument('--atc-csv',help='Enable AI Ground/Tower ATC using local OurAirports frequencies CSV; requires --airport')
    p.add_argument('--atc-voice',action='store_true',help='Speak AI/controller chatter on the cockpit tuned transmitting COM radio')
    p.add_argument('--atc-monitor-frequency',type=float,default=None,help='Monitor a specific MHz frequency without a native cockpit (mock testing)')
    p.add_argument('--duration',type=float,default=0,help='Seconds to run, 0 = indefinite')
    p.add_argument('--follow-user',action='store_true',default=True)
    p.set_defaults(handler=run)
    args=parser.parse_args(argv)
    try:
        return args.handler(args) or 0
    except (ValueError,RuntimeError,OSError) as exc:
        print(f'ERROR: {exc}',file=sys.stderr)
        return 2

if __name__=='__main__':
    sys.exit(main())
