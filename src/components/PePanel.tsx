import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Zap, ChevronsRight } from "lucide-react";
import { OpcodePalette } from "./OpcodePalette";
import { PackerList } from "./PackerList";
import { PePatchWorkbench } from "./PePatchWorkbench";
import { MonoSymbols } from "./MonoSymbols";
import { ReportButton } from "./ReportButton";
import { AntiTamper } from "./AntiTamper";
import type { PeProfile, ApiError } from "../types";

const errMsg = (e: unknown) => (e as Partial<ApiError>)?.message ?? String(e);
const baseName = (p: string) => p.split(/[/\\]/).pop() ?? p;
const hex = (n: number) => "0x" + n.toString(16).toUpperCase();

/** Windows PE (EXE/DLL) profil paneli. */
export function PePanel({ pePath }: { pePath: string }) {
  const [profile, setProfile] = useState<PeProfile | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const analyze = async () => {
    setError(null);
    setBusy(true);
    try {
      setProfile(await invoke<PeProfile>("profile_pe", { path: pePath }));
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="pe-panel">
      <div className="profile-head">
        <h2 title={pePath}>{baseName(pePath)}</h2>
        <div className="profile-head-right">
          {profile && <ReportButton command="report_pe" path={pePath} />}
          {profile && (
            <span
              className={`badge runtime runtime-${profile.is_dotnet ? "unity_mono" : "java_kotlin"}`}
            >
              {profile.is_dotnet ? ".NET / Managed" : "Native PE"}
            </span>
          )}
        </div>
      </div>
      <p className="muted" style={{ margin: "0 0 14px" }}>
        Windows yürütülebilir dosyası (EXE/DLL) — mimari, .NET/native ayrımı, derleyici
        tahmini ve import edilen DLL'ler <b>saf Rust</b> (goblin) ile çözümlenir.
      </p>

      <div className="deploy-actions">
        <button className="primary-btn" onClick={analyze} disabled={busy}>
          <Zap size={15} /> {busy ? "Çözümleniyor…" : "PE'yi Analiz Et"}
        </button>
      </div>
      {error && <p className="error">Hata: {error}</p>}

      {profile && (
        <div className="profile-card" style={{ marginTop: 14 }}>
          <div className="profile-grid">
            <div className="field">
              <span className="field-label">Tür</span>
              <span className="field-value">
                <span className="chip">{profile.is_dll ? "DLL" : "EXE"}</span>
                <span className="chip">{profile.is_64bit ? "64-bit" : "32-bit"}</span>
              </span>
            </div>
            <div className="field">
              <span className="field-label">Mimari</span>
              <span className="field-value">
                <span className="chip">{profile.machine}</span>
              </span>
            </div>
            <div className="field">
              <span className="field-label">Subsystem</span>
              <span className="field-value">{profile.subsystem}</span>
            </div>
            <div className="field">
              <span className="field-label">Derleyici</span>
              <span className="field-value">{profile.compiler}</span>
            </div>
            <div className="field">
              <span className="field-label">.NET</span>
              <span className="field-value">
                {profile.is_dotnet ? (
                  <span className="chip chip-ok">evet (managed)</span>
                ) : (
                  <span className="chip chip-off">hayır (native)</span>
                )}
              </span>
            </div>
            <div className="field">
              <span className="field-label">Entry Point (RVA)</span>
              <span className="field-value mono">{hex(profile.entry_point)}</span>
            </div>
          </div>

          <div className="manifest">
            <div className="manifest-head">
              <span className="field-label">Bölümler (sections)</span>
              <span className="muted">{profile.sections.length}</span>
            </div>
            <div className="field-value" style={{ flexWrap: "wrap" }}>
              {profile.sections.length ? (
                profile.sections.map((s, i) => (
                  <span key={i} className="chip mono">
                    {s}
                  </span>
                ))
              ) : (
                <span className="muted">—</span>
              )}
            </div>
          </div>

          <div className="manifest">
            <div className="manifest-head">
              <span className="field-label">Import edilen DLL'ler</span>
              <span className="muted">{profile.imported_dlls.length}</span>
            </div>
            <div className="field-value" style={{ flexWrap: "wrap" }}>
              {profile.imported_dlls.length ? (
                profile.imported_dlls.map((d, i) => (
                  <span key={i} className="chip mono">
                    {d}
                  </span>
                ))
              ) : (
                <span className="muted">statik / import yok</span>
              )}
            </div>
          </div>

          <div className="manifest">
            <span className="field-label">Koruma / paketleyici</span>
            <PackerList packers={profile.packers} />
          </div>

          <AntiTamper command="antitamper_pe" path={pePath} />

          <div className="manifest">
            <div className="manifest-head">
              <span className="field-label">Section entropisi (paketlenmişlik)</span>
              {profile.likely_packed ? (
                <span className="chip chip-off" style={{ color: "var(--danger)", opacity: 1 }}>
                  yüksek entropi — paketlenmiş/şifreli olası
                </span>
              ) : (
                <span className="chip chip-ok">olağan</span>
              )}
            </div>
            <div className="entropy-rows">
              {profile.section_entropy.map((s, i) => (
                <div key={i} className="entropy-row">
                  <span className="mono entropy-name">{s.name}</span>
                  <div className="entropy-bar">
                    <div
                      className={`entropy-fill ent-${s.class}`}
                      style={{ width: `${(s.bits / 8) * 100}%` }}
                    />
                  </div>
                  <span className="mono entropy-val">{s.bits.toFixed(2)}</span>
                  <span className={`entropy-tag ent-${s.class}`}>{s.class}</span>
                </div>
              ))}
            </div>
          </div>

          <div className="error-hint" style={{ marginTop: 12 }}>
            <ChevronsRight size={14} /> <b>Önerilen hat:</b>{" "}
            {profile.recommended_pipeline}
          </div>

          {profile.is_dotnet ? (
            <MonoSymbols pePath={pePath} />
          ) : (
            <PePatchWorkbench pePath={pePath} />
          )}
        </div>
      )}

      <OpcodePalette arch="x86" />
    </div>
  );
}
