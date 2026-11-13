Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class WinCap {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hWnd);
  public struct RECT { public int Left, Top, Right, Bottom; }
}
"@

$root = Split-Path -Parent $PSScriptRoot

$candidates = @(
  (Join-Path $root 'emocloud.exe'),
  (Join-Path $root 'target\release\emocloud.exe'),
  (Join-Path $root 'target\debug\emocloud.exe'),
  (Join-Path $env:LOCALAPPDATA 'emocloud-target\release\emocloud.exe'),
  (Join-Path $env:LOCALAPPDATA 'emocloud-target\debug\emocloud.exe')
)
$exe = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $exe) { Write-Output 'NO EXE'; exit 1 }

if (-not (Get-Process emocloud -ErrorAction SilentlyContinue)) {
  Start-Process -FilePath $exe -WorkingDirectory (Split-Path $exe)
  Start-Sleep -Seconds 12
}

$p = Get-Process emocloud -ErrorAction SilentlyContinue |
  Where-Object { $_.MainWindowHandle -ne 0 } |
  Select-Object -First 1

if (-not $p) { Write-Output 'NO WINDOW'; exit 1 }
Write-Output ("TITLE: " + $p.MainWindowTitle)

if ([WinCap]::IsIconic($p.MainWindowHandle)) { [WinCap]::ShowWindow($p.MainWindowHandle, 9) | Out-Null }
[WinCap]::SetForegroundWindow($p.MainWindowHandle) | Out-Null
Start-Sleep -Milliseconds 900

$r = New-Object WinCap+RECT
[WinCap]::GetWindowRect($p.MainWindowHandle, [ref]$r) | Out-Null
$w = $r.Right - $r.Left
$h = $r.Bottom - $r.Top
Write-Output ("RECT: " + $w + "x" + $h)

$bmp = New-Object System.Drawing.Bitmap($w, $h)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($r.Left, $r.Top, 0, 0, $bmp.Size)
$bmp.Save((Join-Path $root 'shot-window.png'), [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose()
$bmp.Dispose()
Write-Output 'CAPTURED'
