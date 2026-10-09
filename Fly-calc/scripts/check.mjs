import { spawnSync } from 'node:child_process';
import { readdirSync, readFileSync, mkdirSync, cpSync } from 'node:fs';
import path from 'node:path';
const run=(exe,args,options={})=>{const r=spawnSync(exe,args,{stdio:'inherit',...options});if(r.status!==0)process.exit(r.status||1);};
for (const dir of ['src','scripts','tests']) for (const file of readdirSync(dir)) if (/\.(mjs|cjs)$/.test(file)) run(process.execPath,['--check',path.join(dir,file)]);
run(process.execPath,['node_modules/typescript/bin/tsc','--noEmit']);
const lock=JSON.parse(readFileSync('integrations/fbw/lock.json','utf8'));
for(const simulator of Object.keys(lock)) {
  const destination=path.resolve('output/patch-check',simulator);
  mkdirSync(destination,{recursive:true});
  // Copy only the two actual upstream files. No source is fabricated for this check.
  for(const relative of Object.keys(lock[simulator].sourceHashes)) {
    const target=path.join(destination,relative);mkdirSync(path.dirname(target),{recursive:true});
    cpSync(path.join('evidence/upstream',simulator,relative),target);
  }
  run('git',['apply','--check',path.resolve(lock[simulator].patch)],{cwd:destination});
}
console.log('Syntax, strict TypeScript, and both pinned-source patches checked. Full FBW builds and MSFS checks are separate.');
