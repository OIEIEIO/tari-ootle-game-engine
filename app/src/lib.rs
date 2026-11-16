//! Ootle Game Engine | app/src/lib.rs
//! Version: v0.4.0
//! Backend library: configuration, wallet client, chain reads and decoding, the contract
//! simulator, dice, game view, write pipeline and structured errors. main.rs is HTTP only.
//! v0.3.0 adds seat claims and the leaderboard (claims.rs) and the AI and auto-roll driver (ai.rs).
//! v0.4.0 adds the Town Square catalogue (catalogue.rs) and the lot store (lots.rs).

pub mod ai;
pub mod api_error;
pub mod board;
pub mod catalogue;
pub mod chain;
pub mod claims;
pub mod config;
pub mod decode;
pub mod dice;
pub mod lots;
pub mod ops;
pub mod sim;
pub mod view;
pub mod wallet;

// End of app/src/lib.rs
// Location: app/src/lib.rs · Created: 2026-11-16 15:40:30 UTC
