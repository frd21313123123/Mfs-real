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
