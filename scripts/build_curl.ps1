param(
    [Parameter(Mandatory)][string]$WasiSdk,
    [string]$WolfSsl = 'artifacts/wasm-guests/wolfssl',
    [string]$Source = 'artifacts/guest-sources/curl-8.22.0',
    [string]$Output = 'artifacts/wasm-guests',
    [string]$MsysBash = 'C:\msys64\usr\bin\bash.exe',
    [int]$Jobs = 4,
    [switch]$Reconfigure
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$PSNativeCommandArgumentPassing = 'Standard'
$WasiSdk = (Resolve-Path -LiteralPath $WasiSdk).Path
$WolfSsl = (Resolve-Path -LiteralPath $WolfSsl).Path
$Source = (Resolve-Path -LiteralPath $Source).Path
$project = (Get-Location).Path
# curl includes signal.h even when configuration found no signal functions.
# Guard these includes so the WASI SDK does not require signal emulation.
foreach ($relative in @('lib/transfer.c', 'lib/vdns/hostip.c', 'src/tool_main.c')) {
    $path = Join-Path $Source $relative
    $text = [IO.File]::ReadAllText($path)
    $guard = "#if defined(HAVE_SIGNAL) || defined(HAVE_SIGACTION)`n#include <signal.h>`n#endif"
    if (-not $text.Contains($guard)) {
        if ([regex]::Matches($text, '#include <signal\.h>').Count -ne 1) {
            throw "Unexpected signal include in $relative."
        }
        [IO.File]::WriteAllText($path, $text.Replace('#include <signal.h>', $guard))
    }
}
$resolver = Join-Path $Source 'lib/vdns/hostip.c'
$text = [IO.File]::ReadAllText($resolver)
$jumpInclude = '#include <setjmp.h>  /* for sigjmp_buf, sigsetjmp() */'
$jumpGuard = "#ifdef HAVE_SIGSETJMP`n$jumpInclude`n#endif"
if (-not $text.Contains($jumpGuard)) {
    if (-not $text.Contains($jumpInclude)) { throw 'Unexpected resolver jump include.' }
    [IO.File]::WriteAllText($resolver, $text.Replace($jumpInclude, $jumpGuard))
}
$tls = Join-Path $Source 'lib/vtls/wolfssl.c'
$text = [IO.File]::ReadAllText($tls)
$dateBranch = '    else if(detail == ASN_AFTER_DATE_E) {'
$signatureBranch = @'
    else if(detail == ASN_SIG_CONFIRM_E) {
      failf(data, "server verification failed: certificate signature invalid.");
      return CURLE_PEER_FAILED_VERIFICATION;
    }
'@
if (-not $text.Contains('else if(detail == ASN_SIG_CONFIRM_E)')) {
    if (-not $text.Contains($dateBranch)) { throw 'Unexpected wolfSSL verification branches.' }
    $text = $text.Replace($dateBranch, $signatureBranch.Replace("`r`n", "`n") + "`n" + $dateBranch)
}
# A valid CA file with no issuer for this server is a peer verification error.
$noSigner = '        return CURLE_SSL_CACERT_BADFILE;'
$signerStart = $text.IndexOf('    else if(detail == ASN_NO_SIGNER_E)')
if ($signerStart -lt 0) { throw 'Missing wolfSSL issuer verification branch.' }
$signerEnd = $text.IndexOf('    else if(detail == ASN_SIG_CONFIRM_E)', $signerStart)
$signer = $text.Substring($signerStart, $signerEnd - $signerStart)
$text = $text.Replace($signer, $signer.Replace($noSigner, '        return CURLE_PEER_FAILED_VERIFICATION;'))
[IO.File]::WriteAllText($tls, $text)
New-Item -ItemType Directory -Force -Path $Output | Out-Null
$Output = (Resolve-Path -LiteralPath $Output).Path
$build = Join-Path $Output 'curl-build'
New-Item -ItemType Directory -Force -Path $build | Out-Null
$shim = Join-Path $build 'posix-network.o'
& (Join-Path $WasiSdk 'bin/clang.exe') --target=wasm32-wasip1 -O2 -Wall -Wextra -Werror -Iguests/posix/include -Iguests/include -c guests/posix/network.c -o $shim
$cwd = Join-Path $build 'cwd.o'
& (Join-Path $WasiSdk 'bin/clang.exe') --target=wasm32-wasip1 -O2 -Wall -Wextra -Werror -c guests/lib/cwd.c -o $cwd
& (Join-Path $WasiSdk 'bin/llvm-ar.exe') rcs (Join-Path $build 'libefi_posix.a') $shim $cwd
if ($Reconfigure -and (Test-Path -LiteralPath (Join-Path $build 'Makefile'))) {
    Remove-Item -LiteralPath (Join-Path $build 'Makefile')
}
$script = @'
set -eu
export PATH=/usr/bin:$PATH
source=$(cygpath -u "$1")
sdk=$(cygpath -m "$2")
project=$(cygpath -m "$3")
build=$(cygpath -u "$4")
library=$(cygpath -m "$4")
jobs=$5
wolfssl=$(cygpath -m "$6")
cd "$build"
export CC="$sdk/bin/clang.exe --target=wasm32-wasip1"
export AR="$sdk/bin/llvm-ar.exe"
export RANLIB="$sdk/bin/llvm-ranlib.exe"
export CPPFLAGS="-I$project/guests/posix/include -I$project/guests/include -DWOLFSSL_USER_SETTINGS -I$wolfssl/include -include $wolfssl/include/user_settings.h"
export CFLAGS='-O2 -ffunction-sections -fdata-sections'
export LDFLAGS="-L$library"
export LIBS='-Wl,--wrap=chdir -Wl,--wrap=getcwd -Wl,--undefined=chdir -Wl,--undefined=efi_filesystem_initialize -lefi_posix'
export PKG_CONFIG=false
configured=0
if test ! -f Makefile; then
  bash "$source/configure" --build=x86_64-pc-msys --host=wasm32-unknown-wasi \
    --disable-shared --enable-static --disable-threaded-resolver --disable-socketpair --disable-ipv6 \
    --disable-manual --disable-ldap --disable-ldaps --disable-ftp --disable-tftp --disable-ntlm \
    --with-wolfssl="$wolfssl" --without-zlib \
    --without-brotli --without-zstd --without-libpsl --without-libidn2 \
    --without-librtmp --without-libssh2 --without-ca-bundle --without-ca-path \
    > configure.log 2>&1
  configured=1
fi
if test "$configured" = 1; then
  make -C lib clean > build.log 2>&1
  make -C src clean >> build.log 2>&1
fi
make -C lib -j"$jobs" LIBS="$LIBS" libcurl.la > build.log 2>&1
# The externally linked POSIX archive is not a generated Makefile dependency.
rm -f src/curl
make -C src -j"$jobs" LIBS="$LIBS" curl >> build.log 2>&1
'@
& $MsysBash -c $script 'efi-curl-build' $Source $WasiSdk $project $build $Jobs $WolfSsl
Copy-Item -LiteralPath (Join-Path $build 'src/curl') -Destination (Join-Path $Output 'curl.wasm')
