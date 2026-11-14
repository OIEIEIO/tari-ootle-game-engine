//! Live-only integration constraints: no mock economy is initialized.
use ootle_asset_game::config::Config;
use std::path::Path;
#[test]
fn config_loads_for_local_swarm() {
    Config::load(Path::new("config")).expect("valid live Ootle configuration");
}
#[test]
fn configuration_rejects_invalid_treasury_percentages() {
    let mut config=Config::load(Path::new("config")).expect("valid config");
    config.economy.revenue_rules[0].chance_bps=999;
    assert!(config.validate().is_err());
}
