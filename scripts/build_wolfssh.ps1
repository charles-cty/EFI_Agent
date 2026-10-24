param(
    [Parameter(Mandatory)][string]$WasiSdk,
    [string]$WolfSsl = 'artifacts/wasm-guests/wolfssl',
    [string]$Source = 'artifacts/guest-sources/wolfssh-1.6.0-stable',
    [string]$Output = 'artifacts/wasm-guests'
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$PSNativeCommandArgumentPassing = 'Standard'
$WasiSdk = (Resolve-Path -LiteralPath $WasiSdk).Path
$WolfSsl = (Resolve-Path -LiteralPath $WolfSsl).Path
$Source = (Resolve-Path -LiteralPath $Source).Path
$build = Join-Path $Output 'wolfssh-build'
New-Item -ItemType Directory -Force -Path $build | Out-Null
$clang = Join-Path $WasiSdk 'bin/clang.exe'
$flags = '--target=wasm32-wasip1', '-O2', '-ffunction-sections', '-fdata-sections',
    '-DWOLFSSL_USER_SETTINGS', '-DWOLFSSH_USER_IO', '-DNO_WOLFSSH_SERVER', '-DWOLFSSH_SHELL',
    "-I$Source", "-I$WolfSsl/include", '-Iguests/posix/include', '-Iguests/include'
$objects = @()
foreach ($name in @('ssh', 'internal', 'log', 'io', 'port', 'ossh')) {
    $object = Join-Path $build "$name.o"
    & $clang @flags -c (Join-Path $Source "src/$name.c") -o $object
    $objects += $object
}
$library = Join-Path $build 'libwolfssh.a'
& (Join-Path $WasiSdk 'bin/llvm-ar.exe') rcs $library @objects
$link = '-Wall', '-Wextra', '-Werror', 'guests/wolfssh/main.c', 'guests/posix/network.c', 'guests/lib/cwd.c',
    $library, (Join-Path $WolfSsl 'lib/libwolfssl.a'),
    '-Wl,--fatal-warnings', '-Wl,-z,stack-size=1048576', '-Wl,--max-memory=67108864',
    '-Wl,--wrap=chdir', '-Wl,--wrap=getcwd', '-Wl,--undefined=chdir',
    '-o', (Join-Path $Output 'wolfssh.wasm')
& $clang @flags @link
Copy-Item -LiteralPath (Join-Path $Source 'LICENSING') -Destination (Join-Path $Output 'wolfssh.LICENSING')
