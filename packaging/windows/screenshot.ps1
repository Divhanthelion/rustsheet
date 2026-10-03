<#
.SYNOPSIS
  Capture assets\screenshot.png (README and Store listing) from the demo workbook.

.DESCRIPTION
  Uses PrintWindow, so it records only RustSheet's own window, even if other
  windows cover it. Waits past the 5 s status message so the file path the
  status bar shows on open is not captured.

.EXAMPLE
  cargo build --release; cargo run --release --example demo_workbook -- target\demo.xlsx
  .\packaging\windows\screenshot.ps1
#>
param(
    [string]$Workbook = "target\demo.xlsx",
    [string]$Out = "assets\screenshot.png",
    [int]$Wait = 7
)

$ErrorActionPreference = "Stop"
Set-Location (Resolve-Path "$PSScriptRoot\..\..")

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public class Capture {
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint f);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  public struct RECT { public int L, T, R, B; }
}
"@
# Without this, a scaled display reports a shrunken client rect.
[Capture]::SetProcessDPIAware() | Out-Null

$p = Start-Process target\release\rustsheet.exe -ArgumentList "`"$(Resolve-Path $Workbook)`"" -PassThru
try {
    Start-Sleep $Wait
    $p.Refresh()
    $h = $p.MainWindowHandle
    $r = New-Object Capture+RECT
    [Capture]::GetClientRect($h, [ref]$r) | Out-Null
    $bmp = New-Object System.Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $dc = $g.GetHdc()
    # 3 = PW_CLIENTONLY | PW_RENDERFULLCONTENT
    [Capture]::PrintWindow($h, $dc, 3) | Out-Null
    $g.ReleaseHdc($dc)
    $outPath = if ([IO.Path]::IsPathRooted($Out)) { $Out } else { Join-Path (Get-Location) $Out }
    $bmp.Save($outPath)
    Write-Host "Saved $Out ($($bmp.Width)x$($bmp.Height))"
} finally {
    Stop-Process $p -Force
}
