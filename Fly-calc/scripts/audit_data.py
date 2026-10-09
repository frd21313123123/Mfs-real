"""Reproducible dataset inventory; this is not an ARINC424 routing decoder."""
from collections import Counter, defaultdict
import csv
import gzip
import hashlib
import json
import math
from pathlib import Path
import re
import sqlite3
import zipfile
from datetime import datetime, timezone

ROOT = Path(__file__).resolve().parents[1]
RAW = ROOT / 'data' / 'raw'
SAMPLES = {'Europe':'EGLL', 'Russia':'UUEE', 'North America':'KJFK', 'Asia':'RJTT', 'Africa':'FAOR', 'South America':'SBGR', 'Oceania':'YSSY'}


def read_csv(name):
    with (RAW / 'ourairports' / (name + '.csv')).open(encoding='utf-8-sig', newline='') as stream:
        return list(csv.DictReader(stream))


def point_key(ident, lat, lon):
    return ident, round(float(lat), 4), round(float(lon), 4)


def inventory_flightgear():
    points = set()
    fix_ids = Counter()
    counts, headers, versions = {}, {}, {}
    invalid = []
    for filename in ['fix.dat.gz', 'nav.dat.gz']:
        lines = gzip.open(RAW / 'flightgear' / filename, 'rt', encoding='latin-1').read().splitlines()
        headers[filename] = next(line for line in lines if 'Version' in line)
        versions[filename] = re.search(r'data cycle ([^,]+)', headers[filename]).group(1)
        count = 0
        for line in lines:
            fields = line.split()
            try:
                if filename.startswith('fix') and len(fields) == 3:
                    lat, lon, ident = fields
                    fix_ids[ident] += 1
                elif filename.startswith('nav') and len(fields) >= 9 and fields[0] in ['2','3','4','5','6','7','8','9','12','13']:
                    lat, lon, ident = fields[1], fields[2], fields[7]
                else:
                    continue
                latitude, longitude = float(lat), float(lon)
                if not (-90 <= latitude <= 90 and -180 <= longitude <= 180):
                    invalid.append(line)
                    continue
                points.add(point_key(ident, lat, lon))
                count += 1
            except ValueError:
                invalid.append(line)
        counts[filename] = count
    lines = gzip.open(RAW / 'flightgear/awy.dat.gz', 'rt', encoding='latin-1').read().splitlines()
    headers['awy.dat.gz'] = next(line for line in lines if 'Version' in line)
    versions['awy.dat.gz'] = re.search(r'data cycle ([^,]+)', headers['awy.dat.gz']).group(1)
    segments, endpoints, unresolved = [], set(), []
    altitude_invalid, types = 0, Counter()
    for line in lines:
        f = line.split()
        if len(f) != 10:
            continue
        try:
            start = point_key(f[0], f[1], f[2]); end = point_key(f[3], f[4], f[5])
            lo, hi = int(f[7]), int(f[8])
            altitude_invalid += lo > hi
            endpoints.update([start, end])
            types[f[6]] += 1
            if start not in points or end not in points:
                unresolved.append({'start': start, 'end': end, 'airway': f[9]})
            segments.append((float(f[1]), float(f[2]), float(f[4]), float(f[5]), f[9]))
        except ValueError:
            invalid.append(line)
    return {'cycleByFile': versions, 'counts': counts, 'airwaySegments': len(segments),
            'uniqueAirwayNames': len({name for s in segments for name in s[4].split('-')}),
            'uniqueAirwayFieldLabels': len({s[4] for s in segments}), 'duplicateFixIdentifiers': sum(n > 1 for n in fix_ids.values()),
            'duplicateFixExamples': [{'ident':i, 'occurrences':n} for i,n in fix_ids.most_common(10) if n>1],
            'coordinateKeyDecimals':4, 'unresolvedAirwaySegments':len(unresolved), 'unresolvedExamples':unresolved[:8],
            'invalidAltitudeRanges':altitude_invalid, 'airwayTypeCounts':dict(types),
            'directionEncoding':'Legacy XP640 records have no explicit per-edge direction field. Do not infer current one-way restrictions.',
            'invalidRecords':len(invalid), 'dataLicense': 'GPL-2.0-or-later, explicitly stated in all three dataset headers',
            'proceduresAudited':False, 'worldProceduresConfirmed':False}, points, segments


def faa_inventory():
    rejected = []
    source = None
    for candidate in [RAW / 'faa/CIFP_261001.zip', RAW / 'faa/CIFP_260903.zip']:
        try:
            with zipfile.ZipFile(candidate) as archive:
                if archive.testzip(): raise ValueError('CRC failure')
            source = candidate
            break
        except (OSError, ValueError, zipfile.BadZipFile) as error:
            rejected.append({'file':candidate.name,'error':str(error)})
    if source is None:
        return {'status':'unavailable_or_invalid','rejectedDownloads':rejected,'procedureSemanticsDecoded':False}
    try:
        with zipfile.ZipFile(source) as archive:
            bad = archive.testzip()
            if bad:
                raise ValueError('CRC failure in ' + bad)
            # PDFs and readme files are not aviation records.
            candidates = [i for i in archive.infolist() if i.file_size > 1000000 and not i.filename.lower().endswith('.pdf')]
            if not candidates:
                raise ValueError('No CIFP record file in ZIP')
            record = max(candidates, key=lambda i:i.file_size)
            lines = archive.read(record).decode('ascii').splitlines()
        categories = Counter()
        regions = Counter()
        airports = set()
        cycles = Counter()
        procedures = {'PD':set(),'PE':set(),'PF':set()}
        runways = set()
        runway_transitions = set()
        for line in lines:
            if len(line) != 132 or not line.startswith('S'):
                continue
            section = line[4]
            sub = line[12] if section in ['P','H'] else line[5]
            categories[section + sub] += 1
            regions[line[1:4]] += 1
            cycles[line[128:132]] += 1
            if section == 'P' and sub == 'A':
                airports.add(line[6:10].strip())
            if section == 'P' and sub == 'G':
                runways.add((line[6:10].strip(),line[13:18].strip()))
            if section + sub in procedures:
                procedures[section+sub].add((line[6:10].strip(),line[10:12],line[13:19].strip()))
                transition=line[20:25].strip()
                if re.fullmatch(r'RW\d{2}[LRC]?',transition):
                    runway_transitions.add((line[6:10].strip(),transition))
        return {'status':'inventoried', 'archive':source.name,'rejectedDownloads':rejected,
                'file':record.filename, 'recordCount':len(lines), 'recordCategories':dict(categories),
                'areaCodes':dict(regions), 'datasetCycle':{'CIFP_260903.zip':'2609','CIFP_261001.zip':'2610'}[source.name], 'recordLastChangeCycles':dict(cycles), 'airportCount':len(airports),
                'uniqueProcedureNamesByAirportRegion':{k:len(v) for k,v in procedures.items()},
                'runwayRecords':len(runways),'explicitRunwayTransitions':len(runway_transitions),
                'unmatchedExplicitRunwayTransitions':len(runway_transitions-runways),
                'unmatchedExplicitRunwayExamples':sorted(runway_transitions-runways)[:10],
                'samplePresence':{region:icao in airports for region,icao in SAMPLES.items()},
                'procedureSemanticsDecoded':False, 'note':'Record categories only; not validation of complete ARINC legs, transitions, minima or procedure/runway associations.'}
    except (OSError, ValueError, zipfile.BadZipFile) as error:
        return {'status':'unavailable_or_invalid', 'error':str(error), 'procedureSemanticsDecoded':False}


def main():
    airports, runways, navaids = read_csv('airports'), read_csv('runways'), read_csv('navaids')
    database = ROOT / 'data/research.sqlite'
    db = sqlite3.connect(database)
    db.executescript('DROP TABLE IF EXISTS airports; CREATE TABLE airports (icao TEXT PRIMARY KEY, name TEXT, country TEXT, latitude REAL, longitude REAL);')
    valid = [r for r in airports if re.fullmatch('[A-Z]{4}', r['icao_code'] or '') and r['type'] != 'closed']
    duplicates = Counter(r['icao_code'] for r in valid)
    # Do not silently resolve ambiguous ICAOs; exclude them and report all collisions.
    valid = [r for r in valid if duplicates[r['icao_code']] == 1]
    db.executemany('INSERT INTO airports VALUES (?,?,?,?,?)', [(r['icao_code'],r['name'],r['iso_country'],float(r['latitude_deg']),float(r['longitude_deg'])) for r in valid])
    db.commit(); db.close()
    source_file = RAW / 'ourairports/airports.csv'
    (ROOT / 'data/airports-metadata.json').write_text(json.dumps({
        'snapshotDate':datetime.fromtimestamp(source_file.stat().st_mtime, timezone.utc).date().isoformat(),
        'snapshotDateBasis':'local retrieval file timestamp; not an AIRAC effective date',
        'sourceSha256':hashlib.sha256(source_file.read_bytes()).hexdigest(),
        'recordCount':len(valid)
    },indent=2)+'\n',encoding='utf-8')
    ids = {r['ident'] for r in airports}
    runway_counts = Counter(r['airport_ident'] for r in runways)
    fg, points, segments = inventory_flightgear()
    samples = {}
    for region,icao in SAMPLES.items():
        match = next((r for r in valid if r['icao_code'] == icao), None)
        if match:
            lat,lon = float(match['latitude_deg']),float(match['longitude_deg'])
            # Regional smoke check only: a +/-2 degree box, not a route or completeness claim.
            samples[region] = {'icao':icao, 'name':match['name'], 'country':match['iso_country'],
                               'runwayRecords':runway_counts[match['ident']],
                               'enroutePointsIn2DegreeBox':sum(abs(p[1]-lat)<=2 and abs(p[2]-lon)<=2 for p in points),
                               'airwaySegmentsWithEndpointInBox':sum((abs(s[0]-lat)<=2 and abs(s[1]-lon)<=2) or (abs(s[2]-lat)<=2 and abs(s[3]-lon)<=2) for s in segments)}
        else:
            samples[region] = {'icao':icao,'status':'missing'}
    report = {'auditDate':'2026-10-09', 'worldRoutingReady':False,
              'ourairports':{'totalRecords':len(airports), 'activeUniqueFourLetterIcao':len(valid),
                            'countries':len({r['iso_country'] for r in airports}), 'continents':dict(Counter(r['continent'] or 'NA' for r in airports)),
                            'runwayRecords':len(runways),'navaidRecords':len(navaids),
                            'orphanRunwayRecords':sum(r['airport_ident'] not in ids for r in runways),
                            'duplicateIcaoCodes':{k:v for k,v in duplicates.items() if v>1},
                            'activeIcaoAirportsWithoutRunwayRecords':sum(runway_counts[r['ident']] == 0 for r in valid),
                            'airacCycle':None, 'license':'Public domain', 'waypointsAirwaysProcedures':False},
              'flightgear':fg, 'faa':faa_inventory(), 'regionalSmokeChecks':samples,
              'compatibility':{'mergedNavigationDatabase':False, 'blockingIssues':['OurAirports has no AIRAC cycle or procedure-runway links',
                  'FlightGear cycle 2013.10 differs from FAA 2609 and current airport records',
                  'Legacy fixes lack ICAO region keys; names are not globally unique',
                  'No confirmed free global SID/STAR/approach dataset', 'Legacy airway directions and current RAD/NOTAM constraints not established']}}
    (ROOT / 'evidence/coverage.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'airports':len(valid),'fixes':fg['counts']['fix.dat.gz'],'segments':fg['airwaySegments'],'faaStatus':report['faa']['status']},indent=2))


if __name__ == '__main__':
    main()
