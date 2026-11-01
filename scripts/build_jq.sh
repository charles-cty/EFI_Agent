#!/usr/bin/env bash
# Compile unchanged jq and its bundled Oniguruma with the native WASI SDK.
set -eu
if [[ "$5" == windows ]]; then
    export PATH=/usr/bin:$PATH
    source=$(cygpath -u "$1")
    sdk=$(cygpath -m "$2")
    build=$(cygpath -u "$3")
    library=$(cygpath -m "$3")
    suffix=.exe
    build_args=(--build=x86_64-pc-msys)
else
    source=$1
    sdk=$2
    build=$3
    library=$3
    suffix=
    build_args=()
fi
jobs=$4
cd "$build"
export CC="$sdk/bin/clang$suffix --target=wasm32-wasip1"
export AR="$sdk/bin/llvm-ar$suffix"
export RANLIB="$sdk/bin/llvm-ranlib$suffix"
export CPPFLAGS='-D_WASI_EMULATED_SIGNAL'
export CFLAGS='-O2 -ffunction-sections -fdata-sections'
export LDFLAGS="-L$library -Wl,-z,stack-size=1048576 -Wl,--max-memory=67108864"
export LIBS='-Wl,--wrap=chdir -Wl,--wrap=getcwd -Wl,--wrap=stat -Wl,--wrap=lstat -Wl,--wrap=fstatat -Wl,--undefined=chdir -Wl,--undefined=efi_filesystem_initialize -lefi_cwd -lwasi-emulated-signal'
export PKG_CONFIG=false
if test ! -f Makefile; then
    bash "$source/configure" "${build_args[@]}" --host=wasm32-unknown-wasi \
        --disable-shared --enable-static --disable-maintainer-mode \
        --disable-docs --disable-valgrind --with-oniguruma=builtin \
        > configure.log 2>&1
    make clean > build.log 2>&1
fi
make -C vendor/oniguruma -j"$jobs" >> build.log 2>&1
make src/builtin.inc src/version.h src/config_opts.inc >> build.log 2>&1
# The external cwd archive is not a generated Makefile dependency.
rm -f jq
make -j"$jobs" jq >> build.log 2>&1
