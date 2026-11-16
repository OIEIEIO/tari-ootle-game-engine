<!-- Ootle Game Engine | VALIDATION.md · Version: v0.4.2 -->
# Validation performed in the build environment

| Check | Command | Result |
|---|---|---|
| Rules unit tests (contracts unchanged) | `cd contracts/town_square && cargo test --lib` | 10 passed |
| Deploy script unit tests (unchanged) | `python3 -m unittest discover -s tests -v` | 20 passed |
| v0.4.2 page check against the fake wallet | headless Chrome with every API response delayed 700 ms: the lobby is shown at every sample from the first 250 ms (no board before it); Enter without sound → intro → board on the near-black backdrop | As expected, no page errors |
| Re-run for v0.4.2: backend unit tests, deploy script tests, page scripts parse (the backend has no code change since v0.4.0; the engine tests were last run for v0.4.1) | | 20 passed · 20 passed · OK |
| Backend unit tests (adds catalogue coverage of the board, lot store sync / choose / clear, stale-then-fresh verification reads with the turn 301 numbers, news events, receipts store reload) | `cd app && cargo test --lib` | 20 passed |
| Engine tests: contracts | `cd engine-tests && cargo test --test engine` | 13 passed |
| Engine tests: deploy manifests | `cd engine-tests && cargo test --test deploy_manifests` | 2 passed |
| Engine tests: the backend's manifests and plans | `cd engine-tests && cargo test --test backend_plans` | 8 passed |
| Backend build, no warnings | `cd app && cargo build --release` | Built, 0 warnings |
| Page scripts parse | `node --check` on the five scripts in `app/web/` | OK |
| API against the fake wallet (`scripts/ui-check.sh --clear-offer`) | health, catalogue, state (seat colours, pieces, businesses, special squares), lot store (owned square 3 given a business at random), claim, roll, purchase with `business` on a non-asset refused (`INVALID_ASSET`), turn settled, receipts | As expected |
| Verification reads against the fake wallet | one turn: 4 reads, 500 ms apart, cause `state_fields_differ`, receipt stored in `receipts.jsonl` with reads, steps, manifest and events | As expected (the fake wallet never changes state) |
| Page against the fake wallet | headless Chrome: start gate with health lights, intro with SVG pieces, setup, board, landing scene with Buy / Pass and the three businesses, owned lot at level 1, news stack line, Developer tab with health, lot store, receipts and receipt detail | Rendered, no page errors |
| v0.4.1 page check against the fake wallet | headless Chrome: lobby (black page, title, where-you-are line, two Enter buttons); Enter with sound → audio running when the intro starts; selector label `Game 1 · 4 seats: 1 player, 0 AI, 3 open · round 2`; closed scene layer not displayed; a real click on Roll starts the roll (dice 1 + 6, move animated, landing scene opened); last game stored | As expected, no page errors |
| Scene gallery | all 66 businesses at vacant, bought and levels 1–3, and the 18 special scenes, rendered and inspected | Rendered |

v0.4.0 fixes one v0.3.0 test: `decode::tests::session_fixture_decodes` expected the landed seat and
square to be empty, but the engine-dumped fixture holds seat 4 on square 4 (seat 4 stands there). The
test now expects what the fixture holds; the fixture is unchanged.

`scripts/ui-check.sh` writes a corrected copy of the fixture pointers into its temporary directory:
the fixture's main House pointers are the placeholder `component_00`, which the configuration check
refuses (the v0.3.0 script stopped there).

The fake wallet never changes state, so its settlements report `COMMITTED / UNVERIFIED`, now with
the cause and every read recorded.

Not verified here: sound output (the effects, ambient sound and intro cues are synthesised in the
browser; the build environment has no audio device), the live wallet on the Swarm and on Esmeralda,
and the re-read behaviour against the live wallet that produced turns 297 and 301. The harness
engine (0.53.4) may differ from the live builds.

<!-- End of VALIDATION.md · Location: VALIDATION.md · Created: 2026-11-16 04:06:01 UTC -->
