<#
.SYNOPSIS
  Build target\msix\RustSheet_<version>_x64.msix for the Microsoft Store.

.DESCRIPTION
  Builds the release exe, stages it with the manifest and logos, generates
  resources.pri and packs the MSIX with the Windows SDK. The Store signs the
  package on submission, so the output is unsigned.

  For a Store upload, pass the three identity values from Partner Center
  (Apps and games > RustSheet > Product identity). The defaults are for local
  testing only.

.EXAMPLE
  .\packaging\windows\build-msix.ps1 -IdentityName 12345Divhanthelion.RustSheet `
      -Publisher "CN=XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX" -PublisherDisplayName Divhanthelion
#>
param(
    [string]$IdentityName = "RustSheet.Dev",
    [string]$Publisher = "CN=RustSheet Dev",
    [string]$PublisherDisplayName = "Divhanthelion",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$root = Resolve-Path "$PSScriptRoot\..\.."
Set-Location $root

# Store versions are Major.Minor.Build.0; the last field is reserved.
$cargoVersion = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"').Matches[0].Groups[1].Value
$version = "$($cargoVersion -replace '-.*$', '').0"

$sdkBin = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\10.*\x64\makeappx.exe" |
    Sort-Object { [version]$_.Directory.Parent.Name } | Select-Object -Last 1
if (-not $sdkBin) { throw "makeappx.exe not found. Install the Windows 10/11 SDK." }
$makeappx = $sdkBin.FullName
$makepri = Join-Path $sdkBin.DirectoryName "makepri.exe"

if (-not $SkipBuild) {
    # cargo reports progress on stderr; don't let Windows PowerShell treat that as an error.
    $ErrorActionPreference = "Continue"
    cargo build --release
    $ErrorActionPreference = "Stop"
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
}

$out = Join-Path $root "target\msix"
$stage = Join-Path $out "stage"
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
New-Item -ItemType Directory $stage | Out-Null

Copy-Item target\release\rustsheet.exe $stage
Copy-Item LICENSE $stage
Copy-Item packaging\windows\Assets $stage -Recurse

$manifest = (Get-Content packaging\windows\AppxManifest.xml -Raw) `
    -replace '\$IdentityName\$', [Security.SecurityElement]::Escape($IdentityName) `
    -replace '\$Publisher\$', [Security.SecurityElement]::Escape($Publisher) `
    -replace '\$PublisherDisplayName\$', [Security.SecurityElement]::Escape($PublisherDisplayName) `
    -replace '\$Version\$', $version
[IO.File]::WriteAllText((Join-Path $stage "AppxManifest.xml"), $manifest)

# resources.pri maps Assets\Foo.png to the scale-/targetsize- variants.
$priConfig = Join-Path $out "priconfig.xml"
& $makepri createconfig /cf $priConfig /dq en-US /o | Out-Null
if ($LASTEXITCODE -ne 0) { throw "makepri createconfig failed" }
& $makepri new /pr $stage /cf $priConfig /mn (Join-Path $stage "AppxManifest.xml") /of (Join-Path $stage "resources.pri") /o | Out-Null
if ($LASTEXITCODE -ne 0) { throw "makepri new failed" }

$msix = Join-Path $out "RustSheet_${version}_x64.msix"
& $makeappx pack /d $stage /p $msix /o | Out-Null
if ($LASTEXITCODE -ne 0) { throw "makeappx pack failed" }

Write-Host "Built $msix (identity $IdentityName, version $version)"
