//! Ootle Game Engine | contracts/registry/src/lib.rs
//! Version: v0.1.0
//! Player Registry: one profile per player identity, shared by every game.
//! An identity is the public-key badge of the key that signs the registration transaction,
//! so no identity argument is passed and none can be supplied for someone else.
//! Holds no funds.

use tari_template_lib::prelude::*;

#[template]
mod ootle_registry {
    use super::*;

    pub struct OotleRegistry {
        profiles: Vec<Profile>,
    }

    #[derive(Clone)]
    pub struct Profile {
        pub identity: NonFungibleAddress,
        pub name: String,
        pub icon: u8,
    }

    impl OotleRegistry {
        pub fn new() -> Component<Self> {
            Component::new(Self { profiles: vec![] })
                .with_access_rules(ComponentAccessRules::new()
                    .method("register", rule!(allow_all))
                    .method("update_profile", rule!(allow_all))
                    .method("is_registered", rule!(allow_all))
                    .method("get_profile", rule!(allow_all))
                    .method("get_profiles", rule!(allow_all))
                    .default(rule!(deny_all)))
                .with_owner_rule(OwnerRule::OwnedBySigner)
                .create()
        }

        /// Registers the transaction signer's identity with a display name and icon.
        pub fn register(&mut self, name: String, icon: u8) {
            let identity = Self::signer_identity();
            Self::check_profile(&name, icon);
            assert!(!self.profiles.iter().any(|p| p.identity == identity), "identity already registered");
            self.profiles.push(Profile { identity, name, icon });
        }

        /// Changes the signer's own display name and icon.
        pub fn update_profile(&mut self, name: String, icon: u8) {
            let identity = Self::signer_identity();
            Self::check_profile(&name, icon);
            let p = self.profiles.iter_mut().find(|p| p.identity == identity).expect("identity not registered");
            p.name = name;
            p.icon = icon;
        }

        pub fn is_registered(&self, identity: NonFungibleAddress) -> bool {
            self.profiles.iter().any(|p| p.identity == identity)
        }

        /// (name, icon) for a registered identity.
        pub fn get_profile(&self, identity: NonFungibleAddress) -> Option<(String, u8)> {
            self.profiles.iter().find(|p| p.identity == identity).map(|p| (p.name.clone(), p.icon))
        }

        pub fn get_profiles(&self) -> Vec<Profile> {
            self.profiles.clone()
        }

        fn signer_identity() -> NonFungibleAddress {
            let identity = NonFungibleAddress::from_public_key(CallerContext::transaction_signer_public_key());
            CallerContext::assert_badge_in_scope(&identity);
            identity
        }

        fn check_profile(name: &str, icon: u8) {
            assert!(!name.is_empty() && name.len() <= 48, "display name must be 1..48 bytes");
            assert!(icon < 32, "icon index must be 0..31");
        }
    }
}

// End of contracts/registry/src/lib.rs
// Location: contracts/registry/src/lib.rs · Created: 2026-11-15 21:49:02 UTC
