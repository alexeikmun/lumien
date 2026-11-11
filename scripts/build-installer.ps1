$ErrorActionPreference = "Stop"
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
# Fail before touching release artifacts when the installer tool is unavailable.
$makensis = (Get-ChildItem "C:\Program Files*\NSIS\makensis.exe", "$env:LOCALAPPDATA\Programs\NSIS\makensis.exe" -ErrorAction SilentlyContinue | Select-Object -First 1).FullName
if (-not $makensis) {
    throw "makensis.exe not found. Install NSIS before packaging."
}

$version = (Get-Content "lumien-app\src-tauri\Cargo.toml" | Select-String '^version\s*=\s*"(.*)"').Matches.Groups[1].Value
if (-not $version) { $version = "0.1.0" }

# Build frontend
Write-Host "Building frontend..."
pnpm --dir lumien-app build
if ($LASTEXITCODE -ne 0) { throw "Frontend build failed ($LASTEXITCODE)." }

# Build release binaries
Write-Host "Building desktop release binary..."
cargo build --release --manifest-path lumien-app/src-tauri/Cargo.toml
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
$hashes = Get-ChildItem "releases\download" -Exclude "SHA256SUMS.txt" | Get-FileHash -Algorithm SHA256 | ForEach-Object { "$($_.Hash)  $($_.Path | Split-Path -Leaf)" }
$hashes | Out-File -Encoding utf8 "releases\download\SHA256SUMS.txt"

Write-Host "Build and packaging complete for version $version."
} finally {
    Pop-Location
}
