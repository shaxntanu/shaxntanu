//! Swimming over time: the pose for each frame of a behavior's seamless loop.

use std::f64::consts::{PI, TAU};

use crate::behavior::{ActivityEnergy, Behavior};

/// Frames per second of every behavior's loop; GIF frame delays derive from it.
pub const FRAMES_PER_SECOND: u32 = 15;

const TAIL_BEND_PER_WAVE_STRENGTH: f64 = 0.45;
const TAIL_BEND_LAG_RADIANS: f64 = 1.7;
const TAIL_SPLIT_RADIANS: f64 = 0.1;
const TAIL_SPLIT_LAG_RADIANS: f64 = 2.6;
/// A side fin's tip points where its root pointed this many frames earlier.
const SIDE_FIN_TRAIL_FRAMES: u32 = 3;
const PELVIC_SHARE_OF_SIDE_FIN_OPENING: f64 = 0.5;
const WHISKER_SWAY_RADIANS: f64 = 0.15;
const WHISKER_SWAY_LAG_RADIANS: f64 = 0.6;

/// Rest and surface feeding share this loop.
const REST_FRAME_COUNT: u32 = 90;
const REST_DEPTH: f64 = 0.22;
/// Feeding ripples and pellets repeat this many times per loop.
const FEEDING_CYCLES_PER_LOOP: f64 = 3.0;
/// Cruising and darting breathe this many times per loop, opening this far.
const BREATHS_PER_LOOP: f64 = 5.0;
const BREATH_OPENING: f64 = 0.7;
/// Resting and feeding gulp this many times per loop, opening fully.
const GULPS_PER_LOOP: f64 = 3.0;
const DEEP_REST_FRAME_COUNT: u32 = 120;
const DEEP_REST_DEPTH: f64 = 0.5;

/// The numbers behind one kind of lap swimming. Each loop holds whole numbers of
/// laps, strokes, waves, and sculls, so it closes seamlessly.
struct LapStyle {
    loop_frame_count: u32,
    lap_count: f64,
    stroke_frame_count: f64,
    /// Share of each stroke cycle spent beating; the rest is a glide.
    stroke_share: f64,
    /// Lap speed peaks this many frames after the stroke.
    speed_lag_frames: f64,
    glide_speed: f64,
    stroke_speed_gain: f64,
    wave_frame_count: f64,
    glide_wave_strength: f64,
    stroke_wave_gain: f64,
    glide_tail_spread_radians: f64,
    stroke_tail_spread_radians: f64,
    side_fin_strokes: SideFinStrokes,
    /// On the clockwise lap the right fin is inside the turn: it opens this much more
    /// and the left fin this much less.
    inside_flare_radians: f64,
    scull_swing_radians: f64,
    scull_frame_count: f64,
    scull_phase_radians: f64,
}

/// How both side fins move with each stroke.
enum SideFinStrokes {
    /// Close during the stroke and open to brake through the glide.
    Brake {
        stroke_opening_radians: f64,
        brake_opening_radians: f64,
    },
    /// Tuck against the body during the burst, then spread and relax through the
    /// glide; both movements start and end at rest, so nothing snaps.
    TuckAndSpread {
        tuck_radians: f64,
        spread_radians: f64,
    },
}

/// One slow lap in eight seconds: two strokes, fifteen waves, and six sculls.
const CRUISE_STYLE: LapStyle = LapStyle {
    loop_frame_count: 120,
    lap_count: 1.0,
    stroke_frame_count: 60.0,
    stroke_share: 0.55,
    speed_lag_frames: 6.0,
    glide_speed: 0.75,
    stroke_speed_gain: 0.5,
    wave_frame_count: 8.0,
    glide_wave_strength: 0.3,
    stroke_wave_gain: 0.9,
    glide_tail_spread_radians: 0.05,
    stroke_tail_spread_radians: 0.12,
    side_fin_strokes: SideFinStrokes::Brake {
        stroke_opening_radians: 0.08,
        brake_opening_radians: 0.3,
    },
    inside_flare_radians: 0.12,
    scull_swing_radians: 0.1,
    scull_frame_count: 20.0,
    scull_phase_radians: 1.1,
};

/// Two fast laps in eight seconds: four bursts, twenty waves, and eight sculls.
const DART_STYLE: LapStyle = LapStyle {
    loop_frame_count: 120,
    lap_count: 2.0,
    stroke_frame_count: 30.0,
    stroke_share: 0.35,
    speed_lag_frames: 3.0,
    glide_speed: 0.35,
    stroke_speed_gain: 2.2,
    wave_frame_count: 6.0,
    glide_wave_strength: 0.25,
    stroke_wave_gain: 1.4,
    glide_tail_spread_radians: 0.04,
    stroke_tail_spread_radians: 0.16,
    side_fin_strokes: SideFinStrokes::TuckAndSpread {
        tuck_radians: 0.12,
        spread_radians: 0.4,
    },
    inside_flare_radians: 0.1,
    scull_swing_radians: 0.08,
    scull_frame_count: 15.0,
    scull_phase_radians: 0.0,
};

/// Where the koi swims in a frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Course {
    /// Clockwise around the frame center. The head's angle is measured clockwise on
    /// screen from the right; loops start with the head at the top.
    Lap {
        /// Where the head is on the lap.
        angle_radians: f64,
    },
    /// In place, in the still pose turned by the heading offset and moved by the drift.
    Hover {
        /// How far the still heading is turned.
        heading_offset_radians: f64,
        /// How far the koi is moved from the hover center, in native pixels.
        drift_pixels: [f64; 2],
    },
}

/// How the koi's body and fins look in one frame. Paired values list the left fin,
/// then the right, as the koi sees them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwimPose {
    /// Where the koi is in the frame.
    pub course: Course,
    /// How far under the water the koi is, from zero at the surface to one.
    pub depth: f64,
    /// Where the body wave is in its cycle.
    pub wave_phase_radians: f64,
    /// Multiplies the still pose's wave amplitude.
    pub wave_strength: f64,
    /// Bend along each tail lobe; the lobes trail the wave slightly out of step.
    pub tail_lobe_bends_radians: [f64; 2],
    /// How far each tail lobe turns outward, opening the fan.
    pub tail_spread_radians: f64,
    /// How far each side fin turns outward from its still direction; negative tucks
    /// it toward the body.
    pub side_fin_openings_radians: [f64; 2],
    /// Bend along each side fin, so its tip trails its root.
    pub side_fin_bends_radians: [f64; 2],
    /// How far each pelvic fin turns outward.
    pub pelvic_fin_opening_radians: f64,
    /// The whiskers swing together, trailing the head.
    pub whisker_sway_radians: f64,
    /// Where surface feeding's ripples and pellets are in their cycle, from zero up
    /// to one; none in every other behavior.
    pub feeding_phase: Option<f64>,
    /// How far the mouth is open, from zero closed to one wide open.
    pub mouth_opening: f64,
}

/// The still image: hovering at the surface without turning or drifting, with the
/// gentle wave and resting fins.
pub(crate) const STILL_POSE: SwimPose = SwimPose {
    course: Course::Hover {
        heading_offset_radians: 0.0,
        drift_pixels: [0.0; 2],
    },
    depth: 0.0,
    wave_phase_radians: 0.8,
    wave_strength: 1.0,
    tail_lobe_bends_radians: [0.0; 2],
    tail_spread_radians: 0.0,
    side_fin_openings_radians: [0.0; 2],
    side_fin_bends_radians: [0.0; 2],
    pelvic_fin_opening_radians: 0.0,
    whisker_sway_radians: 0.0,
    feeding_phase: None,
    mouth_opening: 0.0,
};

/// Frames in a behavior's loop at FRAMES_PER_SECOND.
pub fn frame_count(behavior: Behavior) -> u32 {
    match behavior {
        Behavior::Cruise => CRUISE_STYLE.loop_frame_count,
        Behavior::Dart => DART_STYLE.loop_frame_count,
        Behavior::Rest | Behavior::SurfaceFeeding => REST_FRAME_COUNT,
        Behavior::DeepRest => DEEP_REST_FRAME_COUNT,
    }
}

/// The pose for a frame of a behavior's loop; frame numbers wrap around the loop, so
/// frame `frame_count(behavior)` is frame zero again.
///
/// Cruising and darting swim clockwise laps driven by strokes, each a beat and a
/// glide. The koi speeds up a little after each stroke and coasts through the glide;
/// lap speeds are rescaled so the loop covers exactly its laps. Cruising side fins
/// brake open on the glide; darting side fins tuck through each burst and spread
/// through the glide. Resting hovers a little deeper with a slow beat and glide and
/// sculling fins; surface feeding moves the same way at the surface, with ripples
/// and pellets; deep rest hovers lower still, barely moving, with one fin flick per
/// loop. In every behavior the tail lobes bend behind the wave, the side fins' tips
/// trail their roots, the pelvic fins follow at half the opening, and the whiskers
/// sway. The mouth breathes while the koi laps, gulps while it rests or feeds, and
/// takes one slow gulp with deep rest's fin flick.
pub fn calculate_swim_pose(behavior: Behavior, frame_index: u32) -> SwimPose {
    calculate_swim_pose_with_energy(behavior, ActivityEnergy::Steady, frame_index)
}

/// As `calculate_swim_pose`, with active swim tempo scaled to recent contribution volume.
/// Resting and feeding motions keep their slower behavior-specific movement.
pub fn calculate_swim_pose_with_energy(
    behavior: Behavior,
    activity_energy: ActivityEnergy,
    frame_index: u32,
) -> SwimPose {
    let loop_frame_count = frame_count(behavior);
    let loop_frame = frame_index % loop_frame_count;
    let drive = calculate_drive(behavior, activity_energy, loop_frame);
    let trailing_drive = calculate_drive(
        behavior,
        activity_energy,
        (loop_frame + loop_frame_count - SIDE_FIN_TRAIL_FRAMES) % loop_frame_count,
    );
    let tail_bend = TAIL_BEND_PER_WAVE_STRENGTH
        * drive.wave_strength
        * (drive.wave_phase_radians - TAIL_BEND_LAG_RADIANS).sin();
    let tail_split = TAIL_SPLIT_RADIANS * (drive.wave_phase_radians - TAIL_SPLIT_LAG_RADIANS).sin();
    let side_fin_openings = drive.calculate_side_fin_openings();
    let trailing_openings = trailing_drive.calculate_side_fin_openings();
    SwimPose {
        course: drive.course,
        depth: drive.depth,
        wave_phase_radians: drive.wave_phase_radians,
        wave_strength: drive.wave_strength,
        tail_lobe_bends_radians: [tail_bend - tail_split, tail_bend + tail_split],
        tail_spread_radians: drive.tail_spread_radians,
        side_fin_openings_radians: side_fin_openings,
        side_fin_bends_radians: [
            trailing_openings[0] - side_fin_openings[0],
            trailing_openings[1] - side_fin_openings[1],
        ],
        pelvic_fin_opening_radians: PELVIC_SHARE_OF_SIDE_FIN_OPENING * drive.stroke_opening_radians,
        whisker_sway_radians: WHISKER_SWAY_RADIANS
            * (drive.wave_phase_radians - WHISKER_SWAY_LAG_RADIANS).sin(),
        feeding_phase: drive.feeding_phase,
        mouth_opening: drive.mouth_opening,
    }
}

/// What a behavior decides for one frame; the tail, fin trail, pelvic fins, and
/// whiskers follow from it the same way for every behavior.
struct Drive {
    course: Course,
    depth: f64,
    wave_phase_radians: f64,
    wave_strength: f64,
    tail_spread_radians: f64,
    /// The outward turn both side fins share.
    stroke_opening_radians: f64,
    /// Opens the right fin and tucks the left by the same amount: a flare on the
    /// inside of the turn, a scull, or a flick.
    right_fin_offset_radians: f64,
    feeding_phase: Option<f64>,
    mouth_opening: f64,
}

impl Drive {
    fn calculate_side_fin_openings(&self) -> [f64; 2] {
        [
            self.stroke_opening_radians - self.right_fin_offset_radians,
            self.stroke_opening_radians + self.right_fin_offset_radians,
        ]
    }
}

fn calculate_drive(behavior: Behavior, activity_energy: ActivityEnergy, loop_frame: u32) -> Drive {
    match behavior {
        Behavior::Cruise => CRUISE_STYLE.calculate_drive(loop_frame, activity_energy),
        Behavior::Dart => DART_STYLE.calculate_drive(loop_frame, activity_energy),
        Behavior::Rest => calculate_rest_drive(loop_frame, REST_DEPTH),
        Behavior::SurfaceFeeding => Drive {
            feeding_phase: Some(
                (FEEDING_CYCLES_PER_LOOP * f64::from(loop_frame) / f64::from(REST_FRAME_COUNT))
                    .fract(),
            ),
            ..calculate_rest_drive(loop_frame, 0.0)
        },
        Behavior::DeepRest => calculate_deep_rest_drive(loop_frame),
    }
}

/// Hovers in place at the given depth, swinging and drifting slowly, with two slow
/// beats and glides and gently sculling side fins.
fn calculate_rest_drive(loop_frame: u32, depth: f64) -> Drive {
    const STROKE_FRAME_COUNT: f64 = 45.0;
    const STROKE_SHARE: f64 = 0.4;
    const SWING_RADIANS: f64 = 0.1;
    const DRIFT_PIXELS: [f64; 2] = [3.0, 2.0];
    const WAVE_FRAME_COUNT: f64 = 15.0;
    const GLIDE_WAVE_STRENGTH: f64 = 0.15;
    const STROKE_WAVE_GAIN: f64 = 0.5;
    const TAIL_SPREAD_RADIANS: f64 = 0.04;
    const SIDE_FIN_OPENING_RADIANS: f64 = 0.25;
    const SIDE_FIN_BREATH_RADIANS: f64 = 0.08;
    const SCULL_SWING_RADIANS: f64 = 0.12;
    const SCULL_FRAME_COUNT: f64 = 18.0;
    let frame = f64::from(loop_frame);
    let loop_turn = TAU * frame / f64::from(REST_FRAME_COUNT);
    let stroke = calculate_beat(frame, STROKE_FRAME_COUNT, STROKE_SHARE);
    Drive {
        course: Course::Hover {
            heading_offset_radians: SWING_RADIANS * loop_turn.sin(),
            drift_pixels: [
                -DRIFT_PIXELS[0] * loop_turn.sin(),
                -DRIFT_PIXELS[1] * (2.0 * loop_turn + 1.0).sin(),
            ],
        },
        depth,
        wave_phase_radians: STILL_POSE.wave_phase_radians + frame * TAU / WAVE_FRAME_COUNT,
        wave_strength: GLIDE_WAVE_STRENGTH + STROKE_WAVE_GAIN * stroke,
        tail_spread_radians: TAIL_SPREAD_RADIANS,
        stroke_opening_radians: SIDE_FIN_OPENING_RADIANS
            + SIDE_FIN_BREATH_RADIANS * (3.0 * loop_turn).sin(),
        right_fin_offset_radians: SCULL_SWING_RADIANS * (TAU * frame / SCULL_FRAME_COUNT).sin(),
        feeding_phase: None,
        mouth_opening: (GULPS_PER_LOOP * loop_turn).sin().max(0.0),
    }
}

/// Hovers lower still and barely moves: a slow swing and drift, a breathing wave, and
/// one flick per loop in which the right fin opens as the left tucks.
fn calculate_deep_rest_drive(loop_frame: u32) -> Drive {
    const SWING_RADIANS: f64 = 0.05;
    const DRIFT_PIXELS: [f64; 2] = [1.5, 1.0];
    const SINK_PIXELS: f64 = 4.0;
    const WAVE_FRAME_COUNT: f64 = 30.0;
    const WAVE_STRENGTH: f64 = 0.12;
    const WAVE_BREATH: f64 = 0.1;
    const TAIL_SPREAD_RADIANS: f64 = 0.02;
    const SIDE_FIN_OPENING_RADIANS: f64 = 0.2;
    const FLICK_RADIANS: f64 = 0.35;
    const FLICK_START_FRAME: f64 = 70.0;
    const FLICK_FRAME_COUNT: f64 = 8.0;
    // One slow gulp that begins before the flick and ends after it.
    const GULP_START_FRAME: f64 = 66.0;
    const GULP_FRAME_COUNT: f64 = 16.0;
    let frame = f64::from(loop_frame);
    let loop_turn = TAU * frame / f64::from(DEEP_REST_FRAME_COUNT);
    let flick_fraction = (frame - FLICK_START_FRAME) / FLICK_FRAME_COUNT;
    let flick = if (0.0..=1.0).contains(&flick_fraction) {
        (PI * flick_fraction).sin()
    } else {
        0.0
    };
    let gulp_fraction = (frame - GULP_START_FRAME) / GULP_FRAME_COUNT;
    let mouth_opening = if (0.0..=1.0).contains(&gulp_fraction) {
        (PI * gulp_fraction).sin()
    } else {
        0.0
    };
    Drive {
        course: Course::Hover {
            heading_offset_radians: SWING_RADIANS * loop_turn.sin(),
            drift_pixels: [
                -DRIFT_PIXELS[0] * loop_turn.sin(),
                SINK_PIXELS - DRIFT_PIXELS[1] * (2.0 * loop_turn).sin(),
            ],
        },
        depth: DEEP_REST_DEPTH,
        wave_phase_radians: STILL_POSE.wave_phase_radians + frame * TAU / WAVE_FRAME_COUNT,
        wave_strength: WAVE_STRENGTH + WAVE_BREATH * (2.0 * loop_turn).sin(),
        tail_spread_radians: TAIL_SPREAD_RADIANS,
        stroke_opening_radians: SIDE_FIN_OPENING_RADIANS,
        right_fin_offset_radians: FLICK_RADIANS * flick,
        feeding_phase: None,
        mouth_opening,
    }
}

/// Beat strength at a frame: a smooth beat over the first part of each cycle, then
/// nothing through the glide.
fn calculate_beat(frame: f64, cycle_frame_count: f64, beat_share: f64) -> f64 {
    let cycle_fraction = frame.rem_euclid(cycle_frame_count) / cycle_frame_count;
    if cycle_fraction < beat_share {
        (PI * cycle_fraction / beat_share).sin()
    } else {
        0.0
    }
}

impl LapStyle {
    /// Where a frame falls in its stroke cycle, from zero up to one.
    fn calculate_cycle_fraction(&self, frame: f64, stroke_frame_count: f64) -> f64 {
        frame.rem_euclid(stroke_frame_count) / stroke_frame_count
    }

    fn calculate_stroke(&self, frame: f64, stroke_frame_count: f64) -> f64 {
        calculate_beat(frame, stroke_frame_count, self.stroke_share)
    }

    fn calculate_drive(&self, loop_frame: u32, activity_energy: ActivityEnergy) -> Drive {
        let cycle_multiplier = activity_energy.cycle_multiplier();
        let stroke_frame_count = scaled_cycle_frame_count(
            self.loop_frame_count,
            self.stroke_frame_count,
            cycle_multiplier,
        );
        let wave_frame_count = scaled_cycle_frame_count(
            self.loop_frame_count,
            self.wave_frame_count,
            cycle_multiplier,
        );
        let scull_frame_count = scaled_cycle_frame_count(
            self.loop_frame_count,
            self.scull_frame_count,
            cycle_multiplier,
        );
        let lap_speeds: Vec<f64> = (0..self.loop_frame_count)
            .map(|speed_frame| {
                self.glide_speed
                    + self.stroke_speed_gain
                        * self.calculate_stroke(
                            f64::from(speed_frame) - self.speed_lag_frames,
                            stroke_frame_count,
                        )
            })
            .collect();
        let total_speed: f64 = lap_speeds.iter().sum();
        let covered_speed: f64 = lap_speeds[..loop_frame as usize].iter().sum();
        let frame = f64::from(loop_frame);
        let stroke = self.calculate_stroke(frame, stroke_frame_count);
        let wave_phase_radians = STILL_POSE.wave_phase_radians + frame * TAU / wave_frame_count;
        let scull = self.scull_swing_radians
            * (TAU * frame / scull_frame_count + self.scull_phase_radians).sin();
        let breath = (TAU * BREATHS_PER_LOOP * frame / f64::from(self.loop_frame_count))
            .sin()
            .max(0.0);
        Drive {
            course: Course::Lap {
                angle_radians: -PI / 2.0 + TAU * self.lap_count * covered_speed / total_speed,
            },
            depth: 0.0,
            wave_phase_radians,
            wave_strength: self.glide_wave_strength + self.stroke_wave_gain * stroke,
            tail_spread_radians: self.glide_tail_spread_radians
                + self.stroke_tail_spread_radians * stroke * wave_phase_radians.sin().abs(),
            stroke_opening_radians: self.calculate_stroke_opening(frame, stroke_frame_count),
            right_fin_offset_radians: self.inside_flare_radians + scull,
            feeding_phase: None,
            mouth_opening: BREATH_OPENING * breath * breath,
        }
    }

    /// The outward turn both side fins share with the stroke.
    fn calculate_stroke_opening(&self, frame: f64, stroke_frame_count: f64) -> f64 {
        match self.side_fin_strokes {
            SideFinStrokes::Brake {
                stroke_opening_radians,
                brake_opening_radians,
            } => {
                stroke_opening_radians
                    + brake_opening_radians
                        * (1.0 - self.calculate_stroke(frame, stroke_frame_count))
            }
            SideFinStrokes::TuckAndSpread {
                tuck_radians,
                spread_radians,
            } => {
                let cycle_fraction = self.calculate_cycle_fraction(frame, stroke_frame_count);
                if cycle_fraction < self.stroke_share {
                    -tuck_radians * self.calculate_stroke(frame, stroke_frame_count)
                } else {
                    let glide_fraction =
                        (cycle_fraction - self.stroke_share) / (1.0 - self.stroke_share);
                    spread_radians * (PI * glide_fraction).sin()
                }
            }
        }
    }
}

/// Scale whole cycles per loop, so the motion meets itself cleanly at the GIF seam.
fn scaled_cycle_frame_count(loop_frame_count: u32, base_frame_count: f64, multiplier: f64) -> f64 {
    let base_cycles = f64::from(loop_frame_count) / base_frame_count;
    let scaled_cycles = (base_cycles * multiplier).round().max(1.0);
    f64::from(loop_frame_count) / scaled_cycles
}
