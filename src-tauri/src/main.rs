//! StemClean DNR per Mac.
//!
//! La finestra mostra la stessa pagina della versione web (dist/index.html) con il
//! motore Rust compilato in WebAssembly. Questo lato nativo aggiunge solo ciò che
//! una pagina non può fare da sola dentro un'app: salvare i WAV dove scegli tu.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::ipc::{InvokeBody, Request};
use tauri_plugin_dialog::DialogExt;

/// Decodifica un nome codificato con `encodeURIComponent` dalla pagina.
fn decode_uri_component(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(v) = u8::from_str_radix(hex, 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Chiede dove salvare e scrive il file. I byte arrivano "grezzi" (senza JSON),
/// così anche un WAV da centinaia di MB passa senza conversioni.
/// Ritorna `false` se l'utente annulla.
#[tauri::command]
async fn salva_file(app: tauri::AppHandle, request: Request<'_>) -> Result<bool, String> {
    let InvokeBody::Raw(data) = request.body() else {
        return Err("dati del file mancanti".into());
    };
    let name = request
        .headers()
        .get("x-nome")
        .and_then(|v| v.to_str().ok())
        .map(decode_uri_component)
        .unwrap_or_else(|| "stemclean.wav".into());
    let ext = name.rsplit('.').next().unwrap_or("wav").to_lowercase();
    let picked = app
        .dialog()
        .file()
        .set_title("Salva il file di StemClean")
        .set_file_name(&name)
        .add_filter(
            if ext == "zip" {
                "Archivio ZIP"
            } else {
                "Audio WAV"
            },
            &[ext.as_str()],
        )
        .blocking_save_file();
    let Some(picked) = picked else {
        return Ok(false);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    std::fs::write(&path, data).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(true)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![salva_file])
        .run(tauri::generate_context!())
        .expect("impossibile avviare StemClean DNR");
}

#[cfg(test)]
mod tests {
    use super::decode_uri_component;

    #[test]
    fn decodes_names() {
        assert_eq!(
            decode_uri_component("voce%20prova_pulito.wav"),
            "voce prova_pulito.wav"
        );
        assert_eq!(decode_uri_component("citt%C3%A0.wav"), "città.wav");
        assert_eq!(decode_uri_component("100%"), "100%");
    }
}
