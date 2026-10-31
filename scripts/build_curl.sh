#!/usr/bin/env bash
# Compile unchanged curl sources with the native WASI SDK.
set -eu
if [[ "$7" == windows ]]; then
    export PATH=/usr/bin:$PATH
    source=$(cygpath -u "$1")
    sdk=$(cygpath -m "$2")
    project=$(cygpath -m "$3")
    build=$(cygpath -u "$4")
    library=$(cygpath -m "$4")
    wolfssl=$(cygpath -m "$6")
    suffix=.exe
    build_args=(--build=x86_64-pc-msys)
else
    source=$1
    sdk=$2
    project=$3
    build=$4
    library=$4
    wolfssl=$6
    suffix=
    build_args=()
fi
jobs=$5
cd "$build"
export CC="$sdk/bin/clang$suffix --target=wasm32-wasip1"
export AR="$sdk/bin/llvm-ar$suffix"
export RANLIB="$sdk/bin/llvm-ranlib$suffix"
# Expose SDK declarations while keeping signal and jump code disabled.
export CPPFLAGS="-D_WASI_EMULATED_SIGNAL -I$project/guests/posix/include -I$project/guests/include -DWOLFSSL_USER_SETTINGS -I$wolfssl/include -include $wolfssl/include/user_settings.h"
export CFLAGS='-O2 -ffunction-sections -fdata-sections -mexception-handling'
export LDFLAGS="-L$library"
export LIBS='-Wl,--wrap=chdir -Wl,--wrap=getcwd -Wl,--wrap=stat -Wl,--wrap=lstat -Wl,--wrap=fstatat -Wl,--undefined=chdir -Wl,--undefined=efi_filesystem_initialize -lefi_posix'
export PKG_CONFIG=false
export curl_disallow_signal=yes
export curl_disallow_sigaction=yes
export curl_disallow_sigsetjmp=yes
export curl_disallow_alarm=yes
configured=0
if test ! -f Makefile; then
    bash "$source/configure" "${build_args[@]}" --host=wasm32-unknown-wasi \
        --disable-shared --enable-static --disable-threaded-resolver --disable-socketpair --disable-ipv6 \
        --disable-manual --disable-ldap --disable-ldaps --disable-ftp --disable-tftp --disable-ntlm \
        --with-wolfssl="$wolfssl" --without-zlib \
        --without-brotli --without-zstd --without-libpsl --without-libidn2 \
        --without-libssh2 --without-ca-bundle --without-ca-path \
        > configure.log 2>&1
    configured=1
fi
if test "$configured" = 1; then
    make -C lib clean > build.log 2>&1
    make -C src clean >> build.log 2>&1
fi
make -C lib -j"$jobs" LIBS="$LIBS" libcurl.la > build.log 2>&1
# The external POSIX archive is not a generated Makefile dependency.
rm -f src/curl
make -C src -j"$jobs" LIBS="$LIBS" curl >> build.log 2>&1
