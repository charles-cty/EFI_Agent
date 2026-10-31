// WASI process adapter. Quoting, interpolation and ProcessOutput are upstream zx.
const utf8Buffer = text => new Uint8Array(__utf8(text));
const import_node_buffer = {Buffer: {from: utf8Buffer}};
const inspect = value => String(value);
inspect.custom = Symbol('inspect');
const import_node_util2 = {inspect};
const import_vendor_core2 = {chalk: {
  green: value => value, red: value => value, grey: value => value,
}};
const import_util = {once, getLines, iteratorToArray,
  bufArrJoin: values => values.join('')};
const bufToString = value => String(value);
const __yieldStar = value => value;
const DLMTR = /\r?\n/;
// The unchanged upstream ProcessOutput iterator reads this private setting.
const $ = {delimiter: DLMTR};

class ProcessPromise {
  constructor(command, options) {
    this.options = options;
    this.promise = Promise.resolve(command).then(async text => {
      const started = Date.now();
      let result;
      try {
        result = await __shell(text, options.input == null ? '' : String(options.input));
      } catch (error) {
        // The host rejects completed nonzero commands with their captured bytes.
        if (typeof error.exitCode !== 'number') throw error;
        result = error;
      }
      const output = new ProcessOutput(result.exitCode, null,
        result.stdout, result.stderr, result.stdall, '', Date.now() - started,
        result.captureError ? Error(`Command capture failed: ${result.captureError}`) : null);
      output.captureError = result.captureError;
      if (options.verbose && !options.quiet) {
        if (output.stdout) print(output.stdout);
        if (output.stderr) console.error(output.stderr);
      }
      if ((!output.ok || output.captureError) && !options.nothrow) throw output;
      return output;
    });
  }
  then(resolve, reject) { return this.promise.then(resolve, reject); }
  catch(reject) { return this.promise.catch(reject); }
  finally(callback) { return this.promise.finally(callback); }
  nothrow(value = true) { this.options.nothrow = value; return this; }
  quiet(value = true) { this.options.quiet = value; return this; }
  verbose(value = true) { this.options.verbose = value; return this; }
  text(encoding = 'utf8') { return this.then(output => output.text(encoding)); }
  json() { return this.then(output => output.json()); }
  lines(delimiter) { return this.then(output => output.lines(delimiter)); }
  buffer() { return this.then(output => output.buffer()); }
  blob(type) { return this.then(output => output.blob(type)); }
  // A bounded completed-output pipe. Live OS streams are not available in WASI.
  get pipe() {
    const stdout = this._pipe.bind(this, 'stdout');
    return Object.assign(stdout, {stdout,
      stderr: this._pipe.bind(this, 'stderr'),
      stdall: this._pipe.bind(this, 'stdall')});
  }
  _pipe(stream, pieces, ...args) {
    if (!isStringLiteral(pieces, ...args)) throw TypeError('pipe requires a command template');
    return makeCommand(pieces, args, {...this.options,
      input: this.then(output => output[stream])});
  }
}

function checkedQuote(value, quoting) {
  if (value.includes('\0')) throw TypeError('NUL in shell argument');
  return quoting(value);
}

const supportedOptions = new Set([
  'cwd', 'env', 'input', 'nothrow', 'quiet', 'verbose', 'prefix', 'postfix', 'quote',
]);
function checkOptions(options) {
  for (const key of Object.keys(options)) {
    if (!supportedOptions.has(key)) throw TypeError(`Unsupported WASI zx option: ${key}`);
  }
}

function makeCommand(pieces, args, options) {
  if (pieces.some(piece => piece == null)) throw TypeError('Malformed command template escape');
  // Snapshot settings before asynchronous interpolation or concurrent execution.
  options = {...options, env: options.env == null ? null : {...options.env}};
  const quoting = value => checkedQuote(value, options.quote);
  const command = buildCmd(quoting, pieces, args);
  return new ProcessPromise(Promise.all([command, options.input]).then(([text, input]) => {
    if (input != null && typeof input !== 'string') throw TypeError('WASI zx input requires text');
    options.input = input;
    let prelude = '';
    if (options.cwd != null) prelude += `cd ${quoting(String(options.cwd))} || exit $?;\n`;
    if (options.env != null) {
      for (const [name, value] of Object.entries(options.env)) {
        if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name)) throw TypeError(`Invalid environment name: ${name}`);
        prelude += `export ${name}=${quoting(String(value))};\n`;
      }
    }
    return prelude + options.prefix + text + options.postfix;
  }), options);
}

function createShell(options) {
  const invoke = (pieces, ...args) => {
    if (isStringLiteral(pieces, ...args)) return makeCommand(pieces, args, {...options});
    if (args.length || pieces == null || typeof pieces !== 'object' || Array.isArray(pieces))
      throw TypeError('ash requires a command template or options');
    checkOptions(pieces);
    return createShell({...options, ...pieces});
  };
  return new Proxy(invoke, {
    get(target, name) {
      if (supportedOptions.has(name)) return options[name];
      if (name === 'sync' || name === 'kill' || name === 'timeout')
        throw TypeError(`Unsupported WASI zx API: ${String(name)}`);
      return Reflect.get(target, name);
    },
    set(target, name, value) {
      if (!supportedOptions.has(name)) throw TypeError(`Unsupported WASI zx option: ${String(name)}`);
      options[name] = value;
      return true;
    },
  });
}

// QuickJS has no Node Buffer encodings. Keep errors explicit at this boundary.
ProcessOutput.prototype.text = function(encoding = 'utf8') {
  if (encoding !== 'utf8' && encoding !== 'utf-8')
    throw RangeError(`Unsupported WASI text encoding: ${encoding}`);
  return this.toString();
};
globalThis.ash = createShell({quote, prefix: '', postfix: '', quiet: false,
  verbose: false, nothrow: false, cwd: null, env: null, input: ''});
globalThis.quote = quote;
globalThis.ProcessOutput = ProcessOutput;
globalThis.ProcessPromise = ProcessPromise;
