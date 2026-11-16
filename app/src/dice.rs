//! Ootle Game Engine | app/src/dice.rs
//! Version: v0.2.0
//! Backend dice. Each roll is drawn from the operating system's CSPRNG (getrandom, rejection
//! sampled to 1..6) and stored under "game:round:turn:seat" before it is shown. A second roll
//! request for the same turn returns the stored roll, so a roll cannot be redrawn by asking
//! again. For a held seat the pay-or-roll choice is stored with the roll at roll time.
//! The store is a JSON file (config `roll_store`), written on every change.

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::PathBuf, time::{SystemTime, UNIX_EPOCH}};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Roll {
    pub game: u64,
    pub round: u64,
    pub turn: u64,
    pub seat: u8,
    pub die1: u8,
    pub die2: u8,
    /// Held seat only: true = pay the shop fee and storage, then move by this roll.
    pub pay: bool,
    pub rolled_at: u64,
}

pub struct RollStore { path: PathBuf, rolls: BTreeMap<String, Roll> }

pub fn key(game: u64, round: u64, turn: u64, seat: u8) -> String { format!("{game}:{round}:{turn}:{seat}") }

pub fn now() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) }

fn die() -> Result<u8, String> {
    loop {
        let mut b = [0u8; 1];
        getrandom::getrandom(&mut b).map_err(|e| format!("random source unavailable: {e}"))?;
        if b[0] < 252 { return Ok(b[0] % 6 + 1); } // 252 = 42 × 6: no modulo bias
    }
}

impl RollStore {
    pub fn open(path: PathBuf) -> Result<Self, String> {
        let rolls = match fs::read_to_string(&path) {
            Ok(t) => serde_json::from_str(&t).map_err(|e| format!("roll store {}: {e}", path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(e) => return Err(format!("roll store {}: {e}", path.display())),
        };
        Ok(Self { path, rolls })
    }

    fn save(&self) -> Result<(), String> {
        if let Some(d) = self.path.parent() { fs::create_dir_all(d).map_err(|e| e.to_string())?; }
        let tmp = self.path.with_extension("tmp");
        fs::write(&tmp, serde_json::to_string_pretty(&self.rolls).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        fs::rename(&tmp, &self.path).map_err(|e| e.to_string())
    }

    pub fn get(&self, game: u64, round: u64, turn: u64, seat: u8) -> Option<&Roll> { self.rolls.get(&key(game, round, turn, seat)) }

    /// Returns the stored roll for this turn, or draws, stores and returns a new one.
    pub fn roll(&mut self, game: u64, round: u64, turn: u64, seat: u8, pay: bool) -> Result<Roll, String> {
        if let Some(r) = self.get(game, round, turn, seat) { return Ok(r.clone()); }
        // Rolls of earlier turns of this game can no longer be used; drop them.
        self.rolls.retain(|_, r| r.game != game || r.turn >= turn);
        let r = Roll { game, round, turn, seat, die1: die()?, die2: die()?, pay, rolled_at: now() };
        self.rolls.insert(key(game, round, turn, seat), r.clone());
        self.save()?;
        Ok(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roll_is_stored_and_not_redrawn() {
        let p = std::env::temp_dir().join(format!("oge-rolls-{}.json", std::process::id()));
        let _ = fs::remove_file(&p);
        let mut s = RollStore::open(p.clone()).unwrap();
        let a = s.roll(1, 2, 6, 1, true).unwrap();
        assert!((1..=6).contains(&a.die1) && (1..=6).contains(&a.die2));
        let b = s.roll(1, 2, 6, 1, false).unwrap();
        assert_eq!(a, b, "same turn returns the stored roll and the stored pay choice");
        let reopened = RollStore::open(p.clone()).unwrap();
        assert_eq!(reopened.get(1, 2, 6, 1), Some(&a));
        s.roll(1, 2, 7, 2, false).unwrap();
        assert!(s.get(1, 2, 6, 1).is_none(), "older turn pruned");
        let mut hist = [0u32; 6];
        for _ in 0..6000 { hist[usize::from(die().unwrap() - 1)] += 1; }
        assert!(hist.iter().all(|c| *c > 800));
        let _ = fs::remove_file(&p);
    }
}

// End of app/src/dice.rs
// Location: app/src/dice.rs · Created: 2026-11-15 23:05:29 UTC
