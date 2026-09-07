import { useState, useEffect, useRef, useMemo } from "react";
import { Lightbulb } from "lucide-react";
import type { SoSymbol } from "../types";
import { PATCH_LABEL } from "../patchInfo";

const MAX_ROWS = 500;

export function SymbolTable({
  symbols,
  onSelect,
  selected,
  initialQuery = "",
}: {
  symbols: SoSymbol[];
  onSelect: (s: SoSymbol) => void;
  selected: SoSymbol | null;
  initialQuery?: string;
}) {
  const [query, setQuery] = useState(initialQuery);
  const [onlyFunctions, setOnlyFunctions] = useState(true);
  const [cursor, setCursor] = useState(0);
  const rowRefs = useRef<(HTMLTableRowElement | null)[]>([]);

  // Dışarıdan (ör. IL2CPP sembol tıklaması) gelen arama sorgusunu uygula.
  useEffect(() => {
    if (initialQuery) setQuery(initialQuery);
  }, [initialQuery]);

  const q = query.toLowerCase();
  const filtered = useMemo(
    () =>
      symbols
        .filter((s) => (!onlyFunctions || s.is_function) && s.name.toLowerCase().includes(q))
        .slice(0, MAX_ROWS),
    [symbols, onlyFunctions, q]
  );

  // Filtre değişince imleci başa al.
  useEffect(() => {
    setCursor(0);
  }, [q, onlyFunctions]);

  // Seçili satırı görünür alana kaydır.
  useEffect(() => {
    rowRefs.current[cursor]?.scrollIntoView({ block: "nearest" });
  }, [cursor]);

  const patchedCount = filtered.filter((s) => s.patched).length;

  const onKeyDown = (e: React.KeyboardEvent<HTMLTableSectionElement>) => {
    if (filtered.length === 0) return;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      const next = Math.min(cursor + 1, filtered.length - 1);
      setCursor(next);
      onSelect(filtered[next]); // seçili satır otomatik aktif
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      const next = Math.max(cursor - 1, 0);
      setCursor(next);
      onSelect(filtered[next]);
    } else if (e.key === "Enter") {
      e.preventDefault();
      onSelect(filtered[cursor]); // detay/patch panelini aç
    }
  };

  return (
    <div className="profile-card">
      <div className="profile-head">
        <h2>Sembol Tablosu</h2>
        <span className="badge">
          {symbols.length} sembol
          {patchedCount > 0 && (
            <span className="badge-modded-inline" title="İlk baytları bilinen bir yama kalıbıyla birebir aynı olan fonksiyonlar. Modlanmış OLABİLİR ya da zaten öyle döndüren minik fonksiyonlar (return true gibi) olabilir.">
              {" "}· {patchedCount} kalıp eşleşmesi
            </span>
          )}
        </span>
      </div>

      <div className="sym-controls">
        <input
          type="text"
          placeholder="İsim ara…  (↓/↑ gez, Enter seç)"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <label>
          <input
            type="checkbox"
            checked={onlyFunctions}
            onChange={(e) => setOnlyFunctions(e.target.checked)}
          />
          yalnızca fonksiyonlar
        </label>
      </div>

      <div className="quick-filters">
        {(
          [
            ["VIP/Pro", "premium"],
            ["Lisans", "license"],
            ["Kilit", "unlock"],
            ["Satın alma", "purchase"],
            ["Reklam", "ad"],
            ["Ödül", "reward"],
            ["Ekonomi", "coin"],
            ["is/has/check", "is"],
          ] as const
        ).map(([label, qv]) => (
          <button
            key={qv}
            className={query === qv ? "active" : ""}
            onClick={() => setQuery(qv)}
            title={`"${qv}" içeren sembolleri süz`}
          >
            {label}
          </button>
        ))}
        {query && (
          <button className="clear" onClick={() => setQuery("")}>
            temizle
          </button>
        )}
      </div>

      <div className="sym-table-wrap">
        <table className="sym-table">
          <thead>
            <tr>
              <th>Ad</th>
              <th>RVA</th>
              <th>Dosya ofseti</th>
              <th>Boyut</th>
            </tr>
          </thead>
          <tbody tabIndex={0} onKeyDown={onKeyDown} className="sym-tbody">
            {filtered.map((s, i) => (
              <tr
                key={`${s.name}@${s.rva}#${i}`}
                ref={(el) => {
                  rowRefs.current[i] = el;
                }}
                className={`${selected === s ? "sel" : ""} ${i === cursor ? "cursor" : ""}`}
                onClick={() => {
                  setCursor(i);
                  onSelect(s);
                }}
              >
                <td className="mono">
                  {s.patched && (
                    <span
                      className="badge-modded"
                      title={`İlk baytlar "${PATCH_LABEL[s.patched] ?? s.patched}" kalıbında — elle yamalandıysa VEYA fonksiyon zaten böyle döndürüyorsa.`}
                    >
                      = {PATCH_LABEL[s.patched] ?? s.patched}
                    </span>
                  )}
                  {s.name}
                </td>
                <td className="mono">0x{s.rva.toString(16)}</td>
                <td className="mono">
                  {s.file_offset != null ? `0x${s.file_offset.toString(16)}` : "—"}
                </td>
                <td className="mono">{s.size}</td>
              </tr>
            ))}
          </tbody>
        </table>
        {symbols.filter((s) => (!onlyFunctions || s.is_function) && s.name.toLowerCase().includes(q))
          .length > MAX_ROWS && (
          <p className="muted">İlk {MAX_ROWS} gösteriliyor. Aramayı daraltın.</p>
        )}
        {filtered.length === 0 && query && (
          <div className="error-hint" style={{ marginTop: 10 }}>
            <Lightbulb size={14} /> <b>"{query}"</b> bu tabloda yok. Bu ELF sembol tablosu yalnızca
            ~2600 C fonksiyonu içerir (libc + il2cpp API) — oyunun C# metodları burada DEĞİLDİR.
            <br />
            Oyun metodunu adıyla bulup RVA'sını otomatik almak için{" "}
            <b>Profil → IL2CPP → "Tüm Semboller (ara)"</b> sekmesini kullanın; adres çözücü
            metod adını doğrudan yamalanacak adrese çevirir (harici dumper gerekmez).
          </div>
        )}
        {filtered.length === 0 && !query && <p className="muted">Eşleşen sembol yok.</p>}
      </div>
    </div>
  );
}
