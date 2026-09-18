import { createContext, useContext, useMemo, useState, type ReactNode } from "react";
import { tr } from "./tr";
import { en } from "./en";

export type Lang = "tr" | "en";
export type Key = keyof typeof tr;
export type Dict = { [K in Key]: string };

const DICTS: Record<Lang, Dict> = { tr, en };
export const LANG_KEY = "byteforge.lang";
export const LANGS: { id: Lang; label: string }[] = [
  { id: "tr", label: "Türkçe" },
  { id: "en", label: "English" },
];

function detectLang(): Lang {
  try {
    const saved = localStorage.getItem(LANG_KEY);
    if (saved === "tr" || saved === "en") return saved;
  } catch {
    /* localStorage yoksa */
  }
  return (navigator.language || "").toLowerCase().startsWith("tr") ? "tr" : "en";
}

interface Ctx {
  lang: Lang;
  setLang: (l: Lang) => void;
  t: (k: Key, vars?: Record<string, string | number>) => string;
}

const I18nCtx = createContext<Ctx | null>(null);

/** Uygulama geneli dil sağlayıcısı. Dil tercihi localStorage'da kalıcıdır. */
export function I18nProvider({ children }: { children: ReactNode }) {
  const [lang, setLangState] = useState<Lang>(detectLang);
  const value = useMemo<Ctx>(() => {
    const dict = DICTS[lang];
    return {
      lang,
      setLang: (l) => {
        setLangState(l);
        try {
          localStorage.setItem(LANG_KEY, l);
        } catch {
          /* yoksay */
        }
      },
      t: (k, vars) => {
        let s: string = dict[k] ?? (tr as Dict)[k] ?? String(k);
        if (vars) for (const [name, v] of Object.entries(vars)) s = s.split(`{${name}}`).join(String(v));
        return s;
      },
    };
  }, [lang]);
  return <I18nCtx.Provider value={value}>{children}</I18nCtx.Provider>;
}

/** `t("key")` — çeviri; `{var}` yer tutucuları desteklenir. */
export function useT() {
  const ctx = useContext(I18nCtx);
  if (!ctx) throw new Error("useT must be used within I18nProvider");
  return ctx;
}
