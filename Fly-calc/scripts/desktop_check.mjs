import { createRequire } from 'node:module';
import path from 'node:path';
import os from 'node:os';
import { mkdirSync, writeFileSync } from 'node:fs';
import assert from 'node:assert/strict';
const modules=process.env.PLAYWRIGHT_MODULES || path.join(os.homedir(),'.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules');
const require=createRequire(path.join(modules,'playwright/package.json'));
const { _electron }=require('playwright');
mkdirSync('output/screenshots',{recursive:true});
const reachable=async()=>{try{return (await fetch('http://127.0.0.1:8147/api/v1/status')).ok;}catch{return false;}};
const alreadyRunning=await reachable();
const app=await _electron.launch({executablePath:path.resolve('node_modules/electron/dist/electron.exe'),args:[path.resolve('src/desktop.cjs')],cwd:process.cwd(),env:{...process.env,FLYCALC_TEST:'1'}});
try {
  const page=await app.firstWindow();
  const errors=[];
  page.on('pageerror',e=>errors.push(e.message));
  await page.getByText('Локальный сервис подключён',{exact:true}).waitFor();
  assert.equal(await page.locator('select option').count(),6);
  assert.equal(await page.evaluate(()=>typeof window.require),'undefined');
  assert.equal(await app.evaluate(({BrowserWindow})=>BrowserWindow.getAllWindows()[0].webContents.getLastWebPreferences().sandbox),true);
  await page.getByRole('button',{name:'Проверить рейс'}).click();
  await page.getByRole('heading',{name:'Связь проверена'}).waitFor();
  assert.equal(await page.getByText('Не рассчитано',{exact:true}).count(),4);
  assert.deepEqual(errors,[]);
  writeFileSync('evidence/desktop-validation.json',JSON.stringify({status:'passed',platform:'Windows x64',electron:await app.evaluate(()=>process.versions.electron),serviceInitiallyRunning:alreadyRunning,checks:['window loads local UI','API connection','six profiles','probe returns null computations','renderer has no Node require','sandbox enabled'],pageErrors:errors,visualCheck:'browser preview only; Electron tested with hidden window',inSimulator:'not_run'},null,2)+'\n');
  console.log('Windows Electron smoke check passed; no in-simulator checks implied.');
} finally { await app.close(); }
if (!alreadyRunning) {
  await new Promise(resolve=>setTimeout(resolve,500));
  assert.equal(await reachable(),false,'Owned local service should stop when desktop exits');
  const report=JSON.parse((await import('node:fs')).readFileSync('evidence/desktop-validation.json','utf8'));
  report.checks.push('automatic service start','owned service stops on exit');
  writeFileSync('evidence/desktop-validation.json',JSON.stringify(report,null,2)+'\n');
}
