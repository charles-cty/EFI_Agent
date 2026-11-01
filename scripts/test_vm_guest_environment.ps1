param(
    [string]$Qemu = 'qemu-system-x86_64.exe',
    [string]$FirmwareCode = 'artifacts/firmware/OVMF_CODE_4M.fd',
    [string]$FirmwareVariables = 'artifacts/firmware/OVMF_VARS_4M.fd',
    [string]$Esp = 'artifacts/esp',
    [int]$MemoryMiB = 256
)
Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$PSNativeCommandArgumentPassing = 'Standard'
$PSNativeCommandUseErrorActionPreference = $true
$project = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $project
$output = Join-Path ([IO.Path]::GetTempPath()) ('efi-agent/jq-environment-vm-' + [guid]::NewGuid().ToString('N'))
$workspace = Join-Path $output 'host work'
New-Item -ItemType Directory -Path $workspace -Force | Out-Null
$launcher = Join-Path $project 'target/release/efi-agent.exe'
$paths = @($launcher, $FirmwareCode, $FirmwareVariables, $Esp, $workspace) |
    ForEach-Object { "'" + [IO.Path]::GetFullPath($_, $project).Replace("'", "''") + "'" }
$launch = Join-Path $output 'launch.ps1'
$qemuQuoted = "'" + $Qemu.Replace("'", "''") + "'"
$launchLines = @(
    '$env:EFI_AGENT_API_BASE = ''https://example.test/v1'''
    '$env:EFI_AGENT_API_KEY = ''build-only-test'''
    '$env:EFI_AGENT_MODEL = ''build-only-test'''
    '$env:EFI_AGENT_ENVIRONMENT = ''{"VM_ENV":"configured"}'''
    ('& ' + $paths[0] + ' vm ' + $qemuQuoted + ' ' + ($paths[1..4] -join ' ') + ' --memory-mib ' + $MemoryMiB)
    'if ($LASTEXITCODE -ne 0) { throw ''Launcher failed'' }'
    '''JQ_ENVIRONMENT_VM_EXIT_OK'''
)
[IO.File]::WriteAllLines($launch, $launchLines, [Text.UTF8Encoding]::new($false))
$session = 'jq-environment-' + [guid]::NewGuid().ToString('N').Substring(0, 8)
$pane = $null
$captureIndex = 0

function Wait-Screen([scriptblock]$Until, [int]$Seconds = 90) {
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    do {
        $screen = psmux.exe capture-pane -p -t $pane | Out-String
        if (& $Until $screen) {
            $script:captureIndex++
            $screen | Set-Content -LiteralPath (Join-Path $output "screen-$captureIndex.txt")
            return $screen
        }
        Start-Sleep -Milliseconds 150
    } while ([DateTime]::UtcNow -lt $deadline)
    $screen | Set-Content -LiteralPath (Join-Path $output 'failure.txt')
    throw "Terminal condition failed. Evidence: $output"
}

function Wait-Line([string]$Text) {
    Wait-Screen { param($screen)
        @($screen -split '\r?\n' | ForEach-Object { $_.Trim() }).Contains($Text)
    }.GetNewClosure() | Out-Null
}

function Wait-Ready {
    Wait-Screen { param($screen) $screen.Contains('Ready') -and $screen.Contains('history') } | Out-Null
}

function Send-Line([string]$Text) {
    psmux.exe send-keys -t $pane -l $Text
    psmux.exe send-keys -t $pane Enter
}

try {
    psmux.exe new-session -d -s $session -c $project -x 110 -y 36
    $pane = @(psmux.exe list-panes -t $session -F '#{pane_id}')[0].Trim()
    Send-Line 'Write-Output JQ_HOST_READY'
    Wait-Line 'JQ_HOST_READY'
    Send-Line "& '$($launch.Replace("'", "''"))'"
    Wait-Screen { param($screen) $screen.Contains('What would you like to build?') } | Out-Null
    Send-Line '/enter shell'
    Wait-Ready
    Send-Line 'test "$PYTHON_BASIC_REPL:$VM_ENV" = "1:configured" && echo VM_ENV_DEFAULT_OK'
    Wait-Line 'VM_ENV_DEFAULT_OK'
    Send-Line 'jq --version'
    Wait-Line 'jq-1.8.1'
    Send-Line 'printf ''{"items":[2,3,7]}\n'' | jq ''.items|add'' > jq-result; test "$(cat jq-result)" = 12 && echo VM_JQ_PIPE_OK'
    Wait-Line 'VM_JQ_PIPE_OK'
    Send-Line './python.wasm'
    $startup = Wait-Screen { param($screen) $screen.Contains('Python 3.14.7') -and $screen.Contains('>>>') }
    if ($startup.Contains("can't use pyrepl")) { throw 'Python emitted the pyrepl warning' }
    Send-Line 'import os; open("python-env", "w").write(os.environ["PYTHON_BASIC_REPL"] + ":" + os.environ["VM_ENV"]); print("VM_PYTHON_BASIC_OK", 6 * 7)'
    Wait-Line 'VM_PYTHON_BASIC_OK 42'
    Send-Line 'exit()'
    Wait-Ready
    Send-Line 'unset PYTHON_BASIC_REPL; python -c ''import os; assert "PYTHON_BASIC_REPL" not in os.environ; print("VM_UNSET_INHERITED_OK")'''
    Wait-Line 'VM_UNSET_INHERITED_OK'
    Send-Line 'exit'
    Wait-Screen { param($screen) $screen.Contains('Returned to conversation') } | Out-Null
    Send-Line '/enter js'
    Wait-Ready
    Send-Line 'if ((await ash`printf "%s:%s" "$PYTHON_BASIC_REPL" "$VM_ENV"`).stdout !== "1:configured") throw Error("environment"); print("VM_JS_ENV_OK");'
    Wait-Line 'VM_JS_ENV_OK'
    Send-Line 'quit()'
    Wait-Screen { param($screen) $screen.Contains('Returned to conversation') } | Out-Null
    Send-Line '/enter shell'
    Wait-Ready
    Send-Line 'test "$PYTHON_BASIC_REPL:$VM_ENV" = "1:configured" && echo VM_NEW_SESSION_ENV_OK'
    Wait-Line 'VM_NEW_SESSION_ENV_OK'
    Send-Line 'exit'
    Wait-Screen { param($screen) $screen.Contains('Returned to conversation') } | Out-Null
    Send-Line '/exit'
    Wait-Line 'JQ_ENVIRONMENT_VM_EXIT_OK'
    if ([IO.File]::ReadAllText((Join-Path $workspace 'jq-result')) -ne ("12" + [char]10)) { throw 'Saved jq output differs' }
    if ([IO.File]::ReadAllText((Join-Path $workspace 'python-env')) -ne '1:configured') { throw 'Saved Python environment differs' }
    Write-Output 'PASS actual VM: jq pipeline, basic Python REPL without warning, configured inheritance, JS and new sessions, saved host bytes'
    Write-Output "Evidence: $output"
} finally {
    $ownedLaunchers = @(Get-CimInstance Win32_Process -Filter "name='efi-agent.exe'" |
        Where-Object { $_.CommandLine -and $_.CommandLine.Contains($workspace) })
    foreach ($owned in $ownedLaunchers) {
        Get-CimInstance Win32_Process -Filter "name='qemu-system-x86_64.exe'" |
            Where-Object { $_.ParentProcessId -eq $owned.ProcessId } |
            ForEach-Object { Stop-Process -Id $_.ProcessId -ErrorAction SilentlyContinue }
    }
    psmux.exe kill-session -t $session
}
