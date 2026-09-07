import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { RefreshCw, Search, Filter, Pencil, ShieldAlert } from "lucide-react";
import type { ProcInfo, ScanResult, MemValueType, ApiError } from "../types";

const errMsg = (e: unknown) => (e as Partial<ApiError>)?.message ?? String(e);
const hx = (n: number) => "0x" + n.toString(16).toUpperCase();

const TYPES: MemValueType[] = ["i32", "i64", "f32", "f64", "u8"];
const MAX_SHOW = 300;

export function MemScanner() {
  const [procs, setProcs] = useState<ProcInfo[]>([]);
  const [procQuery, setProcQuery] = useState("");
  const [pid, setPid] = useState<number | null>(null);
  const [ty, setTy] = useState<MemValueType>("i32");
  const [value, setValue] = useState("");
  const [result, setResult] = useState<ScanResult | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [scope, setScope] = useState<string>("");
  const [writeVal, setWriteVal] = useState("");
  const [reads, setReads] = useState<Record<number, string>>({});

  const loadProcs = useCallback(async () => {
    setError(null);
    try {
      setProcs(await invoke<ProcInfo[]>("list_processes"));
    } catch (e) {
      setError(errMsg(e));
    }
  }, []);

  useEffect(() => {
    loadProcs();
    invoke<string>("ptrace_scope").then(setScope).catch(() => {});
  }, [loadProcs]);

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

  const scan = () =>
    run("scan", async () => {
      if (pid == null || !value.trim()) return;
      setReads({});
      setResult(await invoke<ScanResult>("scan_process", { pid, value, ty }));
    });

  const refine = () =>
    run("refine", async () => {
      if (pid == null || !result || !value.trim()) return;
      setResult(
        await invoke<ScanResult>("refine_scan", {
          pid,
          addresses: result.addresses,
          value,
          ty,
        })
      );
      setReads({});
    });

  const readAll = () =>
    run("read", async () => {
      if (pid == null || !result) return;
      const out: Record<number, string> = {};
      for (const a of result.addresses.slice(0, MAX_SHOW)) {
        try {
          out[a] = await invoke<string>("read_memory", { pid, address: a, ty });
        } catch {
          out[a] = "?";
        }
      }
      setReads(out);
    });

  const writeAll = () =>
    run("write", async () => {
      if (pid == null || !result || !writeVal.trim()) return;
      let ok = 0;
      for (const a of result.addresses.slice(0, MAX_SHOW)) {
        try {
          await invoke("write_memory", { pid, address: a, value: writeVal, ty });
          ok++;
        } catch (e) {
          setError(errMsg(e));
          break;
        }
      }
      if (ok > 0) {
        setError(null);
      }
    });

  const writeOne = async (addr: number) => {
    if (pid == null || !writeVal.trim()) return;
    try {
      await invoke("write_memory", { pid, address: addr, value: writeVal, ty });
      const v = await invoke<string>("read_memory", { pid, address: addr, ty });
      setReads((r) => ({ ...r, [addr]: v }));
    } catch (e) {
      setError(errMsg(e));
    }
  };

  const pq = procQuery.toLowerCase();
  const filteredProcs = procs.filter(
    (p) => p.name.toLowerCase().includes(pq) || String(p.pid).includes(pq)
  );

  return (
    <div className="memscan">
      <div className="profile-head">
        <h2>Bellek Tarayıcı</h2>
        <span className="badge">Cheat Engine tarzı (canlı süreç)</span>
      </div>

      <div className="error-hint" style={{ marginBottom: 14 }}>
        <ShieldAlert size={14} /> Canlı süreç belleğini okur/yazar — ptrace izni
        gerekir. İzin hatası alırsanız uygulamayı <b>sudo</b> ile çalıştırın ya da{" "}
        <code className="mono">ptrace_scope=0</code> yapın. Mevcut ptrace_scope:{" "}
        <b>{scope || "…"}</b>. Yalnızca kendi/yetkili süreçlerinizde kullanın.
      </div>

      <div className="mm-field">
        <div className="manifest-head">
          <span className="field-label">Süreç seç ({procs.length})</span>
          <button onClick={loadProcs} className="mm-add" title="listeyi yenile">
            <RefreshCw size={13} /> yenile
          </button>
        </div>
        <input
          className="mm-title"
          style={{ maxWidth: "100%" }}
          placeholder="süreç ara — ad veya pid"
          value={procQuery}
          onChange={(e) => setProcQuery(e.target.value)}
        />
        <div className="proc-list">
          {filteredProcs.slice(0, 200).map((p) => (
            <div
              key={p.pid}
              className={`proc-row ${pid === p.pid ? "sel" : ""}`}
              onClick={() => setPid(p.pid)}
            >
              <span className="mono proc-pid">{p.pid}</span>
              <span className="proc-name">{p.name}</span>
            </div>
          ))}
        </div>
      </div>

      <div className="scan-controls">
        <select value={ty} onChange={(e) => setTy(e.target.value as MemValueType)}>
          {TYPES.map((t) => (
            <option key={t} value={t}>
              {t}
            </option>
          ))}
        </select>
        <input
          placeholder="aranacak değer — ör. 1000"
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && scan()}
        />
        <button className="primary-btn" onClick={scan} disabled={busy !== null || pid == null || !value.trim()}>
          <Search size={15} /> {busy === "scan" ? "Taranıyor…" : "İlk Tara"}
        </button>
        <button onClick={refine} disabled={busy !== null || !result}>
          <Filter size={14} /> {busy === "refine" ? "…" : "Daralt (next)"}
        </button>
      </div>

      {error && <p className="error">Hata: {error}</p>}

      {result && (
        <div className="scan-result">
          <div className="sym-stats">
            <span className="sym-stat">
              <b>{result.total.toLocaleString()}</b> eşleşme
            </span>
            {result.truncated && <span className="sym-stat muted">(kırpıldı, daraltın)</span>}
            {result.scanned_regions > 0 && (
              <span className="sym-stat muted">{result.scanned_regions} bölge tarandı</span>
            )}
          </div>

          {result.addresses.length > 0 && result.addresses.length <= 2000 && (
            <div className="scan-write-bar">
              <input
                placeholder="yeni değer (yaz)"
                value={writeVal}
                onChange={(e) => setWriteVal(e.target.value)}
              />
              <button onClick={readAll} disabled={busy !== null}>
                {busy === "read" ? "…" : "değerleri oku"}
              </button>
              <button
                className="patch-apply"
                onClick={writeAll}
                disabled={busy !== null || !writeVal.trim()}
              >
                <Pencil size={14} /> hepsine yaz ({Math.min(result.addresses.length, MAX_SHOW)})
              </button>
            </div>
          )}

          <div className="addr-list">
            {result.addresses.slice(0, MAX_SHOW).map((a) => (
              <div key={a} className="addr-row">
                <span className="mono addr-hex">{hx(a)}</span>
                <span className="mono addr-val">{reads[a] ?? "—"}</span>
                <button className="addr-write" onClick={() => writeOne(a)} disabled={!writeVal.trim()}>
                  yaz
                </button>
              </div>
            ))}
          </div>
          {result.addresses.length > MAX_SHOW && (
            <p className="muted">İlk {MAX_SHOW} adres gösteriliyor. Daha da daraltın.</p>
          )}
        </div>
      )}
    </div>
  );
}
