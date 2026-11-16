<!-- Ootle Game Engine | docs/STATE_LAYOUT.md · Version: v0.4.0 -->
# On-chain state layout

v0.4.0: no layout changes (Town Square still has 49 fields). The lot businesses, receipts and
verification detail are backend stores, not on-chain state.

The engine encodes each component's state as a positional CBOR array in struct field order.
The backend decodes by index (as v0.6.18 does) and must reject any layout with a different field count.
Indices below are verified by the engine tests (`extract_component_value(component, "$.N")`).

## Town Square session (`OotleTownSquare`, 49 fields; 41..48 added in v0.3.0)

| Index | Field | Type | Meaning |
|---|---|---|---|
| 0 | game_id | u64 | Lobby game id |
| 1 | lobby | ComponentAddress | Lobby component |
| 2 | treasury | ComponentAddress | Treasury component |
| 3 | resource | ResourceAddress | tTARI resource |
| 4 | go_reward | u64 | Paid by the House when passing GO |
| 5 | square_reward | u64 | Square 9 reward |
| 6 | fee_amount | u64 | Squares 4, 24, 37 |
| 7 | penalty_amount | u64 | Square 7 |
| 8 | shop_fee | u64 | Maintenance shop fee |
| 9 | storage_fee | u64 | Maintenance storage per failed held turn |
| 10 | house_cost | u64 | Each of the two houses |
| 11 | hotel_cost | u64 | Hotel |
| 12 | buyback_pct | u8 | House buyback percentage of developed value |
| 13 | capacity | u8 | Seats in this game |
| 14 | seats | Vec<ComponentAddress> | Seat account per seat (seat N at index N-1) |
| 15 | out | Vec<bool> | Seat removed (quit or eliminated) |
| 16 | started | bool | Game started |
| 17 | round | u64 | Round, starts at 1 |
| 18 | active | u8 | Active seat, 1-based |
| 19 | turn_count | u64 | Committed moves |
| 20 | doubles_streak | u8 | Consecutive doubles of the active seat |
| 21 | positions | Vec<u16> | Square per seat |
| 22 | last_die1 | u8 | Last die 1 |
| 23 | last_die2 | u8 | Last die 2 |
| 24 | owners | Vec<u8> | 40 entries; 0 unowned, otherwise seat number |
| 25 | improvements | Vec<u8> | 40 entries; 0..3 (house, house, hotel) |
| 26 | held | Vec<bool> | Seat held in Maintenance |
| 27 | held_attempts | Vec<u8> | Failed roll attempts while held |
| 28 | held_owed | Vec<u64> | Shop fee + storage owed |
| 29 | event_kind | u8 | 0 none, 1 Chance, 2 Community, 3 rent, 4 sent to Maintenance, 5 held after failed roll |
| 30 | event_card | u8 | 0..3 card id, 255 otherwise |
| 31 | event_amount | u64 | Card amount, rent, or amount owed (kind 5) |
| 32 | event_positive | bool | Card paid the seat |
| 33 | event_seat | u8 | Seat of the event |
| 34 | event_square | u16 | Square of the event |
| 35 | event_recipient | u8 | Rent owner |
| 36 | offer_square | u16 | 65535 = no pending offer |
| 37 | offer_price | u64 | Offer price |
| 38 | offer_seller | u8 | Seller seat |
| 39 | offer_buyer | u8 | Buyer seat |
| 40 | offer_nonce | u64 | Increments on every proposal |
| 41 | go_floor | u64 | Scaled GO never below this (v0.3.0) |
| 42 | go_scaled | bool | GO = max(floor, go_reward × unowned assets ÷ 22) when true |
| 43 | rent_fee_bps | u16 | Part of each rent routed as a fee (0..5000) |
| 44 | build_on_landing | bool | Build only on the owned asset just landed on |
| 45 | landed_seat | u8 | Seat that last landed (0 = none); cleared at the next move or after a build |
| 46 | landed_square | u16 | Square it landed on (65535 = none) |
| 47 | seat_since | Vec<u64> | Per seat: round its current stint started (1 at start; reset sets the current round) |
| 48 | resets | Vec<u32> | Per seat: number of resets and regenerations |

## Lobby (`OotleLobby`, 5 fields)

| Index | Field | Type |
|---|---|---|
| 0 | registry | ComponentAddress |
| 1 | treasury | ComponentAddress |
| 2 | types | Vec<(id u32, name String, template Option<TemplateAddress>, params (8 × u64, u8))> |
| 3 | games | Vec<(id u64, type_id u32, capacity u8, session Option<ComponentAddress>, status u8)> — status 0 open, 1 started, 2 finished |
| 4 | claims | Vec<(game_id u64, seat u8, identity NonFungibleAddress)> |

## Treasury (`OotleTreasury`, 7 fields)

| Index | Field | Type |
|---|---|---|
| 0 | resource | ResourceAddress |
| 1 | global_house | ComponentAddress |
| 2 | global_community | ComponentAddress |
| 3 | global_chance | ComponentAddress |
| 4 | community_bps | u16 (500) |
| 5 | chance_bps | u16 (500) |
| 6 | games | Vec<(id, session, house, community, chance, house_global, community_global, chance_global)> — addresses are the resolved accounts |

## Registry (`OotleRegistry`, 1 field)

| Index | Field | Type |
|---|---|---|
| 0 | profiles | Vec<(identity NonFungibleAddress, name String, icon u8)> |

<!-- End of docs/STATE_LAYOUT.md · Location: docs/STATE_LAYOUT.md · Created: 2026-11-16 04:06:01 UTC -->
