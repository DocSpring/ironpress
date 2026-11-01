// Checks DocSpring's changes to the WebAssembly package (scripts/build-wasm-package.sh builds it in pkg/):
// - Its stack (.cargo/config.toml) takes a document nested as deep as ironpress allows. With Rust's 1 MiB, 23 nested
//   <div>s trapped with "memory access out of bounds", and every later call trapped too.
// - It ships THIRD_PARTY_NOTICES (docspring/notices/build.sh).
import { readFile } from 'node:fs/promises';

import init, { HtmlConverter } from '../pkg/ironpress.js';

await init({ module_or_path: await readFile(new URL('../pkg/ironpress_bg.wasm', import.meta.url)) });

const render = (depth) => {
  const converter = new HtmlConverter();
  return converter.htmlToPdf(`${'<div>'.repeat(depth)}deep${'</div>'.repeat(depth)}`);
};

// ironpress's own limit, MAX_NESTING_DEPTH, comes first: a clean error, not a trap
const deepest = 500;
if (!(render(deepest) instanceof Uint8Array)) throw new Error(`${deepest} nested <div>s didn't render`);
try {
  render(deepest + 1);
  throw new Error(`${deepest + 1} nested <div>s rendered: ironpress's nesting limit changed, update this test`);
} catch (error) {
  if (error instanceof WebAssembly.RuntimeError) throw error;
  if (!String(error.message ?? error).includes('nesting depth')) throw error;
}
// The module still works after that error
render(10);

const manifest = JSON.parse(await readFile(new URL('../pkg/package.json', import.meta.url), 'utf8'));
if (!manifest.files.includes('THIRD_PARTY_NOTICES')) throw new Error('THIRD_PARTY_NOTICES is not in the package');
const notices = await readFile(new URL('../pkg/THIRD_PARTY_NOTICES', import.meta.url), 'utf8');
if (notices !== (await readFile(new URL('../THIRD_PARTY_NOTICES', import.meta.url), 'utf8'))) {
  throw new Error("pkg/THIRD_PARTY_NOTICES isn't the repository's");
}
console.log('WebAssembly package: nesting and notices OK');
