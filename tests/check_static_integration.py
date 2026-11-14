#!/usr/bin/env python3
"""v0.6.10 static contract/backend/frontend/migration assertions; no live claims."""
from pathlib import Path
import re
ROOT=Path(__file__).resolve().parents[1]
read=lambda name:(ROOT/name).read_text()
contract=read('templates/game_session/src/lib.rs')
backend=read('src/game_session.rs')
main=read('src/main.rs')
js=read('web/game.js')
html=read('web/game.html')
css=read('web/game.css')
migration=read('scripts/deploy-economy-game.py')
for fn in ['propose_sale','decline_sale','accept_sale','build_improvement','play_turn','new_from_existing']:
    assert f'pub fn {fn}(' in contract,fn
assert 'improvements: Vec<u8>' in contract
assert 'self.improvements[square as usize] = level + 1' in contract
assert 'self.improvements[sell_square as usize] = 0' in contract
assert 'self.rent_amount(square)' in contract
assert 'self.developed_value(sell_square)' in contract
assert 'state.len()!=29' in backend
assert 'if len(old_state) != 28:' in migration
assert 'if len(state) != 29:' in migration
assert 'state[:28] != old_state[:28]' in migration
assert "if [int(x) for x in state[28]] != [0] * 40:" in migration
assert 'Pending private trade' in migration
assert '.route("/api/game/build", post(game_build))' in main
assert 'game.build_improvement(' in backend
assert 'after.improvements==expected_improvements' in backend
assert 'game.accept_sale(' in backend
assert 'development-strip' in js and 'development-strip' in css
assert 'showAssetDetails' in js and 'renderImprovements' in js
assert "visibleSquareName(square)" in js
assert 'Maintenance Bay' in js and 'Go to Maintenance' in js
assert 'build-submit' in html
ids=set(re.findall(r'id="([a-zA-Z0-9-]+)"',html))
accesses=set(re.findall(r"\$\('([a-zA-Z0-9-]+)'\)",js))
assert not accesses-ids, f'Missing DOM IDs: {accesses-ids}'
assert 'version = "0.6.15"' in read('Cargo.toml')
assert 'version = "0.6.10"' in read('templates/game_session/Cargo.toml')
print('PASS: v0.6.10 static integration invariants (NOT a Rust compile or live test)')

# v0.6.11 UI integration invariants; contract version stays 0.6.10.
assert 'center-decisions' in html and 'id="asset-feature"' in html
assert 'id="walkway"' in html and 'id="walkway-markers"' in html
assert 'function renderWalkwayMarkers()' in js and 'function renderAssetFeature()' in js
assert 'tile.dataset.square=String(i)' in js
assert 'if(![1,2].some(p=>displayPosition(p)===square.square))return;' in js
assert 'visitor-carpet.owner1' in css and 'visitor-carpet.owner2' in css
assert '.layout.board-focus .sidebar{display:none}' in css
assert '.route("/assets/asset-a-storefront.svg", get(asset_a_image))' in main
assert 'id="execute"' in html and 'id="trade-propose"' in html and 'id="build-submit"' in html
assert html.index('id="execute"') > html.index('id="sidebar"')
print('PASS: v0.6.11 frontend-only integrations')

# v0.6.12: unified stage and player-actions layout, using stdlib only.
from html.parser import HTMLParser
class Hierarchy(HTMLParser):
    def __init__(self):
        super().__init__();self.stack=[];self.parents={};self.ids={}
    def handle_starttag(self, tag, attrs):
        attrs=dict(attrs);id=attrs.get('id')
        if id:self.ids[id]=self.ids.get(id,0)+1;self.parents[id]=list(self.stack)
        if tag not in {'input','meta','link','img','br','hr','source'}:
            self.stack.append((tag,attrs.get('id')))
    def handle_endtag(self,tag):
        for n in range(len(self.stack)-1,-1,-1):
            if self.stack[n][0]==tag:
                del self.stack[n:];break
hier=Hierarchy();hier.feed(html)
assert ('aside','sidebar') in hier.parents['town-square']
assert ('aside','sidebar') in hier.parents['execute']
assert ('section','board-center') in hier.parents['roll-dice']
assert ('section','board-center') in hier.parents['landing-stage']
assert all(hier.ids.get(k)==1 for k in ['execute','trade-propose','build-submit','roll-dice'])
assert 'function renderLandingStage()' in js and 'eventDismissed' in js
assert 'background:#080d14' in css
print('PASS: v0.6.12 stage and sidebar integration assertions')

# v0.6.13: automatic settlement for straightforward visits and additional sample venue art.
assert 'id="stage-kicker"' in html and 'id="stage-scene"' in html and 'id="stage-actions"' in html
assert 'id="recovery-panel"' in html
assert 'function autoSettlePendingTurn()' in js and 'visitStory' in js and 'ASSET_THEMES' in js
assert '/assets/asset-f-bistro.svg' in main and '/assets/asset-l-lounge.svg' in main
assert '.stage-scene' in css and '.stage-chip' in css and '.tile.visited .tile-inner' in css
print('PASS: v0.6.13 auto-settlement and sample venue integration assertions')

assert "function exitVisit()" in js and "scheduleVisitDismiss" not in js
assert "button.addEventListener('click',exitVisit)" in js
print("PASS: v0.6.15 acknowledgement-only manual exit")

assert "Acknowledge · Return to Town Square" in js
assert "setTimeout" not in js
print("PASS: visit does not disappear on a timer")
