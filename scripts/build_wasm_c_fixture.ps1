param(
    [string]$WasiSdk = $env:WASI_SDK_PATH,
    [string]$Output = 'artifacts/wasm-fixtures'
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$PSNativeCommandArgumentPassing = 'Standard'

if (-not $WasiSdk) {
    throw 'Set WASI_SDK_PATH or pass -WasiSdk with a Windows-local WASI SDK directory.'
}
$Clang = Join-Path $WasiSdk 'bin/clang.exe'
$Linker = Join-Path $WasiSdk 'bin/wasm-ld.exe'
$sysroot = Join-Path (Resolve-Path -LiteralPath $WasiSdk).Path 'share/wasi-sysroot'

New-Item -ItemType Directory -Force -Path $Output | Out-Null
$compile = '--target=wasm32-unknown-unknown', '-ffreestanding', '-O2', '-Wall', '-Wextra',
    '-Werror', '-Iguests/include', '-c'
$captureObject = Join-Path $Output 'capture.o'
$processObject = Join-Path $Output 'process.o'
$shellObject = Join-Path $Output 'shell.o'
$dualObject = Join-Path $Output 'dual.o'
$shellCaptureObject = Join-Path $Output 'shell_capture.o'
try {
    & $Clang @compile scripts/fixtures/wasm/capture.c -o $captureObject
    & $Clang @compile guests/lib/process.c -o $processObject
    $link = '--no-entry', '--export=_start', '--export-memory',
        $captureObject, $processObject, '-o', (Join-Path $Output 'capture.wasm')
    & $Linker @link
    & $Clang @compile guests/lib/shell.c -o $shellObject
    & $Clang @compile scripts/fixtures/wasm/dual.c -o $dualObject
    & $Clang @compile scripts/fixtures/wasm/shell_capture.c -o $shellCaptureObject
    & $Linker --no-entry --export=_start --export-memory $dualObject -o (Join-Path $Output 'dual.wasm')
    & $Linker --no-entry --export=_start --export-memory $shellCaptureObject $shellObject $processObject -o (Join-Path $Output 'shell_capture.wasm')
    & $Clang --target=wasm32-wasip1 "--sysroot=$sysroot" -O2 -Wall -Wextra -Werror scripts/fixtures/wasm/ash_probe.c -o (Join-Path $Output 'ash_probe.wasm')
    & $Clang --target=wasm32-wasip1 "--sysroot=$sysroot" -O2 -Wall -Wextra -Werror -Iguests/include scripts/fixtures/wasm/environment.c guests/lib/process.c -o (Join-Path $Output 'environment.wasm')
    & $Clang --target=wasm32-wasip1 "--sysroot=$sysroot" -O2 -Wall -Wextra -Werror -Iguests/include scripts/fixtures/wasm/descriptor_mapping.c -o (Join-Path $Output 'descriptor_mapping.wasm')
    & $Clang --target=wasm32-wasip1 "--sysroot=$sysroot" -O2 -Wall -Wextra -Werror -Iguests/include scripts/fixtures/wasm/ash_task_input.c guests/lib/process.c -o (Join-Path $Output 'ash_task_input.wasm')
    & $Clang --target=wasm32-wasip1 "--sysroot=$sysroot" -O2 -Wall -Wextra -Werror -Iguests/include scripts/fixtures/wasm/ash_exec.c guests/busybox/exec.c guests/lib/process.c -o (Join-Path $Output 'ash_exec.wasm')
    & $Clang --target=wasm32-wasip1 "--sysroot=$sysroot" -O2 -Wall -Wextra -Werror -Iguests/include scripts/fixtures/wasm/ash_child.c -o (Join-Path $Output 'ash_child.wasm')
    & $Clang --target=wasm32-wasip1 "--sysroot=$sysroot" -O2 -Wall -Wextra -Werror -Iguests/include scripts/fixtures/wasm/filesystem.c guests/lib/process.c -o (Join-Path $Output 'filesystem.wasm')
    & $Clang --target=wasm32-wasip1 "--sysroot=$sysroot" -O2 -Wall -Wextra -Werror -Iguests/include scripts/fixtures/wasm/directory.c guests/lib/process.c -o (Join-Path $Output 'directory.wasm')
    & $Clang --target=wasm32-wasip1 "--sysroot=$sysroot" -O2 -Wall -Wextra -Werror scripts/fixtures/wasm/random.c -o (Join-Path $Output 'random.wasm')
    & $Clang --target=wasm32-wasip1 "--sysroot=$sysroot" -O2 -Wall -Wextra -Werror -Iguests/include scripts/fixtures/wasm/network.c guests/lib/process.c -o (Join-Path $Output 'network.wasm')
    & $Clang --target=wasm32-wasip1 "--sysroot=$sysroot" -O2 -Wall -Wextra -Werror -Iguests/posix/include -Iguests/include scripts/fixtures/wasm/posix_network.c guests/posix/network.c guests/lib/process.c -o (Join-Path $Output 'posix_network.wasm')
} finally {
    Remove-Item -LiteralPath $captureObject, $processObject, $shellObject, $dualObject, $shellCaptureObject -ErrorAction SilentlyContinue
}
