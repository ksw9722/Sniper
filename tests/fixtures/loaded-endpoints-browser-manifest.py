#!/usr/bin/env python3
"""Copy only the production map DOM and record synthetic fixture provenance."""
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
source = (ROOT / 'web/index.html').read_text()
start = source.index('          <section class="service-map-workspace"')
end = source.index('          <section class="panel-card hidden" id="targetPathsPanel"', start)
fixture = HERE / 'loaded-endpoints-browser.html'
html = fixture.read_text()
a = html.index('          <section class="service-map-workspace"')
b = html.index('<details class="fixture-log"', a)
fixture.write_text(html[:a] + source[start:end] + html[b:])
paths = ['web/service-map.js', 'web/styles.css', 'web/index.html',
         'tests/fixtures/loaded-endpoints-browser.html', 'tests/fixtures/loaded-endpoints-browser.js',
         'tests/fixtures/loaded-endpoints-browser.css', 'tests/fixtures/loaded-endpoints-browser.README.md',
         'tests/fixtures/loaded-endpoints-browser-manifest.py']
manifest = {'fixture': 'Loaded endpoint filters and exact API pagination',
            'production_controller': 'web/service-map.js', 'production_styles': 'web/styles.css',
            'production_dom': 'targetServiceMapPanel extracted from web/index.html',
            'app_bootstrap': False, 'networking': 'CSP connect-src none and closed synthetic request adapter',
            'sha256': {path: hashlib.sha256((ROOT / path).read_bytes()).hexdigest() for path in paths}}
(HERE / 'loaded-endpoints-browser.manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
print('Synchronized production map DOM and fixture provenance.')
