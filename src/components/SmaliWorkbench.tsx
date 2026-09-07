import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ScanSearch, Wand2, Brain, AlertTriangle } from "lucide-react";
import { SmaliEditor } from "./SmaliEditor";
import { EngineHint } from "./EngineHint";
import { runStream, ProgressBar, IDLE_STREAM, type StreamState } from "./StreamProgress";
import {
  CAT_LABEL as TARGET_CAT_LABEL,
  CAT_ORDER,
  TEMPLATE_LABEL,
  countByCategory,
} from "../categoryInfo";
import type {
  SmaliMatch,
  SmaliMethod,
  SmaliRuleHit,
  SmaliTarget,
  StreamEvent,
  ForcedReturn,
  CheckpointInfo,
  ApiError,
} from "../types";

const CAT_LABEL: Record<string, string> = {
  premium: "Premium/Kilit",
  signature: "İmza",
  billing: "Faturalandırma",
  license: "Lisans",
  root: "Root/Emülatör",
};

const errMsg = (e: unknown) => (e as Partial<ApiError>)?.message ?? String(e);

const FORCED_LABEL: Record<ForcedReturn["kind"], string> = {
  true: "return true",
  false: "return false",
  void: "return-void",
  null: "return null",
  max_int: "return MAX_INT",
};

const DANGEROUS = [
  "CAMERA",
  "RECORD_AUDIO",
  "LOCATION",
  "READ_SMS",
  "SEND_SMS",
  "READ_CONTACTS",
  "READ_PHONE_STATE",
  "READ_EXTERNAL",
  "WRITE_EXTERNAL",
  "CALL_PHONE",
  "READ_CALL_LOG",
];

function optionsFor(m: SmaliMethod): ForcedReturn[] {
  const rt = m.return_type;
  if (rt === "V") return [{ kind: "void" }];
  if (rt.startsWith("L") || rt.startsWith("[")) return [{ kind: "null" }];
  if (rt === "I") return [{ kind: "true" }, { kind: "false" }, { kind: "max_int" }];
  if (["Z", "B", "S", "C"].includes(rt)) return [{ kind: "true" }, { kind: "false" }];
  return [];
}

export function SmaliWorkbench({
  apkPath,
  runtime,
  onGoEngine,
}: {
  apkPath: string;
  runtime?: import("../types").RuntimeKind;
  onGoEngine?: () => void;
}) {
  const [outDir, setOutDir] = useState<string | null>(null);
  const [fileCount, setFileCount] = useState(0);
  const [query, setQuery] = useState("");
  const [matches, setMatches] = useState<SmaliMatch[] | null>(null);
  const [file, setFile] = useState<string | null>(null);
  const [methods, setMethods] = useState<SmaliMethod[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [checkpoints, setCheckpoints] = useState<CheckpointInfo[]>([]);
  const [permissions, setPermissions] = useState<string[]>([]);
  const [selectedPerms, setSelectedPerms] = useState<Set<string>>(new Set());
  const [showEditor, setShowEditor] = useState(false);
  const [rules, setRules] = useState<SmaliRuleHit[] | null>(null);
  const [targets, setTargets] = useState<SmaliTarget[] | null>(null);
  const [targetCat, setTargetCat] = useState<string>("all");
  const [scoreMin, setScoreMin] = useState<number>(70);
  const [stream, setStream] = useState<StreamState>(IDLE_STREAM);

  const loadCheckpoints = async (dir: string) => {
    try {
      setCheckpoints(await invoke<CheckpointInfo[]>("list_checkpoints", { outDir: dir }));
    } catch {
      /* yoksay */
    }
  };

  // Açılışta önbelleği kontrol et — zaten decode edilmişse anında yükle (buton yok).
  useEffect(() => {
    let alive = true;
    invoke<{ cached: boolean; out_dir: string; files: number }>("smali_cache_status", {
      path: apkPath,
    })
      .then((c) => {
        if (!alive || !c.cached) return;
        setOutDir(c.out_dir);
        setFileCount(c.files);
        setStatus(`önbellekten yüklendi — ${c.files.toLocaleString()} smali dosyası`);
        loadCheckpoints(c.out_dir);
        loadPermissions(c.out_dir);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [apkPath]);

  const loadPermissions = async (dir: string) => {
    try {
      setPermissions(await invoke<string[]>("list_manifest_permissions", { outDir: dir }));
      setSelectedPerms(new Set());
    } catch {
      setPermissions([]);
    }
  };

  const togglePerm = (p: string, on: boolean) => {
    setSelectedPerms((prev) => {
      const next = new Set(prev);
      if (on) next.add(p);
      else next.delete(p);
      return next;
    });
  };

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

  const decode = () => {
    setError(null);
    setStatus(null);
    setBusy("decode");
    setStream({ ...IDLE_STREAM, active: true, percent: -1 });
    const onEvent = (ev: StreamEvent) => {
      if (ev.type === "progress") {
        setStream((s) => ({
          ...s,
          active: true,
          percent: ev.percent,
          current: ev.current,
          total: ev.total,
        }));
      } else if (ev.type === "line") {
        setStream((s) => ({ ...s, lastLine: ev.text }));
      } else if (ev.type === "done") {
        setStream(IDLE_STREAM);
        setBusy(null);
        setOutDir(ev.out_dir);
        setFileCount(ev.files);
        setStatus(`Decode edildi — ${ev.files.toLocaleString()} smali dosyası`);
        setMatches(null);
        setFile(null);
        setMethods(null);
        setRules(null);
        loadCheckpoints(ev.out_dir);
        loadPermissions(ev.out_dir);
      } else if (ev.type === "error") {
        setStream(IDLE_STREAM);
        setBusy(null);
        setError(ev.message);
      }
    };
    runStream("decode_apk_stream", { path: apkPath }, onEvent).catch((e) => {
      setStream(IDLE_STREAM);
      setBusy(null);
      setError(errMsg(e));
    });
  };

  const removePerms = () =>
    run("perms", async () => {
      if (!outDir || selectedPerms.size === 0) return;
      const count = await invoke<number>("remove_manifest_permissions", {
        outDir,
        permissions: [...selectedPerms],
      });
      setStatus(`${count} izin kaldırıldı — yeniden derleyip imzalayın`);
      loadPermissions(outDir);
    });

  const snapshot = () =>
    run("snapshot", async () => {
      if (!outDir) return;
      const cp = await invoke<CheckpointInfo>("create_checkpoint", { outDir });
      setStatus(`Checkpoint alındı: ${cp.name}`);
      loadCheckpoints(outDir);
    });

  const restore = (path: string) =>
    run("restore", async () => {
      if (!outDir) return;
      if (!window.confirm("Bu checkpoint'e dönülsün mü? Mevcut değişiklikler kaybolur.")) return;
      const msg = await invoke<string>("restore_checkpoint", { checkpointPath: path, outDir });
      setStatus(msg);
      setMatches(null);
      setFile(null);
      setMethods(null);
    });

  const search = () =>
    run("search", async () => {
      if (!outDir) return;
      const m = await invoke<SmaliMatch[]>("search_smali", { outDir, query });
      setMatches(m);
      setFile(null);
      setMethods(null);
    });

  const openFile = (f: string) =>
    run("methods", async () => {
      if (!outDir) return;
      const ms = await invoke<SmaliMethod[]>("list_smali_methods", { outDir, file: f });
      setFile(f);
      setMethods(ms);
    });

  const patch = (m: SmaliMethod, forced: ForcedReturn) =>
    run("patch", async () => {
      if (!outDir || !file) return;
      await invoke("patch_smali_method", {
        outDir,
        file,
        methodSignature: m.signature,
        forced,
      });
      setStatus(`Yamalandı: ${m.signature} → ${FORCED_LABEL[forced.kind]}`);
    });

  const scanRules = () =>
    run("rules", async () => {
      if (!outDir) return;
      const hits = await invoke<SmaliRuleHit[]>("scan_smali_rules", { outDir });
      setRules(hits);
      setStatus(`Hızlı kural taraması: ${hits.length} aday bulundu`);
    });

  const scanTargets = () =>
    run("targets", async () => {
      if (!outDir) return;
      const t = await invoke<SmaliTarget[]>("smali_smart_targets", { outDir });
      setTargets(t);
      setStatus(`Akıllı tarama: ${t.length} yüksek-olasılıklı hedef bulundu`);
    });

  // Kategoriye/öneriye göre efektif yama türü (ekonomi int → MAX_INT).
  const effectiveKind = (t: SmaliTarget): "true" | "max_int" =>
    t.suggested_template === "return_max_int" && t.return_type !== "Z" ? "max_int" : "true";

  const forceTarget = (t: SmaliTarget) =>
    run("rulepatch", async () => {
      if (!outDir) return;
      const kind = effectiveKind(t);
      await invoke("patch_smali_method", {
        outDir,
        file: t.file,
        methodSignature: t.signature,
        forced: { kind } as ForcedReturn,
      });
      setStatus(`Yamalandı: ${t.signature} → ${kind === "max_int" ? "MAX_INT" : "true"}`);
    });

  const forceTrue = (hit: SmaliRuleHit) =>
    run("rulepatch", async () => {
      if (!outDir || !hit.method) return;
      await invoke("patch_smali_method", {
        outDir,
        file: hit.file,
        methodSignature: hit.method,
        forced: { kind: "true" } as ForcedReturn,
      });
      setStatus(`Yamalandı: ${hit.method} → return true (${hit.file})`);
    });

  const build = () =>
    run("build", async () => {
      if (!outDir) return;
      const apk = await invoke<string>("build_apk", { outDir });
      setStatus(`Yeniden derlendi → ${apk} — Dağıt sekmesinde imzalayın`);
    });

  if (!outDir) {
    return (
      <div className="panel-empty">
        <EngineHint runtime={runtime} onGoEngine={onGoEngine ?? (() => {})} />
        <p>APK'yı Smali'ye açmak için apktool ile decode edin.</p>
        <button className="primary-btn" onClick={decode} disabled={busy !== null}>
          {busy === "decode" ? "Decode ediliyor…" : "APK'yı decode et"}
        </button>
        <ProgressBar state={stream} />
        {error && <p className="error">Hata: {error}</p>}
      </div>
    );
  }

  return (
    <div className="smali-workbench">
      <EngineHint runtime={runtime} onGoEngine={onGoEngine ?? (() => {})} />
      <div className="wb-header">
        <span className="muted mono">{fileCount} smali dosyası</span>
        <div className="wb-actions">
          <button onClick={decode} disabled={busy !== null}>
            {busy === "decode" ? "…" : "yeniden decode"}
          </button>
          <button className="smart-btn" onClick={scanTargets} disabled={busy !== null}>
            <Brain size={14} /> {busy === "targets" ? "Analiz…" : "Akıllı Hedefleri Tara"}
          </button>
          <button onClick={scanRules} disabled={busy !== null}>
            <ScanSearch size={14} /> {busy === "rules" ? "Taranıyor…" : "hızlı kural tara"}
          </button>
          <button onClick={snapshot} disabled={busy !== null}>
            {busy === "snapshot" ? "…" : "checkpoint al"}
          </button>
          <button className="primary-btn" onClick={build} disabled={busy !== null}>
            {busy === "build" ? "Derleniyor…" : "yeniden derle (build)"}
          </button>
        </div>
      </div>

      <ProgressBar state={stream} />

      {checkpoints.length > 0 && (
        <div className="checkpoints">
          <span className="field-label">Checkpoint'ler</span>
          {checkpoints.map((c) => (
            <div key={c.path} className="checkpoint-row">
              <span className="mono">{c.name}</span>
              <button onClick={() => restore(c.path)} disabled={busy !== null}>
                geri yükle
              </button>
            </div>
          ))}
        </div>
      )}

      {permissions.length > 0 && (
        <details className="perm-manager">
          <summary>Manifest izinleri ({permissions.length}) — kaldırmak için seçin</summary>
          <div className="perm-list">
            {permissions.map((p) => (
              <label
                key={p}
                className={DANGEROUS.some((d) => p.includes(d)) ? "perm-danger" : ""}
              >
                <input
                  type="checkbox"
                  checked={selectedPerms.has(p)}
                  onChange={(e) => togglePerm(p, e.target.checked)}
                />
                <span className="mono">{p.replace("android.permission.", "")}</span>
              </label>
            ))}
          </div>
          <button
            className="perm-remove"
            onClick={removePerms}
            disabled={busy !== null || selectedPerms.size === 0}
          >
            {busy === "perms" ? "Kaldırılıyor…" : `${selectedPerms.size} izni kaldır`}
          </button>
        </details>
      )}

      {targets && (
        <div className="rules-panel smart-panel">
          <div className="manifest-head">
            <span className="field-label">
              <Brain size={14} style={{ verticalAlign: "-0.15em" }} /> Yüksek-olasılıklı
              hedefler ({targets.length}) — amaca göre grupla
            </span>
            <button className="rules-close" onClick={() => setTargets(null)}>
              gizle
            </button>
          </div>

          {targets.length === 0 ? (
            <p className="packer-box clean" style={{ marginTop: 8 }}>
              Yüksek puanlı lisans/kilit fonksiyonu bulunamadı.
            </p>
          ) : (
            (() => {
              const scoreFiltered = targets.filter((t) => t.score >= scoreMin);
              const counts = countByCategory(scoreFiltered);
              const cats = CAT_ORDER.filter((c) => counts[c]);
              const shown =
                targetCat === "all"
                  ? scoreFiltered
                  : scoreFiltered.filter((t) => t.category === targetCat);
              return (
                <>
                  <div className="score-pills">
                    <button
                      className={`score-pill sp-high ${scoreMin === 70 ? "active" : ""}`}
                      onClick={() => setScoreMin(70)}
                    >
                      Kesin Hedefler (≥70) <b>{targets.filter((t) => t.score >= 70).length}</b>
                    </button>
                    <button
                      className={`score-pill sp-mid ${scoreMin === 50 ? "active" : ""}`}
                      onClick={() => setScoreMin(50)}
                    >
                      Tüm Olasılar (≥50) <b>{targets.filter((t) => t.score >= 50).length}</b>
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
                      className={`cat-tab ${targetCat === "all" ? "active" : ""}`}
                      onClick={() => setTargetCat("all")}
                    >
                      Hepsi <b>{scoreFiltered.length}</b>
                    </button>
                    {cats.map((c) => (
                      <button
                        key={c}
                        className={`cat-tab cat-${c} ${targetCat === c ? "active" : ""}`}
                        onClick={() => setTargetCat(c)}
                      >
                        {TARGET_CAT_LABEL[c]} <b>{counts[c]}</b>
                      </button>
                    ))}
                  </div>

                  <div className="rules-rows">
                    {shown.map((t, i) => (
                      <div key={i} className={`target-row ${t.risky ? "risky" : ""}`}>
                        <div
                          className={`target-score ${
                            t.score >= 70 ? "ts-high" : t.score >= 50 ? "ts-mid" : "ts-low"
                          }`}
                          title={t.reasons.join(" · ")}
                        >
                          {t.score}
                        </div>
                        <div className="target-body">
                          <div className="target-sig mono">
                            {t.signature}
                            <span className={`cat-chip cat-${t.category}`}>
                              {TARGET_CAT_LABEL[t.category] ?? t.category}
                            </span>
                            {t.risky && (
                              <span className="risk-badge" title="satın-alma/işlem akışı — çökme riski">
                                <AlertTriangle size={12} /> riskli
                              </span>
                            )}
                          </div>
                          <div className="target-conf muted">
                            {t.confidence} · öneri: {TEMPLATE_LABEL[t.suggested_template]}
                          </div>
                          <div className="target-file muted mono">{t.file}</div>
                        </div>
                        {t.patchable && (
                          <button
                            className={`rule-force ${
                              effectiveKind(t) === "max_int" ? "force-maxint" : ""
                            } ${t.risky ? "force-risky" : ""}`}
                            onClick={() => {
                              if (
                                t.risky &&
                                !window.confirm(
                                  `${t.signature}\n\nBu bir satın-alma/işlem akışı olabilir — ` +
                                    `yama oyunu çökertebilir. Yine de uygulansın mı?`
                                )
                              )
                                return;
                              forceTarget(t);
                            }}
                            disabled={busy !== null}
                            title={
                              effectiveKind(t) === "max_int"
                                ? "MAX_INT (4 milyar) bas"
                                : "return true bas"
                            }
                          >
                            {t.risky ? (
                              <AlertTriangle size={13} />
                            ) : (
                              <Wand2 size={13} />
                            )}{" "}
                            {t.risky
                              ? "onayla & bas"
                              : effectiveKind(t) === "max_int"
                                ? "Max Int"
                                : "True"}
                          </button>
                        )}
                      </div>
                    ))}
                  </div>
                </>
              );
            })()
          )}
        </div>
      )}

      {rules && (
        <div className="rules-panel">
          <div className="manifest-head">
            <span className="field-label">
              Hızlı kural adayları ({rules.length})
            </span>
            <button className="rules-close" onClick={() => setRules(null)}>
              gizle
            </button>
          </div>
          {rules.length === 0 ? (
            <p className="packer-box clean" style={{ marginTop: 8 }}>
              Bilinen para-kazanç/koruma kalıbı bulunamadı.
            </p>
          ) : (
            <div className="rules-rows">
              {rules.map((h, i) => (
                <div key={i} className={`rule-row cat-${h.category}`}>
                  <span className="rule-cat">{CAT_LABEL[h.category] ?? h.category}</span>
                  <div className="rule-body">
                    <div className="rule-title">{h.rule}</div>
                    <code className="rule-snip mono muted">{h.snippet}</code>
                    <div className="rule-loc muted mono">
                      {h.file}:{h.line}
                    </div>
                    <div className="rule-sugg muted">{h.suggestion}</div>
                  </div>
                  <div className="rule-actions">
                    <button onClick={() => openFile(h.file)} disabled={busy !== null}>
                      aç
                    </button>
                    {h.category === "premium" && h.method && (
                      <button
                        className="rule-force"
                        onClick={() => forceTrue(h)}
                        disabled={busy !== null}
                        title="metodu return true ile zorla"
                      >
                        <Wand2 size={13} /> true
                      </button>
                    )}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      <div className="inline-field grow">
        <input
          placeholder="tüm DEX'lerde ara — ör. isPremium, checkLicense, unlock"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && search()}
        />
        <button onClick={search} disabled={busy !== null}>
          {busy === "search" ? "…" : "Ara"}
        </button>
      </div>

      {error && <p className="error">Hata: {error}</p>}
      {status && <p className="patch-ok">{status}</p>}

      <div className="wb-split">
        <div className="wb-matches">
          <h4>Eşleşmeler {matches ? `(${matches.length})` : ""}</h4>
          <div className="wb-scroll">
            {matches?.map((m, i) => (
              <div
                key={`${m.file}:${m.line}:${i}`}
                className={`wb-match ${file === m.file ? "sel" : ""}`}
                onClick={() => openFile(m.file)}
                title={m.file}
              >
                <span className="wb-file mono">{m.file}</span>
                <span className="wb-line mono">{m.text}</span>
              </div>
            ))}
            {matches && matches.length === 0 && <p className="muted">Eşleşme yok.</p>}
          </div>
        </div>

        <div className="wb-methods">
          <div className="wb-methods-head">
            <h4>{file ? "Metodlar" : "Bir dosya seçin"}</h4>
            {file && (
              <button className="editor-toggle" onClick={() => setShowEditor((s) => !s)}>
                {showEditor ? "metodlar" : "</> kod"}
              </button>
            )}
          </div>
          <div className="wb-scroll">
            {methods?.map((m, i) => {
              const opts = optionsFor(m);
              return (
                <div key={`${m.signature}#${i}`} className="wb-method">
                  <span className="mono wb-sig">{m.signature}</span>
                  <span className="wb-patchbtns">
                    {opts.length === 0 ? (
                      <span className="muted">—</span>
                    ) : (
                      opts.map((o) => (
                        <button
                          key={o.kind}
                          disabled={busy !== null}
                          onClick={() => patch(m, o)}
                        >
                          {FORCED_LABEL[o.kind]}
                        </button>
                      ))
                    )}
                  </span>
                </div>
              );
            })}
          </div>
        </div>
      </div>

      {file && showEditor && outDir && <SmaliEditor outDir={outDir} file={file} />}
    </div>
  );
}
