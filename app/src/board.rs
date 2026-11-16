//! Ootle Game Engine | app/src/board.rs
//! Version: v0.4.0
//! Presentation catalogue for the Town Square Classic board: square names and kinds, the seat
//! colours and the 20 game pieces, and exact tTARI formatting. Rules and prices come from
//! contracts/town_square/src/rules.rs; nothing here affects settlement.
//! v0.4.0: colour belongs to the seat, not to the piece. Seats 1..10 use the ten most separated
//! colours; seats 11..20 fill the space between them. A piece is a shape only (drawn as a
//! single-colour SVG by app/web/pieces.js) and takes its seat's colour. Piece names carry no colour.

use ootle_town_square::rules;
use serde::Serialize;

pub const KIND_NAMES: [&str; 10] =
    ["go", "asset", "chance", "community", "fee", "penalty", "reward", "utility", "transit", "corner"];

pub fn square_name(sq: u16) -> String {
    if let Some(i) = rules::asset_index(sq) {
        return format!("Asset {}", (b'A' + i as u8) as char);
    }
    match sq {
        0 => "GO", 2 | 22 | 33 => "Chance", 4 | 24 | 37 => "Fee", 5 | 17 | 27 => "Community", 7 => "Penalty",
        9 => "Reward", 10 => "Maintenance Bay", 12 => "Utility", 15 | 35 => "Transit", 20 => "Free Park",
        30 => "Back to Maintenance", _ => "Square",
    }.to_owned()
}

pub fn square_kind(sq: u16) -> &'static str {
    KIND_NAMES[usize::from(rules::KINDS[usize::from(sq)])]
}

/// Seat colours, seat N at index N-1. Seats 1..4 as agreed; 5..10 the next most separated colours;
/// 11..20 in between.
pub const SEAT_COLOURS: [&str; 20] = [
    "#3fa7ff", "#ff9f43", "#2ee6a6", "#b48cff", "#ff5d8f", "#ffd23f", "#4dd0e1", "#a3e635", "#e879f9", "#ff7a59",
    "#ff8fb1", "#14b8a6", "#f59e0b", "#818cf8", "#bef264", "#fca5a5", "#94a3b8", "#eab308", "#ddd6fe", "#d1fae5",
];

pub fn seat_colour(seat: u8) -> &'static str {
    SEAT_COLOURS[usize::from(seat.clamp(1, 20) - 1)]
}

#[derive(Clone, Debug, Serialize)]
pub struct Piece { pub id: u8, pub name: &'static str }

/// Twenty pieces (shapes). Piece N is drawn by app/web/pieces.js under the same number.
pub const PIECES: [Piece; 20] = [
    Piece { id: 1, name: "Roadster" }, Piece { id: 2, name: "Sailboat" }, Piece { id: 3, name: "Jet" },
    Piece { id: 4, name: "Helicopter" }, Piece { id: 5, name: "Locomotive" }, Piece { id: 6, name: "Racer" },
    Piece { id: 7, name: "Tractor" }, Piece { id: 8, name: "Scooter" }, Piece { id: 9, name: "Speedboat" },
    Piece { id: 10, name: "Bus" }, Piece { id: 11, name: "Rocket" }, Piece { id: 12, name: "Pickup" },
    Piece { id: 13, name: "Bicycle" }, Piece { id: 14, name: "Liner" }, Piece { id: 15, name: "Fire engine" },
    Piece { id: 16, name: "Motorcycle" }, Piece { id: 17, name: "Cable car" }, Piece { id: 18, name: "Drone" },
    Piece { id: 19, name: "Tram" }, Piece { id: 20, name: "Saucer" },
];

/// The piece a seat shows: the chosen piece (1..20), or by default the piece with the seat's number.
pub fn piece(seat: u8, chosen: Option<u8>) -> &'static Piece {
    let pick = match chosen { Some(i) if (1..=20).contains(&i) => i, _ => seat.clamp(1, 20) };
    &PIECES[usize::from(pick - 1)]
}

/// Base units to "N.NNNNNN" (6 decimals, exact).
pub fn ttari(units: u64) -> String { format!("{}.{:06}", units / 1_000_000, units % 1_000_000) }

pub fn ttari_signed(units: i128) -> String {
    let sign = if units < 0 { "−" } else if units > 0 { "+" } else { "" };
    let a = units.unsigned_abs();
    format!("{sign}{}.{:06}", a / 1_000_000, a % 1_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_pieces_and_colours() {
        assert_eq!(square_name(1), "Asset A");
        assert_eq!(square_name(39), "Asset V");
        assert_eq!(square_name(30), "Back to Maintenance");
        assert_eq!(square_kind(12), "utility");
        assert_eq!(piece(3, None).name, "Jet");
        assert_eq!(piece(3, Some(1)).name, "Roadster");
        assert_eq!(seat_colour(1), "#3fa7ff");
        assert_eq!(seat_colour(4), "#b48cff");
        let mut c = SEAT_COLOURS.to_vec();
        c.sort();
        c.dedup();
        assert_eq!(c.len(), 20, "every seat colour is distinct");
        assert!(PIECES.iter().all(|p| !["Red", "Blue", "Green", "Orange", "Silver"].iter().any(|w| p.name.contains(w))));
        assert_eq!(ttari(4_500_000), "4.500000");
        assert_eq!(ttari_signed(-1_000_000), "−1.000000");
    }
}

// End of app/src/board.rs
// Location: app/src/board.rs · Created: 2026-11-16 15:40:30 UTC
