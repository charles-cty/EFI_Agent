param(
    [string]$WasiSdk = $env:WASI_SDK_PATH,
    [string]$Source = 'artifacts/guest-sources/quickjs-2026-06-04',
    [string]$Output = 'artifacts/wasm-guests'
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$PSNativeCommandArgumentPassing = 'Standard'
if (-not $WasiSdk) {
    throw 'Set WASI_SDK_PATH or pass -WasiSdk.'
}
$WasiSdk = (Resolve-Path -LiteralPath $WasiSdk).Path
$clang = Join-Path $WasiSdk 'bin/clang.exe'
$sysroot = Join-Path $WasiSdk 'share/wasi-sysroot'
$build = Join-Path $Output 'quickjs-build'
New-Item -ItemType Directory -Force $build | Out-Null
& node.exe scripts/prepare_quickjs.mjs $build
# Patch only the copied engine source. WASI has no threads in this port.
$engine = [IO.File]::ReadAllText((Join-Path $Source 'quickjs.c'))
$atomics = "#if !defined(__EMSCRIPTEN__)`n#define CONFIG_ATOMICS"
if (-not $engine.Contains($atomics)) {
    throw 'QuickJS Atomics guard changed; inspect the port patch.'
}
$engine = $engine.Replace($atomics, "#if !defined(__EMSCRIPTEN__) && !defined(__wasi__)`n#define CONFIG_ATOMICS")
$engine = "#include <malloc.h>`n" + $engine
[IO.File]::WriteAllText((Join-Path $build 'quickjs.c'), $engine)
$dtoa = [IO.File]::ReadAllText((Join-Path $Source 'dtoa.c'))
# dtoa includes setjmp.h but does not call setjmp or longjmp.
$dtoa = $dtoa.Replace("#include <setjmp.h>`n", '')
[IO.File]::WriteAllText((Join-Path $build 'dtoa.c'), $dtoa)
$sources = @((Join-Path $build 'quickjs.c'), (Join-Path $build 'dtoa.c'))
foreach ($name in @('libregexp.c', 'libunicode.c', 'cutils.c')) {
    $sources += Join-Path $Source $name
}
$sources += 'guests/quickjs/main.c', 'guests/lib/process.c', 'guests/lib/shell.c', 'guests/lib/cwd.c'
$version = ([IO.File]::ReadAllText((Join-Path $Source 'VERSION'))).Trim()
$compile = '--target=wasm32-wasip1', "--sysroot=$sysroot", '-O2', '-D_GNU_SOURCE',
    "-DCONFIG_VERSION=`"$version`"", "-I$Source", "-I$build", '-Iguests/include',
    '-Wl,-z,stack-size=1048576', '-Wl,--max-memory=67108864',
    '-Wl,--wrap=chdir', '-Wl,--wrap=getcwd', '-Wl,--undefined=chdir',
    '-Wl,--wrap=stat', '-Wl,--wrap=lstat', '-Wl,--wrap=fstatat', '-lm'
& $clang @compile @sources -o (Join-Path $Output 'quickjs.wasm')
Copy-Item -LiteralPath (Join-Path $Source 'LICENSE') -Destination (Join-Path $Output 'quickjs.LICENSE')
Copy-Item -LiteralPath 'vendor/zx/LICENSE' -Destination (Join-Path $Output 'zx.LICENSE')
Copy-Item -LiteralPath 'vendor/zx/build/3rd-party-licenses' -Destination (Join-Path $Output 'zx-third-party.LICENSE')
