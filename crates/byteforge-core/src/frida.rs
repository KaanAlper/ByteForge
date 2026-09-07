//! Frida script kütüphanesi — hazır, parametrik enstrümantasyon scriptleri.
//!
//! Saf Rust: script metinlerini üretir. Cihazda çalıştırma harici Frida/objection
//! ile yapılır (bu modül yalnızca hazır script kaynağını sağlar).

use serde::Serialize;

/// Kütüphanedeki hazır bir Frida scripti.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FridaScript {
    /// Kısa kimlik (ör. "ssl-bypass").
    pub id: String,
    /// Görünen ad.
    pub title: String,
    /// Ne işe yaradığı (insan-okur).
    pub description: String,
    /// Çalıştırma ipucu (ör. frida komutu).
    pub usage: String,
    /// JavaScript script kaynağı.
    pub script: String,
}

const SSL_BYPASS: &str = r#"// Universal SSL pinning bypass (Android)
Java.perform(function () {
  try {
    var X509TrustManager = Java.use('javax.net.ssl.X509TrustManager');
    var SSLContext = Java.use('javax.net.ssl.SSLContext');
    var TrustManager = Java.registerClass({
      name: 'com.byteforge.TrustAll',
      implements: [X509TrustManager],
      methods: {
        checkClientTrusted: function () {},
        checkServerTrusted: function () {},
        getAcceptedIssuers: function () { return []; }
      }
    });
    var tms = [TrustManager.$new()];
    var initI = SSLContext.init.overload(
      '[Ljavax.net.ssl.KeyManager;',
      '[Ljavax.net.ssl.TrustManager;',
      'java.security.SecureRandom');
    initI.implementation = function (km, tm, sr) { initI.call(this, km, tms, sr); };
    console.log('[+] SSLContext.init hooked (trust-all)');
  } catch (e) { console.log('[-] ' + e); }

  // OkHttp CertificatePinner bypass
  try {
    var CP = Java.use('okhttp3.CertificatePinner');
    CP.check.overload('java.lang.String', 'java.util.List').implementation = function () {
      console.log('[+] OkHttp CertificatePinner.check bypassed');
    };
  } catch (e) {}
});"#;

const ROOT_BYPASS: &str = r#"// Root & emulator detection bypass (Android)
Java.perform(function () {
  // RootBeer
  try {
    var RootBeer = Java.use('com.scottyab.rootbeer.RootBeer');
    ['isRooted', 'isRootedWithoutBusyBoxCheck', 'checkForSuBinary', 'detectRootManagementApps']
      .forEach(function (m) {
        if (RootBeer[m]) RootBeer[m].implementation = function () { return false; };
      });
    console.log('[+] RootBeer neutralized');
  } catch (e) {}

  // File.exists() for su paths
  try {
    var File = Java.use('java.io.File');
    File.exists.implementation = function () {
      var p = this.getAbsolutePath();
      if (p.indexOf('su') >= 0 || p.indexOf('magisk') >= 0 || p.indexOf('supersu') >= 0) {
        console.log('[+] File.exists blocked: ' + p);
        return false;
      }
      return this.exists.call(this);
    };
  } catch (e) {}

  // Runtime.exec('su')
  try {
    var Runtime = Java.use('java.lang.Runtime');
    Runtime.exec.overload('java.lang.String').implementation = function (cmd) {
      if (cmd.indexOf('su') >= 0) { console.log('[+] Runtime.exec blocked: ' + cmd); throw new Error('blocked'); }
      return this.exec.overload('java.lang.String').call(this, cmd);
    };
  } catch (e) {}
});"#;

const HEAP_SEARCH: &str = r#"// Signature check (PackageManager) tracer
Java.perform(function () {
  try {
    var PM = Java.use('android.app.ApplicationPackageManager');
    PM.getPackageInfo.overload('java.lang.String', 'int').implementation = function (pkg, flags) {
      console.log('[*] getPackageInfo("' + pkg + '", ' + flags + ')  <-- imza kontrolü olabilir');
      return this.getPackageInfo(pkg, flags);
    };
    console.log('[+] getPackageInfo tracer kuruldu');
  } catch (e) { console.log('[-] ' + e); }
});"#;

/// Sınıf.metod izleyicisi (parametreler + dönüş değerini loglar).
fn method_tracer(class: &str, method: &str) -> String {
    let (c, m) = (sanitize(class), sanitize(method));
    format!(
        r#"// {c}.{m} method tracer
Java.perform(function () {{
  try {{
    var Cls = Java.use('{c}');
    var overloads = Cls['{m}'].overloads;
    overloads.forEach(function (ov) {{
      ov.implementation = function () {{
        console.log('[>] {c}.{m}(' + Array.prototype.join.call(arguments, ', ') + ')');
        var ret = ov.apply(this, arguments);
        console.log('[<] {c}.{m} => ' + ret);
        return ret;
      }};
    }});
    console.log('[+] {c}.{m} izleniyor (' + overloads.length + ' overload)');
  }} catch (e) {{ console.log('[-] ' + e); }}
}});"#
    )
}

/// Tek tırnak/newline enjeksiyonuna karşı basit temizleme.
fn sanitize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '$' | '<' | '>'))
        .collect()
}

/// Kütüphanedeki sabit scriptleri döndürür.
pub fn builtin_scripts() -> Vec<FridaScript> {
    vec![
        FridaScript {
            id: "ssl-bypass".into(),
            title: "SSL Pinning Bypass".into(),
            description: "TrustManager'ı trust-all yapar + OkHttp CertificatePinner'ı atlar (MitM/trafik analizi için).".into(),
            usage: "frida -U -f <paket> -l ssl-bypass.js".into(),
            script: SSL_BYPASS.into(),
        },
        FridaScript {
            id: "root-bypass".into(),
            title: "Root / Emülatör Tespiti Bypass".into(),
            description: "RootBeer, File.exists(su), Runtime.exec(su) kontrollerini etkisiz kılar.".into(),
            usage: "frida -U -f <paket> -l root-bypass.js".into(),
            script: ROOT_BYPASS.into(),
        },
        FridaScript {
            id: "signature-tracer".into(),
            title: "İmza Kontrolü İzleyici".into(),
            description: "getPackageInfo çağrılarını loglar — imza doğrulama akışını bulmak için.".into(),
            usage: "frida -U -f <paket> -l signature-tracer.js".into(),
            script: HEAP_SEARCH.into(),
        },
    ]
}

/// Parametrik method tracer scriptini üretir.
pub fn tracer_script(class: &str, method: &str) -> FridaScript {
    FridaScript {
        id: "method-tracer".into(),
        title: format!("Method Tracer: {class}.{method}"),
        description: "Belirtilen sınıf.metodun tüm overload'larını izler (argüman + dönüş).".into(),
        usage: "frida -U -f <paket> -l tracer.js".into(),
        script: method_tracer(class, method),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_present() {
        let s = builtin_scripts();
        assert!(s.iter().any(|x| x.id == "ssl-bypass"));
        assert!(s.iter().any(|x| x.id == "root-bypass"));
        assert!(s.iter().all(|x| !x.script.is_empty()));
    }

    #[test]
    fn tracer_embeds_class_and_method() {
        let t = tracer_script("com.example.Billing", "isPremium");
        assert!(t.script.contains("com.example.Billing"));
        assert!(t.script.contains("isPremium"));
    }

    #[test]
    fn tracer_sanitizes_injection() {
        // Kötü niyetli girdi tek tırnak/parantez enjekte edememeli.
        let t = tracer_script("Foo');console.log('x", "bar");
        assert!(!t.script.contains("console.log('x"));
        assert!(t.script.contains("Foo.console.logx") || t.script.contains("Fooconsole.logx"));
    }
}
