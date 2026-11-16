#!/usr/bin/env python3
# File:      fake_wallet.py
# Location:  app/tests/fake_wallet/fake_wallet.py
# Version:   v0.3.0
# Purpose:   Stand-in wallet JSON-RPC for UI checks without a Swarm. Serves the engine-dumped
#            fixtures (app/tests/fixtures) for substates.get, fixed balances, derived keys, and
#            accepts transactions without changing state (so the backend reports
#            COMMITTED / UNVERIFIED). It never signs, never stores anything, listens on 127.0.0.1 only.
# Usage:     python3 -I fake_wallet.py --port 12099 --fixtures ../fixtures
import argparse, json, pathlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ap = argparse.ArgumentParser()
ap.add_argument('--port', type=int, default=12099)
ap.add_argument('--clear-offer', action='store_true', help='serve the session with no pending offer')
ap.add_argument('--fixtures', default=str(pathlib.Path(__file__).resolve().parent.parent / 'fixtures'))
args = ap.parse_args()
FX = pathlib.Path(args.fixtures)
ptr = json.loads((FX / 'pointers.json').read_text())
game = ptr['games'][0]
SUB = {ptr['registry']['component']: 'registry', ptr['lobby']['component']: 'lobby',
       ptr['treasury']['component']: 'treasury', game['session']['component']: 'session'}
NAMES = {s['component']: s['account_name'] for s in game['seats']}
NAMES[game['house']] = game['house_account_name']
NAMES[game['community']] = 'OGE-test-community'
NAMES[game['chance']] = 'OGE-test-chance'
NAMES[ptr['treasury']['global_house']] = ptr.get('operator_account_name', 'OGE-test-operator')
KEYS = {n: i for i, n in enumerate(NAMES.values(), start=19)}
BAL = {'OGE-test-operator': 5000000000, game['house_account_name']: 812450000, 'OGE-test-community': 41200000, 'OGE-test-chance': 39870000}
for k, s in enumerate(game['seats']):
    BAL[s['account_name']] = [142350000, 96120000, 3250000, 77015000][k % 4]
TX = {'n': 0}

def handle(method, p):
    if method == 'auth.request':
        return {'token': 'fake-token'}
    if method == 'substates.get':
        name = SUB.get(p.get('substate_id'))
        if not name:
            raise KeyError('substate not found')
        doc = json.loads((FX / f'{name}.json').read_text())
        if name == 'session' and args.clear_offer:
            st = doc['substate_from_remote']['substate']['Component']['body']['state']
            st[36], st[37], st[38], st[39] = 65535, 0, 0, 0
        return doc
    if method == 'accounts.get':
        key = p.get('name_or_address')
        name = NAMES.get(key, key)
        if name not in KEYS:
            raise KeyError('account not found')
        return {'account': {'name': name, 'owner_key_id': {'Derived': {'index': KEYS[name], 'key_branch': 'account'}}}}
    if method == 'accounts.get_balances':
        return {'balances': [{'resource_address': ptr['resource'], 'balance': str(BAL.get(p.get('account'), 0))}]}
    if method == 'transactions.submit_manifest':
        if p.get('dry_run'):
            return {'result': {'finalize': {'result': {'Accept': {}}}}}
        TX['n'] += 1
        return {'transaction_id': f'{TX["n"]:064x}'}
    if method == 'transactions.wait_result':
        return {'status': 'Accepted', 'timed_out': False, 'final_fee': 4812, 'result': {'result': {'Accept': {}}}}
    if method == 'transactions.get':
        return {'transaction_id': p.get('transaction_id'), 'status': 'Accepted', 'note': 'fake wallet'}
    raise KeyError(f'method {method} not implemented by the fake wallet')

class H(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get('Content-Length', 0))) or b'{}')
        try:
            out = {'jsonrpc': '2.0', 'id': body.get('id'), 'result': handle(body.get('method'), body.get('params') or {})}
        except Exception as e:
            out = {'jsonrpc': '2.0', 'id': body.get('id'), 'error': {'code': -32000, 'message': str(e)}}
        data = json.dumps(out).encode()
        self.send_response(200); self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(data))); self.end_headers(); self.wfile.write(data)

ThreadingHTTPServer(('127.0.0.1', args.port), H).serve_forever()
# End of app/tests/fake_wallet/fake_wallet.py
# Location: app/tests/fake_wallet/fake_wallet.py · Created: 2026-11-16 04:06:01 UTC
