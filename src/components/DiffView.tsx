import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ClipboardList, PackagePlus, FlaskConical, ArrowLeftRight, Syringe } from "lucide-react";
import type { ArchiveDiff, Finding, ModPatch, ApiError } from "../types";

const errMsg = (e: unknown) => (e as Partial<ApiError>)?.message ?? String(e);
const baseName = (p: string) => p.split(/[/\\]/).pop() ?? p;

function Section({
  title,
  items,
  kind,
}: {
  title: string;
  items: string[];
  kind: "added" | "removed" | "modified";
}) {
  if (items.length === 0) return null;
  return (
    <div className={`diff-section diff-${kind}`}>
      <div className="diff-section-head">
        <span className="diff-sign">
          {kind === "added" ? "+" : kind === "removed" ? "−" : "~"}
        </span>
        {title} <span className="diff-count">{items.length}</span>
      </div>
      <ul>
        {items.map((n) => (
          <li key={n} className="mono">
            {n}
          </li>
        ))}
      </ul>
    </div>
  );
}

export function DiffView({
  diff,
  paths,
}: {
  diff: ArchiveDiff;
  paths: { a: string; b: string } | null;
}) {
  const [findings, setFindings] = useState<Finding[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Native mod reçetesi
  const soEntries = diff.modified.filter((n) => n.endsWith(".so"));
  const defaultEntry =
    soEntries.find((n) => n.includes("libil2cpp.so")) ?? soEntries[0] ?? "";
  const [modEntry, setModEntry] = useState(defaultEntry);
  const [swapped, setSwapped] = useState(false); // false: A=temiz, B=modlu
  const [recipe, setRecipe] = useState<ModPatch[] | null>(null);
  const [recipeBusy, setRecipeBusy] = useState<string | null>(null);
  const [recipeErr, setRecipeErr] = useState<string | null>(null);
  const [applyMsg, setApplyMsg] = useState<string | null>(null);

  const cleanPath = paths ? (swapped ? paths.b : paths.a) : null;
  const moddedPath = paths ? (swapped ? paths.a : paths.b) : null;

  const noChanges =
    diff.added.length === 0 &&
    diff.removed.length === 0 &&
    diff.modified.length === 0;

  const report = async () => {
    setError(null);
    setBusy(true);
    try {
      setFindings(await invoke<Finding[]>("forensic_report", { diff }));
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setBusy(false);
    }
  };

  const extractRecipe = async () => {
    if (!cleanPath || !moddedPath || !modEntry) return;
    setRecipeBusy("extract");
    setRecipeErr(null);
    setApplyMsg(null);
    try {
      setRecipe(
        await invoke<ModPatch[]>("native_mod_recipe", {
          clean: cleanPath,
          modded: moddedPath,
          entry: modEntry,
        })
      );
    } catch (e) {
      setRecipeErr(errMsg(e));
      setRecipe(null);
    } finally {
      setRecipeBusy(null);
    }
  };

  const applyRecipe = async () => {
    if (!cleanPath || !modEntry || !recipe || recipe.length === 0) return;
    if (
      !window.confirm(
        `${recipe.length} baytsal yama TEMİZ APK'ya uygulanacak ve modlu bir APK ` +
          `üretilecek (imzasız). Devam?`
      )
    )
      return;
    setRecipeBusy("apply");
    setRecipeErr(null);
    try {
      const outApk = cleanPath.replace(/\.apk$/i, "") + "-modded.apk";
      const msg = await invoke<string>("apply_native_mod", {
        clean: cleanPath,
        entry: modEntry,
        patches: recipe.map((p) => ({ offset: p.offset, new_hex: p.new_hex })),
        outApk,
      });
      setApplyMsg(msg);
    } catch (e) {
      setRecipeErr(errMsg(e));
    } finally {
      setRecipeBusy(null);
    }
  };

  return (
    <div className="profile-card">
      <div className="profile-head">
        <h2>Paket Karşılaştırması</h2>
        <span className="badge">{diff.unchanged} değişmemiş</span>
      </div>

      <div className="diff-hashes">
        <div className="field">
          <span className="field-label">A · SHA-256</span>
          <span className="field-value mono hash">{diff.sha256_a}</span>
        </div>
        <div className="field">
          <span className="field-label">B · SHA-256</span>
          <span className="field-value mono hash">{diff.sha256_b}</span>
        </div>
      </div>

      {noChanges ? (
        <p className="muted" style={{ marginTop: 16 }}>
          İki paket dosya düzeyinde özdeş (yalnızca {diff.unchanged} değişmemiş giriş).
        </p>
      ) : (
        <>
          <div className="deploy-actions" style={{ margin: "14px 0" }}>
            <button className="primary-btn" onClick={report} disabled={busy}>
              <ClipboardList size={15} />{" "}
              {busy ? "Rapor çıkarılıyor…" : "Adli rapor çıkar"}
            </button>
          </div>
          {error && <p className="error">Hata: {error}</p>}

          {findings && (
            <div className="forensic">
              {findings.length === 0 ? (
                <p className="muted">Sınıflandırılabilir bir değişiklik bulunamadı.</p>
              ) : (
                findings.map((f, i) => (
                  <div
                    key={i}
                    className={`finding sev-${f.severity} ${
                      f.category === "mod_menu" ? "finding-modmenu" : ""
                    }`}
                  >
                    <div className="finding-head">
                      {f.category === "mod_menu" && (
                        <PackagePlus size={16} className="finding-modmenu-ic" />
                      )}
                      <span className={`finding-sev sev-${f.severity}`}>{f.severity}</span>
                      <span className="finding-title">{f.title}</span>
                    </div>
                    <p className="finding-detail">{f.detail}</p>
                    <ul className="finding-entries">
                      {f.entries.map((e) => (
                        <li key={e} className="mono">
                          {e}
                        </li>
                      ))}
                    </ul>
                  </div>
                ))
              )}
            </div>
          )}

          {/* ---- Native Mod Reçetesi (öğren + uygula) ---- */}
          {paths && soEntries.length > 0 && (
            <div className="mod-recipe">
              <div className="manifest-head">
                <span className="field-label">
                  <FlaskConical size={15} style={{ verticalAlign: "-0.15em" }} /> Native Mod
                  Reçetesi — modcunun yerinde bayt yamalarını çıkar
                </span>
              </div>
              <div className="recipe-controls">
                <select value={modEntry} onChange={(e) => setModEntry(e.target.value)}>
                  {soEntries.map((n) => (
                    <option key={n} value={n}>
                      {n}
                    </option>
                  ))}
                </select>
                <button className="us-addr-reset" onClick={() => setSwapped((s) => !s)} title="temiz/modlu yönünü değiştir">
                  <ArrowLeftRight size={13} /> {swapped ? "B=temiz · A=modlu" : "A=temiz · B=modlu"}
                </button>
                <button className="primary-btn" onClick={extractRecipe} disabled={recipeBusy !== null}>
                  {recipeBusy === "extract" ? "Çıkarılıyor…" : "Reçeteyi Çıkar"}
                </button>
              </div>
              <div className="recipe-dir muted">
                Temiz: <b>{baseName(cleanPath ?? "")}</b> → Modlu:{" "}
                <b>{baseName(moddedPath ?? "")}</b>
              </div>
              {recipeErr && <p className="error">Hata: {recipeErr}</p>}

              {recipe && (
                <>
                  <div className="sym-stats" style={{ marginTop: 10 }}>
                    <span className="sym-stat">
                      <b>{recipe.length}</b> bayt yaması
                    </span>
                    <span className="sym-stat muted">
                      {recipe.filter((p) => p.template === "return_true").length} ReturnTrue ·{" "}
                      {recipe.reduce((n, p) => n + p.len, 0)} bayt
                    </span>
                  </div>
                  <div className="recipe-table">
                    <div className="recipe-row recipe-head">
                      <span>Fonksiyon / RVA</span>
                      <span>Ofset</span>
                      <span>Eski → Yeni</span>
                      <span>Yorum</span>
                    </div>
                    {recipe.map((p, i) => (
                      <div key={i} className="recipe-row">
                        <span className="mono recipe-sym">
                          {p.symbol ? (
                            <b className="accent-text">{p.symbol}</b>
                          ) : (
                            <span className="muted">
                              {p.rva != null ? "0x" + p.rva.toString(16).toUpperCase() : "—"}
                            </span>
                          )}
                          {p.symbol && p.rva != null && (
                            <span className="muted recipe-sym-rva">
                              0x{p.rva.toString(16).toUpperCase()}
                            </span>
                          )}
                        </span>
                        <span className="mono">0x{p.offset.toString(16).toUpperCase()}</span>
                        <span className="mono recipe-bytes">
                          {p.old_hex} <span className="accent-text">→ {p.new_hex}</span>
                        </span>
                        <span className={p.template === "return_true" ? "accent-text" : ""}>
                          {p.label}
                        </span>
                      </div>
                    ))}
                  </div>
                  <div className="deploy-actions" style={{ marginTop: 12 }}>
                    <button className="primary-btn" onClick={applyRecipe} disabled={recipeBusy !== null}>
                      <Syringe size={15} />{" "}
                      {recipeBusy === "apply" ? "Uygulanıyor…" : "Bu Modu TEMİZ APK'ya Uygula"}
                    </button>
                  </div>
                  <div className="error-hint" style={{ marginTop: 8 }}>
                    <b>Sınav ipucu:</b> Bu ofsetler aynı base .so'da birebir geçerlidir —
                    modlu APK olmadan, temiz APK'ya bu reçeteyi uygulayınca aynı mod çıkar.
                    Üretilen APK imzasızdır; <b>Dağıt</b> sekmesinde imzalayın.
                  </div>
                  {applyMsg && <p className="patch-ok" style={{ marginTop: 8 }}>{applyMsg}</p>}
                </>
              )}
            </div>
          )}

          <Section title="Eklenen" items={diff.added} kind="added" />
          <Section title="Silinen" items={diff.removed} kind="removed" />
          <Section title="Değişen" items={diff.modified} kind="modified" />
        </>
      )}
    </div>
  );
}
