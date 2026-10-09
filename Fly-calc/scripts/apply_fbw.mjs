import { readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root = fileURLToPath(new URL('../', import.meta.url));
const [simulator, checkout, mode = '--check'] = process.argv.slice(2);
const lock = JSON.parse(readFileSync(path.join(root, 'integrations/fbw/lock.json'), 'utf8'));
if (!lock[simulator] || !checkout || !['--check','--apply'].includes(mode)) throw new Error('Usage: node scripts/apply_fbw.mjs msfs2020|msfs2024 CHECKOUT --check|--apply');
const git = (args) => { const r=spawnSync('git',['-C',path.resolve(checkout),...args],{encoding:'utf8'}); if(r.status!==0) throw new Error(r.stderr || r.stdout); return r.stdout.trim(); };
if (git(['rev-parse','HEAD']) !== lock[simulator].sha) throw new Error('Checkout must be at pinned revision '+lock[simulator].sha);
const patch=path.join(root,lock[simulator].patch);
git(['apply','--check',patch]);
if(mode==='--apply') { git(['apply',patch]); console.log('Fly-calc source patch applied. This is not a compiled aircraft package.'); }
else console.log('Patch applies cleanly to the pinned revision.');
