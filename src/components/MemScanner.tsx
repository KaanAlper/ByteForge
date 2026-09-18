import {
  useState,
  useEffect,
  useCallback,
  useRef,
  useMemo,
  MouseEvent as ReactMouseEvent,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Monitor,
  RefreshCw,
  Search,
  Filter,
  Snowflake,
  Plus,
  Trash2,
  Crosshair,
  X,
  ArrowDown,
  Edit2,
  Check,
  RotateCcw,
  Hash,
  AlertTriangle,
  Copy,
  ChevronDown,
  AppWindow,
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

/* ─── helpers ──────────────────────────────────────────────────────────────── */
const errMsg = (e: unknown) => (e as Partial<ApiError>)?.message ?? String(e);
const hx = (n: number) => "0x" + n.toString(16).toUpperCase().padStart(8, "0");
const fmtHex = (v: string) => {
  const n = parseFloat(v);
  if (Number.isFinite(n) && Number.isInteger(n))
    return "0x" + Math.abs(Math.trunc(n)).toString(16).toUpperCase();
  return v;
};
const isPtraceErr = (s: string) =>
  s.includes("os error 13") || s.toLowerCase().includes("ptrace") || s.includes("Permission denied");

const SAVED_KEY = "byteforge.memscan.saved";

/* ─── constants ────────────────────────────────────────────────────────────── */
const TYPES: { id: MemValueType; label: string }[] = [
  { id: "u8", label: "1 Byte (u8)" },
  { id: "i32", label: "4 Bytes (i32)" },
  { id: "i64", label: "8 Bytes (i64)" },
  { id: "f32", label: "Float (f32)" },
  { id: "f64", label: "Double (f64)" },
];

const COMPARES: {
  id: Compare;
  label: string;
  needsValue: boolean;
  needsValue2?: boolean;
  firstScan: boolean;
}[] = [
  { id: "exact",        label: "Exact Value",          needsValue: true,  firstScan: true  },
  { id: "greater",      label: "Bigger than...",        needsValue: true,  firstScan: true  },
  { id: "less",         label: "Smaller than...",       needsValue: true,  firstScan: true  },
  { id: "between",      label: "Value between...",      needsValue: true,  needsValue2: true, firstScan: true },
  { id: "unknown",      label: "Unknown initial value", needsValue: false, firstScan: true  },
  { id: "increased",    label: "Increased value",       needsValue: false, firstScan: false },
  { id: "decreased",    label: "Decreased value",       needsValue: false, firstScan: false },
  { id: "changed",      label: "Changed value",         needsValue: false, firstScan: false },
  { id: "unchanged",    label: "Unchanged value",       needsValue: false, firstScan: false },
  { id: "increased_by", label: "Increased by...",       needsValue: true,  firstScan: false },
  { id: "decreased_by", label: "Decreased by...",       needsValue: true,  firstScan: false },
  { id: "not_equal",    label: "Not equal to...",       needsValue: true,  firstScan: false },
];

const cmpOf = (id: Compare) => COMPARES.find((c) => c.id === id) ?? COMPARES[0];

/* ─── component ────────────────────────────────────────────────────────────── */
export function MemScanner() {
  /* process */
  const [procs, setProcs] = useState<ProcInfo[]>([]);
  const [procQuery, setProcQuery] = useState("");
  const [showProcModal, setShowProcModal] = useState(false);
  const [procTab, setProcTab] = useState<"apps" | "all">("apps");
  const [pid, setPid] = useState<number | null>(null);

  /* scan */
  const [ty, setTy] = useState<MemValueType>("i32");
  const [compare, setCompare] = useState<Compare>("exact");
  const [value, setValue] = useState("");
  const [value2, setValue2] = useState("");
  const [session, setSession] = useState<number | null>(null);
  const [summary, setSummary] = useState<ScanSummary | null>(null);
  const [rows, setRows] = useState<AddrValue[]>([]);
  const [prevMap, setPrevMap] = useState<Record<number, string>>({});
  const [currentScanMap, setCurrentScanMap] = useState<Record<number, string>>({});
  const [undoState, setUndoState] = useState<{ rows: AddrValue[]; summary: ScanSummary } | null>(null);
  const [scanProgress, setScanProgress] = useState(0);
  const [hexDisplay, setHexDisplay] = useState(false);
  const [selectedResultAddr, setSelectedResultAddr] = useState<number | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const [tableHeight, setTableHeight] = useState(280);
  const dragRef = useRef<boolean>(false);
  useEffect(() => {
    const handleMove = (e: MouseEvent) => {
      if (!dragRef.current) return;
      const newHeight = window.innerHeight - e.clientY - 10;
      if (newHeight > 100 && newHeight < window.innerHeight - 200) {
        setTableHeight(newHeight);
      }
    };
    const handleUp = () => { dragRef.current = false; document.body.style.cursor = 'default'; };
    document.addEventListener("mousemove", handleMove);
    document.addEventListener("mouseup", handleUp);
    return () => {
      document.removeEventListener("mousemove", handleMove);
      document.removeEventListener("mouseup", handleUp);
    };
  }, []);
  const [error, setError] = useState<string | null>(null);
  const [scope, setScope] = useState("");

  /* cheat table */
  const [saved, setSaved] = useState<SavedEntry[]>(() => {
    try { return JSON.parse(localStorage.getItem(SAVED_KEY) || "[]"); } catch { return []; }
  });
  const [liveVals, setLiveVals] = useState<Record<string, string>>({});
  const [editingEntryId, setEditingEntryId] = useState<string | null>(null);
  const [editValueText, setEditValueText] = useState("");
  const [entryHexSet, setEntryHexSet] = useState<Set<string>>(new Set());

  /* manual add */
  const [showManualModal, setShowManualModal] = useState(false);
  const [manualAddr, setManualAddr] = useState("");
  const [manualDesc, setManualDesc] = useState("Adres");
  const [manualType, setManualType] = useState<MemValueType>("i32");

  /* pointer scan */
  const [ptrTarget, setPtrTarget] = useState("");
  const [ptrOffset, setPtrOffset] = useState("4096");
  const [ptrDepth, setPtrDepth] = useState("2");
  const [chains, setChains] = useState<PointerChain[] | null>(null);
  const [showPtr, setShowPtr] = useState(false);

  /* context menu */
  const [ctxMenu, setCtxMenu] = useState<{
    x: number; y: number;
    kind: "found" | "saved";
    row?: AddrValue;
    entry?: SavedEntry;
  } | null>(null);

  const savedRef = useRef(saved);
  savedRef.current = saved;
  const rowsRef = useRef(rows);
  rowsRef.current = rows;

  /* persist */
  useEffect(() => {
    localStorage.setItem(SAVED_KEY, JSON.stringify(saved));
  }, [saved]);

  /* load procs */
  const loadProcs = useCallback(async () => {
    try { setProcs(await invoke<ProcInfo[]>("list_processes")); } catch (e) { setError(errMsg(e)); }
  }, []);

  useEffect(() => {
    loadProcs();
    invoke<string>("ptrace_scope").then(setScope).catch(() => {});
  }, [loadProcs]);

  /* close ctx menu on outside click */
  useEffect(() => {
    if (!ctxMenu) return;
    const close = () => setCtxMenu(null);
    window.addEventListener("click", close);
    return () => window.removeEventListener("click", close);
  }, [ctxMenu]);

  /* live value poll */
  useEffect(() => {
    if (pid == null) return;
    const tick = async () => {
      const entries = savedRef.current.filter((e) => e.pid === pid);
      if (entries.length) {
        const next: Record<string, string> = {};
        for (const e of entries) {
          try { next[e.id] = await invoke<string>("read_memory", { pid: e.pid, address: e.address, ty: e.ty }); }
          catch { next[e.id] = "??"; }
        }
        setLiveVals((v) => ({ ...v, ...next }));
      }
      if (session != null) {
        try { setRows(await invoke<AddrValue[]>("scan_read", { session })); } catch {}
      }
    };
    const h = setInterval(tick, 600);
    return () => clearInterval(h);
  }, [pid, session]);

  /* progress simulation */
  const startProgress = useCallback(() => {
    setScanProgress(0);
    let p = 0;
    const h = setInterval(() => {
      p += 8 + Math.random() * 12;
      if (p >= 88) clearInterval(h);
      setScanProgress(Math.min(p, 88));
    }, 70);
    return () => {
      clearInterval(h);
      setScanProgress(100);
      setTimeout(() => { setScanProgress(0); }, 350);
    };
  }, []);

  /* run helper */
  const run = async (label: string, fn: () => Promise<void>) => {
    setError(null);
    setBusy(label);
    try { await fn(); } catch (e) { setError(errMsg(e)); } finally { setBusy(null); }
  };

  /* ── scan actions ── */
  const firstScan = () => {
    const done = startProgress();
    run("scan", async () => {
      if (pid == null) { done(); return; }
      const c = cmpOf(compare);
      const s = await invoke<ScanSummary>("scan_new", {
        pid, ty, compare,
        value: c.needsValue ? value : "",
        value2: c.needsValue2 ? value2 : null,
      });
      const newPrev: Record<number, string> = {};
      s.preview.forEach((r) => { newPrev[r.address] = r.value; });
      setPrevMap(currentScanMap);
      setCurrentScanMap(newPrev);
      setCurrentScanMap(newPrev);
      setSession(s.session);
      setSummary(s);
      setRows(s.preview);
      setUndoState(null);
      done();
    });
  };

  const nextScan = () => {
    const done = startProgress();
    run("refine", async () => {
      if (session == null) { done(); return; }
      const c = cmpOf(compare);
      setUndoState(summary ? { rows, summary } : null);
      const s = await invoke<ScanSummary>("scan_next", {
        session, compare,
        value: c.needsValue ? value : null,
        value2: c.needsValue2 ? value2 : null,
      });
      const newPrev: Record<number, string> = {};
      s.preview.forEach((r) => { newPrev[r.address] = r.value; });
      setPrevMap(newPrev);
      setSummary(s);
      setRows(s.preview);
      done();
    });
  };

  const undoScan = () => {
    if (!undoState) return;
    setRows(undoState.rows);
    setSummary(undoState.summary);
    setUndoState(null);
  };

  const resetScan = useCallback(() =>
    run("reset", async () => {
      if (session != null) { try { await invoke("scan_clear", { session }); } catch {} }
      setSession(null);
      setSummary(null);
      setRows([]);
      setPrevMap({});
      setCurrentScanMap({});
      setUndoState(null);
      setScanProgress(0);
      if (!cmpOf(compare).firstScan) setCompare("exact");
    }), [session, compare]);

  /* ── cheat table ── */
  const addAndEditFound = (addr: number, val: string) => {
    const newId = addToSaved(addr, val);
    if (newId) {
      setTimeout(() => {
        setEditingEntryId(newId);
        setEditValueText(val);
      }, 50);
    }
  };

  const addToSaved = (addr: number, val: string, label?: string, t?: MemValueType) => {
    if (pid == null) return;
    const id = `${pid}-${addr}-${Date.now()}`;
    setSaved((s) => [...s, { id, pid, address: addr, ty: t ?? ty, label: label ?? `Adres ${hx(addr)}`, value: val, frozen: false }]);
    return id;
  };

  const updateEntry = (id: string, patch: Partial<SavedEntry>) =>
    setSaved((s) => s.map((e) => (e.id === id ? { ...e, ...patch } : e)));

  const removeEntry = async (e: SavedEntry) => {
    if (e.frozen) { try { await invoke("freeze_remove", { pid: e.pid, address: e.address }); } catch {} }
    setSaved((s) => s.filter((x) => x.id !== e.id));
  };

  const writeEntry = (e: SavedEntry, newVal: string) =>
    run("write", async () => {
      await invoke("write_memory", { pid: e.pid, address: e.address, value: newVal, ty: e.ty });
      updateEntry(e.id, { value: newVal });
      if (e.frozen) {
        await invoke("freeze_add", { pid: e.pid, address: e.address, value: newVal, ty: e.ty, label: e.label || null });
      }
      setEditingEntryId(null);
    });

  const toggleFreeze = (e: SavedEntry) =>
    run("freeze", async () => {
      if (e.frozen) {
        await invoke("freeze_remove", { pid: e.pid, address: e.address });
        updateEntry(e.id, { frozen: false });
      } else {
        await invoke("freeze_add", { pid: e.pid, address: e.address, value: liveVals[e.id] ?? e.value, ty: e.ty, label: e.label || null });
        updateEntry(e.id, { frozen: true });
      }
    });

  const freezeSign = (e: SavedEntry, sign: 1 | -1) =>
    run("freeze", async () => {
      const raw = liveVals[e.id] ?? e.value;
      const n = parseFloat(raw);
      const v = String(sign * Math.abs(Number.isFinite(n) ? n : 0));
      await invoke("freeze_add", { pid: e.pid, address: e.address, value: v, ty: e.ty, label: e.label || null });
      updateEntry(e.id, { frozen: true });
    });

  const toggleEntryHex = (id: string) =>
    setEntryHexSet((prev) => { const next = new Set(prev); next.has(id) ? next.delete(id) : next.add(id); return next; });

  /* pointer scan */
  const runPointerScan = () =>
    run("ptr", async () => {
      if (pid == null) return;
      const target = parseInt(ptrTarget.trim().replace(/^0x/i, ""), 16);
      if (!Number.isFinite(target)) { setError("Geçerli bir hedef adres girin (Hex formatında)"); return; }
      const c = await invoke<PointerChain[]>("pointer_scan", { pid, target, maxOffset: parseInt(ptrOffset, 10) || 4096, maxDepth: parseInt(ptrDepth, 10) || 2 });
      setChains(c);
    });

  /* filtered procs */
  const filteredProcs = useMemo(() => {
    const q = procQuery.toLowerCase().trim();
    const SYS = ["kworker", "cpuhp", "rcu_", "migration", "ksoftirqd", "kthreadd", "kdevtmpfs", "netns", "khungtaskd"];
    return procs.filter((p) => {
      if (q && !p.name.toLowerCase().includes(q) && !String(p.pid).includes(q)) return false;
      if (procTab === "apps") {
        const n = p.name.toLowerCase();
        if (SYS.some((x) => n.startsWith(x))) return false;
        if (p.pid <= 2) return false;
      }
      return true;
    });
  }, [procs, procQuery, procTab]);

  const activeProc = procs.find((p) => p.pid === pid);
  const c = cmpOf(compare);
  const ptraceErr = error && isPtraceErr(error);
  const availableCompares = session == null ? COMPARES.filter((x) => x.firstScan) : COMPARES;

  /* ctx menu handlers */
  const openFoundCtx = (e: ReactMouseEvent, row: AddrValue) => {
    e.preventDefault(); e.stopPropagation();
    setCtxMenu({ x: e.clientX, y: e.clientY, kind: "found", row });
  };
  const openSavedCtx = (e: ReactMouseEvent, entry: SavedEntry) => {
    e.preventDefault(); e.stopPropagation();
    setCtxMenu({ x: e.clientX, y: e.clientY, kind: "saved", entry });
  };

  /* ── render ──────────────────────────────────────────────────────────────── */
  return (
    <div className="ms-root" onClick={() => { setCtxMenu(null); }}>

      {/* ── TOP BAR ── */}
      <div className="ms-topbar">
        <div className="ms-topbar-left">
          <button
            className={`ms-proc-btn${pid ? " ms-proc-btn--active" : ""}`}
            onClick={() => { loadProcs(); setShowProcModal(true); }}
          >
            <Monitor size={14} />
            <span>Süreç Seç</span>
            <ChevronDown size={12} />
          </button>
          {pid && activeProc ? (
            <div className="ms-proc-badge">
              <span className="ms-proc-dot" />
              <span className="ms-pid mono">{String(activeProc.pid).padStart(6, "0")}</span>
              <span className="ms-proc-sep">—</span>
              <span className="ms-pname">{activeProc.name}</span>
            </div>
          ) : (
            <span className="ms-proc-idle">Süreç seçilmedi</span>
          )}
        </div>
        <div className="ms-topbar-right">
          {scope && (
            <span className={`ms-scope-tag${scope.trim() === "0" ? " ms-scope-tag--ok" : " ms-scope-tag--warn"}`}>
              ptrace_scope: {scope.trim()}
            </span>
          )}
          <button
            className={`ms-pill-btn${hexDisplay ? " ms-pill-btn--on" : ""}`}
            onClick={() => setHexDisplay((v) => !v)}
            title="Bulunan değerleri hex formatında göster"
          >
            <Hash size={13} /> Hex
          </button>
          <button
            className={`ms-pill-btn${showPtr ? " ms-pill-btn--on" : ""}`}
            onClick={() => setShowPtr((v) => !v)}
            title="Pointer Scan paneli"
          >
            <Crosshair size={13} /> Pointer
          </button>
        </div>
      </div>

      {/* ── PROGRESS ── */}
      {scanProgress > 0 && (
        <div className="ms-prog-track">
          <div className="ms-prog-bar" style={{ width: `${scanProgress}%`, transition: scanProgress === 100 ? "width 0.15s" : "width 0.08s linear" }} />
        </div>
      )}

      {/* ── PTRACE WARNING ── */}
      {ptraceErr && (
        <div className="ms-ptrace-warn" onClick={(e) => e.stopPropagation()}>
          <AlertTriangle size={14} className="ms-warn-icon" />
          <span>Bellek erişimi engellendi — ptrace kısıtlı</span>
          <code className="ms-warn-cmd">echo 0 | sudo tee /proc/sys/kernel/yama/ptrace_scope</code>
          <button
            className="ms-warn-copy"
            onClick={() => navigator.clipboard.writeText("echo 0 | sudo tee /proc/sys/kernel/yama/ptrace_scope")}
          >
            <Copy size={11} /> Kopyala
          </button>
          <button className="ms-warn-close" onClick={() => setError(null)}>
            <X size={13} />
          </button>
        </div>
      )}

      {/* ── GENERIC ERROR ── */}
      {error && !ptraceErr && (
        <div className="ms-error-bar">
          <span>{error}</span>
          <button className="ms-warn-close" onClick={() => setError(null)}><X size={13} /></button>
        </div>
      )}

      {/* ── WORKSPACE ── */}
      <div className="ms-workspace">

        {/* LEFT — FOUND LIST */}
        <div className="ms-found-panel">
          <div className="ms-panel-header">
            <span className="ms-panel-label">Bulunan Adresler</span>
            <span className="ms-badge">
              {summary ? summary.total.toLocaleString("en") : "0"}
            </span>
          </div>

          <div className="ms-found-scroller">
            <table className="ms-found-table">
              <thead>
                <tr>
                  <th>Adres</th>
                  <th>Değer</th>
                  <th>Önceki</th>
                </tr>
              </thead>
              <tbody>
                {rows.length === 0 ? (
                  <tr>
                    <td colSpan={3} className="ms-table-empty">
                      {session == null ? "Tarama başlatılmadı" : "Eşleşen adres yok"}
                    </td>
                  </tr>
                ) : rows.map((r) => (
                  <tr
                    key={r.address}
                    className={selectedResultAddr === r.address ? "ms-found-row--sel" : "ms-found-row"}
                    onClick={() => setSelectedResultAddr(r.address)}
                    onDoubleClick={() => addAndEditFound(r.address, r.value)}
                    onContextMenu={(e) => openFoundCtx(e, r)}
                  >
                    <td className="mono ms-cell-addr">{hx(r.address)}</td>
                    <td className="mono ms-cell-val">{hexDisplay ? fmtHex(r.value) : r.value}</td>
                    <td className="mono ms-cell-prev">
                      {prevMap[r.address] != null
                        ? (hexDisplay ? fmtHex(prevMap[r.address]) : prevMap[r.address])
                        : <span className="ms-dash">—</span>
                      }
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          <div className="ms-found-footer">
            <button
              className="ms-drop-btn"
              disabled={selectedResultAddr == null}
              onClick={() => {
                const r = rows.find((x) => x.address === selectedResultAddr);
                if (r) addToSaved(r.address, r.value);
              }}
            >
              <ArrowDown size={12} /> Tabloya Ekle
            </button>
            {summary && (
              <span className="ms-found-total">
                {rows.length < summary.total
                  ? `${rows.length} / ${summary.total.toLocaleString("en")} gösteriliyor`
                  : `${summary.total.toLocaleString("en")} adres`}
              </span>
            )}
          </div>
        </div>

        {/* RIGHT — SCAN CONTROLS */}
        <div className="ms-scan-panel">
          {/* Action row (CE style top buttons) */}
          <div className="ms-scan-actions">
            {session == null ? (
              <button
                className="ms-btn-primary"
                disabled={busy != null || pid == null || (c.needsValue && !value.trim())}
                onClick={() => {
                  if (c.needsValue && (ty.includes("32") || ty.includes("64") || ty === "u8")) {
                    if (!/^-?[0-9]+$/.test(value)) { setError("Hata: " + ty + " taraması için sadece tam sayı girebilirsiniz (harf kullanılamaz)."); return; }
                    if (c.needsValue2 && !/^-?[0-9]+$/.test(value2)) { setError("Hata: İkinci değer de bir tam sayı olmalıdır."); return; }
                  }
                  firstScan();
                }}
              >
                <Search size={13} />
                {busy === "scan" ? "Taranıyor..." : "İlk Tarama"}
              </button>
            ) : (
              <>
                <button
                  className="ms-btn-primary"
                  disabled={busy != null || (c.needsValue && !value.trim())}
                  onClick={() => {
                    if (c.needsValue && (ty.includes("32") || ty.includes("64") || ty === "u8")) {
                      if (!/^-?[0-9]+$/.test(value)) { setError("Hata: " + ty + " taraması için sadece tam sayı girebilirsiniz (harf kullanılamaz)."); return; }
                      if (c.needsValue2 && !/^-?[0-9]+$/.test(value2)) { setError("Hata: İkinci değer de bir tam sayı olmalıdır."); return; }
                    }
                    nextScan();
                  }}
                >
                  <Filter size={13} />
                  {busy === "refine" ? "Daraltılıyor..." : "Sonraki Tarama"}
                </button>
                {undoState && (
                  <button className="ms-btn-secondary" onClick={undoScan} title="Son taramayı geri al">
                    <RotateCcw size={12} /> Geri Al
                  </button>
                )}
                <button className="ms-btn-secondary ms-btn-secondary--ghost" onClick={resetScan} disabled={busy != null} title="Taramayı sıfırla">
                  <X size={12} /> Sıfırla
                </button>
              </>
            )}
          </div>

          <div className="ms-scan-divider" />

          {/* Value inputs (Prominent) */}
          <div className="ms-field" style={{ marginBottom: 18 }}>
            <label className="ms-label" style={{ color: 'var(--accent)' }}>ARANACAK DEĞER</label>
            <input
              className="ms-input mono"
              style={{ fontSize: 16, padding: '10px 14px' }}
              disabled={!c.needsValue}
              placeholder={c.needsValue ? "Örn: 99 veya 100" : "Bu tarama türü için değer gerekmiyor"}
              value={value}
              onChange={(e) => { setValue(e.target.value); setError(null); }}
              onKeyDown={(e) => { if (e.key === "Enter") {
                if (c.needsValue && (ty.includes("32") || ty.includes("64") || ty === "u8") && !/^-?[0-9]+$/.test(value)) { setError("Hata: Geçersiz sayı."); return; }
                session == null ? firstScan() : nextScan(); 
              }}}
            />
          </div>

          {c.needsValue2 && (
            <div className="ms-field" style={{ marginBottom: 18 }}>
              <label className="ms-label" style={{ color: 'var(--accent)' }}>İKİNCİ DEĞER (ÜST SINIR)</label>
              <input
                className="ms-input mono"
                style={{ fontSize: 16, padding: '10px 14px' }}
                placeholder="Örn: 500"
                value={value2}
                onChange={(e) => { setValue2(e.target.value); setError(null); }}
              />
            </div>
          )}

          {/* Scan type */}
          <div className="ms-field">
            <label className="ms-label">TARAMA TÜRÜ (Scan Type)</label>
            <div className="ms-select-wrapper">
              <select
                className="ms-select"
                value={compare}
                onChange={(e) => setCompare(e.target.value as Compare)}
              >
                {availableCompares.map((x) => (
                  <option key={x.id} value={x.id}>{x.label}</option>
                ))}
              </select>
              <ChevronDown className="ms-select-icon" size={14} />
            </div>
          </div>

          {/* Value type */}
          <div className="ms-field">
            <label className="ms-label">VERİ TÜRÜ (Value Type)</label>
            <div className="ms-select-wrapper">
              <select
                className="ms-select"
                value={ty}
                onChange={(e) => setTy(e.target.value as MemValueType)}
                disabled={session != null}
              >
                {TYPES.map((t) => (
                  <option key={t.id} value={t.id}>{t.label}</option>
                ))}
              </select>
              <ChevronDown className="ms-select-icon" size={14} />
            </div>
          </div>

          {/* Scan summary */}
          {summary && (
            <div className="ms-scan-summary">
              <span className="ms-summary-count">{summary.total.toLocaleString("tr-TR")}</span>
              <span className="ms-summary-label">adres bulundu</span>
              {session != null && <span className="ms-session-pill">Oturum #{session}</span>}
            </div>
          )}
        </div>
      </div>

      {/* ── POINTER SCAN DRAWER ── */}
      {showPtr && (
        <div className="ms-ptr-drawer">
          <div className="ms-ptr-header">
            <span className="ms-panel-label">Pointer Zinciri Tarayıcısı</span>
            <button className="ms-pill-btn" onClick={() => setShowPtr(false)}><X size={13} /></button>
          </div>
          <div className="ms-ptr-row-form">
            <input className="ms-input mono" style={{ flex: "2 1 0" }} placeholder="Hedef adres (0x...)" value={ptrTarget} onChange={(e) => setPtrTarget(e.target.value)} />
            <input className="ms-input mono" style={{ flex: "1 1 0" }} placeholder="Max ofset" value={ptrOffset} onChange={(e) => setPtrOffset(e.target.value)} />
            <select className="ms-select" style={{ flex: "1 1 0" }} value={ptrDepth} onChange={(e) => setPtrDepth(e.target.value)}>
              {[1, 2, 3, 4].map((d) => <option key={d} value={String(d)}>Derinlik {d}</option>)}
            </select>
            <button className="ms-btn-primary" onClick={runPointerScan} disabled={busy != null || pid == null || !ptrTarget.trim()}>
              {busy === "ptr" ? "Taranıyor..." : "Tara"}
            </button>
          </div>
          {chains && (
            <div className="ms-ptr-results">
              {chains.length === 0
                ? <span className="ms-table-empty" style={{ padding: "12px 0" }}>Pointer zinciri bulunamadı</span>
                : chains.map((ch, i) => (
                  <div key={i} className="ms-ptr-chain mono">
                    <b className="ms-ptr-base">{ch.base_module}</b>
                    <span className="ms-ptr-off">+{hx(ch.base_offset)}</span>
                    {ch.offsets.map((o, j) => (
                      <span key={j} className="ms-ptr-arrow"> → +{hx(o)}</span>
                    ))}
                  </div>
                ))
              }
            </div>
          )}
        </div>
      )}

      {/* ── CHEAT TABLE ── */}
      <div className="ms-resizer" onMouseDown={(e) => { e.preventDefault(); dragRef.current = true; document.body.style.cursor = 'row-resize'; }}>
        <div className="ms-resizer-line" />
      </div>
      <div className="ms-table-panel" style={{ height: tableHeight, maxHeight: tableHeight, flex: `0 0 ${tableHeight}px` }}>
        <div className="ms-panel-header">
          <span className="ms-panel-label">Cheat Table</span>
          <span className="ms-badge">{saved.length}</span>
          <div className="ms-table-header-actions">
            <button
              className="ms-btn-small"
              disabled={pid == null}
              onClick={() => setShowManualModal(true)}
            >
              <Plus size={12} /> Manuel Ekle
            </button>
            {saved.length > 0 && (
              <button
                className="ms-btn-small ms-btn-small--danger"
                onClick={() => setSaved([])}
              >
                <Trash2 size={11} />
              </button>
            )}
          </div>
        </div>

        <div className="ms-ct-scroller">
          <table className="ms-ct-table">
            <thead>
              <tr>
                <th style={{ width: 44 }}>Sabitle</th>
                <th>Açıklama</th>
                <th style={{ width: "20%" }}>Adres</th>
                <th style={{ width: "9%" }}>Tür</th>
                <th style={{ width: "22%" }}>Değer (Canlı)</th>
                <th style={{ width: 36 }}></th>
              </tr>
            </thead>
            <tbody>
              {saved.length === 0 ? (
                <tr>
                  <td colSpan={6} className="ms-table-empty">
                    Tarama sonuçlarına çift tıklayın veya manuel ekleyin
                  </td>
                </tr>
              ) : saved.map((e) => (
                <tr
                  key={e.id}
                  className={`ms-ct-row${e.frozen ? " ms-ct-row--frozen" : ""}`}
                  onContextMenu={(ev) => openSavedCtx(ev, e)}
                >
                  <td className="ms-ct-center">
                    <button
                      className={`ms-freeze-btn${e.frozen ? " ms-freeze-btn--on" : ""}`}
                      onClick={() => toggleFreeze(e)}
                      title={e.frozen ? "Çöz" : "Sabitle (Freeze)"}
                    >
                      <Snowflake size={12} />
                    </button>
                  </td>
                  <td>
                    <input
                      className="ms-cell-input"
                      value={e.label}
                      onChange={(ev) => updateEntry(e.id, { label: ev.target.value })}
                    />
                  </td>
                  <td className="mono ms-cell-addr">{hx(e.address)}</td>
                  <td className="ms-cell-type">{e.ty}</td>
                  <td>
                    {editingEntryId === e.id ? (
                      <div className="ms-inline-edit">
                        <input
                          autoFocus
                          className="ms-cell-input-edit mono"
                          value={editValueText}
                          onChange={(ev) => setEditValueText(ev.target.value)}
                          onKeyDown={(ev) => {
                            if (ev.key === "Enter") writeEntry(e, editValueText);
                            if (ev.key === "Escape") setEditingEntryId(null);
                          }}
                        />
                        <button className="ms-save-btn" onClick={() => writeEntry(e, editValueText)}>
                          <Check size={11} />
                        </button>
                      </div>
                    ) : (
                      <div
                        className="ms-live-val"
                        onClick={() => { setEditingEntryId(e.id); setEditValueText(liveVals[e.id] ?? e.value); }}
                        title="Tıkla: değeri düzenle"
                      >
                        <span className="mono">
                          {entryHexSet.has(e.id)
                            ? fmtHex(liveVals[e.id] ?? e.value)
                            : (liveVals[e.id] ?? e.value)}
                        </span>
                        <Edit2 size={10} className="ms-edit-icon" />
                      </div>
                    )}
                  </td>
                  <td className="ms-ct-center">
                    <button className="ms-del-btn" onClick={() => removeEntry(e)} title="Sil">
                      <X size={11} />
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>

      {/* ── CONTEXT MENU ── */}
      {ctxMenu && (
        <div
          className="ms-ctx"
          style={{ left: Math.min(ctxMenu.x, window.innerWidth - 220), top: Math.min(ctxMenu.y, window.innerHeight - 200) }}
          onClick={(e) => e.stopPropagation()}
        >
          {ctxMenu.kind === "found" && ctxMenu.row && (() => {
            const r = ctxMenu.row;
            return (
              <>
                <button className="ms-ctx-item" onClick={() => { addAndEditFound(r.address, r.value); setCtxMenu(null); }}>
                  <Edit2 size={12} style={{ marginRight: 6 }} /> Değeri Değiştir
                </button>
                <button className="ms-ctx-item" onClick={() => { addToSaved(r.address, r.value); setCtxMenu(null); }}>
                  Tabloya Ekle
                </button>
                <button className="ms-ctx-item" onClick={() => { navigator.clipboard.writeText(hx(r.address)); setCtxMenu(null); }}>
                  Adresi Kopyala
                </button>
                {prevMap[r.address] != null && <>
                  <div className="ms-ctx-sep" />
                  <button className="ms-ctx-item" onClick={() => {
                    if (pid != null) invoke("write_memory", { pid, address: r.address, value: prevMap[r.address], ty }).catch(() => {});
                    setCtxMenu(null);
                  }}>
                    Önceki Değere Döndür ({prevMap[r.address]})
                  </button>
                </>}
                <div className="ms-ctx-sep" />
                <button className="ms-ctx-item ms-ctx-item--danger" onClick={() => {
                  setRows((prev) => prev.filter((x) => x.address !== r.address));
                  setCtxMenu(null);
                }}>
                  Listeden Çıkar
                </button>
              </>
            );
          })()}

          {ctxMenu.kind === "saved" && ctxMenu.entry && (() => {
            const e = ctxMenu.entry;
            return (
              <>
                <button className="ms-ctx-item" onClick={() => { setEditingEntryId(e.id); setEditValueText(liveVals[e.id] ?? e.value); setCtxMenu(null); }}>
                  <Edit2 size={12} style={{ marginRight: 6 }} /> Değeri Değiştir
                </button>
                <button className="ms-ctx-item" onClick={() => { toggleEntryHex(e.id); setCtxMenu(null); }}>
                  {entryHexSet.has(e.id) ? "Ondalık Göster" : "Hex Göster"}
                </button>
                <div className="ms-ctx-sep" />
                <button className="ms-ctx-item" onClick={() => { freezeSign(e, 1); setCtxMenu(null); }}>
                  Freeze + (pozitif sabitle)
                </button>
                <button className="ms-ctx-item" onClick={() => { freezeSign(e, -1); setCtxMenu(null); }}>
                  Freeze - (negatif sabitle)
                </button>
                <div className="ms-ctx-sep" />
                <button className="ms-ctx-item" onClick={() => { navigator.clipboard.writeText(hx(e.address)); setCtxMenu(null); }}>
                  Adresi Kopyala
                </button>
                <button className="ms-ctx-item ms-ctx-item--danger" onClick={() => { removeEntry(e); setCtxMenu(null); }}>
                  Sil
                </button>
              </>
            );
          })()}
        </div>
      )}

      {/* ── PROCESS MODAL ── */}
      {showProcModal && (
        <div className="ms-overlay" onClick={() => setShowProcModal(false)}>
          <div className="ms-modal" onClick={(e) => e.stopPropagation()}>
            <div className="ms-modal-head">
              <div className="ms-modal-title">
                <Monitor size={15} /> Süreç Listesi
              </div>
              <button className="ms-pill-btn" onClick={() => setShowProcModal(false)}><X size={14} /></button>
            </div>

            <div className="ms-proc-tabs">
              <button className={`ms-tab${procTab === "apps" ? " ms-tab--active" : ""}`} onClick={() => setProcTab("apps")}>
                Uygulamalar
              </button>
              <button className={`ms-tab${procTab === "all" ? " ms-tab--active" : ""}`} onClick={() => setProcTab("all")}>
                Tüm Süreçler
              </button>
              <button className="ms-pill-btn" style={{ marginLeft: "auto" }} onClick={loadProcs} title="Yenile">
                <RefreshCw size={13} />
              </button>
            </div>

            <div className="ms-proc-search">
              <Search size={13} className="ms-search-icon" />
              <input
                autoFocus
                className="ms-search-input"
                placeholder="Süreç adı veya PID..."
                value={procQuery}
                onChange={(e) => setProcQuery(e.target.value)}
              />
            </div>

            <div className="ms-proc-list">
              {filteredProcs.length === 0
                ? <div className="ms-table-empty" style={{ padding: 28 }}>Eşleşen süreç bulunamadı</div>
                : filteredProcs.map((p) => (
                  <div
                    key={p.pid}
                    className={`ms-proc-row${pid === p.pid ? " ms-proc-row--sel" : ""}`}
                    onClick={() => { setPid(p.pid); resetScan(); setShowProcModal(false); setProcQuery(""); }}
                  >
                    <span className="mono ms-pid">{String(p.pid).padStart(6, "0")}</span>
                    {p.icon ? <img src={p.icon} className="ms-proc-icon" style={{ width: 14, height: 14, marginTop: 2, objectFit: "contain" }} /> : <AppWindow size={14} className="ms-proc-icon" style={{ opacity: 0.6, marginTop: 2 }} />}
                    <span className="ms-pname">{p.name}</span>
                  </div>
                ))
              }
            </div>

            <div className="ms-modal-foot">
              <span className="ms-muted">{filteredProcs.length} süreç listelendi</span>
              <button className="ms-btn-secondary" onClick={() => setShowProcModal(false)}>Kapat</button>
            </div>
          </div>
        </div>
      )}

      {/* ── MANUAL ADD MODAL ── */}
      {showManualModal && (
        <div className="ms-overlay" onClick={() => setShowManualModal(false)}>
          <div className="ms-modal ms-modal--sm" onClick={(e) => e.stopPropagation()}>
            <div className="ms-modal-head">
              <span className="ms-modal-title">Manuel Adres Ekle</span>
              <button className="ms-pill-btn" onClick={() => setShowManualModal(false)}><X size={14} /></button>
            </div>
            <div className="ms-modal-body">
              <div className="ms-field">
                <label className="ms-label">Açıklama</label>
                <input className="ms-input" value={manualDesc} onChange={(e) => setManualDesc(e.target.value)} />
              </div>
              <div className="ms-field">
                <label className="ms-label">Adres (Hex)</label>
                <input
                  autoFocus
                  className="ms-input mono"
                  placeholder="0x14023A10"
                  value={manualAddr}
                  onChange={(e) => setManualAddr(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && (() => {
                    const parsed = parseInt(manualAddr.trim().replace(/^0x/i, ""), 16);
                    if (Number.isFinite(parsed)) { addToSaved(parsed, "0", manualDesc, manualType); setShowManualModal(false); setManualAddr(""); }
                    else setError("Geçerli bir hex adresi girin");
                  })()}
                />
              </div>
              <div className="ms-field">
                <label className="ms-label">Veri Türü</label>
                <select className="ms-select" value={manualType} onChange={(e) => setManualType(e.target.value as MemValueType)}>
                  {TYPES.map((t) => <option key={t.id} value={t.id}>{t.label}</option>)}
                </select>
              </div>
            </div>
            <div className="ms-modal-foot">
              <button className="ms-btn-secondary" onClick={() => setShowManualModal(false)}>İptal</button>
              <button
                className="ms-btn-primary"
                onClick={() => {
                  const parsed = parseInt(manualAddr.trim().replace(/^0x/i, ""), 16);
                  if (!Number.isFinite(parsed)) { setError("Geçerli bir hex adresi girin"); return; }
                  addToSaved(parsed, "0", manualDesc, manualType);
                  setShowManualModal(false);
                  setManualAddr("");
                }}
              >
                Tabloya Ekle
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
