$ErrorActionPreference = 'Stop'
# No double quotes in this script: it is passed on the command line.
function Say($text) { [Console]::WriteLine(($text -replace '\s+', ' ').Trim()) }
$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
$computer = $null
try {
    try {
        Add-Type -Path $env:LUMIEN_LHM_DLL
        $computer = New-Object LibreHardwareMonitor.Hardware.Computer
        $computer.IsCpuEnabled = $true
        $computer.Open()
    } catch { Say ('unavailable: Cannot load LibreHardwareMonitor from ' + $env:LUMIEN_LHM_DLL + ': ' + $_.Exception.Message); return }
    $cpus = @($computer.Hardware | Where-Object { $_.HardwareType.ToString() -eq 'Cpu' })
    while ($null -ne [Console]::ReadLine()) {
        try {
            $values = @()
            foreach ($hardware in $cpus) {
                $hardware.Update()
                $sensors = @($hardware.Sensors | Where-Object { $_.SensorType.ToString() -eq 'Temperature' -and $null -ne $_.Value -and [double]$_.Value -gt 0 })
                $preferred = @($sensors | Where-Object { $_.Name -in @('CPU Package', 'Core (Tctl/Tdie)', 'Core (Tdie)', 'Core (Tctl)') })
                if ($preferred.Count -eq 0) { $preferred = @($sensors | Where-Object { $_.Name -match '^CPU Core #\d+$|^Core #\d+$|^Core (Average|Max)$|^CCD\d+ \(Tdie\)$' }) }
                foreach ($sensor in $preferred) { $values += [double]$sensor.Value }
            }
            if ($values.Count -gt 0) {
                Say (($values | Measure-Object -Maximum).Maximum.ToString([Globalization.CultureInfo]::InvariantCulture))
            } elseif ($cpus.Count -eq 0) {
                Say 'unavailable: LibreHardwareMonitor found no CPU'
            } elseif (-not $admin) {
                Say 'unavailable: CPU temperature needs administrator rights. Start Lumien with Run as administrator.'
            } else {
                Say 'unavailable: CPU sensors returned no temperature. The LibreHardwareMonitor driver may be blocked by Windows or antivirus, or this CPU is unsupported.'
            }
        } catch { Say ('unavailable: ' + $_.Exception.Message) }
    }
} finally { if ($null -ne $computer) { $computer.Close() } }
