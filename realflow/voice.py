"""Optional offline push-to-talk speech for simulated ATC.

Vosk models are user-supplied. No secret API keys, external audio upload or
VATSIM/network transmission. Speech quality/recognition requires local testing.
"""
from __future__ import annotations

import json
from pathlib import Path


class VoiceUnavailable(RuntimeError):
    pass


class OfflineVoice:
    def __init__(self, model_dir: str, voice_rate: int = 16000):
        if not Path(model_dir).is_dir():
            raise VoiceUnavailable("Install a Vosk recognition model and provide --vosk-model")
        try:
            import vosk
            import sounddevice
            import pyttsx3
        except ImportError as exc:
            raise VoiceUnavailable("Install voice dependencies: pip install vosk sounddevice pyttsx3") from exc
        if voice_rate != 16000:
            raise VoiceUnavailable("This MVP expects a 16 kHz Vosk model")
        self.vosk = vosk
        self.sd = sounddevice
        self.model = vosk.Model(model_dir)
        self.engine = pyttsx3.init()
        self.engine.setProperty("rate", 165)
        self.rate = voice_rate

    def listen(self, seconds: float = 6.0) -> str:
        """Hold-to-talk style: pilot presses Enter, then records one phrase."""
        if not 1 <= seconds <= 20:
            raise ValueError("Recording length must be 1-20 seconds")
        try:
            samples = self.sd.rec(int(seconds * self.rate), samplerate=self.rate,
                                  channels=1, dtype="int16")
            self.sd.wait()
        except Exception as exc:
            raise VoiceUnavailable("Microphone failed or is unavailable") from exc
        rec = self.vosk.KaldiRecognizer(self.model, self.rate)
        rec.AcceptWaveform(samples.tobytes())
        return json.loads(rec.FinalResult()).get("text", "").strip()

    def speak(self, text: str):
        if not text.strip():
            return
        self.engine.say(text)
        self.engine.runAndWait()


class RadioSpeaker:
    """Non-blocking, opt-in local TTS for AI radio messages.

    Speech work stays off the SimConnect/taxi movement thread. Radio monitoring
    filtering is performed by the caller *before* queueing a message.
    """
    def __init__(self):
        from queue import Queue
        from threading import Thread
        try:
            import pyttsx3  # noqa: F401
        except ImportError as exc:
            raise VoiceUnavailable("Install pyttsx3 for AI voice: pip install pyttsx3") from exc
        self.queue = Queue(maxsize=64)
        self._closed = False
        self._worker = Thread(target=self._play, name="realflow-atc-tts", daemon=True)
        self._worker.start()

    def _play(self):
        import pyttsx3
        try:
            engine = pyttsx3.init()
            engine.setProperty("rate", 166)
        except Exception:
            return
        while True:
            message = self.queue.get()
            if message is None:
                return
            try:
                engine.say(message)
                engine.runAndWait()
            except Exception:
                # An unavailable audio device should not interrupt traffic.
                continue

    def speak(self, message: str):
        from queue import Full
        if self._closed or not message:
            return
        try:
            self.queue.put_nowait(message)
        except Full:
            # Avoid unbounded chatter backlogs and simulator lag.
            return

    def close(self):
        from queue import Full
        if self._closed:
            return
        self._closed = True
        try:
            self.queue.put_nowait(None)
        except Full:
            # The daemon thread will be reclaimed at process exit.
            pass
