import { useState, useEffect, useMemo, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Search,
  Zap,
  RotateCcw,
  ShieldCheck,
  ShieldAlert,
  MousePointerClick,
  AlertTriangle,
} from "lucide-react";
import { CAT_LABEL, CAT_ORDER, TEMPLATE_LABEL, countByCategory } from "../categoryInfo";
import type {
  TargetRow,
  PrologueCheck,
  PatchPreview,
  PatchTemplate,
  HexChunk,
  DisasmLine,
  ResolvedMethod,
  ApiError,
} from "../types";

const errMsg = (e: unknown) => (e as Partial<ApiError>)?.message ?? String(e);
const hx = (n: number) => "0x" + n.toString(16).toUpperCase();
const parseHex = (s: string): number | null => {
  const c = s.trim().toLowerCase().replace(/^0x/, "");
  return /^[0-9a-f]+$/.test(c) ? parseInt(c, 16) : null;
};

// Çözülmüş bir metodun adından kaba kategori (varsayılan şablon + rozet için).
const inferCat = (name: string): string => {
  const n = name.toLowerCase();
  if (/coin|currenc|wallet|money|gem|cash|price|cost|gold|afford|balance/.test(n)) return "economy";
  if (/skin|owned|unlock|purchas|vip|premium|iap|bought/.test(n)) return "purchase_vip";
  if (/character|hero|avatar/.test(n)) return "character";
  if (/ad|advert|reward/.test(n)) return "ads";
  return "other";
};

// Yama şablonları — arm64 + x86, çekirdek patch.rs ile aynı baytlar.
const TEMPLATES: { kind: string; tmpl: PatchTemplate; label: string }[] = [
  { kind: "return_true", tmpl: { kind: "return_true" }, label: "True (1 döndür)" },
  { kind: "return_max_int", tmpl: { kind: "return_max_int" }, label: "Max Int (dev sayı)" },
  { kind: "return_one_float", tmpl: { kind: "return_one_float" }, label: "1.0 Float (oran hep dolu → sınırsız)" },
  { kind: "return_false", tmpl: { kind: "return_false" }, label: "False (0 döndür)" },
  { kind: "nop", tmpl: { kind: "nop" }, label: "NOP (etkisizleştir)" },
  { kind: "ret", tmpl: { kind: "ret" }, label: "Ret (hemen dön)" },
];
const tmplByKind = (k: string): PatchTemplate =>
  TEMPLATES.find((t) => t.kind === k)?.tmpl ?? { kind: "return_true" };

// Sezgisel skor için varsayılan şablon.
const defaultKindForCat = (cat: string) => (cat === "economy" ? "return_max_int" : "return_true");

interface PatchState {
  offset: number;
  kind: string;
}

// Birleşik seçim: ya sezgisel skor (RVA yok) ya çözülmüş metod (RVA var).
type Sel =
  | { kind: "target"; name: string; category: string; risky: boolean }
  | { kind: "resolved"; name: string; category: string; risky: boolean; rva: number; image: string };

/**
 * Birleşik Native & IL2CPP Atölyesi — sol: hedef gezgini (sezgisel skorlar +
 * çözülmüş sembol arama), sağ: canlı makine kodu + yama tezgahı. Çözücü sayesinde
 * bir isim aranınca RVA otomatik dolar; tek pencerede ara → seç → prologue denetle
 * → şablon seç → tek tıkla yamala.
 */
export function UnifiedStudio({
  soPath,
  outDir,
  resolvable,
  targets,
  metadataVersion,
}: {
  soPath: string | null;
  outDir: string | null;
  resolvable: boolean;
  targets: TargetRow[];
  metadataVersion: number;
}) {
  // Master (sol) durum
  const [masterTab, setMasterTab] = useState<"targets" | "resolved">("targets");
  const [scoreMin, setScoreMin] = useState(70);
  const [cat, setCat] = useState("all");
  const [q, setQ] = useState("");
  const [selected, setSelected] = useState<Sel | null>(null);
  const [patched, setPatched] = useState<Record<string, PatchState>>({});

  // Çözülmüş sembol arama durumu
  const [rq, setRq] = useState("");
  const [rResults, setRResults] = useState<ResolvedMethod[] | null>(null);
  const [rBusy, setRBusy] = useState(false);
  const [rErr, setRErr] = useState<string | null>(null);

  // Detail (sağ) durum
  const [rvaInput, setRvaInput] = useState("");
  const [offset, setOffset] = useState<number | null>(null);
  const [template, setTemplate] = useState<string>("return_true");
  const [prologue, setPrologue] = useState<PrologueCheck | null>(null);
  const [preview, setPreview] = useState<PatchPreview | null>(null);
  const [hex, setHex] = useState<HexChunk | null>(null);
  const [disasm, setDisasm] = useState<DisasmLine[] | null>(null);
  const [baseRva, setBaseRva] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);

  const filtered = useMemo(() => {
    const ql = q.toLowerCase();
    return targets
      .filter((t) => t.score >= scoreMin)
      .filter((t) => cat === "all" || t.category === cat)
      .filter((t) => !q || t.name.toLowerCase().includes(ql));
  }, [targets, scoreMin, cat, q]);

  const counts = useMemo(
    () => countByCategory(targets.filter((t) => t.score >= scoreMin)),
    [targets, scoreMin]
  );

  // Çözülmüş sembol araması (resolved-methods.tsv üzerinde).
  const runResolvedSearch = useCallback(async (override?: string) => {
    if (!outDir) return;
    const query = (override ?? rq).trim();
    setRBusy(true);
    setRErr(null);
    try {
      const rows = await invoke<ResolvedMethod[]>("il2cpp_resolve_search", {
        outDir,
        query,
        limit: 300,
      });
      setRResults(rows);
    } catch (e) {
      setRErr(errMsg(e));
      setRResults(null);
    } finally {
      setRBusy(false);
    }
  }, [outDir, rq]);

  // Detay verisini (hex + prologue + önizleme) belirli bir ofset + şablon için yükler.
  const loadDetail = useCallback(
    async (off: number, tmplKind: string, base: number) => {
      if (!soPath) return;
      setError(null);
      try {
        const start = off >= 16 ? off - 16 : 0;
        const [h, pro, prev, dis] = await Promise.all([
          invoke<HexChunk>("read_hex", { path: soPath, offset: start, len: 64 }),
          invoke<PrologueCheck>("check_prologue", { path: soPath, offset: off, arch: "arm64" }),
          invoke<PatchPreview>("patch_preview", {
            path: soPath,
            offset: off,
            template: tmplByKind(tmplKind),
          }),
          invoke<DisasmLine[]>("disassemble_range", {
            path: soPath,
            offset: off,
            len: 64,
            base,
            arch: "arm64",
          }),
        ]);
        setHex(h);
        setPrologue(pro);
        setPreview(prev);
        setDisasm(dis);
      } catch (e) {
        setError(errMsg(e));
      }
    },
    [soPath]
  );

  // Sembol seçilince detay panelini hazırla.
  useEffect(() => {
    setError(null);
    setStatus(null);
    setPreview(null);
    setPrologue(null);
    setHex(null);
    if (!selected) {
      setOffset(null);
      setRvaInput("");
      return;
    }
    const defKind = defaultKindForCat(selected.category);
    setTemplate(defKind);
    const prev = patched[selected.name];

    if (selected.kind === "resolved") {
      // RVA biliniyor → ofseti otomatik çöz.
      setRvaInput("");
      if (soPath) {
        invoke<number | null>("rva_to_offset", { path: soPath, rva: selected.rva })
          .then((off) => {
            if (off == null) {
              setError("Bu RVA bir kod segmentine düşmüyor.");
              setOffset(null);
              return;
            }
            setOffset(off);
            setBaseRva(selected.rva);
            return loadDetail(off, prev?.kind ?? defKind, selected.rva);
          })
          .catch((e) => setError(errMsg(e)));
      }
    } else if (prev) {
      setOffset(prev.offset);
      setRvaInput("");
      loadDetail(prev.offset, prev.kind, baseRva);
    } else {
      setOffset(null);
      setRvaInput("");
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selected]);

  const resolveRva = async () => {
    if (!soPath || !selected) return;
    const rva = parseHex(rvaInput);
    if (rva === null) {
      setError("Geçersiz RVA (hex bekleniyor, ör. 0x2311D70)");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const off = await invoke<number | null>("rva_to_offset", { path: soPath, rva });
      if (off === null) {
        setError("Bu RVA bir kod segmentine düşmüyor (dosyada karşılığı yok).");
        setOffset(null);
        return;
      }
      setOffset(off);
      setBaseRva(rva);
      await loadDetail(off, template, rva);
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setBusy(false);
    }
  };

  // Şablon değişince önizlemeyi yenile.
  const changeTemplate = async (kind: string) => {
    setTemplate(kind);
    if (offset != null) await loadDetail(offset, kind, baseRva);
  };

  const applyPatch = async () => {
    if (!soPath || !selected || offset == null) return;
    if (
      selected.risky &&
      !window.confirm(
        `${selected.name}\n\nBu bir satın-alma/işlem akışı olabilir — yama oyunu ` +
          `çökertebilir. Yine de uygulansın mı?`
      )
    )
      return;
    setBusy(true);
    setError(null);
    try {
      await invoke<string>("apply_so_patch", {
        path: soPath,
        offset,
        template: tmplByKind(template),
      });
      setPatched((p) => ({ ...p, [selected.name]: { offset, kind: template } }));
      setStatus(`Yamalandı: ${selected.name} → ${TEMPLATE_LABEL[template]} (@${hx(offset)})`);
      await loadDetail(offset, template, baseRva);
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setBusy(false);
    }
  };

  const revertPatch = async () => {
    if (!soPath || !selected || offset == null) return;
    setBusy(true);
    setError(null);
    try {
      const msg = await invoke<string>("revert_patch_at", { path: soPath, offset });
      setPatched((p) => {
        const n = { ...p };
        delete n[selected.name];
        return n;
      });
      setStatus(msg);
      await loadDetail(offset, template, baseRva);
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setBusy(false);
    }
  };

  const isPatched = (name: string) => name in patched;
  const cats = CAT_ORDER.filter((c) => counts[c]);

  const hexView = () => {
    if (!hex || offset == null) return null;
    const hl = offset - hex.offset;
    const hlLen = preview ? preview.replacement.trim().split(/\s+/).length : 4;
    return (
      <div className="us-hex">
        {hex.bytes.map((b, i) => (
          <span key={i} className={`us-hex-byte ${i >= hl && i < hl + hlLen ? "hl" : ""}`}>
            {b.toString(16).padStart(2, "0").toUpperCase()}
          </span>
        ))}
      </div>
    );
  };

  return (
    <div className="unified-studio">
      {/* SOL — Master */}
      <div className="us-master">
        <div className="us-master-tabs">
          <button
            className={`us-mtab ${masterTab === "targets" ? "active" : ""}`}
            onClick={() => setMasterTab("targets")}
          >
            Sezgisel Hedefler
          </button>
          <button
            className={`us-mtab ${masterTab === "resolved" ? "active" : ""}`}
            onClick={() => setMasterTab("resolved")}
            disabled={!resolvable}
            title={resolvable ? "" : "Bu ikilide otomatik çözüm desteklenmiyor"}
          >
            Tüm Semboller (ara)
          </button>
        </div>

        {masterTab === "targets" ? (
          <>
            <div className="score-pills">
              <button
                className={`score-pill sp-high ${scoreMin === 70 ? "active" : ""}`}
                onClick={() => setScoreMin(70)}
              >
                Kesin (≥70) <b>{targets.filter((t) => t.score >= 70).length}</b>
              </button>
              <button
                className={`score-pill sp-mid ${scoreMin === 50 ? "active" : ""}`}
                onClick={() => setScoreMin(50)}
              >
                Olası (≥50) <b>{targets.filter((t) => t.score >= 50).length}</b>
              </button>
              <button
                className={`score-pill ${scoreMin === 0 ? "active" : ""}`}
                onClick={() => setScoreMin(0)}
              >
                Tümü <b>{targets.length}</b>
              </button>
            </div>
            <div className="cat-tabs">
              <button
                className={`cat-tab ${cat === "all" ? "active" : ""}`}
                onClick={() => setCat("all")}
              >
                Hepsi <b>{targets.filter((t) => t.score >= scoreMin).length}</b>
              </button>
              {cats.map((c) => (
                <button
                  key={c}
                  className={`cat-tab cat-${c} ${cat === c ? "active" : ""}`}
                  onClick={() => setCat(c)}
                >
                  {CAT_LABEL[c]} <b>{counts[c]}</b>
                </button>
              ))}
            </div>
            <div className="search-field" style={{ marginTop: 8 }}>
              <Search className="search-field-icon" size={15} />
              <input
                className="il2cpp-search"
                placeholder="hedef ara — ör. HasCharacter, Coin"
                value={q}
                onChange={(e) => setQ(e.target.value)}
              />
            </div>
            <div className="us-list">
              {filtered.map((t, i) => (
                <div
                  key={i}
                  className={`us-row ${selected && selected.name === t.name ? "sel" : ""}`}
                  onClick={() =>
                    setSelected(
                      t.rva != null
                        ? {
                            kind: "resolved",
                            name: t.name,
                            category: t.category,
                            risky: t.risky,
                            rva: t.rva,
                            image: t.image ?? "",
                          }
                        : { kind: "target", name: t.name, category: t.category, risky: t.risky }
                    )
                  }
                >
                  <span
                    className={`target-score ${
                      t.score >= 70 ? "ts-high" : t.score >= 50 ? "ts-mid" : "ts-low"
                    }`}
                  >
                    {t.score}
                  </span>
                  <span className="us-row-name mono">{t.name}</span>
                  {t.rva != null && <span className="us-rva-tag mono">{hx(t.rva)}</span>}
                  {isPatched(t.name) && <span className="badge-modded">YAMALI</span>}
                  <span className={`cat-chip cat-${t.category}`}>{CAT_LABEL[t.category]}</span>
                </div>
              ))}
              {filtered.length === 0 && (
                <p className="muted" style={{ padding: 12 }}>
                  Eşleşme yok.
                </p>
              )}
            </div>
          </>
        ) : (
          <>
            <div className="search-field" style={{ marginTop: 8 }}>
              <Search className="search-field-icon" size={15} />
              <input
                className="il2cpp-search"
                placeholder="metod/tip ara — ör. IsSkinOwned, GetCurrency, ActiveDurationRatio"
                value={rq}
                onChange={(e) => setRq(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && runResolvedSearch()}
              />
              <button className="primary-btn" onClick={() => runResolvedSearch()} disabled={rBusy || !outDir}>
                {rBusy ? "…" : "Ara"}
              </button>
            </div>
            <p className="muted" style={{ margin: "6px 2px", fontSize: 12.5 }}>
              Adres çözücü aktif — bir metod adı yaz, RVA otomatik gelsin. Tıkla → sağda yamala.
            </p>
            {rErr && <p className="error">Hata: {rErr}</p>}
            <div className="us-list">
              {rResults?.map((m, i) => (
                <div
                  key={i}
                  className={`us-row ${
                    selected?.kind === "resolved" && selected.rva === m.rva && selected.name === m.name
                      ? "sel"
                      : ""
                  }`}
                  onClick={() =>
                    setSelected({
                      kind: "resolved",
                      name: m.name,
                      category: inferCat(m.name),
                      risky: /deferred|pending|receipt|transaction|restore/i.test(m.name),
                      rva: m.rva,
                      image: m.image,
                    })
                  }
                >
                  <span className="mono us-rva-tag">{hx(m.rva)}</span>
                  <span className="us-row-name mono">{m.name}</span>
                  {isPatched(m.name) && <span className="badge-modded">YAMALI</span>}
                  <span className={`cat-chip cat-${inferCat(m.name)}`}>{CAT_LABEL[inferCat(m.name)]}</span>
                </div>
              ))}
              {rResults && rResults.length === 0 && (
                <p className="muted" style={{ padding: 12 }}>
                  Eşleşme yok — farklı bir isim deneyin.
                </p>
              )}
              {rResults && rResults.length >= 300 && (
                <p className="muted" style={{ padding: 8 }}>
                  İlk 300 sonuç — aramayı daraltın.
                </p>
              )}
              {!rResults && !rBusy && (
                <div className="us-search-empty">
                  <Search className="se-ic" size={30} />
                  <p>124 bin metot içinde adıyla ara — adresi otomatik gelsin.</p>
                  <div className="us-examples">
                    {["IsSkinOwned", "GetCurrency", "ActiveDurationRatio", "HasBeenUnlocked", "get_IsUnlocked"].map(
                      (ex) => (
                        <button
                          key={ex}
                          onClick={() => {
                            setRq(ex);
                            runResolvedSearch(ex);
                          }}
                        >
                          {ex}
                        </button>
                      )
                    )}
                  </div>
                </div>
              )}
            </div>
          </>
        )}
      </div>

      {/* SAĞ — Detail */}
      <div className="us-detail">
        {!selected ? (
          <div className="us-empty">
            <MousePointerClick size={34} />
            <p>Soldaki listeden incelemek ve yamalamak istediğiniz bir fonksiyon seçin.</p>
          </div>
        ) : (
          <>
            <div className="us-detail-head">
              <h3 className="mono">{selected.name}</h3>
              {selected.kind === "resolved" ? (
                <span className="us-rva-tag mono">{hx(selected.rva)}</span>
              ) : (
                <span className={`cat-chip cat-${selected.category}`}>
                  {CAT_LABEL[selected.category]}
                </span>
              )}
              {isPatched(selected.name) && <span className="badge-modded">YAMALI</span>}
              {selected.risky && (
                <span className="risk-badge">
                  <AlertTriangle size={12} /> riskli
                </span>
              )}
            </div>
            {selected.kind === "resolved" && (
              <p className="muted mono" style={{ marginTop: -4, fontSize: 12.5 }}>
                {selected.image}
              </p>
            )}

            {!soPath ? (
              <div className="error-hint">
                libil2cpp.so çıkarılmadı — bu APK'da native binary yok ya da henüz hazırlanmadı.
              </div>
            ) : offset == null ? (
              selected.kind === "resolved" ? (
                <div className="us-rva-box">
                  <p className="muted" style={{ marginTop: 0 }}>
                    Adres çözülüyor…
                  </p>
                  {error && <p className="error">Hata: {error}</p>}
                </div>
              ) : (
                <div className="us-rva-box">
                  <p className="muted" style={{ marginTop: 0 }}>
                    Bu sezgisel hedefin adresi otomatik gelmedi.{" "}
                    {resolvable ? (
                      <>
                        <b>Tüm Semboller</b> sekmesinden adıyla arayıp otomatik RVA alabilirsiniz,
                      </>
                    ) : (
                      <>IL2CPP metadata v{metadataVersion} obfuske olabilir;</>
                    )}{" "}
                    ya da RVA'yı elle girin:
                  </p>
                  <div className="inline-field">
                    <input
                      className="mono"
                      placeholder="RVA — ör. 0x2311D70"
                      value={rvaInput}
                      onChange={(e) => setRvaInput(e.target.value)}
                      onKeyDown={(e) => e.key === "Enter" && resolveRva()}
                    />
                    <button className="primary-btn" onClick={resolveRva} disabled={busy}>
                      Çözümle
                    </button>
                  </div>
                  {error && <p className="error">Hata: {error}</p>}
                </div>
              )
            ) : (
              <>
                <div className="us-addr">
                  <span>
                    <span className="field-label">Dosya Ofseti</span>
                    <span className="mono">{hx(offset)}</span>
                  </span>
                  <button className="us-addr-reset" onClick={() => setOffset(null)}>
                    başka RVA
                  </button>
                </div>

                {prologue && prologue.status === "prologue" && (
                  <div className="prologue-ok">
                    <ShieldCheck size={14} /> Fonksiyon Başı — Güvenli: {prologue.detail}
                  </div>
                )}
                {prologue && prologue.status === "not_prologue" && (
                  <div className="prologue-warn">
                    <ShieldAlert size={15} /> {prologue.warning}
                  </div>
                )}
                {prologue && prologue.status === "already_patched" && (
                  <div className="prologue-ok">
                    <ShieldCheck size={14} /> Zaten yamalı ({TEMPLATE_LABEL[prologue.template]})
                  </div>
                )}

                <div className="us-template">
                  <span className="field-label">Yama şablonu</span>
                  <select value={template} onChange={(e) => changeTemplate(e.target.value)}>
                    {TEMPLATES.map((t) => (
                      <option key={t.kind} value={t.kind}>
                        {t.label}
                      </option>
                    ))}
                  </select>
                </div>

                {preview && (
                  <div className="patch-diff">
                    <div className="pd-col pd-before">
                      <span className="field-label">ÖNCESİ</span>
                      <span className="mono">{preview.current || "—"}</span>
                    </div>
                    <div className="pd-arrow">→</div>
                    <div className="pd-col pd-after">
                      <span className="field-label">SONRASI</span>
                      <span className="mono accent-text">{preview.replacement}</span>
                    </div>
                  </div>
                )}

                <div className="patch-actions">
                  <button
                    className={`patch-apply ${template === "return_max_int" ? "force-maxint" : ""}`}
                    onClick={applyPatch}
                    disabled={busy}
                  >
                    <Zap size={15} /> {TEMPLATE_LABEL[template]} Uygula (.bak yedekli)
                  </button>
                  {isPatched(selected.name) && (
                    <button className="patch-revert" onClick={revertPatch} disabled={busy}>
                      <RotateCcw size={15} /> Orijinale Geri Al
                    </button>
                  )}
                </div>
                {status && <p className="patch-ok">{status}</p>}
                {error && <p className="error">Hata: {error}</p>}

                <div className="us-hex-card">
                  <span className="field-label">
                    Hex Gözatıcı — {hx(hex?.offset ?? 0)} çevresi (64 bayt)
                  </span>
                  {hexView()}
                </div>

                {disasm && disasm.length > 0 && (
                  <div className="us-disasm-card">
                    <span className="field-label">Disassembly (ARM64) — fonksiyon başı</span>
                    <div className="us-disasm">
                      {disasm.map((l, k) => (
                        <div key={k} className={`us-disasm-row ${k === 0 ? "hl" : ""}`}>
                          <span className="ud-addr mono">
                            0x{l.address.toString(16).toUpperCase()}
                          </span>
                          <span className="ud-bytes mono">{l.bytes}</span>
                          <span className="ud-text mono">{l.text}</span>
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </>
            )}
          </>
        )}
      </div>
    </div>
  );
}
