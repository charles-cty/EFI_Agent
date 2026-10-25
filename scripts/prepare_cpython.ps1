param(
    [string]$Archive,
    [string]$Output = 'artifacts/wasm-guests'
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$cache = Join-Path (Get-Location) 'artifacts/python-prebuilt'
New-Item -ItemType Directory -Force -Path $cache, $Output | Out-Null
if (-not $Archive) {
    $Archive = Join-Path $cache 'python-3.14.7-wasi_sdk-24.zip'
    if (-not (Test-Path -LiteralPath $Archive)) {
        $uri = 'https://github.com/brettcannon/cpython-wasi-build/releases/download/v3.14.7/python-3.14.7-wasi_sdk-24.zip'
        Invoke-WebRequest -Uri $uri -OutFile $Archive
    }
}
$distribution = Join-Path $cache 'extracted'
if (Test-Path -LiteralPath $distribution) {
    Remove-Item -LiteralPath $distribution -Recurse -Force
}
Expand-Archive -LiteralPath $Archive -DestinationPath $distribution
$library = Join-Path $distribution 'lib/python3.14/site-packages'
Copy-Item -LiteralPath guests/python/sitecustomize.py, guests/python/_efi_process.py, guests/python/_efi_filesystem.py -Destination $library
Copy-Item -LiteralPath (Join-Path $distribution 'python.wasm') -Destination (Join-Path $Output 'python.wasm')
$outputLibrary = Join-Path $Output 'lib/python3.14'
if (Test-Path -LiteralPath $outputLibrary) {
    Remove-Item -LiteralPath $outputLibrary -Recurse -Force
}
Copy-Item -LiteralPath (Join-Path $distribution 'lib') -Destination $Output -Recurse -Force
Copy-Item -LiteralPath (Join-Path $distribution 'LICENSE') -Destination (Join-Path $Output 'python.LICENSE')
Write-Output 'Prepared unchanged CPython 3.14.7 WASI binary, upstream standard library and Python startup adapter.'
