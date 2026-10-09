import { build } from 'esbuild';
import { spawnSync } from 'node:child_process';
const checked = spawnSync(process.execPath, ['node_modules/typescript/bin/tsc', '--noEmit'], { stdio: 'inherit' });
if (checked.status !== 0) process.exit(checked.status || 1);
await build({ entryPoints: ['src/preview.tsx'], bundle: true, outfile: 'public/app.js', target: 'es2017', format: 'iife', minify: true, sourcemap: false, define: { 'process.env.NODE_ENV': '"production"' } });
console.log('TypeScript checked; shared flyPad/desktop interface built for ES2017.');
