import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Zap, Info } from "lucide-react";
import type { ApiError } from "../types";

const errMsg = (e: unknown) => (e as Partial<ApiError>)?.message ?? String(e);

interface MonoDump {
  count: number;
  symbols: string[];
}

const QUICK = ["get_", "set_", "is", "Premium", "Vip", "Unlock", "Coin", "Money", "Purchase"];

/** .NET / Unity Mono (Assembly-CSharp.dll) sembol adı dökümü. */
export function MonoSymbols({ pePath }: { pePath: string }) {
  const [dump, setDump] = useState<MonoDump | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [q, setQ] = useState("");

  const run = async () => {
    setBusy(true);
    setError(null);
    try {
      setDump(await invoke<MonoDump>("dotnet_symbols", { path: pePath }));
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setBusy(false);
    }
  };

  const all = dump?.symbols ?? [];
  const ql = q.toLowerCase();
  const filtered = q ? all.filter((s) => s.toLowerCase().includes(ql)) : all;

  return (
    <div className="manifest">
      <div className="manifest-head">
        <span className="field-label">Mono sembolleri (.NET metadata)</span>
        <button className="primary-btn" onClick={run} disabled={busy}>
          <Zap size={15} /> {busy ? "Çözümleniyor…" : "Sembolleri Çözümle (Mono)"}
        </button>
      </div>
      {error && <p className="error">Hata: {error}</p>}

      {dump && (
        <>
          <div className="sym-stats" style={{ marginTop: 10 }}>
            <span className="sym-stat">
              <b>{dump.count.toLocaleString()}</b> sembol adı
            </span>
            {dump.count > dump.symbols.length && (
              <span className="sym-stat muted">(ilk {dump.symbols.length} gösteriliyor)</span>
            )}
          </div>
          <div className="search-field" style={{ marginTop: 8 }}>
            <input
              className="il2cpp-search"
              placeholder="sembol ara — ör. get_isPremium, PlayerData"
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
          </div>
          <div className="il2cpp-symlist" style={{ marginTop: 8 }}>
            {filtered.slice(0, 1000).map((s, i) => (
              <div key={i} className="il2cpp-sym">
                <span className="mono sym-name">{s}</span>
              </div>
            ))}
            {filtered.length === 0 && <p className="muted">Eşleşme yok.</p>}
          </div>
          <div className="error-hint">
            <Info size={14} /> Bunlar .NET metadata'sındaki tip/metod/alan adları
            (#Strings heap). Mantığı düzenlemek için harici dnSpy/ILSpy ile IL
            seviyesinde açın; ByteForge adları/yerleri native çıkarır.
          </div>
        </>
      )}
    </div>
  );
}
