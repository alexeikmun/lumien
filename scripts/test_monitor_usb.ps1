$ErrorActionPreference = 'Stop'
$testRoot = Join-Path ([IO.Path]::GetTempPath()) ("lumien-monitor-test-" + [guid]::NewGuid())
[IO.Directory]::CreateDirectory($testRoot) | Out-Null
try {
    $script:poll = 0
    $global:LumienMonitorTestOutput = [System.Collections.Generic.List[string]]::new()
    function Write-Host { param($Object, $ForegroundColor) $global:LumienMonitorTestOutput.Add([string]$Object) }
    function Get-PnpDevice {
        [CmdletBinding()] param($InstanceId)
        $script:poll++
        $offline = [pscustomobject]@{Present=$false;Status='Unknown';Problem='CM_PROB_PHANTOM';InstanceId='USB\VID_391A&PID_1021\old'}
        $online = [pscustomobject]@{Present=$true;Status='OK';Problem='CM_PROB_NONE';InstanceId='USB\VID_391A&PID_1021\live'}
        switch ($script:poll) {
            1 { $offline; $online } # Historical phantom must not hide present device.
            2 { throw 'Synthetic PnP query failure' }
            3 { $offline }
            4 { } # No node: still absent, no duplicate disconnect.
        }
    }
    $tx = Join-Path $testRoot 'transactions.log'
    $events = Join-Path $testRoot 'events.log'
    & "$PSScriptRoot/monitor_usb.ps1" -LogFile $tx -EventLogFile $events -Samples 4 -PollMilliseconds 0
    $lines = @(Get-Content -LiteralPath $events)
    if (@($lines | Where-Object { $_ -match '\[DISCONNECT\]' }).Count -ne 1) { throw 'Expected one removal event' }
    if (@($lines | Where-Object { $_ -match '\[QUERY-ERROR\]' }).Count -ne 1) { throw 'Query error misclassified' }
    if ($lines[0] -notmatch 'Present=True') { throw 'Phantom selected over live device' }
    if (Test-Path -LiteralPath $tx) { throw 'Monitor wrote to transaction log' }
    foreach ($line in $lines) {
        if (@($global:LumienMonitorTestOutput | Where-Object { $_ -eq $line }).Count -ne 1) { throw 'Event echoed more than once' }
    }
    [Console]::WriteLine('PASS: present-node selection, missing node, query failure and single event output')
} finally {
    # Only remove files created by this test, never a computed directory tree.
    foreach ($file in @('events.log','transactions.log')) {
        $target = Join-Path $testRoot $file
        if (Test-Path -LiteralPath $target) { Remove-Item -LiteralPath $target }
    }
    [IO.Directory]::Delete($testRoot)
    Remove-Variable LumienMonitorTestOutput -Scope Global -ErrorAction SilentlyContinue
}
