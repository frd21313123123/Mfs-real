"""Simple Windows-friendly Tkinter control and offline airport preview.

UI preview does not connect to the actual game unless launched explicitly via CLI.
"""
from __future__ import annotations
import tkinter as tk
from tkinter import ttk,filedialog,messagebox
import threading
from pathlib import Path
from .geo import Position
from .airport import AirportGraph
from .bridge import MockBridge
from .config import Settings
from .fsltl import discover_root,scan_models,ModelMatcher,AircraftModel
from .ground import GroundEngine
from .manager import TrafficManager
from .cli import EXAMPLE_GRAPH

BG='#111b29'
PANEL='#192738'
LIGHT='#dbe5f5'
TEAL='#45d4ce'
MUTED='#92a5bc'

class Application(tk.Tk):
    def __init__(self):
        super().__init__()
        self.title('RealFlow Traffic · 0.1 Prototype')
        self.geometry('1080x710')
        self.minsize(880,600)
        self.configure(bg=BG)
        self.settings=Settings.load('config.json')
        self.airport=AirportGraph.from_json(EXAMPLE_GRAPH)
        self.demo=None
        self.now=1000000
        self.running=False
        self._build()
        self.after(120,self._loop)
        self.protocol('WM_DELETE_WINDOW',self.shutdown)

    def _label(self,parent,text,color=LIGHT,size=10,bold=False):
        return tk.Label(parent,text=text,bg=parent.cget('bg'),fg=color,
                        font=('Segoe UI',size,'bold' if bold else 'normal'))

    def _build(self):
        top=tk.Frame(self,bg=BG);top.pack(fill='x',padx=24,pady=(20,13))
        self._label(top,'✈ REALFLOW',TEAL,22,True).pack(side='left')
        self._label(top,'TRAFFIC  ·  MSFS 2020  ·  HYBRID',MUTED,10).pack(side='left',padx=22,pady=(12,0))
        body=tk.Frame(self,bg=BG);body.pack(fill='both',expand=True,padx=20,pady=12)
        sidebar=tk.Frame(body,bg=PANEL,width=286);sidebar.pack(side='left',fill='y',padx=(0,14));sidebar.pack_propagate(False)
        frame=tk.Frame(sidebar,bg=PANEL);frame.pack(fill='x',padx=17,pady=17)
        self._label(frame,'CONFIGURATION',TEAL,11,True).pack(anchor='w',pady=(0,12))
        self.mode=tk.StringVar(value=self.settings.mode)
        self._label(frame,'Traffic source',MUTED).pack(anchor='w')
        cb=ttk.Combobox(frame,textvariable=self.mode,values=['hybrid','live','simulation'],state='readonly')
        cb.pack(fill='x',pady=(3,12))
        self.air_limit=tk.IntVar(value=self.settings.airborne_limit)
        self.ground_limit=tk.IntVar(value=self.settings.ground_limit)
        for name,var,limit in [('Airborne limit',self.air_limit,35),('Ground limit',self.ground_limit,30)]:
            row=tk.Frame(frame,bg=PANEL);row.pack(fill='x')
            self._label(row,name,MUTED).pack(side='left')
            tk.Spinbox(row,from_=0,to=limit,width=4,textvariable=var,bg=BG,fg=LIGHT,buttonbackground=PANEL).pack(side='right',pady=5)
        self.fsltl=tk.StringVar(value=self.settings.fsltl_path)
        self._label(frame,'FSLTL package path',MUTED).pack(anchor='w',pady=(14,3))
        entry=tk.Entry(frame,textvariable=self.fsltl,bg=BG,fg=LIGHT,insertbackground=LIGHT,relief='flat')
        entry.pack(fill='x',ipady=7)
        tk.Button(frame,text='Browse directory',command=self.browse,bg=PANEL,fg=LIGHT,relief='flat').pack(anchor='w',pady=(5,12))
        tk.Button(frame,text='SCAN FSLTL',command=self.scan,bg='#28516b',fg='white',relief='flat',pady=10).pack(fill='x',pady=4)
        tk.Button(frame,text='START OFFLINE DEMO',command=self.toggle,bg='#138f86',fg='white',relief='flat',pady=11).pack(fill='x',pady=4)
        tk.Button(frame,text='SAVE CONFIG',command=self.save,bg='#28516b',fg='white',relief='flat',pady=10).pack(fill='x',pady=4)
        self.info=tk.StringVar(value='No connection. Offline preview is available.')
        status=self._label(sidebar,'');status.configure(textvariable=self.info,wraplength=250,justify='left',fg=MUTED)
        status.pack(anchor='w',padx=17,pady=18)
        right=tk.Frame(body,bg=BG);right.pack(side='left',fill='both',expand=True)
        info=tk.Frame(right,bg=BG);info.pack(fill='x',pady=(0,12))
        self.airstat=tk.StringVar(value='0 / 35')
        self.groundstat=tk.StringVar(value='0 / 30')
        self.runwaystat=tk.StringVar(value='FREE')
        for label,var in [('AIRCRAFT IN AIR',self.airstat),('GROUND AIRCRAFT',self.groundstat),('RUNWAY STATUS',self.runwaystat)]:
            panel=tk.Frame(info,bg=PANEL);panel.pack(side='left',fill='both',expand=True,padx=(0,7),ipadx=10,ipady=8)
            self._label(panel,label,MUTED,9).pack(anchor='w',padx=10,pady=(9,2))
            self._label(panel,'',LIGHT,18,True).pack(anchor='w',padx=10,pady=(0,10))
            panel.winfo_children()[-1].configure(textvariable=var)
        mapframe=tk.Frame(right,bg=PANEL);mapframe.pack(fill='both',expand=True)
        self._label(mapframe,'OFFLINE TAXIWAY MAP  ·  FICTIONAL TEST DATA',MUTED,10,True).pack(anchor='w',padx=17,pady=(14,0))
        self.canvas=tk.Canvas(mapframe,bg='#0b1722',highlightthickness=0)
        self.canvas.pack(fill='both',expand=True,padx=12,pady=12)
        self.canvas.bind('<Configure>',lambda evt:self.render())
        bottom=tk.Frame(self,bg=BG);bottom.pack(fill='x',padx=24,pady=(0,14))
        self._label(bottom,'Prototype: native SimConnect aircraft movement is experimental. Demo does not use the game.',MUTED,9).pack(anchor='w')

    def browse(self):
        path=filedialog.askdirectory(title='Choose fsltl-traffic-base or Community folder')
        if path:self.fsltl.set(path)

    def scan(self):
        root=discover_root(self.fsltl.get())
        if not root:
            self.info.set('FSLTL Base Models not found. Choose the installed package folder.')
            return
        count=len(scan_models(root))
        self.info.set(f'FSLTL models indexed: {count}. Root: {root}')

    def save(self):
        self.settings.mode=self.mode.get()
        self.settings.airborne_limit=self.air_limit.get()
        self.settings.ground_limit=self.ground_limit.get()
        self.settings.fsltl_path=self.fsltl.get()
        try:
            self.settings.save('config.json')
            self.info.set('Configuration saved to config.json')
        except ValueError as exc: messagebox.showerror('Invalid configuration',str(exc))

    def toggle(self):
        if self.running:
            self.running=False
            if self.demo:self.demo.close()
            self.demo=None
            self.info.set('Offline demonstration stopped')
            return
        self.settings.mode=self.mode.get()
        self.settings.airborne_limit=self.air_limit.get()
        self.settings.ground_limit=self.ground_limit.get()
        try: self.settings.validate()
        except ValueError as exc: messagebox.showerror('Invalid settings',str(exc));return
        root=discover_root(self.fsltl.get())
        models=scan_models(root) if root else []
        if not models:
            models=[AircraftModel('DEMO A320 (not in MSFS)','A320','AFL','OFFLINE')]
        bridge=MockBridge()
        ground=GroundEngine(self.airport,max_ground=self.settings.ground_limit)
        if self.settings.ground_limit>=1:ground.add('TEST101',models[0].title,'GATE_A','RUNWAY_EXIT')
        if self.settings.ground_limit>=2:ground.add('TEST102',models[0].title,'GATE_B','RUNWAY_EXIT')
        self.demo=TrafficManager(self.settings,ModelMatcher(models),bridge,ground)
        self.running=True
        self.now=1000000
        self.info.set('Offline simulator is active. No MSFS connection.')

    def _loop(self):
        if self.running and self.demo:
            for _ in range(3):
                self.now+=1
                stats=self.demo.tick(1,self.now,Position(47.452,-122.309,5000))
            self.airstat.set(f"{stats['live']+stats['synthetic']} / {self.settings.airborne_limit}")
            self.groundstat.set(f"{stats['ground']} / {self.settings.ground_limit}")
            r=self.demo.ground.runways.reservations
            self.runwaystat.set('OCCUPIED' if r else 'FREE')
            self.render()
        self.after(120,self._loop)

    def render(self):
        canvas=self.canvas
        canvas.delete('all')
        width=max(20,canvas.winfo_width());height=max(20,canvas.winfo_height())
        nodes=self.airport.nodes
        lats=[n.position.lat for n in nodes.values()];lons=[n.position.lon for n in nodes.values()]
        lat_min,lat_max=min(lats),max(lats);lon_min,lon_max=min(lons),max(lons)
        pad=38
        def pos(p):
            return (pad+(p.lon-lon_min)/(lon_max-lon_min)*(width-2*pad),
                    height-pad-(p.lat-lat_min)/(lat_max-lat_min)*(height-2*pad))
        for edge in self.airport.edges.values():
            x1,y1=pos(nodes[edge.src].position);x2,y2=pos(nodes[edge.dst].position)
            canvas.create_line(x1,y1,x2,y2,fill='#9b8e54' if edge.runway else '#52627a',width=7 if edge.runway else 3)
        for node in nodes.values():
            x,y=pos(node.position)
            color=TEAL if node.kind=='gate' else '#efa775' if node.kind=='hold' else '#7d8ca0'
            canvas.create_oval(x-3,y-3,x+3,y+3,fill=color,outline='')
            if node.kind in ('gate','hold'):
                canvas.create_text(x+6,y-7,text=node.id,fill=LIGHT,anchor='w',font=('Segoe UI',8))
        if self.demo and self.demo.ground:
            for plane in self.demo.ground.aircraft.values():
                if plane.phase.value in ('AIRBORNE','COMPLETE'):continue
                x,y=pos(plane.position)
                canvas.create_oval(x-7,y-7,x+7,y+7,fill=TEAL,outline='#ffffff',width=1)
                canvas.create_text(x+9,y+12,text=plane.id+' '+plane.phase.value,fill=LIGHT,anchor='w',font=('Segoe UI',9,'bold'))

    def shutdown(self):
        if self.demo: self.demo.close()
        self.destroy()

def main():
    Application().mainloop()

if __name__=='__main__':main()
