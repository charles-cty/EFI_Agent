// Embed unchanged portable code from the pinned zx@lite distribution.
import {readFileSync, writeFileSync} from 'node:fs';
import {join} from 'node:path';

const output = process.argv[2];
if (!output) throw new Error('Usage: node prepare_quickjs.mjs BUILD_DIRECTORY');
const read = name => readFileSync(join('vendor/zx/build', name), 'utf8');
function section(source, first, next) {
  const start = source.indexOf(first);
  const end = source.indexOf(next, start + first.length);
  if (start < 0 || end < 0) throw new Error(`Missing pinned zx section: ${first}`);
  return source.slice(start, end);
}
const util = read('util.cjs');
const core = read('core.cjs');
const vendor = read('vendor-core.cjs');
const portable = [
  section(core, 'var EXIT_CODES =', '// src/log.ts'),
  section(util, 'function quote(arg)', 'function quotePowerShell'),
  section(vendor, 'var isPromiseLike =', 'var assign ='),
  section(vendor, 'var buildCmd =', '// node_modules/zurk/target/esm/spawn.mjs'),
  section(util, 'var once =', 'var proxyOverride ='),
  section(util, 'var getLines =', '/* c8 ignore next'),
].join('\n');
const adapter = readFileSync('guests/quickjs/zx.js', 'utf8');
const boundary = adapter.indexOf('class ProcessPromise');
if (boundary < 0) throw new Error('Missing WASI ProcessPromise adapter boundary');
const source = `(() => {\n${portable}\n${adapter.slice(0, boundary)}\n` +
  section(core, 'var _ProcessOutput =', 'var useBash =') +
  `\n${adapter.slice(boundary)}\n})();\n`;
const bytes = Buffer.from(source);
writeFileSync(join(output, 'zx-bootstrap.h'),
  '// Generated from vendor/zx and guests/quickjs/zx.js.\n' +
  `static const char zx_bootstrap[] = {${[...bytes, 0].join(',')}};\n`);
