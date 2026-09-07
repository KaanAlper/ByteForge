import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChevronLeft, ChevronRight } from "lucide-react";
import type { HexChunk, ApiError } from "../types";

const PAGE = 512;
const COLS = 16;

const errMsg = (e: unknown) => (e as Partial<ApiError>)?.message ?? String(e);

export function HexEditor({ path }: { path: string }) {
  const [offset, setOffset] = useState(0);
  const [chunk, setChunk] = useState<HexChunk | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [gotoInput, setGotoInput] = useState("");
  const [pattern, setPattern] = useState("");
  const [matches, setMatches] = useState<number[] | null>(null);
  const [sel, setSel] = useState<number | null>(null);
  const [editHex, setEditHex] = useState("");
  const [status, setStatus] = useState<string | null>(null);

  const load = useCallback(
    async (off: number) => {
      setError(null);
      try {
        const c = await invoke<HexChunk>("read_hex", { path, offset: off, len: PAGE });
        setChunk(c);
        setOffset(off);
      } catch (e) {
        setError(errMsg(e));
      }
    },
    [path]
  );

  useEffect(() => {
    load(0);
  }, [load]);

  const total = chunk?.total ?? 0;

  const gotoOffset = () => {
    const v = parseInt(gotoInput.replace(/^0x/i, ""), 16);
    if (!Number.isNaN(v)) load(Math.max(0, v - (v % COLS)));
  };

  const search = async () => {
    setError(null);
    setStatus(null);
    try {
      const m = await invoke<number[]>("search_hex", { path, pattern });
      setMatches(m);
      if (m.length > 0) load(m[0] - (m[0] % COLS));
    } catch (e) {
      setError(errMsg(e));
    }
  };

  const write = async () => {
    if (sel == null) return;
    if (!window.confirm(`Ofset 0x${sel.toString(16)} yazılacak (.bak yedekli). Devam?`)) return;
    try {
      const bak = await invoke<string>("write_hex", { path, offset: sel, hex: editHex });
      setStatus(`Yazıldı → ${bak}`);
      load(offset);
    } catch (e) {
      setError(errMsg(e));
    }
  };

  const rows: { off: number; bytes: number[] }[] = [];
  if (chunk) {
    for (let r = 0; r < chunk.bytes.length; r += COLS) {
      rows.push({ off: offset + r, bytes: chunk.bytes.slice(r, r + COLS) });
    }
  }

  return (
    <div className="hex-editor">
      <div className="hex-toolbar">
        <span className="muted mono">{total.toLocaleString()} bayt</span>
        <div className="inline-field">
          <input
            placeholder="offset (hex)"
            value={gotoInput}
            onChange={(e) => setGotoInput(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && gotoOffset()}
          />
          <button onClick={gotoOffset}>Git</button>
        </div>
        <div className="inline-field grow">
          <input
            placeholder="hex ara — ör. 1F 20 03 D5"
            value={pattern}
            onChange={(e) => setPattern(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && search()}
          />
          <button onClick={search}>Ara</button>
        </div>
      </div>

      {matches && (
        <p className="muted">
          {matches.length} eşleşme
          {matches.length > 0 ? ` — ilk 0x${matches[0].toString(16)}` : ""}
        </p>
      )}
      {error && <p className="error">Hata: {error}</p>}

      <div className="hex-grid mono">
        {rows.map((row) => (
          <div className="hex-row" key={row.off}>
            <span className="hex-off">{row.off.toString(16).padStart(8, "0")}</span>
            <span className="hex-bytes">
              {row.bytes.map((b, i) => {
                const abs = row.off + i;
                return (
                  <span
                    key={i}
                    className={`hex-b ${sel === abs ? "sel" : ""}`}
                    onClick={() => {
                      setSel(abs);
                      setEditHex(b.toString(16).padStart(2, "0"));
                    }}
                  >
                    {b.toString(16).padStart(2, "0")}
                  </span>
                );
              })}
            </span>
            <span className="hex-ascii">
              {row.bytes.map((b, i) => (
                <span key={i}>{b >= 32 && b < 127 ? String.fromCharCode(b) : "·"}</span>
              ))}
            </span>
          </div>
        ))}
      </div>

      <div className="hex-pager">
        <button disabled={offset <= 0} onClick={() => load(Math.max(0, offset - PAGE))}>
          <ChevronLeft size={14} /> önceki
        </button>
        <span className="muted mono">0x{offset.toString(16)}</span>
        <button disabled={offset + PAGE >= total} onClick={() => load(offset + PAGE)}>
          sonraki <ChevronRight size={14} />
        </button>
      </div>

      {sel != null && (
        <div className="hex-edit">
          <span className="field-label">Ofset 0x{sel.toString(16)}</span>
          <input
            value={editHex}
            onChange={(e) => setEditHex(e.target.value)}
            placeholder="hex baytlar (ör. 1F 20 03 D5)"
          />
          <button onClick={write}>Yaz (.bak yedekli)</button>
        </div>
      )}
      {status && <p className="patch-ok">{status}</p>}
    </div>
  );
}
