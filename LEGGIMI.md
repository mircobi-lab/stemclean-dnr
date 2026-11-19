# StemClean DNR 2.0 — motore in Rust

StemClean toglie dagli stem separati (per esempio il cantato uscito da MVSep) i rientri degli altri strumenti. Lavora banda per banda, su 27 bande da 1/3 d'ottava, con un principio simile all'espansore dei sistemi Dolby/DNR.

Dalla versione 2.0 tutto il calcolo è scritto in Rust. Lo stesso codice gira in tre forme:

| Forma | Dove | Cosa contiene |
|---|---|---|
| Pagina web | `dist/index.html` | Un solo file, con il motore Rust incorporato in WebAssembly |
| App Mac | `StemClean DNR.app` / `.dmg` | La stessa pagina in una finestra nativa (Tauri 2). I file si salvano dove scegli tu |
| Terminale | `stemclean` | Il motore in codice nativo, per pulire i WAV senza interfaccia |

## Struttura

```
crates/stemclean-core   motore: FFT, bande, analisi, guadagni, metriche, spettrogramma, WAV, ZIP
crates/stemclean-wasm   lo stesso motore esportato in WebAssembly (nessuna dipendenza esterna)
crates/stemclean-cli    programma da Terminale
web/                    interfaccia (HTML) e collegamento JS → WebAssembly
src-tauri/              app Mac
scripts/                script di build e di verifica
```

Nella pagina JavaScript restano solo le cose che il browser fa da sé: la decodifica di MP3/FLAC/M4A, l'ascolto A/B e il disegno sul canvas.

## Preparare il Mac (una volta sola)

1. Strumenti di Apple: `xcode-select --install`
2. Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`, poi chiudi e riapri il Terminale
3. Node.js LTS da https://nodejs.org (serve per lo strumento di Tauri)

## Compilare

```bash
./scripts/build-web.sh             # solo la pagina → dist/index.html
./scripts/build-mac.sh             # app per questo Mac → src-tauri/target/release/bundle/
./scripts/build-mac.sh universal   # app unica per Mac Intel e Apple Silicon
./scripts/build-cli.sh             # programma da Terminale → ~/bin/stemclean
cargo test -p stemclean-core       # test del motore
```

La prima compilazione dell'app scarica le librerie di Tauri e richiede qualche minuto. Le successive sono rapide.

L'app non è firmata da Apple, quindi alla prima apertura macOS la blocca. Si sblocca così:

```bash
xattr -dr com.apple.quarantine "/Applications/StemClean DNR.app"
```

Con GitHub, il workflow `.github/workflows/build-mac.yml` crea il `.dmg` universale. Si avvia dalla scheda Actions ("Run workflow") oppure pubblicando un tag `v2.0.0`.

## Programma da Terminale

```bash
stemclean voce.wav -r basso.wav -r batteria.wav -r altro.wav            # preset Consigliato
stemclean voce.wav -r altro.wav -p delicato -b 32 -o ~/Desktop/puliti
stemclean voce.wav --modo exp                                             # solo espansore
stemclean -h                                                              # tutte le opzioni
```

Crea `voce_pulito.wav` e `voce_tolto.wav`. Accetta WAV a 8, 16, 24 o 32 bit interi e WAV float a 32 o 64 bit. I riferimenti devono avere la stessa frequenza di campionamento dello stem.

## Verifiche

- `node scripts/parity.cjs <stemclean v1.html> <stemclean_wasm.wasm>` confronta il motore Rust con quello JavaScript della versione 1. Su tutti i preset la differenza massima è 9·10⁻⁸, circa −141 dBFS: è un solo passo di arrotondamento del formato float a 32 bit.
- Il programma da Terminale (nativo) e la pagina (WebAssembly) producono WAV identici bit per bit.
- `python3 scripts/browser-test.py dist/index.html stem.wav rif.wav` prova la pagina in Chrome senza interfaccia: carica i file, elabora, disegna il grafico e scarica i WAV.

MBStudio91 · Piedmont Electronic Sound


## Aspetto

Interfaccia nello stile hardware MBStudio: telaio nero, scritte bianche stampate, LED, tasti in gomma. Font Bebas Neue (© Dharma Type) e Antonio (© Vernon Adams) incorporati nella pagina, SIL Open Font License 1.1, quindi funziona anche senza internet. Logo MBSTUDIO91: marchio di MBStudio91.
