//! Ootle Game Engine | app/src/lots.rs
//! Version: v0.4.0
//! The lot store: which business stands on each owned lot of each game. Presentation only, kept by
//! the backend (JSON file `lots_store`); the chain records ownership and development level, not the
//! business. Rules:
//!   - a human buyer chooses one of the lot zone's three businesses at purchase; an AI buyer, or a
//!     purchase that names none, gets one at random;
//!   - an owned lot with no business (for example a lot bought before v0.4.0) is assigned one at
//!     random the first time the backend sees it (source "assigned");
//!   - the business stays through development and private sales; when the chain shows the lot
//!     unowned again (sale to the House, reset, regeneration) the record is removed and the lot is
//!     vacant for the next buyer.

use crate::catalogue::Catalogue;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::{Path, PathBuf}};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct LotChoice {
    pub business: String,
    /// Seat that owned the lot when the business was chosen or assigned.
    pub seat: u8,
    /// "player", "ai" or "assigned".
    pub source: String,
    pub at: u64,
}

#[derive(Default, Deserialize, Serialize)]
struct LotsFile { games: BTreeMap<u64, BTreeMap<u16, LotChoice>> }

pub struct Lots { path: PathBuf, f: LotsFile }

/// A uniformly random index in 0..n from the operating system's random source.
pub fn random_index(n: usize) -> Result<usize, String> {
    if n == 0 { return Err("nothing to choose from".into()); }
    let limit = 256 - 256 % n;
    loop {
        let mut b = [0u8; 1];
        getrandom::getrandom(&mut b).map_err(|e| format!("random source unavailable: {e}"))?;
        if usize::from(b[0]) < limit { return Ok(usize::from(b[0]) % n); }
    }
}

fn store(path: &Path, f: &LotsFile) -> Result<(), String> {
    if let Some(d) = path.parent() { fs::create_dir_all(d).map_err(|e| e.to_string())?; }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, serde_json::to_string_pretty(f).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

impl Lots {
    pub fn open(path: PathBuf) -> Result<Self, String> {
        let f = match fs::read_to_string(&path) {
            Ok(t) => serde_json::from_str(&t).map_err(|e| format!("{}: {e}", path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => LotsFile::default(),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        Ok(Self { path, f })
    }

    pub fn get(&self, game: u64, square: u16) -> Option<&LotChoice> { self.f.games.get(&game).and_then(|g| g.get(&square)) }

    pub fn game(&self, game: u64) -> BTreeMap<u16, LotChoice> { self.f.games.get(&game).cloned().unwrap_or_default() }

    /// Brings the store in line with the chain's owners vector (40 entries, 0 = unowned):
    /// removes records of unowned lots and assigns a random business to owned lots without one.
    /// Returns true when the store changed (and was saved).
    pub fn sync(&mut self, game: u64, owners: &[u8], cat: &Catalogue, now: u64) -> Result<bool, String> {
        let mut changed = false;
        let g = self.f.games.entry(game).or_default();
        let before = g.len();
        g.retain(|sq, _| owners.get(usize::from(*sq)).is_some_and(|o| *o != 0));
        changed |= g.len() != before;
        for lot in &cat.lots {
            let owner = owners.get(usize::from(lot.square)).copied().unwrap_or(0);
            if owner != 0 && !g.contains_key(&lot.square) {
                let b = &lot.businesses[random_index(lot.businesses.len())?];
                g.insert(lot.square, LotChoice { business: b.id.clone(), seat: owner, source: "assigned".into(), at: now });
                changed = true;
            }
        }
        if changed { store(&self.path, &self.f)?; }
        Ok(changed)
    }

    /// Records the business chosen at a purchase. `business` None (AI or not named) picks at random.
    /// An id that is not one of the lot's three businesses is refused.
    pub fn choose(&mut self, game: u64, square: u16, seat: u8, business: Option<&str>, source: &str, cat: &Catalogue,
                  now: u64) -> Result<LotChoice, String> {
        let lot = cat.lot(square).ok_or_else(|| format!("square {square} is not a lot"))?;
        let id = match business {
            Some(id) => lot.businesses.iter().find(|b| b.id == id).map(|b| b.id.clone())
                .ok_or_else(|| format!("business {id} is not offered on square {square}"))?,
            None => lot.businesses[random_index(lot.businesses.len())?].id.clone(),
        };
        let c = LotChoice { business: id, seat, source: source.into(), at: now };
        self.f.games.entry(game).or_default().insert(square, c.clone());
        store(&self.path, &self.f)?;
        Ok(c)
    }

    /// Checks a business id before a purchase is submitted (same rule as `choose`).
    pub fn check(square: u16, business: Option<&str>, cat: &Catalogue) -> Result<(), String> {
        let lot = cat.lot(square).ok_or_else(|| format!("square {square} is not a lot"))?;
        match business {
            Some(id) if !lot.businesses.iter().any(|b| b.id == id) => Err(format!("business {id} is not offered on square {square}")),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sync_assign_choose_and_clear() {
        let cat = Catalogue::load(Path::new("config/catalogue.json")).unwrap();
        let p = std::env::temp_dir().join(format!("oge-lots-{}.json", std::process::id()));
        let _ = fs::remove_file(&p);
        let mut l = Lots::open(p.clone()).unwrap();
        let mut owners = [0u8; 40];
        owners[16] = 1;
        owners[23] = 2;
        assert!(l.sync(1, &owners, &cat, 5).unwrap());
        let a = l.get(1, 16).unwrap().clone();
        assert_eq!((a.seat, a.source.as_str()), (1, "assigned"));
        assert!(cat.business(16, &a.business).is_some());
        assert!(!l.sync(1, &owners, &cat, 6).unwrap(), "nothing changes on a second sync");
        // Purchase with a named business, then the lot returns to the House.
        owners[26] = 3;
        assert!(l.choose(1, 26, 3, Some("not-a-business"), "player", &cat, 7).is_err());
        let c = l.choose(1, 26, 3, Some("gearbox-garage"), "player", &cat, 7).unwrap();
        assert_eq!(c.business, "gearbox-garage");
        assert!(Lots::check(26, Some("copper-kettle"), &cat).is_err());
        let r = l.choose(1, 1, 4, None, "ai", &cat, 8).unwrap();
        assert!(cat.business(1, &r.business).is_some());
        owners[16] = 0;
        owners[1] = 4;
        l.sync(1, &owners, &cat, 9).unwrap();
        assert!(l.get(1, 16).is_none(), "unowned lot cleared");
        assert_eq!(Lots::open(p.clone()).unwrap().get(1, 26).unwrap().business, "gearbox-garage");
        let mut hist = [0usize; 3];
        for _ in 0..3000 { hist[random_index(3).unwrap()] += 1; }
        assert!(hist.iter().all(|h| *h > 800));
        let _ = fs::remove_file(&p);
    }
}

// End of app/src/lots.rs
// Location: app/src/lots.rs · Created: 2026-11-16 15:40:30 UTC
