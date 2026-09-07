import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Plus, Trash2, Syringe, CheckCircle2, Info } from "lucide-react";
import type { MenuConfig, MenuFeature, FeatureTarget, InjectResult, ResolvedMethod, ApiError } from "../types";

const errMsg = (e: unknown) => (e as Partial<ApiError>)?.message ?? String(e);
const hx = (n: number) => "0x" + n.toString(16).toUpperCase();

type Kind = FeatureTarget["kind"];

const TEMPLATES = ["return_true", "return_false", "return_max_int", "nop", "ret"] as const;

function defaultTarget(kind: Kind): FeatureTarget {
  if (kind === "smali_boolean")
    return { kind, class: "", method: "", ret: true };
  return { kind, offset: 0, template: "return_true" };
}

function parseHex(s: string): number {
  const c = s.trim().toLowerCase().replace(/^0x/, "");
  return /^[0-9a-f]+$/.test(c) ? parseInt(c, 16) : 0;
}

export function ModMenuStudio({
  apkPath,
  runtime,
}: {
  apkPath: string;
  runtime?: string;
}) {
  const isIl2cpp = runtime === "unity_il2cpp";
  // Özellik başına isim-arama (IL2CPP): metod adı → çözülmüş RVA.
  const [lookup, setLookup] = useState<Record<number, ResolvedMethod[]>>({});
  const [picked, setPicked] = useState<Record<number, string>>({});
  const doLookup = async (i: number, q: string) => {
    if (q.trim().length < 2) {
      setLookup((l) => ({ ...l, [i]: [] }));
      return;
    }
    try {
      const rows = await invoke<ResolvedMethod[]>("il2cpp_lookup", {
        apk: apkPath,
        query: q.trim(),
        limit: 30,
      });
      setLookup((l) => ({ ...l, [i]: rows }));
    } catch {
      setLookup((l) => ({ ...l, [i]: [] }));
    }
  };
  const pickMethod = (i: number, m: ResolvedMethod, tmpl: string) => {
    updateTarget(i, { kind: "il2cpp_rva", offset: m.rva, template: tmpl });
    setPicked((pk) => ({ ...pk, [i]: m.name }));
    setLookup((l) => ({ ...l, [i]: [] }));
    setFeatures((f) =>
      f.map((x, idx) => (idx === i && !x.name ? { ...x, name: m.method_name } : x))
    );
  };
  const [title, setTitle] = useState("Android Koç v1.0");
  const [features, setFeatures] = useState<MenuFeature[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<InjectResult | null>(null);

  const addFeature = () =>
    setFeatures((f) => [
      ...f,
      { name: "", target: defaultTarget(isIl2cpp ? "il2cpp_rva" : "smali_boolean") },
    ]);

  const removeFeature = (i: number) =>
    setFeatures((f) => f.filter((_, idx) => idx !== i));

  const updateFeature = (i: number, patch: Partial<MenuFeature>) =>
    setFeatures((f) => f.map((x, idx) => (idx === i ? { ...x, ...patch } : x)));

  const updateTarget = (i: number, target: FeatureTarget) =>
    setFeatures((f) => f.map((x, idx) => (idx === i ? { ...x, target } : x)));

  const inject = async () => {
    setError(null);
    setResult(null);
    setBusy(true);
    try {
      const config: MenuConfig = { title, features };
      setResult(await invoke<InjectResult>("inject_mod_menu", { path: apkPath, config }));
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setBusy(false);
    }
  };

  const kindLabel = (k: Kind) =>
    k === "smali_boolean" ? "Smali boolean" : k === "il2cpp_rva" ? "IL2CPP RVA" : "Native ofset";

  return (
    <div className="modmenu-studio">
      <div className="profile-head">
        <h2>Mod Menu Studio</h2>
        <span className="badge">Yüzen Hile Menüsü Enjektörü</span>
      </div>
      <p className="muted" style={{ marginTop: 0 }}>
        Bir APK'ya sıfırdan yüzen mod menüsü enjekte eder: manifest izni +
        MenuLoader smali + launcher onCreate kancası + statik özellik yamaları →
        yeniden derler (imzasız; <b>Dağıt</b> sekmesinde imzalayın).
      </p>

      <div className="mm-field">
        <span className="field-label">Menü Başlığı</span>
        <input
          className="mm-title"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder="ör. Android Koç v1.0"
        />
      </div>

      <div className="mm-features">
        <div className="manifest-head">
          <span className="field-label">Özellikler ({features.length})</span>
          <button className="mm-add" onClick={addFeature}>
            <Plus size={14} /> Özellik Ekle
          </button>
        </div>

        {features.length === 0 && (
          <p className="muted">Henüz özellik yok. "Özellik Ekle" ile başlayın.</p>
        )}

        {features.map((f, i) => (
          <div key={i} className="mm-feature">
            <div className="mm-feature-top">
              <input
                className="mm-fname"
                value={f.name}
                onChange={(e) => updateFeature(i, { name: e.target.value })}
                placeholder="Özellik adı — ör. VIP / Kilitleri Aç"
              />
              <select
                value={f.target.kind}
                onChange={(e) => updateTarget(i, defaultTarget(e.target.value as Kind))}
              >
                <option value="smali_boolean">{kindLabel("smali_boolean")}</option>
                <option value="il2cpp_rva">{kindLabel("il2cpp_rva")}</option>
                <option value="native_offset">{kindLabel("native_offset")}</option>
              </select>
              <button className="mm-del" onClick={() => removeFeature(i)} title="kaldır">
                <Trash2 size={15} />
              </button>
            </div>

            {f.target.kind === "smali_boolean" ? (
              <div className="mm-target-row">
                <input
                  className="mono"
                  value={f.target.class}
                  onChange={(e) =>
                    updateTarget(i, { ...(f.target as any), class: e.target.value })
                  }
                  placeholder="Sınıf — Lcom/app/Billing;"
                />
                <input
                  className="mono"
                  value={f.target.method}
                  onChange={(e) =>
                    updateTarget(i, { ...(f.target as any), method: e.target.value })
                  }
                  placeholder="Metod — isPremium()Z"
                />
                <select
                  value={f.target.ret ? "true" : "false"}
                  onChange={(e) =>
                    updateTarget(i, { ...(f.target as any), ret: e.target.value === "true" })
                  }
                >
                  <option value="true">Return True</option>
                  <option value="false">Return False</option>
                </select>
              </div>
            ) : f.target.kind === "il2cpp_rva" ? (
              <div className="mm-target-row mm-il2cpp">
                <div className="mm-ac">
                  <input
                    className="mono"
                    placeholder="metod ara — ör. IsSkinOwned, GetCurrency"
                    value={picked[i] ?? ""}
                    onChange={(e) => {
                      setPicked((pk) => ({ ...pk, [i]: e.target.value }));
                      doLookup(i, e.target.value);
                    }}
                  />
                  {(lookup[i]?.length ?? 0) > 0 && (
                    <div className="mm-ac-list">
                      {lookup[i].map((m, k) => (
                        <div
                          key={k}
                          className="mm-ac-row"
                          onClick={() =>
                            pickMethod(i, m, (f.target as { template: string }).template)
                          }
                        >
                          <span className="us-rva-tag mono">{hx(m.rva)}</span>
                          <span className="mono mm-ac-name">{m.name}</span>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
                <span className="mm-rva mono">
                  {f.target.offset ? hx(f.target.offset) : "RVA yok"}
                </span>
                <select
                  value={f.target.template}
                  onChange={(e) =>
                    updateTarget(i, { ...(f.target as any), template: e.target.value })
                  }
                >
                  {TEMPLATES.map((t) => (
                    <option key={t} value={t}>
                      {t}
                    </option>
                  ))}
                </select>
              </div>
            ) : (
              <div className="mm-target-row">
                <input
                  className="mono"
                  defaultValue={f.target.offset ? "0x" + f.target.offset.toString(16) : ""}
                  onChange={(e) =>
                    updateTarget(i, { ...(f.target as any), offset: parseHex(e.target.value) })
                  }
                  placeholder="Ofset (hex) — ör. 0x1FB7100"
                />
                <select
                  value={f.target.template}
                  onChange={(e) =>
                    updateTarget(i, { ...(f.target as any), template: e.target.value })
                  }
                >
                  {TEMPLATES.map((t) => (
                    <option key={t} value={t}>
                      {t}
                    </option>
                  ))}
                </select>
              </div>
            )}
          </div>
        ))}
      </div>

      <div className="deploy-actions" style={{ marginTop: 16 }}>
        <button className="primary-btn" onClick={inject} disabled={busy || !title.trim()}>
          <Syringe size={15} /> {busy ? "Enjekte ediliyor…" : "Menüyü APK'ya Enjekte Et"}
        </button>
      </div>

      {error && <p className="error">Hata: {error}</p>}

      {result && (
        <div className="mm-result">
          <p className="patch-ok">
            <CheckCircle2 size={15} className="ic-ok" /> Enjekte edildi — imzasız APK:
          </p>
          <code className="mono mm-outpath">{result.out_apk}</code>
          <div className="mm-steps">
            {result.steps.map((s, i) => (
              <div key={i} className="mm-step">
                <CheckCircle2 size={13} className="ic-ok" /> {s}
              </div>
            ))}
          </div>
          {result.deferred_native.length > 0 && (
            <div className="error-hint" style={{ marginTop: 10 }}>
              <Info size={14} /> <b>Native sekmesinde uygulanacak</b> (bu özellikler .so
              yaması ister, statik smali'de uygulanmadı):
              <ul className="finding-entries" style={{ marginTop: 6 }}>
                {result.deferred_native.map((d, i) => (
                  <li key={i}>{d}</li>
                ))}
              </ul>
            </div>
          )}
          <p className="muted" style={{ marginTop: 10, fontSize: 13 }}>
            Sonraki adım: <b>Dağıt</b> sekmesinde bu APK'yı imzalayıp cihaza kurun.
          </p>
        </div>
      )}
    </div>
  );
}
