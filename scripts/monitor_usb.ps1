<# Read-only PnP polling plus Lumien transaction-log tail. PnP events use a separate file. #>
[CmdletBinding()]
param(
    [string]$LogFile = "$env:USERPROFILE\Desktop\tryx_usb_transactions.log",
    [string]$EventLogFile = "",
    [ValidateRange(0, 2147483647)][int]$Samples = 0,
    [ValidateRange(0, 60000)][int]$PollMilliseconds = 250
)
if (-not $EventLogFile) { $EventLogFile = "$LogFile.pnp.log" }
if ([IO.Path]::GetFullPath($EventLogFile) -eq [IO.Path]::GetFullPath($LogFile)) {
    throw "PnP event log must differ from transaction log (avoids duplicate output)."
}
$eventDir = Split-Path -Parent ([IO.Path]::GetFullPath($EventLogFile))
[IO.Directory]::CreateDirectory($eventDir) | Out-Null
$instancePattern = 'USB\VID_391A&PID_1021\*'
$lastState = $null
$filePosition = if (Test-Path -LiteralPath $LogFile) { (Get-Item -LiteralPath $LogFile).Length } else { 0 }
function Log-Message([string]$level, [string]$msg, [ConsoleColor]$color) {
    $line = "[$(Get-Date -Format 'yyyy-MM-dd HH:mm:ss.fff')] [$level] $msg"
    Write-Host $line -ForegroundColor $color
    Add-Content -LiteralPath $EventLogFile -Value $line -Encoding UTF8
}
Write-Host "Lumien read-only USB monitor. Poll timestamps are observations, not exact bus-event times."
Write-Host "Transactions: $LogFile"
Write-Host "PnP events: $EventLogFile"
$sample = 0
while ($Samples -eq 0 -or $sample -lt $Samples) {
    $sample++
    try {
        # Prefer a currently present node over historical phantom instances.
        $nodes = @(Get-PnpDevice -InstanceId $instancePattern -ErrorAction SilentlyContinue)
        $dev = $nodes | Sort-Object @{Expression={if ($_.Present) { 0 } else { 1 }}}, InstanceId | Select-Object -First 1
        $present = [bool]($dev -and $dev.Present)
        $state = "$present|$($dev.Status)|$($dev.Problem)|$($dev.InstanceId)"
        if ($state -ne $lastState) {
            $wasPresent = $lastState -and $lastState.StartsWith('True|')
            $level = if (-not $lastState) { 'INIT' } elseif ($present -and -not $wasPresent) { 'RECONNECT' } elseif (-not $present -and $wasPresent) { 'DISCONNECT' } else { 'STATUS' }
            Log-Message $level "Present=$present Status=$($dev.Status) Problem=$($dev.Problem) Instance=$($dev.InstanceId)" $(if ($present) { 'Green' } else { 'Yellow' })
            if ($level -eq 'DISCONNECT') {
                Log-Message 'DIAGNOSTIC' 'Windows reports device absent. Cause unknown; no reboot, over-current, or suspend evidence collected by this poll. Correlate Kernel-PnP and USB event logs.' 'DarkYellow'
            }
            $lastState = $state
        }
    } catch {
        # A failed PnP query is not a device removal.
        Log-Message 'QUERY-ERROR' $_.Exception.Message 'Yellow'
    }
    if (Test-Path -LiteralPath $LogFile) {
        $length = (Get-Item -LiteralPath $LogFile).Length
        if ($length -lt $filePosition) { $filePosition = 0 }
        if ($length -gt $filePosition) {
            $stream = [IO.File]::Open($LogFile, 'Open', 'Read', 'ReadWrite')
            try {
                $stream.Position = $filePosition
                $reader = [IO.StreamReader]::new($stream)
                $text = $reader.ReadToEnd()
                # Defer incomplete trailing lines until the writer finishes them.
                $end = $text.LastIndexOf([char]10)
                if ($end -ge 0) {
                    $complete = $text.Substring(0, $end + 1)
                    $filePosition += [Text.Encoding]::UTF8.GetByteCount($complete)
                    foreach ($line in ($complete.TrimEnd([char]10,[char]13) -split '\r?\n')) {
                        $color = if ($line -match '\[(ERROR|FATAL)') { 'Red' } elseif ($line -match '\[SYNC') { 'Green' } else { 'Gray' }
                        Write-Host $line -ForegroundColor $color
                    }
                }
            } finally {
                if ($reader) { $reader.Dispose() }
                $stream.Dispose()
            }
        }
    }
    if ($Samples -eq 0 -or $sample -lt $Samples) { Start-Sleep -Milliseconds $PollMilliseconds }
}
