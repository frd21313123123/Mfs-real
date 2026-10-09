import ctypes
from realflow.simconnect import InitPosition,AssignedID,SimobjectData

def test_ctypes_abi_layout():
    assert ctypes.sizeof(InitPosition)==56
    assert ctypes.sizeof(AssignedID)==20
    assert ctypes.sizeof(SimobjectData)==64
