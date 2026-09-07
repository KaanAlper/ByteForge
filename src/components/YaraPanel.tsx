import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ShieldAlert, ScanLine } from "lucide-react";
import type { YaraMatch, ApiError } from "../types";

const errMsg = (e: unknown) => (e as Partial<ApiError>)?.message ?? String(e);
const baseName = (p: string) => p.split(/[/\\]/).pop() ?? p;

/**
 * Yara imza taraması — native Rust (yara-x). Yüklü ikilide packer, kripto,
 * anti-debug, root/emülatör/Frida imzalarını arar.
 */
export function YaraPanel({ path }: { path: string | null }) {
  const [matches, setMatches] = useState<YaraMatch[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const scan = async () => {
    if (!path) return;
    setBusy(true);
    setError(null);
    try {
      setMatches(await invoke<YaraMatch[]>("yara_scan", { path }));
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setBusy(false);
    }
  };

  if (!path) {
    return (
      <div className="panel-empty">
        <ShieldAlert size={30} />
        <p>Bir ikili (APK / .so / EXE) yükleyin — Yara imza taraması yapılır.</p>
      </div>
    );
  }

  return (
    <div className="profile-card">
      <div className="profile-head">
        <h2>Yara İmza Taraması</h2>
        <span className="badge">native yara-x</span>
      </div>
      <p className="muted" style={{ marginTop: 0 }}>
        <b className="mono">{baseName(path)}</b> — packer, kripto, anti-debug, root/emülatör/Frida
        imzalarını gömülü kural setiyle arar.
      </p>
      <div className="deploy-actions">
        <button className="smart-btn primary-btn" onClick={scan} disabled={busy}>
          <ScanLine size={15} /> {busy ? "Taranıyor…" : "Tara"}
        </button>
      </div>
      {error && <p className="error">Hata: {error}</p>}

      {matches &&
        (matches.length === 0 ? (
          <div className="packer-box clean" style={{ marginTop: 12 }}>
            Bilinen bir imza eşleşmedi.
          </div>
        ) : (
          <div className="yara-results">
            {matches.map((m, i) => (
              <div key={i} className="yara-hit">
                <div className="yara-hit-head">
                  <span className="yara-rule mono">{m.rule}</span>
                  {m.tags.map((t) => (
                    <span key={t} className={`cat-chip cat-${t}`}>
                      {t}
                    </span>
                  ))}
                  <span className="yara-count muted">{m.hit_count} eşleşme</span>
                </div>
                <p className="finding-detail">{m.description}</p>
              </div>
            ))}
          </div>
        ))}
    </div>
  );
}
