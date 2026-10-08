# repos-manager installer for Windows.
#
#   irm https://raw.githubusercontent.com/Dxsk/repos-manager/main/install/install.ps1 | iex
#
# Pin a version with $env:REPOS_MANAGER_VERSION = "v1.0.0" before running,
# or download the script and call it with -Version / -Source forge / -Uninstall.

[CmdletBinding()]
param(
    [string]$Version = $env:REPOS_MANAGER_VERSION,
    [ValidateSet('github', 'forge')]
    [string]$Source = 'github',
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'Programs\repos-manager'),
    [switch]$Uninstall
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
# Windows PowerShell 5.1 defaults to TLS 1.0, which GitHub rejects.
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$Asset = 'repos-manager-windows-x86_64.zip'
$GitHub = 'https://github.com/Dxsk/repos-manager'
$Forge = 'https://forge.infrasouveraine.fr/dxsk/repos-manager'
$ForgeApi = 'https://forge.infrasouveraine.fr/api/v1/repos/dxsk/repos-manager'

# Edit the raw registry value so %VAR% entries stay unexpanded (REG_EXPAND_SZ);
# [Environment]::SetEnvironmentVariable would flatten them.
function Set-UserPath([scriptblock]$Transform) {
    $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')
    $current = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    $parts = @($current -split ';' | Where-Object { $_ })
    $updated = @(& $Transform $parts) -join ';'
    if ($updated -eq $current) { return $false }
    $key.SetValue('Path', $updated, 'ExpandString')
    # Setting and clearing a dummy variable broadcasts WM_SETTINGCHANGE.
    [Environment]::SetEnvironmentVariable('REPOS_MANAGER_TMP', '1', 'User')
    [Environment]::SetEnvironmentVariable('REPOS_MANAGER_TMP', $null, 'User')
    return $true
}

if ($Uninstall) {
    if (Test-Path $InstallDir) { Remove-Item -Recurse -Force $InstallDir }
    [void](Set-UserPath { param($p) $p | Where-Object { $_ -ne $InstallDir } })
    Write-Host "Removed $InstallDir"
    return
}

if (-not [Environment]::Is64BitOperatingSystem) { throw 'Only 64-bit Windows is supported.' }

if ($Version -and -not $Version.StartsWith('v')) { $Version = "v$Version" }

if ($Source -eq 'forge') {
    if (-not $Version) { $Version = (Invoke-RestMethod "$ForgeApi/releases/latest").tag_name }
    $Base = "$Forge/releases/download/$Version"
} elseif ($Version) {
    $Base = "$GitHub/releases/download/$Version"
} else {
    $Base = "$GitHub/releases/latest/download"
}

$Tmp = Join-Path ([IO.Path]::GetTempPath()) ("repos-manager-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $Tmp | Out-Null
try {
    Write-Host "Downloading $Asset ($(if ($Version) { $Version } else { 'latest' })) from $Source..."
    Invoke-WebRequest "$Base/$Asset" -OutFile (Join-Path $Tmp $Asset) -UseBasicParsing
    Invoke-WebRequest "$Base/SHA256SUMS" -OutFile (Join-Path $Tmp 'SHA256SUMS') -UseBasicParsing

    $line = Get-Content (Join-Path $Tmp 'SHA256SUMS') | Where-Object { ($_ -split '\s+')[1] -replace '^\*', '' -eq $Asset }
    if (-not $line) { throw "$Asset not listed in SHA256SUMS" }
    $expected = ($line -split '\s+')[0].ToLower()
    $actual = (Get-FileHash (Join-Path $Tmp $Asset) -Algorithm SHA256).Hash.ToLower()
    if ($expected -ne $actual) { throw "Checksum mismatch for $Asset" }

    Expand-Archive (Join-Path $Tmp $Asset) -DestinationPath (Join-Path $Tmp 'x') -Force
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Copy-Item (Join-Path $Tmp 'x\repos-manager.exe') $InstallDir -Force
} finally {
    Remove-Item -Recurse -Force $Tmp -ErrorAction SilentlyContinue
}

Write-Host "Installed to $InstallDir\repos-manager.exe"
if (Set-UserPath { param($p) if ($p -contains $InstallDir) { $p } else { $p + $InstallDir } }) {
    Write-Host "Added $InstallDir to your user PATH. Open a new terminal to use 'repos-manager'."
}
