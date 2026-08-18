<#
.SYNOPSIS
  Install or remove claude-code-sync on Windows.

.DESCRIPTION
  Installs to %LOCALAPPDATA%\Programs\claude-code-sync and adds that directory to your user
  PATH. No administrator rights needed, because nothing is written outside your profile.

.EXAMPLE
  irm https://raw.githubusercontent.com/ehsan18t/claude-code-sync/main/install.ps1 | iex

.EXAMPLE
  & ([scriptblock]::Create((irm https://raw.githubusercontent.com/ehsan18t/claude-code-sync/main/install.ps1))) -Uninstall
#>
[CmdletBinding()]
param(
    [switch]$Uninstall,
    [string]$InstallDir = "$env:LOCALAPPDATA\Programs\claude-code-sync"
)

$ErrorActionPreference = 'Stop'

$Repo = 'ehsan18t/claude-code-sync'
$Name = 'claude-code-sync'
$Exe = Join-Path $InstallDir "$Name.exe"

function Get-UserPath {
    [Environment]::GetEnvironmentVariable('Path', 'User')
}

function Add-ToPath {
    param([string]$Directory)
    $current = Get-UserPath
    $entries = @($current -split ';' | Where-Object { $_ })
    if ($entries -contains $Directory) { return $false }
    [Environment]::SetEnvironmentVariable('Path', (@($entries) + $Directory) -join ';', 'User')
    return $true
}

function Remove-FromPath {
    param([string]$Directory)
    $entries = @(Get-UserPath -split ';' | Where-Object { $_ -and $_ -ne $Directory })
    [Environment]::SetEnvironmentVariable('Path', $entries -join ';', 'User')
}

if ($Uninstall) {
    if (-not (Test-Path $InstallDir)) {
        Write-Error "$Name is not installed at $InstallDir"
        exit 1
    }
    Remove-Item -Recurse -Force $InstallDir
    Remove-FromPath $InstallDir
    Write-Host "Removed $InstallDir and its PATH entry."
    Write-Host ''
    Write-Host 'Your backups and config were not touched. To remove those as well:'
    Write-Host '  Remove-Item -Recurse -Force "$env:USERPROFILE\.claude\backups"'
    exit 0
}

$arch = switch ($env:PROCESSOR_ARCHITECTURE) {
    'AMD64' { 'x86_64' }
    'ARM64' { 'arm64' }
    default { throw "unsupported architecture: $env:PROCESSOR_ARCHITECTURE" }
}

$asset = "$Name-windows-$arch.exe"
$url = "https://github.com/$Repo/releases/latest/download/$asset"

Write-Host "Downloading $asset"
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
$temp = Join-Path ([IO.Path]::GetTempPath()) "$Name-$([guid]::NewGuid()).exe"

try {
    Invoke-WebRequest -Uri $url -OutFile $temp -UseBasicParsing
} catch {
    throw "download failed. Is there a published release with $asset? ($_)"
}

if ((Get-Item $temp).Length -eq 0) { throw 'downloaded file is empty' }

# Move-Item -Force fails while a previous copy is running; stop it first if we can.
Get-Process -Name $Name -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Move-Item -Force $temp $Exe

Write-Host "Installed to $Exe"

if (Add-ToPath $InstallDir) {
    $env:Path = "$env:Path;$InstallDir"
    Write-Host "Added $InstallDir to your user PATH."
    Write-Host 'Open a new terminal for it to take effect everywhere.'
}

Write-Host ''
& $Exe --version
Write-Host "Run '$Name' with no arguments for usage."
