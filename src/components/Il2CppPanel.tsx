import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Zap,
  Search,
  Save,
  Info,
  Brain,
  LoaderCircle,
  ChevronRight,
  ChevronDown,
  Package,
} from "lucide-react";
import { UnifiedStudio } from "./UnifiedStudio";
import type { Il2CppResult, DumpResult, Score, TargetRow, RankedTarget, ResolveSummary, ApiError } from "../types";

const errMsg = (e: unknown) => (e as Partial<ApiError>)?.message ?? String(e);

/** Derleyici-üretimi (lambda/closure/backing field) sembol mü? */
function isGenerated(s: string): boolean {
  return (
    s.startsWith("<") ||
    s.includes(">b__") ||
    s.includes(">d__") ||
    s.includes(">c__") ||
    s.includes("__BackingField") ||
    s.includes("<PrivateImplementation")
  );
}

type SymType = { label: string; cls: string };
function symType(s: string): SymType {
  if (s.startsWith("get_")) return { label: "get", cls: "st-get" };
  if (s.startsWith("set_")) return { label: "set", cls: "st-set" };
  if (isGenerated(s)) return { label: "sys", cls: "st-sys" };
  if (s.startsWith("Is") || s.startsWith("Has") || s.startsWith("Can"))
    return { label: "bool?", cls: "st-bool" };
  return { label: "id", cls: "st-id" };
}

const QUICK = ["get_", "set_", "Is", "unlock", "premium", "purchase", "reward", "coin", "money"];

export function Il2CppPanel({
  apkPath,
  onOpenNative,
}: {
  apkPath: string;
  onOpenNative?: (soPath: string, query: string) => void;
}) {
  const [result, setResult] = useState<Il2CppResult | null>(null);
  const [dump, setDump] = useState<DumpResult | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [q, setQ] = useState("");
  const [hideGen, setHideGen] = useState(true);
  const [targets, setTargets] = useState<TargetRow[] | null>(null);
  const [showAllSymbols, setShowAllSymbols] = useState(false);
  const [resolvable, setResolvable] = useState(false);
  const [packMsg, setPackMsg] = useState<string | null>(null);

  const run = async (label: string, fn: () => Promise<void>) => {
    setError(null);
    setBusy(label);
    try {
      await fn();
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setBusy(null);
    }
  };

  const extract = () =>
    run("extract", async () => {
      setResult(await invoke<Il2CppResult>("extract_il2cpp", { path: apkPath }));
    });

  const dumpSymbols = () =>
    run("dump", async () => {
      // .so yolunu bilmek için (Native'de açmak üzere) çıkarmayı da garantiye al.
      const r = result ?? (await invoke<Il2CppResult>("extract_il2cpp", { path: apkPath }));
      if (!result) setResult(r);
      setDump(await invoke<DumpResult>("dump_il2cpp_symbols", { path: apkPath }));
    });

  // Yamalı cache .so'yu APK'ya geri paketle + imzala → telefona kurulabilir APK.
  // "Uygula" (cache .so'yu yamalar) ile "Dağıt" (APK imzalar) arasındaki köprü.
  const packageMod = () =>
    run("package", async () => {
      if (!result) return;
      const abi = result.binary_arch.find((a) => a.includes("arm64")) ?? result.binary_arch[0];
      const entry = `lib/${abi}/libil2cpp.so`;
      const so = `${result.out_dir}/libil2cpp-${abi}.so`;
      const outApk = apkPath.replace(/\.apk$/i, "") + "-modlu.apk";
      setPackMsg(null);
      const packaged = await invoke<string>("package_patched_so", {
        apk: apkPath,
        entry,
        soPath: so,
        outApk,
      });
      const signed = await invoke<string>("resign_apk", { path: packaged });
      // Bağlı cihaza doğrudan kur; cihaz yoksa imzalı APK yolunu göster.
      try {
        const r = await invoke<string>("install_apk", { serial: null, path: signed });
        setPackMsg(`Kuruldu ve hazır: ${signed}\n${r}`);
      } catch (e) {
        setPackMsg(
          `İmzalı APK hazır: ${signed}\nCihaza otomatik kurulamadı (${errMsg(e)}). ` +
            `Telefonu bağlayıp tekrar dene ya da Dağıt sekmesinden kur.`
        );
      }
    });

  const scanTargets = () =>
    run("targets", async () => {
      if (resolvable && result) {
        setTargets(
          await invoke<RankedTarget[]>("il2cpp_rank_resolved", {
            outDir: result.out_dir,
            top: 300,
          })
        );
      } else if (dump) {
        setTargets(await invoke<Score[]>("rank_symbols", { names: dump.symbols, top: 150 }));
      }
    });

  // OTOMATİK HAZIRLIK: APK yüklenince arka planda çıkar → sembol dök → akıllı
  // tarama. Kullanıcı IL2CPP'ye geçtiğinde her şey hazır olsun.
  useEffect(() => {
    let alive = true;
    setBusy("auto");
    (async () => {
      try {
        const r = await invoke<Il2CppResult>("extract_il2cpp", { path: apkPath });
        if (alive) setResult(r);
        const d = await invoke<DumpResult>("dump_il2cpp_symbols", { path: apkPath });
        if (alive) setDump(d);
        // Adres çözücü: metod adı → RVA (saf Rust). Başarılıysa hedefler RVA'lı gelir.
        let resolved = false;
        try {
          await invoke<ResolveSummary>("il2cpp_resolve", { outDir: r.out_dir });
          resolved = true;
        } catch {
          resolved = false;
        }
        if (alive) setResolvable(resolved);
        if (resolved) {
          const rt = await invoke<RankedTarget[]>("il2cpp_rank_resolved", {
            outDir: r.out_dir,
            top: 300,
          });
          if (alive) setTargets(rt);
        } else {
          const t = await invoke<Score[]>("rank_symbols", { names: d.symbols, top: 150 });
          if (alive) setTargets(t);
        }
      } catch (e) {
        if (alive) setError(errMsg(e));
      } finally {
        if (alive) setBusy(null);
      }
    })();
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [apkPath]);

  // Çıkarılmış libil2cpp.so yolu (ilk ABI). onOpenNative için hedef.
  const soPath =
    result?.has_binary && result.binary_arch.length
      ? `${result.out_dir}/libil2cpp-${result.binary_arch[0]}.so`
      : null;

  const openNative = (query: string) => {
    if (soPath && onOpenNative) {
      if (query) navigator.clipboard.writeText(query).catch(() => {});
      onOpenNative(soPath, query);
    }
  };

  const all = dump?.symbols ?? [];
  const generatedCount = all.filter(isGenerated).length;
  const ql = q.toLowerCase();
  const filtered = all.filter(
    (s) => (!hideGen || !isGenerated(s)) && (!q || s.toLowerCase().includes(ql))
  );

  return (
    <div className="il2cpp-panel">
      <h3>IL2CPP Analizi</h3>
      <p className="muted" style={{ margin: "0 0 14px" }}>
        global-metadata.dat ve libil2cpp.so çıkarılır; semboller <b>saf Rust</b> ile
        çözümlenir (harici Il2CppDumper / .NET gerekmez).
      </p>

      {busy === "auto" && (
        <div className="auto-prep">
          <LoaderCircle size={15} className="spin" /> Otomatik hazırlanıyor — metadata
          çıkarılıyor, semboller çözümleniyor ve akıllı hedefler taranıyor…
        </div>
      )}

      <div className="deploy-actions">
        <button className="primary-btn" onClick={extract} disabled={busy !== null}>
          {busy === "extract" ? "…" : "Dosyaları çıkar & analiz et"}
        </button>
        <button className="primary-btn" onClick={dumpSymbols} disabled={busy !== null}>
          <Zap size={15} /> {busy === "dump" ? "Çözümleniyor…" : "Sembolleri Çözümle"}
        </button>
        {soPath && onOpenNative && (
          <button onClick={() => openNative("")} disabled={busy !== null}>
            libil2cpp.so'yu Native'de aç
          </button>
        )}
      </div>
      {error && <p className="error">Hata: {error}</p>}

      {result && (
        <div className="il2cpp-result">
          <div className="profile-grid">
            <div className="field">
              <span className="field-label">Metadata</span>
              <span className="field-value">
                {result.metadata_valid
                  ? `geçerli (v${result.metadata_version})`
                  : "bulunamadı / geçersiz"}
              </span>
            </div>
            <div className="field">
              <span className="field-label">Boyut</span>
              <span className="field-value mono">
                {(result.metadata_size / 1024 / 1024).toFixed(1)} MB
              </span>
            </div>
            <div className="field">
              <span className="field-label">Binary (ABI)</span>
              <span className="field-value">
                {result.has_binary ? result.binary_arch.join(", ") : "yok"}
              </span>
            </div>
          </div>
        </div>
      )}

      {dump && (
        <div className="il2cpp-result">
          {/* Sayaç şeridi */}
          <div className="sym-stats">
            <span className="sym-stat">
              <b>{dump.count.toLocaleString()}</b> toplam
            </span>
            <span className="sym-stat">
              <b>{(dump.count - generatedCount).toLocaleString()}</b> gerçek
            </span>
            <span className="sym-stat muted">
              {generatedCount.toLocaleString()} derleyici
            </span>
            <span className="sym-stat">metadata v{dump.metadata_version}</span>
          </div>

          <div className="deploy-actions" style={{ margin: "10px 0" }}>
            <button className="smart-btn primary-btn" onClick={scanTargets} disabled={busy !== null}>
              <Brain size={15} /> {busy === "targets" ? "Analiz…" : "Akıllı Hedefleri Tara"}
            </button>
            {soPath && (
              <button className="primary-btn" onClick={packageMod} disabled={busy !== null} title="Uyguladığın yamaları içeren, imzalı, kurulabilir APK üretir">
                <Package size={15} /> {busy === "package" ? "Paketlenip kuruluyor…" : "Yamaları Paketle, İmzala ve Cihaza Kur"}
              </button>
            )}
          </div>
          {packMsg && <p className="patch-ok" style={{ wordBreak: "break-all", whiteSpace: "pre-wrap" }}>{packMsg}</p>}

          {targets && targets.length > 0 && (
            <UnifiedStudio
              soPath={soPath}
              outDir={result?.out_dir ?? null}
              resolvable={resolvable}
              targets={targets}
              metadataVersion={dump.metadata_version}
            />
          )}
          {targets && targets.length === 0 && (
            <p className="packer-box clean" style={{ marginBottom: 12 }}>
              Yüksek puanlı lisans/kilit adayı bulunamadı.
            </p>
          )}

          {/* Ham sembol dökümü — çözücü YOKSA (v39/obfuske) yedek arayüz; varsa gizli */}
          {!resolvable && (
            <>
          <button
            className="symlist-toggle"
            onClick={() => setShowAllSymbols((s) => !s)}
          >
            {showAllSymbols ? <ChevronDown size={15} /> : <ChevronRight size={15} />}
            Tüm ham sembol dökümü ({filtered.length.toLocaleString()})
            {!showAllSymbols && <span className="muted"> — genelde gerekmez, aç/kapa</span>}
          </button>

          {showAllSymbols && (
            <>
          {/* Arama + hızlı filtreler */}
          <div className="search-field">
            <Search className="search-field-icon" size={15} />
            <input
              className="il2cpp-search"
              placeholder="sembol ara — ör. get_isPremium, Balance, Unlock"
              value={q}
              onChange={(e) => setQ(e.target.value)}
            />
          </div>
          <div className="quick-filters">
            {QUICK.map((f) => (
              <button key={f} className={q === f ? "active" : ""} onClick={() => setQ(f)}>
                {f}
              </button>
            ))}
            {q && (
              <button className="clear" onClick={() => setQ("")}>
                temizle
              </button>
            )}
            <label className="sym-toggle">
              <input
                type="checkbox"
                checked={hideGen}
                onChange={(e) => setHideGen(e.target.checked)}
              />
              derleyici sembollerini gizle
            </label>
          </div>

          <div className="sym-result-count muted">
            {filtered.length.toLocaleString()} sonuç
            {filtered.length > 1000 ? " (ilk 1000 gösteriliyor)" : ""}
            {soPath && onOpenNative && " · bir sembole tıkla → Native'de aç"}
          </div>

          <div className="il2cpp-symlist">
            {filtered.slice(0, 1000).map((s, i) => {
              const t = symType(s);
              const clickable = !!(soPath && onOpenNative);
              return (
                <div
                  key={i}
                  className={`il2cpp-sym ${clickable ? "clickable" : ""}`}
                  onClick={clickable ? () => openNative(s) : undefined}
                  title={clickable ? "Native sekmesinde bu adı ara" : undefined}
                >
                  <span className={`sym-badge ${t.cls}`}>{t.label}</span>
                  <span className="mono sym-name">{s}</span>
                </div>
              );
            })}
            {filtered.length === 0 && <p className="muted">Eşleşme yok.</p>}
          </div>

          <p
            className="muted mono"
            style={{
              marginTop: 8,
              wordBreak: "break-all",
              fontSize: 11,
              display: "flex",
              alignItems: "center",
              gap: 6,
            }}
          >
            <Save size={13} style={{ flexShrink: 0 }} /> {dump.out_file}
          </p>
          <div className="error-hint">
            <Info size={14} /> Bu metadata (v{dump.metadata_version}) otomatik çözülemedi
            (obfuske/paketli olabilir), bu yüzden adresler burada yok. RVA'yı bir dumper/IDA'dan
            alıp <b>Native</b> sekmesindeki <b>RVA ↔ Ofset dönüştürücüsüne</b> yapıştırarak
            yamalayın — ELF segmentlerinden kesin hesaplanır, sürümden bağımsız.
          </div>
            </>
          )}
            </>
          )}
        </div>
      )}
    </div>
  );
}
