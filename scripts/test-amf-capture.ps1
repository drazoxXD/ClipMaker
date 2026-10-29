# Compares AMD's driver-level screen capture (vsrc_amf, the ReLive path) against what the recorder
# gets today, under real game load. Run it, switch into the game within 10 seconds, and play.
#   pwsh -File scripts\test-amf-capture.ps1              (desktop-duplication mode)
#   pwsh -File scripts\test-amf-capture.ps1 -Direct      (driver flip capture, works without composition)
param([switch]$Direct)

$ErrorActionPreference = 'Stop'
# duplicate_output=1 copies the composited desktop: nothing arrives while an exclusive-fullscreen game
# bypasses composition. Direct mode reads the displayed surface on every present instead.
$capture = if ($Direct) { 'duplicate_output=0:capture_mode=wait_for_present' } else { 'duplicate_output=1' }
$ff = Join-Path $PSScriptRoot '..\src-tauri\ffmpeg\bin'
$out = Join-Path $env:TEMP 'drx-amf-capture-test.mkv'

Write-Host 'Switch to your game now. Recording starts in 10 seconds and runs for 20 seconds...' -ForegroundColor Yellow
Start-Sleep 10
Write-Host 'Recording (AMF capture -> AMF scale -> HEVC AMF, 60 fps, 30 Mbps)...' -ForegroundColor Cyan
& "$ff\ffmpeg.exe" -hide_banner -loglevel error -y `
	-filter_complex "vsrc_amf=monitor_index=0:framerate=60:$capture,vpp_amf=w=2580:h=1080:format=nv12" `
	-c:v hevc_amf -b:v 30M -quality balanced -t 20 $out
[console]::Beep(880, 200)

$packets = & "$ff\ffprobe.exe" -v error -select_streams v:0 -count_packets -show_entries stream=nb_read_packets -of csv=p=0 $out
$unique = ((& "$ff\ffmpeg.exe" -hide_banner -i $out -vf mpdecimate -an -f null NUL 2>&1 | Select-String 'frame=' | Select-Object -Last 1).ToString()) -replace '.*frame=\s*(\d+).*', '$1'
Write-Host ''
Write-Host ("Frames: {0} in 20 s = {1:N1} fps" -f $packets, ($packets / 20))
Write-Host ("Unique: {0} = {1:N1} real fps (the number that matters; 60 is perfect)" -f $unique, ($unique / 20)) -ForegroundColor Green
Write-Host "Clip: $out"
