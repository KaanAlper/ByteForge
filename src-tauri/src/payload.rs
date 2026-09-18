use std::fs;
use std::process::Command;

#[tauri::command]
pub fn generate_payload(os: String, _domain: String, response: String) -> Result<String, String> {
    if os == "linux" {
        let code = format!(r#"// Linux LD_PRELOAD Hook (C)
// ByteForge Payload Generator
#define _GNU_SOURCE
#include <dlfcn.h>
#include <string.h>
#include <stdio.h>

const char* FAKE_RESPONSE = "HTTP/1.1 200 OK\r\n"
                            "Content-Type: application/json\r\n"
                            "Content-Length: {len}\r\n\r\n"
                            "{resp}";

typedef ssize_t (*orig_recv_type)(int sockfd, void *buf, size_t len, int flags);

ssize_t recv(int sockfd, void *buf, size_t len, int flags) {{
    orig_recv_type orig_recv;
    orig_recv = (orig_recv_type)dlsym(RTLD_NEXT, "recv");
    
    ssize_t result = orig_recv(sockfd, buf, len, flags);
    
    if (result > 0 && len > strlen(FAKE_RESPONSE)) {{
        strncpy((char*)buf, FAKE_RESPONSE, len);
        return strlen(FAKE_RESPONSE);
    }}
    
    return result;
}}
"#, len=response.len(), resp=response);
        Ok(code)
    } else {
        let code = format!(r#"// Windows DLL API Hook (C++)
// ByteForge Payload Generator
#include <windows.h>
#include <winsock2.h>
#include "MinHook.h"

const char* FAKE_RESPONSE = "HTTP/1.1 200 OK\r\n"
                            "Content-Type: application/json\r\n"
                            "Content-Length: {len}\r\n\r\n"
                            "{resp}";

typedef int (WINAPI *RECV)(SOCKET, char*, int, int);
RECV fpRecv = NULL;

int WINAPI DetourRecv(SOCKET s, char* buf, int len, int flags) {{
    if (len > strlen(FAKE_RESPONSE)) {{
        strncpy(buf, FAKE_RESPONSE, len);
        return strlen(FAKE_RESPONSE);
    }}
    return fpRecv(s, buf, len, flags);
}}

BOOL APIENTRY DllMain(HMODULE hModule, DWORD ul_reason_for_call, LPVOID lpReserved) {{
    switch (ul_reason_for_call) {{
    case DLL_PROCESS_ATTACH:
        MH_Initialize();
        MH_CreateHookApi(L"ws2_32", "recv", &DetourRecv, (LPVOID*)&fpRecv);
        MH_EnableHook(MH_ALL_HOOKS);
        break;
    case DLL_PROCESS_DETACH:
        MH_Uninitialize();
        break;
    }}
    return TRUE;
}}
"#, len=response.len(), resp=response);
        Ok(code)
    }
}

#[tauri::command]
pub fn compile_payload(os: String, code: String) -> Result<String, String> {
    if os != "linux" {
        return Err("Otomatik derleme ve enjekte etme özelliği şu anda yalnızca Linux (LD_PRELOAD) için desteklenmektedir. Windows için manuel derleme yapmalısınız.".to_string());
    }

    let tmp_dir = std::env::temp_dir();
    let c_path = tmp_dir.join("byteforge_hook.c");
    let out_path = tmp_dir.join("byteforge_hook.so");
    
    fs::write(&c_path, code).map_err(|e| format!("C dosyası yazılamadı: {}", e))?;

    let output = Command::new("gcc")
        .arg("-shared")
        .arg("-fPIC")
        .arg("-o")
        .arg(&out_path)
        .arg(&c_path)
        .arg("-ldl")
        .output()
        .map_err(|e| format!("GCC çalıştırılamadı (gcc yüklü mü?): {}", e))?;

    if !output.status.success() {
        return Err(format!("Derleme hatası: {}", String::from_utf8_lossy(&output.stderr)));
    }

    Ok(out_path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn launch_with_hook(exe_path: String, hook_path: String) -> Result<(), String> {
    // Check if executable exists
    if !std::path::Path::new(&exe_path).exists() {
        return Err(format!("Belirtilen uygulama bulunamadı: {}", exe_path));
    }

    Command::new(&exe_path)
        .env("LD_PRELOAD", &hook_path)
        .spawn()
        .map_err(|e| format!("Uygulama başlatılamadı: {}", e))?;
        
    Ok(())
}
