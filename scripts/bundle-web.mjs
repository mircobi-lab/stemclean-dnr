// Crea dist/index.html: un unico file con la pagina, il collegamento JS e il motore Rust incorporato.
// Uso: node scripts/bundle-web.mjs <percorso del file .wasm>
import { readFileSync, writeFileSync, mkdirSync, copyFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const wasmPath = process.argv[2] || join(root, 'target/wasm32-unknown-unknown/release/stemclean_wasm.wasm');
const wasm = readFileSync(wasmPath);
let html = readFileSync(join(root, 'web/index.html'), 'utf8');
const glue = readFileSync(join(root, 'web/stemclean-engine.js'), 'utf8');

const tag = '<script src="stemclean-engine.js"></script>';
if (!html.includes(tag) || !html.includes('__STEMCLEAN_WASM_BASE64__')) throw new Error('index.html non ha i segnaposto attesi');
html = html.replace(tag, () => `<script>\n${glue}</script>`);
html = html.replace('__STEMCLEAN_WASM_BASE64__', () => wasm.toString('base64'));

mkdirSync(join(root, 'dist'), { recursive: true });
writeFileSync(join(root, 'dist/index.html'), html);
copyFileSync(wasmPath, join(root, 'web/stemclean.wasm')); // per aprire web/ con un server di sviluppo
console.log(`dist/index.html pronto (${(html.length / 1024).toFixed(0)} KB, motore Rust ${(wasm.length / 1024).toFixed(0)} KB)`);
