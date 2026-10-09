"""Read cockpit COM1/COM2/transponder without writing radio values.

Experimental SimConnect subclass: an in-game Windows smoke test is REQUIRED.
This reads the selected COM transmit channel, not the transponder frequency
(because a transponder is not a voice radio).
"""
from __future__ import annotations

import ctypes as C
from dataclasses import dataclass

from .simconnect import SimConnectBridge, Recv, DWORD


@dataclass(frozen=True)
class CockpitRadio:
    com1_mhz: float | None
    com2_mhz: float | None
    transmitting: int
    squawk: str | None


class _RadioObjectData(C.Structure):
    _fields_ = [
        ("header", Recv),
        ("dwRequestID", DWORD), ("dwObjectID", DWORD),
        ("dwDefineID", DWORD), ("dwFlags", DWORD),
        ("dwentrynumber", DWORD), ("dwoutof", DWORD), ("dwDefineCount", DWORD),
        ("data", C.c_double * 5),
    ]


def _valid_mhz(raw: float) -> float | None:
    if 118.0 <= raw <= 136.99:
        return round(raw, 3)
    return None


def _decode_squawk(raw: float) -> str | None:
    # With SimConnect unit 'Number' MSFS returns the displayed digits for
    # TRANSPONDER CODE:1. BCO16 is a different representation.
    code = str(int(raw))
    return code.zfill(4) if 1 <= len(code) <= 4 and all(c in "01234567" for c in code) else None


class CockpitSimConnectBridge(SimConnectBridge):
    DEFINITION_RADIO = 3
    REQUEST_RADIO = 101

    def __init__(self, *args, **kwargs):
        self.player_radio: CockpitRadio | None = None
        super().__init__(*args, **kwargs)

    def connect(self):
        if self.connected:
            return
        super().connect()
        for name, unit in (
            (b"COM ACTIVE FREQUENCY:1", b"MHz"),
            (b"COM ACTIVE FREQUENCY:2", b"MHz"),
            (b"COM TRANSMIT:1", b"Bool"),
            (b"COM TRANSMIT:2", b"Bool"),
            (b"TRANSPONDER CODE:1", b"Number"),
        ):
            self._check(self.fn_adddef(self._handle, self.DEFINITION_RADIO, name, unit, 4, 0, -1),
                        "AddToDataDefinition radio")
        # Request radio updates once per second, not once per rendered frame.
        self._check(self.fn_req(self._handle, self.REQUEST_RADIO, self.DEFINITION_RADIO,
                                0, 4, 0, 0, 0, 0), "RequestData radio")

    def _dispatch(self, ptr, cb_data, context):
        hdr = ptr.contents
        if hdr.dwID == self.RECV_SIMOBJECT_DATA and cb_data >= C.sizeof(_RadioObjectData):
            msg = C.cast(ptr, C.POINTER(_RadioObjectData)).contents
            if msg.dwRequestID == self.REQUEST_RADIO:
                com1, com2, tx1, tx2, xpdr = msg.data
                transmitting = 2 if tx2 > 0.5 and tx1 <= 0.5 else 1
                self.player_radio = CockpitRadio(_valid_mhz(com1), _valid_mhz(com2),
                                                 transmitting, _decode_squawk(xpdr))
                return
        super()._dispatch(ptr, cb_data, context)
