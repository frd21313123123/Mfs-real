"""Build immutable, reviewable patches from the fetched source snapshots."""
import difflib
import hashlib
import json
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
EFB = 'fbw-common/src/systems/instruments/src/EFB'


def main():
    lock = json.loads((ROOT/'evidence/fbw-lock.json').read_text())
    destination = ROOT/'integrations/fbw/patches'
    destination.mkdir(parents=True,exist_ok=True)
    results = {}
    for simulator, revision in lock.items():
        patches = []
        hashes = {}
        base = ROOT/'evidence/upstream'/simulator
        for relative in [EFB+'/Efb.tsx', EFB+'/ToolBar/ToolBar.tsx']:
            old = (base/relative).read_text(encoding='utf-8')
            hashes[relative] = hashlib.sha256((base/relative).read_bytes()).hexdigest()
            if relative.endswith('/Efb.tsx'):
                anchor = "import { Dashboard } from './Dashboard/Dashboard';"
                route = '<Route path="/dispatch" component={Dispatch} />'
                if old.count(anchor)!=1 or old.count(route)!=1: raise ValueError('Upstream route anchors changed')
                new = old.replace(anchor,anchor+"\nimport { FlyCalc } from './FlyCalc/FlyCalc';").replace(route,route+'\n                  <Route path="/fly-calc" component={FlyCalc} />')
            else:
                anchor = '<ToolBarButton to="/ground" tooltipText={t(\'Ground.Title\')}>'
                if old.count(anchor)!=1: raise ValueError('Upstream toolbar anchor changed')
                new = old.replace(anchor, '<ToolBarButton to="/fly-calc" tooltipText="Fly-calc">\n        <span className="text-2xl font-bold">FC</span>\n      </ToolBarButton>\n      '+anchor)
            new = new.replace('Copyright (c) 2023-2024', 'Copyright (c) 2023-2026').replace('Copyright (c) 2023-2025', 'Copyright (c) 2023-2026')
            patches.extend(difflib.unified_diff(old.splitlines(True),new.splitlines(True),fromfile='a/'+relative,tofile='b/'+relative))
        for name in ['FlyCalc.tsx','client.ts','flycalc.css']:
            new=(ROOT/'src'/name).read_text(encoding='utf-8')
            patches.extend(difflib.unified_diff([],new.splitlines(True),fromfile='/dev/null',tofile='b/'+EFB+'/FlyCalc/'+name))
        filename=destination/(simulator+'.patch')
        filename.write_text(''.join(patches),encoding='utf-8',newline='\n')
        results[simulator]={**revision,'patch':filename.relative_to(ROOT).as_posix(),'sha256':hashlib.sha256(filename.read_bytes()).hexdigest(),'sourceHashes':hashes,'fullAircraftBuild':'not_run','inSimulator':'not_run'}
    (ROOT/'integrations/fbw/lock.json').write_text(json.dumps(results,indent=2)+'\n')
    print('Generated patches for MSFS 2020 and 2024, pinned to separate revisions.')


if __name__=='__main__': main()
