# Builds the release, closes the running recorder, installs the new version silently and starts it.
#   pwsh -File scripts\release-install.ps1

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent

Write-Host 'Building release...' -ForegroundColor Cyan
Push-Location $root
try {
	npm run tauri build
	if ($LASTEXITCODE -ne 0) { throw "build failed ($LASTEXITCODE)" }
} finally {
	Pop-Location
}

$setup = Get-ChildItem (Join-Path $root 'src-tauri\target\release\bundle\nsis') -Filter '*-setup.exe' | Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $setup) { throw 'installer not found' }

Write-Host 'Closing the recorder...' -ForegroundColor Cyan
Get-Process drazox-recorder -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 800

Write-Host "Installing $($setup.Name)..." -ForegroundColor Cyan
Start-Process $setup.FullName -ArgumentList '/S' -Wait

# NSIS per-user installs land in %LOCALAPPDATA%\<productName>.
$exe = Join-Path $env:LOCALAPPDATA 'Drazox Recorder\drazox-recorder.exe'
if (-not (Test-Path $exe)) { throw "installed exe not found at $exe" }
Start-Process $exe
Write-Host "Running $exe" -ForegroundColor Green
