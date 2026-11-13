Add-Type -AssemblyName System.Drawing

# Генерирует из одной большой картинки весь набор иконок:
#   icons\icon.ico       - многоразмерный (16,24,32,48,64,128,256)
#   icons\32x32.png
#   icons\128x128.png
#   icons\128x128@2x.png (256)
#   icons\icon.png       (512)
#   assets\tray.png      (32)  - иконка в трее
#   assets\tray@2x.png   (64)

$root   = Split-Path -Parent $PSScriptRoot
$icons  = Join-Path $root 'icons'
$assets = Join-Path $root 'assets'

$iconSource = Join-Path $icons 'source.png'
if (-not (Test-Path $iconSource)) {
    Copy-Item (Join-Path $icons 'icon.png') $iconSource
}

$traySource = Join-Path $assets 'tray-source.png'
if (-not (Test-Path $traySource)) {
    Copy-Item (Join-Path $assets 'tray.png') $traySource
}

function Resize-Image {
    param([System.Drawing.Image]$Image, [int]$Size)
    $bmp = New-Object System.Drawing.Bitmap($Size, $Size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.PixelOffsetMode   = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.SmoothingMode     = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
    $g.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
    $g.DrawImage($Image, 0, 0, $Size, $Size)
    $g.Dispose()
    return $bmp
}

function Get-PngBytes {
    param([System.Drawing.Image]$Image, [int]$Size)
    $bmp = Resize-Image -Image $Image -Size $Size
    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    return ,$ms.ToArray()
}

function Save-Png {
    param([System.Drawing.Image]$Image, [int]$Size, [string]$Path)
    $bmp = Resize-Image -Image $Image -Size $Size
    $bmp.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    Write-Output ("  {0,-22} {1}x{1}  {2} KB" -f (Split-Path $Path -Leaf), $Size, [math]::Round((Get-Item $Path).Length / 1KB))
}

$iconMaster = [System.Drawing.Image]::FromFile($iconSource)
$trayMaster = [System.Drawing.Image]::FromFile($traySource)

"icon source: $($iconMaster.Width)x$($iconMaster.Height)"
"tray source: $($trayMaster.Width)x$($trayMaster.Height)"

# --- PNG ---
Save-Png $iconMaster 32  (Join-Path $icons '32x32.png')
Save-Png $iconMaster 128 (Join-Path $icons '128x128.png')
Save-Png $iconMaster 256 (Join-Path $icons '128x128@2x.png')
Save-Png $iconMaster 512 (Join-Path $icons 'icon.png')
Save-Png $trayMaster 32  (Join-Path $assets 'tray.png')
Save-Png $trayMaster 64  (Join-Path $assets 'tray@2x.png')

# --- ICO (кадры в PNG, Windows 10/11 поддерживает) ---
$sizes = @(16, 24, 32, 48, 64, 128, 256)
$frames = @()
foreach ($s in $sizes) { $frames += ,(Get-PngBytes -Image $iconMaster -Size $s) }

$ms = New-Object System.IO.MemoryStream
$bw = New-Object System.IO.BinaryWriter($ms)
$bw.Write([UInt16]0)               # reserved
$bw.Write([UInt16]1)               # 1 = icon
$bw.Write([UInt16]$sizes.Count)

$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $s = $sizes[$i]
    $data = $frames[$i]
    $dim = if ($s -ge 256) { 0 } else { $s }
    $bw.Write([byte]$dim)
    $bw.Write([byte]$dim)
    $bw.Write([byte]0)             # palette
    $bw.Write([byte]0)             # reserved
    $bw.Write([UInt16]1)           # planes
    $bw.Write([UInt16]32)          # bpp
    $bw.Write([UInt32]$data.Length)
    $bw.Write([UInt32]$offset)
    $offset += $data.Length
}
foreach ($d in $frames) { $bw.Write($d) }
$bw.Flush()
$icoPath = Join-Path $icons 'icon.ico'
[IO.File]::WriteAllBytes($icoPath, $ms.ToArray())
$bw.Dispose(); $ms.Dispose()
Write-Output ("  {0,-22} {1} kadrov  {2} KB" -f 'icon.ico', $sizes.Count, [math]::Round((Get-Item $icoPath).Length / 1KB))

$iconMaster.Dispose()
$trayMaster.Dispose()
"OK"
