$ErrorActionPreference = 'Stop'
try {
    $dll = Join-Path $env:ProgramFiles 'KANALI\resources\Connect\LibreHardwareMonitorLib.dll'
    Add-Type -Path $dll
    $computer = New-Object LibreHardwareMonitor.Hardware.Computer
    $computer.IsCpuEnabled = $true
    $computer.Open()
    while ($null -ne [Console]::ReadLine()) {
        try {
            $values = @()
            foreach ($hardware in $computer.Hardware) {
                if ($hardware.HardwareType.ToString() -ne 'Cpu') { continue }
                $hardware.Update()
                $sensors = @($hardware.Sensors | Where-Object { $_.SensorType.ToString() -eq 'Temperature' -and $null -ne $_.Value })
                $preferred = @($sensors | Where-Object { $_.Name -in @('CPU Package', 'Core (Tctl/Tdie)', 'Core (Tdie)') })
                if ($preferred.Count -eq 0) { $preferred = @($sensors | Where-Object { $_.Name -match '^CPU Core #\d+$|^Core #\d+$|^Core Average$' }) }
                foreach ($sensor in $preferred) { $values += [double]$sensor.Value }
            }
            if ($values.Count -gt 0) {
                [Console]::WriteLine(($values | Measure-Object -Maximum).Maximum.ToString([Globalization.CultureInfo]::InvariantCulture))
            } else { [Console]::WriteLine('unavailable') }
        } catch { [Console]::WriteLine('unavailable') }
    }
} catch { [Console]::WriteLine('unavailable') }
finally { if ($null -ne $computer) { $computer.Close() } }
