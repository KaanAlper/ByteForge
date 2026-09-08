import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  RefreshCw,
  Search,
  Filter,
  Snowflake,
  ShieldAlert,
  Plus,
  Trash2,
  Crosshair,
  Save,
  X,
} from "lucide-react";
import type {
  ProcInfo,
  ScanSummary,
  AddrValue,
  MemValueType,
  Compare,
  SavedEntry,
  PointerChain,
  ApiError,
} from "../types";

const errMsg = (e: unknown) => (e as Partial<ApiError>)?.message ?? String(e);
const hx = (n: number) => "0x" + n.toString(16).toUpperCase();
const SAVED_KEY = "byteforge.memscan.saved";

const TYPES: MemValueType[] = ["i32", "i64", "f32", "f64", "u8"];

// Karşılaştırma seçenekleri — Türkçe etiket + ilk-tarama/daraltma uygunluğu.
const COMPARES: {
  id: Compare;
  label: string;
  needsValue: boolean;
  needsValue2?: boolean;
  firstScan: boolean; // ilk taramada (session yokken) seçilebilir mi
}[] = [
  { id: "exact", label: "eşit  =", needsValue: true, firstScan: true },
  { id: "not_equal", label: "eşit değil  ≠", needsValue: true, firstScan: true },
  { id: "greater", label: "büyük  >", needsValue: true, firstScan: true },
  { id: "less", label: "küçük  <", needsValue: true, firstScan: true },
  { id: "between", label: "aralık  a–b", needsValue: true, needsValue2: true, firstScan: true },
  { id: "unknown", label: "bilinmeyen ilk değer", needsValue: false, firstScan: true },
  { id: "increased", label: "arttı", needsValue: false, firstScan: false },
  { id: "decreased", label: "azaldı", needsValue: false, firstScan: false },
  { id: "changed", label: "değişti", needsValue: false, firstScan: false },
  { id: "unchanged", label: "değişmedi", needsValue: false, firstScan: false },
  { id: "increased_by", label: "şu kadar arttı", needsValue: true, firstScan: false },
  { id: "decreased_by", label: "şu kadar azaldı", needsValue: true, firstScan: false },
];

const cmpOf = (id: Compare) => COMPARES.find((c) => c.id === id)!;

export function MemScanner() {
  const [procs, setProcs] = useState<ProcInfo[]>([]);
  const [procQuery, setProcQuery] = useState("");
  const [pid, setPid] = useState<number | null>(null);
  const [ty, setTy] = useState<MemValueType>("i32");
  const [compare, setCompare] = useState<Compare>("exact");
  const [value, setValue] = useState("");
  const [value2, setValue2] = useState("");
  const [session, setSession] = useState<number | null>(null);
  const [summary, setSummary] = useState<ScanSummary | null>(null);
  const [rows, setRows] = useState<AddrValue[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [scope, setScope] = useState("");

  // Kayıtlı adres tablosu (kalıcı).
  const [saved, setSaved] = useState<SavedEntry[]>(() => {
    try {
      return JSON.parse(localStorage.getItem(SAVED_KEY) || "[]");
    } catch {
      return [];
    }
  });
  const [liveVals, setLiveVals] = useState<Record<string, string>>({});

  // Pointer scan.
  const [ptrTarget, setPtrTarget] = useState("");
  const [ptrOffset, setPtrOffset] = useState("4096");
  const [ptrDepth, setPtrDepth] = useState("2");
  const [chains, setChains] = useState<PointerChain[] | null>(null);
  const [showPtr, setShowPtr] = useState(false);

  const savedRef = useRef(saved);
  savedRef.current = saved;

  useEffect(() => {
    localStorage.setItem(SAVED_KEY, JSON.stringify(saved));
  }, [saved]);

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

  // Canlı değer güncelleme: kayıtlı girişler + sonuç önizlemesi (700ms).
  useEffect(() => {
    if (pid == null) return;
    const tick = async () => {
      // kayıtlı girişler
      const entries = savedRef.current.filter((e) => e.pid === pid);
      if (entries.length) {
        const next: Record<string, string> = {};
        for (const e of entries) {
          try {
            next[e.id] = await invoke<string>("read_memory", {
              pid: e.pid,
              address: e.address,
              ty: e.ty,
            });
          } catch {
            next[e.id] = "?";
          }
        }
        setLiveVals((v) => ({ ...v, ...next }));
      }
      // sonuç önizlemesi
      if (session != null) {
        try {
          setRows(await invoke<AddrValue[]>("scan_read", { session }));
        } catch {
          /* oturum bitmiş olabilir */
        }
      }
    };
    const h = setInterval(tick, 700);
    return () => clearInterval(h);
  }, [pid, session]);

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

  const firstScan = () =>
    run("scan", async () => {
      if (pid == null) return;
      const c = cmpOf(compare);
      const s = await invoke<ScanSummary>("scan_new", {
        pid,
        ty,
        compare,
        value: c.needsValue ? value : "",
        value2: c.needsValue2 ? value2 : null,
      });
      setSession(s.session);
      setSummary(s);
      setRows(s.preview);
    });

  const nextScan = () =>
    run("refine", async () => {
      if (session == null) return;
      const c = cmpOf(compare);
      const s = await invoke<ScanSummary>("scan_next", {
        session,
        compare,
        value: c.needsValue ? value : null,
        value2: c.needsValue2 ? value2 : null,
      });
      setSummary(s);
      setRows(s.preview);
    });

  const resetScan = () =>
    run("reset", async () => {
      if (session != null) await invoke("scan_clear", { session });
      setSession(null);
      setSummary(null);
      setRows([]);
      // ilk-tarama uygun bir karşılaştırmaya geri dön
      if (!cmpOf(compare).firstScan) setCompare("exact");
    });

  // Kayıtlı tabloya ekle.
  const addToSaved = (addr: number, val: string) => {
    if (pid == null) return;
    const id = `${pid}-${addr}-${Date.now()}`;
    setSaved((s) => [
      ...s,
      { id, pid, address: addr, ty, label: "", value: val, frozen: false },
    ]);
  };

  const updateEntry = (id: string, patch: Partial<SavedEntry>) =>
    setSaved((s) => s.map((e) => (e.id === id ? { ...e, ...patch } : e)));

  const removeEntry = async (e: SavedEntry) => {
    if (e.frozen) {
      try {
        await invoke("freeze_remove", { pid: e.pid, address: e.address });
      } catch {
        /* yok say */
      }
    }
    setSaved((s) => s.filter((x) => x.id !== e.id));
  };

  const writeEntry = (e: SavedEntry) =>
    run("write", async () => {
      await invoke("write_memory", {
        pid: e.pid,
        address: e.address,
        value: e.value,
        ty: e.ty,
      });
      if (e.frozen) {
        await invoke("freeze_add", {
          pid: e.pid,
          address: e.address,
          value: e.value,
          ty: e.ty,
          label: e.label || null,
        });
      }
    });

  const toggleFreeze = (e: SavedEntry) =>
    run("freeze", async () => {
      if (e.frozen) {
        await invoke("freeze_remove", { pid: e.pid, address: e.address });
        updateEntry(e.id, { frozen: false });
      } else {
        await invoke("freeze_add", {
          pid: e.pid,
          address: e.address,
          value: e.value,
          ty: e.ty,
          label: e.label || null,
        });
        updateEntry(e.id, { frozen: true });
      }
    });

  const runPointerScan = () =>
    run("ptr", async () => {
      if (pid == null) return;
      const target = parseInt(ptrTarget.trim().replace(/^0x/i, ""), 16);
      if (!Number.isFinite(target)) {
        setError("geçerli bir hedef adres girin (hex)");
        return;
      }
      const c = await invoke<PointerChain[]>("pointer_scan", {
        pid,
        target,
        maxOffset: parseInt(ptrOffset, 10) || 4096,
        maxDepth: parseInt(ptrDepth, 10) || 2,
      });
      setChains(c);
    });

  const pq = procQuery.toLowerCase();
  const filteredProcs = procs.filter(
    (p) => p.name.toLowerCase().includes(pq) || String(p.pid).includes(pq)
  );
  const c = cmpOf(compare);
  const availableCompares = session == null ? COMPARES.filter((x) => x.firstScan) : COMPARES;

  return (
    <div className="memscan">
      <div className="profile-head">
        <h2>Bellek Tarayıcı</h2>
        <span className="badge">Cheat Engine tarzı — çoklu platform</span>
      </div>

      <div className="error-hint" style={{ marginBottom: 14 }}>
        <ShieldAlert size={14} /> Canlı süreç belleğini okur/yazar — izin gerekir
        (Linux: ptrace, Windows: yönetici, macOS: debugger yetkisi). Linux
        ptrace_scope: <b>{scope || "…"}</b>. Yalnızca kendi/yetkili süreçlerinizde
        kullanın.
      </div>

      {/* Süreç seçimi */}
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

      {/* Tarama kontrolleri */}
      <div className="scan-controls">
        <select value={ty} onChange={(e) => setTy(e.target.value as MemValueType)} disabled={session != null}>
          {TYPES.map((t) => (
            <option key={t} value={t}>
              {t}
            </option>
          ))}
        </select>
        <select value={compare} onChange={(e) => setCompare(e.target.value as Compare)}>
          {availableCompares.map((x) => (
            <option key={x.id} value={x.id}>
              {x.label}
            </option>
          ))}
        </select>
        {c.needsValue && (
          <input
            placeholder={c.needsValue2 ? "alt (a)" : "değer"}
            value={value}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && (session == null ? firstScan() : nextScan())}
          />
        )}
        {c.needsValue2 && (
          <input placeholder="üst (b)" value={value2} onChange={(e) => setValue2(e.target.value)} />
        )}
        {session == null ? (
          <button
            className="primary-btn"
            onClick={firstScan}
            disabled={busy !== null || pid == null || (c.needsValue && !value.trim())}
          >
            <Search size={15} /> {busy === "scan" ? "Taranıyor…" : "İlk Tara"}
          </button>
        ) : (
          <>
            <button onClick={nextScan} disabled={busy !== null}>
              <Filter size={14} /> {busy === "refine" ? "…" : "Daralt"}
            </button>
            <button onClick={resetScan} disabled={busy !== null} title="yeni tarama">
              <X size={14} /> Sıfırla
            </button>
          </>
        )}
      </div>

      {error && <p className="error">Hata: {error}</p>}

      {/* Sonuçlar */}
      {summary && (
        <div className="scan-result">
          <div className="sym-stats">
            <span className="sym-stat">
              <b>{summary.total.toLocaleString()}</b> eşleşme
            </span>
            {summary.truncated && <span className="sym-stat muted">(kırpıldı, daha çok daraltın)</span>}
            {summary.scanned_regions > 0 && (
              <span className="sym-stat muted">{summary.scanned_regions} bölge tarandı</span>
            )}
            {summary.total > rows.length && (
              <span className="sym-stat muted">ilk {rows.length} gösteriliyor</span>
            )}
          </div>

          <div className="addr-list">
            {rows.map((r) => (
              <div key={r.address} className="addr-row">
                <span className="mono addr-hex">{hx(r.address)}</span>
                <span className="mono addr-val">{r.value}</span>
                <button
                  className="addr-write"
                  title="kayıtlı tabloya ekle"
                  onClick={() => addToSaved(r.address, r.value)}
                >
                  <Plus size={13} /> ekle
                </button>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Kayıtlı adres tablosu */}
      <div className="saved-table">
        <div className="manifest-head">
          <span className="field-label">
            <Save size={13} /> Kayıtlı Adresler ({saved.length})
          </span>
          {saved.length > 0 && (
            <button className="mm-add" onClick={() => setSaved([])} title="tümünü temizle">
              <Trash2 size={13} /> temizle
            </button>
          )}
        </div>
        {saved.length === 0 ? (
          <p className="muted" style={{ margin: "6px 0" }}>
            Sonuçlardan <b>ekle</b> ile adres kaydedin — isimlendirin, değer yazın, dondurun.
          </p>
        ) : (
          <div className="saved-rows">
            {saved.map((e) => (
              <div key={e.id} className={`saved-row ${e.frozen ? "frozen" : ""}`}>
                <button
                  className={`freeze-btn ${e.frozen ? "on" : ""}`}
                  onClick={() => toggleFreeze(e)}
                  title={e.frozen ? "dondurmayı kaldır" : "dondur"}
                >
                  <Snowflake size={14} />
                </button>
                <input
                  className="saved-label"
                  placeholder="isim"
                  value={e.label}
                  onChange={(ev) => updateEntry(e.id, { label: ev.target.value })}
                />
                <span className="mono saved-addr">{hx(e.address)}</span>
                <span className="saved-ty muted">{e.ty}</span>
                <span className="mono saved-live" title="canlı değer">
                  {liveVals[e.id] ?? "—"}
                </span>
                <input
                  className="saved-write"
                  value={e.value}
                  onChange={(ev) => updateEntry(e.id, { value: ev.target.value })}
                  onKeyDown={(ev) => ev.key === "Enter" && writeEntry(e)}
                />
                <button className="addr-write" onClick={() => writeEntry(e)} title="değer yaz">
                  yaz
                </button>
                <button className="saved-del" onClick={() => removeEntry(e)} title="sil">
                  <X size={13} />
                </button>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Pointer scan (gelişmiş) */}
      <div className="ptr-scan">
        <button className="ptr-toggle" onClick={() => setShowPtr((v) => !v)}>
          <Crosshair size={13} /> Pointer Scan {showPtr ? "▲" : "▼"}
        </button>
        {showPtr && (
          <div className="ptr-body">
            <p className="muted" style={{ marginTop: 0 }}>
              Dinamik bir adrese ulaşan statik pointer zincirlerini bulur (yeniden
              başlatmaya dayanıklı adres). Hedef, taramada bulduğun adres olabilir.
            </p>
            <div className="scan-controls">
              <input
                placeholder="hedef adres (hex, ör. 7f...)"
                value={ptrTarget}
                onChange={(e) => setPtrTarget(e.target.value)}
              />
              <input
                style={{ maxWidth: 120 }}
                placeholder="max ofset"
                value={ptrOffset}
                onChange={(e) => setPtrOffset(e.target.value)}
                title="max ofset (bayt)"
              />
              <select value={ptrDepth} onChange={(e) => setPtrDepth(e.target.value)} title="derinlik">
                {["1", "2", "3", "4"].map((d) => (
                  <option key={d} value={d}>
                    derinlik {d}
                  </option>
                ))}
              </select>
              <button
                className="primary-btn"
                onClick={runPointerScan}
                disabled={busy !== null || pid == null || !ptrTarget.trim()}
              >
                <Crosshair size={14} /> {busy === "ptr" ? "Aranıyor…" : "Tara"}
              </button>
            </div>
            {chains && (
              <div className="ptr-results">
                {chains.length === 0 ? (
                  <p className="muted">Zincir bulunamadı — max ofseti/derinliği artırın.</p>
                ) : (
                  <>
                    <p className="muted">{chains.length} zincir (statik köklü):</p>
                    {chains.map((ch, i) => (
                      <div key={i} className="ptr-chain mono">
                        <b>{ch.base_module}</b>+{hx(ch.base_offset)}
                        {ch.offsets.map((o, j) => (
                          <span key={j} className="ptr-off"> → +{hx(o)}</span>
                        ))}
                      </div>
                    ))}
                  </>
                )}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
