# Downloads the FFmpeg 9.0 shared build (BtbN, win64 gpl-shared) into src-tauri/ffmpeg: the headers
# and import libs the Rust bindings build against, and the DLLs shipped next to the app.
#   pwsh -File scripts\fetch-ffmpeg.ps1

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$dest = Join-Path $root 'src-tauri\ffmpeg'
$url = 'https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-n9.0-latest-win64-gpl-shared-9.0.zip'

if (Test-Path (Join-Path $dest 'bin\avcodec-*.dll')) {
	Write-Host "FFmpeg already present in $dest"
	return
}

$zip = Join-Path $env:TEMP 'clipmaker-ffmpeg.zip'
$tmp = Join-Path $env:TEMP 'clipmaker-ffmpeg'
Write-Host "Downloading $url"
Invoke-WebRequest -Uri $url -OutFile $zip
if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp }
Expand-Archive $zip $tmp
$inner = Get-ChildItem $tmp -Directory | Select-Object -First 1
New-Item -ItemType Directory -Force $dest | Out-Null
Copy-Item (Join-Path $inner.FullName '*') $dest -Recurse -Force
Remove-Item -Recurse -Force $tmp, $zip
Write-Host "FFmpeg ready in $dest"
