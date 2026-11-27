<#
.SYNOPSIS
  Çevrimdışı çevirinin dil modüllerini Pdfrp'nin "models-v3.0" Release'ine hazırlar: programa
  gömülen manifesti ve Release'e yüklenecek dosyaların listesini üretir.

.DESCRIPTION
  Girdi: Mozilla'nın Remote Settings hizmetinden bir kez çekilmiş modellerin klasörü
  (<Klasor>\<kaynak>-<hedef>\*.zst ve <Klasor>\models-manifest.json; Mozilla'daki özgün
  attachment.hash "sha256", decompressedHash "acik_sha256" alanında). Klasöre yazılmaz.

  Çıktılar:
  - crates\translate\models-v3.0.json: programa gömülen manifest (migez_translate::manifest).
    Her dosyanın Release'teki adı "<kaynak>-<hedef>.<Mozilla'daki dosya adı>" olur: en-zh-Hans ile
    en-zh-Hant'ın dosya adları Mozilla'da aynı (model.enzh..., srcvocab.enzh...), Release'te bir ad
    bir kez geçebilir. Ön ek her dosyaya konur, böylece kural tektir ve ad yönü söyler.
  - <Cikti>\models-manifest.json: aynı dosya, Release'e yüklenir (Mozilla'daki özgün SHA-256
    değerleriyle; hukuk incelemesi 26.11).
  - <Cikti>\yukleme-listesi.tsv: Release'teki ad, yerel dosya, bayt, SHA-256 (yüklenecek her dosya).
  - <Cikti>\SHA256SUMS.txt: Release'teki bütün dosyaların SHA-256'sı.
  - <Cikti>\LICENSE-MPL-2.0.txt: Mozilla Public License 2.0 metni (vendor\fxtranslate\LICENSE).

  Ağa çıkmaz; yalnız yerel dosyaları okur ve yazar. Yükleme (gh release) bu betiğin işi değildir.
  Model Release'i "Latest" yapılmaz (gh release create ... --latest=false): güncelleme denetimi
  releases/latest'i okur ve "models-v3.0" bir sürüm numarası değildir.

.EXAMPLE
  pwsh -NoProfile -File kurulum\ceviri-modelleri.ps1 -Klasor <models-v3 klasörü> -Cikti <çıktı klasörü>
#>
param(
    [Parameter(Mandatory)] [string] $Klasor,
    [string] $Cikti = (Join-Path $PSScriptRoot 'cikti\models-v3.0'),
    [string] $Etiket = 'models-v3.0',
    [string] $Baslik = 'Pdfrp dil modülleri 3.0',
    [string] $Depo = 'ceceys/pdfrp'
)
$ErrorActionPreference = 'Stop'
$Kok = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$Utf8 = [System.Text.UTF8Encoding]::new($false)
$Taban = "https://github.com/$Depo/releases/download/$Etiket/"

$kaynakManifest = Get-Content -Raw -Encoding utf8 -LiteralPath (Join-Path $Klasor 'models-manifest.json') | ConvertFrom-Json
$yonler = [System.Collections.Generic.List[object]]::new()
$liste = [System.Collections.Generic.List[string]]::new()
$sums = [System.Collections.Generic.List[string]]::new()
$adlar = [System.Collections.Generic.HashSet[string]]::new()
$toplam = [long]0
$liste.Add("ad`tyerel_dosya`tbayt`tsha256")
foreach ($y in ($kaynakManifest.yonler | Sort-Object kaynak, hedef)) {
    $yon = "$($y.kaynak)-$($y.hedef)"
    $dosyalar = [System.Collections.Generic.List[object]]::new()
    foreach ($d in $y.dosyalar) {
        $yerel = Join-Path (Join-Path $Klasor $yon) $d.dosya
        $item = Get-Item -LiteralPath $yerel
        if ($item.Length -ne [long]$d.bayt) { throw "$yerel : boyut $($item.Length), manifestte $($d.bayt)" }
        $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $yerel).Hash.ToLowerInvariant()
        if ($hash -ne $d.sha256) { throw "$yerel : SHA-256 manifesttekiyle tutmuyor" }
        $ad = "$yon.$($d.dosya)"
        if (-not $adlar.Add($ad)) { throw "Release'te iki dosya aynı adı alıyor: $ad" }
        if ($ad -notmatch '^[A-Za-z0-9._-]+$') { throw "Release adı izin verilmeyen karakter içeriyor: $ad" }
        $dosyalar.Add([ordered]@{
            kind            = $d.tur
            name            = $d.ad
            asset           = $ad
            size            = [long]$d.bayt
            sha256          = $d.sha256
            unpacked_size   = [long]$d.acik_bayt
            unpacked_sha256 = $d.acik_sha256
        })
        $liste.Add("$ad`t$yerel`t$($d.bayt)`t$($d.sha256)")
        $sums.Add("$($d.sha256)  $ad")
        $toplam += [long]$d.bayt
    }
    $yonler.Add([ordered]@{ from = $y.kaynak; to = $y.hedef; version = $y.surum; architecture = $y.mimari; files = $dosyalar })
}

$diller = @($yonler | ForEach-Object { if ($_.from -ne 'en') { $_.from } else { $_.to } } | Sort-Object -Unique)
$manifest = [ordered]@{
    tag         = $Etiket
    title       = $Baslik
    base_url    = $Taban
    source      = $kaynakManifest.kaynak
    fetched     = $kaynakManifest.cekilme
    license     = $kaynakManifest.lisans
    hashes      = 'sha256 is Mozilla''s attachment.hash of the .zst file, unpacked_sha256 its decompressedHash, both as Mozilla published them; the files are republished unmodified.'
    asset_names = '<from>-<to>.<Mozilla''s file name>'
    languages   = $diller.Count
    directions  = $yonler
}
$json = ($manifest | ConvertTo-Json -Depth 8) -replace "`r`n", "`n"

$gomulu = Join-Path $Kok 'crates\translate\models-v3.0.json'
[System.IO.File]::WriteAllText($gomulu, $json + "`n", $Utf8)
New-Item -ItemType Directory -Force -Path $Cikti | Out-Null
[System.IO.File]::WriteAllText((Join-Path $Cikti 'models-manifest.json'), $json + "`n", $Utf8)
$lisans = Join-Path $Cikti 'LICENSE-MPL-2.0.txt'
Copy-Item -LiteralPath (Join-Path $Kok 'vendor\fxtranslate\LICENSE') -Destination $lisans -Force
foreach ($ek in @('models-manifest.json', 'LICENSE-MPL-2.0.txt')) {
    $yol = Join-Path $Cikti $ek
    $h = (Get-FileHash -Algorithm SHA256 -LiteralPath $yol).Hash.ToLowerInvariant()
    $liste.Add("$ek`t$yol`t$((Get-Item -LiteralPath $yol).Length)`t$h")
    $sums.Add("$h  $ek")
    $toplam += (Get-Item -LiteralPath $yol).Length
}
[System.IO.File]::WriteAllText((Join-Path $Cikti 'SHA256SUMS.txt'), (($sums | Sort-Object { $_.Substring(66) }) -join "`n") + "`n", $Utf8)
$sumsYol = Join-Path $Cikti 'SHA256SUMS.txt'
$liste.Add("SHA256SUMS.txt`t$sumsYol`t$((Get-Item -LiteralPath $sumsYol).Length)`t-")
$toplam += (Get-Item -LiteralPath $sumsYol).Length
[System.IO.File]::WriteAllText((Join-Path $Cikti 'yukleme-listesi.tsv'), ($liste -join "`n") + "`n", $Utf8)

"{0} yön, {1} dil, Release'e {2} dosya, toplam {3:N0} bayt ({4:N2} GB)" -f $yonler.Count, $diller.Count, ($liste.Count - 1), $toplam, ($toplam / 1GB)
"Gömülü manifest: $gomulu"
"Yükleme listesi: $(Join-Path $Cikti 'yukleme-listesi.tsv')"
