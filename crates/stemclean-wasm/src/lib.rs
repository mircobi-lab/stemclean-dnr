//! Interfaccia WebAssembly di StemClean DNR.
//!
//! Funzioni `extern "C"` semplici, senza wasm-bindgen: il file `.wasm` non ha
//! dipendenze e la pagina lo carica con `WebAssembly.instantiate`. I numeri
//! viaggiano come argomenti, i blocchi audio passano dalla memoria del modulo:
//! la pagina chiede un puntatore (`sc_buffer`) e ci scrive o ci legge.
//!
//! Attenzione lato JavaScript: dopo ogni chiamata che può allocare memoria il
//! `memory.buffer` può cambiare, quindi le viste `Float32Array` vanno ricreate.

// Le funzioni ricevono il puntatore creato da `sc_new`: è il contratto con la pagina, non un dato esterno.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use stemclean_core::{
    Engine, Mode, Params, Source,
    view::{self, Palette},
    wav::{self, Bits},
    zip::ZipWriter,
};

pub struct State {
    engine: Engine,
    stage_l: Vec<f32>,
    stage_r: Vec<f32>,
    view: Vec<f32>,
    pixels: Vec<u8>,
    export: Vec<u8>,
    text: Vec<u8>,
    zip: Option<ZipWriter>,
}

fn st<'a>(h: *mut State) -> &'a mut State {
    // SAFETY: `h` viene solo da `sc_new` e la pagina non lo usa dopo `sc_free`.
    unsafe { &mut *h }
}

/// Versione del motore (per il controllo nella pagina): 2.0.0 → 20000.
#[unsafe(no_mangle)]
pub extern "C" fn sc_version() -> u32 {
    20000
}

/// Crea un motore per uno stem lungo `len` campioni a `sr` Hz.
#[unsafe(no_mangle)]
pub extern "C" fn sc_new(sr: f64, len: u32) -> *mut State {
    Box::into_raw(Box::new(State {
        engine: Engine::new(sr, len as usize),
        stage_l: Vec::new(),
        stage_r: Vec::new(),
        view: Vec::new(),
        pixels: Vec::new(),
        export: Vec::new(),
        text: Vec::new(),
        zip: None,
    }))
}

#[unsafe(no_mangle)]
pub extern "C" fn sc_free(h: *mut State) {
    if !h.is_null() {
        // SAFETY: vedi `st`.
        drop(unsafe { Box::from_raw(h) });
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn sc_frames(h: *mut State) -> u32 {
    st(h).engine.frames as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn sc_bands(h: *mut State) -> u32 {
    st(h).engine.bands.nb as u32
}

/// Puntatore a un buffer audio del motore:
/// 0/1 stem L/R · 2/3 area di appoggio per un riferimento · 10/11 pulito · 12/13 solo tolto.
#[unsafe(no_mangle)]
pub extern "C" fn sc_buffer(h: *mut State, which: u32) -> *mut f32 {
    let s = st(h);
    match which {
        0 => s.engine.l.as_mut_ptr(),
        1 => s.engine.r.as_mut_ptr(),
        2 => s.stage_l.as_mut_ptr(),
        3 => s.stage_r.as_mut_ptr(),
        10 => s.engine.out_l.as_mut_ptr(),
        11 => s.engine.out_r.as_mut_ptr(),
        12 => s.engine.rem_l.as_mut_ptr(),
        13 => s.engine.rem_r.as_mut_ptr(),
        _ => std::ptr::null_mut(),
    }
}

/// Prepara l'area di appoggio per un riferimento lungo `n` campioni.
#[unsafe(no_mangle)]
pub extern "C" fn sc_stage(h: *mut State, n: u32) {
    let s = st(h);
    s.stage_l = vec![0.0; n as usize];
    s.stage_r = vec![0.0; n as usize];
}

/// Somma il riferimento appena copiato nell'area di appoggio e la libera.
#[unsafe(no_mangle)]
pub extern "C" fn sc_add_staged_reference(h: *mut State) {
    let s = st(h);
    let (l, r) = (
        std::mem::take(&mut s.stage_l),
        std::mem::take(&mut s.stage_r),
    );
    s.engine.add_reference(&l, &r);
}

#[unsafe(no_mangle)]
pub extern "C" fn sc_analyze_target(h: *mut State, t0: u32, t1: u32) {
    st(h).engine.analyze_target(t0 as usize, t1 as usize);
}

#[unsafe(no_mangle)]
pub extern "C" fn sc_analyze_reference(h: *mut State, t0: u32, t1: u32) {
    st(h).engine.analyze_reference(t0 as usize, t1 as usize);
}

#[unsafe(no_mangle)]
pub extern "C" fn sc_release_reference(h: *mut State) {
    st(h).engine.release_reference_audio();
}

#[unsafe(no_mangle)]
pub extern "C" fn sc_has_reference(h: *mut State) -> u32 {
    st(h).engine.has_reference() as u32
}

/// Calcola inviluppi e guadagni. `mode`: 0 sidechain · 1 solo espansore · 2 combinata.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn sc_prepare(
    h: *mut State,
    mode: u32,
    margin: f64,
    slope: f64,
    cut: f64,
    thr: f64,
    ratio: f64,
    att: f64,
    rel: f64,
) {
    let p = Params {
        mode: Mode::from_code(mode),
        margin,
        slope,
        cut,
        thr,
        ratio,
        att,
        rel,
    };
    st(h).engine.prepare(&p);
}

#[unsafe(no_mangle)]
pub extern "C" fn sc_apply(h: *mut State, t0: u32, t1: u32) {
    st(h).engine.apply(t0 as usize, t1 as usize);
}

#[unsafe(no_mangle)]
pub extern "C" fn sc_finish(h: *mut State) {
    st(h).engine.finish();
}

/// 0 = dB tolti · 1 = variazione media sui passaggi forti · 2 = 1% peggiore.
#[unsafe(no_mangle)]
pub extern "C" fn sc_metric(h: *mut State, i: u32) -> f64 {
    let m = st(h).engine.metrics;
    match i {
        0 => m.removed_db,
        1 => m.sing_mean,
        _ => m.sing_worst,
    }
}

/// Spettrogramma del segnale `src` (0 originale · 1 pulito · 2 solo tolto). Ritorna `pw·ph` float.
#[unsafe(no_mangle)]
pub extern "C" fn sc_spectrogram(h: *mut State, src: u32, pw: u32, ph: u32) -> *const f32 {
    let s = st(h);
    s.view = s
        .engine
        .spectrogram(Source::from_code(src), pw as usize, ph as usize);
    s.view.as_ptr()
}

/// Mappa della riduzione applicata. Ritorna `pw·ph` float (dB ≤ 0).
#[unsafe(no_mangle)]
pub extern "C" fn sc_reduction(h: *mut State, pw: u32, ph: u32) -> *const f32 {
    let s = st(h);
    s.view = s.engine.reduction_map(pw as usize, ph as usize);
    s.view.as_ptr()
}

/// Prepara il buffer del grafico per `n` valori scritti dalla pagina (per ricolorare dati già calcolati).
#[unsafe(no_mangle)]
pub extern "C" fn sc_view_buffer(h: *mut State, n: u32) -> *mut f32 {
    let s = st(h);
    s.view = vec![0.0; n as usize];
    s.view.as_mut_ptr()
}

/// Colora l'ultimo spettrogramma o mappa. `palette`: 0 magma · 1 ambra. Ritorna `pw·ph·4` byte RGBA.
#[unsafe(no_mangle)]
pub extern "C" fn sc_pixels(h: *mut State, vmin: f64, vmax: f64, palette: u32) -> *const u8 {
    let s = st(h);
    s.pixels = view::rgba(&s.view, vmin, vmax, Palette::from_code(palette));
    s.pixels.as_ptr()
}

/// Colore di una tavolozza per un valore 0..1, impacchettato come 0xRRGGBB (per la barra colori).
#[unsafe(no_mangle)]
pub extern "C" fn sc_palette_color(palette: u32, v: f64) -> u32 {
    let c = Palette::from_code(palette).color(v);
    ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32
}

/// Crea il WAV del segnale `src` (1 pulito · 2 solo tolto · 0 originale), `bits` 24 o 32.
/// Ritorna la lunghezza in byte; i dati sono in `sc_export_ptr`.
#[unsafe(no_mangle)]
pub extern "C" fn sc_wav(h: *mut State, src: u32, bits: u32) -> u32 {
    let s = st(h);
    let (l, r) = s.engine.channels(Source::from_code(src));
    let bits = Bits::from_number(bits).unwrap_or(Bits::Int24);
    s.export = wav::encode(l, r, s.engine.sr as u32, bits);
    s.export.len() as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn sc_export_ptr(h: *mut State) -> *const u8 {
    st(h).export.as_ptr()
}

/// Area per passare un testo UTF-8 (il nome di un file) dalla pagina al motore.
#[unsafe(no_mangle)]
pub extern "C" fn sc_text(h: *mut State, n: u32) -> *mut u8 {
    let s = st(h);
    s.text = vec![0; n as usize];
    s.text.as_mut_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn sc_zip_begin(h: *mut State) {
    st(h).zip = Some(ZipWriter::new());
}

/// Aggiunge allo zip il WAV appena creato con `sc_wav`, con il nome scritto in `sc_text`.
#[unsafe(no_mangle)]
pub extern "C" fn sc_zip_add_export(h: *mut State) {
    let s = st(h);
    let name = String::from_utf8_lossy(&s.text).into_owned();
    if let Some(z) = s.zip.as_mut() {
        z.add(&name, &s.export);
    }
    s.export = Vec::new();
}

/// Chiude lo zip: i byte sono in `sc_export_ptr`, la lunghezza è il valore di ritorno.
#[unsafe(no_mangle)]
pub extern "C" fn sc_zip_finish(h: *mut State) -> u32 {
    let s = st(h);
    s.export = s.zip.take().map(ZipWriter::finish).unwrap_or_default();
    s.export.len() as u32
}

/// Libera i buffer di esportazione e del grafico.
#[unsafe(no_mangle)]
pub extern "C" fn sc_release_export(h: *mut State) {
    let s = st(h);
    s.export = Vec::new();
}
