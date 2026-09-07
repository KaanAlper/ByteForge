import type { AppProfile, RuntimeKind } from "../types";
import { PackerList } from "./PackerList";
import { ReportButton } from "./ReportButton";

const RUNTIME_LABEL: Record<RuntimeKind, string> = {
  unity_il2cpp: "Unity (IL2CPP)",
  unity_mono: "Unity (Mono)",
  flutter: "Flutter / Dart AOT",
  react_native: "React Native",
  java_kotlin: "Java / Kotlin",
  unknown: "Bilinmeyen",
};

export function ProfileCard({
  profile,
  apkPath,
}: {
  profile: AppProfile;
  apkPath?: string;
}) {
  const sig = profile.signatures;
  const anySig = sig.v1 || sig.v2 || sig.v3;
  return (
    <div className="profile-card">
      <div className="profile-head">
        <h2 title={profile.file_name}>{profile.file_name}</h2>
        <div className="profile-head-right">
          {apkPath && <ReportButton command="report_apk" path={apkPath} />}
          <span className={`badge runtime runtime-${profile.runtime}`}>
            {RUNTIME_LABEL[profile.runtime]}
          </span>
        </div>
      </div>

      <div className="profile-grid">
        <div className="field">
          <span className="field-label">Mimari</span>
          <span className="field-value">
            {profile.abis.length ? (
              profile.abis.map((a) => (
                <span key={a} className="chip">
                  {a}
                </span>
              ))
            ) : (
              <span className="muted">—</span>
            )}
          </span>
        </div>

        <div className="field">
          <span className="field-label">İmza</span>
          <span className="field-value">
            {anySig ? (
              (["v1", "v2", "v3"] as const).map((v) =>
                sig[v] ? (
                  <span key={v} className="chip chip-ok">
                    {v}
                  </span>
                ) : (
                  <span key={v} className="chip chip-off">
                    {v}
                  </span>
                )
              )
            ) : (
              <span className="muted">imza yok</span>
            )}
          </span>
        </div>

        <div className="field">
          <span className="field-label">Giriş sayısı</span>
          <span className="field-value mono">{profile.entry_count}</span>
        </div>
      </div>

      {profile.manifest && (
        <div className="manifest">
          <div className="manifest-head">
            <span className="field-label">Manifest</span>
            {profile.manifest.debuggable && (
              <span className="badge badge-warn">debuggable</span>
            )}
          </div>
          <div className="profile-grid">
            <div className="field">
              <span className="field-label">Paket</span>
              <span className="field-value mono">{profile.manifest.package ?? "—"}</span>
            </div>
            <div className="field">
              <span className="field-label">Sürüm</span>
              <span className="field-value mono">
                {profile.manifest.version_name ?? "—"}
                {profile.manifest.version_code ? ` (${profile.manifest.version_code})` : ""}
              </span>
            </div>
            <div className="field">
              <span className="field-label">SDK (min / target)</span>
              <span className="field-value mono">
                {(profile.manifest.min_sdk ?? "—") + " / " + (profile.manifest.target_sdk ?? "—")}
              </span>
            </div>
            <div className="field">
              <span className="field-label">Başlatılabilir</span>
              <span className="field-value">
                {profile.manifest.launchable_activities.length ? (
                  profile.manifest.launchable_activities.map((a) => (
                    <span key={a} className="chip">
                      {a}
                    </span>
                  ))
                ) : (
                  <span className="muted">—</span>
                )}
              </span>
            </div>
          </div>
          {profile.manifest.permissions.length > 0 && (
            <details className="perms">
              <summary>{profile.manifest.permissions.length} izin</summary>
              <ul>
                {profile.manifest.permissions.map((p) => (
                  <li key={p} className="mono">
                    {p}
                  </li>
                ))}
              </ul>
            </details>
          )}
        </div>
      )}

      <div className="manifest">
        <span className="field-label">Koruma / paketleyici</span>
        <PackerList packers={profile.packers} />
      </div>

      <div className="pipeline">
        <span className="field-label">Önerilen pipeline</span>
        <p>{profile.recommended_pipeline}</p>
      </div>
    </div>
  );
}
