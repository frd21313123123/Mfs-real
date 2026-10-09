# RealFlow Integrated ATC: experimental prototype

**Status: automated offline ATC/AI tests, not a verified in-game release.** No VATSIM connection, no real controller or authoritative flight navigation.

## New in this branch

* AIFlightDirector issues synthetic AI aircraft heading, altitude, speed and approach clearances. Kinematics limit turns to 3 degrees/sec, climb and descent to about 1700-1900 ft/min and speed changes to 3 knots/sec.
* The shared RunwayController arbitrates synthetic arrivals, managed ground AI and player clearance. A busy runway triggers a go-around.
* Basic separation of 5.5 km horizontally and 1000 ft vertically; the player's observed aircraft has priority. Not a real separation assurance system.
* Departure/arrival ownership handoff between GroundEngine and AIFlightDirector, retaining the managed SimConnect object identity where possible. Arrival ground handoff requires verified geometry, runway-node proximity and an available gate.
* Ground/Tower/Approach/Center AI dialogues are emitted on available published COM frequencies and spoken through optional Windows TTS when tuned.
* PilotRadioConsole receives typed requests or optional local Vosk microphone commands on an input thread while aircraft keep moving. Player COM1/COM2 and XPDR come from experimental SimConnect read-only inputs.
* PilotFlightMonitor observes takeoff to release runway reservation, triggers a tower-to-departure handoff and may alert about previously stabilized altitude deviations.
* The LIVE ADS-B layer is still observer-only; it never receives synthetic flight commands.

## Fast verification without simulator

    py -3 -m pip install -e . pytest
    py -3 -m pytest -q
    py -3 -m realflow.atc_demo --steps 600 --output atc-demo-results.json

This uses fictional airport geometry, fictional frequencies and placeholder model titles with MockBridge only. CI runs the scenario on Windows and Linux using Python 3.11 and 3.13.

## Windows MSFS 2020 setup

1. Install FSLTL Traffic Base Models, but do not run a competing AI traffic injector.
2. Export scenery-editor SOURCE XML of the chosen airport to a JSON graph. Verify all taxi edges, runway tags, gates, elevations and scenery placement in-game. Compiled BGL extraction is not supported.
3. Download an optional OurAirports frequency directory (community maintained, NOT authoritative), and verify the needed frequencies against official published information:

    py -3 -m realflow.atc_cli update
    py -3 -m realflow.atc_cli frequencies --icao UUEE

4. Convert scenery editor XML, inspect the result and correct it before use:

    py -3 -m realflow convert-airport --xml "C:\airport-project\airport.xml" --icao UUEE --output "C:\realflow\UUEE_verified.json"

5. Install optional local/offline Vosk ASR and Windows speech:

    py -3 -m pip install vosk sounddevice pyttsx3

6. Start an experimental unified ATC plus traffic session:

    py -3 -m realflow run --bridge simconnect --fsltl "D:\MSFS\Community\fsltl-traffic-base" --airport "C:\realflow\UUEE_verified.json" --atc-csv "%USERPROFILE%\.realflow\airport-frequencies.csv" --atc-airborne --atc-voice --pilot-callsign AFL101 --pilot-destination ULLI --pilot-runway 24L --pilot-voice-model "C:\models\vosk-model-en-us" --allow-motion

The runway designator must map uniquely onto a graph runway tag. A mismatch causes an error rather than allowing divergent runway locks. The pilot controls the actual cockpit COM1/COM2 for voice frequency and the transponder for squawk. The program never sets the player's radios or flight controls.

ATC requests can be typed in the console; for microphone PTT press Enter on an empty console line to record roughly six seconds of audio. Vosk processing is local. Replies and tuned AI radio chatter use Windows audio, NOT the MSFS in-game audio bus. For text-only testing, omit --atc-voice and --pilot-voice-model.

## Limitations and safety

- The kinematics and approach paths are approximations without SIDs, STARs, actual ILS or terrain-clearance validation. No claim of operationally correct procedure design or realistic full airline performance.
- Native SimConnect AI object movement, cockpit radio variables, FSLTL animations and performance are not tested in an actual MSFS install. Keep position writes disabled unless testing explicitly.
- Live ADS-B positions, aircraft from other addons and VATSIM users are outside RealFlow's direct AI traffic control.
- Published frequency data can be stale or omit sector frequencies. ATC does not invent unknown real frequencies.
- The rule-based conversation is not human-quality VATSIM traffic and has no live VATSIM network integration.
- If no valid graph/gate exists, an arrived AI plane is removed instead of teleported to a fabricated parking stand.
