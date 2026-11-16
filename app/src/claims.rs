//! Ootle Game Engine | app/src/claims.rs
//! Version: v0.4.0
//! Who plays each seat. Seat keys stay in the backend (custodial) and each seat's on-chain claim
//! in the Lobby is held by its own key, so claiming a seat from the web page is recorded here and
//! the chain does not change. A browser holds a random player token; the token's seats are the
//! only seats it can act for. One game per token: release before claiming in another game.
//! A released seat keeps its cash, assets and position; the next player takes it over or resets it.
//! AI seats belong to a game's humans: when the last human leaves, the game's AI seats are released
//! too, so a game with no humans does not keep moving and spending House funds.
//! Also holds the leaderboard milestones (a seat's stint reaching the milestone round count).
//! Both stores are JSON files written on every change.
//! v0.4.0: a seat's `vehicle` field holds the chosen piece id (1..20, board::PIECES); the seat colour
//! comes from the seat number (board::seat_colour), not from the piece. Stored files are unchanged.

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::{Path, PathBuf}};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Holder {
    Open,
    Human { token: String, name: String, since: u64 },
    Ai,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SeatClaim {
    #[serde(flatten)]
    pub holder: Holder,
    /// Vehicle piece, 1..20 (board::VEHICLES); None = the seat's default piece.
    #[serde(default)]
    pub vehicle: Option<u8>,
}

impl Default for SeatClaim { fn default() -> Self { SeatClaim { holder: Holder::Open, vehicle: None } } }

#[derive(Default, Deserialize, Serialize)]
struct ClaimsFile { games: BTreeMap<u64, BTreeMap<u8, SeatClaim>> }

pub struct Claims { path: PathBuf, f: ClaimsFile }

fn load<T: for<'a> Deserialize<'a> + Default>(path: &Path) -> Result<T, String> {
    match fs::read_to_string(path) {
        Ok(t) => serde_json::from_str(&t).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn store<T: Serialize>(path: &Path, v: &T) -> Result<(), String> {
    if let Some(d) = path.parent() { fs::create_dir_all(d).map_err(|e| e.to_string())?; }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, serde_json::to_string_pretty(v).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub fn valid_name(name: &str) -> Result<String, String> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > 24 { return Err("player name must be 1..24 characters".into()); }
    if n.chars().any(|c| c.is_control() || "<>&\"'`".contains(c)) { return Err("player name contains a character that is not allowed".into()); }
    Ok(n.to_owned())
}

impl Claims {
    pub fn open(path: PathBuf) -> Result<Self, String> { let f = load(&path)?; Ok(Self { path, f }) }
    fn save(&self) -> Result<(), String> { store(&self.path, &self.f) }

    pub fn seat(&self, game: u64, seat: u8) -> SeatClaim {
        self.f.games.get(&game).and_then(|g| g.get(&seat)).cloned().unwrap_or_default()
    }

    pub fn seats(&self, game: u64, capacity: u8) -> Vec<SeatClaim> { (1..=capacity).map(|s| self.seat(game, s)).collect() }

    pub fn humans(&self, game: u64) -> usize {
        self.f.games.get(&game).map(|g| g.values().filter(|c| matches!(c.holder, Holder::Human { .. })).count()).unwrap_or(0)
    }

    pub fn is_ai(&self, game: u64, seat: u8) -> bool { self.seat(game, seat).holder == Holder::Ai }

    /// The game and seats a token holds.
    pub fn of_token(&self, token: &str) -> Option<(u64, Vec<u8>)> {
        for (g, seats) in &self.f.games {
            let mine: Vec<u8> = seats.iter().filter(|(_, c)| matches!(&c.holder, Holder::Human { token: t, .. } if t == token))
                .map(|(s, _)| *s).collect();
            if !mine.is_empty() { return Some((*g, mine)); }
        }
        None
    }

    pub fn owns(&self, token: &str, game: u64, seat: u8) -> bool {
        matches!(self.seat(game, seat).holder, Holder::Human { token: ref t, .. } if t == token)
    }

    /// Claims `seats` for `token` in `game`; when the game has no humans yet, `ai` more open seats
    /// (lowest numbers first) become AI. `vehicles` pairs a claimed seat with a piece.
    #[allow(clippy::too_many_arguments)]
    pub fn claim(&mut self, token: &str, name: &str, game: u64, capacity: u8, seats: &[u8], ai: u8,
                 vehicles: &[(u8, u8)], now: u64) -> Result<(), String> {
        let name = valid_name(name)?;
        if token.len() != 32 || !token.bytes().all(|b| b.is_ascii_hexdigit()) { return Err("invalid player token".into()); }
        if seats.is_empty() { return Err("choose at least one seat".into()); }
        if let Some((g, _)) = self.of_token(token) {
            if g != game { return Err(format!("release your seat in game {g} before joining another game")); }
        }
        for s in seats {
            if *s == 0 || *s > capacity { return Err("invalid seat".into()); }
            if self.seat(game, *s).holder != Holder::Open { return Err(format!("seat {s} is not open")); }
        }
        let fresh = self.humans(game) == 0;
        let open_after: Vec<u8> = (1..=capacity).filter(|s| !seats.contains(s) && self.seat(game, *s).holder == Holder::Open).collect();
        if ai > 0 && !fresh { return Err("AI opponents are chosen by the first player in a game".into()); }
        if usize::from(ai) > open_after.len() { return Err(format!("only {} seats are left for AI opponents", open_after.len())); }
        for (s, v) in vehicles {
            if !seats.contains(s) || !(1..=20).contains(v) { return Err("invalid vehicle choice".into()); }
        }
        let g = self.f.games.entry(game).or_default();
        for s in seats {
            let vehicle = vehicles.iter().find(|(x, _)| x == s).map(|(_, v)| *v);
            g.insert(*s, SeatClaim { holder: Holder::Human { token: token.to_owned(), name: name.clone(), since: now }, vehicle });
        }
        for s in open_after.iter().take(usize::from(ai)) {
            g.insert(*s, SeatClaim { holder: Holder::Ai, vehicle: None });
        }
        self.save()
    }

    /// Releases every seat the token holds. The seats keep their on-chain state. When no human is
    /// left in that game, its AI seats are released as well. Returns the game, if any.
    pub fn release(&mut self, token: &str) -> Result<Option<u64>, String> {
        let Some((game, mine)) = self.of_token(token) else { return Ok(None) };
        let g = self.f.games.entry(game).or_default();
        for s in mine { g.insert(s, SeatClaim::default()); }
        if !g.values().any(|c| matches!(c.holder, Holder::Human { .. })) {
            for c in g.values_mut() { if c.holder == Holder::Ai { *c = SeatClaim::default(); } }
        }
        self.save()?;
        Ok(Some(game))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Milestone {
    pub game: u64, pub seat: u8, pub stint: u32, pub kind: String, pub name: String,
    pub rounds: u64, pub round: u64, pub net_worth: u64, pub cash: u64, pub at: u64,
}

#[derive(Default, Deserialize, Serialize)]
struct BoardFile { milestones: Vec<Milestone> }

pub struct Leaderboard { path: PathBuf, f: BoardFile }

impl Leaderboard {
    pub fn open(path: PathBuf) -> Result<Self, String> { let f = load(&path)?; Ok(Self { path, f }) }
    pub fn milestones(&self) -> &[Milestone] { &self.f.milestones }
    /// Records the milestone once per (game, seat, stint).
    pub fn record(&mut self, m: Milestone) -> Result<bool, String> {
        if self.f.milestones.iter().any(|x| x.game == m.game && x.seat == m.seat && x.stint == m.stint) { return Ok(false); }
        self.f.milestones.push(m);
        store(&self.path, &self.f)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const T1: &str = "0123456789abcdef0123456789abcdef";
    const T2: &str = "fedcba9876543210fedcba9876543210";

    #[test]
    fn claim_release_and_ai() {
        let p = std::env::temp_dir().join(format!("oge-claims-{}.json", std::process::id()));
        let _ = fs::remove_file(&p);
        let mut c = Claims::open(p.clone()).unwrap();
        c.claim(T1, "Jorge", 1, 4, &[1], 3, &[(1, 6)], 10).unwrap();
        assert_eq!(c.humans(1), 1);
        assert!(c.is_ai(1, 2) && c.is_ai(1, 4));
        assert_eq!(c.seat(1, 1).vehicle, Some(6));
        assert!(c.claim(T2, "Ana", 1, 4, &[2], 0, &[], 11).unwrap_err().contains("not open"));
        assert!(c.claim(T1, "Jorge", 2, 4, &[1], 0, &[], 12).unwrap_err().contains("release your seat"));
        // Two humans, one browser, plus two AI.
        c.claim(T2, "Ana", 2, 4, &[1, 2], 2, &[], 13).unwrap();
        assert_eq!(c.of_token(T2), Some((2, vec![1, 2])));
        assert!(c.claim(T1, "Jorge", 2, 4, &[3], 0, &[], 14).is_err(), "seat 3 is AI");
        let reopened = Claims::open(p.clone()).unwrap();
        assert!(reopened.owns(T1, 1, 1));
        // Last human leaves: AI seats are released too.
        assert_eq!(c.release(T1).unwrap(), Some(1));
        assert_eq!(c.humans(1), 0);
        assert!((1..=4).all(|s| c.seat(1, s).holder == Holder::Open));
        assert!(c.claim(T1, "<b>", 1, 4, &[1], 0, &[], 15).is_err());
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn milestone_once_per_stint() {
        let p = std::env::temp_dir().join(format!("oge-board-{}.json", std::process::id()));
        let _ = fs::remove_file(&p);
        let mut b = Leaderboard::open(p.clone()).unwrap();
        let m = Milestone { game: 1, seat: 2, stint: 0, kind: "ai".into(), name: "AI".into(), rounds: 100, round: 101,
            net_worth: 1, cash: 1, at: 0 };
        assert!(b.record(m.clone()).unwrap());
        assert!(!b.record(m).unwrap());
        assert_eq!(Leaderboard::open(p.clone()).unwrap().milestones().len(), 1);
        let _ = fs::remove_file(&p);
    }
}

// End of app/src/claims.rs
// Location: app/src/claims.rs · Created: 2026-11-16 04:06:01 UTC
