import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ShieldAlert, ShieldCheck } from "lucide-react";
import type { TamperHit, ApiError } from "../types";

const errMsg = (e: unknown) => (e as Partial<ApiError>)?.message ?? String(e);

const CAT_LABEL: Record<string, string> = {
  anti_debug: "Anti-Debug",
  signature: "İmza Kontrolü",
  integrity: "Bütünlük",
  anti_emulator: "Anti-Emülatör",
};

/** Bir ikilinin anti-tamper / anti-debug göstergelerini tarar ve gösterir. */
export function AntiTamper({ command, path }: { command: string; path: string }) {
  const [hits, setHits] = useState<TamperHit[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const scan = async () => {
    setBusy(true);
    setError(null);
    try {
      setHits(await invoke<TamperHit[]>(command, { path }));
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="manifest">
      <div className="manifest-head">
        <span className="field-label">Anti-Tamper / Anti-Debug</span>
        <button className="report-btn" onClick={scan} disabled={busy}>
          <ShieldAlert size={14} /> {busy ? "Taranıyor…" : "Koruma Tara"}
        </button>
      </div>
      {error && <p className="error">Hata: {error}</p>}
      {hits &&
        (hits.length === 0 ? (
          <div className="packer-box clean" style={{ marginTop: 8 }}>
            <ShieldCheck size={16} /> Bilinen anti-debug/anti-tamper göstergesi bulunamadı.
          </div>
        ) : (
          <div className="tamper-rows">
            {hits.map((h, i) => (
              <div key={i} className={`tamper-row cat-${h.category}`}>
                <span className="tamper-cat">{CAT_LABEL[h.category] ?? h.category}</span>
                <div className="tamper-body">
                  <div className="tamper-name mono">{h.name}</div>
                  <div className="tamper-note muted">{h.note}</div>
                </div>
              </div>
            ))}
          </div>
        ))}
    </div>
  );
}
