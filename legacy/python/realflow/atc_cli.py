"""Command-line ATC session, radio directory and optional offline voice demo.

Usage: python -m realflow.atc_cli update
       python -m realflow.atc_cli frequencies --icao UUEE
       python -m realflow.atc_cli session --icao UUEE --destination ULLI
              --callsign AFL101 --runway 24L [--cockpit] [--voice --vosk-model ...]
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

from .frequencies import FrequencyDirectory, download_ourairports
from .atc import PilotATC, AIAirportController
from .runway import RunwayController

DEFAULT_CSV = str(Path.home() / ".realflow" / "airport-frequencies.csv")


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description="RealFlow ATC: offline VATSIM-style simulation")
    parser.add_argument("--csv", default=DEFAULT_CSV, help="OurAirports local frequency CSV")
    sub = parser.add_subparsers(dest="cmd", required=True)
    sub.add_parser("update", help="Download published frequencies (manual, on demand)")
    frequencies = sub.add_parser("frequencies", help="List available stations for an airport")
    frequencies.add_argument("--icao", required=True)
    session = sub.add_parser("session", help="Local simulated ATC radio session")
    session.add_argument("--icao", required=True)
    session.add_argument("--destination", required=True)
    session.add_argument("--callsign", required=True)
    session.add_argument("--runway", required=True)
    session.add_argument("--com1", type=float, help="Mock radio frequency for offline use")
    session.add_argument("--cockpit", action="store_true", help="Read real COM1/COM2/XPDR from MSFS")
    session.add_argument("--voice", action="store_true", help="Optional Vosk push-to-talk and TTS")
    session.add_argument("--vosk-model", default="", help="Locally installed 16 kHz Vosk model")
    session.add_argument("--ai-demo", action="store_true",
                         help="Create two offline fictional ground aircraft with AI radio")
    args = parser.parse_args(argv)
    try:
        if args.cmd == "update":
            count = download_ourairports(args.csv)
            print(f"Saved {count} community frequency records to {args.csv}. NOT verified operational data.")
            return 0
        directory = FrequencyDirectory.load(args.csv)
        if args.cmd == "frequencies":
            stations = directory.lookup(args.icao)
            if not stations:
                print(f"No published frequencies found for {args.icao}; no fallback invented.")
                return 1
            for f in stations:
                print(f"{f.airport}: {f.station:10s} {f.mhz:7.3f}  {f.description}")
            print("Source: OurAirports community data; verify using official AIP/airport charts.")
            return 0
        return run_session(args, directory)
    except (ValueError, RuntimeError, OSError) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 2


def run_session(args, directory: FrequencyDirectory) -> int:
    stations = directory.primary(args.icao)
    if not stations:
        raise ValueError("No real published frequencies available for this airport. Use official data.")
    runways = RunwayController()
    pilot = PilotATC(args.callsign, args.icao, args.destination,
                     stations, args.runway, runways=runways)
    voice = None
    if args.voice:
        from .voice import OfflineVoice
        if not args.vosk_model:
            raise ValueError("Voice mode needs --vosk-model /path/to/vosk-model")
        voice = OfflineVoice(args.vosk_model)
    bridge = None
    ground = None
    ai = None
    cursor = 0
    if args.cockpit:
        from .cockpit_radio import CockpitSimConnectBridge
        bridge = CockpitSimConnectBridge(enable_position_writes=False)
        bridge.connect()
    else:
        frequency = args.com1 if args.com1 else next(iter(stations.values()))
        pilot.update_cockpit(frequency)

    if args.ai_demo:
        from .airport import AirportGraph
        from .ground import GroundEngine
        # ONLY a mock graph; never instantiate fictional models in native MSFS.
        graph = AirportGraph.from_json(Path(__file__).resolve().parent.parent / "examples/demo_airport.json")
        ground = GroundEngine(graph, runways=runways, max_ground=2)
        ai = AIAirportController(stations, runways)
        ground.clearance_provider = ai.allow_edge
        ground.add("SIM101", "OFFLINE ONLY", "GATE_A", "RUNWAY_EXIT")
        ground.add("SIM102", "OFFLINE ONLY", "GATE_B", "RUNWAY_EXIT")
        print("AI demonstration uses fictional taxi geometry. NOT controlled in MSFS.")

    print("RealFlow LOCAL ATC, not live VATSIM. Published frequencies may be outdated.")
    print("Available:", ", ".join(f"{s} {f:.3f}" for s, f in stations.items()))
    print("Commands: /status, /tune 121.900 (offline), /tick 10 (AI only), /quit.")
    if voice:
        print("Press Enter to record one ~6-second radio message.")
    try:
        while True:
            if bridge:
                bridge.poll()
                if bridge.player_radio is not None:
                    radio = bridge.player_radio
                    pilot.update_cockpit(radio.com1_mhz, radio.com2_mhz,
                                         radio.transmitting, radio.squawk)
            prompt = f"[{pilot.stage} {pilot.station or 'NO ATC'} {pilot.tuned or 0:.3f}] > "
            try:
                raw = input(prompt).strip()
            except EOFError:
                break
            if raw in ("/quit", "/exit"):
                break
            if raw == "/status":
                print(f"COM1 {pilot.com1}, COM2 {pilot.com2}, transmitting COM{pilot.transmitting}, "
                      f"squawk {pilot.actual_squawk or 'unknown'}, ATC stage {pilot.stage}")
                continue
            if raw.startswith("/tune "):
                if bridge:
                    print("Change COM frequencies using your AIRCRAFT RADIO, not this command.")
                else:
                    pilot.update_cockpit(float(raw.split()[1]), pilot.com2, pilot.transmitting)
                continue
            if raw.startswith("/tick ") and ground:
                seconds = min(300, max(0, int(raw.split()[1])))
                for _ in range(seconds):
                    ground.tick(1)
                new_lines = ai.lines[cursor:]
                cursor = len(ai.lines)
                for line in new_lines:
                    if line.mhz is not None and pilot.tuned is not None and abs(line.mhz-pilot.tuned) <= 0.001:
                        print(f"{line.speaker}: {line.message}")
                        if voice:
                            voice.speak(line.message)
                continue
            if raw.startswith("/"):
                print("Unknown command")
                continue
            if voice and not raw:
                print("PTT recording...")
                raw = voice.listen()
                print("Recognized:", raw or "(no speech)")
            if not raw:
                continue
            reply = pilot.transmit(raw)
            print(f"{reply.speaker}: {reply.message}")
            if voice and reply.mhz is not None and pilot.tuned is not None and abs(reply.mhz-pilot.tuned) <= 0.001:
                voice.speak(reply.message)
    finally:
        pilot.close()
        if bridge:
            bridge.close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
