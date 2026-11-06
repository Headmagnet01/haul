# Builds the post-processing test archives in .\archives from the files in .\src.
# Run once (outputs are committed):  powershell -NoProfile -File make-fixtures.ps1
# Needs the bundled 7-Zip and WinRAR's Rar.exe (only for the RAR fixtures).
$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
$sz = Join-Path $here '..\..\..\..\app\src-tauri\resources\7zip\7z.exe' | Resolve-Path
$rar = 'C:\Program Files\WinRAR\Rar.exe'
$src = Join-Path $here 'src'
$out = Join-Path $here 'archives'

# Source tree: hello.txt, inner\readme.md, and 64 KiB of fixed pseudo-random bytes so volumes really split.
New-Item -ItemType Directory -Force (Join-Path $src 'inner') | Out-Null
[IO.File]::WriteAllText((Join-Path $src 'hello.txt'), "hello haul`n")
[IO.File]::WriteAllText((Join-Path $src 'inner\readme.md'), "# inner`n")
$bytes = New-Object byte[] 65536
(New-Object Random 42).NextBytes($bytes)
[IO.File]::WriteAllBytes((Join-Path $src 'data.bin'), $bytes)

if (Test-Path $out) { Get-ChildItem $out | Remove-Item -Force }
New-Item -ItemType Directory -Force $out | Out-Null

function Run($exe, [string[]]$a) {
    & $exe @a | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "$exe $a exited $LASTEXITCODE" }
}

Push-Location $src
try {
    Run $sz @('a', '-tzip', "$out\plain.zip", 'hello.txt', 'inner')
    Run $sz @('a', '-t7z', "$out\plain.7z", 'hello.txt', 'inner')
    Run $sz @('a', '-t7z', '-mx0', '-v24k', "$out\split.7z", 'hello.txt', 'inner', 'data.bin')
    Run $rar @('a', '-idq', '-m0', '-v24k', '-r', "$out\multi.rar", 'hello.txt', 'inner', 'data.bin')
    Run $sz @('a', '-t7z', '-phaul-secret', '-mhe=on', "$out\secret.7z", 'hello.txt')
    Run $rar @('a', '-idq', '-phaul-secret', "$out\secret.rar", 'hello.txt')
} finally { Pop-Location }

# Old-style names (name.rar, .r00, .r01): RAR 7 can no longer write RAR4, so these are the RAR5
# volumes above under the old names. 7-Zip finds the next volume by the first volume's name.
Copy-Item "$out\multi.part1.rar" "$out\oldstyle.rar"
Copy-Item "$out\multi.part2.rar" "$out\oldstyle.r00"
Copy-Item "$out\multi.part3.rar" "$out\oldstyle.r01"

# damaged.7z: plain.7z with bytes 40-80 overwritten.
$d = [IO.File]::ReadAllBytes("$out\plain.7z")
for ($i = 40; $i -lt 80 -and $i -lt $d.Length; $i++) { $d[$i] = 0xFF }
[IO.File]::WriteAllBytes("$out\damaged.7z", $d)

# nested.zip holds plain.7z.
Push-Location $out
try { Run $sz @('a', '-tzip', 'nested.zip', 'plain.7z') } finally { Pop-Location }

# SHA256SUMS for plain.zip and plain.7z.
$lines = foreach ($f in 'plain.zip', 'plain.7z') {
    $h = (Get-FileHash -Algorithm SHA256 "$out\$f").Hash.ToLower()
    "$h *$f"
}
[IO.File]::WriteAllText("$out\SHA256SUMS", (($lines -join "`n") + "`n"))

Get-ChildItem $out | Select-Object Name, Length | Format-Table -AutoSize
