param(
    [Parameter(Mandatory)][string]$Qemu,
    [Parameter(Mandatory)][string]$Code,
    [Parameter(Mandatory)][string]$Vars,
    [string]$Psmux = '',
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
if (-not $Psmux) {
    $psmuxCommand = Get-Command psmux.exe -CommandType Application -ErrorAction SilentlyContinue
    if (-not $psmuxCommand) { throw 'psmux.exe is not on PATH. Supply its executable path with -Psmux.' }
    $Psmux = $psmuxCommand.Source
}
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
$originalClipboard = Get-Clipboard -Raw

function Wait-Screen([string]$Marker, [int]$Seconds = 30, [switch]$IgnoreWhitespace, [switch]$ExactLine) {
    $deadline = (Get-Date).AddSeconds($Seconds)
    $capture = ''
    do {
        $capture = (& $Psmux capture-pane -p -t $pane | Out-String)
        if ($ExactLine) {
            if ($capture -match ('(?m)^' + [regex]::Escape($Marker) + '\r?$')) { return $capture }
        } elseif ($capture.Contains($Marker)) { return $capture }
        # ConPTY captures can omit spaces in cells updated by separate writes.
        if ($IgnoreWhitespace -and ($capture -replace '\s', '').Contains(($Marker -replace '\s', ''))) {
            return $capture
        }
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


try {
    & $Psmux new-session -d -s $session -c $root
    $pane = @(& $Psmux list-panes -t $session -F '#{pane_id}')[0].Trim()
    if (-not $pane) { throw 'psmux created no pane' }
    # First wait for PowerShell to load its profile and establish input handling.
    $null = Wait-Screen ($root + '>') 30
    Send-Text "Write-Output ('EFI_' + 'SHELL_READY')"
    $null = Wait-Screen 'EFI_SHELL_READY'
    Send-Text '$env:EFI_AGENT_API_BASE = "http://127.0.0.1:1/v1"; $env:EFI_AGENT_API_KEY = "test"; $env:EFI_AGENT_MODEL = "test"'
    $command = '& ' + (Quote-PS $Launcher) + ' vm ' +
        ((@($Qemu, $Code, $Vars, $Esp, $workspace) | ForEach-Object { Quote-PS $_ }) -join ' ')
    if ($PSBoundParameters.ContainsKey('MemoryMiB')) {
        $command += " --memory-mib $MemoryMiB"
    }
    $exitLog = Join-Path $Output 'exit-stderr.log'
    $exitCommand = $command + ' 2> ' + (Quote-PS $exitLog)
    Send-Text ($exitCommand + "; Write-Output ('EFI_EXIT_' + `$LASTEXITCODE)")
    $screen = Wait-Screen 'What would you like to build?' 60
    Set-Content -LiteralPath (Join-Path $Output 'initial.txt') -Value $screen -Encoding utf8NoBOM
    Write-Output 'PASS Windows WHPX boot and ConPTY TUI'
    Set-Clipboard -Value "/unsupported clipboard 中`r`nsecond line"
    & $Psmux send-keys -t $pane C-v
    $null = Wait-Screen 'second line' -IgnoreWhitespace
    $screen = (& $Psmux capture-pane -p -t $pane | Out-String)
    if ($screen.Contains('Unknown command. Use /help.')) { throw 'Clipboard paste submitted the prompt' }
    & $Psmux send-keys -t $pane Enter
    $null = Wait-Screen 'Unknown command. Use /help.' -IgnoreWhitespace
    Write-Output 'PASS Windows system clipboard multiline paste without submission'
    & $Psmux send-keys -t $pane -l '/unsupported left 中X right'
    & $Psmux send-keys -t $pane Left Left Left Left Left Left Backspace Home End
    $null = Wait-Screen '/unsupported left 中 right' -IgnoreWhitespace
    & $Psmux send-keys -t $pane C-j
    & $Psmux send-keys -t $pane -l 'second 中'
    $null = Wait-Screen 'second 中'
    & $Psmux send-keys -t $pane Enter
    $null = Wait-Screen 'Unknown command. Use /help.' -IgnoreWhitespace
    Write-Output 'PASS Windows Unicode cursor editing and multiline input'
    Send-Text '/status'
    $null = Wait-Screen 'Direct firmware requests: 0' -IgnoreWhitespace
    Send-Text '/exit'
    $screen = Wait-Screen 'EFI_EXIT_0' 15
    Set-Content -LiteralPath (Join-Path $Output 'exit.txt') -Value $screen -Encoding utf8NoBOM
    Send-Text "Write-Output ('EFI_' + 'SHELL_RESTORED')"
    $null = Wait-Screen 'EFI_SHELL_RESTORED' -ExactLine
    Assert-MainScreen
    # QEMU can emit its own platform warnings; check the launcher diagnostics.
    if ([IO.File]::ReadAllText($exitLog) -match 'EFI Agent:') {
        throw 'Normal /exit wrote launcher errors to stderr'
    }
    Write-Output 'PASS /exit returns to the Windows shell'
    # Keep native console handles for Ctrl+C. PowerShell redirection changes
    # native pipeline cancellation and can terminate the child before cleanup.
    Send-Text ($command + "; Write-Output ('EFI_INTERRUPT_' + `$LASTEXITCODE)")
    $null = Wait-Screen 'What would you like to build?' 60
    Send-Text '/status'
    $null = Wait-Screen 'Direct firmware requests: 0' -IgnoreWhitespace
    & $Psmux send-keys -t $pane C-c
    $null = Wait-Screen 'Press Ctrl+C again within 1 second to exit' -IgnoreWhitespace
    & $Psmux send-keys -t $pane C-c
    # A real console Ctrl+C also cancels the enclosing PowerShell pipeline,
    # so its trailing exit-code command need not run. Verify a fresh command.
    $null = Wait-Screen ($root + '>') 15
    Send-Text "Write-Output ('EFI_' + 'INTERRUPT_RESTORED')"
    $screen = Wait-Screen 'EFI_INTERRUPT_RESTORED' -ExactLine
    Assert-MainScreen
    if ($screen -match 'EFI Agent:') {
        throw 'Ctrl+C displayed launcher errors'
    }
    Set-Content -LiteralPath (Join-Path $Output 'interrupt.txt') -Value $screen -Encoding utf8NoBOM
    $remaining = @(Get-CimInstance Win32_Process | Where-Object {
        $_.Name -eq 'qemu-system-x86_64.exe' -and $_.CommandLine.Contains($workspace)
    })
    if ($remaining.Count) { throw 'Ctrl+C left the test QEMU process running' }
    Write-Output 'PASS Ctrl+C returns to the Windows shell'
} finally {
    if ($null -eq $originalClipboard) { Set-Clipboard -Value '' }
    else { Set-Clipboard -Value $originalClipboard }
    if ($pane) {
        & $Psmux send-keys -t $pane C-c
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
