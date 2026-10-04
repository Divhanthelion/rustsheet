<#
.SYNOPSIS
  Build MSIX packages for the Microsoft Store: one per architecture, plus a
  .msixbundle when more than one is built.

.DESCRIPTION
  Builds the release exe for each architecture, stages it with the manifest
  and logos, generates resources.pri and packs the MSIX with the Windows SDK.
  With several architectures, the packages are bundled into
  target\msix\RustSheet_<version>.msixbundle, which is what to upload. The
  Store signs packages on submission, so the output is unsigned.

  For a Store upload, pass the three identity values from Partner Center
  (Apps and games > RustSheet > Product identity). The defaults are for local
  testing only.

  arm64 needs the Rust target (rustup target add aarch64-pc-windows-msvc)
  and Visual Studio's "MSVC ARM64 build tools".

.EXAMPLE
  .\packaging\windows\build-msix.ps1                       # x64 only
  .\packaging\windows\build-msix.ps1 -Arch x64,arm64 `
      -IdentityName 12345Divhanthelion.RustSheet `
      -Publisher "CN=XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX" -PublisherDisplayName Divhanthelion
#>
param(
    [string]$IdentityName = "RustSheet.Dev",
    [string]$Publisher = "CN=RustSheet Dev",
    [string]$PublisherDisplayName = "Divhanthelion",
    [ValidateSet("x64", "arm64")]
    [string[]]$Arch = @("x64"),
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

$triples = @{ x64 = "x86_64-pc-windows-msvc"; arm64 = "aarch64-pc-windows-msvc" }
$out = Join-Path $root "target\msix"
New-Item -ItemType Directory $out -Force | Out-Null
$packages = @()

foreach ($a in $Arch) {
    $triple = $triples[$a]
    # x64 builds where `cargo build --release` puts it, so local runs reuse it.
    $exe = if ($a -eq "x64") { "target\release\rustsheet.exe" } else { "target\$triple\release\rustsheet.exe" }
    if (-not $SkipBuild) {
        # cargo reports progress on stderr; don't let Windows PowerShell treat that as an error.
        $ErrorActionPreference = "Continue"
        if ($a -eq "x64") { cargo build --release } else { cargo build --release --target $triple }
        $ErrorActionPreference = "Stop"
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed for $a" }
    }

    $stage = Join-Path $out $(if ($a -eq "x64") { "stage" } else { "stage-$a" })
    if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
    New-Item -ItemType Directory $stage | Out-Null
    Copy-Item $exe $stage
    Copy-Item LICENSE $stage
    Copy-Item packaging\windows\Assets $stage -Recurse

    $manifest = (Get-Content packaging\windows\AppxManifest.xml -Raw) `
        -replace '\$IdentityName\$', [Security.SecurityElement]::Escape($IdentityName) `
        -replace '\$Publisher\$', [Security.SecurityElement]::Escape($Publisher) `
        -replace '\$PublisherDisplayName\$', [Security.SecurityElement]::Escape($PublisherDisplayName) `
        -replace '\$Version\$', $version `
        -replace '\$Arch\$', $a
    [IO.File]::WriteAllText((Join-Path $stage "AppxManifest.xml"), $manifest)

    # resources.pri maps Assets\Foo.png to the scale-/targetsize- variants.
    $priConfig = Join-Path $out "priconfig.xml"
    & $makepri createconfig /cf $priConfig /dq en-US /o | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "makepri createconfig failed" }
    & $makepri new /pr $stage /cf $priConfig /mn (Join-Path $stage "AppxManifest.xml") /of (Join-Path $stage "resources.pri") /o | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "makepri new failed" }

    $msix = Join-Path $out "RustSheet_${version}_$a.msix"
    & $makeappx pack /d $stage /p $msix /o | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "makeappx pack failed for $a" }
    $packages += $msix
    Write-Host "Built $msix (identity $IdentityName, version $version)"
}

if ($packages.Count -gt 1) {
    $bundleDir = Join-Path $out "bundle"
    if (Test-Path $bundleDir) { Remove-Item $bundleDir -Recurse -Force }
    New-Item -ItemType Directory $bundleDir | Out-Null
    foreach ($p in $packages) { Copy-Item $p $bundleDir }
    $bundle = Join-Path $out "RustSheet_${version}.msixbundle"
    & $makeappx bundle /d $bundleDir /p $bundle /bv $version /o | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "makeappx bundle failed" }
    Write-Host "Built ${bundle} (upload this one)"
}
