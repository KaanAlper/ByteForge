/** Sezgisel hedef kategorileri — etiket + kısa açıklama. */
export const CAT_LABEL: Record<string, string> = {
  character: "Karakter / Skin",
  economy: "Para / Ekonomi",
  purchase_vip: "Satın Alma / VIP",
  ads: "Reklam",
  reward: "Ödül",
  other: "Diğer",
};

/** Kategori sırası (filtre çubuğu için). */
export const CAT_ORDER = ["character", "economy", "purchase_vip", "ads", "reward", "other"];

/** Önerilen yama şablonu → kısa etiket. */
export const TEMPLATE_LABEL: Record<string, string> = {
  return_true: "True",
  return_false: "False",
  return_max_int: "MAX_INT",
  return_one_float: "1.0 Float",
  nop: "NOP",
  ret: "RET",
};

/** Kategoriye göre sayımları hesaplar. */
export function countByCategory<T extends { category: string }>(items: T[]): Record<string, number> {
  const out: Record<string, number> = {};
  for (const it of items) out[it.category] = (out[it.category] ?? 0) + 1;
  return out;
}
