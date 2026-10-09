"""Player radio service in the same simulation loop as RealFlow AI traffic.

Console/microphone input is collected in a daemon thread; SimConnect and
traffic run in the main thread. Commands never write to the player's aircraft.
"""
from __future__ import annotations

import queue
import threading
import time
from .atc import PilotATC, RadioLine
from .geo import Position


class PlayerFlightMonitor:
    """Observes a *human* flight and makes non-authoritative ATC advisories."""
    def __init__(self, pilot: PilotATC, departure_field_alt_ft: float):
        self.pilot = pilot
        self.field_alt_ft = departure_field_alt_ft
        self.captured_altitude: float | None = None
        self.deviation_started_at: float | None = None
        self.last_warning_at = -1e9

    def tick(self, position: Position | None, now: float) -> RadioLine | None:
        if position is None:
            return None
        p = self.pilot
        if (p.stage == "departure" and not position.on_ground and
                position.alt_ft > self.field_alt_ft + 350):
            # Player has actually left the runway (observed, not guessed).
            p._release_runway()
            p.stage = "airborne"
            return p._handoff("tower", "departure")
        target = None
        if p.stage == "cruise":
            target = 30000.0
        if p.stage == "approach":
            target = float(p.altitude_ft)
        if target is None:
            self.captured_altitude = None
            self.deviation_started_at = None
            return None
        # Never reprimand a pilot for climbing/descent still in progress.
        if abs(position.alt_ft-target) < 300:
            self.captured_altitude = target
            self.deviation_started_at = None
            return None
        if self.captured_altitude != target:
            return None
        if abs(position.alt_ft-target) < 550:
            self.deviation_started_at = None
            return None
        if self.deviation_started_at is None:
            self.deviation_started_at = now
            return None
        if now - self.deviation_started_at >= 20 and now - self.last_warning_at >= 90:
            self.last_warning_at = now
            station = p.station or "center"
            if station not in p.frequencies:
                return None
            return p._say(station,
                    f"check altitude, maintain {round(target):d} feet.", "warning")
        return None


class PilotRadioConsole:
    """Nonblocking push-to-talk service suitable for an active traffic loop.

    In text mode, each Enter-submitted line is one radio transmission. In
    optional Vosk mode, pressing Enter on an empty line records six seconds.
    """
    def __init__(self, pilot: PilotATC, field_alt_ft: float,
                 voice_model: str = "", offline_com1: float | None = None):
        self.pilot = pilot
        self.monitor = PlayerFlightMonitor(pilot, field_alt_ft)
        self.offline_com1 = offline_com1
        self.queue: queue.Queue[str] = queue.Queue(maxsize=64)
        self._closed = False
        self._thread = None
        self.voice_model = voice_model
        self._voice = None
        if offline_com1 is not None:
            pilot.update_cockpit(offline_com1)

    def start(self):
        if self._thread is not None:
            return
        if self.voice_model:
            from .voice import OfflineVoice
            self._voice = OfflineVoice(self.voice_model)
        self._thread = threading.Thread(target=self._input, name="realflow-player-ptt", daemon=True)
        self._thread.start()

    def _input(self):
        print("PLAYER ATC: type a radio request and Enter. Empty Enter records microphone (if enabled).")
        while not self._closed:
            try:
                text = input("PLAYER ATC > ").strip()
                if not text and self._voice:
                    print("PTT: speak for 6 seconds...")
                    text = self._voice.listen().strip()
                    print("PTT heard:", text or "(silence)")
                if text:
                    try:
                        self.queue.put_nowait(text)
                    except queue.Full:
                        print("ATC radio busy, message dropped.")
            except EOFError:
                return
            except Exception as exc:
                print("ATC microphone/console issue:", exc)
                return

    def submit(self, text: str) -> bool:
        """Programmatic/GUI integration hook; safe for unit tests."""
        if self._closed or not text.strip():
            return False
        try:
            self.queue.put_nowait(text.strip())
            return True
        except queue.Full:
            return False

    def poll(self, radio=None, player_position: Position | None = None,
             now: float | None = None) -> list[RadioLine]:
        if radio is not None:
            self.pilot.update_cockpit(radio.com1_mhz, radio.com2_mhz,
                                      radio.transmitting, radio.squawk)
        elif self.offline_com1 is not None:
            self.pilot.update_cockpit(self.offline_com1)
        events = []
        now = time.monotonic() if now is None else now
        event = self.monitor.tick(player_position, now)
        if event is not None:
            events.append(event)
        for _ in range(16):  # bounded; no blocking in SimConnect loop
            try:
                message = self.queue.get_nowait()
            except queue.Empty:
                break
            if message == "/status":
                print("Player radio:", self.pilot.stage,
                      "COM:", self.pilot.tuned, "squawk:", self.pilot.actual_squawk)
                continue
            if message.startswith("/tune ") and radio is None:
                try:
                    mhz = float(message.split()[1])
                except (ValueError, IndexError):
                    print("Usage: /tune <MHz>")
                    continue
                self.offline_com1 = mhz
                self.pilot.update_cockpit(mhz)
                continue
            if message.startswith("/"):
                print("Unknown ATC console command:", message)
                continue
            events.append(self.pilot.transmit(message))
        return events

    def close(self):
        self._closed = True
        self.pilot.close()
