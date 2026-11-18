// Checks DocSpring's changes to the WebAssembly package (scripts/build-wasm-package.sh builds it in pkg/):
// - A document nested deeper than maxNestingDepth fails with a clean error, and the module keeps working after it.
// - Its stack (.cargo/config.toml) takes the nesting DocSpring allows (110 levels of flex containers, the
//   deepest-stacked layout). With Rust's 1 MiB, 23 nested <div>s trapped with "memory access out of bounds", and every
//   later call trapped too.
// - The sanitizer's own limit (500) still applies whatever maxNestingDepth says.
// - It ships THIRD_PARTY_NOTICES (docspring/notices/build.sh).
import { readFile } from 'node:fs/promises';

import init, { HtmlConverter } from '../pkg/ironpress.js';

await init({ module_or_path: await readFile(new URL('../pkg/ironpress_bg.wasm', import.meta.url)) });

const nested = (open, depth) => `${open.repeat(depth)}deep${'</div>'.repeat(depth)}`;
const render = (html, maxNestingDepth) => {
  const converter = new HtmlConverter();
  if (maxNestingDepth !== undefined) converter.maxNestingDepth(maxNestingDepth);
  return converter.htmlToPdf(html);
};
const expectError = (html, maxNestingDepth, message) => {
  try {
    render(html, maxNestingDepth);
  } catch (error) {
    if (error instanceof WebAssembly.RuntimeError) throw error;
    if (!String(error.message ?? error).includes(message)) throw error;
    return;
  }
  throw new Error(`rendered a document that should fail with "${message}"`);
};

if (!(render(nested('<div>', 31)) instanceof Uint8Array)) throw new Error("31 nested <div>s didn't render");
expectError(nested('<div>', 32), undefined, 'nested more than 31 deep');
// The module still works after that error
render(nested('<div>', 10));

const flex = '<div style="display: flex">';
if (!(render(nested(flex, 110), 110) instanceof Uint8Array)) {
  throw new Error("110 nested flex containers didn't render with maxNestingDepth(110)");
}
expectError(nested('<div>', 501), 1000, 'nesting depth');
render(nested('<div>', 10));

const manifest = JSON.parse(await readFile(new URL('../pkg/package.json', import.meta.url), 'utf8'));
if (!manifest.files.includes('THIRD_PARTY_NOTICES')) throw new Error('THIRD_PARTY_NOTICES is not in the package');
const notices = await readFile(new URL('../pkg/THIRD_PARTY_NOTICES', import.meta.url), 'utf8');
if (notices !== (await readFile(new URL('../THIRD_PARTY_NOTICES', import.meta.url), 'utf8'))) {
  throw new Error("pkg/THIRD_PARTY_NOTICES isn't the repository's");
}
console.log('WebAssembly package: nesting, stack and notices OK');
