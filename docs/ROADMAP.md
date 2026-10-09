# RealFlow Traffic - Development status and blockers

## v0.1 prototype (delivered)

- [x] Python project + 25+ offline automated tests
- [x] Hybrid strategy: LIVE takes priority over SYNTHETIC
- [x] Dynamic Flight objects and bounded synthetic flight simulation
- [x] ICAO24 position observations from OpenSky client
- [x] FSLTL `aircraft.cfg` parsing and airline/type scoring
- [x] Airport graph routing from JSON
- [x] MSFS scenery source XML converter for taxiway points, parking, runway edges
- [x] Ground motion model with pushback, taxi speed and edge/junction locks
- [x] Simplified runway exclusivity
- [x] Mock simulator and interactive standalone map preview
- [x] Native SimConnect functions written (not executed against actual simulator)
- [x] Windows Actions EXE build recipe (not yet run)

## Gate A: Native SimConnect test (next critical work)

- [ ] Build on Windows and ensure `SimConnect.dll` is visible
- [ ] Validate native structure ABI, receive dispatch and exception decoding
- [ ] Create one FSLTL aircraft with exact `title` from installed package
- [ ] Read assigned object ID and remove aircraft without leftovers
- [ ] Confirm writing `Initial Position` works for a non-ATC FSLTL aircraft
- [ ] Measure visual smoothness, freeze/unfreeze simulation AI movement
- [ ] Verify engine animations and lights are controllable on FSLTL model
- [ ] Fix any simulator-specific errors, create Windows in-game integration test suite

Release cannot be marked as stable until this gate passes.

## Gate B: Correct ground geometry and physics

- [ ] Read actual installed airport facility data via SimConnect / scenery SDK, not only project XML
- [ ] Validate graph topology, closed taxiways, elevation and width/aircraft wingspan constraints
- [ ] Pushback paths and tug animation with native controls
- [ ] Cross-runway occupation conflict graph and runway intersection handling
- [ ] Avoid collisions with player and external AI aircraft
- [ ] Implement turn radius, jerk limitation, taxiway intersection priority and multi-aircraft deadlock resolution
- [ ] Use actual hold-short and displaced-threshold positions

## Gate C: Full flight lifecycle

- [ ] True self-contained performance model: V1, VR, takeoff acceleration and rotation
- [ ] Flight plan, SID/STAR, ILS/visual approach and go-around logic
- [ ] SimConnect animation events: flaps, landing gear, lights, speedbrakes, reverse
- [ ] Arrivals from runway to assigned gate without teleport
- [ ] Reconstruct route/airport from licensed schedule provider
- [ ] Ground ADS-B only where coverage adequate, else mark positions simulated

## Gate D: Reliability and real release

- [ ] FPS / memory telemetry to downscale AI density automatically
- [ ] Real traffic provider failover, token refresh and backoff stress tests
- [ ] Resilient object creation acknowledgement, fail/retry logic and teardown
- [ ] Simulator pause, time acceleration, weather and wind support
- [ ] Windows GUI actual live traffic controls + provider status + model list
- [ ] Signed Windows release, installer and upgrade path
- [ ] Full test matrix at default airports and 3rd-party scenery

## Known prototype limitations

1. The offline `AIRBORNE` phase currently means leaving the ground engine, **not** a complete dynamic takeoff injected into the simulator.
2. Native `SetDataOnSimObject` uses a high-level initial-position write. This needs verification for flight-model interaction and visuals.
3. Source airport XML is not compiled BGL; source is not always available. The converter produces a first approximation, not automatically a safe graph.
4. Current crossing/runway controller has no wind-aware runway selection or separation intervals.
5. AIS ground movement from live ADS-B is not linked to GroundEngine; that integration requires actual verified airport surface tracks and data.
6. Some installed FSLTL flight simulator packages might not be reachable through default paths and need explicit setup.
7. The prototype does not interact with built-in ATC or third-party ATC tools.
