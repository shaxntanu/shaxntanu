//! How strongly the current and longest streaks show on the koi.

use crate::koi::{KoiDevelopment, RecentStreak};

const STREAK_FIN_FULL_WEEKS: f64 = 13.0;
const FADE_RATE_PER_WEEK: f64 = 0.5;
const FADE_WEEKS: f64 = 6.5;
/// Shapes the longest-streak log curve: smaller values rise faster at first.
const RECORD_CURVE_WEEKS: f64 = 0.6;
const NORMAL_COLOR_WEEKS: f64 = 4.0;
const VIVID_COLOR_WEEKS: f64 = 52.0;
/// A placeholder for when glimmer reaches full strength.
const FULL_GLIMMER_WEEKS: f64 = 208.0;

/// Streak mark strengths for the renderer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StreakMarks {
    /// Share of fin pixels in the body's streak color, from zero through one.
    pub streak_fin_strength: f64,
    /// Language color vibrancy: 0 dull, 1 normal, 2 extra bright and vivid.
    pub color_vibrancy: f64,
    /// Gin-rin glimmer on the color patches, from zero through one.
    pub glimmer_strength: f64,
}

/// Derive the streak marks from a development, measuring time against its
/// observation date.
///
/// Streak fins build linearly to full after STREAK_FIN_FULL_WEEKS of the recent
/// streak. Once it ends, they fade from the strength they reached on a negative
/// exponential that reaches zero FADE_WEEKS after the recorded end date. The
/// longest streak sets the other marks on one log curve: colors go from dull to
/// normal by NORMAL_COLOR_WEEKS and to vivid by VIVID_COLOR_WEEKS, then glimmer
/// builds until FULL_GLIMMER_WEEKS. The longest streak never shrinks, so neither do
/// these marks. This reads no clock.
pub fn calculate_streak_marks(koi_development: &KoiDevelopment) -> StreakMarks {
    let streak_fin_strength = match &koi_development.streaks.recent {
        None => 0.0,
        Some(RecentStreak::Current(streak_weeks)) => calculate_fin_build(streak_weeks.len()),
        Some(RecentStreak::Ended {
            weeks: streak_weeks,
            ended_on,
        }) => {
            let weeks_since_end =
                (koi_development.observed_on - *ended_on).num_days().max(0) as f64 / 7.0;
            calculate_fin_build(streak_weeks.len()) * calculate_fade(weeks_since_end)
        }
    };
    let record_strength = calculate_record_strength(koi_development.streaks.longest.len() as f64);
    let normal_strength = calculate_record_strength(NORMAL_COLOR_WEEKS);
    let vivid_strength = calculate_record_strength(VIVID_COLOR_WEEKS);
    let color_vibrancy = if record_strength < normal_strength {
        record_strength / normal_strength
    } else {
        1.0 + ((record_strength - normal_strength) / (vivid_strength - normal_strength)).min(1.0)
    };
    StreakMarks {
        streak_fin_strength,
        color_vibrancy,
        glimmer_strength: ((record_strength - vivid_strength) / (1.0 - vivid_strength))
            .clamp(0.0, 1.0),
    }
}

fn calculate_fin_build(streak_week_count: usize) -> f64 {
    (streak_week_count as f64 / STREAK_FIN_FULL_WEEKS).min(1.0)
}

/// Remaining share after a streak ends: a negative exponential shifted and scaled
/// so it starts at one and reaches zero at FADE_WEEKS.
fn calculate_fade(weeks_since_end: f64) -> f64 {
    let fade_floor = (-FADE_RATE_PER_WEEK * FADE_WEEKS).exp();
    (((-FADE_RATE_PER_WEEK * weeks_since_end).exp() - fade_floor) / (1.0 - fade_floor)).max(0.0)
}

fn calculate_record_strength(longest_weeks: f64) -> f64 {
    ((1.0 + longest_weeks / RECORD_CURVE_WEEKS).ln()
        / (1.0 + FULL_GLIMMER_WEEKS / RECORD_CURVE_WEEKS).ln())
    .min(1.0)
}
