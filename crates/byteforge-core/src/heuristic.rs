//! Sezgisel hedef bulucu — 180K fonksiyon içinden lisans/kilit/ekonomi
//! fonksiyonlarını semantik puanlamayla en üste çıkarır. Saf Rust, test edilebilir.

use serde::Serialize;

/// Bir metod/sembol için hesaplanan puan ve gerekçeleri.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Score {
    pub name: String,
    pub score: i32,
    /// Puana katkı yapan gerekçeler (insan-okur).
    pub reasons: Vec<String>,
    /// Güven etiketi.
    pub confidence: String,
    /// Elenmesi gereken bir aday mı (negatif filtre).
    pub excluded: bool,
    /// Amaç kategorisi: character / economy / purchase_vip / ads / reward / other.
    pub category: String,
    /// Bu kategoriye önerilen yama: "return_true" | "return_max_int".
    pub suggested_template: String,
    /// Satın-alma/deferred akışı → körü körüne True basılırsa çökme riski.
    pub risky: bool,
}

/// Bir ismi amaç kategorisine sınıflandırır.
pub fn categorize(name: &str) -> &'static str {
    let l = name.to_lowercase();
    let any = |ks: &[&str]| ks.iter().any(|k| l.contains(k));
    if any(&[
        "adfree", "noads", "removedads", "removead", "isad", "hasad", "adremov",
        "interstitial", "rewardedvideo", "rewardedad", "watchad", "showad",
        "shouldshowad", "shouldshowinterstitial", "onrewarded", "adwatched",
        "handleadwatch", "admanager", "adplacement", "adshandler", "requestinterstitial",
        "showrewarded", "playrewarded",
    ]) {
        "ads"
    } else if any(&[
        "coin", "wallet", "currency", "money", "gem", "cash", "gold", "afford", "balance", "price",
        "cost", "token", "credit",
    ]) {
        "economy"
    } else if any(&[
        "character",
        "skin",
        "board",
        "hat",
        "outfit",
        "costume",
        "avatar",
        "hero",
        "weapon",
        "car",
        "vehicle",
        "level",
        "map",
        "stage",
        "unlock",
    ]) {
        "character"
    } else if any(&[
        "reward",
        "daily",
        "bonus",
        "gift",
        "multiplier",
        "prize",
        "loot",
    ]) {
        "reward"
    } else if any(&[
        "purchas", "vip", "premium", "subscri", "license", "licence", "owned", "entitl", "ispro",
        "ispaid", "bought", "buy", "restore",
    ]) {
        "purchase_vip"
    } else {
        "other"
    }
}

/// Bu isim bir satın-alma/işlem akışı fonksiyonu mu? (körü körüne True → çökme)
/// Örn: HasAnyDeferredPurchase, PendingTransaction, RestorePurchase, AcknowledgePurchase.
pub fn is_risky(name: &str) -> bool {
    let l = name.to_lowercase();
    const RISK: &[&str] = &[
        "deferred",
        "pending",
        "receipt",
        "transaction",
        "restore",
        "consume",
        "acknowled",
        "queue",
        "refund",
        "verifypurchase",
        "finishtransaction",
    ];
    RISK.iter().any(|k| l.contains(k))
}

/// Kategoriye göre önerilen yama şablonu.
fn suggested_for(category: &str, name: &str) -> &'static str {
    let l = name.to_lowercase();
    // Ekonomi: bakiye/miktar döndüren getter → MAX_INT; "canAfford" gibi bool → true.
    if category == "economy" {
        let is_bool_like = l.starts_with("is")
            || l.starts_with("has")
            || l.starts_with("can")
            || l.contains("afford")
            || l.contains("enough");
        if is_bool_like {
            "return_true"
        } else {
            "return_max_int"
        }
    } else {
        "return_true"
    }
}

/// Erişim/güvenlik/ekonomi anahtar kelimeleri (+40).
/// Reklam-kontrol fonksiyonları (göster/oynat/izle/istek/ödül-callback) — reklam
/// kaldırma / ödül modlarının hedefleri. Bunlar bool-getter değil, o yüzden ayrı puan.
const AD_CONTROL_KEYS: &[&str] = &[
    "interstitial",
    "rewardedvideo",
    "rewardedad",
    "watchad",
    "showad",
    "playrewarded",
    "playloaded",
    "shouldshowad",
    "shouldshowinterstitial",
    "handleadwatch",
    "adwatched",
    "onrewarded",
    "requestrewarded",
    "requestinterstitial",
    "showcommercial",
    "showrewarded",
    "adplacement",
];

const SECURITY_KEYS: &[&str] = &[
    "purchas",
    "unlock",
    "premium",
    "vip",
    "license",
    "licence",
    "valid",
    "subscri",
    "bitizen",
    "noads",
    "adfree",
    "unlimited",
    "ispro",
    "ispaid",
    "owned",
    "entitl",
    "nolimit",
    "fullversion",
    "isupgrad",
    "activated",
    "coin",
    "wallet",
    "currency",
    "character",
    "board",
    "godmode",
    "infinite",
    "reward",
    "multiplier",
    "allitems",
    "cheat",
];

/// Negatif filtre — bool olsa da hedef DEĞİL (yamalanırsa çökme riski ya da
/// tamamen alakasız donanım/bellek/framework kavramı).
const NEGATIVE_KEYS: &[&str] = &[
    "isdead",
    "iserror",
    "error",
    "cancel",
    "failed",
    "loading",
    "isnull",
    "isempty",
    "isnetwork",
    "isconnect",
    "isloaded",
    "isready",
    "isvisible",
    "isenabled",
    "isactive",
    "isrunning",
    "ispaused",
    "isdirty",
    "keyboard",
    "monkey",
    // Donanım / "board" yanılgıları (kaykay/karakter DEĞİL)
    "buildboard",
    "motherboard",
    "clipboard",
    "cardboard",
    "dashboard",
    "storyboard",
    "billboard",
    "leaderboard",
    "scoreboard",
    "chessboard",
    "surfboard",
    "onboard",
    "keyboard",
    // Bellek/eşzamanlılık kilitleri (lisans/karakter kilidi DEĞİL)
    "mutex",
    "semaphore",
    "spinlock",
    "reentrantlock",
    "readwritelock",
    "readlock",
    "writelock",
    "bytebuffer",
    "cachedcontent",
    "codec",
    "socket",
];

/// Bilinen üçüncü-parti SDK / kütüphane paket önekleri — akıllı taramadan
/// TAMAMEN elenir (sadece uygulamanın kendi kodu taranır).
const LIBRARY_PREFIXES: &[&str] = &[
    "androidx/",
    "android/support/",
    "com/google/",
    "com/android/",
    "kotlin/",
    "kotlinx/",
    "com/facebook/",
    "com/unity3d/",
    "gatewayprotocol/",
    "com/adjust/",
    "com/appsflyer/",
    "com/ironsource/",
    "com/applovin/",
    "com/vungle/",
    "com/mbridge/",
    "com/bytedance/",
    "okhttp3/",
    "okio/",
    "retrofit2/",
    "com/bumptech/",
    "dagger/",
    "javax/",
    "org/json/",
    "org/apache/",
    "com/squareup/",
    "io/reactivex/",
    "io/grpc/",
    "com/onesignal/",
    "com/mixpanel/",
    "bolts/",
    "j$/",
    "google/protobuf/",
    "com/tapjoy/",
    "com/mopub/",
    "io/branch/",
    "com/chartboost/",
    "com/amplitude/",
    "io/flutter/",
    "org/chromium/",
];

/// Bir sınıf/dosya yolunun (ör. "com/google/gson/Foo" veya "com.google.gson.Foo")
/// bilinen bir SDK/kütüphaneye ait olup olmadığı.
pub fn is_library_path(path: &str) -> bool {
    let norm = path.replace('.', "/");
    LIBRARY_PREFIXES
        .iter()
        .any(|p| norm.starts_with(p) || norm.contains(&format!("/{p}")))
}

/// İsim erişim-kontrolü kalıbıyla başlıyor mu?
fn has_access_prefix(low: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "is", "has", "can", "check", "get_is", "get_has", "get_", "allow", "should",
    ];
    PREFIXES.iter().any(|p| low.starts_with(p))
}

/// C# / IL2CPP derleyicisinin ürettiği iç semboller (lambda, backing field, state machine).
pub fn is_compiler_generated(name: &str) -> bool {
    name.starts_with('<')
        || name.contains("k__BackingField")
        || name.contains("<>")
        || name.contains('$')
        || name.contains("__")
}

fn confidence_label(score: i32) -> &'static str {
    if score >= 70 {
        "çok yüksek (~%95 lisans/kilit)"
    } else if score >= 50 {
        "yüksek"
    } else if score >= 30 {
        "orta"
    } else if score >= 10 {
        "olası"
    } else {
        "düşük"
    }
}

/// Bir sembolü puanlar.
/// - `ret_bool`: dönüş tipi bool mu? (bilinmiyorsa None)
/// - `parameterless`: parametresiz mi? (bilinmiyorsa None)
pub fn score_symbol(name: &str, ret_bool: Option<bool>, parameterless: Option<bool>) -> Score {
    let low = name.to_lowercase();
    let mut score = 0i32;
    let mut reasons = Vec::new();

    // Derleyici üretimi (backing field, lambda closure, coroutine) → elenmeli.
    if is_compiler_generated(name) {
        return Score {
            name: name.to_string(),
            score: -100,
            confidence: "derleyici üretimi (elenmiş)".into(),
            reasons: vec![
                "derleyici üretimi (<...> / backing field / lambda) → yama hedefi değil".into(),
            ],
            excluded: true,
            category: "other".into(),
            suggested_template: "return_true".into(),
            risky: false,
        };
    }

    // Negatif filtre — önce.
    let excluded = NEGATIVE_KEYS.iter().any(|k| low.contains(k));
    if excluded {
        score -= 50;
        reasons.push("negatif kalıp (isDead/error/loading...) → hedef değil".into());
    }

    // Güvenlik/erişim anahtar kelimeleri (+40).
    if let Some(k) = SECURITY_KEYS.iter().find(|k| low.contains(**k)) {
        score += 40;
        reasons.push(format!("erişim anahtar kelimesi '{k}' (+40)"));
    }

    // Reklam kontrol fonksiyonu (göster/oynat/izle/ödül-callback) (+35).
    if AD_CONTROL_KEYS.iter().any(|k| low.contains(k)) {
        score += 35;
        reasons.push("reklam kontrol fonksiyonu (göster/oynat/ödül) (+35)".into());
    }

    // İsim erişim-kontrolü öneki (+20).
    if has_access_prefix(&low) {
        score += 20;
        reasons.push("erişim öneki (is/has/can/check/get_is...) (+20)".into());
    }

    // bool + parametresiz (+20).
    match (ret_bool, parameterless) {
        (Some(true), Some(true)) => {
            score += 20;
            reasons.push("bool dönüş + parametresiz (+20)".into());
        }
        (Some(true), _) => {
            score += 10;
            reasons.push("bool dönüş (+10)".into());
        }
        _ => {}
    }

    let category = categorize(name);
    let risky = is_risky(name);
    if risky {
        reasons.push("DİKKAT: satın-alma/işlem akışı — körü körüne True basma, çökebilir".into());
    }

    Score {
        name: name.to_string(),
        score,
        confidence: confidence_label(score).to_string(),
        reasons,
        excluded,
        category: category.to_string(),
        suggested_template: suggested_for(category, name).to_string(),
        risky,
    }
}

/// Bir isim listesini (tip bilgisi olmadan) puanlayıp en yüksek `top` adayı
/// döndürür (elenenler hariç, azalan puan). IL2CPP/Mono sembol adları için.
pub fn rank_names(names: &[String], top: usize) -> Vec<Score> {
    let mut scored: Vec<Score> = names
        .iter()
        .map(|n| {
            // İsimden bool-getter çıkarımı: "get_is"/"is"/"has"/"can" → muhtemel bool.
            let low = n.to_lowercase();
            let inferred_bool = low.starts_with("get_is")
                || low.starts_with("get_has")
                || low.starts_with("is")
                || low.starts_with("has")
                || low.starts_with("can");
            score_symbol(n, if inferred_bool { Some(true) } else { None }, None)
        })
        .filter(|s| !s.excluded && s.score >= 20)
        .collect();
    scored.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.name.cmp(&b.name)));
    scored.truncate(top);
    scored
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premium_bool_getter_scores_high() {
        // get_isPremium: erişim kelimesi(premium +40) + önek(get_is +20) + bool+parametresiz(+20)
        let s = score_symbol("get_isPremium", Some(true), Some(true));
        assert_eq!(s.score, 80);
        assert!(!s.excluded);
        assert!(s.confidence.contains("çok yüksek"));
    }

    #[test]
    fn isdead_is_excluded() {
        let s = score_symbol("isDead", Some(true), Some(true));
        assert!(s.excluded);
        assert!(s.score < 20);
    }

    #[test]
    fn neutral_getter_low_score() {
        let s = score_symbol("getName", None, None);
        assert!(s.score < 20);
    }

    #[test]
    fn security_keyword_alone_scores() {
        // UserBoughtCharacter: "owned" yok ama "purchas" yok... "bought" yok listede.
        // "unlockAll" → unlock +40
        let s = score_symbol("unlockAll", None, None);
        assert!(s.score >= 40);
    }

    #[test]
    fn rank_names_surfaces_targets() {
        let names: Vec<String> = [
            "getName",
            "get_isPremium",
            "isDead",
            "checkLicense",
            "update",
            "hasVipAccess",
            "isNetworkError",
            "renderFrame",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let top = rank_names(&names, 5);
        let top_names: Vec<&str> = top.iter().map(|s| s.name.as_str()).collect();
        assert!(top_names.contains(&"get_isPremium"));
        assert!(top_names.contains(&"checkLicense"));
        assert!(top_names.contains(&"hasVipAccess"));
        // Elenenler ve nötrler gelmemeli.
        assert!(!top_names.contains(&"isDead"));
        assert!(!top_names.contains(&"isNetworkError"));
        assert!(!top_names.contains(&"getName"));
        assert!(!top_names.contains(&"renderFrame"));
    }

    #[test]
    fn library_paths_are_detected() {
        assert!(is_library_path(
            "androidx/media3/datasource/cache/CachedContent"
        ));
        assert!(is_library_path("com.google.protobuf.StaticDeviceInfo"));
        assert!(is_library_path("smali/kotlin/coroutines/Foo"));
        assert!(is_library_path("com/unity3d/services/Ads"));
        // Oyunun kendi kodu → kütüphane değil.
        assert!(!is_library_path("com/candywriter/bitlife/Player"));
        assert!(!is_library_path("com/game/Economy"));
    }

    #[test]
    fn hardware_board_words_excluded() {
        // Protobuf hasBuildBoard / keyboard / clipboard → hedef değil.
        assert!(score_symbol("hasBuildBoard", Some(true), Some(true)).excluded);
        assert!(score_symbol("isFullyKeyboard", Some(true), Some(true)).excluded);
        // Bellek kilidi → hedef değil.
        assert!(score_symbol("unlockMutex", None, None).excluded);
        assert!(score_symbol("isFullyUnlockedByteBuffer", Some(true), Some(true)).excluded);
    }

    #[test]
    fn categorizes_by_purpose() {
        assert_eq!(categorize("HasCharacter"), "character");
        assert_eq!(categorize("get_CoinBalance"), "economy");
        assert_eq!(categorize("IsAdFree"), "ads");
        assert_eq!(categorize("HasPremium"), "purchase_vip");
        assert_eq!(categorize("DailyReward"), "reward");
        assert_eq!(categorize("SomethingElse"), "other");
    }

    #[test]
    fn economy_getter_suggests_max_int() {
        // Bakiye getter → MAX_INT, canAfford (bool) → true.
        let s = score_symbol("get_CoinBalance", None, None);
        assert_eq!(s.category, "economy");
        assert_eq!(s.suggested_template, "return_max_int");
        let a = score_symbol("CanAffordItem", Some(true), Some(false));
        assert_eq!(a.suggested_template, "return_true");
    }

    #[test]
    fn deferred_purchase_is_risky() {
        let s = score_symbol("HasAnyDeferredPurchase", Some(true), Some(true));
        assert!(s.risky);
        assert!(s.reasons.iter().any(|r| r.contains("DİKKAT")));
        // Yine de yüksek puanlı (aday), sadece uyarılı.
        assert!(s.score >= 40);
    }

    #[test]
    fn compiler_generated_is_excluded() {
        let s = score_symbol("<BuildProductsFromPurchase>b__0", None, None);
        assert!(s.excluded);
        assert_eq!(s.score, -100);

        let names: Vec<String> = vec![
            "<ActivateDoubleScoreLater>d__5".into(),
            "<BuildProductsFromPurchase>b__0".into(),
            "HasBeenUnlocked".into(),
        ];
        let top = rank_names(&names, 5);
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].name, "HasBeenUnlocked");
    }
}
