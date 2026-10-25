"""Real ash oracles for the vendored zx command and output integration."""

ZX_CHECK = r'''
var checkZx = (condition, detail) => { if (!condition) throw Error(detail); };
var wordsZx = ['', 'a b', "a'b", '"quoted"', '\\', '$HOME', '$(touch /zx-injected)',
  '`touch /zx-injected`', ';touch /zx-injected', '*?[a]', '-n', '中λ😀',
  'line\nnext', '\t\r\f\v', 'trailing\n', 'a=b', '#comment', '\u001b'];
var stateZx = 731;
var alphabetSizeZx = wordsZx.length;
for (var iZx = 0; iZx < 80; ++iZx) {
  var wordZx = '';
  for (var jZx = 0; jZx < 8; ++jZx) {
    stateZx = (Math.imul(stateZx, 1664525) + 1013904223) >>> 0;
    wordZx += "a '"[stateZx % 3] + wordsZx[stateZx % alphabetSizeZx];
  }
  wordsZx.push(wordZx);
}
var quotedZx = await $`printf '%s\\000' ${wordsZx}`;
checkZx(quotedZx.stdout === wordsZx.map(word => word + '\0').join(''), 'Quote byte oracle');
await $`test ! -e /zx-injected`;
var promisedZx = await $`printf '%s' ${Promise.resolve('a b')}`;
checkZx(promisedZx.stdout === 'a b', 'Promise interpolation');
var priorZx = await $`printf 'one\n\n'`;
checkZx((await $`printf '%s' ${priorZx}`).stdout === 'one\n', 'Output interpolation trims one newline');
var failedZx = await $`echo failure >&2; exit 7`.nothrow();
checkZx(failedZx instanceof Error && failedZx instanceof ProcessOutput &&
  !failedZx.ok && failedZx.exitCode === 7 && failedZx.stderr === 'failure\n', 'ProcessOutput failure');
checkZx(String(failedZx.message).includes('exit code: 7'), 'Readable failure');
var caughtZx = false;
try { await $`exit 9`; } catch (error) { caughtZx = error instanceof ProcessOutput && error.exitCode === 9; }
checkZx(caughtZx, 'Default rejection');
checkZx((await $`printf '{"value":731}'`.json()).value === 731, 'JSON output');
checkZx(JSON.stringify(await $`printf 'a\nb\n'`.lines()) === '["a","b"]', 'Lines output');
checkZx(await $`printf text`.text() === 'text', 'Text output');
var bytesZx = await $`printf '中\\000'`.buffer();
checkZx(bytesZx instanceof Uint8Array && bytesZx.join(',') === '228,184,173,0', 'UTF8 byte output');
var configuredZx = $({cwd: '/tmp', env: {ZX_VALUE: 'a b\n;$(false)'}, input: '中\0input\n'});
var inputZx = await configuredZx`printf '%s\\000' "$PWD" "$ZX_VALUE"; cat`;
checkZx(inputZx.stdout === '/tmp\0a b\n;$(false)\0中\0input\n', 'cwd/env/input');
checkZx((await $`printf '%s' "\${ZX_VALUE-unset}"`).stdout === 'unset', 'Environment isolation');
checkZx((await $({input: 'x'.repeat(196613)})`cat`.pipe`wc -c`).stdout.trim() === '196613', 'Large completed pipe');
checkZx(await $`printf err >&2`.pipe.stderr`cat`.text() === 'err', 'Stderr pipe');
checkZx(await $`printf all`.pipe.stdall`cat`.text() === 'all', 'Combined pipe');
var malformedZx = false;
try { await $`\unicode`; } catch (error) { malformedZx = error instanceof TypeError; }
checkZx(malformedZx, 'Malformed template rejection');
var combinedZx = await $`printf first; sleep 0.01; printf second >&2; sleep 0.01; printf third`;
checkZx(combinedZx.stdall === 'firstsecondthird', 'Combined capture read order');
var parallelZx = await Promise.all([$({input:'left'})`cat`, $({input:'right'})`cat`]);
checkZx(parallelZx.map(output => output.stdout).join(',') === 'left,right', 'Concurrent input isolation');
for (var optionZx of ['timeout', 'sync', 'shell', 'unknown']) {
  var unsupportedZx = false;
  try { $({[optionZx]: true}); } catch (error) { unsupportedZx = error instanceof TypeError; }
  checkZx(unsupportedZx, 'Unsupported option ' + optionZx);
}
var nulZx = false;
try { await $`printf '%s' ${'a\0b'}`; } catch (error) { nulZx = error instanceof TypeError; }
checkZx(nulZx, 'NUL argument rejection');
var overflowZx = false;
try { await $({input:'x'.repeat(524289)})`cat`; } catch (error) { overflowZx = true; }
checkZx(overflowZx, 'Input limit');
checkZx((await $({input:'x'.repeat(524288)})`wc -c`).stdout.trim() === '524288', 'Input limit inclusive');
print('ZX_PORT_VERIFIED');
'''
