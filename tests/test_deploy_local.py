# Ootle Game Engine | tests/test_deploy_local.py
# Version: v0.4.3
# Unit tests for scripts/deploy_local.py: manifest rendering and journal safety. No wallet needed.
# The rendered manifests are also executed in the Tari engine by engine-tests/tests/deploy_manifests.rs.
# v0.3.0: a pool of games, per-game economics, seat cash settlement, and the journal version check.
# v0.4.3: --fund-from-house plan, balance check before any transfer, journaled transfers, resume.
import importlib.util
import json
import pathlib
import tempfile
import unittest
from unittest.mock import patch

PATH = pathlib.Path(__file__).resolve().parents[1] / 'scripts/deploy_local.py'
spec = importlib.util.spec_from_file_location('deploy_local', PATH)
d = importlib.util.module_from_spec(spec)
spec.loader.exec_module(d)

C = lambda c: 'component_' + c * 64
T = 'ab' * 32


def ctx(mode='separate', players=4, games=2):
    acc = {'global_house': C('0'), 'global_community': C('1'), 'global_chance': C('2')}
    for g in range(1, games + 1):
        acc.update({f'g{g}_house': C('3'), f'g{g}_community': C('4'), f'g{g}_chance': C('5')})
        acc.update({f'g{g}_seat{i}': C('e') for i in range(1, players + 1)})
    acc['operator'] = acc['global_house']
    comps = {'registry': C('a'), 'treasury': C('b'), 'lobby': C('c')}
    comps.update({f'session{g}': C('d') for g in range(1, games + 1)})
    return {'templates': {k: T for k in ['registry', 'lobby', 'treasury', 'town_square']},
            'components': comps, 'accounts': acc, 'resource': d.RESOURCE, 'players': players, 'games': games,
            'treasury_mode': mode, 'economics': dict(d.DEFAULT_ECONOMICS), 'seat_start': d.SEAT_START,
            'seat_cash': {f'g{g}_seat{i}': 30_000_000 for g in range(1, games + 1) for i in range(1, players + 1)}}


class Render(unittest.TestCase):
    def test_step_order(self):
        s = d.step_names(3, 2)
        self.assertEqual(s[:4], ['init:registry', 'init:treasury', 'init:lobby', 'lobby:add_type'])
        self.assertEqual(s[4:8], ['lobby:create_game:1', 'init:session:1', 'setup:attach_register:1', 'setup:economics:1'])
        self.assertEqual(s[-1], 'game:start:2')
        self.assertEqual(sum(x.startswith('setup:add_seat:') for x in s), 6)
        self.assertEqual(sum(x.startswith('seat:register_claim:') for x in s), 6)
        self.assertEqual(sum(x.startswith('seat:settle:') for x in s), 6)
        self.assertEqual(len(s), 4 + 2 * (4 + 3 * 3 + 1))

    def test_every_step_renders_without_vec_literals(self):
        for step in d.step_names(4, 2):
            r = d.render(step, ctx())
            self.assertIn('fn fee_main()', r['manifest'])
            self.assertNotIn('vec!', r['manifest'])
            self.assertTrue('fee' in r['variables'] or 'house' in r['variables'])

    def test_method_targets_are_wallet_variables(self):
        r = d.render('setup:attach_register:2', ctx())
        self.assertIn('let lobby = var!["lobby"];', r['manifest'])
        self.assertIn('let treasury = var!["treasury"];', r['manifest'])
        self.assertIn('lobby.attach_session(2u64,', r['manifest'])
        self.assertEqual(r['variables']['lobby'], C('c'))

    def test_seat_claim_is_signed_and_paid_by_the_seat(self):
        r = d.render('seat:register_claim:2:3', ctx())
        self.assertEqual(r['signer'], 'seat:2:3')
        self.assertEqual(r['variables']['fee'], C('e'))
        self.assertIn('registry.register("Game 2 Seat 3", 3u8);', r['manifest'])
        self.assertIn('lobby.claim_seat(2u64, 3u8);', r['manifest'])

    def test_session_carries_its_game_id(self):
        r = d.render('init:session:4', ctx(games=4))
        self.assertIn('TownSquare::new(4u64,', r['manifest'])
        self.assertEqual(r['creates'], 'session4')

    def test_economics_defaults_and_options(self):
        r = d.render('setup:economics:1', ctx())
        self.assertIn('session.set_economics(20000000u64, false, 0u16, true, 1000000u64, 2000000u64);', r['manifest'])
        c = ctx()
        c['economics'] = {'go_floor': 0, 'go_scaled': True, 'rent_fee_bps': 1500, 'build_on_landing': True,
                          'fee_amount': 2_000_000, 'penalty_amount': 4_000_000}
        self.assertIn('set_economics(0u64, true, 1500u16, true, 2000000u64, 4000000u64);', d.render('setup:economics:1', c)['manifest'])
        a = d.parse_args(['--go-scaled', '--go-floor', '0', '--rent-fee-bps', '1500', '--fee-amount', '2', '--penalty-amount', '4'])
        self.assertEqual(a.economics, c['economics'])
        self.assertEqual(d.parse_args([]).economics, d.DEFAULT_ECONOMICS)
        self.assertEqual(d.parse_args(['--seat-start', '20.5']).seat_start, 20_500_000)

    def test_seat_settle_moves_only_the_difference(self):
        c = ctx()
        r = d.render('seat:settle:1:2', c)
        self.assertEqual(r['signer'], 'house+seat:1:2')
        self.assertIn('house.pay_fee(', r['manifest'])
        self.assertIn('seat.withdraw(address!("%s"), 10000000u64); house.deposit(excess);' % d.RESOURCE, r['manifest'])
        c['seat_cash']['g1_seat2'] = 5_000_000
        self.assertIn('house.withdraw(address!("%s"), 15000000u64); seat.deposit(top_up);' % d.RESOURCE,
                      d.render('seat:settle:1:2', c)['manifest'])
        c['seat_cash']['g1_seat2'] = d.SEAT_START
        self.assertTrue(d.render('seat:settle:1:2', c)['skip'])

    def test_treasury_mode_selects_accounts_and_flags(self):
        sep = d.render('setup:attach_register:1', ctx('separate'))['manifest']
        glo = d.render('setup:attach_register:1', ctx('global'))['manifest']
        self.assertIn('false, false, false', sep)
        self.assertIn('true, true, true', glo)
        self.assertIn(C('3'), sep)          # game House account in separate mode
        self.assertIn(C('0'), glo)          # global House account in global mode

    def test_params_match_v0618_values(self):
        self.assertEqual(d.param_args(), '20000000u64, 2000000u64, 1000000u64, 2000000u64, 5000000u64, '
                                         '2000000u64, 5000000u64, 15000000u64, 80u8')


class Journal(unittest.TestCase):
    def test_unknown_outcome_is_never_resubmitted(self):
        with tempfile.TemporaryDirectory() as tmp:
            st = {'transactions': {'x': {'id': 'tx1', 'confirmed': False}}}
            with self.assertRaisesRegex(RuntimeError, 'unknown status'):
                d.journal_tx(st, 'x', lambda: self.fail('resubmitted'), 'tok', pathlib.Path(tmp) / 's.json')

    def test_id_is_journaled_before_waiting(self):
        with tempfile.TemporaryDirectory() as tmp:
            f = pathlib.Path(tmp) / 's.json'
            with patch.object(d, 'receipt', side_effect=RuntimeError('timed out')):
                st = {'transactions': {}}
                with self.assertRaises(RuntimeError):
                    d.journal_tx(st, 'x', lambda: 'tx-9', 'tok', f)
                saved = json.loads(f.read_text())
                self.assertEqual(saved['transactions']['x'], {'id': 'tx-9', 'confirmed': False})

    def test_receipt_must_be_accepted(self):
        with patch.object(d, 'rpc', return_value={'status': 'Rejected', 'result': {'result': {'Reject': 'x'}}}):
            with self.assertRaisesRegex(RuntimeError, 'not accepted'):
                d.receipt('id', 'tok')
        with patch.object(d, 'rpc', return_value={'timed_out': True}):
            with self.assertRaisesRegex(RuntimeError, 'timed out'):
                d.receipt('id', 'tok')

    def test_signing_requires_a_derived_account_key(self):
        with self.assertRaisesRegex(RuntimeError, 'derived account key'):
            d.key_index({'name': 'x', 'owner_key_id': None})
        self.assertEqual(d.key_index({'name': 'x', 'owner_key_id': {'Derived': {'index': 7, 'key_branch': 'account'}}}), 7)

    def test_confirmed_step_is_not_dry_run_again(self):
        st = {'transactions': {'init:registry': {'confirmed': True, 'receipt': {'ok': 1}}}}
        with patch.object(d, 'rpc') as r:
            self.assertEqual(d.submit_step(st, 'init:registry', ctx(), 'tok', pathlib.Path('/nonexistent')), {'ok': 1})
            r.assert_not_called()

    def test_settle_reads_the_seat_balance_and_uses_the_house_seal(self):
        key = lambda i: {'Derived': {'index': i, 'key_branch': 'account'}}
        st = {'treasury_mode': 'separate', 'transactions': {}, 'templates': {}, 'components': {},
              'accounts': {'g1_house': {'name': 'H', 'owner_key_id': key(3)}, 'g1_seat2': {'name': 'S', 'owner_key_id': key(9)}}}
        seen = []

        def fake(method, params, token=None):
            seen.append((method, params))
            if method == 'accounts.get_balances':
                return {'balances': [{'resource_address': d.RESOURCE, 'balance': '25000000'}]}
            if method == 'transactions.submit_manifest' and params['dry_run']:
                return {'result': {'finalize': {'result': {'Accept': {}}}}}
            if method == 'transactions.submit_manifest':
                return {'transaction_id': 'tx'}
            return {'status': 'Accepted', 'result': {'result': {'Accept': {}}}}
        with tempfile.TemporaryDirectory() as tmp, patch.object(d, 'rpc', side_effect=fake):
            d.submit_step(st, 'seat:settle:1:2', ctx(), 'tok', pathlib.Path(tmp) / 's.json')
        sub = [p for m, p in seen if m == 'transactions.submit_manifest']
        self.assertEqual(sub[0]['seal_signer_key_id'], key(3))
        self.assertEqual(sub[0]['signing_key_ids'], [key(9)])
        self.assertIn('5000000u64); house.deposit(excess);', sub[0]['manifest'])

    def test_missing_wasm_stops_before_prompt_accounts_and_faucet(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            calls = []
            fake_rpc = lambda method, params, token=None: calls.append(method) or {}
            with patch.object(d, 'ROOT', root), \
                    patch.object(d, 'auth', return_value='tok'), patch.object(d, 'rpc', side_effect=fake_rpc), \
                    patch('builtins.input', side_effect=AssertionError('prompted before the WASM check')), \
                    patch('sys.argv', ['deploy_local.py', '--players', '4', '--state', str(root / 's.json'),
                                       '--pointers', str(root / 'p.json')]):
                with self.assertRaisesRegex(RuntimeError, 'Missing WASM'):
                    d.main()
            self.assertEqual(calls, [], 'no wallet call may happen before the WASM check')

    def test_earlier_journal_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            (root / 's.json').write_text(json.dumps({'tag': 'x', 'players': 4, 'treasury_mode': 'separate'}))
            with patch.object(d, 'auth', return_value='tok'), patch.object(d, 'rpc', side_effect=AssertionError('rpc')), \
                    patch('sys.argv', ['deploy_local.py', '--state', str(root / 's.json'), '--pointers', str(root / 'p.json')]):
                with self.assertRaisesRegex(RuntimeError, 'earlier deploy script'):
                    d.main()

    def test_wasm_paths_skip_published_templates(self):
        self.assertEqual(set(d.wasm_paths({'templates': {'registry': T}})), {'lobby', 'treasury', 'town_square'})
        self.assertEqual(d.wasm_paths({'templates': {k: T for k in ['registry', 'lobby', 'treasury', 'town_square']}}), {})

    def test_roles_per_game(self):
        r = d.roles_of({'games': 2, 'players': 3, 'treasury_mode': 'separate'})
        self.assertEqual(len(r), 3 + 2 * (3 + 3))
        self.assertIn('g2_seat3', r)
        self.assertEqual(len(d.roles_of({'games': 2, 'players': 3, 'treasury_mode': 'global'})), 3 + 2 * 3)

    def test_fund_plan_amounts(self):
        st = {'games': 5, 'players': 4, 'treasury_mode': 'separate', 'seat_start': d.SEAT_START}
        plan = dict(d.fund_plan(st))
        self.assertNotIn('global_house', plan)
        self.assertEqual(len(plan), 37)
        self.assertEqual(plan['g1_seat1'], 2_000_000)
        self.assertEqual(plan['g3_house'], 88_000_000)
        self.assertEqual(plan['g5_chance'], 1_000_000)
        self.assertEqual(plan['global_community'], 1_000_000)
        self.assertEqual(sum(plan.values()), 492_000_000)
        self.assertEqual(d.fund_need(st), 492_000_000 + 37 * d.FEE + d.FUND_HOUSE_RESERVE)

    def _fund_state(self):
        st = {'games': 1, 'players': 2, 'treasury_mode': 'separate', 'seat_start': d.SEAT_START,
              'accounts': {}, 'funded': {}, 'transactions': {}}
        for r in d.roles_of(st):
            st['accounts'][r] = {'name': 'OGE-t-' + r, 'component': C('0')}
        return st

    def test_fund_from_house_refuses_when_the_main_house_is_short(self):
        st, sent = self._fund_state(), []
        def fake(method, params, token=None):
            sent.append(method)
            return {'balances': [{'resource_address': d.RESOURCE, 'balance': '1000000'}]} if method == 'accounts.get_balances' else {}
        with tempfile.TemporaryDirectory() as tmp, patch.object(d, 'rpc', side_effect=fake):
            with self.assertRaisesRegex(RuntimeError, 'Nothing sent'):
                d.fund_from_house(st, 'tok', pathlib.Path(tmp) / 's.json')
        self.assertNotIn('accounts.transfer', sent)

    def test_fund_from_house_journals_each_transfer_and_resumes(self):
        st, sent = self._fund_state(), []
        def fake(method, params, token=None):
            if method == 'accounts.get_balances':
                return {'balances': [{'resource_address': d.RESOURCE, 'balance': str(10**12)}]}
            if method == 'accounts.get':
                return {'account': {'owner_public_key': 'ab' * 32}}
            if method == 'accounts.transfer':
                sent.append(params)
                return {'transaction_id': 'tx%d' % len(sent)}
            if method == 'transactions.wait_result':
                return {'status': 'Accepted', 'result': {'result': {'Accept': {}}}}
            raise AssertionError(method)
        with tempfile.TemporaryDirectory() as tmp, patch.object(d, 'rpc', side_effect=fake):
            path = pathlib.Path(tmp) / 's.json'
            d.fund_from_house(st, 'tok', path)
            self.assertEqual(len(sent), len(d.fund_plan(st)))
            self.assertTrue(all(p['account'] == {'Name': 'OGE-t-global_house'} and p['resource_address'] == d.RESOURCE
                                and p['dry_run'] is False for p in sent))
            self.assertEqual({k for k in st['transactions']}, {'fund:' + r for r, _ in d.fund_plan(st)})
            d.fund_from_house(st, 'tok', path)          # everything funded: nothing is sent again
            self.assertEqual(len(sent), len(d.fund_plan(st)))

    def test_address_validation(self):
        self.assertEqual(d.component(C('a')), C('a'))
        for bad in ('component_xyz', C('a')[:-1], 'resource_' + 'a' * 64):
            with self.assertRaises(ValueError):
                d.component(bad)


if __name__ == '__main__':
    unittest.main()

# End of tests/test_deploy_local.py
# Location: tests/test_deploy_local.py · Created: 2026-11-16 04:06:01 UTC
