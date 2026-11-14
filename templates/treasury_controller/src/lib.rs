//! Ootle Asset Game | templates/treasury_controller/src/lib.rs
//! First independent Tari Ootle WASM treasury-distribution component.
//! Notes: This component does NOT custody funds. Each distribution consumes one
//! caller-provided revenue Bucket and deposits shares atomically in three
//! existing, independently addressed Account components.
//! IMPORTANT: Until validated against the exact local Tari release, this is
//! candidate on-chain code, NOT a deployed or audited contract.

use tari_template_lib::prelude::*;

#[template]
mod ootle_treasury_controller {
    use super::*;

    pub struct OotleTreasuryController {
        house: ComponentAddress,
        community: ComponentAddress,
        chance: ComponentAddress,
        resource: ResourceAddress,
        // Base-point allocations per revenue category, not wallet balances.
        community_property_bps: u16,
        chance_property_bps: u16,
        community_fee_bps: u16,
        chance_fee_bps: u16,
        community_sponsor_bps: u16,
        chance_sponsor_bps: u16,
        distributions: u64,
    }

    impl OotleTreasuryController {
        /// Instantiate one controller for the three existing treasury accounts.
        /// Fixed recipient addresses prevent a caller from redirecting revenue.
        pub fn new(
            house: ComponentAddress,
            community: ComponentAddress,
            chance: ComponentAddress,
            resource: ResourceAddress,
        ) -> Component<Self> {
            assert!(house != community && house != chance && community != chance, "duplicate treasuries");
            Component::new(Self {
                house, community, chance, resource,
                community_property_bps: 500,
                chance_property_bps: 500,
                community_fee_bps: 500,
                chance_fee_bps: 500,
                community_sponsor_bps: 1500,
                chance_sponsor_bps: 500,
                distributions: 0,
            })
            .with_access_rules(
                ComponentAccessRules::new()
                    .method("distribute", rule!(allow_all))
                    .method("get_distribution_count", rule!(allow_all))
                    .method("get_rates", rule!(allow_all))
                    .default(rule!(deny_all))
            )
            .with_owner_rule(OwnerRule::OwnedBySigner)
            .create()
        }

        /// Category: 0=asset sale, 1=game fee, 2=sponsorship.
        /// Each integer amount denotes the exact base units contained in revenue.
        /// Validation of the revenue Bucket defeats caller-supplied amount spoofing.
        /// Important: a source-account withdrawal must precede this method in
        /// the signed transaction. This method itself cannot debit an account.
        pub fn distribute(&mut self, category: u8, mut revenue: Bucket, units: u64) {
            assert!(units > 0, "zero revenue");
            assert!(revenue.resource_address() == self.resource, "incorrect resource");
            assert!(revenue.amount() == Amount::from(units), "amount does not match bucket");
            let (community_bps, chance_bps) = self.rates_for(category);
            assert!(u32::from(community_bps) + u32::from(chance_bps) <= 10_000, "invalid bps");

            // u128 intermediates prevent multiplication overflow. Any rounding
            // remainder belongs to the House, consistent with the native engine.
            let community_units = ((u128::from(units) * u128::from(community_bps)) / 10_000) as u64;
            let chance_units = ((u128::from(units) * u128::from(chance_bps)) / 10_000) as u64;

            if community_units > 0 {
                let bucket = revenue.take(Amount::from(community_units));
                ComponentManager::get(self.community).invoke("deposit", args![bucket]);
            }
            if chance_units > 0 {
                let bucket = revenue.take(Amount::from(chance_units));
                ComponentManager::get(self.chance).invoke("deposit", args![bucket]);
            }
            // Remainder — including rounding dust — is deposited into the House.
            ComponentManager::get(self.house).invoke("deposit", args![revenue]);
            self.distributions = self.distributions.checked_add(1).expect("counter overflow");
        }

        pub fn get_distribution_count(&self) -> u64 { self.distributions }

        /// Returns the stored community/chance basis points for a category.
        pub fn get_rates(&self, category: u8) -> (u16, u16) { self.rates_for(category) }

        fn rates_for(&self, category: u8) -> (u16, u16) {
            match category {
                0 => (self.community_property_bps, self.chance_property_bps),
                1 => (self.community_fee_bps, self.chance_fee_bps),
                2 => (self.community_sponsor_bps, self.chance_sponsor_bps),
                _ => panic!("unknown category"),
            }
        }
    }
}

// End of templates/treasury_controller/src/lib.rs
