# Installs cometdesk-agent for the current user on a Windows target, pairs it and starts it.
# Served by cometdesk-bridge on the Comet's USB link; cometdesk-gui types the command for you
# (no brackets or backslashes: they need AltGr on some layouts and may not arrive over HID):
#
#   powershell -NoProfile -ExecutionPolicy Bypass -Command "iwr -UseBasicParsing http://192.168.5.1:47475/install.ps1 -OutFile $env:TEMP/cometdesk-install.ps1; & $env:TEMP/cometdesk-install.ps1 <code>"
#
# No admin needed. Installs to %LOCALAPPDATA%\cometdesk-agent, starts at logon (HKCU Run key).
param(
    [Parameter(Mandatory = $true, Position = 0)][string]$Code,
    [Parameter(Mandatory = $false, Position = 1)][string]$ExpectedSha256
)
$ErrorActionPreference = 'Stop'
$relay = 'http://192.168.5.1:47475'
$dir = Join-Path $env:LOCALAPPDATA 'cometdesk-agent'
$exe = Join-Path $dir 'cometdesk-agent.exe'
$log = Join-Path $dir 'cometdesk-agent.log'
New-Item -ItemType Directory -Force -Path $dir | Out-Null

if ([string]::IsNullOrWhiteSpace($ExpectedSha256)) {
    Write-Host 'Querying expected checksum for windows-x86_64.exe from the Comet...'
    $shaResp = (Invoke-WebRequest -UseBasicParsing "$relay/agents/windows-x86_64.exe.sha256").Content.Trim()
    $ExpectedSha256 = ($shaResp -split '\s+')[0].Trim()
}
if ([string]::IsNullOrWhiteSpace($ExpectedSha256)) {
    throw 'Failed to obtain expected SHA-256 checksum for windows-x86_64.exe'
}

Write-Host 'Downloading cometdesk-agent from the Comet...'
Invoke-WebRequest -UseBasicParsing "$relay/agents/windows-x86_64.exe" -OutFile "$exe.new"

if (Get-Command Get-FileHash -ErrorAction SilentlyContinue) {
    $actualHash = (Get-FileHash -Path "$exe.new" -Algorithm SHA256).Hash
} else {
    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    $stream = [System.IO.File]::OpenRead("$exe.new")
    $bytes = $sha256.ComputeHash($stream)
    $stream.Close()
    $actualHash = -join ($bytes | ForEach-Object { $_.ToString("X2") })
}

if (-not $actualHash.Equals($ExpectedSha256, [System.StringComparison]::OrdinalIgnoreCase)) {
    Remove-Item -Force "$exe.new" -ErrorAction SilentlyContinue
    throw "Checksum verification failed for windows-x86_64.exe (expected $ExpectedSha256, got $actualHash)"
}
Write-Host "Verified cometdesk-agent binary integrity ($actualHash)."

Get-Process cometdesk-agent -ErrorAction SilentlyContinue | Stop-Process -Force
Move-Item -Force "$exe.new" $exe
# Leftovers of the former name (glkvm-agent).
Get-Process glkvm-agent -ErrorAction SilentlyContinue | Stop-Process -Force
Remove-Item -Recurse -Force (Join-Path $env:LOCALAPPDATA 'glkvm-agent') -ErrorAction SilentlyContinue
Remove-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name 'glkvm-agent' -ErrorAction SilentlyContinue

& $exe pair $Code | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'pairing failed' }

$run = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
Set-ItemProperty -Path $run -Name 'cometdesk-agent' -Value "`"$exe`" run --log `"$log`""
Start-Process -WindowStyle Hidden -FilePath $exe -ArgumentList @('run', '--log', "`"$log`"")
Write-Host "cometdesk-agent is installed, paired and running. It starts when you log on."
Write-Host "Log: $log    Remove: & '$exe' uninstall"
