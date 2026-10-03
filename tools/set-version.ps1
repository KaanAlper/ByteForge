# Sürümü tek komutla her yere yazar: package.json, src-tauri/tauri.conf.json, src-tauri/Cargo.toml,
# crates/byteforge-core/Cargo.toml (Cargo.lock bir sonraki cargo derlemesinde kendini günceller).
#   pwsh ./tools/set-version.ps1 -Version 1.2.3
param([Parameter(Mandatory)][string]$Version)
$ErrorActionPreference = 'Stop'
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "Sürüm x.y.z olmalı: $Version" }
$root = Split-Path $PSScriptRoot -Parent
$utf8 = New-Object Text.UTF8Encoding $false

function Set-InFile([string]$rel, [string]$pattern, [string]$replacement) {
    $path = Join-Path $root $rel
    $text = [IO.File]::ReadAllText($path)
    $re = [regex]::new($pattern, 'Multiline')
    if (-not $re.IsMatch($text)) { throw "$rel içinde sürüm satırı yok" }
    [IO.File]::WriteAllText($path, $re.Replace($text, $replacement, 1), $utf8)
    Write-Host "$rel -> $Version"
}
# JSON: yalnız üst düzey "version" (ilk eşleşme; iki dosyada da en üstte)
Set-InFile 'package.json' '^(\s*"version"\s*:\s*")[^"]+(")' "`${1}$Version`${2}"
Set-InFile 'src-tauri/tauri.conf.json' '^(\s*"version"\s*:\s*")[^"]+(")' "`${1}$Version`${2}"
# Cargo: [package] altındaki ilk version satırı
Set-InFile 'src-tauri/Cargo.toml' '^(version\s*=\s*")[^"]+(")' "`${1}$Version`${2}"
Set-InFile 'crates/byteforge-core/Cargo.toml' '^(version\s*=\s*")[^"]+(")' "`${1}$Version`${2}"
