"""Airport JSON graph, pathfinding with edge occupancy and runway tags."""
from __future__ import annotations
import json
import heapq
from dataclasses import dataclass
from pathlib import Path
from .geo import Position, distance_m

@dataclass(frozen=True)
class Node:
    id: str
    position: Position
    kind: str = 'taxi'

@dataclass(frozen=True)
class Edge:
    src: str
    dst: str
    speed_kt: float = 12.0
    kind: str = 'taxi'
    runway: str = ''

class AirportGraph:
    def __init__(self, nodes: list[Node], edges: list[Edge], icao: str='DEMO'):
        self.icao=icao
        self.nodes={n.id:n for n in nodes}
        if len(self.nodes)!=len(nodes): raise ValueError('duplicate node IDs')
        self.edges={}
        self.out={n.id:[] for n in nodes}
        for edge in edges:
            if edge.src not in self.nodes or edge.dst not in self.nodes: raise ValueError('unknown edge endpoint')
            if edge.src == edge.dst: raise ValueError('self edge')
            if edge.speed_kt <= 0: raise ValueError('nonpositive edge speed')
            self.edges[(edge.src,edge.dst)]=edge
            self.out[edge.src].append(edge)

    @classmethod
    def from_json(cls, path: str | Path):
        data=json.loads(Path(path).read_text(encoding='utf-8'))
        nodes=[Node(n['id'],Position(n['lat'],n['lon'],n.get('alt_ft',0),on_ground=True), n.get('kind','taxi')) for n in data['nodes']]
        edges=[]
        for item in data['edges']:
            edge=Edge(item['src'],item['dst'],item.get('speed_kt',12),item.get('kind','taxi'),item.get('runway',''))
            edges.append(edge)
            if item.get('bidirectional',False):
                edges.append(Edge(edge.dst,edge.src,edge.speed_kt,edge.kind,edge.runway))
        return cls(nodes,edges,data.get('icao','DEMO'))

    def route(self, start:str, target:str, forbidden: set[tuple[str,str]] | None = None) -> list[str]:
        if start not in self.nodes or target not in self.nodes: raise ValueError('node does not exist')
        if start==target: return [start]
        excluded=forbidden or set()
        costs={start:0.0}; prev={}; frontier=[(0.0,start)]
        while frontier:
            _,node=heapq.heappop(frontier)
            if node==target: break
            for edge in self.out[node]:
                if (edge.src,edge.dst) in excluded or edge.kind == 'closed': continue
                distance=distance_m(self.nodes[node].position,self.nodes[edge.dst].position)
                penalty=80 if edge.kind=='runway' else 0
                tentative=costs[node]+distance/edge.speed_kt+penalty
                if tentative < costs.get(edge.dst,float('inf')):
                    costs[edge.dst]=tentative; prev[edge.dst]=node
                    heuristic=distance_m(self.nodes[edge.dst].position,self.nodes[target].position)/35
                    heapq.heappush(frontier,(tentative+heuristic,edge.dst))
        if target not in prev: return []
        result=[target]
        while result[-1]!=start: result.append(prev[result[-1]])
        result.reverse()
        return result
