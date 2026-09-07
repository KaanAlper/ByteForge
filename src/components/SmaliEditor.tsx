import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ApiError } from "../types";

const errMsg = (e: unknown) => (e as Partial<ApiError>)?.message ?? String(e);

function lineClass(line: string): string {
  const t = line.trim();
  if (t.startsWith("#")) return "sm-comment";
  if (t.startsWith(".")) return "sm-directive";
  if (
    /^(return|invoke|const|new-|move|if-|goto|iget|iput|sget|sput|check-cast|instance-of|throw)/.test(
      t
    )
  )
    return "sm-op";
  return "";
}

export function SmaliEditor({ outDir, file }: { outDir: string; file: string }) {
  const [content, setContent] = useState<string | null>(null);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    setStatus(null);
    setEditing(false);
    try {
      const c = await invoke<string>("read_smali_file", { outDir, file });
      setContent(c);
      setDraft(c);
    } catch (e) {
      setError(errMsg(e));
    }
  }, [outDir, file]);

  useEffect(() => {
    load();
  }, [load]);

  const save = async () => {
    try {
      await invoke("write_smali_file", { outDir, file, content: draft });
      setContent(draft);
      setEditing(false);
      setStatus("Kaydedildi");
    } catch (e) {
      setError(errMsg(e));
    }
  };

  if (content == null) return <p className="muted">yükleniyor…</p>;

  return (
    <div className="smali-editor">
      <div className="wb-header">
        <span className="mono muted">{file}</span>
        <div className="wb-actions">
          {editing ? (
            <>
              <button
                onClick={() => {
                  setDraft(content);
                  setEditing(false);
                }}
              >
                iptal
              </button>
              <button className="primary-btn" onClick={save}>
                kaydet
              </button>
            </>
          ) : (
            <button onClick={() => setEditing(true)}>düzenle</button>
          )}
        </div>
      </div>
      {status && <p className="patch-ok">{status}</p>}
      {error && <p className="error">Hata: {error}</p>}

      {editing ? (
        <textarea
          className="smali-textarea mono"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          spellCheck={false}
        />
      ) : (
        <div className="smali-code mono">
          {content.split("\n").map((l, i) => (
            <div key={i} className={lineClass(l)}>
              <span className="sm-ln">{i + 1}</span>
              {l || " "}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
