<#
.SYNOPSIS
    cargo-cleanme public bootstrap installer for Windows.

.DESCRIPTION
    Binary-first, fail-closed installer for the verified GitHub release assets.
    Producer asset names and target triples are owned by Eggpack
    (release/eggpack/distribution.toml); this wrapper owns latest/exact version
    selection, host mapping, integrity and identity verification, destination
    scope, and the Cargo source fallback policy.

    Security posture:
      * HTTPS only; TLS 1.2 is the enforced floor.
      * The SHA-256 sidecar is mandatory. A missing or malformed digest is a
        hard failure, never a Cargo fallback.
      * The downloaded candidate must print exactly "cargo-cleanme X.Y.Z"
        before it is placed.
      * No internal privilege escalation: this script never calls Start-Process
        with -Verb RunAs. Run it yourself from an elevated prompt for a
        machine-wide install.
      * The machine PATH is never modified. Bounded guidance is printed when
        the install directory is not on PATH.

.PARAMETER Version
    Exact X.Y.Z release to install instead of the latest stable one.

.PARAMETER Directory
    Install directory. Defaults to a user-local application bin location, or
    the system location when already running elevated.

.PARAMETER Force
    Replace an existing cargo-cleanme in the install directory.

.EXAMPLE
    irm https://raw.githubusercontent.com/dbowm91/cargo-cleanme/main/packaging/install.ps1 | iex

.EXAMPLE
    ./install.ps1 -Version 0.1.0
#>
[CmdletBinding()]
param(
    [string] $Version,
    [string] $Directory,
    [switch] $Force
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$Product = 'cargo-cleanme'
$Repo = 'dbowm91/cargo-cleanme'
# Overridable for deterministic local fixture tests. The default is the public
# release origin; a non-HTTPS override is refused by the transport checks.
$BaseUrl = if ($env:CARGO_CLEANME_INSTALL_BASE_URL) { $env:CARGO_CLEANME_INSTALL_BASE_URL } else { "https://github.com/$Repo/releases/download" }
$LatestUrl = if ($env:CARGO_CLEANME_INSTALL_LATEST_URL) { $env:CARGO_CLEANME_INSTALL_LATEST_URL } else { "https://github.com/$Repo/releases/latest/download" }
$UserAgent = "$Product-installer"

function Stop-Install {
    param([string] $Message)
    Write-Error "$Product`: $Message"
    exit 1
}

function Write-Note {
    param([string] $Message)
    Write-Host $Message
}

# ---------------------------------------------------------------- validation
if ($Version -and $Version -notmatch '^\d+\.\d+\.\d+$') {
    Stop-Install "-Version must be an exact X.Y.Z release, got: $Version"
}

# ---------------------------------------------------------------- host mapping
# Triples and aliases below are contract projections; keep them in sync via
# scripts/check-installer-contract.py.
if (-not $IsWindows) {
    Stop-Install 'install.ps1 is the Windows installer. Use install.sh on Linux and macOS.'
}

$architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
switch ($architecture) {
    'X64' { $archFamily = 'x64' }
    'Arm64' { $archFamily = 'arm64' }
    default { $archFamily = 'unsupported' }
}

# The published target set. A non-x64 Windows host has no prebuilt binary and
# is Cargo-only.
$target = ''
if ($archFamily -eq 'x64') {
    $target = 'x86_64-pc-windows-msvc'
}

$cargoFallbackHost = $false
if (-not $target) {
    $cargoFallbackHost = $true
    Write-Note "$Product`: windows/$archFamily has no prebuilt binary; using the Cargo source fallback."
}

# ------------------------------------------------------------- destination
$isElevated = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)

if (-not $Directory) {
    if ($isElevated) {
        $Directory = Join-Path $env:ProgramFiles 'cargo-cleanme\bin'
    }
    else {
        $localAppData = if ($env:LOCALAPPDATA) { $env:LOCALAPPDATA } else { Join-Path $env:USERPROFILE 'AppData\Local' }
        $Directory = Join-Path $localAppData 'cargo-cleanme\bin'
    }
}

$binaryName = "$Product.exe"
$destination = Join-Path $Directory $binaryName

if ((Test-Path -LiteralPath $destination) -and -not $Force) {
    Stop-Install "$destination already exists. Re-run with -Force to replace it."
}

# ------------------------------------------------------------------- helpers
function Get-Sha256 {
    param([string] $Path)
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

# Download to a file. Any transport, TLS, or timeout failure is a hard error.
function Get-RemoteFile {
    param([string] $Uri, [string] $OutFile)

    if ($Uri -notmatch '^https://') {
        if ($Uri -notmatch '^http://' -or $env:CARGO_CLEANME_INSTALL_ALLOW_INSECURE -ne '1') {
            Stop-Install "refusing a non-HTTPS release URL: $Uri"
        }
    }

    # A fresh service point per invocation keeps a poisoned session or a stale
    # proxy config from silently changing behavior between runs.
    $previous = $null
    try { $previous = [System.Net.ServicePointManager]::SecurityProtocol } catch { $previous = $null }
    try {
        [System.Net.ServicePointManager]::SecurityProtocol = [System.Net.SecurityProtocolType]::Tls12
        $client = New-Object System.Net.Http.HttpClient
        $client.Timeout = [TimeSpan]::FromSeconds(300)
        $client.DefaultRequestHeaders.Add('User-Agent', $UserAgent)
        $response = $client.GetAsync($Uri, [System.Net.Http.HttpCompletionOption]::ResponseHeadersRead).GetAwaiter().GetResult()
        if ($response.StatusCode -eq [System.Net.HttpStatusCode]::NotFound) {
            return $false
        }
        $response.EnsureSuccessStatusCode() | Out-Null
        $bytes = $response.Content.ReadAsByteArrayAsync().GetAwaiter().GetResult()
        [System.IO.File]::WriteAllBytes($OutFile, $bytes)
        return $true
    }
    finally {
        if ($previous) { [System.Net.ServicePointManager]::SecurityProtocol = $previous }
    }
}

function Get-CandidateVersion {
    param([string] $Path)
    try {
        $output = & $Path --version 2>$null
        if ($LASTEXITCODE -ne 0) { return '' }
        $line = ($output | Select-Object -First 1)
        if ($line -notmatch '^(\S+)\s+(\S+)$') { return '' }
        return @{
            Product  = $Matches[1]
            Version  = $Matches[2]
            Release  = ($Matches[2] -match '^\d+\.\d+\.\d+$')
        }
    }
    catch {
        return ''
    }
}

# ------------------------------------------------------------------- install
$workDir = Join-Path ([System.IO.Path]::GetTempPath()) ("$Product-" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $workDir -Force | Out-Null

try {
    if (-not $cargoFallbackHost) {
        $asset = "$Product-$target.exe"
        $sidecar = "$asset.sha256"

        if ($Version) {
            $binUrl = "$BaseUrl/v$Version/$asset"
            $sumUrl = "$BaseUrl/v$Version/$sidecar"
        }
        else {
            $binUrl = "$LatestUrl/$asset"
            $sumUrl = "$LatestUrl/$sidecar"
        }

        Write-Note "$Product`: downloading $asset for $target..."
        $staged = Join-Path $workDir $binaryName

        if (-not (Get-RemoteFile -Uri $binUrl -OutFile $staged)) {
            # Absence of the selected binary is the one condition that may
            # enter the Cargo source fallback. A transport or TLS failure is
            # not absence.
            Write-Note "$Product`: no published binary for this release; using the Cargo source fallback."
            $cargoFallbackHost = $true
        }
        elseif ((Get-Item -LiteralPath $staged).Length -eq 0) {
            Stop-Install "downloaded artifact is empty: $binUrl"
        }
    }

    if (-not $cargoFallbackHost) {
        # The digest sidecar is mandatory evidence. Its absence is a hard
        # failure, not a fallback signal.
        $sumPath = Join-Path $workDir $sidecar
        if (-not (Get-RemoteFile -Uri $sumUrl -OutFile $sumPath)) {
            Stop-Install "release checksum not found: $sumUrl"
        }
        $expected = ((Get-Content -LiteralPath $sumPath -TotalCount 1) -split '\s+')[0]
        if ($expected -notmatch '^[0-9a-f]{64}$') {
            Stop-Install "malformed checksum file: $sumUrl"
        }
        $actual = Get-Sha256 -Path $staged
        if ($actual -ne $expected) {
            Stop-Install "checksum mismatch for $asset`n  expected $expected`n  actual   $actual"
        }

        $identity = Get-CandidateVersion -Path $staged
        if (-not $identity) {
            Stop-Install 'the downloaded candidate did not report a version'
        }
        if ($identity.Product -ne $Product) {
            Stop-Install "the downloaded candidate reports product $($identity.Product), expected $Product"
        }
        if (-not $identity.Release) {
            Stop-Install "the downloaded candidate reported a non-release version: $($identity.Version)"
        }
        if ($Version -and $identity.Version -ne $Version) {
            Stop-Install "requested $Version but the candidate reports $($identity.Version)"
        }
        $resolvedVersion = $identity.Version
        $source = $staged
    }

    if ($cargoFallbackHost) {
        if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
            Stop-Install 'cargo is required for the source fallback but was not found'
        }
        if ($Version) {
            Write-Note "$Product`: building $Product $Version from source with Cargo..."
        }
        else {
            Write-Note "$Product`: building $Product from source with Cargo..."
        }
        # Build into an invocation-owned private root so a failed build never
        # touches the destination or the user's Cargo installation state.
        $cargoRoot = Join-Path $workDir 'cargo-root'
        New-Item -ItemType Directory -Path $cargoRoot -Force | Out-Null
        $cargoArgs = @('install', $Product, '--locked', '--root', $cargoRoot)
        if ($Version) { $cargoArgs += @('--version', "=$Version") }
        & cargo @cargoArgs
        if ($LASTEXITCODE -ne 0) {
            Stop-Install 'cargo install failed'
        }
        $source = Join-Path (Join-Path $cargoRoot 'bin') $binaryName
        if (-not (Test-Path -LiteralPath $source)) {
            Stop-Install "Cargo reported success but $source was not produced"
        }
        $identity = Get-CandidateVersion -Path $source
        if (-not $identity -or -not $identity.Release) {
            Stop-Install 'the Cargo-built candidate reported an unusable version'
        }
        if ($Version -and $identity.Version -ne $Version) {
            Stop-Install "requested $Version but the Cargo build produced $($identity.Version)"
        }
        $resolvedVersion = $identity.Version
    }

    if (-not (Test-Path -LiteralPath $Directory)) {
        New-Item -ItemType Directory -Path $Directory -Force | Out-Null
    }
    if (-not (Test-Path -LiteralPath $Directory)) {
        Stop-Install "install directory could not be created: $Directory"
    }

    # Copy, verify the placed bytes, then report. A partial write cannot
    # masquerade as a successful install.
    Copy-Item -LiteralPath $source -Destination $destination -Force
    $placed = Get-Sha256 -Path $destination
    $sourceHash = Get-Sha256 -Path $source
    if ($placed -ne $sourceHash) {
        Remove-Item -LiteralPath $destination -Force -ErrorAction SilentlyContinue
        Stop-Install 'installed bytes do not match the verified candidate'
    }

    Write-Host ''
    Write-Host "$Product $resolvedVersion installed to $destination"

    # Never mutate the machine PATH; only report.
    $machinePath = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    $onPath = (($machinePath, $userPath) -join ';').Split(';') -contains $Directory
    if (-not $onPath) {
        Write-Host ''
        Write-Host "$Directory is not on your PATH. Add it with:"
        Write-Host "  [Environment]::SetEnvironmentVariable('Path', [Environment]::GetEnvironmentVariable('Path', 'User') + ';$Directory', 'User')"
    }
}
finally {
    # Clean only invocation-owned temporary state.
    Remove-Item -LiteralPath $workDir -Recurse -Force -ErrorAction SilentlyContinue
}

exit 0
