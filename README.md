# RealFlow Traffic 0.1.0

[![CI](https://github.com/frd21313123123/Mfs-real/actions/workflows/ci.yml/badge.svg)](https://github.com/frd21313123123/Mfs-real/actions/workflows/ci.yml) [![Windows prototype](https://github.com/frd21313123123/Mfs-real/actions/workflows/windows-exe.yml/badge.svg)](https://github.com/frd21313123123/Mfs-real/actions/workflows/windows-exe.yml)

**MSFS 2020 traffic research prototype: Hybrid ADS-B + synthetic traffic + local FSLTL aircraft models.**

> **CURRENT STATUS: ENGINE PROTOTYPE, NOT A STABLE MSFS PLUGIN.**
> All automated tests have run in the offline mock, but **native SimConnect integration, positioning of FSLTL aircraft, taxi animations and airport motion have NOT been tested in MSFS 2020**. Do not call this a working game release yet.

## What's actually implemented

| Feature | Status |
|---|---|
| FSLTL `aircraft.cfg` scan (titles, ICAO type and airline) | Implemented and offline-tested |
| Detect standard Community folders and read `InstalledPackagesPath` | Implemented, Windows verification pending |
| Flight model matching, optional ICAO24 aircraft-type CSV | Implemented and offline-tested |
| OpenSky ADS-B state query with OAuth2 and basic rate-limit protection | Implemented, live network test pending |
| LIVE and PREDICTED track updates; distance/age filters | Implemented and offline-tested |
| Synthetic airborne flights, deterministic demo | Implemented and offline-tested |
| Hybrid prioritization LIVE over synthetic | Implemented and offline-tested |
| Max airborne 35, max ground 30 | Implemented and offline-tested |
| Airport graph JSON, shortest-path routing | Implemented and offline-tested |
| MSFS Scenery Editor source XML -> taxi graph converter | Implemented and offline-tested |
| Pushback / taxi kinematics, edge and junction locks | Implemented and offline-tested |
| Runway exclusivity for owned traffic | Implemented and offline-tested |
| Offline Tkinter control panel and taxiway map | Implemented; GUI smoke test requires a desktop |
| Native SimConnect connect / create / move / remove via Windows ctypes | Written but **not game-tested** |
| Clean shutdown for owned aircraft | Tested in mock; native test pending |
| Collision prediction for all other AI, player aircraft | Not implemented |
| Pull airport taxi geometry directly from compiled BGL or SimConnect facilities | Not implemented |
| Full animated flight phases, true takeoff and landing in game | Not implemented |
| Complete gate-to-gate timetable and dispatch | Not implemented |
| A true Windows installer, signed release | Not implemented |

These distinctions matter. A working offline engine is not proof that the game's simobject physics / FSLTL animations will behave correctly under frequent position writes.

## Requirements

- Windows 10/11 (64-bit), MSFS 2020, **FSLTL Traffic Base Models** installed via FlyByWire installer.
- Python 3.10 or newer, with Tkinter (included with standard Windows Python distribution).
- `SimConnect.dll` accessible through the Windows DLL search path (installed with a legitimate simulator runtime / SDK; **not bundled** here).
- Access to the OpenSky API for online mode. Optional OAuth2 API client credentials.

### Run without MSFS (recommended first)

Extract the archive, open a terminal in the extracted `RealFlowTraffic` directory, run:

```powershell
py -3 -m realflow doctor
py -3 -m realflow demo --steps 600 --output demo-results.json
py -3 -m realflow.ui
```

Or double-click `run-demo.bat` / `start-ui.bat`. For an experimental real-simulator connection, use `start-msfs-experimental.bat` after reading the safety notes below.

The demo runs without access to any real MSFS files and uses **dummy aircraft model titles exclusively within MockBridge**. It never attempts to load the dummy titles into MSFS. The map in the preview is **fictional**.

### Detect local FSLTL models

```powershell
py -3 -m realflow doctor
py -3 -m realflow scan --limit 40
```

Specify path manually when needed:

```powershell
py -3 -m realflow scan --fsltl "D:\\MSFS\\Community\\fsltl-traffic-base"
```

The scanner does not edit, copy or redistribute any installed FSLTL asset. It finds `aircraft.cfg` files and reads `[FLTSIM.N]` titles.

### Use OpenSky real flight positions

1. Optionally create an OpenSky OAuth2 API client in your own OpenSky account.
2. Set `OPENSKY_CLIENT_ID` and `OPENSKY_CLIENT_SECRET` in environment variables. Avoid storing credentials in the repository.
3. To **test the tracking without touching MSFS**, start:

```powershell
py -3 -m realflow run --bridge mock --lat 57.914 --lon 56.02 --duration 300
```

This needs installed FSLTL models for model matching. Anonymous access may work under reduced API limits. The client defaults to a minimum 120-second polling interval and handles common 401/429 errors. OpenSky doesn't guarantee flight destination or ICAO aircraft type in its `states/all` data.

To improve type matching, create an aircraft database CSV:

```csv
icao24,icao_type
abcdef,A320
123abc,B738
```

Then add `--metadata "C:\\flightdata\\aircraft.csv"` to the run command. Exact ICAO24 IDs and types must come from a legitimate metadata source.

### Convert airport editor XML to taxi graph

```powershell
py -3 -m realflow convert-airport --xml "C:\\AirportProject\\airport.xml" --icao YOUR --output airport.json
```

The converter accepts source XML containing `<Airport>`, `<TaxiwayPoint>`, `<TaxiwayParking>` and `<TaxiwayPath>` as described in MSFS 2020 scenery SDK documentation. XML source **is not the same as compiled BGL**. You must compare the resulting graph to the airport in the simulator before using it; XML import alone does not verify safety, elevations, obstacle clearance or runway usage. Some projects can require edits to the generated JSON.

### Experimental connection to MSFS 2020

**Use this only after checking the SimConnect SDK and with safe scenarios.**

1. Start the game and begin a flight.
2. Install FSLTL Base Models. Disable overlapping injected AI traffic, simulator AI traffic and ground density in simulator settings.
3. Ensure the native `SimConnect.dll` from a legitimate source can be loaded.
4. Run an injection trial (position writes off by default):

```powershell
py -3 -m realflow run --bridge simconnect --fsltl "D:\\MSFS\\Community\\fsltl-traffic-base" --duration 60
```

To **experiment** with applying object positions in real time, add `--allow-motion`. This is **not** a verified or stable integration. It may cause nonphysical movement or visible snaps. Test on a copy of your MSFS configuration, preferably with only one or two AI aircraft.

For ground taxiing, add `--airport YOUR_VERIFIED_AIRPORT.json --allow-motion`. The bundled `examples/demo_airport.json` is refused in native game mode because its geometry is fictional.

### Default user configuration (`config.json`)

```json
{
  "mode": "hybrid",
  "airborne_limit": 35,
  "ground_limit": 30,
  "realistic_taxi": true,
  "runway_control": true,
  "sync_real_flights": true,
  "fetch_seconds": 120,
  "max_radius_km": 120.0,
  "fps_target": 35,
  "fsltl_path": "",
  "airport_graph": "",
  "enable_simconnect_position_writes": false
}
```

The source is built with **the exact requested 35/30 Hybrid configuration**. Some settings, such as `fps_target` and the boolean feature switches (currently always-on in the prototype engines), are persisted but not yet hooked to an in-simulator performance monitor. Position writes can only be enabled with the experimental CLI flag at present.

## Unit tests

```powershell
py -3 -m pip install pytest
py -3 -m pytest -q
```

Tests cover model parsing, model matching, aircraft metadata enrichment, OpenSky response mapping, airway dead-reckoning, limit enforcement, pathfinding, parking/taxi/runway transitions, converging taxiways, occupied runways, airport XML import, simulated bridge cleanup and native ABI structure sizes. An MSFS-in-the-loop integration test is still needed.

## Windows executable build

The [Windows prototype EXE workflow](https://github.com/frd21313123123/Mfs-real/actions/workflows/windows-exe.yml) runs automatically when `main` changes and can also be started manually from the **Actions** tab. After a successful run, download the `RealFlowTraffic-Windows-prototype-*` artifact containing `RealFlowTraffic.exe` and demo results. Its first GitHub-hosted Windows build succeeded on 2026-10-09. This EXE is an **experimental offline UI prototype**, not a verified live-MSFS release. SimConnect binaries and FSLTL models are not bundled.

## Architecture

```text
OpenSky API / local metadata
         |
    live.py + fleet.py --------+------- synthetic.py
                               |
                        manager.py  (Hybrid)
                               |
                   +-----------+-----------+
                   |                       |
              ground.py                bridge.py
                   |                       |
             airport.py              simconnect.py
                   |                  (Windows native)
           runway.py / XML converter         |
                   +-----------------------> MSFS 2020

fsltl.py -> ModelMatcher -> correct local aircraft.cfg 'title'
```

For planned flight dynamics, weather-driven runway selection, runway crossings, native airport geometry, passenger gates and true ATC integration, see `docs/ROADMAP.md`.

## References and rights

- MSFS 2020 SimConnect: https://docs.flightsimulator.com/html/Programming_Tools/SimConnect/SimConnect_API_Reference.htm
- `AICreateNonATCAircraft`: https://docs.flightsimulator.com/html/Programming_Tools/SimConnect/API_Reference/AI_Object/SimConnect_AICreateNonATCAircraft.htm
- `SetDataOnSimObject`: https://docs.flightsimulator.com/html/Programming_Tools/SimConnect/API_Reference/Events_And_Data/SimConnect_SetDataOnSimObject.htm
- MSFS taxiway XML: https://docs.flightsimulator.com/html/Content_Configuration/Environment/Airports_And_Facilities/Taxiway_Definition_Properties.htm
- OpenSky REST API and OAuth2: https://github.com/openskynetwork/opensky-api/blob/master/docs/free/rest.rst
- FSLTL base models: https://github.com/FSLiveTrafficLiveries/base

This repository contains independently written prototype code only. No Microsoft SimConnect runtime, FSLTL aircraft models, liveries or proprietary airport scenery are redistributed. Use OpenSky according to its API terms and access credits.


## Experimental ATC: voice, cockpit COM and RealFlow AI

This ATC module provides a local, rule-based IFR radio simulation. **It does not connect to VATSIM or to any real controller**, and it is not a real-world aviation navigation aid.

### Real-world frequency database

Install the optional community-maintained OurAirports frequency list:

    py -3 -m realflow.atc_cli update
    py -3 -m realflow.atc_cli frequencies --icao UUEE

The downloaded file is stored in the current user's home directory under .realflow/airport-frequencies.csv. It is downloaded only on explicit request. Published frequencies are community records, not guaranteed current ATC services. Verify them against official AIP/NOTAM and airport charts. Missing frequencies are NOT fabricated, and airport sectors are not selected automatically.

### Cockpit radios, not the transponder

Use COM1/COM2 for voice communication. Use the transponder for the **squawk code only**. COM1 and COM2 active frequencies and selected transmitting radio, plus XPDR code, are read through experimental MSFS 2020 SimConnect variables.

Start MSFS 2020, then run this ATC-only console:

    py -3 -m realflow.atc_cli session --icao UUEE --destination ULLI --callsign AFL101 --runway 24L --cockpit

Tune the station's frequency using the actual cockpit COM panel. Select COM1 or COM2 transmit on the aircraft audio panel. Request clearance, read back assigned altitude and squawk, enter squawk on the physical cockpit transponder, then switch frequencies on the COM panel after handoff. These are **simulated** clearances.

For a typed, offline example without MSFS, replace --cockpit with --com1 121.900, using a published station frequency for the selected airport.

### Real microphone voice, optional, with no paid APIs

Install voice dependencies, and separately download a 16kHz English Vosk recognition model:

    py -3 -m pip install vosk sounddevice pyttsx3
    py -3 -m realflow.atc_cli session --icao UUEE --destination ULLI --callsign AFL101 --runway 24L --cockpit --voice --vosk-model "C:\models\vosk-model-en-us"

Press Enter in the console to record approximately six seconds from the actual microphone (one push-to-talk message). Vosk performs offline speech recognition; pyttsx3 uses the Windows speaker/audio device for dispatcher speech. Pilot transmissions do not leave the PC. The recognition model and MSFS are not included in the project. Speech and voice quality must be validated on Windows.

### AI pilots and ATC are linked to the Ground Engine

RealFlow-owned AI ground traffic automatically exchanges request, clearance and readback messages before taxiway/runway operations. A shared RunwayController prevents issuing a conflicting runway operation to another managed plane. Missing controller frequencies do not silently turn into fictitious stations.

Offline dialogue test (fictional airport graph, not MSFS movement):

    py -3 -m realflow.atc_cli session --icao UUEE --destination ULLI --callsign AFL101 --runway 24L --ai-demo

Use /tick 120 and /status to observe AI conversations and radio state. AI chatter is shown only when tuned to the corresponding frequency. The fictional airport geometry is never used for live game movement.

Experimental live integration, using FSLTL and a VERIFIED custom airport graph:

    py -3 -m realflow run --bridge simconnect --airport "C:\realflow\UUEE_verified.json" --atc-csv "C:\realflow\airport-frequencies.csv" --allow-motion

**Known restrictions:** COM frequency readings, BCO16/transponder values, third-party cockpit audio panels, taxi/landing animations and native SimConnect controls need in-game verification. Bots currently comply with RealFlow ground/runway permissions, but synthetic aircraft in flight do not yet comply with ATC altitude, heading or spacing instructions. A separate ATC voice console is used; the in-sim ATC radio audio channel is not directly replaced. Real ADS-B aircraft are not controllable and do not actually receive clearances. This remains an experimental prototype, not a stable game release.
