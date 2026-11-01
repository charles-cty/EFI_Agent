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
$output = Join-Path ([IO.Path]::GetTempPath()) "efi-agent/terminal-$([guid]::NewGuid().ToString('N'))"
$workspace = Join-Path $output 'host work'
New-Item -ItemType Directory -Path $workspace -Force | Out-Null
[IO.File]::WriteAllText((Join-Path $workspace 'vi.txt'), "original`n")
[IO.File]::WriteAllText((Join-Path $workspace 'ed.txt'), "ed seed`n")
$pages = 1..160 | ForEach-Object { 'PAGE_{0:D3}' -f $_ }
[IO.File]::WriteAllText((Join-Path $workspace 'pages'), ($pages -join "`n") + "`n")
$launcher = Join-Path $project 'target/release/efi-agent.exe'
$paths = @($launcher, $FirmwareCode, $FirmwareVariables, $Esp, $workspace) |
    ForEach-Object { "'" + [IO.Path]::GetFullPath($_, $project).Replace("'", "''") + "'" }
$launch = Join-Path $output 'launch.ps1'
$qemuLog = Join-Path $output 'qemu.log'
$qemuQuoted = "'" + $Qemu.Replace("'", "''") + "'"
@"
`$env:EFI_AGENT_API_BASE = 'https://example.test/v1'
`$env:EFI_AGENT_API_KEY = 'build-only-test'
`$env:EFI_AGENT_MODEL = 'build-only-test'
& $($paths[0]) vm $qemuQuoted $($paths[1..4] -join ' ') --memory-mib $MemoryMiB 2> '$($qemuLog.Replace("'", "''"))'
if (`$LASTEXITCODE -ne 0) { throw 'Launcher failed' }
'BUSYBOX_TERMINAL_VM_EXIT_OK'
"@ | Set-Content -LiteralPath $launch -Encoding utf8
$session = "busybox-terminal-$([guid]::NewGuid().ToString('N').Substring(0, 8))"
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
        @($screen -split "`r?`n" | ForEach-Object { $_.Trim() }).Contains($Text)
    }.GetNewClosure() | Out-Null
}

function Wait-Ready {
    Wait-Screen { param($screen) $screen.Contains('Ready') -and $screen.Contains('history') } | Out-Null
}

function Wait-Raw {
    Wait-Screen { param($screen) $screen.Contains('Guest terminal') } | Out-Null
}

function Send-Line([string]$Text) {
    psmux.exe send-keys -t $pane -l $Text
    psmux.exe send-keys -t $pane Enter
}

function Test-Pager([string]$Command, [switch]$Resize) {
    Send-Line $Command
    $first = Wait-Screen { param($screen)
        $screen.Contains('Guest terminal') -and [regex]::Matches($screen, 'PAGE_(\d{3})').Count -ge 3
    }
    $firstNumbers = @([regex]::Matches($first, 'PAGE_(\d{3})') | ForEach-Object { [int]$_.Groups[1].Value })
    if ($firstNumbers.Count -lt 3) { throw 'Pager did not display the input' }
    psmux.exe send-keys -t $pane -l ' '
    Wait-Screen { param($screen)
        $numbers = @([regex]::Matches($screen, 'PAGE_(\d{3})') | ForEach-Object { [int]$_.Groups[1].Value })
        $numbers.Count -gt 0 -and $numbers[0] -gt $firstNumbers[0]
    }.GetNewClosure() | Out-Null
    if ($Resize) {
        psmux.exe resize-window -t $session -x 110 -y 36
        $resized = Wait-Screen { param($screen)
            $screen.Contains('Guest terminal') -and
                [regex]::Matches($screen, 'PAGE_(\d{3})').Count -ge $firstNumbers.Count + 3
        }.GetNewClosure()
        $resizedFirst = [int][regex]::Match($resized, 'PAGE_(\d{3})').Groups[1].Value
        psmux.exe send-keys -t $pane PageDown
        Wait-Screen { param($screen)
            $match = [regex]::Match($screen, 'PAGE_(\d{3})')
            $match.Success -and [int]$match.Groups[1].Value -gt $resizedFirst
        }.GetNewClosure() | Out-Null
    }
    psmux.exe send-keys -t $pane -l 'q'
    Wait-Ready
}

try {
    psmux.exe new-session -d -s $session -c $project -x 100 -y 30
    $pane = @(psmux.exe list-panes -t $session -F '#{pane_id}')[0].Trim()
    Send-Line "& '$($launch.Replace("'", "''"))'"
    Wait-Screen { param($screen) $screen.Contains('What would you like to build?') } | Out-Null
    Send-Line '/enter shell'
    Wait-Ready
    Send-Line 'vi vi.txt'
    Wait-Raw
    psmux.exe send-keys -t $pane -l 'iedited '
    psmux.exe send-keys -t $pane Esc
    Start-Sleep -Milliseconds 400
    Send-Line ':wq'
    Wait-Ready
    Send-Line 'test "$(cat vi.txt)" = "edited original" && echo VM_VI_SAVED_OK'
    Wait-Line 'VM_VI_SAVED_OK'
    Send-Line 'vi vi.txt'
    Wait-Raw
    psmux.exe send-keys -t $pane -l 'iUNSAVED'
    psmux.exe send-keys -t $pane C-c
    Start-Sleep -Milliseconds 400
    Send-Line ':q!'
    Wait-Ready
    Send-Line 'test "$(cat vi.txt)" = "edited original" && echo VM_VI_DISCARD_OK'
    Wait-Line 'VM_VI_DISCARD_OK'
    Send-Line 'ed ed.txt'
    Wait-Screen { param($screen) $screen.Contains('Running') -and $screen.Contains('history') } | Out-Null
    foreach ($line in @('a', 'added line', '.', 'w', 'q')) { Send-Line $line }
    Wait-Ready
    Send-Line 'test "$(wc -l < ed.txt)" = 2 && echo VM_ED_SAVED_OK'
    Wait-Line 'VM_ED_SAVED_OK'
    Test-Pager 'more pages'
    Test-Pager 'less pages' -Resize
    Test-Pager 'cat pages | less'
    Send-Line 'vi vi.txt'
    Wait-Raw
    psmux.exe send-keys -t $pane C-q
    Wait-Ready
    Send-Line 'sleep 100 & p=$!; kill -0 "$p" && kill -TERM "$p"; wait "$p"; s=$?; test "$s" = 143 && echo VM_KILL_STATUS_OK'
    Wait-Line 'VM_KILL_STATUS_OK'
    Send-Line 'ps -o pid,ppid,comm; echo VM_PROCESS_QUERY_OK'
    Wait-Line 'VM_PROCESS_QUERY_OK'
    Send-Line 'exit'
    Wait-Screen { param($screen) $screen.Contains('Returned to conversation') } | Out-Null
    Send-Line '/exit'
    Wait-Line 'BUSYBOX_TERMINAL_VM_EXIT_OK'
    if ([IO.File]::ReadAllText((Join-Path $workspace 'vi.txt')) -ne "edited original`n") { throw 'vi host bytes differ' }
    if ([IO.File]::ReadAllText((Join-Path $workspace 'ed.txt')) -ne "ed seed`nadded line`n") { throw 'ed host bytes differ' }
    'PASS actual VM: vi save/discard, ed save, more/less Space/q, pipe keyboard, resize, cancellation, process commands and host bytes'
    "Evidence: $output"
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
