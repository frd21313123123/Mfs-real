"""Bridge contract and verified cross-platform mock."""
from dataclasses import dataclass,field
from .geo import Position

class BridgeError(RuntimeError): pass

class MockBridge:
    def __init__(self):
        self.objects:dict[str,tuple[str,Position]]={}
        self.events:list[tuple]=[]
        self.connected=True
        self.player_position:Position|None=None

    def connect(self): self.connected=True
    def poll(self): pass

    def create(self,key:str,title:str,position:Position):
        if key in self.objects: return
        self.objects[key]=(title,position)
        self.events.append(('create',key,title))

    def update(self,key:str,position:Position):
        if key not in self.objects: return
        model=self.objects[key][0]
        self.objects[key]=(model,position)
        self.events.append(('update',key))

    def remove(self,key:str):
        if key in self.objects:
            del self.objects[key]
            self.events.append(('remove',key))

    def close(self):
        for key in list(self.objects): self.remove(key)
        self.connected=False
