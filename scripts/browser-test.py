"""Prova della pagina in Chrome headless: carica i file, elabora, controlla il grafico e scarica i WAV.

Uso: python3 scripts/browser-test.py dist/index.html <stem.wav> <rif1.wav> [rif2.wav ...]
"""
import sys, time, pathlib
from playwright.sync_api import sync_playwright

page_path, stem, *refs = sys.argv[1:]
out = pathlib.Path('/tmp/browser-test'); out.mkdir(exist_ok=True)
errors = []
with sync_playwright() as p:
    b = p.chromium.launch(executable_path='/usr/bin/google-chrome', args=['--autoplay-policy=no-user-gesture-required'])
    pg = b.new_page(viewport={'width': 1300, 'height': 1000}, accept_downloads=True)
    pg.on('pageerror', lambda e: errors.append(str(e)))
    pg.on('console', lambda m: m.type == 'error' and errors.append(m.text))
    pg.goto(pathlib.Path(page_path).resolve().as_uri())
    pg.wait_for_function('StemCleanRust.ready', timeout=15000)
    print('motore:', pg.inner_text('#engineTag'))
    pg.set_input_files('#fileT', stem)
    pg.set_input_files('#fileR', refs)
    t0 = time.time()
    pg.click('#go')
    pg.wait_for_function("document.querySelector('#status').textContent.startsWith('Fatto') || document.querySelector('#status').textContent.startsWith('Errore')", timeout=180000)
    print(f'stato: {pg.inner_text("#status")}  ({time.time() - t0:.1f} s compresa la decodifica)')
    print('metriche:', pg.inner_text('#m1'), '|', pg.inner_text('#m2'), '|', pg.inner_text('#m3'))
    pg.wait_for_timeout(500)
    pg.locator('#cv').screenshot(path=str(out / 'spettrogramma.png'))
    pg.select_option('#view', 'red')
    pg.wait_for_timeout(300)
    pg.locator('#cv').screenshot(path=str(out / 'riduzione.png'))
    # ascolto A/B: play, cambio sorgente, pausa
    pg.click('#play'); pg.wait_for_timeout(700); pg.keyboard.press('3'); pg.wait_for_timeout(400); pg.click('#play')
    print('tempo player:', pg.inner_text('#time'))
    # ri-elaborazione con un altro preset (usa l'analisi in memoria)
    t1 = time.time()
    pg.click('#presets button[data-p="estremo"]'); pg.click('#go')
    pg.wait_for_function("document.querySelector('#status').textContent.startsWith('Fatto')", timeout=120000)
    print(f'secondo passaggio (Estremo): {time.time() - t1:.1f} s · metriche:', pg.inner_text('#m1'), '|', pg.inner_text('#m2'), '|', pg.inner_text('#m3'))
    for bits in ['24', '32']:
        pg.select_option('#bits', bits)
        with pg.expect_download() as d:
            pg.click('#dlClean')
        f = out / f'pulito_{bits}.wav'
        d.value.save_as(f)
        print(f'scaricato {d.value.suggested_filename} ({bits} bit): {f.stat().st_size} byte')
    pg.screenshot(path=str(out / 'pagina.png'), full_page=True)
    b.close()
print('errori nella pagina:', errors or 'nessuno')
