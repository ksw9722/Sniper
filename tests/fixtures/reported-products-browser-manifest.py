#!/usr/bin/env python3
"""Synchronize production map DOM and record inert product fixture provenance."""
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
source = (ROOT / 'web/index.html').read_text()
start = source.index('          <section class="service-map-workspace"')
end = source.index('          <section class="panel-card hidden" id="targetPathsPanel"', start)
fixture = HERE / 'reported-products-browser.html'
html = fixture.read_text()
a = html.index('          <section class="service-map-workspace"')
b = html.index('<details class="fixture-log"', a)
fixture.write_text(html[:a] + source[start:end] + html[b:])
paths = ['web/service-map.js', 'web/styles.css', 'web/index.html',
         'tests/fixtures/reported-products-browser.html', 'tests/fixtures/reported-products-browser.js',
         'tests/fixtures/reported-products-browser.css', 'tests/fixtures/reported-products-browser.README.md',
         'tests/fixtures/reported-products-browser-manifest.py']
manifest = {'fixture': 'Reported product and literal-version filters',
            'production_controller': 'web/service-map.js', 'production_styles': 'web/styles.css',
            'production_dom': 'targetServiceMapPanel extracted from web/index.html',
            'app_bootstrap': False, 'synthetic_origins': 29, 'reference_only_origins': 1, 'metadata_redirects': 29,
            'primary_endpoints': 130, 'loaded_endpoints': 354, 'explicit_peer_origin_groups': 30,
            'networking': 'CSP connect-src none; guarded browser connections; closed in-memory GET adapter',
            'ordinary_map_errors': ['busy', 'scope_work_limit', 'session_changed', 'scope_changed', 'history_changed', 'build_failed', 'unknown', 'duplicate', 'mismatch', 'missing'],
            'scenarios': ['base', 'reordered', 'disappeared-product', 'disappeared-version', 'partial', 'legacy', 'present-empty', 'field-missing', 'empty'],
            'sha256': {path: hashlib.sha256((ROOT / path).read_bytes()).hexdigest() for path in paths}}
(HERE / 'reported-products-browser.manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
print('Synchronized production map DOM and reported-product fixture provenance.')
