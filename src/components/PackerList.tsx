import { ShieldAlert, ShieldCheck } from "lucide-react";
import type { PackerHit } from "../types";

/**
 * Tespit edilen koruyucu/paketleyicileri gösterir. Boşsa "temiz görünüyor"
 * durumunu bildirir (kesin kanıt değil — imza tabanlı sezgisel tespit).
 */
export function PackerList({ packers }: { packers: PackerHit[] }) {
  if (packers.length === 0) {
    return (
      <div className="packer-box clean">
        <ShieldCheck size={16} />
        <span>Bilinen bir paketleyici/koruyucu imzası bulunamadı.</span>
      </div>
    );
  }
  return (
    <div className="packer-box">
      <div className="packer-head">
        <ShieldAlert size={16} />
        <span>{packers.length} koruyucu/paketleyici tespit edildi</span>
      </div>
      <div className="packer-rows">
        {packers.map((p, i) => (
          <div key={i} className="packer-row">
            <span className="packer-name">{p.name}</span>
            <span className={`packer-conf ${p.confidence === "kesin" ? "sure" : "maybe"}`}>
              {p.confidence}
            </span>
            <span className="packer-vendor muted">{p.vendor}</span>
            <code className="packer-ev mono muted">{p.evidence}</code>
          </div>
        ))}
      </div>
    </div>
  );
}
