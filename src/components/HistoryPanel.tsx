import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Upload, Download, Copy, Check } from "lucide-react";
import type { PatchRecord, ApiError } from "../types";

const baseName = (p: string) => p.split(/[/\\]/).pop() ?? p;
const errMsg = (e: unknown) => (e as Partial<ApiError>)?.message ?? String(e);

interface RecipeStep {
  kind: string;
  target: string;
  offset: number;
  old_hex: string;
  new_hex: string;
  note: string;
}
interface Recipe {
  version: number;
  name: string;
  note: string;
  app_hint: string;
  steps: RecipeStep[];
}

export function HistoryPanel() {
  const [records, setRecords] = useState<PatchRecord[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [recipeJson, setRecipeJson] = useState<string>("");
  const [imported, setImported] = useState<Recipe | null>(null);
  const [copied, setCopied] = useState(false);
  const [showRecipes, setShowRecipes] = useState(false);

  const load = useCallback(async () => {
    setError(null);
    try {
      setRecords(await invoke<PatchRecord[]>("patch_history"));
    } catch (e) {
      setError(errMsg(e));
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const revert = async (id: number) => {
    setError(null);
    setStatus(null);
    try {
      const msg = await invoke<string>("revert_patch", { id });
      setStatus(msg);
      load();
    } catch (e) {
      setError(errMsg(e));
    }
  };

  const clearAll = async () => {
    if (!window.confirm("Tüm yama geçmişi silinsin mi? (dosyalar değişmez)")) return;
    try {
      await invoke("clear_patch_history");
      load();
    } catch (e) {
      setError(errMsg(e));
    }
  };

  const exportRecipe = async () => {
    setError(null);
    const name = window.prompt("Tarif adı:", "Yamalarım");
    if (name === null) return;
    try {
      const json = await invoke<string>("export_recipe", {
        name: name || "Yamalarım",
        note: "",
        appHint: records?.[0] ? baseName(records[0].path) : "",
      });
      setRecipeJson(json);
      setShowRecipes(true);
      setImported(null);
      setStatus("Tarif oluşturuldu — kopyalayıp paylaşabilirsiniz");
    } catch (e) {
      setError(errMsg(e));
    }
  };

  const importRecipe = async () => {
    setError(null);
    try {
      setImported(await invoke<Recipe>("import_recipe", { json: recipeJson }));
      setStatus(null);
    } catch (e) {
      setError(errMsg(e));
      setImported(null);
    }
  };

  const copyJson = async () => {
    try {
      await navigator.clipboard.writeText(recipeJson);
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    } catch {
      /* geç */
    }
  };

  const empty = records && records.length === 0;

  return (
    <div className="history-panel">
      <div className="wb-header">
        <span className="muted">{records?.length ?? 0} yama kaydı</span>
        <div className="wb-actions">
          <button onClick={load}>yenile</button>
          <button onClick={() => setShowRecipes((s) => !s)}>
            {showRecipes ? "tarifleri gizle" : "tarifler (recipe)"}
          </button>
          {!empty && (
            <button onClick={exportRecipe}>
              <Download size={14} /> dışa aktar
            </button>
          )}
          {!empty && <button onClick={clearAll}>geçmişi temizle</button>}
        </div>
      </div>

      {error && <p className="error">Hata: {error}</p>}
      {status && <p className="patch-ok">{status}</p>}

      {showRecipes && (
        <div className="recipe-box">
          <div className="manifest-head">
            <span className="field-label">Yama tarifi (JSON) — paylaş / içe aktar</span>
            {recipeJson && (
              <button className="recipe-copy" onClick={copyJson}>
                {copied ? <Check size={14} /> : <Copy size={14} />}
                {copied ? "kopyalandı" : "kopyala"}
              </button>
            )}
          </div>
          <textarea
            className="recipe-json mono"
            placeholder="Tarif JSON'unu buraya yapıştırın (içe aktarmak için) ya da 'dışa aktar' ile üretin"
            value={recipeJson}
            onChange={(e) => setRecipeJson(e.target.value)}
          />
          <div className="deploy-actions" style={{ marginTop: 8 }}>
            <button className="primary-btn" onClick={importRecipe} disabled={!recipeJson.trim()}>
              <Upload size={15} /> içe aktar & çöz
            </button>
          </div>
          {imported && (
            <div className="recipe-preview">
              <div className="recipe-meta">
                <b>{imported.name}</b>
                {imported.app_hint && <span className="muted"> · {imported.app_hint}</span>}
                <span className="muted"> · {imported.steps.length} adım</span>
              </div>
              {imported.steps.map((s, i) => (
                <div key={i} className="recipe-step">
                  <span className={`hist-kind hist-${s.kind}`}>{s.kind}</span>
                  <span className="mono">{s.target}</span>
                  <span className="mono muted">@0x{s.offset.toString(16)}</span>
                  <span className="mono recipe-bytes">
                    {s.old_hex} → {s.new_hex}
                  </span>
                  {s.note && <span className="muted">{s.note}</span>}
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {empty ? (
        <div className="panel-empty" style={{ minHeight: 200 }}>
          Henüz yama uygulanmadı. Native/Hex yamaları burada listelenir.
        </div>
      ) : (
        <div className="hist-list">
          {records?.map((r) => (
            <div key={r.id} className={`hist-row ${r.reverted ? "reverted" : ""}`}>
              <div className="hist-main">
                <span className={`hist-kind hist-${r.kind}`}>{r.kind}</span>
                <span className="mono hist-file" title={r.path}>
                  {baseName(r.path)}
                </span>
                <span className="mono muted">@0x{r.offset.toString(16)}</span>
              </div>
              <div className="hist-bytes mono">
                <span className="hist-old">{r.old_hex || "—"}</span>
                <span className="hist-arrow">→</span>
                <span className="hist-new">{r.new_hex}</span>
              </div>
              <div className="hist-foot">
                <span className="muted">{r.note}</span>
                {r.reverted ? (
                  <span className="hist-reverted-tag">geri alındı</span>
                ) : (
                  <button className="hist-revert" onClick={() => revert(r.id)}>
                    geri al
                  </button>
                )}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
