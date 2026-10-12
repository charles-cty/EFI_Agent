param(
    [Parameter(Mandatory)][string]$WasiSdk,
    [string]$Source = 'artifacts/guest-sources/wolfssl-5.9.4-stable',
    [string]$Output = 'artifacts/wasm-guests/wolfssl',
    [int]$Jobs = 4
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$PSNativeCommandArgumentPassing = 'Standard'
$WasiSdk = (Resolve-Path -LiteralPath $WasiSdk).Path.Replace('\', '/')
$Source = (Resolve-Path -LiteralPath $Source).Path
New-Item -ItemType Directory -Force -Path $Output | Out-Null
$Output = (Resolve-Path -LiteralPath $Output).Path
$build = Join-Path $Output 'build'
$settings = (Resolve-Path -LiteralPath 'guests/wolfssl').Path.Replace('\', '/')
$toolchain = Join-Path $WasiSdk 'share/cmake/wasi-sdk-p1.cmake'
$configure = '--fresh', '-S', $Source, '-B', $build, '-G', 'Ninja',
    "-DCMAKE_TOOLCHAIN_FILE=$toolchain", "-DWASI_SDK_PREFIX=$WasiSdk",
    '-DCMAKE_BUILD_TYPE=Release', '-DBUILD_SHARED_LIBS=OFF',
    "-DCMAKE_INSTALL_PREFIX=$Output", "-DCMAKE_C_FLAGS=-I$settings",
    '-DWOLFSSL_USER_SETTINGS=yes', '-DWOLFSSL_SINGLE_THREADED=yes',
    '-DWOLFSSL_ASM=no', '-DWOLFSSL_EXAMPLES=no', '-DWOLFSSL_CRYPT_TESTS=no',
    '-DWOLFSSL_CRYPT_BENCHMARKS=no'
& cmake.exe @configure
& cmake.exe --build $build --target wolfssl --parallel $Jobs
& cmake.exe --install $build
$platform = Join-Path $build 'efi-platform.o'
& (Join-Path $WasiSdk 'bin/clang.exe') --target=wasm32-wasip1 -O2 -Wall -Wextra -Werror -DWOLFSSL_USER_SETTINGS "-I$settings" "-I$Output/include" -c guests/wolfssl/platform.c -o $platform
& (Join-Path $WasiSdk 'bin/llvm-ar.exe') rcs (Join-Path $Output 'lib/libwolfssl.a') $platform
Copy-Item -LiteralPath guests/wolfssl/user_settings.h -Destination (Join-Path $Output 'include/user_settings.h')
Copy-Item -LiteralPath (Join-Path $Source 'COPYING') -Destination (Join-Path $Output 'COPYING')
