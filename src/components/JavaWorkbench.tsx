import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { JavaMatch, CacheStatus, StreamEvent, ApiError } from "../types";
import { runStream, ProgressBar, IDLE_STREAM, type StreamState } from "./StreamProgress";
import { EngineHint } from "./EngineHint";

const errMsg = (e: unknown) => (e as Partial<ApiError>)?.message ?? String(e);
const baseName = (p: string) => p.split(/[/\\]/).pop() ?? p;

export function JavaWorkbench({
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
  const [matches, setMatches] = useState<JavaMatch[] | null>(null);
  const [file, setFile] = useState<string | null>(null);
  const [content, setContent] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [stream, setStream] = useState<StreamState>(IDLE_STREAM);
  const [checkedCache, setCheckedCache] = useState(false);

  // Açılışta önbelleği kontrol et — zaten decompile edilmişse doğrudan yükle.
  useEffect(() => {
    let alive = true;
    invoke<CacheStatus>("jadx_cache_status", { path: apkPath })
      .then((c) => {
        if (!alive) return;
        if (c.cached) {
          setOutDir(c.out_dir);
          setFileCount(c.files);
          setStatus(`önbellekten yüklendi — ${c.files.toLocaleString()} Java dosyası`);
        }
      })
      .catch(() => {})
      .finally(() => alive && setCheckedCache(true));
    return () => {
      alive = false;
    };
  }, [apkPath]);

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

  const decompile = useCallback(() => {
    setError(null);
    setBusy("decompile");
    setStatus(null);
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
        setStatus(`${ev.files.toLocaleString()} Java dosyası (kısmi hatalar tolere edildi)`);
        setMatches(null);
        setFile(null);
        setContent(null);
      } else if (ev.type === "error") {
        setStream(IDLE_STREAM);
        setBusy(null);
        setError(ev.message);
      }
    };
    runStream("jadx_decompile_stream", { path: apkPath }, onEvent).catch((e) => {
      setStream(IDLE_STREAM);
      setBusy(null);
      setError(errMsg(e));
    });
  }, [apkPath]);

  const search = () =>
    run("search", async () => {
      if (!outDir) return;
      setMatches(await invoke<JavaMatch[]>("search_java", { outDir, query }));
      setFile(null);
      setContent(null);
    });

  const open = (f: string) =>
    run("read", async () => {
      if (!outDir) return;
      setContent(await invoke<string>("read_java", { outDir, file: f }));
      setFile(f);
    });

  if (!outDir) {
    return (
      <div className="panel-empty">
        <EngineHint runtime={runtime} onGoEngine={onGoEngine ?? (() => {})} />
        <p>APK'yı jadx ile okunabilir Java kaynağına çevirin (Smali'nin okunur hali).</p>
        <button className="primary-btn" onClick={decompile} disabled={busy !== null || !checkedCache}>
          {busy === "decompile" ? "Decompile ediliyor…" : "jadx ile decompile et"}
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
        <span className="muted mono">{fileCount.toLocaleString()} Java dosyası</span>
        <div className="wb-actions">
          <button onClick={decompile} disabled={busy !== null}>
            {busy === "decompile" ? "…" : "yeniden decompile"}
          </button>
        </div>
      </div>

      <ProgressBar state={stream} />

      <div className="inline-field grow">
        <input
          placeholder="Java kaynağında ara — ör. isVip, checkLicense, getCoins"
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
                onClick={() => open(m.file)}
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
          <h4>{file ? baseName(file) : "Bir dosya seçin"}</h4>
          <div className="smali-code mono" style={{ maxHeight: 460 }}>
            {content ? (
              content.split("\n").map((l, i) => (
                <div key={i}>
                  <span className="sm-ln">{i + 1}</span>
                  {l || " "}
                </div>
              ))
            ) : (
              <span className="muted">—</span>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
