param(
    [Parameter(Mandatory)][string]$Qemu,
    [Parameter(Mandatory)][string]$Code,
    [Parameter(Mandatory)][string]$Vars,
    [string]$Psmux = 'C:\Programs\psmux\psmux.exe',
    [string]$Launcher = '',
    [string]$Esp = '',
    [string]$Output = '',
    [ValidateRange(1, 2147483647)][int]$MemoryMiB = 128
)
Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$PSNativeCommandArgumentPassing = 'Standard'
$PSNativeCommandUseErrorActionPreference = $true
$root = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $root
if (-not $Launcher) { $Launcher = Join-Path $root 'target\debug\efi-agent.exe' }
if (-not $Esp) { $Esp = Join-Path $root 'artifacts\esp' }
if (-not $Output) { $Output = Join-Path $root 'artifacts\windows-smoke' }
foreach ($path in @($Qemu, $Code, $Vars, $Psmux, $Launcher, $Esp)) {
    if (-not (Test-Path -LiteralPath $path)) { throw "Missing test input: $path" }
}
New-Item -ItemType Directory -Force -Path $Output | Out-Null
$workspace = Join-Path $Output ('workspace-' + [guid]::NewGuid().ToString('N').Substring(0, 12))
New-Item -ItemType Directory -Force -Path $workspace | Out-Null
$session = "efi-smoke-$([guid]::NewGuid().ToString('N').Substring(0, 12))"
$pane = ''

function Wait-Screen([string]$Marker, [int]$Seconds = 30) {
    $deadline = (Get-Date).AddSeconds($Seconds)
    $capture = ''
    do {
        $capture = (& $Psmux capture-pane -p -t $pane | Out-String)
        if ($capture.Contains($Marker)) { return $capture }
        Start-Sleep -Milliseconds 100
    } while ((Get-Date) -lt $deadline)
    Set-Content -LiteralPath (Join-Path $Output 'failure.txt') -Value $capture -Encoding utf8NoBOM
    throw "ConPTY did not show: $Marker"
}

function Send-Text([string]$Text) {
    & $Psmux send-keys -t $pane -l $Text
    & $Psmux send-keys -t $pane Enter
}

function Quote-PS([string]$Text) { return "'" + $Text.Replace("'", "''") + "'" }

function Assert-MainScreen {
    $alternate = (& $Psmux display-message -p -t $pane '#{alternate_on}' | Out-String).Trim()
    if ($alternate -ne '0') { throw 'Launcher left the alternate screen active' }
}

function Wait-File([string]$Name, [string]$Expected) {
    $file = Join-Path $workspace $Name
    $deadline = (Get-Date).AddSeconds(60)
    do {
        if ((Test-Path -LiteralPath $file) -and
            [IO.File]::ReadAllText($file) -eq $Expected) { return }
        Start-Sleep -Milliseconds 100
    } while ((Get-Date) -lt $deadline)
    & $Psmux capture-pane -p -t $pane |
        Set-Content -LiteralPath (Join-Path $Output 'failure.txt') -Encoding utf8NoBOM
    throw "Host file content mismatch: $Name"
}

try {
    & $Psmux new-session -d -s $session -c $root
    $pane = @(& $Psmux list-panes -t $session -F '#{pane_id}')[0].Trim()
    if (-not $pane) { throw 'psmux created no pane' }
    # First wait for PowerShell to load its profile and establish input handling.
    $null = Wait-Screen ($root + '>') 30
    Send-Text "Write-Output ('EFI_' + 'SHELL_READY')"
    $null = Wait-Screen 'EFI_SHELL_READY'
    $command = '& ' + (Quote-PS $Launcher) + ' vm ' +
        ((@($Qemu, $Code, $Vars, $Esp, $workspace) | ForEach-Object { Quote-PS $_ }) -join ' ')
    if ($PSBoundParameters.ContainsKey('MemoryMiB')) {
        $command += " --memory-mib $MemoryMiB"
    }
    Send-Text ($command + "; Write-Output ('EFI_EXIT_' + `$LASTEXITCODE)")
    $screen = Wait-Screen 'What would you like to build?' 60
    Set-Content -LiteralPath (Join-Path $Output 'initial.txt') -Value $screen -Encoding utf8NoBOM
    Write-Output 'PASS Windows WHPX boot and ConPTY TUI'
    Send-Text '/host-write unicode.txt left 中 right'
    Wait-File 'unicode.txt' 'left 中 right'
    & $Psmux send-keys -t $pane -l '/host-read unicode.txtX'
    & $Psmux send-keys -t $pane Backspace Enter
    $screen = Wait-Screen 'left 中 right'
    Set-Content -LiteralPath (Join-Path $Output 'file.txt') -Value $screen -Encoding utf8NoBOM
    Write-Output 'PASS Windows Unicode input and guest TCP4 host filesystem'
    & $Psmux send-keys -t $pane -l '/host-write editor.txt left 中X right'
    & $Psmux send-keys -t $pane Left Left Left Left Left Left Backspace Home End Enter
    Wait-File 'editor.txt' 'left 中 right'
    & $Psmux send-keys -t $pane -l '/host-write multiline.txt first'
    & $Psmux send-keys -t $pane C-j
    & $Psmux send-keys -t $pane -l 'second 中'
    $null = Wait-Screen 'second 中'
    if (Test-Path -LiteralPath (Join-Path $workspace 'multiline.txt')) {
        throw 'Ctrl+J submitted a partial prompt'
    }
    & $Psmux send-keys -t $pane Enter
    Wait-File 'multiline.txt' "first`nsecond 中"
    Write-Output 'PASS Windows cursor editing and multiline input'
    Send-Text '/quit'
    $screen = Wait-Screen 'EFI_EXIT_0' 15
    Set-Content -LiteralPath (Join-Path $Output 'exit.txt') -Value $screen -Encoding utf8NoBOM
    Send-Text "Write-Output ('EFI_' + 'SHELL_RESTORED')"
    $null = Wait-Screen 'EFI_SHELL_RESTORED'
    Assert-MainScreen
    Write-Output 'PASS /quit returns to the Windows shell'
    Send-Text ($command + "; Write-Output ('EFI_INTERRUPT_' + `$LASTEXITCODE)")
    $null = Wait-Screen 'What would you like to build?' 60
    & $Psmux send-keys -t $pane C-c
    # A real console Ctrl+C also cancels the enclosing PowerShell pipeline,
    # so its trailing exit-code command need not run. Verify a fresh command.
    $null = Wait-Screen ($root + '>') 15
    Send-Text "Write-Output ('EFI_' + 'INTERRUPT_RESTORED')"
    $screen = Wait-Screen 'EFI_INTERRUPT_RESTORED'
    Assert-MainScreen
    Set-Content -LiteralPath (Join-Path $Output 'interrupt.txt') -Value $screen -Encoding utf8NoBOM
    $remaining = @(Get-CimInstance Win32_Process | Where-Object {
        $_.Name -eq 'qemu-system-x86_64.exe' -and $_.CommandLine.Contains($workspace)
    })
    if ($remaining.Count) { throw 'Ctrl+C left the test QEMU process running' }
    Write-Output 'PASS Ctrl+C returns to the Windows shell'
} finally {
    if ($pane) {
        & $Psmux send-keys -t $pane C-c
        # Give the launcher's cleanup time to stop its own QEMU child.
        $deadline = (Get-Date).AddSeconds(10)
        do {
            $screen = (& $Psmux capture-pane -p -t $pane | Out-String)
            if ($screen.Contains($root + '>')) { break }
            Start-Sleep -Milliseconds 100
        } while ((Get-Date) -lt $deadline)
        & $Psmux kill-session -t $session
    }
}
