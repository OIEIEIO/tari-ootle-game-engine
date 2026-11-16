//! Ootle Game Engine | contracts/treasury/src/lib.rs
//! Version: v0.1.0
//! Treasury router for every game. Holds no funds.
//! Each game registers its House, Community and Chance accounts; each of the three is either
//! the game's own account (separate) or the global pool account (global), chosen at launch.
//! `distribute` only accepts calls from the game's registered session component (caller badge),
//! splits Community 5% / Chance 5% and deposits the remainder, including rounding dust, to the House.
//! `distribute` does not write Treasury state, so turns of different games do not contend here.

use tari_template_lib::prelude::*;

#[template]
mod ootle_treasury {
    use super::*;

    pub struct OotleTreasury {
        resource: ResourceAddress,
        global_house: ComponentAddress,
        global_community: ComponentAddress,
        global_chance: ComponentAddress,
        community_bps: u16,
        chance_bps: u16,
        games: Vec<TreasuryGame>,
    }

    #[derive(Clone)]
    pub struct TreasuryGame {
        pub id: u64,
        pub session: ComponentAddress,
        pub house: ComponentAddress,
        pub community: ComponentAddress,
        pub chance: ComponentAddress,
        pub house_global: bool,
        pub community_global: bool,
        pub chance_global: bool,
    }

    impl OotleTreasury {
        pub fn new(resource: ResourceAddress, global_house: ComponentAddress, global_community: ComponentAddress,
                   global_chance: ComponentAddress) -> Component<Self> {
            let owner = CallerContext::transaction_signer_public_key();
            assert!(global_house != global_community && global_house != global_chance
                && global_community != global_chance, "duplicate global pool accounts");
            Component::new(Self {
                resource, global_house, global_community, global_chance,
                community_bps: 500, chance_bps: 500, games: vec![],
            })
            .with_access_rules(ComponentAccessRules::new()
                .method("register_game", rule!(public_key(owner)))
                .method("distribute", rule!(allow_all))
                .method("get_game", rule!(allow_all))
                .method("get_games", rule!(allow_all))
                .method("get_rates", rule!(allow_all))
                .default(rule!(deny_all)))
            .with_owner_rule(OwnerRule::OwnedBySigner)
            .create()
        }

        /// Registers a game. For each pool, `*_global = true` routes to the global account and
        /// the matching separate address argument is ignored.
        #[allow(clippy::too_many_arguments)]
        pub fn register_game(&mut self, id: u64, session: ComponentAddress, house: ComponentAddress,
                             community: ComponentAddress, chance: ComponentAddress,
                             house_global: bool, community_global: bool, chance_global: bool) {
            assert!(!self.games.iter().any(|g| g.id == id || g.session == session), "duplicate game");
            let house = if house_global { self.global_house } else { house };
            let community = if community_global { self.global_community } else { community };
            let chance = if chance_global { self.global_chance } else { chance };
            assert!(house != community && house != chance && community != chance, "duplicate pool accounts");
            self.games.push(TreasuryGame { id, session, house, community, chance,
                house_global, community_global, chance_global });
        }

        /// Category: 0 purchase or construction, 1 fee, penalty or Maintenance, 2 sponsorship.
        pub fn distribute(&self, game_id: u64, category: u8, mut revenue: Bucket, units: u64) {
            assert!(category <= 2, "unknown revenue category");
            assert!(units > 0, "zero revenue");
            assert!(revenue.resource_address() == self.resource, "wrong resource");
            assert!(revenue.amount() == Amount::from(units), "bucket amount mismatch");
            let g = self.games.iter().find(|g| g.id == game_id).expect("unregistered game");
            // Only the registered session component of this game may route revenue.
            CallerContext::assert_badge_in_scope(&NonFungibleAddress::caller_component_badge(g.session));
            let c = (u128::from(units) * u128::from(self.community_bps) / 10_000) as u64;
            let h = (u128::from(units) * u128::from(self.chance_bps) / 10_000) as u64;
            if c > 0 { ComponentManager::get(g.community).invoke("deposit", args![revenue.take(Amount::from(c))]); }
            if h > 0 { ComponentManager::get(g.chance).invoke("deposit", args![revenue.take(Amount::from(h))]); }
            ComponentManager::get(g.house).invoke("deposit", args![revenue]);
        }

        /// (session, house, community, chance, house_global, community_global, chance_global)
        pub fn get_game(&self, id: u64) -> Option<(ComponentAddress, ComponentAddress, ComponentAddress, ComponentAddress, bool, bool, bool)> {
            self.games.iter().find(|g| g.id == id)
                .map(|g| (g.session, g.house, g.community, g.chance, g.house_global, g.community_global, g.chance_global))
        }

        pub fn get_games(&self) -> Vec<TreasuryGame> {
            self.games.clone()
        }

        pub fn get_rates(&self) -> (u16, u16) {
            (self.community_bps, self.chance_bps)
        }
    }
}

// End of contracts/treasury/src/lib.rs
// Location: contracts/treasury/src/lib.rs · Created: 2026-11-15 21:49:02 UTC
