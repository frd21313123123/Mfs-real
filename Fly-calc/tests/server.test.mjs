import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { DatabaseSync } from 'node:sqlite';
import { mkdtempSync, rmSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import http from 'node:http';
import { createServer } from '../src/server.mjs';

let server, url, db, directory;
before(async () => {
  db = new DatabaseSync(':memory:');
  db.exec('CREATE TABLE airports(icao TEXT PRIMARY KEY,name TEXT,country TEXT,latitude REAL,longitude REAL)');
  const insert = db.prepare('INSERT INTO airports VALUES(?,?,?,?,?)');
  insert.run('UUEE', 'Sheremetyevo', 'RU', 55.9726, 37.4146);
  insert.run('ULLI', 'Pulkovo', 'RU', 59.8003, 30.2625);
  insert.run('EGLL', 'Heathrow', 'GB', 51.4706, -0.4619);
  directory = mkdtempSync(path.join(os.tmpdir(), 'flycalc-tests-'));
  server = await createServer({ database: db, documentRoot: directory });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  url = 'http://127.0.0.1:' + server.address().port;
});
after(async () => {
  server.closeAllConnections();
  await new Promise((resolve) => server.close(resolve));
  db.close();
  const resolved = path.resolve(directory);
  if (path.dirname(resolved) !== path.resolve(os.tmpdir()) || !path.basename(resolved).startsWith('flycalc-tests-')) throw new Error('Unsafe test cleanup path');
  rmSync(resolved, { recursive: true, force: true });
});
const post = (body, endpoint = '/api/v1/probes') => fetch(url + endpoint, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });

test('status exposes closed feasibility gate and all six unverified aircraft', async () => {
  const status = await (await fetch(url + '/api/v1/status')).json();
  assert.equal(status.worldRoutingReady, false);
  assert.equal(status.airportCount, 3);
  assert.equal(status.fenix.cockpitIntegration, 'unconfirmed');
  const aircraft = await (await fetch(url + '/api/v1/aircraft')).json();
  assert.equal(aircraft.length, 6);
  assert.ok(aircraft.every((a) => a.status === 'unverified' && a.accuracyPercent === null));
});
test('production flight endpoint never substitutes an approximate route', async () => {
  const r = await post({ departure:'UUEE', arrival:'ULLI', aircraft:'fbw-a32nx' }, '/api/v1/flights');
  assert.equal(r.status, 503);
  assert.equal((await r.json()).code, 'FEASIBILITY_GATE_CLOSED');
});
test('ICAO input rejects invalid codes, identical airports, unknown aircraft and unknown airport', async () => {
  const cases = [
    [null, 400], [{ departure:32,arrival:'ULLI' },400],
    [{ departure:'UU1E',arrival:'ULLI',aircraft:'fbw-a32nx' },400],
    [{ departure:'UUEE',arrival:'UUEE',aircraft:'fbw-a32nx' },400],
    [{ departure:'UUEE',arrival:'ULLI',aircraft:'fake' },400],
    [{ departure:'ZZZZ',arrival:'ULLI',aircraft:'fbw-a32nx' },404],
  ];
  for (const [body, code] of cases) assert.equal((await post(body)).status, code);
  const malformed = await fetch(url + '/api/v1/probes', { method:'POST', body:'{' });
  assert.equal(malformed.status,400);
});
test('probe normalizes ICAO and returns no calculated aviation values', async () => {
  const r = await post({ departure:' uuee ',arrival:'ulli',aircraft:'fbw-a32nx' });
  assert.equal(r.status,200);
  const result = await r.json();
  assert.equal(result.status,'integration-probe');
  assert.equal(result.departure.icao,'UUEE');
  for(const field of ['route','alternate','fuel','duration','mass']) assert.equal(result[field],null);
  assert.ok(result.warnings.includes('PERFORMANCE_NOT_VALIDATED'));
  assert.ok(result.document,'PDF renderer must work in this test environment');
  assert.equal(result.document.pages,1);
  const pdf = Buffer.from(await (await fetch(url + result.document.pdfUrl)).arrayBuffer());
  assert.equal(pdf.subarray(0,5).toString(),'%PDF-');
  const png = Buffer.from(await (await fetch(url + result.document.pageUrls[0])).arrayBuffer());
  assert.equal(png.subarray(1,4).toString(),'PNG');
  const uploaded = await fetch(url + '/api/v1/documents?icao=EGLL&label=own-chart.pdf',{method:'POST',headers:{'Content-Type':'application/pdf'},body:pdf});
  assert.equal(uploaded.status,201);
  assert.equal((await uploaded.json()).icao,'EGLL');
});
test('PDF import rejects wrong content, malformed PDFs and oversized requests', async () => {
  assert.equal((await fetch(url+'/api/v1/documents?icao=EGLL',{method:'POST',body:'not pdf'})).status,400);
  assert.equal((await fetch(url+'/api/v1/documents?icao=EGLL',{method:'POST',body:'%PDF-broken'})).status,422);
  assert.equal((await fetch(url+'/api/v1/probes',{method:'POST',body:'x'.repeat(9000)})).status,413);
});
test('untrusted browser origins, DNS-rebinding hosts and arbitrary paths are rejected', async () => {
  assert.equal((await fetch(url+'/api/v1/status',{headers:{Origin:'https://untrusted.example'}})).status,403);
  // Undici rewrites Host; use an actual HTTP request to exercise rebinding protection.
  const invalidHost = await new Promise((resolve,reject) => {
    const req=http.get(url+'/api/v1/status',{headers:{Host:'attacker.example'}},res=>{res.resume();resolve(res.statusCode);});
    req.on('error',reject);
  });
  assert.equal(invalidHost,403);
  assert.equal((await fetch(url+'/data/aircraft.json')).status,404);
  const allowed=await fetch(url+'/api/v1/status',{headers:{Origin:'coui://html_ui'}});
  assert.equal(allowed.status,200);
  assert.equal(allowed.headers.get('access-control-allow-origin'),'coui://html_ui');
});
test('connection can be re-established after local service restart', async () => {
  const before=await (await fetch(url+'/api/v1/status')).json();
  server.closeAllConnections();
  const port=server.address().port;
  await new Promise((resolve)=>server.close(resolve));
  await assert.rejects(fetch(url+'/api/v1/status'));
  server=await createServer({database:db,documentRoot:directory});
  await new Promise((resolve)=>server.listen(port,'127.0.0.1',resolve));
  assert.equal((await (await fetch(url+'/api/v1/status')).json()).airportCount,before.airportCount);
});
