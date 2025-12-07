//! Experience and leveling system.

use crate::components::{Experience, Stats};
use crate::constants::*;

/// XP needed to reach the next level
pub fn xp_for_level(level: u32) -> u32 {
    level * XP_PER_LEVEL_MULTIPLIER
}

/// Calculate XP progress toward next level (0.0 to 1.0)
pub fn xp_progress(exp: &Experience) -> f32 {
    exp.current as f32 / xp_for_level(exp.level) as f32
}

/// Add XP to an experience component, handling level ups
pub fn grant_xp(exp: &mut Experience, amount: u32) -> bool {
    exp.current += amount;
    let mut leveled_up = false;
    while exp.current >= xp_for_level(exp.level) {
        exp.current -= xp_for_level(exp.level);
        exp.level += 1;
        leveled_up = true;
    }
    leveled_up
}

/// Calculate total stat points
pub fn stats_total(stats: &Stats) -> i32 {
    stats.strength + stats.intelligence + stats.agility
}

/// Calculate XP value of an entity based on its stats
pub fn calculate_xp_value(stats: Option<&Stats>) -> u32 {
    stats.map(|s| stats_total(s) as u32).unwrap_or(5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xp_for_level() {
        assert_eq!(xp_for_level(1), XP_PER_LEVEL_MULTIPLIER);
        assert_eq!(xp_for_level(2), 2 * XP_PER_LEVEL_MULTIPLIER);
        assert_eq!(xp_for_level(5), 5 * XP_PER_LEVEL_MULTIPLIER);
    }

    #[test]
    fn test_xp_progress() {
        let exp = Experience { current: 50, level: 1 };
        let progress = xp_progress(&exp);
        assert!(progress > 0.0 && progress <= 1.0);
    }

    #[test]
    fn test_grant_xp_no_level_up() {
        let mut exp = Experience { current: 0, level: 1 };
        let leveled = grant_xp(&mut exp, 10);
        assert!(!leveled);
        assert_eq!(exp.current, 10);
        assert_eq!(exp.level, 1);
    }

    #[test]
    fn test_grant_xp_level_up() {
        let mut exp = Experience { current: 0, level: 1 };
        let xp_needed = xp_for_level(1);
        let leveled = grant_xp(&mut exp, xp_needed);
        assert!(leveled);
        assert_eq!(exp.level, 2);
        assert_eq!(exp.current, 0);
    }

    #[test]
    fn test_grant_xp_multiple_level_ups() {
        let mut exp = Experience { current: 0, level: 1 };
        // Give enough XP for multiple levels
        let xp_needed = xp_for_level(1) + xp_for_level(2) + xp_for_level(3);
        let leveled = grant_xp(&mut exp, xp_needed);
        assert!(leveled);
        assert_eq!(exp.level, 4);
        assert_eq!(exp.current, 0);
    }

    #[test]
    fn test_stats_total() {
        let stats = Stats { strength: 5, intelligence: 3, agility: 2 };
        assert_eq!(stats_total(&stats), 10);
    }

    #[test]
    fn test_calculate_xp_value_with_stats() {
        let stats = Stats { strength: 5, intelligence: 3, agility: 2 };
        assert_eq!(calculate_xp_value(Some(&stats)), 10);
    }

    #[test]
    fn test_calculate_xp_value_without_stats() {
        assert_eq!(calculate_xp_value(None), 5);
    }
}
