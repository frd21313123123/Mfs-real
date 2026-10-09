"""Fetch only public research inputs; record hashes and failures, without credentials."""
from pathlib import Path
import argparse
import concurrent.futures
import hashlib
import json
import urllib.request
import io
import zipfile
from datetime import datetime, timezone

ROOT = Path(__file__).resolve().parents[1]
RAW = ROOT / "data" / "raw"
EVIDENCE = ROOT / "evidence"
FBW = {
    "msfs2020": {"branch": "fs2020-master", "sha": "672384b9602a84d6832b8852cd6ec0abdd006f85"},
    "msfs2024": {"branch": "master", "sha": "255546cb02db1c7b40d2505ac621a9d36748d258"},
}
records = []


def download(url, target):
    request = urllib.request.Request(url, headers={"User-Agent": "Fly-calc-personal-research/0.1"})
    with urllib.request.urlopen(request, timeout=60) as response:
        body = response.read(64 * 1024 * 1024 + 1)
        if len(body) > 64 * 1024 * 1024:
            raise ValueError("Research download exceeds 64 MiB")
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.suffix == '.zip' and not body.startswith(b'PK\x03\x04'):
        raise ValueError('Server did not return ZIP data; response was not accepted as a dataset')
    if target.suffix == '.zip':
        with zipfile.ZipFile(io.BytesIO(body)) as archive:
            broken = archive.testzip()
            if broken:
                raise ValueError('ZIP checksum failed: ' + broken)
    if target.suffix == '.pdf' and not body.startswith(b'%PDF-'):
        raise ValueError('Server did not return PDF data; response was not accepted as a document')
    temp = target.with_name(target.name + '.download')
    temp.write_bytes(body)
    temp.replace(target)
    return {"url": url, "path": target.relative_to(ROOT).as_posix(), "bytes": len(body),
            "sha256": hashlib.sha256(body).hexdigest(), "status": "downloaded", "downloadedAt": datetime.now(timezone.utc).isoformat()}


def fetch(url, target):
    try:
        record = download(url, target)
    except Exception as error:
        record = {"url": url, "path": target.relative_to(ROOT).as_posix(), "status": "failed", "error": str(error)}
    records.append(record)
    return record


def github_snapshot(simulator, lock):
    base = EVIDENCE / "upstream" / simulator
    api = "https://api.github.com/repos/flybywiresim/aircraft"
    commit_record = fetch(f"{api}/commits/{lock['sha']}", base / "commit.json")
    tree_record = fetch(f"{api}/git/trees/{lock['sha']}?recursive=1", base / "tree.json")
    if tree_record["status"] != "downloaded":
        return
    tree = json.loads((base / "tree.json").read_text(encoding="utf-8"))
    if tree.get("truncated"):
        raise ValueError("Truncated GitHub tree; cannot establish integration paths")
    candidates = [entry["path"] for entry in tree["tree"] if entry["type"] == "blob"]
    chosen = [p for p in candidates if p.endswith(("/EFB/Efb.tsx", "/ToolBar/ToolBar.tsx"))]
    chosen += [p for p in candidates if p in {"package.json", "AGENTS.md", ".gitmodules", "LICENSE", "Dockerfile",
                 "igniter.config.mjs", "fbw-a32nx/mach.config.js", "fbw-a32nx/src/systems/instruments/src/EFB/tsconfig.json",
                 "fbw-a32nx/src/systems/instruments/tsconfig.react.json", "fbw-common/src/systems/instruments/src/EFB/tsconfig.json",
                 "scripts/build_a32nx.sh", "scripts/build.sh", "scripts/setup.sh", "scripts/dev-env/run.cmd"}]
    chosen += [p for p in candidates if p.endswith("AGENTS.md") and ("fbw-common" in p or "fbw-a32nx" in p)]
    for path in sorted(set(chosen)):
        fetch(f"https://raw.githubusercontent.com/flybywiresim/aircraft/{lock['sha']}/{path}", base / path)
    print(simulator, lock["sha"], "source files:", len(set(chosen)), flush=True)


def flightgear_snapshot():
    api = "https://gitlab.com/api/v4/projects/flightgear%2Ffgdata/repository"
    target = EVIDENCE / "upstream" / "flightgear"
    r = fetch(f"{api}/commits?per_page=1", target / "commits.json")
    if r["status"] != "downloaded":
        return
    sha = json.loads((target / "commits.json").read_text())[0]["id"]
    fetch(f"{api}/tree?ref={sha}&per_page=100", target / "root-tree.json")
    fetch(f"{api}/tree?ref={sha}&path=Navaids&per_page=100", target / "navaids-tree.json")
    for path in ["Navaids/nav.dat.gz", "Navaids/fix.dat.gz", "Navaids/awy.dat.gz", "Navaids/ReadMe.FG226.txt", "LICENSE.md"]:
        dest = RAW / "flightgear" / Path(path).name
        fetch(f"https://gitlab.com/flightgear/fgdata/-/raw/{sha}/{path}", dest)
    (EVIDENCE / "flightgear-lock.json").write_text(json.dumps({"sha": sha}, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--group", choices=["all", "fbw", "data", "supplement"], default="all")
    args = parser.parse_args()
    EVIDENCE.mkdir(exist_ok=True)
    manifest = EVIDENCE / "downloads.json"
    previous = json.loads(manifest.read_text()) if manifest.exists() else {"records": []}
    if args.group == 'supplement':
        for simulator, lock in FBW.items():
            base = EVIDENCE / 'upstream' / simulator
            candidates = [entry['path'] for entry in json.loads((base / 'tree.json').read_text())['tree']]
            wanted = ['igniter.config.mjs','fbw-a32nx/mach.config.js','fbw-a32nx/src/systems/instruments/src/EFB/tsconfig.json',
                      'fbw-a32nx/src/systems/instruments/tsconfig.react.json','fbw-common/src/systems/instruments/src/EFB/tsconfig.json']
            for path in wanted:
                if path in candidates:
                    fetch(f"https://raw.githubusercontent.com/flybywiresim/aircraft/{lock['sha']}/{path}", base / path)
        fg_sha = json.loads((EVIDENCE / 'flightgear-lock.json').read_text())['sha']
        for path in ['LICENSE.md','Navaids/ReadMe.FG226.txt']:
            fetch(f'https://gitlab.com/flightgear/fgdata/-/raw/{fg_sha}/{path}', RAW / 'flightgear' / Path(path).name)
        fetch('https://api.core.openaip.net/api/system/specs/v1/schema.json', EVIDENCE / 'openaip-schema.json')
        fetch('https://aeronav.faa.gov/Upload_313-d/cifp/CIFP_260903.zip', RAW / 'faa/CIFP_260903.zip')
    if args.group in {"all", "fbw"}:
        for simulator, lock in FBW.items():
            github_snapshot(simulator, lock)
        (EVIDENCE / "fbw-lock.json").write_text(json.dumps(FBW, indent=2) + "\n")
    if args.group in {"all", "data"}:
        jobs = []
        for name in ["airports", "runways", "navaids", "countries"]:
            jobs.append((f"https://raw.githubusercontent.com/davidmegginson/ourairports-data/main/{name}.csv", RAW / "ourairports" / f"{name}.csv"))
        jobs += [("https://aeronav.faa.gov/Upload_313-d/cifp/CIFP_261001.zip", RAW / "faa" / "CIFP_261001.zip"),
                 ("https://aeronav.faa.gov/Upload_313-d/cifp/CIFP%20Readme.pdf", RAW / "faa" / "CIFP-Readme.pdf")]
        with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
            list(pool.map(lambda job: fetch(*job), jobs))
        flightgear_snapshot()
    merged = {r["url"]: r for r in previous["records"]}
    merged.update({r["url"]: r for r in records})
    manifest.write_text(json.dumps({"fetchedAt": datetime.now(timezone.utc).isoformat(), "records": list(merged.values())}, indent=2) + "\n")
    print(json.dumps({"downloaded": sum(r["status"] == "downloaded" for r in records), "failed": [r for r in records if r["status"] == "failed"]}, indent=2))


if __name__ == "__main__":
    main()
