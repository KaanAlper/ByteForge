<div align="center">

# ByteForge

**Tersine mühendislik ve modlama stüdyosu — tek uygulama, tek tık.**
*A cross-platform reverse-engineering & modding studio built in Rust.*

[![Release](https://img.shields.io/github/v/release/KaanAlper/ByteForge?display_name=tag)](https://github.com/KaanAlper/ByteForge/releases/latest)
[![CI](https://github.com/KaanAlper/ByteForge/actions/workflows/ci.yml/badge.svg)](https://github.com/KaanAlper/ByteForge/actions/workflows/ci.yml)
[![Platforms](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-2f6d8f)](#kurulum)

</div>

ByteForge; APK/IL2CPP modlama, native disassembly ve mini-decompile, Yara imza taraması, hex/diff ve Cheat Engine tarzı canlı bellek düzenlemeyi tek bir masaüstü uygulamasında toplar. Harici Il2CppDumper, Ghidra ya da Cheat Engine kurmadan çalışır: motorların tamamı native Rust'tır.

---

## Kurulum

### Linux — tek satır

```sh
curl -fsSL https://raw.githubusercontent.com/KaanAlper/ByteForge/master/install.sh | bash
```

En son sürümün taşınabilir `.AppImage`'ını `~/.local/bin/byteforge` olarak kurar; `sudo` istemez. Kurulum dizini `BYTEFORGE_BIN` ile değiştirilebilir. Paket yöneticisi tercih ediyorsan `.deb` / `.rpm` dosyalarını [Releases](https://github.com/KaanAlper/ByteForge/releases/latest) sayfasından indir.

### İndirilebilir paketler

| Platform | Paket | Not |
|---|---|---|
| Windows 10/11 | `ByteForge_x.y.z_x64-setup.exe` · `ByteForge_x.y.z_x64_en-US.msi` | İmzasız derleme: SmartScreen uyarısında *Daha fazla bilgi → Yine de çalıştır*. |
| macOS (Apple Silicon) | `ByteForge_x.y.z_aarch64.dmg` | İlk açılışta sağ tık → *Aç* (Gatekeeper). |
| macOS (Intel) | `ByteForge_x.y.z_x64.dmg` | Aynı şekilde. |
| Linux | `.AppImage` · `.deb` · `.rpm` | AppImage FUSE ister; yoksa `byteforge --appimage-extract-and-run`. |

Tüm paketler: **[github.com/KaanAlper/ByteForge/releases](https://github.com/KaanAlper/ByteForge/releases)**

### Uygulama içi güncelleme

Sol menünün altındaki **Güncelle** düğmesi yeni sürümü denetler, imzalı paketi indirir ve *"Güncelleme indirildi — Yeniden başlat"* der. Açılışta sessizce kontrol eder; yeni sürüm varsa düğmede küçük bir nokta belirir, iş akışını bölmez. Güncellemeler minisign ile imzalıdır; imzası doğrulanmayan paket kurulmaz.

---

## Neler yapıyor

### Android

| Adım | İşlev |
|---|---|
| Genel Bakış | Manifest, izinler, motor tespiti (IL2CPP / Mono / Flutter / native), packer & anti-tamper imzaları |
| Hedefler & Yama | **Native IL2CPP çözücü**: `global-metadata.dat` + `libil2cpp.so` içinden 100k+ metodu isim → RVA olarak çıkarır (v27–v31, ARM64 relocation uygulanır). Sezgisel puanlama ile para, can, reklam, satın alma hedeflerini öne çıkarır. Yama kalıpları: `return true/false/0/1/1.0f`, `nop`, `ret`. |
| Mod Menü | Sınıf/metot otomatik tamamlama ile yüzen mod menüsü enjeksiyonu |
| Dağıt & Kur | Yamalı `.so`'yu APK'ya geri paketler, zipalign + apksigner ile imzalar, `adb` ile kurar. `.xapk`/`.apks` bundle desteği. |

### Native analiz

- **Disassembler** — ARM64 ve x86-64, saf Rust (`yaxpeax`); harici araç yok.
- **Mini-decompiler** — Ghidra'nın mimarisi model alınarak yazıldı: sembolik register durumu, sadeleştirme kuralları, `cmp/cset` → bool, IL2CPP init-guard atlama. Basit getter/setter'ları C-benzeri sözde koda çevirir; karmaşık fonksiyonları dürüstçe işaretler.
- **Yara** — native `yara-x` motoru ile gömülü kural seti: UPX, AES S-box, ptrace anti-debug, root/emülatör/Frida tespiti, gömülü uç noktalar.

### Araçlar

| Araç | İşlev |
|---|---|
| Hex | Ofset/kalıp arama, doğrudan bayt düzenleme |
| Diff | İki ikili arasındaki farklar; modlu referanstan yama reçetesi öğrenme |
| **Bellek** | Cheat Engine paritesi: kesin + fuzzy tarama (*bilinmeyen ilk değer / arttı / azaldı / değişti / değişmedi / aralık*), **değer dondurma**, kayıtlı adres tablosu (canlı değer, isim, yaz), **pointer scan**. Windows / macOS / Linux için ortak `ProcessMemory` katmanı. |
| Frida | Hook betiği üretimi ve çalıştırma |
| Konsol | Tüm işlemlerin kaydı |

---

## Platform desteği

| | Windows | macOS | Linux |
|---|:---:|:---:|:---:|
| APK/IL2CPP modlama | ✓ | ✓ | ✓ |
| Disassembly / decompile / Yara | ✓ | ✓ | ✓ |
| Canlı bellek (Cheat Engine) | ✓ `ReadProcessMemory` | ✓ `mach_vm` (debugger yetkisi) | ✓ `/proc` (ptrace) |
| Uygulama içi güncelleme | ✓ | ✓ | ✓ AppImage |

Android akışı için makinede `adb`, `apksigner`, `zipalign` (Android SDK build-tools) ve isteğe bağlı `apktool`, `jadx` bulunmalı; **Ayarlar** sekmesi eksikleri ve kurulum komutunu gösterir.

---

## Kaynaktan derleme

Gerekenler: Rust (stable), Node 20+, Linux'ta `libwebkit2gtk-4.1-dev librsvg2-dev patchelf`.

```sh
git clone https://github.com/KaanAlper/ByteForge.git
cd ByteForge
npm ci
npm run tauri dev          # geliştirme
npm run tauri build        # platformuna göre kurulum paketleri
```

Kalite kapıları (CI'da da çalışır):

```sh
cargo clippy -p byteforge -p byteforge-core --all-targets -- -D warnings
cargo test  -p byteforge-core -p byteforge
npm run build
```

### Sürüm çıkarma

`v*` etiketi push'lamak yeterli — GitHub Actions Windows, macOS (arm64 + x64) ve Linux paketlerini derler, imzalar ve Release'e yükler; `latest.json` uygulama içi güncelleyiciyi besler.

```sh
git tag v0.2.0 && git push origin v0.2.0
```

---

## Mimari

```
crates/byteforge-core/   saf Rust motorlar: il2cpp_resolve, patch, disasm, decompile, yara, memscan, sign …
src-tauri/               Tauri 2 kabuğu: komutlar, çoklu platform bellek katmanı (mem/), freeze, pointerscan, updater
src/                     React + TypeScript arayüz (Vite)
```

Motorlar UI'dan bağımsızdır; `byteforge-core` bir kütüphane olarak başka projelerde de kullanılabilir.

---

## Sorumlu kullanım

ByteForge eğitim, güvenlik araştırması ve **sahibi olduğun ya da izinli olduğun** yazılımların analizi içindir. Canlı bellek düzenleme yalnızca kendi süreçlerinde çalıştırılmalıdır. Başkalarına ait uygulamaları izinsiz değiştirmek, çevrimiçi oyunlarda hile yapmak veya lisans şartlarını ihlal etmek kullanıcının sorumluluğundadır.
