#!/usr/bin/env python3
"""Ootle Asset Game v0.6.10: verified migration from an existing 40-square session.

Publishes a NEW GameSession template and instantiates a new component while preserving:
- round / active player / positions / last recorded totals / turn count
- all 22 asset ownership slots
- house / controller / resource references

The source component remains unchanged. Source must have 28 fields (v0.6.9).
All 28 fields are copied; the new improvement vector starts empty. This script only updates THIS package's
config/game_session.toml after publication, instantiation, and live verification.
"""
import argparse
import base64
import json
import re
import sys
import tomllib
from pathlib import Path
from urllib.error import HTTPError
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parent.parent
BINARY = ROOT / 'templates/game_session/target/wasm32-unknown-unknown/release/ootle_game_session.wasm'
DEST = ROOT / 'config/game_session.toml'
RPC = 'http://127.0.0.1:12010/json_rpc'
FEE = 1_000_000
ASSET_SQUARES = [1,3,6,8,11,13,14,16,18,19,21,23,25,26,28,29,31,32,34,36,38,39]


def rpc(method, params, token=None):
    headers = {'Content-Type': 'application/json'}
    if token:
        headers['Authorization'] = 'Bearer ' + token
    data = json.dumps({'jsonrpc': '2.0', 'id': 1, 'method': method, 'params': params}).encode()
    try:
        with urlopen(Request(RPC, data=data, headers=headers), timeout=150) as response:
            result = json.load(response)
    except HTTPError as exc:
        result = json.load(exc)
    if 'error' in result:
        raise RuntimeError(f'{method}: {result["error"]}')
    return result['result']


def auth():
    return rpc('auth.request', {'permissions': ['Admin'], 'credentials': 'None'})['token']


def finalized(txid):
    result = rpc('transactions.wait_result', {'transaction_id': txid, 'timeout_secs': 120}, auth())
    if result.get('timed_out') or result.get('status') != 'Accepted' or 'Accept' not in (result.get('result') or {}).get('result', {}):
        raise RuntimeError(f'Not finalized successfully: {txid}: {str(result)[:2800]}')
    return result


def event_addr(outcome, topic, template=None):
    for event in outcome['result'].get('events', []):
        if event.get('topic') == topic and (template is None or event.get('template_address') == template):
            return event['substate_id'] if topic == 'std.component.created' else event['template_address']
    raise RuntimeError(f'Missing {topic} for template {template}')


def tag(field, kind, prefix):
    if field.get('@cbor') == 'tag':
        actual = field.get('tag')
        value = field['value']['hex']
    else:
        actual = field.get('tag')
        value = field.get('value', {}).get('hex')
    if actual != kind:
        raise RuntimeError(f'Invalid address tag (expected {kind})')
    if not re.fullmatch(r'[0-9a-fA-F]{64}', value or ''):
        raise RuntimeError('Invalid on-chain address bytes')
    return prefix + value.lower()


def state_at(address):
    response = rpc('substates.get', {'substate_id': address}, auth())
    entry = response['substate_from_remote']
    return entry['substate']['Component'], entry.get('version')


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--from-session', type=Path, required=True,
                    help='Existing deployed 40-square session config/game_session.toml; never modified')
args = parser.parse_args()
if not args.from_session.is_file():
    sys.exit(f'Cannot read source session: {args.from_session}')
source = tomllib.loads(args.from_session.read_text())
current = tomllib.loads(DEST.read_text())
if current.get('component_address') or current.get('template_address'):
    sys.exit('This package already has a configured component. Refusing to overwrite or deploy.')
if not BINARY.is_file():
    sys.exit('Missing WASM; run ./scripts/build-game-wasm.sh')
old = source.get('component_address', '')
if not old.startswith('component_') or len(old) != 74:
    sys.exit('Source config has no valid deployed game component')
if source.get('board_size') != 40:
    sys.exit('Migration requires an existing 40-square game')
component, old_version = state_at(old)
if component['header']['template_address'] != source.get('template_address'):
    sys.exit('Source template does not match verified on-chain template; STOP')
old_state = component['body']['state']
if len(old_state) != 28:
    sys.exit(f'Expected 28-field v0.6.9 state, got {len(old_state)}. Refusing migration.')
if int(old_state[23]) != 65535 or any(int(x) != 0 for x in old_state[24:27]):
    sys.exit('Pending private trade: resolve it in v0.6.9 BEFORE migrating.')

p1, p2 = tag(old_state[0], 128, 'component_'), tag(old_state[1], 128, 'component_')
board_size = int(old_state[2])
round_num, active, pos1, pos2, roll1, roll2, turns = (int(x) for x in old_state[3:10])
owners = [int(x) for x in old_state[12]]
if board_size != 40:
    sys.exit('Source board size is not 40')
if not (round_num >= 1 and active in (1, 2) and 0 <= pos1 < 40 and 0 <= pos2 < 40 and len(owners) == 40 and all(0 <= x <= 2 for x in owners)):
    sys.exit('Inconsistent source game state; refusing migration')

accounts = tomllib.loads((ROOT / 'config/accounts.toml').read_text())
addr = {a['id']: a['component_address'] for a in accounts['accounts']}
controller = tomllib.loads((ROOT / 'config/deployment.toml').read_text())['component_address']
resource = accounts['resource_address']
if (p1 != addr['player1'] or p2 != addr['player2']
        or tag(old_state[13], 128, 'component_') != addr['house']
        or tag(old_state[14], 128, 'component_') != controller
        or tag(old_state[15], 131, 'resource_') != resource):
    sys.exit('Source account / treasury / resource configuration mismatch; refusing migration')

print('VERIFIED SOURCE:', old, 'version', old_version, flush=True)
print(f'Round {round_num}, active P{active}, positions {pos1}/{pos2}, turns {turns}', flush=True)
print('Publishing NEW v0.6.10 template; old component remains unchanged.', flush=True)

published = rpc('transactions.publish_template', {
    'binary': base64.b64encode(BINARY.read_bytes()).decode('ascii'),
    'fee_account': {'Name': 'Game House'},
    'max_fee': 3_000_000, 'detect_inputs': True, 'dry_run': False,
}, auth())['transaction_id']
print('Publication TX:', published, flush=True)
template = event_addr(finalized(published), 'std.template.publish')
print('Template:', template, flush=True)

packed_owners = sum(owners[sq] << (2 * idx) for idx, sq in enumerate(ASSET_SQUARES))
if packed_owners >= 1 << 44:
    sys.exit('Packed owners exceed 44 bits; refusing deployment')
manifest = f'''
use template_{template} as GameSession;
fn fee_main() {{
    let house = var!["house"];
    house.pay_fee({FEE}u64);
}}
fn main() {{
    let game = GameSession::new_from_existing(
        address!("{p1}"), address!("{p2}"),
        address!("{addr['house']}"), address!("{controller}"),
        address!("{resource}"),
        {round_num}u64, {active}u8, {pos1}u16, {pos2}u16,
        {roll1}u8, {roll2}u8, {turns}u64,
        {packed_owners}u64,
        {int(old_state[16])}u8, {int(old_state[17])}u8, {int(old_state[18])}u64,
        {int(old_state[19])}u8, {int(old_state[20])}u8,
        {int(old_state[21])}u16, {int(old_state[22])}u8,
        {int(old_state[27])}u64
    );
}}
'''
params = {
    'manifest': manifest,
    'variables': {'house': addr['house']},
    'seal_signer_key_id': {'Derived': {'index': 3, 'key_branch': 'account'}},
    'signing_key_ids': [], 'max_fee': FEE, 'dry_run': True, 'blobs': {},
}
dry = rpc('transactions.submit_manifest', params, auth())
if 'Accept' not in (dry.get('result') or {}).get('finalize', {}).get('result', {}):
    sys.exit(f'Constructor dry run not accepted; no component instantiated: {dry}')
print('Constructor dry run accepted. Required fee:', dry['required_fees'], flush=True)
params['dry_run'] = False
instantiation = rpc('transactions.submit_manifest', params, auth())['transaction_id']
print('Instantiation TX:', instantiation, flush=True)
new_component = event_addr(finalized(instantiation), 'std.component.created', template)
print('New component:', new_component, flush=True)

new_state, version = state_at(new_component)
if new_state['header']['template_address'] != template or version != 0:
    sys.exit('New component is missing expected template or version; pointer NOT changed')
state = new_state['body']['state']
if len(state) != 29:
    sys.exit(f'Expected 29 new state fields, got {len(state)}; pointer NOT changed')
if state[:28] != old_state[:28]:
    sys.exit('Source game state differs after migration; pointer NOT changed')
if [int(x) for x in state[28]] != [0] * 40:
    sys.exit('New development slots not initialized as expected; pointer NOT changed')
if [int(x) for x in state[12]] != owners:
    sys.exit('Asset ownership mismatched after migration; pointer NOT changed')
if state[13:16] != old_state[13:16]:
    sys.exit('Treasury/account references differ; pointer NOT changed')
source_after, source_version_after = state_at(old)
if source_version_after != old_version or source_after['body']['state'] != old_state:
    sys.exit('Source changed during migration; new pointer NOT changed')

content = (
    '# Ootle Asset Game v0.6.10 | confirmed migrated game pointer.\n'
    '# New component was independently read and matched the previous on-chain game state.\n'
    f'template_address = "{template}"\n'
    f'component_address = "{new_component}"\n'
    f'publication_transaction_id = "{published}"\n'
    f'instantiation_transaction_id = "{instantiation}"\n'
    f'migrated_from_component = "{old}"\n'
    'board_size = 40\n'
)
if tomllib.loads(DEST.read_text()).get('component_address'):
    sys.exit('Destination config changed during deployment; pointer NOT replaced')
tmp = DEST.with_suffix('.toml.tmp')
tmp.write_text(content)
tmp.replace(DEST)
print('Migration verified and config updated:', DEST)
print('Old component preserved:', old)
print('Next: OOTLE_ENABLE_LIVE_TESTS=1 cargo run')
