//! `stemclean` — StemClean DNR da Terminale, in codice nativo.
//!
//! Esempio:
//!   stemclean voce.wav -r basso.wav -r batteria.wav -r altro.wav --preset consigliato
//! crea voce_pulito.wav e voce_tolto.wav accanto allo stem (oppure nella cartella --uscita).

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use stemclean_core::{Engine, Mode, Params, Source, wav};

const AIUTO: &str = "\
StemClean DNR 2.0 — toglie dagli stem separati i rientri degli altri strumenti (motore Rust).

USO
  stemclean <stem.wav> [opzioni]

OPZIONI
  -r, --rif <file.wav>     stem di riferimento (ripetibile: vengono sommati)
  -p, --preset <nome>      delicato | consigliato (predefinito) | estremo | dnr
      --modo <m>           sc (sidechain) | exp (solo espansore) | both (combinata)
      --margine <dB>       margine del sidechain
      --pendenza <x>       pendenza del sidechain
      --taglio <dB>        attenuazione massima
      --soglia <dB>        soglia dell'espansore sopra il fondo
      --rapporto <x>       rapporto di espansione (1:x)
      --attacco <ms>       attacco degli inviluppi
      --rilascio <ms>      rilascio degli inviluppi
  -b, --bit <24|32>        formato dei WAV in uscita (predefinito 24)
  -o, --uscita <cartella>  dove salvare (predefinito: la cartella dello stem)
      --solo-pulito        non salvare il file \"_tolto\"
  -h, --aiuto              questo messaggio

Accetta WAV PCM 8/16/24/32 bit e float 32/64 bit, mono o stereo. I riferimenti
devono avere la stessa frequenza di campionamento dello stem.";

struct Options {
    stem: PathBuf,
    refs: Vec<PathBuf>,
    params: Params,
    bits: wav::Bits,
    out_dir: Option<PathBuf>,
    only_clean: bool,
}

fn parse_args(args: Vec<String>) -> Result<Option<Options>, String> {
    let mut stem = None;
    let mut refs = Vec::new();
    let mut params = Params::preset("consigliato").unwrap();
    let mut overrides: Vec<(String, String)> = Vec::new();
    let mut bits = wav::Bits::Int24;
    let mut out_dir = None;
    let mut only_clean = false;
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().ok_or(format!("manca il valore dopo {name}"));
        match a.as_str() {
            "-h" | "--aiuto" | "--help" => return Ok(None),
            "-r" | "--rif" => refs.push(PathBuf::from(value(&a)?)),
            "-p" | "--preset" => {
                let v = value(&a)?;
                params = Params::preset(&v).ok_or(format!("preset sconosciuto: {v}"))?;
            }
            "-b" | "--bit" => {
                let v = value(&a)?;
                bits = v
                    .parse()
                    .ok()
                    .and_then(wav::Bits::from_number)
                    .ok_or("--bit accetta 24 o 32")?;
            }
            "-o" | "--uscita" => out_dir = Some(PathBuf::from(value(&a)?)),
            "--solo-pulito" => only_clean = true,
            "--modo" | "--margine" | "--pendenza" | "--taglio" | "--soglia" | "--rapporto"
            | "--attacco" | "--rilascio" => {
                let v = value(&a)?;
                overrides.push((a, v));
            }
            s if s.starts_with('-') => return Err(format!("opzione sconosciuta: {s}")),
            _ if stem.is_none() => stem = Some(PathBuf::from(a)),
            _ => return Err(format!("un solo stem per volta (in più: {a})")),
        }
    }
    // le regolazioni singole valgono sopra il preset, in qualunque ordine siano scritte
    for (name, v) in overrides {
        if name == "--modo" {
            params.mode = Mode::parse(&v).ok_or(format!("modo sconosciuto: {v}"))?;
            continue;
        }
        let x: f64 = v
            .replace(',', ".")
            .parse()
            .map_err(|_| format!("{name}: numero non valido ({v})"))?;
        match name.as_str() {
            "--margine" => params.margin = x,
            "--pendenza" => params.slope = x,
            "--taglio" => params.cut = x.abs(),
            "--soglia" => params.thr = x,
            "--rapporto" => params.ratio = x,
            "--attacco" => params.att = x,
            _ => params.rel = x,
        }
    }
    let stem = stem.ok_or("indica lo stem da pulire (stemclean -h per l'aiuto)")?;
    Ok(Some(Options {
        stem,
        refs,
        params,
        bits,
        out_dir,
        only_clean,
    }))
}

fn read_wav(path: &Path) -> Result<wav::Decoded, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    wav::decode(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

fn run(o: Options) -> Result<(), String> {
    let started = Instant::now();
    let stem = read_wav(&o.stem)?;
    let sr = stem.sample_rate;
    let mut engine = Engine::with_stem(sr as f64, &stem.left, &stem.right);
    for r in &o.refs {
        let d = read_wav(r)?;
        if d.sample_rate != sr {
            return Err(format!(
                "{}: è a {} Hz, lo stem a {sr} Hz — convertili alla stessa frequenza",
                r.display(),
                d.sample_rate
            ));
        }
        engine.add_reference(&d.left, &d.right);
    }
    let secs = stem.left.len() as f64 / sr as f64;
    eprintln!(
        "Stem: {} — {:.0}:{:04.1}, {} Hz, {} can. · riferimenti: {}",
        o.stem.display(),
        (secs / 60.0).floor(),
        secs % 60.0,
        sr,
        stem.channels,
        o.refs.len()
    );
    if o.refs.is_empty() && o.params.mode != Mode::Expander {
        eprintln!("Nessun riferimento: uso solo l'espansore.");
    }
    let mut last = String::new();
    engine.process(&o.params, |stage, f| {
        let line = format!("{stage} {:>3.0}%", f * 100.0);
        if line != last {
            eprint!("\r{line:<40}");
            last = line;
        }
    });
    eprintln!("\r{:<40}", "elaborazione completata");

    let m = engine.metrics;
    println!(
        "Energia tolta: {:.1} dB · perdita media sul canto forte: {:+.2} dB · 1% peggiore: {:+.1} dB",
        m.removed_db, m.sing_mean, m.sing_worst
    );
    if m.sing_worst < -2.0 || m.sing_mean < -0.5 {
        println!(
            "Attenzione: lo stem perde troppo nei passaggi forti, prova il preset \"delicato\"."
        );
    }

    let dir = o
        .out_dir
        .clone()
        .unwrap_or_else(|| o.stem.parent().map(Path::to_path_buf).unwrap_or_default());
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let base = o
        .stem
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "stem".into());
    let mut outputs = vec![(Source::Clean, "pulito")];
    if !o.only_clean {
        outputs.push((Source::Removed, "tolto"));
    }
    for (src, suffix) in outputs {
        let (l, r) = engine.channels(src);
        let path = dir.join(format!("{base}_{suffix}.wav"));
        std::fs::write(&path, wav::encode(l, r, sr, o.bits))
            .map_err(|e| format!("{}: {e}", path.display()))?;
        println!("Salvato: {}", path.display());
    }
    eprintln!("Tempo totale: {:.2} s", started.elapsed().as_secs_f64());
    Ok(())
}

fn main() -> ExitCode {
    match parse_args(std::env::args().skip(1).collect()) {
        Ok(None) => {
            println!("{AIUTO}");
            ExitCode::SUCCESS
        }
        Ok(Some(o)) => match run(o) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("Errore: {e}");
                ExitCode::FAILURE
            }
        },
        Err(e) => {
            eprintln!("Errore: {e}");
            ExitCode::from(2)
        }
    }
}
