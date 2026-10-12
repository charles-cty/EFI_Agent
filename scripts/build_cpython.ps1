param(
    [Parameter(Mandatory)][string]$WasiSdk,
    [Parameter(Mandatory)][string]$BuildPython,
    [string]$Source = 'artifacts/guest-sources/Python-3.14.0',
    [string]$Output = 'artifacts/wasm-guests',
    [string]$MsysBash = 'C:\msys64\usr\bin\bash.exe',
    [int]$Jobs = 4,
    [string]$Uv = 'uv.exe',
    [switch]$Reconfigure
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$PSNativeCommandArgumentPassing = 'Standard'
$WasiSdk = (Resolve-Path -LiteralPath $WasiSdk).Path
$BuildPython = (Resolve-Path -LiteralPath $BuildPython).Path
$Source = (Resolve-Path -LiteralPath $Source).Path
$project = (Get-Location).Path
$build = Join-Path $Output 'python-build'
$frozen = Join-Path $build 'frozen'
New-Item -ItemType Directory -Force -Path $frozen | Out-Null
$build = (Resolve-Path -LiteralPath $build).Path
$frozen = (Resolve-Path -LiteralPath $frozen).Path
$Output = (Resolve-Path -LiteralPath $Output).Path
if ($Reconfigure -and (Test-Path -LiteralPath (Join-Path $build 'Makefile'))) {
    Remove-Item -LiteralPath (Join-Path $build 'Makefile')
}

# SDK 34 has linkable pthread stubs. Select CPython's single-thread backend
# explicitly instead of accepting those symbols as a native thread facility.
$configureSource = [IO.File]::ReadAllText((Join-Path $Source 'configure'))
$threadCheck = 'if test "$ac_cv_pthread_is_default" = yes'
if (-not $configureSource.Contains($threadCheck)) { throw 'CPython thread detection changed.' }
$configureSource = $configureSource.Replace($threadCheck,
    'if test "$ac_sys_system" = WASI && test "$enable_wasm_pthreads" != yes; then' + "`n" +
    '    posix_threads=stub' + "`n" + 'elif test "$ac_cv_pthread_is_default" = yes')
[IO.File]::WriteAllText((Join-Path $build 'configure'), $configureSource)
New-Item -ItemType Directory -Force -Path (Join-Path $build 'Modules') | Out-Null
[IO.File]::WriteAllText((Join-Path $build 'Modules/Setup.local'), "*disabled*`n_decimal`n_socket`nselect`n")

# Configure and make run in native MSYS2; compiler and Python are Windows tools.
$configure = @'
set -eu
export PATH=/usr/bin:$PATH
source=$(cygpath -u "$1")
sdk=$(cygpath -u "$2")
python=$(cygpath -u "$3")
build=$(cygpath -u "$4")
cd "$build"
export CC="$sdk/bin/clang.exe --target=wasm32-wasip1"
export CXX="$sdk/bin/clang++.exe --target=wasm32-wasip1"
export CPP="$sdk/bin/clang.exe --target=wasm32-wasip1 -E"
export AR="$sdk/bin/llvm-ar.exe"
export RANLIB="$sdk/bin/llvm-ranlib.exe"
export CONFIG_SITE="$source/Tools/wasm/wasi/config.site-wasm32-wasi"
export PKG_CONFIG_LIBDIR=
export PKG_CONFIG_PATH=
export PKG_CONFIG=false
export ac_cv_lib_dl_dlopen=no
export ac_cv_func_dlopen=no
export ac_cv_func_readlink=no
export ac_cv_func_realpath=no
export py_cv_module__decimal=n/a
if test ! -f Makefile; then
  bash ./configure --srcdir="$source" --build=x86_64-pc-msys --host=wasm32-unknown-wasip1 \
    --with-build-python="$python" --disable-test-modules --without-ensurepip \
    --disable-shared --disable-wasm-pthreads \
    CFLAGS='-O2 -ffunction-sections -fdata-sections' >configure.log 2>&1
fi
'@
& $MsysBash -c $configure 'efi-python-build' $Source $WasiSdk $BuildPython $build $Jobs

# Remove host-facing built-in entry points from the generated module table.
# Core initialization functions can still link from their translation units.
$configuration = Join-Path $build 'Modules/config.c'
$text = [IO.File]::ReadAllText($configuration)
$text = [regex]::Replace($text, '(?m)^\s*\{\s*"(?:posix|pwd|time|_datetime|_signal|faulthandler|_sysconfig|_locale)"[^\r\n]*\r?\n', '')
[IO.File]::WriteAllText($configuration, $text)

# Keep CPython's standard streams without exposing file names or other fds.
$fileio = [IO.File]::ReadAllText((Join-Path $Source 'Modules/_io/fileio.c'))
$start = '_io_FileIO___init___impl(fileio *self, PyObject *nameobj, const char *mode,'
$position = $fileio.IndexOf($start, [StringComparison]::Ordinal)
if ($position -lt 0) { throw 'CPython FileIO initialization changed; inspect the port.' }
$brace = $fileio.IndexOf('{', $position)
$guard = @'

    if (!PyLong_Check(nameobj) || PyBool_Check(nameobj) || opener != Py_None) {
        PyErr_SetString(PyExc_PermissionError, "Only standard-stream descriptors are available");
        return -1;
    }
    int permitted_fd = PyLong_AsInt(nameobj);
    if (permitted_fd == -1 && PyErr_Occurred()) {
        return -1;
    }
    if (permitted_fd < 0 || permitted_fd > 2) {
        PyErr_SetString(PyExc_PermissionError, "Only standard-stream descriptors are available");
        return -1;
    }
'@
$fileio = $fileio.Insert($brace + 1, $guard + "`n")
$ioDirectory = Join-Path $build 'Modules/_io'
New-Item -ItemType Directory -Force -Path $ioDirectory | Out-Null
[IO.File]::WriteAllText((Join-Path $ioDirectory 'fileio.c'), $fileio)

# This platform supports built-in and frozen imports only. Keep core import
# initialization, but do not install filesystem, zip or extension importers.
$import = [IO.File]::ReadAllText((Join-Path $Source 'Python/import.c'))
$externalStart = $import.IndexOf("PyStatus`n_PyImport_InitExternal(PyThreadState *tstate)`n{", [StringComparison]::Ordinal)
if ($externalStart -lt 0) { throw 'CPython external importer initialization changed.' }
$externalEnd = $import.IndexOf("`nvoid`n_PyImport_FiniExternal", $externalStart, [StringComparison]::Ordinal)
if ($externalEnd -lt 0) { throw 'CPython external importer finalization changed.' }
$restrictedImport = @'
PyStatus
_PyImport_InitExternal(PyThreadState *tstate)
{
    (void)tstate;
    return _PyStatus_OK();
}

'@
$import = $import.Substring(0, $externalStart) + $restrictedImport + $import.Substring($externalEnd)
$pythonDirectory = Join-Path $build 'Python'
New-Item -ItemType Directory -Force -Path $pythonDirectory | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $pythonDirectory 'clinic') | Out-Null
Copy-Item -LiteralPath (Join-Path $Source 'Python/clinic/import.c.h') -Destination (Join-Path $pythonDirectory 'clinic/import.c.h')
[IO.File]::WriteAllText((Join-Path $pythonDirectory 'import.c'), $import)
$makefile = Join-Path $build 'Makefile'
$makefileText = [IO.File]::ReadAllText($makefile)
$makefileText = $makefileText.Replace('$(srcdir)/Modules/_io/fileio.c', 'Modules/_io/fileio.c')
$makefileText = $makefileText.Replace('$(srcdir)/Python/import.c', 'Python/import.c')
[IO.File]::WriteAllText($makefile, $makefileText)
$make = @'
set -eu
export PATH=/usr/bin:$PATH
cd "$(cygpath -u "$1")"
make -f Makefile -f efi-libraries.mk -j "$2" efi-libraries >make.log 2>&1
make -s -f Makefile -f efi-libraries.mk efi-link-flags >link-flags.txt
'@
$libraries = @'
efi-libraries: libpython3.14.a $(filter %.a,$(MODLIBS))
efi-link-flags:
	@printf '%s\n' '$(MODLIBS) $(LIBS) $(SYSLIBS)'
'@
[IO.File]::WriteAllText((Join-Path $build 'efi-libraries.mk'), $libraries + "`n")
& $MsysBash -c $make 'efi-python-make' $build $Jobs

$freezer = Join-Path $Source 'Programs/_freeze_module.py'
foreach ($module in @(
    @('encodings', 'Lib/encodings/__init__.py', 'encodings.h'),
    @('encodings.aliases', 'Lib/encodings/aliases.py', 'encodings_aliases.h'),
    @('encodings.utf_8', 'Lib/encodings/utf_8.py', 'encodings_utf_8.h')
)) {
    & $Uv run --no-project --python $BuildPython $freezer $module[0] (Join-Path $Source $module[1]) (Join-Path $frozen $module[2])
}
& $Uv run --no-project --python $BuildPython $freezer efi guests/python/efi.py (Join-Path $frozen 'efi.h')
$clang = Join-Path $WasiSdk 'bin/clang.exe'
$sysroot = Join-Path $WasiSdk 'share/wasi-sysroot'
$compile = '--target=wasm32-wasip1', "--sysroot=$sysroot", '-O2', "-I$Source/Include",
    "-I$build", "-I$frozen", "-I$project/guests/include", (Join-Path $project 'guests/python/main.c'),
    (Join-Path $project 'guests/python/shell.c'), (Join-Path $project 'guests/lib/process.c'),
    (Join-Path $project 'guests/lib/shell.c'), (Join-Path $build 'libpython3.14.a'),
    '-Wl,-z,stack-size=16777216', '-Wl,--stack-first', '-Wl,--max-memory=67108864', '-Wl,--strip-debug',
    '-o', (Join-Path $Output 'python.wasm')
$compile += ([IO.File]::ReadAllText((Join-Path $build 'link-flags.txt')).Trim() -split '\s+')
Push-Location -LiteralPath $build
try { & $clang @compile } finally { Pop-Location }
Copy-Item -LiteralPath (Join-Path $Source 'LICENSE') -Destination (Join-Path $Output 'python.LICENSE')
