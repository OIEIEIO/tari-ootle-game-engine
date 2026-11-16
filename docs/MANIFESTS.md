<!-- Ootle Game Engine | docs/MANIFESTS.md · Version: v0.4.0 -->
# Session methods and the manifests the backend submits

v0.4.0: no manifest changes. A purchase's business choice is recorded off chain after the
transaction is accepted (`app/src/lots.rs`); it is not part of any manifest.

Every seat action asserts that the seat's claimed identity badge is in scope, so the seat's own
key signs the transaction (`signing_key_ids`). The game House key seals it and pays the fee.
Amounts are exact; the contract rejects any bucket that differs by one base unit.

| Method | Arguments | Signing seat | Buckets |
|---|---|---|---|
| play_turn | seat u8, die1 u8, die2 u8, expected_round u64, expected_turn u64, buy bool, sell_square u16 (65535 none), seat_funds, house_funds | acting seat | House + seat |
| maintenance_turn | seat, pay bool, die1, die2, expected_round, expected_turn, buy, sell_square, seat_funds, house_funds | acting seat | House + seat |
| sell_to_house | seat, square u16, expected_round, expected_turn, house_funds (buyback value) | acting seat | House |
| pay_shop_fee | payer u8, held_seat u8, expected_round, expected_turn, seat_funds (amount owed) | payer (active) | seat |
| propose_sale | seller u8, buyer u8, square u16, price u64, expected_round, expected_turn | seller (active) | none |
| accept_sale | buyer u8, expected_nonce u64, expected_round, expected_turn, seat_funds (price) | buyer | seat |
| decline_sale | buyer u8, expected_nonce, expected_round, expected_turn | buyer | none |
| build_improvement | seat, square, expected_round, expected_turn, expected_level u8, seat_funds (next cost) | acting seat | seat |
| remove_seat | seat u8 (owner only) | operator | not used by the backend |

Turn funding (same arithmetic as v0.6.18, `contracts/town_square/src/rules.rs`):

- incoming (House bucket) = sale buyback + GO reward + square 9 reward + positive card
- outgoing = purchase + fees (fee squares + negative card + Maintenance payment) + rent
- seat bucket = outgoing − incoming, floored at 0; any surplus is deposited to the seat

## Manifest shape (app/src/sim.rs `manifest`)

Variables: `house` (game House account), `seat_account` (signing seat's account), `game` (session).
Only the variables a method uses are bound. Example, seat 1 on square 3 rolls 2 + 2 to the Penalty:

```
fn fee_main() {
    let house = var!["house"];
    house.pay_fee(1000000u64);
}
fn main() {
    let game = var!["game"];
    let house = var!["house"];
    let house_funds = house.withdraw(address!("resource_0101…0101"), 0u64);
    let seat_account = var!["seat_account"];
    let seat_funds = seat_account.withdraw(address!("resource_0101…0101"), 2000000u64);
    game.play_turn(1u8, 2u8, 2u8, 2u64, 6u64, false, 65535u16, seat_funds, house_funds);
}
```

`engine-tests/tests/backend_plans.rs` parses and executes these exact texts in the Tari engine and
checks the resulting state and every balance change against the backend's plan.

## v0.3.0 additions

- **Turn + build** (`sim::plan_manifest`): one transaction with the turn, then
  `game.build_improvement(seat, square, round, turn, level, seat_funds_2)`; the second call's buckets are
  suffixed `_2`. `round` and `turn` are the values after the turn (the contract checks them then).
- **Reset / regenerate**: `game.reset_seat(seat, pass_turn, round, turn)`, preceded by the cash
  settlement — `seat_account.withdraw(excess)` → `house.deposit` when the seat holds more than the seat
  start, `house.withdraw(top_up)` → `seat_account.deposit` when less. Sealed by the game House; the seat
  key and the operator key (owner method) are in `signing_key_ids`.
- **House top-up** (`ops::top_up_manifest`): `operator.pay_fee`; `operator.withdraw` → `house.deposit`.
  Variables `operator` (main House) and `house`; sealed by the operator.
- **Deploy**: `session.set_economics(go_floor, go_scaled, rent_fee_bps, build_on_landing, fee, penalty)`
  per game, and `seat:settle:G:I` (the seat-start settlement above, without `reset_seat`).

<!-- End of docs/MANIFESTS.md · Location: docs/MANIFESTS.md · Created: 2026-11-16 04:06:01 UTC -->
