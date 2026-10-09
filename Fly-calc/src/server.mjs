import http from 'node:http';
import { randomUUID } from 'node:crypto';
import { readFileSync, existsSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { readFile, stat } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';
import { DatabaseSync } from 'node:sqlite';

export const ROOT = fileURLToPath(new URL('../', import.meta.url));
const AIRCRAFT = JSON.parse(readFileSync(path.join(ROOT, 'data/aircraft.json'), 'utf8'));
const ICAO = /^[A-Z]{4}$/;
const ID = /^[a-f0-9-]{36}$/;
const TYPES = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.css': 'text/css; charset=utf-8', '.png': 'image/png', '.pdf': 'application/pdf' };

function pdfCommand(python, args, payload) {
  return new Promise((resolve, reject) => {
    const child = spawn(python, [path.join(ROOT, 'scripts/pdf_tools.py'), ...args], { windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
    let stdout = '', stderr = '';
    const timer = setTimeout(() => { child.kill(); reject(new Error('PDF_TIMEOUT')); }, 45000);
    child.on('error', (error) => { clearTimeout(timer); reject(error); });
    child.stdout.on('data', (data) => { stdout += data; });
    child.stderr.on('data', (data) => { stderr += data; });
    child.stdin.on('error', () => {});
    child.on('close', (code) => {
      clearTimeout(timer);
      if (code !== 0) reject(new Error('PDF_ERROR: ' + stderr.slice(-500)));
      else { try { resolve(JSON.parse(stdout)); } catch { reject(new Error('PDF_INVALID_RESPONSE')); } }
    });
    child.stdin.end(payload ? JSON.stringify(payload) : '');
  });
}

async function body(request, limit) {
  const chunks = [];
  let size = 0;
  for await (const chunk of request) {
    size += chunk.length;
    if (size > limit) throw Object.assign(new Error('Превышен допустимый размер запроса'), { status: 413 });
    chunks.push(chunk);
  }
  return Buffer.concat(chunks);
}

export async function createServer(options = {}) {
  const databasePath = options.databasePath || path.join(ROOT, 'data/research.sqlite');
  const db = options.database || (existsSync(databasePath) ? new DatabaseSync(databasePath, { readOnly: true }) : null);
  const metadataFile = path.join(ROOT, 'data/airports-metadata.json');
  const airportMetadata = existsSync(metadataFile) ? JSON.parse(readFileSync(metadataFile, 'utf8')) : {};
  const documentRoot = options.documentRoot || path.join(ROOT, 'output/documents');
  mkdirSync(documentRoot, { recursive: true });
  const python = options.python || process.env.FLYCALC_PYTHON || (existsSync(path.join(ROOT, '.local/runtime.json')) ? JSON.parse(readFileSync(path.join(ROOT, '.local/runtime.json'), 'utf8').replace(/^\uFEFF/, '')).python : 'python');
  let pdfAvailable = false;
  try { await pdfCommand(python, ['check']); pdfAvailable = true; } catch { /* UI advertises unavailable PDFs. */ }
  const jobs = new Map();
  let pdfJobs = 0;
  const airport = (icao) => db ? db.prepare('SELECT icao, name, country, latitude, longitude FROM airports WHERE icao=?').get(icao) : null;

  const server = http.createServer(async (req, res) => {
    const json = (code, value) => { res.writeHead(code, { 'Content-Type': 'application/json; charset=utf-8' }); res.end(JSON.stringify(value)); };
    try {
      const host = req.headers.host || '';
      if (!/^(127\.0\.0\.1|localhost)(:\d+)?$/.test(host)) return json(403, { error: 'Invalid local host' });
      const origin = req.headers.origin;
      // Coherent may send coui://html_ui or a null origin. This prototype has no simulator control APIs.
      if (origin && !/^http:\/\/(127\.0\.0\.1|localhost)(:\d+)?$/.test(origin) && origin !== 'coui://html_ui' && origin !== 'null') return json(403, { error: 'Origin is not allowed' });
      if (origin) res.setHeader('Access-Control-Allow-Origin', origin);
      res.setHeader('Vary', 'Origin');
      res.setHeader('Access-Control-Allow-Methods', 'GET, POST, OPTIONS');
      res.setHeader('Access-Control-Allow-Headers', 'Content-Type');
      res.setHeader('X-Content-Type-Options', 'nosniff');
      if (req.method === 'OPTIONS') { res.writeHead(204); return res.end(); }
      const url = new URL(req.url, 'http://127.0.0.1');
      if (req.method === 'GET' && url.pathname === '/api/v1/status') return json(200, {
        apiVersion: '1', worldRoutingReady: false, airportCount: db ? db.prepare('SELECT COUNT(*) AS count FROM airports').get().count : 0,
        documentsAvailable: pdfAvailable,
        sources: { airports: 'OurAirports · snapshot ' + (airportMetadata.snapshotDate || 'date unconfirmed'), enrouteCycle: 'FlightGear 2013.10 · примерно ' + (new Date().getFullYear() - 2013) + ' лет', procedures: 'Global source not confirmed' },
        fenix: { cockpitIntegration: 'unconfirmed', companionUrl: '/?display=fenix' },
      });
      if (req.method === 'GET' && url.pathname === '/api/v1/aircraft') return json(200, AIRCRAFT);
      if (req.method === 'GET' && url.pathname === '/api/v1/airports') {
        const q = (url.searchParams.get('q') || '').trim().toUpperCase();
        if (q.length < 2 || q.length > 80) return json(400, { error: 'Поиск: от 2 до 80 символов' });
        return json(200, db ? db.prepare('SELECT icao, name, country, latitude, longitude FROM airports WHERE icao LIKE ? OR name LIKE ? ORDER BY icao LIMIT 20').all(q + '%', '%' + q + '%') : []);
      }
      if (req.method === 'POST' && url.pathname === '/api/v1/flights') return json(503, { error: 'Полная мировая база процедур и профили самолётов не подтверждены. Расчёт недоступен.', code: 'FEASIBILITY_GATE_CLOSED' });
      if (req.method === 'POST' && url.pathname === '/api/v1/probes') {
        let input;
        try { input = JSON.parse((await body(req, 8192)).toString('utf8')); } catch (e) { if (e.status) throw e; return json(400, { error: 'Ожидается JSON' }); }
        if (!input || typeof input !== 'object' || typeof input.departure !== 'string' || typeof input.arrival !== 'string') return json(400, { error: 'Укажите два ICAO' });
        const dep = input.departure.trim().toUpperCase(), arr = input.arrival.trim().toUpperCase();
        if (!ICAO.test(dep) || !ICAO.test(arr)) return json(400, { error: 'ICAO должен состоять из четырёх латинских букв' });
        if (dep === arr) return json(400, { error: 'Аэропорты вылета и прилёта должны различаться' });
        const aircraft = AIRCRAFT.find((a) => a.id === input.aircraft);
        if (!aircraft) return json(400, { error: 'Неизвестный самолёт' });
        if (!db) return json(503, { error: 'Справочник аэропортов ещё не загружен' });
        const departure = airport(dep), arrival = airport(arr);
        if (!departure || !arrival) return json(404, { error: 'Аэропорт не найден: ' + (!departure ? dep : arr) });
        const result = { id: randomUUID(), status: 'integration-probe', departure, arrival, aircraft,
          route: null, alternate: null, fuel: null, duration: null, mass: null, document: null,
          warnings: ['INTEGRATION_TEST_ONLY', 'GLOBAL_PROCEDURES_NOT_CONFIRMED', 'PERFORMANCE_NOT_VALIDATED', 'RAD_NOTAM_NOT_CHECKED'] };
        if (pdfAvailable && pdfJobs < 2) {
          pdfJobs++;
          try { result.document = await makeDocument({ id: result.id, label: dep + '-' + arr + ' / integration test', icao: dep, kind: 'probe' }, null, { departure: dep, arrival: arr }); }
          catch { result.warnings.push('PDF_UNAVAILABLE'); }
          finally { pdfJobs--; }
        }
        jobs.set(result.id, result);
        // A probe is transient; no production flight or dispatch record is stored.
        if (jobs.size > 100) jobs.delete(jobs.keys().next().value);
        return json(200, result);
      }
      if (req.method === 'GET' && url.pathname === '/api/v1/documents') {
        const { readdir } = await import('node:fs/promises');
        const entries = await readdir(documentRoot, { withFileTypes: true });
        const items = [];
        for (const entry of entries) if (entry.isDirectory() && ID.test(entry.name)) {
          try { items.push(JSON.parse(await readFile(path.join(documentRoot, entry.name, 'metadata.json'), 'utf8'))); } catch { /* Ignore incomplete jobs. */ }
        }
        return json(200, items);
      }
      if (req.method === 'POST' && url.pathname === '/api/v1/documents') {
        if (!pdfAvailable) return json(503, { error: 'Для PDF нужны Python, reportlab и pypdfium2' });
        if (pdfJobs >= 2) return json(429, { error: 'Обрабатываются другие документы. Повторите запрос позже.' });
        const icao = (url.searchParams.get('icao') || '').trim().toUpperCase();
        if (!ICAO.test(icao)) return json(400, { error: 'Укажите ICAO аэропорта чарта' });
        const buffer = await body(req, 20 * 1024 * 1024);
        if (buffer.subarray(0, 5).toString('ascii') !== '%PDF-') return json(400, { error: 'Выбранный файл не является PDF' });
        const label = (url.searchParams.get('label') || 'User PDF').slice(0, 150);
        pdfJobs++;
        try { return json(201, await makeDocument({ id: randomUUID(), icao, label, kind: 'chart' }, buffer)); }
        catch { return json(422, { error: 'Не удалось прочитать PDF. Допускаются незашифрованные PDF от 1 до 50 страниц.' }); }
        finally { pdfJobs--; }
      }
      let filename;
      const docMatch = url.pathname.match(/^\/api\/v1\/documents\/([a-f0-9-]{36})\/(document\.pdf|page-[1-9]\d?\.png)$/);
      if (req.method === 'GET' && docMatch) filename = path.join(documentRoot, docMatch[1], docMatch[2]);
      else if (req.method === 'GET' && ['/', '/fenix', '/index.html', '/app.js', '/app.css'].includes(url.pathname)) filename = path.join(ROOT, 'public', ['/', '/fenix'].includes(url.pathname) ? 'index.html' : url.pathname.slice(1));
      if (filename && existsSync(filename)) {
        const info = await stat(filename);
        if (!info.isFile()) return json(404, { error: 'Not found' });
        res.writeHead(200, { 'Content-Type': TYPES[path.extname(filename)] || 'application/octet-stream', 'Content-Length': info.size, 'Cache-Control': 'no-store' });
        return res.end(await readFile(filename));
      }
      return json(404, { error: 'Not found' });
    } catch (error) { if (!res.headersSent) json(error.status || 500, { error: error.status ? error.message : 'Ошибка локального сервиса' }); else res.end(); }
  });
  server.requestTimeout = 60000;
  server.headersTimeout = 10000;
  server.on('close', () => { if (db && !options.database) db.close(); });

  async function makeDocument(metadata, bytes, payload) {
    const directory = path.join(documentRoot, metadata.id);
    mkdirSync(directory, { recursive: true });
    const pdf = path.join(directory, 'document.pdf');
    try {
      if (bytes) writeFileSync(pdf, bytes);
      const rendered = await pdfCommand(python, [bytes ? 'render' : 'probe', '--pdf', pdf, '--pages', directory], payload);
      const prefix = '/api/v1/documents/' + metadata.id + '/';
      const result = { ...metadata, pages: rendered.pages, pdfUrl: prefix + 'document.pdf', pageUrls: Array.from({ length: rendered.pages }, (_, i) => prefix + 'page-' + (i + 1) + '.png') };
      writeFileSync(path.join(directory, 'metadata.json'), JSON.stringify(result, null, 2));
      return result;
    } catch (e) {
      // Only a UUID directory created by this job, inside the explicit output root, may be removed.
      const resolved = path.resolve(directory), parent = path.resolve(documentRoot);
      if (ID.test(metadata.id) && resolved.startsWith(parent + path.sep)) rmSync(resolved, { recursive: true, force: true });
      throw e;
    }
  }
  return server;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const server = await createServer();
  server.listen(8147, '127.0.0.1', () => console.log('Fly-calc: http://127.0.0.1:8147'));
}
