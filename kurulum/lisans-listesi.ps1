<#
.SYNOPSIS
  Pdfrp.exe'ye derlenen üçüncü taraf Rust bileşenlerinin lisans listesini üretir.

.DESCRIPTION
  Cargo.lock'taki çözülmüş bağımlılıklardan, migez-pdf'in Windows hedeflerinde (MSVC ve GNU)
  programa giren bütün paketleri (normal bağımlılıklar; derleme betikleri ve makrolar programa
  girmez) bulur. Her paket için adını, sürümünü, SPDX lisansını, kaynağını ve paketin kendi
  lisans/telif dosyalarının metnini tek bir HTML dosyasına yazar. Kurulum bu dosyayı
  {app}\lisanslar\ucuncu-taraf-lisanslar.html olarak kurar (kurulum\pdfrp.iss).

  Ağa çıkmaz: cargo metadata ve cargo tree --offline --locked çalışır; bağımlılıklar önceden
  indirilmiş olmalıdır (bir derleme ya da cargo fetch sonrası).

.EXAMPLE
  pwsh -NoProfile -File kurulum/lisans-listesi.ps1 -Cikti kurulum/cikti/ucuncu-taraf-lisanslar.html
#>
param(
    [string] $Cikti = (Join-Path $PSScriptRoot 'cikti\ucuncu-taraf-lisanslar.html'),
    [string[]] $Hedefler = @('x86_64-pc-windows-msvc', 'x86_64-pc-windows-gnu'),
    [string] $Paket = 'migez-pdf'
)
$ErrorActionPreference = 'Stop'
$Kok = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$Bildirim = Join-Path $Kok 'Cargo.toml'

function Invoke-Cargo([string[]] $Arguments) {
    $out = & cargo @Arguments
    if ($LASTEXITCODE -ne 0) { throw "cargo $($Arguments -join ' ') çıkış kodu $LASTEXITCODE" }
    return $out
}

# Programa giren paketler (ad + sürüm), iki Windows hedefinin birleşimi.
$giren = [System.Collections.Generic.HashSet[string]]::new()
foreach ($hedef in $Hedefler) {
    $satirlar = Invoke-Cargo @('tree', '--manifest-path', $Bildirim, '--offline', '--locked', '-p', $Paket, '-e', 'normal', '--target', $hedef, '--prefix', 'none', '--format', '{p}')
    foreach ($s in $satirlar) {
        if ($s -match '^(\S+) v(\S+)') { [void]$giren.Add("$($Matches[1]) $($Matches[2])") }
    }
}

$meta = (Invoke-Cargo @('metadata', '--manifest-path', $Bildirim, '--offline', '--locked', '--format-version', '1')) -join "`n" | ConvertFrom-Json -Depth 64
$uyeler = [System.Collections.Generic.HashSet[string]]::new([string[]]$meta.workspace_members)
$paketler = $meta.packages | Where-Object { $giren.Contains("$($_.name) $($_.version)") -and -not $uyeler.Contains($_.id) } | Sort-Object name, version

function Escape-Html([string] $s) { [System.Net.WebUtility]::HtmlEncode($s) }

$sb = [System.Text.StringBuilder]::new()
[void]$sb.AppendLine('<!DOCTYPE html>')
[void]$sb.AppendLine('<html lang="tr"><head><meta charset="utf-8"><title>Pdfrp: üçüncü taraf bileşenlerin lisansları</title>')
[void]$sb.AppendLine('<style>body{font-family:Segoe UI,Arial,sans-serif;margin:2em;max-width:60em}pre{white-space:pre-wrap;background:#f4f4f4;padding:.6em;font-size:.85em}td,th{padding:.15em .6em;text-align:left;vertical-align:top}h2{margin-top:2em;font-size:1.1em}</style></head><body>')
[void]$sb.AppendLine('<h1>Pdfrp: üçüncü taraf bileşenlerin lisansları</h1>')
[void]$sb.AppendLine("<p>Pdfrp.exe aşağıdaki $($paketler.Count) açık kaynak Rust bileşeniyle derlenmiştir. Her birinin adı, sürümü, lisansı, kaynağı ve paketle birlikte dağıtılan lisans ve telif bildirimi aşağıdadır. Pdfrp'nin kendi lisansı ve üst kaynağın telif bildirimi bu klasördeki LICENSE-MIT ve NOTICE dosyalarında; yazı tipleri, simgeler ve veriler ATTRIBUTION.md dosyasındadır.</p>")
[void]$sb.AppendLine('<p>This list was generated from Cargo.lock: every third-party Rust package compiled into Pdfrp.exe, with its licence and the licence and copyright files it ships.</p>')
[void]$sb.AppendLine('<table><tr><th>Bileşen</th><th>Sürüm</th><th>Lisans</th></tr>')
foreach ($p in $paketler) {
    $lisans = if ($p.license) { $p.license } elseif ($p.license_file) { 'dosyada: ' + $p.license_file } else { '?' }
    [void]$sb.AppendLine("<tr><td><a href=""#$(Escape-Html "$($p.name)-$($p.version)")"">$(Escape-Html $p.name)</a></td><td>$(Escape-Html $p.version)</td><td>$(Escape-Html $lisans)</td></tr>")
}
[void]$sb.AppendLine('</table>')

# Kültürden bağımsız eşleşme: Türkçe Windows'ta büyük-küçük harf duyarsız -match "LICENSE"ı
# "licen…" ile eşleştirmez (I ve ı ayrı harfler).
$desen = [regex]::new('^(licen[cs]e|copying|notice|copyright|unlicense)([-_. ].*)?$', 'IgnoreCase, CultureInvariant')
$eksik = 0
foreach ($p in $paketler) {
    $klasor = Split-Path -Parent $p.manifest_path
    [void]$sb.AppendLine("<h2 id=""$(Escape-Html "$($p.name)-$($p.version)")"">$(Escape-Html $p.name) $(Escape-Html $p.version)</h2>")
    $lisans = if ($p.license) { $p.license } else { '' }
    $kaynak = if ($p.repository) { $p.repository } elseif ($p.homepage) { $p.homepage } else { '' }
    [void]$sb.AppendLine("<p>Lisans: $(Escape-Html $lisans)<br>Kaynak: $(Escape-Html $kaynak)<br>Yazarlar: $(Escape-Html (($p.authors | ForEach-Object { $_ }) -join ', '))</p>")
    $dosyalar = @(Get-ChildItem -LiteralPath $klasor -File | Where-Object { $desen.IsMatch($_.Name) } | Sort-Object Name)
    if ($p.license_file) {
        $ozel = Join-Path $klasor $p.license_file
        if ((Test-Path -LiteralPath $ozel -PathType Leaf) -and -not ($dosyalar | Where-Object { $_.FullName -eq (Resolve-Path -LiteralPath $ozel).Path })) { $dosyalar += Get-Item -LiteralPath $ozel }
    }
    if (-not $dosyalar) {
        $eksik++
        [void]$sb.AppendLine('<p><i>Paket ayrı bir lisans dosyası taşımıyor; lisansın metni SPDX adıyla anılan standart metindir (Apache-2.0 metni bu klasördeki LICENSE-APACHE dosyasındadır).</i></p>')
        continue
    }
    foreach ($d in $dosyalar) {
        $metin = [System.IO.File]::ReadAllText($d.FullName)
        if ($metin.Length -gt 200000) { $metin = $metin.Substring(0, 200000) + "`n[…]" }
        [void]$sb.AppendLine("<p><b>$(Escape-Html $d.Name)</b></p><pre>$(Escape-Html $metin)</pre>")
    }
}
[void]$sb.AppendLine('</body></html>')

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Cikti) | Out-Null
[System.IO.File]::WriteAllText($Cikti, $sb.ToString(), [System.Text.UTF8Encoding]::new($false))
Write-Output ("{0}: {1} bileşen ({2} tanesi ayrı lisans dosyası taşımıyor), {3:N0} KB" -f $Cikti, $paketler.Count, $eksik, ((Get-Item -LiteralPath $Cikti).Length / 1KB))
