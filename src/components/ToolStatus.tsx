import type { ToolReport } from "../types";

export function ToolStatusPanel({ report }: { report: ToolReport }) {
  return (
    <div className="tool-status">
      <h3>Araç Durumu</h3>
      <ul>
        {report.tools.map((t) => (
          <li key={t.name} className={t.available ? "ok" : "missing"}>
            <span className="dot">{t.available ? "●" : "○"}</span>
            <b>{t.name}</b>
            <span className="detail">{t.detail}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}
