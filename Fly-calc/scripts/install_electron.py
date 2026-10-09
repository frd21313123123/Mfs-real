"""Fallback Windows x64 installer, using the checksums in the pinned Electron npm package."""
import hashlib
import json
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import time
import sys
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]
package = ROOT / 'node_modules/electron'
version = json.loads((package / 'package.json').read_text())['version']
filename = f'electron-v{version}-win32-x64.zip'
expected = json.loads((package / 'checksums.json').read_text())[filename]
if (package / 'dist/electron.exe').exists() and (package / 'dist/version').read_text().strip().lstrip('v') == version:
    print(f'Electron {version} is already installed locally.')
    sys.exit(0)
target = ROOT / 'output/install' / filename
target.parent.mkdir(parents=True, exist_ok=True)
url = f'https://github.com/electron/electron/releases/download/v{version}/{filename}'
deadline = time.monotonic() + 600
with urllib.request.urlopen(urllib.request.Request(url, headers={'Range':'bytes=0-0'}), timeout=30) as response:
    if response.status != 206:
        raise ValueError('Official download server must support validated byte ranges')
    total = int(response.headers['Content-Range'].split('/')[-1])
if total > 240 * 1024 * 1024:
    raise ValueError('Electron archive exceeds size limit')
parts = target.with_suffix('.parts')
parts.mkdir(exist_ok=True)
chunk_size = 4 * 1024 * 1024
ranges = [(start, min(start + chunk_size, total) - 1) for start in range(0, total, chunk_size)]

def download_part(bounds):
    start, end = bounds
    part = parts / str(start)
    if part.exists() and part.stat().st_size == end - start + 1:
        return part
    request = urllib.request.Request(url, headers={'Range':f'bytes={start}-{end}'})
    for attempt in range(5):
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                if response.status != 206 or response.headers.get('Content-Range') != f'bytes {start}-{end}/{total}':
                    raise ValueError('Download server returned an unexpected range')
                data = response.read(end - start + 2)
            break
        except OSError:
            if attempt == 4 or time.monotonic() > deadline:
                raise
            time.sleep(1 + attempt)
    if len(data) != end - start + 1 or time.monotonic() > deadline:
        raise ValueError('Electron range incomplete or download deadline exceeded')
    part.write_bytes(data)
    return part

with ThreadPoolExecutor(max_workers=8) as pool:
    downloaded = list(pool.map(download_part, ranges))
digest = hashlib.sha256()
with target.open('wb') as stream:
    for part in downloaded:
        data = part.read_bytes()
        stream.write(data)
        digest.update(data)
if digest.hexdigest() != expected:
    raise ValueError('Electron SHA-256 mismatch; archive was not installed')
(ROOT / 'evidence/electron-runtime.json').write_text(json.dumps({'version':version,'platform':'Windows x64','url':url,'bytes':total,'sha256':digest.hexdigest(),'checksumBasis':'pinned npm package checksums.json','verification':'passed'},indent=2)+'\n')
dist = (package / 'dist').resolve()
dist.mkdir(exist_ok=True)
with zipfile.ZipFile(target) as archive:
    if archive.testzip():
        raise ValueError('Electron ZIP checksum failure')
    for item in archive.infolist():
        dest = (dist / item.filename).resolve()
        if not dest.is_relative_to(dist):
            raise ValueError('Unsafe ZIP path')
    archive.extractall(dist)
(package / 'path.txt').write_text('electron.exe')
print(f'Installed official Electron {version}, Windows x64; SHA-256 verified.', flush=True)
