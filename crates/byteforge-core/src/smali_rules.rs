//! Smali "hızlı kural" tarayıcısı — saf Rust, sezgisel kalıp eşleştirme.
//!
//! Decode edilmiş smali içinde para-kazanç / koruma kalıplarını bulur:
//! premium/kilit boolean getter'ları (doğrudan `ForcedReturn::True` ile
//! zorlanabilir), imza doğrulama, faturalandırma, root/emülatör tespiti,
//! lisans kontrolü. Otomatik yama yapmaz — aday listesi üretir.

use crate::smali::find_methods;
use serde::Serialize;

/// Bir kural eşleşmesi (tek satır ya da tek metod).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RuleHit {
    /// Kural adı (insan-okur).
    pub rule: String,
    /// Kategori: premium / signature / billing / root / license.
    pub category: String,
    /// İlgili metod imzası (yalnızca metod-düzeyi kurallarda).
    pub method: Option<String>,
    /// 1-tabanlı satır numarası.
    pub line: usize,
    /// Eşleşen satırın kırpılmış içeriği.
    pub snippet: String,
    /// Önerilen aksiyon (insan-okur).
    pub suggestion: String,
}

/// Premium/kilit çağrıştıran boolean getter adı mı? (küçük harf karşılaştırma)
fn is_premium_bool_name(sig_lower: &str) -> bool {
    const KEYS: &[&str] = &[
        "premium",
        "isvip",
        "vip(",
        "ispro",
        "pro(",
        "ispaid",
        "paid(",
        "purchased",
        "subscrib",
        "unlock",
        "adfree",
        "ad_free",
        "noads",
        "isnoads",
        "iselite",
        "elite(",
        "isgold",
        "gold(",
        "isplus",
        "plus(",
        "islicensed",
        "islicenced",
        "isfull",
        "fullversion",
        "ispurchase",
        "isupgrad",
        "isactivated",
        "isactive",
    ];
    KEYS.iter().any(|k| sig_lower.contains(k))
}

/// Satır-düzeyi anahtar kalıplar: (alt-dizi, kural, kategori, öneri).
const LINE_RULES: &[(&str, &str, &str, &str)] = &[
    // İmza doğrulama
    (
        "getpackageinfo",
        "İmza kontrolü (getPackageInfo)",
        "signature",
        "İmza doğrulama olabilir — CorePatch/atlatma için ilgili boolean'ı ReturnTrue yap",
    ),
    (
        "->signatures",
        "İmza dizisine erişim",
        "signature",
        "İmza karşılaştırması — sonucu üreten metodu zorla",
    ),
    (
        "getsignatures",
        "getSignatures çağrısı",
        "signature",
        "İmza doğrulama — bypass adayı",
    ),
    (
        "checksignatures",
        "PackageManager.checkSignatures",
        "signature",
        "İmza eşleşme kontrolü — SIGNATURE_MATCH döndürecek şekilde zorla",
    ),
    (
        "get_signatures",
        "GET_SIGNATURES bayrağı",
        "signature",
        "İmza sorgusu — doğrulama akışını incele",
    ),
    // Faturalandırma
    (
        "com/android/vending/billing",
        "Google Play Billing",
        "billing",
        "Satın alma doğrulaması — purchased/owned boolean'larını zorla",
    ),
    (
        "com/android/billingclient",
        "Play BillingClient",
        "billing",
        "BillingClient akışı — satın alma durumunu zorla",
    ),
    (
        "isbillingsupported",
        "isBillingSupported",
        "billing",
        "Faturalandırma desteği kontrolü",
    ),
    // Lisans
    (
        "com/google/android/vending/licensing",
        "Google LVL lisans",
        "license",
        "License Verification Library — allow() sonucunu zorla",
    ),
    (
        "licensechecker",
        "LicenseChecker",
        "license",
        "Lisans kontrolü — izin verilen duruma zorla",
    ),
    // Root / emülatör tespiti
    (
        "/system/bin/su",
        "su ikili yolu (root tespiti)",
        "root",
        "Root tespiti — kontrol metodunu ReturnFalse ile zorla",
    ),
    (
        "test-keys",
        "build tags 'test-keys'",
        "root",
        "Root/custom ROM tespiti — ReturnFalse",
    ),
    (
        "isdevicerooted",
        "isDeviceRooted",
        "root",
        "Root tespiti — ReturnFalse",
    ),
    (
        "rootbeer",
        "RootBeer kütüphanesi",
        "root",
        "Root tespit kütüphanesi — sonucu ReturnFalse ile zorla",
    ),
    (
        "isemulator",
        "isEmulator",
        "root",
        "Emülatör tespiti — ReturnFalse",
    ),
];

/// Bir smali dosyasının metnini tarayıp kural eşleşmelerini döndürür.
pub fn scan_rules(text: &str) -> Vec<RuleHit> {
    let mut hits = Vec::new();

    // 1) Metod-düzeyi: premium/kilit boolean getter'ları (doğrudan zorlanabilir).
    for m in find_methods(text) {
        if m.return_type == "Z" && is_premium_bool_name(&m.signature.to_lowercase()) {
            hits.push(RuleHit {
                rule: "Premium/kilit boolean getter".into(),
                category: "premium".into(),
                method: Some(m.signature.clone()),
                line: m.start_line + 1,
                snippet: m.signature.clone(),
                suggestion: "Bu Z metodunu ReturnTrue ile zorla (premium/kilit aç)".into(),
            });
        }
    }

    // 2) Satır-düzeyi anahtar kalıplar.
    for (idx, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let low = line.to_lowercase();
        for (needle, rule, category, suggestion) in LINE_RULES {
            if low.contains(needle) {
                hits.push(RuleHit {
                    rule: (*rule).to_string(),
                    category: (*category).to_string(),
                    method: None,
                    line: idx + 1,
                    snippet: line.chars().take(160).collect(),
                    suggestion: (*suggestion).to_string(),
                });
            }
        }
    }

    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_premium_boolean_getter() {
        let src = "\
.method public isPremium()Z
    .registers 1
    const/4 v0, 0x0
    return v0
.end method
";
        let hits = scan_rules(src);
        assert!(hits
            .iter()
            .any(|h| h.category == "premium" && h.method.as_deref() == Some("isPremium()Z")));
    }

    #[test]
    fn ignores_non_boolean_premium_named() {
        // getPremiumName()Ljava/lang/String; boolean değil → premium kuralı tetiklenmez
        let src = "\
.method public getPremiumName()Ljava/lang/String;
    .registers 1
    const-string v0, \"x\"
    return-object v0
.end method
";
        let hits = scan_rules(src);
        assert!(!hits.iter().any(|h| h.category == "premium"));
    }

    #[test]
    fn detects_signature_check_line() {
        let src = "    invoke-virtual {v0, v1, v2}, Landroid/content/pm/PackageManager;->getPackageInfo(Ljava/lang/String;I)Landroid/content/pm/PackageInfo;";
        let hits = scan_rules(src);
        assert!(hits.iter().any(|h| h.category == "signature"));
        assert_eq!(hits[0].line, 1);
    }

    #[test]
    fn detects_root_and_billing() {
        let src = "\
    const-string v0, \"/system/bin/su\"
    const-string v1, \"com/android/vending/billing/IInAppBillingService\"
";
        let hits = scan_rules(src);
        assert!(hits.iter().any(|h| h.category == "root"));
        assert!(hits.iter().any(|h| h.category == "billing"));
    }

    #[test]
    fn clean_code_no_hits() {
        let src = "\
.method public getName()Ljava/lang/String;
    .registers 1
    const-string v0, \"hello world\"
    return-object v0
.end method
";
        assert!(scan_rules(src).is_empty());
    }
}
