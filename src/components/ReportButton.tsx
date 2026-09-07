import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FileText, Check } from "lucide-react";
import type { ApiError } from "../types";

const errMsg = (e: unknown) => (e as Partial<ApiError>)?.message ?? String(e);

/** Analiz raporunu (Markdown) üretip panoya kopyalar. */
export function ReportButton({ command, path }: { command: string; path: string }) {
  const [copied, setCopied] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = async () => {
    setBusy(true);
    setError(null);
    try {
      const md = await invoke<string>(command, { path });
      await navigator.clipboard.writeText(md);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch (e) {
      setError(errMsg(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <button className="report-btn" onClick={run} disabled={busy} title="Markdown raporu panoya kopyala">
        {copied ? <Check size={14} /> : <FileText size={14} />}
        {copied ? "kopyalandı" : busy ? "…" : "Rapor (MD)"}
      </button>
      {error && <span className="error" style={{ fontSize: 12 }}>{error}</span>}
    </>
  );
}
