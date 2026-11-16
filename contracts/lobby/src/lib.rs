//! Ootle Game Engine | contracts/lobby/src/lib.rs
//! Version: v0.1.0
//! Game Lobby: catalogue of game types, the directory of game instances, and seat claims.
//! - A game type holds its parameters as data; its rules are the session template. The type's
//!   template is pinned from the first session attached to it; later sessions must match.
//! - Creating games, attaching sessions and starting/finishing games are owner-only.
//! - Players claim or release seats themselves (signer identity, must be registered in the Registry).
//!   Seats can only be released while the game is open, so a started game cannot lose a seat this way.
//! Holds no funds.

use tari_template_lib::prelude::*;

#[template]
mod ootle_lobby {
    use super::*;

    pub struct OotleLobby {
        registry: ComponentAddress,
        treasury: ComponentAddress,
        types: Vec<GameType>,
        games: Vec<Game>,
        claims: Vec<SeatClaim>,
    }

    #[derive(Clone)]
    pub struct GameType {
        pub id: u32,
        pub name: String,
        pub template: Option<TemplateAddress>,
        // go_reward, square_reward, fee_amount, penalty_amount, shop_fee, storage_fee, house_cost, hotel_cost, buyback_pct
        pub params: (u64, u64, u64, u64, u64, u64, u64, u64, u8),
    }

    #[derive(Clone)]
    pub struct Game {
        pub id: u64,
        pub type_id: u32,
        pub capacity: u8,
        pub session: Option<ComponentAddress>,
        pub status: u8, // 0 open, 1 started, 2 finished
    }

    #[derive(Clone)]
    pub struct SeatClaim {
        pub game_id: u64,
        pub seat: u8,
        pub identity: NonFungibleAddress,
    }

    impl OotleLobby {
        pub fn new(registry: ComponentAddress, treasury: ComponentAddress) -> Component<Self> {
            let owner = CallerContext::transaction_signer_public_key();
            Component::new(Self { registry, treasury, types: vec![], games: vec![], claims: vec![] })
                .with_access_rules(ComponentAccessRules::new()
                    .method("add_game_type", rule!(public_key(owner)))
                    .method("create_game", rule!(public_key(owner)))
                    .method("attach_session", rule!(public_key(owner)))
                    .method("mark_started", rule!(public_key(owner)))
                    .method("mark_finished", rule!(public_key(owner)))
                    .method("claim_seat", rule!(allow_all))
                    .method("release_seat", rule!(allow_all))
                    .method("seat_identity", rule!(allow_all))
                    .method("get_types", rule!(allow_all))
                    .method("get_games", rule!(allow_all))
                    .method("get_claims", rule!(allow_all))
                    .default(rule!(deny_all)))
                .with_owner_rule(OwnerRule::OwnedBySigner)
                .create()
        }

        /// Adds a game type. Returns its id (1, 2, ...).
        #[allow(clippy::too_many_arguments)]
        pub fn add_game_type(&mut self, name: String, go_reward: u64, square_reward: u64, fee_amount: u64,
                             penalty_amount: u64, shop_fee: u64, storage_fee: u64, house_cost: u64,
                             hotel_cost: u64, buyback_pct: u8) -> u32 {
            assert!(!name.is_empty() && name.len() <= 64, "type name must be 1..64 bytes");
            assert!(buyback_pct >= 1 && buyback_pct <= 100, "buyback percentage must be 1..100");
            let id = self.types.len() as u32 + 1;
            self.types.push(GameType { id, name, template: None,
                params: (go_reward, square_reward, fee_amount, penalty_amount, shop_fee, storage_fee,
                         house_cost, hotel_cost, buyback_pct) });
            id
        }

        /// Creates a game instance of a type. Returns its id (1, 2, ...).
        pub fn create_game(&mut self, type_id: u32, capacity: u8) -> u64 {
            assert!(self.types.iter().any(|t| t.id == type_id), "unknown game type");
            assert!((2..=20).contains(&capacity), "capacity outside 2..20");
            let id = self.games.len() as u64 + 1;
            self.games.push(Game { id, type_id, capacity, session: None, status: 0 });
            id
        }

        /// Links an instantiated session to its game after checking the session's own setup
        /// (game id, lobby, capacity, parameters) and template against the game and its type.
        pub fn attach_session(&mut self, game_id: u64, session: ComponentAddress) {
            let me = CallerContext::current_component_address();
            let gi = self.games.iter().position(|g| g.id == game_id).expect("unknown game");
            assert!(self.games[gi].session.is_none(), "session already attached");
            assert!(!self.games.iter().any(|g| g.session == Some(session)), "session attached to another game");
            let setup: (u64, ComponentAddress, u8, u64, u64, u64, u64, u64, u64, u64, u64, u8) =
                ComponentManager::get(session).call("get_setup", args![]);
            let ti = self.types.iter().position(|t| t.id == self.games[gi].type_id).expect("unknown game type");
            let t = &self.types[ti];
            assert!(setup.0 == game_id, "session game id mismatch");
            assert!(setup.1 == me, "session points to a different lobby");
            assert!(setup.2 == self.games[gi].capacity, "session capacity mismatch");
            let p = (setup.3, setup.4, setup.5, setup.6, setup.7, setup.8, setup.9, setup.10, setup.11);
            assert!(p == t.params, "session parameters differ from the game type");
            let template = ComponentManager::get(session).get_template_address();
            match t.template {
                Some(pinned) => assert!(pinned == template, "session template differs from the game type"),
                None => self.types[ti].template = Some(template),
            }
            self.games[gi].session = Some(session);
        }

        /// The signer claims a seat in an open game. The signer must be registered in the Registry.
        pub fn claim_seat(&mut self, game_id: u64, seat: u8) {
            let identity = Self::signer_identity();
            let g = self.games.iter().find(|g| g.id == game_id).expect("unknown game");
            assert!(g.status == 0, "game is not open for seat claims");
            assert!(seat >= 1 && seat <= g.capacity, "seat out of range");
            let registered: bool = ComponentManager::get(self.registry).call("is_registered", args![identity.clone()]);
            assert!(registered, "identity not registered");
            assert!(!self.claims.iter().any(|c| c.game_id == game_id && c.seat == seat), "seat already claimed");
            assert!(!self.claims.iter().any(|c| c.game_id == game_id && c.identity == identity), "identity already seated in this game");
            self.claims.push(SeatClaim { game_id, seat, identity });
        }

        /// The signer releases its own seat while the game is still open.
        pub fn release_seat(&mut self, game_id: u64, seat: u8) {
            let identity = Self::signer_identity();
            let g = self.games.iter().find(|g| g.id == game_id).expect("unknown game");
            assert!(g.status == 0, "seats can only be released while the game is open");
            let i = self.claims.iter().position(|c| c.game_id == game_id && c.seat == seat && c.identity == identity)
                .expect("seat not claimed by the signer");
            self.claims.remove(i);
        }

        /// Marks a game started. Requires an attached session and every seat claimed.
        /// Submit together with the session's `start` in one transaction.
        pub fn mark_started(&mut self, game_id: u64) {
            let gi = self.games.iter().position(|g| g.id == game_id).expect("unknown game");
            assert!(self.games[gi].status == 0, "game is not open");
            assert!(self.games[gi].session.is_some(), "no session attached");
            for seat in 1..=self.games[gi].capacity {
                assert!(self.claims.iter().any(|c| c.game_id == game_id && c.seat == seat), "unclaimed seat");
            }
            self.games[gi].status = 1;
        }

        pub fn mark_finished(&mut self, game_id: u64) {
            let gi = self.games.iter().position(|g| g.id == game_id).expect("unknown game");
            assert!(self.games[gi].status == 1, "game is not started");
            self.games[gi].status = 2;
        }

        pub fn seat_identity(&self, game_id: u64, seat: u8) -> Option<NonFungibleAddress> {
            self.claims.iter().find(|c| c.game_id == game_id && c.seat == seat).map(|c| c.identity.clone())
        }

        pub fn get_types(&self) -> Vec<GameType> { self.types.clone() }
        pub fn get_games(&self) -> Vec<Game> { self.games.clone() }
        pub fn get_claims(&self) -> Vec<SeatClaim> { self.claims.clone() }

        fn signer_identity() -> NonFungibleAddress {
            let identity = NonFungibleAddress::from_public_key(CallerContext::transaction_signer_public_key());
            CallerContext::assert_badge_in_scope(&identity);
            identity
        }
    }
}

// End of contracts/lobby/src/lib.rs
// Location: contracts/lobby/src/lib.rs · Created: 2026-11-15 21:49:02 UTC
