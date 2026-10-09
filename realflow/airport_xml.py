"""Convert MSFS airport scenery source XML into RealFlow taxi graphs.

Use on *source* XML from the MSFS scenery editor, not compiled BGL files.
All output must be checked against the actual airport before enabling movement.
"""
from __future__ import annotations
import json
import math
import xml.etree.ElementTree as ET
from pathlib import Path
from .airport import AirportGraph


def _tag(element:ET.Element)->str:
    return element.tag.rsplit('}',1)[-1].lower()


def _metres_to_feet(text:str)->float:
    text=(text or '0').strip().upper()
    if text.endswith('F'): return float(text[:-1])
    if text.endswith('M'): text=text[:-1]
    return float(text)*3.280839895


def airport_xml_to_graph(source:str|Path,icao:str|None=None)->dict:
    tree=ET.parse(source)
    airports=[e for e in tree.iter() if _tag(e)=='airport']
    if not airports: raise ValueError('XML contains no <Airport>')
    selected=next((e for e in airports if (e.get('ident') or '').upper()==(icao or '').upper()),
                  None) if icao else airports[0]
    if selected is None: raise ValueError(f'Airport {icao} not found')
    apt_ident=(selected.get('ident') or selected.get('icao') or 'UNKNOWN').upper()
    airport_lat=float(selected.get('lat','0'))
    airport_lon=float(selected.get('lon','0'))
    airport_alt=_metres_to_feet(selected.get('alt','0'))
    nodes:dict[int,dict]={}
    warnings=[]
    for item in selected:
        name=_tag(item)
        if name not in ('taxiwaypoint','taxiwayparking'):continue
        if item.get('index') is None:
            warnings.append(f'{name} without index')
            continue
        index=int(item.attrib['index'])
        if 'lat' in item.attrib and 'lon' in item.attrib:
            lat=float(item.attrib['lat']);lon=float(item.attrib['lon'])
        else:
            bx=float(item.get('biasX','0'));bz=float(item.get('biasZ','0'))
            lat=airport_lat+bz/111_195.0
            lon=airport_lon+bx/(111_195.0*max(.01,math.cos(math.radians(airport_lat))))
        kind='gate' if name=='taxiwayparking' else 'hold' if 'HOLD_SHORT' in item.get('type','').upper() else 'taxi'
        nodes[index]={'id':f'N{index}','lat':lat,'lon':lon,'alt_ft':airport_alt,'kind':kind}
    edges=[]
    for item in selected:
        if _tag(item)!='taxiwaypath': continue
        typ=item.get('type','TAXI').upper()
        if typ in ('CLOSED','VEHICLE','ROAD'):continue
        try:
            start=int(item.attrib['start']);end=int(item.attrib['end'])
        except (KeyError,ValueError):continue
        if start not in nodes or end not in nodes:
            warnings.append(f'path {start}->{end} references nonexistent index')
            continue
        runway=''
        if typ=='RUNWAY':
            runway=f'{apt_ident}/{item.get("number","R")}{item.get("designator","")}'
            nodes[start]['kind']='runway' if nodes[start]['kind']!='hold' else 'hold'
            nodes[end]['kind']='runway' if nodes[end]['kind']!='hold' else 'hold'
        if typ=='PARKING':
            # Build an outbound pushback edge and inbound taxi/parking edge.
            a,b=nodes[start],nodes[end]
            if a['kind']=='gate':
                gate,taxi=start,end
            elif b['kind']=='gate':
                gate,taxi=end,start
            else:
                warnings.append(f'parking path {start}->{end} has no gate')
                continue
            edges.append({'src':f'N{gate}','dst':f'N{taxi}','speed_kt':3,'kind':'pushback'})
            edges.append({'src':f'N{taxi}','dst':f'N{gate}','speed_kt':4,'kind':'taxi'})
        else:
            edges.append({'src':f'N{start}','dst':f'N{end}',
                          'speed_kt':100 if runway else 12,
                          'kind':'runway' if runway else 'taxi',
                          'runway':runway,'bidirectional':True})
    if not nodes: raise ValueError('No taxiway points or parking spaces were found')
    return {'icao':apt_ident,'nodes':list(nodes.values()),'edges':edges,
            '_NOTICE':'Generated from MSFS source XML; requires validation before real simulator use.',
            '_warnings':warnings}


def convert(source:str|Path,destination:str|Path,icao:str|None=None)->dict:
    data=airport_xml_to_graph(source,icao)
    target=Path(destination)
    target.parent.mkdir(parents=True,exist_ok=True)
    target.write_text(json.dumps(data,indent=2),encoding='utf-8')
    graph=AirportGraph.from_json(target)
    return {'icao':graph.icao,'nodes':len(graph.nodes),'edges':len(graph.edges),'warnings':data['_warnings']}
