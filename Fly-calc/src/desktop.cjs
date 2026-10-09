const { app, BrowserWindow, shell } = require('electron');
const path = require('node:path');
const fs = require('node:fs');
const { spawn } = require('node:child_process');
let service;
const address = 'http://127.0.0.1:8147';

if (!app.requestSingleInstanceLock()) app.quit();
else app.whenReady().then(async () => {
  const configFile = path.join(__dirname, '../.local/runtime.json');
  const config = fs.existsSync(configFile) ? JSON.parse(fs.readFileSync(configFile, 'utf8').replace(/^\uFEFF/, '')) : {};
  const healthy = async () => { try { const r = await fetch(address + '/api/v1/status'); return r.ok && (await r.json()).apiVersion === '1'; } catch { return false; } };
  if (!await healthy()) {
    service = spawn(config.node || 'node', [path.join(__dirname, 'server.mjs')], { windowsHide: true, env: { ...process.env, FLYCALC_PYTHON: config.python || 'python' }, stdio: 'ignore' });
    service.on('error', () => {});
    for (let i = 0; i < 100 && !await healthy(); i++) await new Promise((resolve) => setTimeout(resolve, 100));
  }
  const win = new BrowserWindow({ width: 1430, height: 1000, minWidth: 800, minHeight: 650, title: 'Fly-calc', backgroundColor: '#0b1220', autoHideMenuBar: true, show: process.env.FLYCALC_TEST !== '1',
    webPreferences: { nodeIntegration: false, contextIsolation: true, sandbox: true } });
  win.webContents.setWindowOpenHandler(({ url }) => { if (url.startsWith(address + '/api/v1/documents/')) shell.openExternal(url); return { action: 'deny' }; });
  win.webContents.on('will-navigate', (event, url) => { if (!url.startsWith(address + '/')) event.preventDefault(); });
  if (await healthy()) await win.loadURL(address);
  else await win.loadURL('data:text/html;charset=utf-8,' + encodeURIComponent('<h1>Fly-calc</h1><p>Не удалось запустить локальный сервис. Проверьте README и порт 8147.</p>'));
});
app.on('window-all-closed', () => app.quit());
app.on('before-quit', () => { if (service) service.kill(); });
