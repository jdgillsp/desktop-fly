$env:DESKTOPFLY_DATA = "F:\projects\desktop-fly\data"
$env:DESKTOPFLY_SNAPSHOT_BG = "13151f"
Set-Location F:\projects\desktop-fly
$exe = "F:\projects\desktop-fly\rust\target-web\run\desktopfly.exe"
$root = "F:\projects\desktop-fly\output\showcase\frames"
$fps = 12; $secs = 10
$plan = @(
  @{ id = "drosophila"; start = 2.0 },
  @{ id = "salticid";   start = 3.0 },
  @{ id = "araneus";    start = 40.0 },
  @{ id = "koi";        start = 6.0 }
)
foreach ($p in $plan) {
  $dir = Join-Path $root $p.id; New-Item -ItemType Directory -Force $dir | Out-Null
  $sw = [Diagnostics.Stopwatch]::StartNew()
  for ($i = 0; $i -lt ($fps * $secs); $i++) {
    $t = [math]::Round($p.start + $i / $fps, 4)
    $out = Join-Path $dir ("f{0:D4}.png" -f $i)
    if (Test-Path $out) { continue }
    & $exe --creature $p.id --habitat --closeup --snapshot $out --size 900 --snapshot-seconds $t | Out-Null
  }
  "$($p.id): $((Get-ChildItem $dir).Count) frames in $([int]$sw.Elapsed.TotalSeconds)s"
}
"done"
