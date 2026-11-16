#!/usr/bin/env python3
# Ootle Game Engine | scripts/deploy_local.py
# Version: v0.3.0
# Deployment of the four-contract Ootle game engine and its pool of games (default 5) through a
# wallet daemon on this machine (local Swarm, or Esmeralda through the server's own wallet daemon).
#
# What it does, in order, journaling every step in the state file (default deployments/bootstrap-state.json):
#   1. creates new wallet accounts: the main House (operator) and global Community/Chance, and per game
#      its House/Community/Chance (separate mode only) and one account per seat; requests faucet funds;
#   2. publishes the four templates (Registry, Lobby, Treasury, Town Square);
#   3. instantiates Registry, Treasury and Lobby and adds the "Town Square Classic" game type;
#   4. per game: creates it in the Lobby, instantiates its session, attaches it and registers it with
#      the Treasury, sets its economics (v0.3.0), adds every seat account, registers and claims each seat
#      (signed by that seat's own key), settles each seat's cash to the seat start (v0.3.0: the excess
#      faucet funds go to the game House, or the game House tops the seat up), and starts the game;
#   5. verifies all components remotely and writes the pointers file (default deployments/local-swarm.json).
#
# Economics (v0.3.0, set per game with Town Square `set_economics`, changeable later by the operator):
#   defaults keep v0.2.0 play except build on landing, which is on (agreed rule); GO not scaled,
#   rent fee 0, fee and penalty squares 1.000000 / 2.000000. See --help.
#
# Safety: every WASM still to publish is checked before the DEPLOY prompt, so nothing is created or
# funded without it; one explicit DEPLOY confirmation; every submitted transaction id is journaled
# before waiting; an unknown outcome stops the run instead of resubmitting; finished steps are skipped.
# A journal written by an earlier version is refused (use a fresh deployments/ folder).
# Earlier deployments (v0.6.18, v0.1.x, v0.2.0) are never read or changed.
#
# Test hook: `--list-steps` and `--render STEP --context FILE` print the exact manifests this
# script submits, so engine-tests can execute them in the Tari engine harness.

import argparse
import base64
import json
import os
import re
import secrets
import sys
import time
from pathlib import Path
from urllib.request import Request, urlopen

VERSION = '0.3.0'
ROOT = Path(__file__).resolve().parent.parent
DEPLOY = ROOT / 'deployments'
RPC = os.environ.get('OOTLE_WALLET_RPC', 'http://127.0.0.1:12010/json_rpc')
RESOURCE = 'resource_' + '01' * 32          # tTARI (native token)
FEE = 2_000_000                             # max fee per transaction, base units
PUBLISH_FEE = 3_000_000

CONTRACTS = {
    'registry': ('contracts/registry', 'ootle_registry', 'OotleRegistry'),
    'lobby': ('contracts/lobby', 'ootle_lobby', 'OotleLobby'),
    'treasury': ('contracts/treasury', 'ootle_treasury', 'OotleTreasury'),
    'town_square': ('contracts/town_square', 'ootle_town_square', 'OotleTownSquare'),
}

# Town Square Classic parameters, base units (6 decimals). Same values as v0.6.18 plus Maintenance.
GAME_TYPE_NAME = 'Town Square Classic'
PARAMS = {
    'go_reward': 20_000_000, 'square_reward': 2_000_000, 'fee_amount': 1_000_000,
    'penalty_amount': 2_000_000, 'shop_fee': 5_000_000, 'storage_fee': 2_000_000,
    'house_cost': 5_000_000, 'hotel_cost': 15_000_000, 'buyback_pct': 80,
}
PARAM_ORDER = ['go_reward', 'square_reward', 'fee_amount', 'penalty_amount', 'shop_fee',
               'storage_fee', 'house_cost', 'hotel_cost', 'buyback_pct']
TYPE_ID = 1
DEFAULT_ECONOMICS = {'go_floor': PARAMS['go_reward'], 'go_scaled': False, 'rent_fee_bps': 0,
                     'build_on_landing': True, 'fee_amount': PARAMS['fee_amount'],
                     'penalty_amount': PARAMS['penalty_amount']}
SEAT_START = 20_000_000


def units(text):
    """'20' or '0.5' tTARI → base units (exact, up to 6 decimals)."""
    m = re.fullmatch(r'(\d+)(?:\.(\d{1,6}))?', text.strip())
    if not m:
        raise argparse.ArgumentTypeError(f'invalid tTARI amount: {text}')
    return int(m.group(1)) * 1_000_000 + int((m.group(2) or '').ljust(6, '0') or 0)


# ---------------------------------------------------------------- manifests (pure)

def fee_main(var='fee'):
    return f'fn fee_main() {{ let {var} = var!["{var}"]; {var}.pay_fee({FEE}u64); }}\n'


def param_args():
    return ', '.join(f'{PARAMS[k]}u8' if k == 'buyback_pct' else f'{PARAMS[k]}u64' for k in PARAM_ORDER)


def m_new(template_hex, alias, ctor, args):
    return (f'use template_{template_hex} as {alias};\n' + fee_main() +
            f'fn main() {{ {alias}::{ctor}({args}); }}\n')


def m_calls(calls):
    """calls: list of (variable, method, args). Each target is bound with var![...]."""
    names = []
    for var, _, _ in calls:
        if var not in names:
            names.append(var)
    lets = ' '.join(f'let {v} = var!["{v}"];' for v in names)
    body = ' '.join(f'{v}.{m}({a});' for v, m, a in calls)
    return fee_main() + f'fn main() {{ {lets} {body} }}\n'


def addr(s):
    return f'address!("{s}")'


def b(v):
    return 'true' if v else 'false'


def step_names(players, games=1):
    names = ['init:registry', 'init:treasury', 'init:lobby', 'lobby:add_type']
    for g in range(1, games + 1):
        names += [f'lobby:create_game:{g}', f'init:session:{g}', f'setup:attach_register:{g}', f'setup:economics:{g}']
        names += [f'setup:add_seat:{g}:{i}' for i in range(1, players + 1)]
        names += [f'seat:register_claim:{g}:{i}' for i in range(1, players + 1)]
        names += [f'seat:settle:{g}:{i}' for i in range(1, players + 1)]
        names += [f'game:start:{g}']
    return names


def pools(ctx, g):
    a = ctx['accounts']
    if ctx['treasury_mode'] == 'separate':
        return a[f'g{g}_house'], a[f'g{g}_community'], a[f'g{g}_chance']
    return a['global_house'], a['global_community'], a['global_chance']


def render(step, ctx):
    """Returns dict(manifest, variables, signer, creates[, skip]). `signer` is 'operator', 'seat:G:I'
    (seal and fee by the seat) or 'house+seat:G:I' (seal and fee by game G's House, seat key also signs).
    `creates` names the component an instantiation step produces (or None)."""
    t, c, a = ctx['templates'], ctx.get('components', {}), ctx['accounts']
    op_fee = {'fee': a['operator']}
    econ = {**DEFAULT_ECONOMICS, **ctx.get('economics', {})}
    if step == 'init:registry':
        return dict(manifest=m_new(t['registry'], 'Registry', 'new', ''), variables=op_fee,
                    signer='operator', creates='registry')
    if step == 'init:treasury':
        args = ', '.join([addr(ctx['resource']), addr(a['global_house']), addr(a['global_community']),
                          addr(a['global_chance'])])
        return dict(manifest=m_new(t['treasury'], 'Treasury', 'new', args), variables=op_fee,
                    signer='operator', creates='treasury')
    if step == 'init:lobby':
        args = f'{addr(c["registry"])}, {addr(c["treasury"])}'
        return dict(manifest=m_new(t['lobby'], 'Lobby', 'new', args), variables=op_fee,
                    signer='operator', creates='lobby')
    if step == 'lobby:add_type':
        return dict(manifest=m_calls([('lobby', 'add_game_type', f'"{GAME_TYPE_NAME}", {param_args()}')]),
                    variables={**op_fee, 'lobby': c['lobby']}, signer='operator', creates=None)
    m = re.fullmatch(r'lobby:create_game:(\d+)', step)
    if m:
        return dict(manifest=m_calls([('lobby', 'create_game', f'{TYPE_ID}u32, {ctx["players"]}u8')]),
                    variables={**op_fee, 'lobby': c['lobby']}, signer='operator', creates=None)
    m = re.fullmatch(r'init:session:(\d+)', step)
    if m:
        g = int(m.group(1))
        args = (f'{g}u64, {addr(c["lobby"])}, {addr(c["treasury"])}, {addr(ctx["resource"])}, '
                f'{ctx["players"]}u8, {param_args()}')
        return dict(manifest=m_new(t['town_square'], 'TownSquare', 'new', args), variables=op_fee,
                    signer='operator', creates=f'session{g}')
    m = re.fullmatch(r'setup:attach_register:(\d+)', step)
    if m:
        g = int(m.group(1))
        h, co, ch = pools(ctx, g)
        flag = b(ctx['treasury_mode'] != 'separate')
        s = c[f'session{g}']
        calls = [('lobby', 'attach_session', f'{g}u64, {addr(s)}'),
                 ('treasury', 'register_game', f'{g}u64, {addr(s)}, {addr(h)}, {addr(co)}, {addr(ch)}, {flag}, {flag}, {flag}')]
        return dict(manifest=m_calls(calls), variables={**op_fee, 'lobby': c['lobby'], 'treasury': c['treasury']},
                    signer='operator', creates=None)
    m = re.fullmatch(r'setup:economics:(\d+)', step)
    if m:
        g = int(m.group(1))
        args = (f'{econ["go_floor"]}u64, {b(econ["go_scaled"])}, {econ["rent_fee_bps"]}u16, '
                f'{b(econ["build_on_landing"])}, {econ["fee_amount"]}u64, {econ["penalty_amount"]}u64')
        return dict(manifest=m_calls([('session', 'set_economics', args)]),
                    variables={**op_fee, 'session': c[f'session{g}']}, signer='operator', creates=None)
    m = re.fullmatch(r'setup:add_seat:(\d+):(\d+)', step)
    if m:
        g, i = int(m.group(1)), int(m.group(2))
        return dict(manifest=m_calls([('session', 'add_seat', addr(a[f'g{g}_seat{i}']))]),
                    variables={**op_fee, 'session': c[f'session{g}']}, signer='operator', creates=None)
    m = re.fullmatch(r'seat:register_claim:(\d+):(\d+)', step)
    if m:
        g, i = int(m.group(1)), int(m.group(2))
        calls = [('registry', 'register', f'"Game {g} Seat {i}", {i}u8'),
                 ('lobby', 'claim_seat', f'{g}u64, {i}u8')]
        return dict(manifest=m_calls(calls),
                    variables={'fee': a[f'g{g}_seat{i}'], 'registry': c['registry'], 'lobby': c['lobby']},
                    signer=f'seat:{g}:{i}', creates=None)
    m = re.fullmatch(r'seat:settle:(\d+):(\d+)', step)
    if m:
        g, i = int(m.group(1)), int(m.group(2))
        start = ctx.get('seat_start', SEAT_START)
        cash = ctx.get('seat_cash', {}).get(f'g{g}_seat{i}')
        if cash is None:
            raise ValueError(f'{step}: seat cash not known')
        house = pools(ctx, g)[0]
        variables = {'house': house, 'seat': a[f'g{g}_seat{i}']}
        res = addr(ctx['resource'])
        if cash > start:
            body = (f'let house = var!["house"]; let seat = var!["seat"]; '
                    f'let excess = seat.withdraw({res}, {cash - start}u64); house.deposit(excess);')
        elif cash < start:
            body = (f'let house = var!["house"]; let seat = var!["seat"]; '
                    f'let top_up = house.withdraw({res}, {start - cash}u64); seat.deposit(top_up);')
        else:
            return dict(manifest='', variables=variables, signer=f'house+seat:{g}:{i}', creates=None, skip=True)
        return dict(manifest=fee_main('house') + f'fn main() {{ {body} }}\n', variables=variables,
                    signer=f'house+seat:{g}:{i}', creates=None)
    m = re.fullmatch(r'game:start:(\d+)', step)
    if m:
        g = int(m.group(1))
        calls = [('session', 'start', ''), ('lobby', 'mark_started', f'{g}u64')]
        return dict(manifest=m_calls(calls), variables={**op_fee, 'session': c[f'session{g}'], 'lobby': c['lobby']},
                    signer='operator', creates=None)
    raise ValueError(f'unknown step {step}')


# ---------------------------------------------------------------- wallet RPC

def save(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + '.tmp')
    tmp.write_text(json.dumps(data, indent=2) + '\n')
    tmp.replace(path)


def rpc(method, params, token=None):
    h = {'Content-Type': 'application/json'}
    if token:
        h['Authorization'] = 'Bearer ' + token
    payload = json.dumps({'jsonrpc': '2.0', 'id': 1, 'method': method, 'params': params}).encode()
    with urlopen(Request(RPC, data=payload, headers=h), timeout=180) as r:
        answer = json.load(r)
    if answer.get('error'):
        raise RuntimeError(f'{method}: {answer["error"]}')
    if 'result' not in answer:
        raise RuntimeError(f'{method}: missing result')
    return answer['result']


def auth():
    cookie = os.environ.get('OOTLE_WALLET_COOKIE_PATH')
    if cookie:
        token = Path(cookie).read_text().strip()
        if not token:
            raise RuntimeError('Empty wallet auth cookie')
        return token
    return rpc('auth.request', {'permissions': ['Admin'], 'credentials': 'None'})['token']


def component(s):
    if not isinstance(s, str) or not re.fullmatch(r'component_[0-9a-fA-F]{64}', s):
        raise ValueError(f'Invalid component address: {s}')
    return s


def template(s):
    if not isinstance(s, str) or not re.fullmatch(r'[0-9a-fA-F]{64}', s):
        raise ValueError(f'Invalid template address: {s}')
    return s


def receipt(tx, token):
    out = rpc('transactions.wait_result', {'transaction_id': tx, 'timeout_secs': 120}, token)
    if out.get('timed_out'):
        raise RuntimeError(f'Transaction {tx} timed out. STOP; inspect the on-chain outcome before retrying.')
    result = (out.get('result') or {}).get('result') or {}
    if out.get('status') != 'Accepted' or 'Accept' not in result:
        raise RuntimeError(f'Transaction {tx} not accepted: {str(out)[:1500]}')
    return out


def event(out, topic, template_filter=None):
    for x in (out.get('result') or {}).get('events', []):
        if x.get('topic') == topic and (template_filter is None or x.get('template_address') == template_filter):
            return x.get('substate_id') if topic == 'std.component.created' else x.get('template_address')
    raise RuntimeError('Accepted transaction missing expected event: ' + topic)


def journal_tx(state, key, submit, token, state_path):
    prior = state['transactions'].get(key)
    if prior:
        if prior.get('confirmed'):
            return prior['receipt']
        raise RuntimeError(f'{key}: transaction {prior["id"]} has unknown status. STOP; do not resubmit.')
    txid = submit()
    state['transactions'][key] = {'id': txid, 'confirmed': False}
    save(state_path, state)                 # persisted before waiting, so a timeout is never resubmitted
    out = receipt(txid, token)
    state['transactions'][key] = {'id': txid, 'confirmed': True, 'receipt': out}
    save(state_path, state)
    return out


def key_index(acc):
    d = (acc.get('owner_key_id') or {}).get('Derived')
    if not isinstance(d, dict) or d.get('key_branch') != 'account' or not isinstance(d.get('index'), int):
        raise RuntimeError(f'{acc["name"]}: wallet did not return a derived account key; refusing to sign')
    return d['index']


def derived(acc):
    return {'Derived': {'index': key_index(acc), 'key_branch': 'account'}}


def balance(name, token):
    r = rpc('accounts.get_balances', {'account': name, 'refresh': True}, token)
    for x in r.get('balances', []):
        if x.get('resource_address') == RESOURCE:
            return int(x.get('balance') or 0)
    return 0


def roles_of(state):
    roles = ['global_house', 'global_community', 'global_chance']
    for g in range(1, state['games'] + 1):
        if state['treasury_mode'] == 'separate':
            roles += [f'g{g}_house', f'g{g}_community', f'g{g}_chance']
        roles += [f'g{g}_seat{i}' for i in range(1, state['players'] + 1)]
    return roles


def submit_step(state, step, ctx, token, state_path):
    prior = state['transactions'].get(step)
    if prior and prior.get('confirmed'):
        return prior['receipt']
    if step.startswith('seat:settle:') and not prior:
        g, i = step.split(':')[2:]
        ctx = {**ctx, 'seat_cash': {f'g{g}_seat{i}': balance(state['accounts'][f'g{g}_seat{i}']['name'], token)}}
    r = render(step, ctx)
    if r.get('skip'):
        state['transactions'][step] = {'id': None, 'confirmed': True, 'receipt': {'skipped': 'seat already at the seat start'}}
        save(state_path, state)
        print(f'{step}: seat already holds the seat start; nothing to submit')
        return None
    signer = r['signer']
    acc = state['accounts']
    signing = []
    if signer == 'operator':
        seal = acc['global_house']
    elif signer.startswith('seat:'):
        g, i = signer.split(':')[1:]
        seal = acc[f'g{g}_seat{i}']
    else:
        g, i = signer.split(':')[1:]
        seal = acc[f'g{g}_house'] if state['treasury_mode'] == 'separate' else acc['global_house']
        signing = [derived(acc[f'g{g}_seat{i}'])]
    args = {'manifest': r['manifest'], 'variables': r['variables'], 'seal_signer_key_id': derived(seal),
            'signing_key_ids': signing, 'max_fee': FEE, 'dry_run': True, 'blobs': {}}
    if not prior:
        dry = rpc('transactions.submit_manifest', args, token)
        if 'Accept' not in ((dry.get('result') or {}).get('finalize') or {}).get('result', {}):
            raise RuntimeError(f'{step}: dry run rejected; nothing submitted: {str(dry)[:1500]}')
        print(f'{step}: dry run accepted')

    def go():
        args['dry_run'] = False
        return rpc('transactions.submit_manifest', args, token)['transaction_id']
    out = journal_tx(state, step, go, token, state_path)
    if r['creates']:
        kind = 'town_square' if r['creates'].startswith('session') else r['creates']
        comp = component(event(out, 'std.component.created', state['templates'][kind]))
        state['components'][r['creates']] = comp
        save(state_path, state)
        print(f'{step}: {r["creates"]} = {comp}')
    else:
        print(f'{step}: confirmed')
    return out


def create_account(state, role, token, state_path):
    if role in state['accounts']:
        return
    name = f'OGE-{state["tag"]}-{role}'
    state['creating'] = {'role': role, 'name': name}
    save(state_path, state)
    r = rpc('accounts.create', {'account_name': name, 'is_default': False}, token)
    a = r.get('account', r)
    state['accounts'][role] = {'name': name, 'component': component(a['component_address']),
                               'owner_key_id': a.get('owner_key_id')}
    state.pop('creating', None)
    save(state_path, state)
    print(f'Created account {role}: {state["accounts"][role]["component"]}')


def wait_on_chain(addr_, token, label):
    last = None
    for _ in range(30):
        try:
            if rpc('substates.get', {'substate_id': addr_}, token).get('substate_from_remote'):
                return
            last = 'remote substate absent'
        except RuntimeError as exc:
            last = str(exc)
        time.sleep(2)
    raise RuntimeError(f'{label} {addr_} not confirmed on-chain: {last}')


def checked_remote(addr_, expected_template, token):
    entry = rpc('substates.get', {'substate_id': component(addr_)}, token).get('substate_from_remote')
    if not entry:
        raise RuntimeError('Component not found remotely: ' + addr_)
    actual = entry['substate']['Component']['header']['template_address']
    if actual != template(expected_template):
        raise RuntimeError(f'Remote template mismatch for {addr_}: {actual}')
    return entry


def context(state):
    accounts = {k: v['component'] for k, v in state['accounts'].items()}
    accounts['operator'] = accounts['global_house']
    return {'templates': state['templates'], 'components': state['components'], 'accounts': accounts,
            'resource': RESOURCE, 'players': state['players'], 'games': state['games'],
            'treasury_mode': state['treasury_mode'], 'economics': state['economics'],
            'seat_start': state['seat_start']}


def write_pointers(state, token, pointers_path):
    tpl = state['templates']
    for role, key in [('registry', 'registry'), ('treasury', 'treasury'), ('lobby', 'lobby')]:
        checked_remote(state['components'][role], tpl[key], token)
    acc = state['accounts']
    separate = state['treasury_mode'] == 'separate'
    games = []
    for g in range(1, state['games'] + 1):
        checked_remote(state['components'][f'session{g}'], tpl['town_square'], token)
        p = (f'g{g}_house', f'g{g}_community', f'g{g}_chance') if separate else ('global_house', 'global_community', 'global_chance')
        games.append({
            'id': g, 'type_id': TYPE_ID, 'capacity': state['players'],
            'session': {'component': state['components'][f'session{g}'], 'template': tpl['town_square']},
            'treasury_mode': state['treasury_mode'],
            'house': acc[p[0]]['component'], 'community': acc[p[1]]['component'], 'chance': acc[p[2]]['component'],
            'house_account_name': acc[p[0]]['name'], 'community_account_name': acc[p[1]]['name'],
            'chance_account_name': acc[p[2]]['name'],
            'seats': [{'seat': i, 'account_name': acc[f'g{g}_seat{i}']['name'],
                       'component': acc[f'g{g}_seat{i}']['component'],
                       'key_index': key_index(acc[f'g{g}_seat{i}'])} for i in range(1, state['players'] + 1)],
        })
    pointers = {
        'network': state['network'],
        'engine_version': VERSION,
        'resource': RESOURCE,
        'operator_account_name': acc['global_house']['name'],
        'registry': {'component': state['components']['registry'], 'template': tpl['registry']},
        'treasury': {'component': state['components']['treasury'], 'template': tpl['treasury'],
                     'global_house': acc['global_house']['component'],
                     'global_community': acc['global_community']['component'],
                     'global_chance': acc['global_chance']['component']},
        'lobby': {'component': state['components']['lobby'], 'template': tpl['lobby']},
        'game_types': [{'id': TYPE_ID, 'name': GAME_TYPE_NAME, 'template': tpl['town_square'], 'params': PARAMS,
                        'economics': state['economics']}],
        'games': games,
    }
    save(pointers_path, pointers)


def wasm_paths(state):
    """WASM file for every contract whose template is not yet published, in publish order."""
    return {key: ROOT / path / 'target/wasm32-unknown-unknown/release' / f'{crate}.wasm'
            for key, (path, crate, _) in CONTRACTS.items() if key not in state['templates']}


def parse_args(argv=None):
    ap = argparse.ArgumentParser(description='Deploy the Ootle game engine (four contracts) and its pool of games.')
    ap.add_argument('--games', type=int, default=5, choices=range(1, 11), metavar='1..10', help='games in the pool (default 5)')
    ap.add_argument('--players', type=int, default=4, choices=range(2, 21), metavar='2..20', help='seats per game (default 4)')
    ap.add_argument('--treasury', choices=['separate', 'global'], default='separate',
                    help='each game\'s House/Community/Chance: its own accounts (separate, default; games run in '
                         'parallel) or the global pools')
    ap.add_argument('--network-name', default='Local Ootle Swarm', help='label written to the pointers file')
    ap.add_argument('--pointers', default=str(DEPLOY / 'local-swarm.json'), help='pointers file to write')
    ap.add_argument('--state', default=str(DEPLOY / 'bootstrap-state.json'), help='deployment journal')
    ap.add_argument('--seat-start', type=units, default=SEAT_START, help='cash each seat holds at start, tTARI (default 20)')
    ap.add_argument('--go-scaled', action='store_true', help='scale GO by unowned assets (default off)')
    ap.add_argument('--go-floor', type=units, default=None, help='GO floor when scaled, tTARI (default: the GO reward)')
    ap.add_argument('--rent-fee-bps', type=int, default=0, help='rent fee in basis points, 0..5000 (default 0)')
    ap.add_argument('--build-anywhere', action='store_true', help='turn the build-on-landing rule off')
    ap.add_argument('--fee-amount', type=units, default=PARAMS['fee_amount'], help='fee squares, tTARI (default 1)')
    ap.add_argument('--penalty-amount', type=units, default=PARAMS['penalty_amount'], help='penalty square, tTARI (default 2)')
    ap.add_argument('--no-faucet', action='store_true', help='do not request faucet funds')
    ap.add_argument('--verify-only', action='store_true', help='read and verify the recorded deployment; no writes')
    ap.add_argument('--list-steps', action='store_true', help=argparse.SUPPRESS)
    ap.add_argument('--render', metavar='STEP', help=argparse.SUPPRESS)
    ap.add_argument('--context', metavar='FILE', help=argparse.SUPPRESS)
    a = ap.parse_args(argv)
    if not 0 <= a.rent_fee_bps <= 5000:
        ap.error('--rent-fee-bps must be 0..5000')
    floor = PARAMS['go_reward'] if a.go_floor is None else a.go_floor
    if floor > PARAMS['go_reward']:
        ap.error('--go-floor cannot be above the GO reward')
    a.economics = {'go_floor': floor, 'go_scaled': a.go_scaled, 'rent_fee_bps': a.rent_fee_bps,
                   'build_on_landing': not a.build_anywhere, 'fee_amount': a.fee_amount,
                   'penalty_amount': a.penalty_amount}
    return a


def main():
    a = parse_args()
    if a.list_steps:
        print(json.dumps(step_names(a.players, a.games)))
        return
    if a.render:
        print(json.dumps(render(a.render, json.loads(Path(a.context).read_text()))))
        return

    if not (RPC.startswith('http://127.0.0.1:') or RPC.startswith('http://localhost:')):
        raise RuntimeError('Deployment requires a loopback wallet RPC (the wallet daemon on this machine)')
    state_path, pointers_path = Path(a.state), Path(a.pointers)
    token = auth()

    if state_path.exists():
        state = json.loads(state_path.read_text())
        if state.get('version') != VERSION:
            raise RuntimeError(f'{state_path} was written by an earlier deploy script (version '
                               f'{state.get("version", "0.2.0 or older")}). Move the deployments/ folder aside or '
                               f'pass --state and --pointers with new file names; the earlier deployment is not changed.')
    elif a.verify_only:
        raise RuntimeError('No deployment journal to verify')
    else:
        state = {'version': VERSION, 'tag': secrets.token_hex(4), 'games': a.games, 'players': a.players,
                 'treasury_mode': a.treasury, 'network': a.network_name, 'seat_start': a.seat_start,
                 'economics': a.economics, 'accounts': {}, 'funded': {}, 'templates': {}, 'components': {},
                 'transactions': {}}
        save(state_path, state)
    recorded = (state['games'], state['players'], state['treasury_mode'], state['seat_start'], state['economics'])
    if recorded != (a.games, a.players, a.treasury, a.seat_start, a.economics) and not a.verify_only:
        raise RuntimeError(f'Journal records games={state["games"]} players={state["players"]} '
                           f'treasury={state["treasury_mode"]} seat_start={state["seat_start"]} '
                           f'economics={state["economics"]}; rerun with those options or use a new --state file')
    if a.verify_only:
        write_pointers(state, token, pointers_path)
        print('Verified all components; pointers rewritten:', pointers_path)
        return

    if state.get('creating'):
        role, name = state['creating']['role'], state['creating']['name']
        r = rpc('accounts.get', {'name_or_address': name}, token)
        acc = r.get('account', r)
        state['accounts'][role] = {'name': name, 'component': component(acc['component_address']),
                                   'owner_key_id': acc.get('owner_key_id')}
        state.pop('creating')
        save(state_path, state)
        print('Recovered interrupted account creation:', name)

    # Every template still to publish must be built before anything is created or funded.
    missing = [str(w) for w in wasm_paths(state).values() if not w.is_file()]
    if missing:
        raise RuntimeError('Missing WASM (nothing created, funded or submitted):\n  ' + '\n  '.join(missing) +
                           '\nRun ./scripts/build-all.sh first.')

    roles = roles_of(state)
    steps = step_names(state['players'], state['games'])
    print(f'Wallet: {RPC}  ·  network label: {state["network"]}')
    print(f'Ootle game engine v{VERSION} deployment {state["tag"]}: {state["games"]} games × {state["players"]} seats, '
          f'{state["treasury_mode"]} treasury, seat start {state["seat_start"] / 1e6:.6f} tTARI.')
    print(f'Economics per game: {state["economics"]}')
    print(f'{len(roles)} new accounts{"" if a.no_faucet else " (each funded from the faucet)"}, 4 templates, '
          f'{len(steps)} setup transactions. Earlier deployments are not touched.')
    if input('Type DEPLOY to authorize these on-chain operations: ').strip() != 'DEPLOY':
        print('Cancelled. Nothing submitted.')
        return

    for role in roles:
        create_account(state, role, token, state_path)
    if not a.no_faucet:
        for role in roles:
            if state['funded'].get(role):
                continue
            rpc('accounts.create_free_test_coins', {'account': {'Name': state['accounts'][role]['name']},
                                                    'max_fee': 10000}, token)
            state['funded'][role] = True
            save(state_path, state)
            print('Faucet funds requested:', role)
    for role in roles:
        wait_on_chain(state['accounts'][role]['component'], token, role)

    operator = state['accounts']['global_house']
    for key, wasm in wasm_paths(state).items():
        def publish(w=wasm):
            r = rpc('transactions.publish_template', {'binary': base64.b64encode(w.read_bytes()).decode(),
                    'fee_account': {'Name': operator['name']}, 'max_fee': PUBLISH_FEE,
                    'detect_inputs': True, 'dry_run': False}, token)
            return r['transaction_id']
        out = journal_tx(state, 'publish:' + key, publish, token, state_path)
        state['templates'][key] = template(event(out, 'std.template.publish'))
        save(state_path, state)
        print(f'Published {key}: {state["templates"][key]}')

    for step in steps:
        submit_step(state, step, context(state), token, state_path)

    write_pointers(state, token, pointers_path)
    print('\nDeployment verified. Pointers written to', pointers_path)


if __name__ == '__main__':
    try:
        main()
    except (RuntimeError, ValueError, KeyError, OSError) as e:
        print('DEPLOYMENT STOPPED:', e, file=sys.stderr)
        sys.exit(1)

# End of scripts/deploy_local.py
# Location: scripts/deploy_local.py · Created: 2026-11-16 04:06:01 UTC
