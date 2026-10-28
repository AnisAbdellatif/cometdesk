# Installs glkvm-agent for the current user on a Windows target, pairs it and starts it.
# Served by glkvm-bridge on the Comet's USB link; glkvm-gui types the command for you
# (no brackets or backslashes: they need AltGr on some layouts and may not arrive over HID):
#
#   powershell -NoProfile -ExecutionPolicy Bypass -Command "iwr -UseBasicParsing http://192.168.5.1:47475/install.ps1 -OutFile $env:TEMP/glkvm-install.ps1; & $env:TEMP/glkvm-install.ps1 <code>"
#
# No admin needed. Installs to %LOCALAPPDATA%\glkvm-agent, starts at logon (HKCU Run key).
param([Parameter(Mandatory = $true)][string]$Code)
$ErrorActionPreference = 'Stop'
$relay = 'http://192.168.5.1:47475'
$dir = Join-Path $env:LOCALAPPDATA 'glkvm-agent'
$exe = Join-Path $dir 'glkvm-agent.exe'
$log = Join-Path $dir 'glkvm-agent.log'
New-Item -ItemType Directory -Force -Path $dir | Out-Null

Write-Host 'Downloading glkvm-agent from the Comet...'
Invoke-WebRequest -UseBasicParsing "$relay/agents/windows-x86_64.exe" -OutFile "$exe.new"
Get-Process glkvm-agent -ErrorAction SilentlyContinue | Stop-Process -Force
Move-Item -Force "$exe.new" $exe

& $exe pair $Code | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'pairing failed' }

$run = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
Set-ItemProperty -Path $run -Name 'glkvm-agent' -Value "`"$exe`" run --log `"$log`""
Start-Process -WindowStyle Hidden -FilePath $exe -ArgumentList @('run', '--log', "`"$log`"")
Write-Host "glkvm-agent is installed, paired and running. It starts when you log on."
Write-Host "Log: $log    Remove: & '$exe' uninstall"
