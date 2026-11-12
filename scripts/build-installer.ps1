$ErrorActionPreference = "Stop"
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
# Fail before touching release artifacts when the installer tool is unavailable.
$makensis = (Get-ChildItem "C:\Program Files*\NSIS\makensis.exe", "$env:LOCALAPPDATA\Programs\NSIS\makensis.exe" -ErrorAction SilentlyContinue | Select-Object -First 1).FullName
if (-not $makensis) {
    throw "makensis.exe not found. Install NSIS before packaging."
}

$version = (Get-Content "lumien-app\src-tauri\Cargo.toml" | Select-String '^version\s*=\s*"(.*)"').Matches.Groups[1].Value
if (-not $version) { throw "Lumien version missing from Cargo.toml." }

# Build desktop release binary (runs frontend build + embeds assets)
Write-Host "Building desktop release binary via Tauri..."
pnpm --dir lumien-app tauri build --no-bundle
if ($LASTEXITCODE -ne 0) { throw "Desktop release build failed ($LASTEXITCODE)." }

Write-Host "Building CLI release binary..."
cargo build --release --bin lumien-cli
if ($LASTEXITCODE -ne 0) { throw "CLI release build failed ($LASTEXITCODE)." }

# Ensure output directory exists
New-Item -ItemType Directory -Force -Path "releases\download" | Out-Null

Write-Host "Building NSIS installer..."
& $makensis "installer.nsi"
if ($LASTEXITCODE -ne 0) { throw "Installer build failed ($LASTEXITCODE)." }

# Copy standalone binaries
Copy-Item -Path "lumien-app\src-tauri\target\release\lumien-app.exe" -Destination "releases\download\lumien-app.exe" -Force
Copy-Item -Path "lumien-app\src-tauri\target\release\lumien-app.exe" -Destination "releases\download\lumien_${version}_x64.exe" -Force
if (Test-Path "target\release\lumien-cli.exe") {
    Copy-Item -Path "target\release\lumien-cli.exe" -Destination "releases\download\lumien-cli.exe" -Force
}

# Create zip bundle
Compress-Archive -Path "lumien-app\src-tauri\target\release\lumien-app.exe", "README.md" -DestinationPath "releases\download\lumien-v${version}-windows-x64.zip" -Force

# Update SHA256 hashes
$sha256 = [System.Security.Cryptography.SHA256]::Create()
try {
    $hashes = Get-ChildItem -LiteralPath "releases\download" -File | Where-Object Name -ne "SHA256SUMS.txt" | ForEach-Object {
        $stream = [System.IO.File]::OpenRead($_.FullName)
        try {
            $hex = [BitConverter]::ToString($sha256.ComputeHash($stream)).Replace("-", "")
            "$hex  $($_.Name)"
        } finally {
            $stream.Dispose()
        }
    }
} finally {
    $sha256.Dispose()
}
$hashes | Out-File -Encoding utf8 "releases\download\SHA256SUMS.txt"

Write-Host "Build and packaging complete for version $version."
} finally {
    Pop-Location
}
