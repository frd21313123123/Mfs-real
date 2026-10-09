"""Experimental native Windows SimConnect bridge.

Implements official SimConnect ABI using ctypes, without bundling any Microsoft SDK DLL.
Requires a real Windows MSFS2020 test before marking bridge as stable.
Writes are opt-in: enable_position_writes defaults to False.
"""
import ctypes as C
import os
from .bridge import BridgeError
from .geo import Position

DWORD=C.c_uint32
HANDLE=C.c_void_p

class InitPosition(C.Structure):
    _fields_=[('Latitude',C.c_double),('Longitude',C.c_double),('Altitude',C.c_double),
              ('Pitch',C.c_double),('Bank',C.c_double),('Heading',C.c_double),
              ('OnGround',DWORD),('Airspeed',DWORD)]

class Recv(C.Structure):
    _fields_=[('dwSize',DWORD),('dwVersion',DWORD),('dwID',DWORD)]

class AssignedID(C.Structure):
    _fields_=[('header',Recv),('dwRequestID',DWORD),('dwObjectID',DWORD)]

class SimobjectData(C.Structure):
    _fields_=[('header',Recv),('dwRequestID',DWORD),('dwObjectID',DWORD),
              ('dwDefineID',DWORD),('dwFlags',DWORD),('dwentrynumber',DWORD),
              ('dwoutof',DWORD),('dwDefineCount',DWORD),('data',C.c_double*3)]


class SimConnectBridge:
    RECV_QUIT=3
    RECV_EXCEPTION=1
    RECV_SIMOBJECT_DATA=8
    RECV_ASSIGNED=12
    DEFINITION_POSITION=1
    DEFINITION_PLAYER=2
    REQUEST_PLAYER=100

    def __init__(self,enable_position_writes:bool=False,dll_name:str='SimConnect.dll'):
        if os.name!='nt':
            raise BridgeError('SimConnect bridge is available only on Windows')
        self.enable_position_writes=enable_position_writes
        try: self.dll=C.WinDLL(dll_name)
        except OSError as e: raise BridgeError('SimConnect.dll not found; install MSFS2020 SDK/runtime') from e
        self.connected=False
        self._handle=HANDLE()
        self._req=1000
        self._request_to_key={}
        self._key_to_object={}
        self._pending_positions={}
        self._errors=[]
        self.player_position=None
        self._configure()
        self._callback_type=C.WINFUNCTYPE(None,C.POINTER(Recv),DWORD,C.c_void_p)
        self._callback=self._callback_type(self._dispatch)

    def _f(self,name,argtypes):
        func=getattr(self.dll,'SimConnect_'+name)
        func.argtypes=argtypes
        func.restype=C.c_int32
        return func

    def _configure(self):
        self.fn_open=self._f('Open',[C.POINTER(HANDLE),C.c_char_p,C.c_void_p,DWORD,HANDLE,DWORD])
        self.fn_close=self._f('Close',[HANDLE])
        self.fn_call=self._f('CallDispatch',[HANDLE,C.c_void_p,C.c_void_p])
        self.fn_create=self._f('AICreateNonATCAircraft',[HANDLE,C.c_char_p,C.c_char_p,InitPosition,DWORD])
        self.fn_remove=self._f('AIRemoveObject',[HANDLE,DWORD,DWORD])
        self.fn_adddef=self._f('AddToDataDefinition',[HANDLE,DWORD,C.c_char_p,C.c_char_p,DWORD,C.c_float,DWORD])
        self.fn_set=self._f('SetDataOnSimObject',[HANDLE,DWORD,DWORD,DWORD,DWORD,DWORD,C.c_void_p])
        self.fn_req=self._f('RequestDataOnSimObject',[HANDLE,DWORD,DWORD,DWORD,DWORD,DWORD,DWORD,DWORD,DWORD])

    def _check(self,result,op):
        if result<0: raise BridgeError(f'SimConnect {op} failed: HRESULT {result:#010x}')

    def _new_req(self):
        self._req+=1
        return self._req

    @staticmethod
    def _init(pos:Position):
        return InitPosition(pos.lat,pos.lon,max(-1000,pos.alt_ft),pos.pitch,pos.bank,pos.heading,
                            1 if pos.on_ground else 0,max(0,int(pos.speed_kt)))

    def connect(self):
        if self.connected: return
        self._check(self.fn_open(C.byref(self._handle),b'RealFlow Traffic',None,0,None,0),'Open')
        self.connected=True
        # POSITION writes utilize the official "Initial Position" struct, dataype 12.
        self._check(self.fn_adddef(self._handle,self.DEFINITION_POSITION,b'Initial Position',None,12,0,-1),'AddToDataDefinition position')
        for variable in (b'PLANE LATITUDE',b'PLANE LONGITUDE',b'PLANE ALTITUDE'):
            unit=b'feet' if variable==b'PLANE ALTITUDE' else b'degrees'
            self._check(self.fn_adddef(self._handle,self.DEFINITION_PLAYER,variable,unit,4,0,-1),'AddToDataDefinition player')
        self._check(self.fn_req(self._handle,self.REQUEST_PLAYER,self.DEFINITION_PLAYER,0,4,0,0,0,0),'RequestData player')

    def _dispatch(self,ptr,cb_data,context):
        hdr=ptr.contents
        if hdr.dwID==self.RECV_ASSIGNED and cb_data>=C.sizeof(AssignedID):
            msg=C.cast(ptr,C.POINTER(AssignedID)).contents
            key=self._request_to_key.pop(msg.dwRequestID,None)
            if key and key.startswith('__REMOVED__'):
                if self.connected:
                    self._check(self.fn_remove(self._handle,msg.dwObjectID,self._new_req()),'AIRemoveObject orphan')
                return
            if key:
                self._key_to_object[key]=msg.dwObjectID
                if self.enable_position_writes and key in self._pending_positions:
                    self.update(key,self._pending_positions[key])
        elif hdr.dwID==self.RECV_SIMOBJECT_DATA and cb_data>=C.sizeof(SimobjectData):
            msg=C.cast(ptr,C.POINTER(SimobjectData)).contents
            if msg.dwRequestID==self.REQUEST_PLAYER:
                self.player_position=Position(float(msg.data[0]),float(msg.data[1]),float(msg.data[2]))
        elif hdr.dwID==self.RECV_QUIT:
            self.connected=False
        elif hdr.dwID==self.RECV_EXCEPTION:
            self._errors.append('Received SimConnect exception (check SDK output)')

    def poll(self):
        if self.connected:
            self._check(self.fn_call(self._handle,self._callback,None),'CallDispatch')

    def create(self,key:str,title:str,position:Position):
        if key in self._key_to_object or key in self._request_to_key.values(): return
        if not self.connected: raise BridgeError('Not connected')
        req=self._new_req()
        self._request_to_key[req]=key
        self._pending_positions[key]=position
        try:
            self._check(self.fn_create(self._handle,title.encode('utf-8'),key[-12:].encode('ascii',errors='ignore'),self._init(position),req),'AICreateNonATCAircraft')
        except Exception:
            del self._request_to_key[req]
            self._pending_positions.pop(key,None)
            raise

    def update(self,key:str,position:Position):
        self._pending_positions[key]=position
        if not self.connected or not self.enable_position_writes:return
        object_id=self._key_to_object.get(key)
        if object_id is None:return
        pos=self._init(position)
        self._check(self.fn_set(self._handle,self.DEFINITION_POSITION,object_id,0,0,C.sizeof(pos),C.byref(pos)),'SetDataOnSimObject')

    def remove(self,key:str):
        self._pending_positions.pop(key,None)
        # Pending AI creation cannot be cancelled directly; clean it when its ID arrives.
        for req,k in list(self._request_to_key.items()):
            if k==key: self._request_to_key[req]='__REMOVED__'+key
        object_id=self._key_to_object.pop(key,None)
        if self.connected and object_id is not None:
            self._check(self.fn_remove(self._handle,object_id,self._new_req()),'AIRemoveObject')

    def close(self):
        if self.connected:
            for key in list(self._key_to_object):
                self.remove(key)
            self.fn_close(self._handle)
        self.connected=False
        self._request_to_key.clear()
        self._pending_positions.clear()
