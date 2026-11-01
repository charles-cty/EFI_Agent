# jq WASM command

The build prepares jq 1.8.1 from the official release archive. It compiles
the unchanged sources and bundled Oniguruma 6.9.10 with WASI SDK, then links
the shared EFI Agent cwd adapter. The root Windows and Linux build entry
points include jq. Windows uses MSYS2 configure/make, as for curl.

For a component build on Windows:

```powershell
./scripts/build_jq.ps1 -WasiSdk $env:WASI_SDK_PATH
```

On Linux:

```bash
uv run scripts/build_guests.py --component jq --wasi-sdk "$WASI_SDK_PATH"
```

The prepared module is `artifacts/wasm-guests/jq.wasm`. The generated
`programs.json` installs the command as `/bin/jq`. Both boot packages contain
the module in `work` and the jq/Oniguruma notices in `EFI/AGENT/LICENSES`.
The runtime loads the module from the shared workspace when requested.

Use normal jq filters, file input, shell pipes, `-r`, `-c`, `-s`, `-n`,
`--arg`, `--argjson`, `-f`, `-L`, regex functions and `-e` exit statuses.
Module lookup and relative file paths use the process cwd. Commands also
work through Python subprocess and the QuickJS ash template.

```sh
printf '%s\n' '{"items":[1,2,3]}' | jq '.items | add'
jq -n '{basic_repl:env.PYTHON_BASIC_REPL}'
```

jq has a 1 MiB stack and a 64 MiB linear-memory limit. OS-dependent functions
use the existing WASI runtime; these limits apply to jq as well. This port
does not add native libraries, threads, or new host services.

`scripts/test_jq.py` checks the real WASM module with files, pipes, relative
module imports, Unicode regex, large JSON streams and process exit statuses.
