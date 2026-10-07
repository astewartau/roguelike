//! Seeded-run metadata: run records, the `runs_history.jsonl` append log,
//! and seed-text parsing for the start screen.
//!
//! All file IO here is best-effort: a missing or corrupt history file never
//! panics — bad lines are skipped, write errors are logged and ignored.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

/// History file, one JSON object per line, in the working directory.
pub const RUN_HISTORY_FILE: &str = "runs_history.jsonl";

/// How many past runs the start screen panel shows.
pub const PAST_RUNS_SHOWN: usize = 10;

/// One finished (or abandoned) run, as persisted to `runs_history.jsonl`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunRecord {
    /// Unix timestamp (seconds) when the run ended.
    pub timestamp: u64,
    /// The seed the run was generated from (retry it by entering this).
    pub seed: u64,
    /// Player class name ("Fighter", ...).
    pub class: String,
    /// Deepest floor reached (zero-based, as stored on GameState).
    pub floor_reached: u32,
    /// Game-clock seconds survived.
    pub game_time_survived_secs: f32,
    /// Hostile enemies slain.
    pub kills: u32,
    /// Best-effort cause of death ("Goblin", "burning", "abandoned", ...).
    pub cause_of_death: String,
}

/// How the start screen picks the next run's seed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SeedMode {
    /// A fresh seed from OS entropy each run.
    #[default]
    Random,
    /// A seed derived from today's UTC date — same dungeon for everyone today.
    Daily,
    /// Whatever the player typed in the seed field.
    Custom,
}

/// Current unix time in seconds (0 if the clock is somehow before the epoch).
pub fn unix_now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Civil date from days since the unix epoch (Howard Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // year of era
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year [0, 365]
    let mp = (5 * doy + 2) / 153; // month index, March-based [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Today's UTC date as "YYYY-MM-DD" (shown next to the Daily seed option).
pub fn daily_date_string() -> String {
    let (y, m, d) = civil_from_days((unix_now_secs() / 86_400) as i64);
    format!("{y:04}-{m:02}-{d:02}")
}

/// The seed for today's daily run: a stable hash of today's UTC date, so
/// every player (and every restart) gets the same dungeon until midnight UTC.
pub fn daily_seed() -> u64 {
    seed_from_text(&format!("daily-{}", daily_date_string()))
        .expect("daily seed text is never empty")
}

/// Append one record to the default history file. Errors are non-fatal.
pub fn append_run(record: &RunRecord) {
    append_run_to(Path::new(RUN_HISTORY_FILE), record);
}

/// Append one record as a JSON line to `path`. Errors are logged and ignored.
pub fn append_run_to(path: &Path, record: &RunRecord) {
    let line = match serde_json::to_string(record) {
        Ok(line) => line,
        Err(err) => {
            eprintln!("Warning: could not serialize run record: {err}");
            return;
        }
    };
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path);
    match file {
        Ok(mut file) => {
            if let Err(err) = writeln!(file, "{line}") {
                eprintln!("Warning: could not write run history: {err}");
            }
        }
        Err(err) => {
            eprintln!("Warning: could not open run history file: {err}");
        }
    }
}

/// Load the most recent `limit` runs from the default history file,
/// newest first. Missing file = empty list.
pub fn load_recent(limit: usize) -> Vec<RunRecord> {
    load_recent_from(Path::new(RUN_HISTORY_FILE), limit)
}

/// Load the most recent `limit` runs from `path`, newest first.
/// Unparseable lines are skipped; a missing file yields an empty list.
pub fn load_recent_from(path: &Path, limit: usize) -> Vec<RunRecord> {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut records: Vec<RunRecord> = contents
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    // Keep only the tail (most recent appends), newest first.
    if records.len() > limit {
        records.drain(..records.len() - limit);
    }
    records.reverse();
    records
}

/// Turn arbitrary seed-field text into a seed.
///
/// - Empty / whitespace-only text: `None` (caller rolls a random seed).
/// - A plain unsigned number parses as itself, so a seed copied off the
///   game-over screen reproduces the run exactly.
/// - Anything else (words like "dagger") hashes to a stable u64 (FNV-1a).
pub fn seed_from_text(text: &str) -> Option<u64> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(n) = trimmed.parse::<u64>() {
        return Some(n);
    }
    // FNV-1a 64-bit over the trimmed bytes.
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in trimmed.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    Some(hash)
}

/// A fresh random seed from OS entropy.
pub fn random_seed() -> u64 {
    use rand::Rng;
    rand::thread_rng().gen()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_record(seed: u64) -> RunRecord {
        RunRecord {
            timestamp: 1_770_000_000,
            seed,
            class: "Fighter".to_string(),
            floor_reached: 3,
            game_time_survived_secs: 412.5,
            kills: 17,
            cause_of_death: "Orc Warlord".to_string(),
        }
    }

    fn temp_history_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "grid_run_history_test_{tag}_{}.jsonl",
            std::process::id()
        ))
    }

    #[test]
    fn test_jsonl_round_trip() {
        let path = temp_history_path("round_trip");
        let _ = std::fs::remove_file(&path);

        let a = sample_record(111);
        let mut b = sample_record(222);
        b.class = "Necromancer".to_string();
        b.cause_of_death = "burning".to_string();
        append_run_to(&path, &a);
        append_run_to(&path, &b);

        let loaded = load_recent_from(&path, 10);
        // Newest first.
        assert_eq!(loaded, vec![b, a]);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_corrupt_lines_are_skipped() {
        let path = temp_history_path("corrupt");
        let record = sample_record(7);
        let good = serde_json::to_string(&record).expect("serialize test record");
        std::fs::write(
            &path,
            format!("not json at all\n{good}\n{{\"seed\": 1}}\n\n"),
        )
        .expect("write test file");

        let loaded = load_recent_from(&path, 10);
        assert_eq!(loaded, vec![record]);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_missing_file_is_empty() {
        let path = temp_history_path("does_not_exist");
        let _ = std::fs::remove_file(&path);
        assert!(load_recent_from(&path, 10).is_empty());
    }

    #[test]
    fn test_limit_keeps_newest() {
        let path = temp_history_path("limit");
        let _ = std::fs::remove_file(&path);
        for seed in 0..15u64 {
            append_run_to(&path, &sample_record(seed));
        }
        let loaded = load_recent_from(&path, 10);
        assert_eq!(loaded.len(), 10);
        // Newest (seed 14) first, oldest kept is seed 5.
        assert_eq!(loaded[0].seed, 14);
        assert_eq!(loaded[9].seed, 5);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_seed_from_text_numeric() {
        assert_eq!(seed_from_text("12345"), Some(12345));
        assert_eq!(seed_from_text("  42 "), Some(42));
        assert_eq!(seed_from_text(&u64::MAX.to_string()), Some(u64::MAX));
    }

    #[test]
    fn test_seed_from_text_words_hash_stably() {
        let a = seed_from_text("dagger");
        let b = seed_from_text("dagger");
        assert!(a.is_some());
        assert_eq!(a, b);
        assert_ne!(seed_from_text("dagger"), seed_from_text("Dagger"));
        // Whitespace around a word doesn't change the seed.
        assert_eq!(seed_from_text(" dagger "), seed_from_text("dagger"));
    }

    #[test]
    fn test_seed_from_text_empty_is_none() {
        assert_eq!(seed_from_text(""), None);
        assert_eq!(seed_from_text("   "), None);
    }

    #[test]
    fn test_civil_from_days() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_724), (2024, 1, 2));
        // Leap day.
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
    }

    #[test]
    fn test_daily_seed_is_stable_today() {
        assert_eq!(daily_seed(), daily_seed());
    }
}
