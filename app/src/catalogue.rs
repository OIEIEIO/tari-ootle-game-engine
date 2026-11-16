//! Ootle Game Engine | app/src/catalogue.rs
//! Version: v0.4.0
//! The Town Square catalogue (app/config/catalogue.json): every one of the 40 squares has a
//! landing scene. The 22 asset lots each have a zone and exactly three businesses (name, theme,
//! colours, signature motif, ambient layer); the 18 special squares each have a named scene.
//! Presentation only: settlement never reads it. Loaded and validated once at start; a catalogue
//! that does not cover the board exactly is refused.

use ootle_town_square::rules;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Business {
    pub id: String,
    pub name: String,
    pub theme: String,
    pub main: String,
    pub accent: String,
    /// Up to 8 letters, shown on the flagship's vertical sign.
    pub short: String,
    /// Signature detail drawn by app/web/scenes.js.
    pub motif: String,
    /// Ambient layer played by app/web/audio.js on top of the zone's base ambience.
    pub layer: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Lot {
    pub square: u16,
    pub asset: String,
    pub zone: String,
    pub zone_name: String,
    pub district: String,
    pub businesses: Vec<Business>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Special {
    pub square: u16,
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    pub shows: String,
    pub scene: String,
    pub ambient: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct District { pub name: String, pub squares: Vec<u16> }

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Catalogue {
    pub version: String,
    pub town: String,
    pub districts: Vec<District>,
    pub lots: Vec<Lot>,
    pub specials: Vec<Special>,
}

fn is_hex_colour(s: &str) -> bool { s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit()) }

impl Catalogue {
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = fs::read_to_string(path).map_err(|e| format!("catalogue {}: {e}", path.display()))?;
        let c: Catalogue = serde_json::from_str(&text).map_err(|e| format!("catalogue {}: {e}", path.display()))?;
        c.validate().map_err(|e| format!("catalogue {}: {e}", path.display()))?;
        Ok(c)
    }

    /// Exactly the 22 asset squares as lots (three distinct businesses each) and the other 18 squares as specials.
    pub fn validate(&self) -> Result<(), String> {
        let mut lots: Vec<u16> = self.lots.iter().map(|l| l.square).collect();
        lots.sort_unstable();
        let assets: Vec<u16> = rules::ASSETS.iter().map(|(s, _)| *s).collect();
        if lots != assets { return Err("lots must be exactly the 22 asset squares".into()); }
        let mut specials: Vec<u16> = self.specials.iter().map(|s| s.square).collect();
        specials.sort_unstable();
        let others: Vec<u16> = (0..rules::BOARD_SIZE).filter(|q| !rules::is_asset(*q)).collect();
        if specials != others { return Err("specials must be exactly the 18 non-asset squares".into()); }
        let mut ids = vec![];
        for l in &self.lots {
            if l.businesses.len() != 3 { return Err(format!("square {}: exactly three businesses required", l.square)); }
            for b in &l.businesses {
                if b.id.is_empty() || !b.id.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-') {
                    return Err(format!("square {}: invalid business id {:?}", l.square, b.id));
                }
                if !is_hex_colour(&b.main) || !is_hex_colour(&b.accent) { return Err(format!("{}: colours must be #rrggbb", b.id)); }
                if b.short.is_empty() || b.short.chars().count() > 8 { return Err(format!("{}: short sign must be 1..8 letters", b.id)); }
                if b.name.trim().is_empty() || b.motif.is_empty() || b.layer.is_empty() { return Err(format!("{}: name, motif and layer required", b.id)); }
                ids.push(b.id.clone());
            }
        }
        let n = ids.len();
        ids.sort();
        ids.dedup();
        if ids.len() != n { return Err("business ids must be unique".into()); }
        Ok(())
    }

    pub fn lot(&self, square: u16) -> Option<&Lot> { self.lots.iter().find(|l| l.square == square) }

    pub fn business(&self, square: u16, id: &str) -> Option<&Business> {
        self.lot(square).and_then(|l| l.businesses.iter().find(|b| b.id == id))
    }

    pub fn special(&self, square: u16) -> Option<&Special> { self.specials.iter().find(|s| s.square == square) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shipped_catalogue_covers_the_board() {
        let c = Catalogue::load(Path::new("config/catalogue.json")).unwrap();
        assert_eq!(c.lots.len(), 22);
        assert_eq!(c.specials.len(), 18);
        assert_eq!(c.lots.iter().map(|l| l.businesses.len()).sum::<usize>(), 66);
        assert_eq!(c.business(16, "copper-kettle").unwrap().name, "The Copper Kettle");
        assert_eq!(c.lot(26).unwrap().zone, "garage");
        assert_eq!(c.special(0).unwrap().name, "The Town Gate");
        let mut bad = c.clone();
        bad.lots[0].businesses.pop();
        assert!(bad.validate().is_err());
        let mut bad = c.clone();
        bad.lots[1].businesses[0].id = bad.lots[0].businesses[0].id.clone();
        assert!(bad.validate().unwrap_err().contains("unique"));
    }
}

// End of app/src/catalogue.rs
// Location: app/src/catalogue.rs · Created: 2026-11-16 15:40:30 UTC
