"""OpenSky OAuth2/anonymous source, with bounding boxes and rate-limit handling."""
import json
import math
import os
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from .geo import Position, FT_TO_M, KNOT_TO_MPS

API_URL='https://opensky-network.org/api/states/all'
TOKEN_URL='https://auth.opensky-network.org/auth/realms/opensky-network/protocol/openid-connect/token'

@dataclass(frozen=True)
class Observation:
    icao24: str
    callsign: str
    position: Position
    vertical_mps: float
    observed_at: float
    icao_type: str = ''


def parse_states(payload:dict) -> list[Observation]:
    results=[]
    for row in payload.get('states') or []:
        try:
            if len(row)<17 or row[5] is None or row[6] is None: continue
            icao=str(row[0] or '').lower().strip()
            if not icao: continue
            # OpenSky field 7 is barometric altitude, 13 geometric altitude (meters).
            altitude_m=row[13] if row[13] is not None else row[7]
            if altitude_m is None: continue
            speed_mps=float(row[9] or 0)
            heading=float(row[10] or 0)
            pos=Position(float(row[6]),float(row[5]),float(altitude_m)/FT_TO_M,
                heading=heading,speed_kt=speed_mps/KNOT_TO_MPS,on_ground=bool(row[8]))
            ts=float(row[4] or row[3] or payload.get('time') or 0)
            results.append(Observation(icao,(row[1] or '').strip(),pos,float(row[11] or 0),ts))
        except (ValueError,TypeError,IndexError):
            continue
    return results

class OpenSkyClient:
    def __init__(self, client_id:str|None=None,client_secret:str|None=None,minimum_interval:int=120):
        if minimum_interval<60: raise ValueError('minimum poll interval is 60s')
        self.client_id=client_id or os.getenv('OPENSKY_CLIENT_ID','')
        self.client_secret=client_secret or os.getenv('OPENSKY_CLIENT_SECRET','')
        self.minimum_interval=minimum_interval
        self._token=''; self._expires=0.0; self._last_poll=0.0
        self.next_allowed=0.0

    def _request(self,url:str,headers:dict|None=None,data:bytes|None=None):
        req=urllib.request.Request(url,headers={'User-Agent':'RealFlowTraffic/0.1','Accept':'application/json',**(headers or {})},data=data)
        with urllib.request.urlopen(req,timeout=15) as resp:
            return json.load(resp)

    def _auth(self)->str:
        if not self.client_id or not self.client_secret: return ''
        if self._token and time.monotonic()<self._expires: return self._token
        body=urllib.parse.urlencode({'grant_type':'client_credentials',
                'client_id':self.client_id,'client_secret':self.client_secret}).encode()
        payload=self._request(TOKEN_URL,{'Content-Type':'application/x-www-form-urlencoded'},body)
        self._token=payload['access_token']
        self._expires=time.monotonic()+max(1,int(payload.get('expires_in',1800))-30)
        return self._token

    def poll(self,lat:float,lon:float,radius_km:float=100.0)->list[Observation]:
        now=time.monotonic()
        if now<self.next_allowed: return []
        if radius_km<=0 or radius_km>250: raise ValueError('radius must be >0 and <=250km')
        dy=radius_km/111.2
        dx=min(180,radius_km/(111.2*max(0.1,abs(math.cos(math.radians(lat))))))
        bbox={'lamin':max(-90,lat-dy),'lamax':min(90,lat+dy),
              'lomin':max(-180,lon-dx),'lomax':min(180,lon+dx)}
        headers={}
        token=self._auth()
        if token: headers['Authorization']='Bearer '+token
        try:
            payload=self._request(API_URL+'?'+urllib.parse.urlencode(bbox),headers)
        except urllib.error.HTTPError as e:
            if e.code==401 and self.client_id:
                self._token='';self._expires=0.0
            if e.code in (401,403,429):
                wait=int(e.headers.get('X-Rate-Limit-Retry-After-Seconds','300')) if e.headers else 300
                self.next_allowed=now+max(self.minimum_interval,wait)
            raise
        self.next_allowed=now+self.minimum_interval
        return parse_states(payload)
