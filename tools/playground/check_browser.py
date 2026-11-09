#!/usr/bin/env python3
"""Check the actual generated WebAssembly UI in a fresh Chromium context."""
import argparse
import functools
import hashlib
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import threading
from playwright.sync_api import sync_playwright


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--directory', type=Path, default=Path('target/playground'))
    ap.add_argument('--output', type=Path, required=True)
    ap.add_argument('--chromium', help='optional system Chromium; otherwise use Playwright browser')
    a = ap.parse_args()
    a.output.mkdir(parents=True, exist_ok=False)
    handler = functools.partial(SimpleHTTPRequestHandler, directory=str(a.directory.resolve()))
    server = ThreadingHTTPServer(('127.0.0.1', 0), handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    resources, errors = [], []
    try:
        with sync_playwright() as pw:
            launch = {'headless': True, 'args': ['--no-sandbox', '--disable-dev-shm-usage']}
            if a.chromium:
                launch['executable_path'] = a.chromium
            browser = pw.chromium.launch(**launch)
            context = browser.new_context(viewport={'width': 1280, 'height': 1000})
            page = context.new_page()
            page.on('pageerror', lambda error: errors.append(str(error)))
            page.on('response', lambda response: resources.append({'url': response.url, 'status': response.status}))
            page.goto(f'http://127.0.0.1:{server.server_port}/')
            page.locator('#x0').filter(has_text='0x0000000000000000').wait_for()
            assert any(r['url'].endswith('.wasm') and r['status'] == 200 for r in resources)
            context.set_offline(True)  # Every following state change must execute locally.
            page.get_by_role('button', name='Step', exact=True).click()
            assert page.locator('#x0').inner_text() == '0x0000000000000001'
            assert page.locator('#pc').inner_text() == '0x0000000000001004'
            page.get_by_role('button', name='Step', exact=True).click()
            assert page.locator('#x0').inner_text() == '0x0000000000000003'
            page.screenshot(path=str(a.output / 'integer-step.png'), full_page=True)
            page.get_by_role('button', name='Step', exact=True).click()
            assert 'SVC #0' in page.locator('#status').inner_text()
            assert page.get_by_role('button', name='Step', exact=True).is_disabled()
            page.locator('#program').fill('not-hex')
            page.get_by_role('button', name='Load program', exact=True).click()
            assert '32-bit hexadecimal' in page.locator('#error').inner_text()
            assert page.locator('#x0').inner_text() == '0x0000000000000003'
            page.locator('#example').select_option('nan')
            page.get_by_role('button', name='Step', exact=True).click()
            assert page.locator('#v0').inner_text() == '0x7ff80000000000007ff8000000000000'
            page.screenshot(path=str(a.output / 'vector-step.png'), full_page=True)
            page.get_by_role('button', name='Reset', exact=True).click()
            assert page.locator('#steps').inner_text() == '0'
            assert page.locator('#v2').inner_text() == '0x7ff00000000000007ff0000000000000'
            page.locator('#example').select_option('loop')
            for _ in range(10):
                page.get_by_role('button', name='Run 1000', exact=True).click()
            assert page.locator('#steps').inner_text() == '10000'
            assert 'budget' in page.locator('#status').inner_text()
            assert page.get_by_role('button', name='Run 1000', exact=True).is_disabled()
            page.set_viewport_size({'width': 390, 'height': 844})
            page.locator('#example').select_option('add')
            page.get_by_role('button', name='Step', exact=True).click()
            assert page.locator('#x0').inner_text() == '0x0000000000000001'
            page.screenshot(path=str(a.output / 'mobile-step.png'), full_page=True)
            assert not errors, errors
            report = {
                'browser': browser.version,
                'wasm_sha256': hashlib.sha256((a.directory / 'pkg/ounce_learn_bg.wasm').read_bytes()).hexdigest(),
                'execution_after_network_disabled': True,
                'checks': ['MOV/ADD actual register and PC changes', 'SVC stop', 'transactional invalid input',
                           'hardware vector case', 'reset preserves vector input', 'bounded loop', 'mobile step'],
                'page_errors': errors, 'resources': resources,
            }
            (a.output / 'acceptance.json').write_text(json.dumps(report, indent=2) + '\n')
            browser.close()
    finally:
        server.shutdown()


if __name__ == '__main__':
    main()
