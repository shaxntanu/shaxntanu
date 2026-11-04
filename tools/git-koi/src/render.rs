//! A curved koi colored from its languages on a chosen body, with Classic shading and
//! transparent surroundings.
//!
//! The body and fins follow sampled curves, with shading that follows the body surface
//! and fin rays that follow each fin's direction. The head widens from a round nose
//! along a gentle slant, and short pixel whiskers angle forward from the snout.
//! Contribution totals set the size through [`crate::koi::calculate_growth_fraction`],
//! and growth slows as the fish matures. Young koi have a rounder head, larger eyes, a
//! slimmer body, and short rounded fins, and grow into the adult shape by
//! [`crate::koi::YOUNG_SHAPE_CONTRIBUTIONS`]. After [`crate::koi::ADULT_CONTRIBUTIONS`]
//! the body keeps lengthening, broadens slightly, and its fins and tail lengthen; the
//! maximum form keeps its anatomy at higher totals. Quiet days alone do not change the
//! base anatomy. The pose and framing account for the body and fins.
//!
//! Every growth level shares one wide native canvas of [`FRAME_WIDTH_PIXELS`] by
//! [`FRAME_HEIGHT_PIXELS`], which the encoders in [`crate::output`] scale up exactly.
//! Anatomical proportions live here; pigments, bodies, and palettes in [`crate::coat`];
//! streak mark strengths in [`crate::markings`]; poses in [`crate::motion`].

use std::f64::consts::{PI, TAU};

use chrono::{FixedOffset, Timelike};

use crate::behavior::{ActivityEnergy, Behavior};
use crate::coat::{KoiBody, KoiCoat, WHITE_RAMP};
use crate::history::{LanguageColor, PublicActivityEvent, PublicActivityKind};
use crate::koi::{
    ADULT_CONTRIBUTIONS, KoiDevelopment, YOUNG_SHAPE_CONTRIBUTIONS, calculate_growth_fraction,
};
use crate::motion::{
    Course, STILL_POSE, SwimPose, calculate_swim_pose, calculate_swim_pose_with_energy, frame_count,
};

/// Width of the native canvas every koi is drawn on, at every growth level.
pub const FRAME_WIDTH_PIXELS: u32 = 320;
/// Height of the native canvas every koi is drawn on, at every growth level.
pub const FRAME_HEIGHT_PIXELS: u32 = 288;
const SCENE_DESIGN_WIDTH_PIXELS: i32 = 256;
const SCENE_DESIGN_HEIGHT_PIXELS: i32 = 256;
const SCENE_X_OFFSET: i32 = (FRAME_WIDTH_PIXELS as i32 - SCENE_DESIGN_WIDTH_PIXELS) / 2;
const SCENE_Y_OFFSET: i32 = (FRAME_HEIGHT_PIXELS as i32 - SCENE_DESIGN_HEIGHT_PIXELS) / 2;
const MAX_LAP_RADIUS_PIXELS: f64 = 88.0;
const LAP_CLEARANCE_FACTOR: f64 = 1.25;
const POND_SHORE_DEPTH_PIXELS: i32 = 28;
const POND_HALF_HEIGHT_PIXELS: i32 =
    (MAX_LAP_RADIUS_PIXELS * LAP_CLEARANCE_FACTOR) as i32 + POND_SHORE_DEPTH_PIXELS;

/// A lighting palette selected from the latest public activity's local hour.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PondLighting {
    /// Cool daylight caustics on the water.
    Day,
    /// A low warm sun and reflected dusk light.
    Dusk,
    /// Moonlight and stars over the pond.
    Night,
}

/// Choose pond lighting from the newest event after applying the requested UTC offset.
/// Accounts with no recent public events use daylight as a stable default.
pub fn select_pond_lighting(
    public_activity: &[PublicActivityEvent],
    utc_offset: FixedOffset,
) -> PondLighting {
    let Some(latest_event) = public_activity.iter().max_by_key(|event| event.happened_at) else {
        return PondLighting::Day;
    };
    match latest_event.happened_at.with_timezone(&utc_offset).hour() {
        7..=16 => PondLighting::Day,
        17..=19 => PondLighting::Dusk,
        _ => PondLighting::Night,
    }
}

/// A renderer-owned, fixed-size frame with row-major RGBA pixels.
#[derive(Debug, Eq, PartialEq)]
pub struct KoiFrame {
    rgba_pixels: Vec<[u8; 4]>,
}

impl KoiFrame {
    /// The pixels as bytes, row by row from the top left, four channels per pixel: red,
    /// green, blue, alpha. The length is the canvas area times four.
    pub fn as_rgba_bytes(&self) -> &[u8] {
        self.rgba_pixels.as_flattened()
    }

    /// Build a frame from chosen pixels, for encoder tests that need colors no koi has.
    #[cfg(test)]
    pub(crate) fn from_rgba_pixels(rgba_pixels: Vec<[u8; 4]>) -> Self {
        Self { rgba_pixels }
    }
}

/// Render the koi in a centered, fixed swimming pose, colored from its languages on
/// the caller's chosen body. Contribution totals set its size and how far its colors
/// have come in. The account seed places patches in body coordinates, so growth does
/// not reroll the pattern. Streak marks follow the development's streaks as of its
/// observation date. Identical inputs produce identical pixels without I/O, clock
/// reads, or input mutation.
pub fn render_koi(koi_development: &KoiDevelopment, koi_body: KoiBody) -> KoiFrame {
    let koi_anatomy = calculate_anatomy(koi_development.total_contributions);
    render_pose(
        koi_development,
        koi_body,
        &koi_anatomy,
        &STILL_POSE,
        Framing::CenterOnKoi,
    )
}

/// Render one frame of a behavior's seamless loop, colored like `render_koi`, in the
/// pose `motion::calculate_swim_pose` gives; frame numbers wrap around the loop of
/// `motion::frame_count(behavior)` frames. Laps circle the frame center, and longer
/// koi swim a tighter lap so every size fits. A hovering loop is framed once, centered
/// on the koi's sweep, and its drift shrinks only as far as the largest koi need to
/// stay inside. The view never follows the fins, so it never shakes. Under water the koi
/// turns toward water blue and partly drops out, more with depth. Identical inputs
/// produce identical pixels without I/O, clock reads, or input mutation.
pub fn render_swim_frame(
    koi_development: &KoiDevelopment,
    koi_body: KoiBody,
    behavior: Behavior,
    frame_index: u32,
) -> KoiFrame {
    render_swim_frame_with_energy(
        koi_development,
        koi_body,
        behavior,
        ActivityEnergy::Steady,
        frame_index,
    )
}

/// Render one swim frame with tail and fin tempo scaled to recent activity.
pub fn render_swim_frame_with_energy(
    koi_development: &KoiDevelopment,
    koi_body: KoiBody,
    behavior: Behavior,
    activity_energy: ActivityEnergy,
    frame_index: u32,
) -> KoiFrame {
    render_swim_frame_with_activity(
        koi_development,
        koi_body,
        behavior,
        activity_energy,
        &[],
        frame_index,
    )
}

/// Render one swim frame with activity-driven tempo and event choreography.
pub fn render_swim_frame_with_activity(
    koi_development: &KoiDevelopment,
    koi_body: KoiBody,
    behavior: Behavior,
    activity_energy: ActivityEnergy,
    public_activity: &[PublicActivityEvent],
    frame_index: u32,
) -> KoiFrame {
    render_swim_frame_with_lighting(
        koi_development,
        koi_body,
        behavior,
        activity_energy,
        public_activity,
        PondLighting::Day,
        frame_index,
    )
}

/// Render a swim frame with activity choreography and a chosen pond light.
pub fn render_swim_frame_with_lighting(
    koi_development: &KoiDevelopment,
    koi_body: KoiBody,
    behavior: Behavior,
    activity_energy: ActivityEnergy,
    public_activity: &[PublicActivityEvent],
    pond_lighting: PondLighting,
    frame_index: u32,
) -> KoiFrame {
    let koi_anatomy = calculate_anatomy(koi_development.total_contributions);
    let swim_pose = calculate_swim_pose_with_energy(behavior, activity_energy, frame_index);
    let pose_center = match swim_pose.course {
        Course::Lap { .. } => [0.0, 0.0],
        Course::Hover { drift_pixels, .. } => {
            let hover_fit = fit_hover_loop(behavior, &koi_anatomy);
            [0, 1].map(|axis_index| {
                hover_fit.sweep_center[axis_index]
                    - hover_fit.drift_scales[axis_index] * drift_pixels[axis_index]
            })
        }
    };
    let mut koi_frame = render_pose(
        koi_development,
        koi_body,
        &koi_anatomy,
        &swim_pose,
        Framing::At(pose_center),
    );
    paint_pond_lighting(&mut koi_frame.rgba_pixels, pond_lighting, frame_index);
    paint_event_choreography(&mut koi_frame.rgba_pixels, public_activity, frame_index);
    paint_pond_surface(&mut koi_frame.rgba_pixels, pond_lighting, frame_index);
    koi_frame
}

fn paint_pond_surface(rgba_pixels: &mut [[u8; 4]], pond_lighting: PondLighting, frame_index: u32) {
    let (bank, shelf, water, deep_water, glint, lily, lily_vein) = match pond_lighting {
        PondLighting::Day => (
            [37, 82, 62, 255],
            [33, 116, 123, 255],
            [13, 61, 80, 255],
            [8, 43, 62, 255],
            [104, 205, 196, 255],
            [43, 132, 87, 255],
            [150, 210, 147, 255],
        ),
        PondLighting::Dusk => (
            [89, 59, 72, 255],
            [153, 85, 112, 255],
            [78, 50, 88, 255],
            [43, 33, 63, 255],
            [245, 164, 145, 255],
            [137, 104, 89, 255],
            [227, 178, 131, 255],
        ),
        PondLighting::Night => (
            [26, 49, 78, 255],
            [40, 85, 132, 255],
            [16, 43, 79, 255],
            [8, 28, 54, 255],
            [130, 179, 226, 255],
            [47, 83, 104, 255],
            [117, 165, 180, 255],
        ),
    };

    let bob = ((frame_index / 15) % 2) as i32;
    paint_lily_pad(
        rgba_pixels,
        [53 + SCENE_X_OFFSET, 188 + SCENE_Y_OFFSET + bob],
        lily,
        lily_vein,
    );
    paint_lily_pad(
        rgba_pixels,
        [202 + SCENE_X_OFFSET, 192 + SCENE_Y_OFFSET - bob],
        lily,
        lily_vein,
    );

    let shimmer = ((frame_index / 10) % 3) as i32 - 1;
    for [x, y] in [
        [68, 82],
        [91, 79],
        [166, 76],
        [190, 83],
        [78, 203],
        [115, 215],
        [171, 203],
        [199, 210],
    ] {
        for offset in -4_i32..=4 {
            if offset.abs() % 3 != 0 {
                paint_pixel_if_clear(
                    rgba_pixels,
                    [x + SCENE_X_OFFSET + offset, y + SCENE_Y_OFFSET + shimmer],
                    glint,
                );
            }
        }
    }

    const CENTER: [i32; 2] = [128 + SCENE_X_OFFSET, 128 + SCENE_Y_OFFSET];
    const HALF_SIZE: [i32; 2] = [154, POND_HALF_HEIGHT_PIXELS];
    const CORNER_RADIUS: f64 = 30.0;
    for y in CENTER[1] - HALF_SIZE[1]..=CENTER[1] + HALF_SIZE[1] {
        let vertical_offset = (y - CENTER[1]).abs();
        let corner_y = f64::from(vertical_offset) - (f64::from(HALF_SIZE[1]) - CORNER_RADIUS);
        for x in CENTER[0] - HALF_SIZE[0]..=CENTER[0] + HALF_SIZE[0] {
            let horizontal_offset = (x - CENTER[0]).abs();
            let corner_x = f64::from(horizontal_offset) - (f64::from(HALF_SIZE[0]) - CORNER_RADIUS);
            let corner_distance = if corner_x > 0.0 && corner_y > 0.0 {
                corner_x.hypot(corner_y)
            } else {
                0.0
            };
            if corner_distance > CORNER_RADIUS {
                continue;
            }
            let edge_depth = if corner_x > 0.0 && corner_y > 0.0 {
                CORNER_RADIUS - corner_distance
            } else {
                f64::from((HALF_SIZE[0] - horizontal_offset).min(HALF_SIZE[1] - vertical_offset))
            };
            let color = if edge_depth < 5.0 {
                bank
            } else if edge_depth < 13.0 {
                shelf
            } else if edge_depth < f64::from(POND_SHORE_DEPTH_PIXELS) {
                water
            } else {
                deep_water
            };
            paint_pixel_if_clear(rgba_pixels, [x, y], color);
        }
    }
}

fn paint_lily_pad(rgba_pixels: &mut [[u8; 4]], center: [i32; 2], lily: [u8; 4], vein: [u8; 4]) {
    for delta_y in -3_i32..=3 {
        let half_width = match delta_y.abs() {
            0 => 8,
            1 => 7,
            2 => 5,
            _ => 2,
        };
        for delta_x in -half_width..=half_width {
            if delta_y == 0 && delta_x == half_width {
                continue;
            }
            let color = if delta_y == 0 && delta_x % 2 == 0 {
                vein
            } else {
                lily
            };
            paint_pixel_if_clear(
                rgba_pixels,
                [center[0] + delta_x, center[1] + delta_y],
                color,
            );
        }
    }
}

fn paint_pond_lighting(rgba_pixels: &mut [[u8; 4]], pond_lighting: PondLighting, frame_index: u32) {
    match pond_lighting {
        PondLighting::Day => {
            let shimmer = if frame_index / 10 % 2 == 0 { 0 } else { 1 };
            for center in [[57, 38], [112, 47], [187, 70], [53, 174], [204, 184]] {
                paint_light_sparkle(
                    rgba_pixels,
                    [
                        center[0] + SCENE_X_OFFSET + shimmer,
                        center[1] + SCENE_Y_OFFSET,
                    ],
                    [101, 222, 224, 255],
                );
            }
        }
        PondLighting::Dusk => {
            let sun_drop = ((frame_index / 12) % 3) as i32;
            let sun_center = [207 + SCENE_X_OFFSET, 177 + SCENE_Y_OFFSET + sun_drop];
            for delta_y in -4_i32..=4 {
                for delta_x in -4_i32..=4 {
                    if delta_x * delta_x + delta_y * delta_y <= 20
                        && (delta_y < 1 || delta_y % 2 == 0)
                    {
                        paint_pixel_if_clear(
                            rgba_pixels,
                            [sun_center[0] + delta_x, sun_center[1] + delta_y],
                            [255, 178, 112, 255],
                        );
                    }
                }
            }
            for offset in -7_i32..=7 {
                if offset.abs() % 3 != 1 {
                    paint_pixel_if_clear(
                        rgba_pixels,
                        [sun_center[0] + offset, 185 + SCENE_Y_OFFSET + sun_drop],
                        [237, 137, 130, 255],
                    );
                }
            }
            paint_light_sparkle(
                rgba_pixels,
                [57 + SCENE_X_OFFSET, 42 + SCENE_Y_OFFSET],
                [207, 148, 166, 255],
            );
        }
        PondLighting::Night => {
            let twinkle = if frame_index / 9 % 2 == 0 { 0 } else { 1 };
            for center in [
                [45, 43],
                [102, 31],
                [167, 48],
                [194, 94],
                [56, 164],
                [205, 163],
            ] {
                paint_light_sparkle(
                    rgba_pixels,
                    [
                        center[0] + SCENE_X_OFFSET,
                        center[1] + SCENE_Y_OFFSET + twinkle,
                    ],
                    [128, 172, 226, 255],
                );
            }
            let moon = [208 + SCENE_X_OFFSET, 39 + SCENE_Y_OFFSET];
            for delta_y in -4_i32..=4 {
                for delta_x in -4_i32..=4 {
                    if delta_x * delta_x + delta_y * delta_y <= 16 && (delta_x + delta_y) % 4 != 0 {
                        paint_pixel_if_clear(
                            rgba_pixels,
                            [moon[0] + delta_x, moon[1] + delta_y],
                            [192, 220, 249, 255],
                        );
                    }
                }
            }
        }
    }
}

fn paint_light_sparkle(rgba_pixels: &mut [[u8; 4]], center: [i32; 2], color: [u8; 4]) {
    for point in [[0, 0], [0, -1], [-1, 0], [1, 0], [0, 1]] {
        paint_pixel_if_clear(
            rgba_pixels,
            [center[0] + point[0], center[1] + point[1]],
            color,
        );
    }
}

fn paint_event_choreography(
    rgba_pixels: &mut [[u8; 4]],
    public_activity: &[PublicActivityEvent],
    frame_index: u32,
) {
    let count_kind = |kind| {
        public_activity
            .iter()
            .filter(|event| event.kind == kind)
            .count()
            .min(3)
    };
    if count_kind(PublicActivityKind::Issue) > 0 {
        paint_event_ripples(
            rgba_pixels,
            [26 + SCENE_X_OFFSET, 50 + SCENE_Y_OFFSET],
            frame_index,
            count_kind(PublicActivityKind::Issue),
        );
    }
    if count_kind(PublicActivityKind::PullRequest) > 0 {
        paint_pull_request_pad(rgba_pixels, frame_index);
    }
    if count_kind(PublicActivityKind::PullRequestReview) > 0 {
        paint_review_bubbles(
            rgba_pixels,
            [28 + SCENE_X_OFFSET, 210 + SCENE_Y_OFFSET],
            frame_index,
            count_kind(PublicActivityKind::PullRequestReview),
        );
    }
    paint_repository_trail(rgba_pixels, public_activity, frame_index);
}

fn paint_repository_trail(
    rgba_pixels: &mut [[u8; 4]],
    public_activity: &[PublicActivityEvent],
    frame_index: u32,
) {
    let mut repositories =
        std::collections::BTreeMap::<String, (usize, Option<LanguageColor>)>::new();
    for event in public_activity
        .iter()
        .filter(|event| event.kind == PublicActivityKind::Push)
    {
        let entry = repositories
            .entry(event.repository.clone())
            .or_insert((0, None));
        entry.0 += 1;
        if event.language_color.is_some() {
            entry.1 = event.language_color;
        }
    }
    let mut repositories: Vec<_> = repositories.into_iter().collect();
    repositories.sort_by(|left, right| right.1.0.cmp(&left.1.0).then_with(|| left.0.cmp(&right.0)));
    repositories.truncate(5);
    if repositories.is_empty() {
        return;
    }

    const REPOSITORY_MARKERS: [[i32; 2]; 5] = [
        [36 + SCENE_X_OFFSET, 196 + SCENE_Y_OFFSET],
        [74 + SCENE_X_OFFSET, 202 + SCENE_Y_OFFSET],
        [128 + SCENE_X_OFFSET, 169 + SCENE_Y_OFFSET],
        [182 + SCENE_X_OFFSET, 202 + SCENE_Y_OFFSET],
        [220 + SCENE_X_OFFSET, 196 + SCENE_Y_OFFSET],
    ];
    const TRAIL_CONTROLS: [[i32; 2]; 5] = [
        [95 + SCENE_X_OFFSET, 232 + SCENE_Y_OFFSET],
        [104 + SCENE_X_OFFSET, 190 + SCENE_Y_OFFSET],
        [139 + SCENE_X_OFFSET, 180 + SCENE_Y_OFFSET],
        [152 + SCENE_X_OFFSET, 190 + SCENE_Y_OFFSET],
        [161 + SCENE_X_OFFSET, 232 + SCENE_Y_OFFSET],
    ];
    const TRAIL_ORIGIN: [i32; 2] = [128 + SCENE_X_OFFSET, 216 + SCENE_Y_OFFSET];
    for (repository_index, (_, (_, language_color))) in repositories.iter().enumerate() {
        let marker = REPOSITORY_MARKERS[repository_index];
        let color = language_color.map_or([94, 198, 225, 255], language_marker_color);
        paint_repository_marker(rgba_pixels, marker, color);
        let progress = f64::from((frame_index + repository_index as u32 * 6) % 30) / 30.0;
        paint_trail_curve(
            rgba_pixels,
            TRAIL_ORIGIN,
            TRAIL_CONTROLS[repository_index],
            marker,
            progress,
            language_trail_color(*language_color),
            color,
        );
    }
    paint_github_hub(rgba_pixels, TRAIL_ORIGIN);
}

fn language_marker_color(language_color: LanguageColor) -> [u8; 4] {
    let [red, green, blue] = language_color.0;
    let lift = |channel: u8| channel + (255 - channel) / 3;
    [lift(red), lift(green), lift(blue), 255]
}

fn language_trail_color(language_color: Option<LanguageColor>) -> [u8; 4] {
    let LanguageColor([red, green, blue]) = language_color.unwrap_or(LanguageColor([70, 185, 205]));
    let blend =
        |language: u8, water: u8| (u16::from(language) * 3 / 5 + u16::from(water) * 2 / 5) as u8;
    [blend(red, 70), blend(green, 185), blend(blue, 205), 255]
}

fn paint_trail_curve(
    rgba_pixels: &mut [[u8; 4]],
    start: [i32; 2],
    control: [i32; 2],
    end: [i32; 2],
    progress: f64,
    trail_color: [u8; 4],
    particle_color: [u8; 4],
) {
    let point_at = |progress: f64| {
        let inverse = 1.0 - progress;
        [0, 1].map(|axis_index| {
            (inverse * inverse * f64::from(start[axis_index])
                + 2.0 * inverse * progress * f64::from(control[axis_index])
                + progress * progress * f64::from(end[axis_index]))
            .round() as i32
        })
    };
    let particle = point_at(progress);
    paint_pixel_if_clear(rgba_pixels, particle, [240, 250, 255, 255]);
    paint_pixel_if_clear(rgba_pixels, [particle[0] - 1, particle[1]], particle_color);
    let mut previous = start;
    for step in 1..=24 {
        let point = point_at(f64::from(step) / 24.0);
        paint_trail_line(rgba_pixels, previous, point, trail_color);
        previous = point;
    }
}

fn paint_repository_marker(rgba_pixels: &mut [[u8; 4]], center: [i32; 2], color: [u8; 4]) {
    for delta_y in -3_i32..=3 {
        for delta_x in -3_i32..=3 {
            let edge = delta_x.abs() == 3 || delta_y.abs() == 3;
            let marker_color = if edge || (delta_x == 0 && delta_y.abs() <= 1) {
                [20, 66, 83, 255]
            } else if delta_x == -2 && delta_y == -2 {
                [240, 250, 255, 255]
            } else {
                color
            };
            paint_pixel_if_clear(
                rgba_pixels,
                [center[0] + delta_x, center[1] + delta_y],
                marker_color,
            );
        }
    }
}

fn paint_github_hub(rgba_pixels: &mut [[u8; 4]], center: [i32; 2]) {
    for delta_y in -4_i32..=4 {
        let half_width: i32 = if delta_y.abs() == 4 { 2 } else { 4 };
        for delta_x in -half_width..=half_width {
            let color = if delta_x.abs() == half_width || delta_y.abs() == 4 {
                [93, 205, 211, 255]
            } else {
                [12, 49, 68, 255]
            };
            paint_pixel_if_clear(
                rgba_pixels,
                [center[0] + delta_x, center[1] + delta_y],
                color,
            );
        }
    }
    const GITHUB_GLYPHS: [[u8; 5]; 6] = [
        [3, 4, 5, 5, 3],
        [7, 2, 2, 2, 7],
        [7, 2, 2, 2, 2],
        [5, 5, 7, 5, 5],
        [5, 5, 5, 5, 7],
        [6, 5, 6, 5, 6],
    ];
    let label_start = center[0] - 23;
    for (character_index, glyph) in GITHUB_GLYPHS.iter().enumerate() {
        for (row, bits) in glyph.iter().enumerate() {
            for column in 0..3 {
                if bits & (1 << (2 - column)) == 0 {
                    continue;
                }
                for scale_y in 0..2 {
                    for scale_x in 0..2 {
                        paint_pixel_if_clear(
                            rgba_pixels,
                            [
                                label_start + character_index as i32 * 8 + column * 2 + scale_x,
                                center[1] + 6 + row as i32 * 2 + scale_y,
                            ],
                            [177, 227, 224, 255],
                        );
                    }
                }
            }
        }
    }
}

fn paint_trail_line(rgba_pixels: &mut [[u8; 4]], start: [i32; 2], end: [i32; 2], color: [u8; 4]) {
    let [mut x, mut y] = start;
    let dx = (end[0] - start[0]).abs();
    let step_x = if start[0] < end[0] { 1 } else { -1 };
    let dy = -(end[1] - start[1]).abs();
    let step_y = if start[1] < end[1] { 1 } else { -1 };
    let mut error = dx + dy;
    loop {
        paint_pixel_if_clear(rgba_pixels, [x, y], color);
        if [x, y] == end {
            break;
        }
        let doubled_error = error * 2;
        if doubled_error >= dy {
            error += dy;
            x += step_x;
        }
        if doubled_error <= dx {
            error += dx;
            y += step_y;
        }
    }
}

fn paint_event_ripples(
    rgba_pixels: &mut [[u8; 4]],
    center: [i32; 2],
    frame_index: u32,
    ring_count: usize,
) {
    const ISSUE_COLOR: [u8; 4] = [95, 187, 255, 255];
    let pulse = (frame_index / 20) % 2;
    for ring_index in 0..ring_count {
        let radius = 2 + i32::try_from(ring_index).unwrap_or(0) + pulse as i32;
        for offset in -radius..=radius {
            paint_pixel_if_clear(
                rgba_pixels,
                [center[0] + offset, center[1] - radius],
                ISSUE_COLOR,
            );
            paint_pixel_if_clear(
                rgba_pixels,
                [center[0] + offset, center[1] + radius],
                ISSUE_COLOR,
            );
            paint_pixel_if_clear(
                rgba_pixels,
                [center[0] - radius, center[1] + offset],
                ISSUE_COLOR,
            );
            paint_pixel_if_clear(
                rgba_pixels,
                [center[0] + radius, center[1] + offset],
                ISSUE_COLOR,
            );
        }
    }
}

fn paint_pull_request_pad(rgba_pixels: &mut [[u8; 4]], frame_index: u32) {
    const PAD_COLOR: [u8; 4] = [94, 198, 149, 255];
    const VEIN_COLOR: [u8; 4] = [190, 238, 193, 255];
    let bob = ((frame_index / 18) % 2) as i32;
    let center = [225 + SCENE_X_OFFSET, 48 + SCENE_Y_OFFSET + bob];
    for delta_y in -2..=2 {
        for delta_x in -4..=4 {
            if delta_x * delta_x * 4 + delta_y * delta_y * 16 <= 64
                && !(delta_x == 4 && delta_y == 0)
            {
                paint_pixel_if_clear(
                    rgba_pixels,
                    [center[0] + delta_x, center[1] + delta_y],
                    PAD_COLOR,
                );
            }
        }
    }
    for offset in -2..=2 {
        paint_pixel_if_clear(rgba_pixels, [center[0] + offset, center[1]], VEIN_COLOR);
    }
}

fn paint_review_bubbles(
    rgba_pixels: &mut [[u8; 4]],
    origin: [i32; 2],
    frame_index: u32,
    bubble_count: usize,
) {
    const REVIEW_COLOR: [u8; 4] = [255, 211, 122, 255];
    let rise = ((frame_index / 15) % 4) as i32;
    for bubble_index in 0..bubble_count {
        let offset_x = i32::try_from(bubble_index).unwrap_or(0) * 5;
        let offset_y = i32::try_from(bubble_index % 2).unwrap_or(0) * 4;
        let center = [origin[0] + offset_x, origin[1] - rise - offset_y];
        for point in [[0, -1], [-1, 0], [1, 0], [0, 1]] {
            paint_pixel_if_clear(
                rgba_pixels,
                [center[0] + point[0], center[1] + point[1]],
                REVIEW_COLOR,
            );
        }
    }
}

fn paint_pixel_if_clear(rgba_pixels: &mut [[u8; 4]], position: [i32; 2], color: [u8; 4]) {
    let [x, y] = position;
    if x < 0 || y < 0 || x >= FRAME_WIDTH_PIXELS as i32 || y >= FRAME_HEIGHT_PIXELS as i32 {
        return;
    }
    let pixel_index = y as usize * FRAME_WIDTH_PIXELS as usize + x as usize;
    if rgba_pixels[pixel_index][3] == 0 {
        rgba_pixels[pixel_index] = color;
    }
}

/// Where the view is centered: on the koi itself for the still image, or at a point
/// fixed for the whole loop.
enum Framing {
    CenterOnKoi,
    At([f64; 2]),
}

/// One framing for a whole hovering loop.
struct HoverFit {
    sweep_center: [f64; 2],
    /// Shares of the pose's drift that keep every frame inside, from zero through one.
    drift_scales: [f64; 2],
}

/// Center a hovering loop on everything the koi covers across its frames, then shrink
/// the drift on each axis only as far as needed to keep every frame inside the frame.
fn fit_hover_loop(behavior: Behavior, koi_anatomy: &KoiAnatomy) -> HoverFit {
    // Room for the outline, a clear pixel at the frame edge, and rounding.
    const FIT_MARGIN_PIXELS: f64 = 2.5;
    let frame_bounds: Vec<([[f64; 2]; 2], [f64; 2])> = (0..frame_count(behavior))
        .map(|frame_index| {
            let swim_pose = calculate_swim_pose(behavior, frame_index);
            let body_spine = build_spine(koi_anatomy, &swim_pose);
            let fin_curves = build_fins(&body_spine, koi_anatomy, &swim_pose);
            let [mut min_corner, mut max_corner] = measure_curve_bounds(&body_spine, &fin_curves);
            // A whisker point covers the pixel to its right and below.
            for whisker_point in
                calculate_whisker_points(&body_spine, koi_anatomy, swim_pose.whisker_sway_radians)
            {
                for axis_index in 0..2 {
                    min_corner[axis_index] = min_corner[axis_index].min(whisker_point[axis_index]);
                    max_corner[axis_index] =
                        max_corner[axis_index].max(whisker_point[axis_index] + 1.0);
                }
            }
            let drift_pixels = match swim_pose.course {
                Course::Hover { drift_pixels, .. } => drift_pixels,
                Course::Lap { .. } => [0.0; 2],
            };
            ([min_corner, max_corner], drift_pixels)
        })
        .collect();
    let frame_sizes = [
        f64::from(FRAME_WIDTH_PIXELS),
        f64::from(FRAME_HEIGHT_PIXELS),
    ];
    let mut hover_fit = HoverFit {
        sweep_center: [0.0; 2],
        drift_scales: [1.0; 2],
    };
    for axis_index in 0..2 {
        let sweep_min = frame_bounds
            .iter()
            .map(|(bounds, _)| bounds[0][axis_index])
            .fold(f64::INFINITY, f64::min);
        let sweep_max = frame_bounds
            .iter()
            .map(|(bounds, _)| bounds[1][axis_index])
            .fold(f64::NEG_INFINITY, f64::max);
        let sweep_center = (sweep_min + sweep_max) / 2.0;
        let frame_half = frame_sizes[axis_index] / 2.0;
        let mut drift_scale: f64 = 1.0;
        for (bounds, drift_pixels) in &frame_bounds {
            let drift = drift_pixels[axis_index];
            let room = if drift > 0.0 {
                frame_half - FIT_MARGIN_PIXELS - (bounds[1][axis_index] - sweep_center)
            } else {
                frame_half - FIT_MARGIN_PIXELS + (bounds[0][axis_index] - sweep_center)
            };
            if drift != 0.0 {
                drift_scale = drift_scale.min(room / drift.abs());
            }
        }
        hover_fit.sweep_center[axis_index] = sweep_center;
        hover_fit.drift_scales[axis_index] = drift_scale.clamp(0.0, 1.0);
    }
    hover_fit
}

fn render_pose(
    koi_development: &KoiDevelopment,
    koi_body: KoiBody,
    koi_anatomy: &KoiAnatomy,
    swim_pose: &SwimPose,
    framing: Framing,
) -> KoiFrame {
    let koi_coat = KoiCoat::new(koi_development, koi_body, |along_fraction| {
        calculate_body_half_width(along_fraction, koi_anatomy)
    });
    let body_spine = build_spine(koi_anatomy, swim_pose);
    let fin_curves = build_fins(&body_spine, koi_anatomy, swim_pose);
    let [min_corner, max_corner] = measure_curve_bounds(&body_spine, &fin_curves);
    let pose_center = match framing {
        Framing::CenterOnKoi => [
            (min_corner[0] + max_corner[0]) / 2.0,
            (min_corner[1] + max_corner[1]) / 2.0,
        ],
        Framing::At(pose_center) => pose_center,
    };
    let koi_appearance = KoiAppearance {
        koi_anatomy,
        koi_coat: &koi_coat,
    };
    let mut rgba_pixels = Vec::with_capacity((FRAME_WIDTH_PIXELS * FRAME_HEIGHT_PIXELS) as usize);
    for pixel_y in 0..FRAME_HEIGHT_PIXELS {
        for pixel_x in 0..FRAME_WIDTH_PIXELS {
            let sample_position = [
                f64::from(pixel_x) + 0.5 - f64::from(FRAME_WIDTH_PIXELS) / 2.0 + pose_center[0],
                f64::from(pixel_y) + 0.5 - f64::from(FRAME_HEIGHT_PIXELS) / 2.0 + pose_center[1],
            ];
            // Nothing outside the curves' bounds can be covered, so skip sampling it.
            let is_inside_bounds = (0..2).all(|axis_index| {
                (min_corner[axis_index]..=max_corner[axis_index])
                    .contains(&sample_position[axis_index])
            });
            rgba_pixels.push(if is_inside_bounds {
                sample_koi(
                    &body_spine,
                    &fin_curves,
                    sample_position,
                    [pixel_x, pixel_y],
                    &koi_appearance,
                )
            } else {
                [0; 4]
            });
        }
    }
    paint_mouth(
        &mut rgba_pixels,
        &body_spine,
        koi_body,
        pose_center,
        swim_pose.mouth_opening,
    );
    let fish_pixel_mask: Vec<bool> = rgba_pixels
        .iter()
        .map(|pixel_channels| pixel_channels[3] != 0)
        .collect();
    let whisker_points =
        calculate_whisker_points(&body_spine, koi_anatomy, swim_pose.whisker_sway_radians);
    paint_whiskers(&mut rgba_pixels, &whisker_points, pose_center);
    paint_outline(&mut rgba_pixels);
    if let Some(feeding_phase) = swim_pose.feeding_phase {
        paint_feeding(
            &mut rgba_pixels,
            &body_spine,
            koi_anatomy,
            pose_center,
            feeding_phase,
        );
    }
    if swim_pose.depth > 0.0 {
        paint_depth(&mut rgba_pixels, &fish_pixel_mask, swim_pose.depth);
    }
    KoiFrame { rgba_pixels }
}

/// Where the mouth opens: on the snout's center line, just inside its front.
fn calculate_mouth_position(body_spine: &[CurvePoint]) -> [f64; 2] {
    const MOUTH_FORWARD_FRACTION: f64 = 0.75;
    let snout = body_spine[0];
    let forward_direction = calculate_forward_direction(body_spine, 0);
    [0, 1].map(|axis_index| {
        snout.position[axis_index]
            + forward_direction[axis_index] * snout.half_width_pixels * MOUTH_FORWARD_FRACTION
    })
}

/// The mouth: a circle at the front of the snout with a rim in the body's color around
/// a dark opening, growing from under a pixel to half the snout's width as it opens.
/// It is painted before the outline, so it is outlined like the koi, and a mostly
/// closed mouth paints nothing.
fn paint_mouth(
    rgba_pixels: &mut [[u8; 4]],
    body_spine: &[CurvePoint],
    koi_body: KoiBody,
    pose_center: [f64; 2],
    mouth_opening: f64,
) {
    const VISIBLE_OPENING: f64 = 0.1;
    const CLOSED_RADIUS_PIXELS: f64 = 0.8;
    const OPEN_RADIUS_PER_SNOUT_RADIUS: f64 = 0.5;
    const MIN_OPEN_RADIUS_PIXELS: f64 = 1.8;
    const RIM_WIDTH_PIXELS: f64 = 0.9;
    if mouth_opening <= VISIBLE_OPENING {
        return;
    }
    let open_radius_pixels = (body_spine[0].half_width_pixels * OPEN_RADIUS_PER_SNOUT_RADIUS)
        .max(MIN_OPEN_RADIUS_PIXELS);
    let radius_pixels = interpolate(
        CLOSED_RADIUS_PIXELS,
        open_radius_pixels,
        mouth_opening.min(1.0),
    );
    let mouth_world = calculate_mouth_position(body_spine);
    let frame_sizes = [
        f64::from(FRAME_WIDTH_PIXELS),
        f64::from(FRAME_HEIGHT_PIXELS),
    ];
    let mouth = [0, 1].map(|axis_index| {
        mouth_world[axis_index] - pose_center[axis_index] + frame_sizes[axis_index] / 2.0
    });
    let rim_color = koi_body.select_mouth_rim_color();
    let frame_width = FRAME_WIDTH_PIXELS as usize;
    let first_y = (mouth[1] - radius_pixels).floor().max(0.0) as usize;
    let last_y = ((mouth[1] + radius_pixels).ceil() as usize).min(FRAME_HEIGHT_PIXELS as usize - 1);
    let first_x = (mouth[0] - radius_pixels).floor().max(0.0) as usize;
    let last_x = ((mouth[0] + radius_pixels).ceil() as usize).min(frame_width - 1);
    for pixel_y in first_y..=last_y {
        for pixel_x in first_x..=last_x {
            let distance_pixels =
                (pixel_x as f64 + 0.5 - mouth[0]).hypot(pixel_y as f64 + 0.5 - mouth[1]);
            if distance_pixels <= radius_pixels {
                rgba_pixels[pixel_y * frame_width + pixel_x] =
                    if distance_pixels > radius_pixels - RIM_WIDTH_PIXELS {
                        rim_color
                    } else {
                        OUTLINE_COLOR
                    };
            }
        }
    }
}

/// Surface feeding: ripple rings spreading from the mouth and food pellets drifting in
/// to it, drawn on the water around the koi. Where the frame edge leaves too little
/// room, both shrink toward the mouth instead of running off the edge.
fn paint_feeding(
    rgba_pixels: &mut [[u8; 4]],
    body_spine: &[CurvePoint],
    koi_anatomy: &KoiAnatomy,
    pose_center: [f64; 2],
    feeding_phase: f64,
) {
    const RING_COUNT: usize = 3;
    const RING_WIDTH_PER_BODY_LENGTH: f64 = 0.3;
    const RING_HEIGHT_PER_WIDTH: f64 = 0.4;
    const RING_ANGLE_STEP_RADIANS: f64 = 0.035;
    const PELLET_COUNT: usize = 5;
    const PELLET_NEAREST_PIXELS: f64 = 4.0;
    const PELLET_REACH_PIXELS: f64 = 14.0;
    const PELLET_WANDER_PIXELS: f64 = 9.0;
    // Pellets vanish just before they reach the mouth.
    const PELLET_EATEN_LIFE: f64 = 0.92;
    const PELLET_COLORS: [[u8; 4]; 2] = [[228, 166, 114, 255], [184, 111, 80, 255]];
    // A clear pixel between the extras and the frame edge, plus rounding.
    const EDGE_MARGIN_PIXELS: f64 = 2.0;
    let frame_sizes = [
        f64::from(FRAME_WIDTH_PIXELS),
        f64::from(FRAME_HEIGHT_PIXELS),
    ];
    let mouth_world = calculate_mouth_position(body_spine);
    let mouth = [0, 1].map(|axis_index| {
        mouth_world[axis_index] - pose_center[axis_index] + frame_sizes[axis_index] / 2.0
    });
    let room = [0, 1].map(|axis_index| {
        (mouth[axis_index] - EDGE_MARGIN_PIXELS)
            .min(frame_sizes[axis_index] - 1.0 - EDGE_MARGIN_PIXELS - mouth[axis_index])
            .max(0.0)
    });
    let forward_direction = calculate_forward_direction(body_spine, 0);
    let frame_width = FRAME_WIDTH_PIXELS as usize;
    let mut paint_water = |point: [f64; 2], color: [u8; 4], dither_limit: f64| {
        let [pixel_x, pixel_y] = point.map(f64::floor);
        if !(0.0..frame_sizes[0]).contains(&pixel_x) || !(0.0..frame_sizes[1]).contains(&pixel_y) {
            return;
        }
        let pixel_index = pixel_y as usize * frame_width + pixel_x as usize;
        if rgba_pixels[pixel_index][3] == 0
            && calculate_dither_threshold([pixel_x as u32, pixel_y as u32]) < dither_limit
        {
            rgba_pixels[pixel_index] = color;
        }
    };
    let largest_ring_width = koi_anatomy.body_length_pixels * RING_WIDTH_PER_BODY_LENGTH * 0.85;
    let ring_scale = 1.0_f64
        .min(room[0] / largest_ring_width)
        .min(room[1] / (largest_ring_width * RING_HEIGHT_PER_WIDTH));
    for ring_index in 0..RING_COUNT {
        let ring_phase = (feeding_phase + ring_index as f64 / RING_COUNT as f64).fract();
        let ring_width = ring_scale
            * koi_anatomy.body_length_pixels
            * RING_WIDTH_PER_BODY_LENGTH
            * (0.25 + 0.6 * ring_phase);
        let ring_color = WHITE_RAMP[if ring_index % 2 == 1 { 2 } else { 1 }];
        let mut ring_angle: f64 = 0.0;
        while ring_angle < TAU {
            paint_water(
                [
                    mouth[0] + ring_angle.cos() * ring_width,
                    mouth[1] + ring_angle.sin() * ring_width * RING_HEIGHT_PER_WIDTH,
                ],
                ring_color,
                (1.0 - ring_phase) * 0.75,
            );
            ring_angle += RING_ANGLE_STEP_RADIANS;
        }
    }
    // The second pellet pixel sits one to the right, so leave it room too.
    let farthest_pellet_pixels = PELLET_NEAREST_PIXELS + PELLET_REACH_PIXELS + PELLET_WANDER_PIXELS;
    let pellet_scale = 1.0_f64.min((room[0].min(room[1]) - 1.0).max(0.0) / farthest_pellet_pixels);
    for pellet_index in 0..PELLET_COUNT {
        let life = (feeding_phase + pellet_index as f64 / PELLET_COUNT as f64).fract();
        if life >= PELLET_EATEN_LIFE {
            continue;
        }
        let reach = PELLET_NEAREST_PIXELS + (1.0 - life) * PELLET_REACH_PIXELS;
        let wander = (1.0 - life) * PELLET_WANDER_PIXELS;
        let pellet_point = [
            mouth[0]
                + pellet_scale
                    * (forward_direction[0] * reach + (pellet_index as f64 * 2.1).sin() * wander),
            mouth[1]
                + pellet_scale
                    * (forward_direction[1] * reach + (pellet_index as f64 * 1.7).cos() * wander),
        ];
        paint_water(pellet_point, PELLET_COLORS[0], 1.0);
        paint_water(
            [pellet_point[0] + 1.0, pellet_point[1]],
            PELLET_COLORS[1],
            1.0,
        );
    }
}

/// Under water the koi becomes a blue koi: each body and fin pixel maps its own
/// brightness onto a water-blue scale, the whiskers and outline soften to deep blue,
/// and a dithered share of each drops out, more for the outline.
fn paint_depth(rgba_pixels: &mut [[u8; 4]], fish_pixel_mask: &[bool], depth: f64) {
    const DEEP_WATER: [f64; 3] = [18.0, 78.0, 137.0];
    const MID_WATER: [f64; 3] = [0.0, 153.0, 219.0];
    const SHALLOW_WATER: [f64; 3] = [190.0, 225.0, 245.0];
    const OUTLINE_WATER: [f64; 3] = [24.0, 48.0, 92.0];
    const FISH_DROPOUT_PER_DEPTH: f64 = 0.15;
    const OUTLINE_DROPOUT_PER_DEPTH: f64 = 0.45;
    const TINT_PER_DEPTH: f64 = 1.15;
    let tint = (depth * TINT_PER_DEPTH).min(1.0);
    let frame_width = FRAME_WIDTH_PIXELS as usize;
    for (pixel_index, pixel_channels) in rgba_pixels.iter_mut().enumerate() {
        if pixel_channels[3] == 0 {
            continue;
        }
        let is_fish_pixel = fish_pixel_mask[pixel_index];
        let dropout_per_depth = if is_fish_pixel {
            FISH_DROPOUT_PER_DEPTH
        } else {
            OUTLINE_DROPOUT_PER_DEPTH
        };
        let pixel_position = [
            (pixel_index % frame_width) as u32,
            (pixel_index / frame_width) as u32,
        ];
        if calculate_dither_threshold(pixel_position) < depth * dropout_per_depth {
            *pixel_channels = [0; 4];
            continue;
        }
        let original_color =
            [0, 1, 2].map(|channel_index| f64::from(pixel_channels[channel_index]));
        let water_color = if is_fish_pixel {
            let brightness =
                (0.3 * original_color[0] + 0.59 * original_color[1] + 0.11 * original_color[2])
                    / 255.0;
            let (from_color, to_color, blend) = if brightness < 0.5 {
                (DEEP_WATER, MID_WATER, brightness * 2.0)
            } else {
                (MID_WATER, SHALLOW_WATER, brightness * 2.0 - 1.0)
            };
            [0, 1, 2].map(|channel_index| {
                interpolate(from_color[channel_index], to_color[channel_index], blend)
            })
        } else {
            OUTLINE_WATER
        };
        for channel_index in 0..3 {
            pixel_channels[channel_index] = interpolate(
                original_color[channel_index],
                water_color[channel_index],
                tint,
            )
            .round() as u8;
        }
    }
}

const OUTLINE_COLOR: [u8; 4] = [24, 20, 37, 255];
const DITHER_RANKS: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

struct KoiAnatomy {
    body_length_pixels: f64,
    body_width_pixels: f64,
    /// Share of the body length over which the head widens from the nose.
    head_slant_fraction: f64,
    /// Nose width as a share of the full body width.
    nose_width_fraction: f64,
    /// Exponent of the taper from the shoulders to the tail stalk.
    tail_taper_exponent: f64,
    /// Fin shapes blend from young (0) to adult (1).
    adult_fin_fraction: f64,
    fin_length_multiplier: f64,
    eye_along_fraction: f64,
    /// Young koi have proportionally larger eyes; adults use 1.
    eye_scale: f64,
}

struct KoiAppearance<'a> {
    koi_anatomy: &'a KoiAnatomy,
    koi_coat: &'a KoiCoat,
}

// Positions and half widths are pixels; consecutive points form tapered strokes.
#[derive(Clone, Copy)]
struct CurvePoint {
    position: [f64; 2],
    half_width_pixels: f64,
}

struct CurveSample {
    along_fraction: f64,
    lateral_pixels: f64,
    distance_pixels: f64,
    half_width_pixels: f64,
    normal_direction: [f64; 2],
}

#[derive(Clone, Copy)]
struct FinShape {
    length_body_widths: f64,
    half_width_body_widths: f64,
    width_start_fraction: f64,
    taper_exponent: f64,
}

impl FinShape {
    fn blend(self, other: Self, other_fraction: f64) -> Self {
        Self {
            length_body_widths: interpolate(
                self.length_body_widths,
                other.length_body_widths,
                other_fraction,
            ),
            half_width_body_widths: interpolate(
                self.half_width_body_widths,
                other.half_width_body_widths,
                other_fraction,
            ),
            width_start_fraction: interpolate(
                self.width_start_fraction,
                other.width_start_fraction,
                other_fraction,
            ),
            taper_exponent: interpolate(self.taper_exponent, other.taper_exponent, other_fraction),
        }
    }
}

// Each pair shares its proportions: short rounded fins when young, longer adult fins.
// Direction offsets in radians describe the two fins independently, relative to the
// spine's backward tangent.
struct FinPair {
    fin_kind: FinKind,
    root_along_fraction: f64,
    root_side_fraction: f64,
    young_shape: FinShape,
    adult_shape: FinShape,
    direction_offsets: [&'static [f64]; 2],
}

#[derive(Clone, Copy)]
enum FinKind {
    Tail,
    Side,
    Pelvic,
}

const FIN_PAIRS: [FinPair; 3] = [
    FinPair {
        fin_kind: FinKind::Tail,
        root_along_fraction: 1.0,
        root_side_fraction: 0.0,
        young_shape: FinShape {
            length_body_widths: 1.05,
            half_width_body_widths: 0.3,
            width_start_fraction: 0.16,
            taper_exponent: 0.6,
        },
        adult_shape: FinShape {
            length_body_widths: 1.62,
            half_width_body_widths: 0.3,
            width_start_fraction: 0.125,
            taper_exponent: 0.69,
        },
        direction_offsets: [
            &[-0.18035, -0.25648, -0.37367, -0.50421, -0.61344, -0.66997],
            &[0.248, 0.30115, 0.29856, 0.24064, 0.14695, 0.05079],
        ],
    },
    FinPair {
        fin_kind: FinKind::Side,
        root_along_fraction: 0.22,
        root_side_fraction: 0.8,
        young_shape: FinShape {
            length_body_widths: 0.66,
            half_width_body_widths: 0.25,
            width_start_fraction: 0.18,
            taper_exponent: 0.45,
        },
        adult_shape: FinShape {
            length_body_widths: 1.13,
            half_width_body_widths: 0.24,
            width_start_fraction: 0.09,
            taper_exponent: 0.54,
        },
        direction_offsets: [
            &[0.87, 0.77239, 0.71583, 0.70044],
            &[-0.65221, -0.58002, -0.5356, -0.48311],
        ],
    },
    FinPair {
        fin_kind: FinKind::Pelvic,
        root_along_fraction: 0.47,
        root_side_fraction: 0.7,
        young_shape: FinShape {
            length_body_widths: 0.36,
            half_width_body_widths: 0.1,
            width_start_fraction: 0.15,
            taper_exponent: 0.5,
        },
        adult_shape: FinShape {
            length_body_widths: 0.57,
            half_width_body_widths: 0.11,
            width_start_fraction: 0.11,
            taper_exponent: 0.59,
        },
        direction_offsets: [&[0.39139, 0.3769, 0.39376], &[-0.62997, -0.48164, -0.32809]],
    },
];

fn calculate_anatomy(total_contributions: u64) -> KoiAnatomy {
    const MIN_BODY_LENGTH_PIXELS: f64 = 37.5;
    const ADULT_BODY_LENGTH_PIXELS: f64 = 137.5;
    const MAX_BODY_LENGTH_PIXELS: f64 = 167.5;
    const YOUNG_BODY_WIDTH_FRACTION: f64 = 0.17;
    const ADULT_BODY_WIDTH_FRACTION: f64 = 0.2;
    const MATURE_BODY_WIDTH_GAIN: f64 = 0.045;
    const YOUNG_HEAD_SLANT_FRACTION: f64 = 0.14;
    const ADULT_HEAD_SLANT_FRACTION: f64 = 0.187;
    const YOUNG_NOSE_WIDTH_FRACTION: f64 = 0.75;
    const ADULT_NOSE_WIDTH_FRACTION: f64 = 0.517;
    const YOUNG_TAIL_TAPER_EXPONENT: f64 = 1.25;
    const ADULT_TAIL_TAPER_EXPONENT: f64 = 1.8;
    const YOUNG_EYE_ALONG_FRACTION: f64 = 0.08;
    const ADULT_EYE_ALONG_FRACTION: f64 = 0.06;
    const YOUNG_EYE_SCALE: f64 = 2.1;
    const YOUNG_FIN_LENGTH_MULTIPLIER: f64 = 0.85;
    const MAX_FIN_LENGTH_MULTIPLIER: f64 = 1.1;
    // Fins keep their young shape for the first quarter of the young stage.
    const YOUNG_FIN_HOLD_FRACTION: f64 = 0.25;
    let growth_fraction = calculate_growth_fraction(total_contributions);
    let adult_growth_fraction = calculate_growth_fraction(ADULT_CONTRIBUTIONS);
    let young_growth_fraction = (growth_fraction / adult_growth_fraction).min(1.0);
    let mature_growth_fraction =
        ((growth_fraction - adult_growth_fraction) / (1.0 - adult_growth_fraction)).max(0.0);
    let adult_shape_fraction =
        (growth_fraction / calculate_growth_fraction(YOUNG_SHAPE_CONTRIBUTIONS)).min(1.0);
    let body_length_pixels = MIN_BODY_LENGTH_PIXELS
        + (ADULT_BODY_LENGTH_PIXELS - MIN_BODY_LENGTH_PIXELS) * young_growth_fraction
        + (MAX_BODY_LENGTH_PIXELS - ADULT_BODY_LENGTH_PIXELS) * mature_growth_fraction;
    let body_width_fraction = interpolate(
        YOUNG_BODY_WIDTH_FRACTION,
        ADULT_BODY_WIDTH_FRACTION,
        adult_shape_fraction,
    ) + MATURE_BODY_WIDTH_GAIN * mature_growth_fraction;
    KoiAnatomy {
        body_length_pixels,
        body_width_pixels: body_length_pixels * body_width_fraction,
        head_slant_fraction: interpolate(
            YOUNG_HEAD_SLANT_FRACTION,
            ADULT_HEAD_SLANT_FRACTION,
            adult_shape_fraction,
        ),
        nose_width_fraction: interpolate(
            YOUNG_NOSE_WIDTH_FRACTION,
            ADULT_NOSE_WIDTH_FRACTION,
            adult_shape_fraction,
        ),
        tail_taper_exponent: interpolate(
            YOUNG_TAIL_TAPER_EXPONENT,
            ADULT_TAIL_TAPER_EXPONENT,
            adult_shape_fraction,
        ),
        adult_fin_fraction: smooth_step(
            (adult_shape_fraction - YOUNG_FIN_HOLD_FRACTION) / (1.0 - YOUNG_FIN_HOLD_FRACTION),
        ),
        fin_length_multiplier: interpolate(
            YOUNG_FIN_LENGTH_MULTIPLIER,
            1.0,
            smooth_step(adult_shape_fraction),
        ) * interpolate(
            1.0,
            MAX_FIN_LENGTH_MULTIPLIER,
            mature_growth_fraction,
        ),
        eye_along_fraction: interpolate(
            YOUNG_EYE_ALONG_FRACTION,
            ADULT_EYE_ALONG_FRACTION,
            adult_shape_fraction,
        ),
        eye_scale: interpolate(YOUNG_EYE_SCALE, 1.0, adult_shape_fraction),
    }
}

fn interpolate(from: f64, to: f64, fraction: f64) -> f64 {
    from + (to - from) * fraction
}

/// Ease in and out across zero through one; values outside are clamped.
fn smooth_step(fraction: f64) -> f64 {
    let fraction = fraction.clamp(0.0, 1.0);
    fraction * fraction * (3.0 - 2.0 * fraction)
}

/// The spine follows the pose's course: the still pose's fixed heading and gentle bend,
/// turned by any heading offset, or a clockwise circle around the frame center.
fn build_spine(koi_anatomy: &KoiAnatomy, swim_pose: &SwimPose) -> Vec<CurvePoint> {
    const HEADING_RADIANS: f64 = -0.08;
    const BEND_RADIANS: f64 = 0.35;
    const WAVE_AMPLITUDE_FRACTION: f64 = 0.06;
    const WAVE_CYCLES: f64 = 0.9;
    let body_length_pixels = koi_anatomy.body_length_pixels;
    let point_count = body_length_pixels.round() as usize;
    const MIN_HALF_WIDTH_PIXELS: f64 = 1.3;
    let calculate_point_half_width = |point_index: usize| {
        calculate_body_half_width(point_index as f64 / point_count as f64, koi_anatomy)
            .max(MIN_HALF_WIDTH_PIXELS)
    };
    let center_points = match swim_pose.course {
        Course::Hover {
            heading_offset_radians,
            ..
        } => {
            let mut center_points = vec![CurvePoint {
                position: [0.0, 0.0],
                half_width_pixels: calculate_point_half_width(0),
            }];
            for point_index in 1..point_count {
                let backward_angle_radians = HEADING_RADIANS
                    + heading_offset_radians
                    + PI
                    + BEND_RADIANS * point_index as f64 / point_count as f64;
                let [previous_x, previous_y] = center_points[point_index - 1].position;
                center_points.push(CurvePoint {
                    position: [
                        previous_x + backward_angle_radians.cos(),
                        previous_y + backward_angle_radians.sin(),
                    ],
                    half_width_pixels: calculate_point_half_width(point_index),
                });
            }
            center_points
        }
        // Each body point sits one pixel of arc behind the one in front.
        Course::Lap { angle_radians } => {
            let radius_pixels = calculate_lap_radius(body_length_pixels);
            (0..point_count)
                .map(|point_index| {
                    let point_angle_radians = angle_radians - point_index as f64 / radius_pixels;
                    CurvePoint {
                        position: [
                            radius_pixels * point_angle_radians.cos(),
                            radius_pixels * point_angle_radians.sin(),
                        ],
                        half_width_pixels: calculate_point_half_width(point_index),
                    }
                })
                .collect()
        }
    };
    let mut body_spine = Vec::with_capacity(point_count);
    for (point_index, center_point) in center_points.iter().enumerate() {
        let along_fraction = point_index as f64 / point_count as f64;
        let forward_direction = calculate_forward_direction(&center_points, point_index);
        let head_wave_fraction = 0.1;
        let wave_envelope =
            head_wave_fraction + (1.0 - head_wave_fraction) * along_fraction.powi(2);
        let wave_offset = body_length_pixels
            * WAVE_AMPLITUDE_FRACTION
            * swim_pose.wave_strength
            * wave_envelope
            * (along_fraction * TAU * WAVE_CYCLES - swim_pose.wave_phase_radians).sin();
        body_spine.push(CurvePoint {
            position: [
                center_point.position[0] - forward_direction[1] * wave_offset,
                center_point.position[1] + forward_direction[0] * wave_offset,
            ],
            half_width_pixels: center_point.half_width_pixels,
        });
    }
    body_spine
}

/// Each fin turns at its root and bends along its length as the pose says; a turn
/// outward is away from the body, whichever side the fin is on.
fn build_fins(
    body_spine: &[CurvePoint],
    koi_anatomy: &KoiAnatomy,
    swim_pose: &SwimPose,
) -> Vec<Vec<CurvePoint>> {
    let mut fin_curves = Vec::new();
    for fin_pair in &FIN_PAIRS {
        let root_index =
            (fin_pair.root_along_fraction * (body_spine.len() - 1) as f64).round() as usize;
        let root_point = body_spine[root_index];
        let forward_direction = calculate_forward_direction(body_spine, root_index);
        let backward_angle_radians = (-forward_direction[1]).atan2(-forward_direction[0]);
        for (side_index, (body_side, direction_offsets)) in [-1.0, 1.0]
            .into_iter()
            .zip(fin_pair.direction_offsets)
            .enumerate()
        {
            let outward_sign = direction_offsets[0].signum();
            let (root_turn_radians, bend_radians) = match fin_pair.fin_kind {
                FinKind::Tail => (
                    outward_sign * swim_pose.tail_spread_radians,
                    swim_pose.tail_lobe_bends_radians[side_index],
                ),
                FinKind::Side => (
                    outward_sign * swim_pose.side_fin_openings_radians[side_index],
                    outward_sign * swim_pose.side_fin_bends_radians[side_index],
                ),
                FinKind::Pelvic => (outward_sign * swim_pose.pelvic_fin_opening_radians, 0.0),
            };
            let root_offset_pixels =
                root_point.half_width_pixels * fin_pair.root_side_fraction * body_side;
            let root_position = [
                root_point.position[0] - forward_direction[1] * root_offset_pixels,
                root_point.position[1] + forward_direction[0] * root_offset_pixels,
            ];
            fin_curves.push(build_fin(
                fin_pair,
                root_position,
                backward_angle_radians + root_turn_radians,
                koi_anatomy,
                direction_offsets,
                bend_radians,
            ));
        }
    }
    fin_curves
}

fn build_fin(
    fin_pair: &FinPair,
    root_position: [f64; 2],
    backward_angle_radians: f64,
    koi_anatomy: &KoiAnatomy,
    direction_offsets: &[f64],
    bend_radians: f64,
) -> Vec<CurvePoint> {
    let fin_shape = fin_pair
        .young_shape
        .blend(fin_pair.adult_shape, koi_anatomy.adult_fin_fraction);
    let fin_length_pixels = koi_anatomy.body_width_pixels
        * fin_shape.length_body_widths
        * koi_anatomy.fin_length_multiplier;
    let segment_count = fin_length_pixels.ceil() as usize;
    let segment_length_pixels = fin_length_pixels / segment_count as f64;
    let mut center_position = root_position;
    let mut fin_points = Vec::new();
    for point_index in 0..=segment_count {
        let along_fraction = point_index as f64 / segment_count as f64;
        let width_progress = fin_shape.width_start_fraction
            + (1.0 - fin_shape.width_start_fraction) * along_fraction;
        fin_points.push(CurvePoint {
            position: center_position,
            half_width_pixels: koi_anatomy.body_width_pixels
                * fin_shape.half_width_body_widths
                * (PI * width_progress)
                    .sin()
                    .max(0.0)
                    .powf(fin_shape.taper_exponent),
        });
        let direction_position = (along_fraction * direction_offsets.len() as f64 - 0.5)
            .clamp(0.0, (direction_offsets.len() - 1) as f64);
        let direction_index = direction_position.floor() as usize;
        let start_angle = direction_offsets[direction_index];
        let end_angle = direction_offsets[(direction_index + 1).min(direction_offsets.len() - 1)];
        let segment_angle = backward_angle_radians
            + start_angle
            + (end_angle - start_angle) * direction_position.fract()
            + bend_radians * along_fraction;
        center_position[0] += segment_length_pixels * segment_angle.cos();
        center_position[1] += segment_length_pixels * segment_angle.sin();
    }
    fin_points
}

/// Paint the whisker points. They fill only empty pixels and are painted before the
/// outline, so they are outlined like the koi.
fn paint_whiskers(rgba_pixels: &mut [[u8; 4]], whisker_points: &[[f64; 2]], pose_center: [f64; 2]) {
    let frame_width = FRAME_WIDTH_PIXELS as usize;
    for whisker_point in whisker_points {
        let pixel_x =
            (whisker_point[0] - pose_center[0] + f64::from(FRAME_WIDTH_PIXELS) / 2.0).floor();
        let pixel_y =
            (whisker_point[1] - pose_center[1] + f64::from(FRAME_HEIGHT_PIXELS) / 2.0).floor();
        if !(0.0..f64::from(FRAME_WIDTH_PIXELS)).contains(&pixel_x)
            || !(0.0..f64::from(FRAME_HEIGHT_PIXELS)).contains(&pixel_y)
        {
            continue;
        }
        let pixel_index = pixel_y as usize * frame_width + pixel_x as usize;
        if rgba_pixels[pixel_index][3] == 0 {
            rgba_pixels[pixel_index] = WHITE_RAMP[1];
        }
    }
}

/// One point per whisker pixel: short lines angled forward and out from the edge of
/// the snout, swung together by the sway.
fn calculate_whisker_points(
    body_spine: &[CurvePoint],
    koi_anatomy: &KoiAnatomy,
    sway_radians: f64,
) -> Vec<[f64; 2]> {
    const SPREAD_RADIANS: f64 = 0.85;
    const ROOT_SIDE_FRACTION: f64 = 0.8;
    const ROOT_FORWARD_FRACTION: f64 = 0.55;
    let whisker_length_pixels = calculate_whisker_length(koi_anatomy);
    let snout_position = body_spine[0].position;
    let snout_radius_pixels = body_spine[0].half_width_pixels;
    let forward_direction = calculate_forward_direction(body_spine, 0);
    let heading_radians = forward_direction[1].atan2(forward_direction[0]);
    let mut whisker_points = Vec::new();
    for body_side in [-1.0, 1.0] {
        let side_offset_pixels = snout_radius_pixels * ROOT_SIDE_FRACTION * body_side;
        let forward_offset_pixels = snout_radius_pixels * ROOT_FORWARD_FRACTION;
        let root_position = [
            snout_position[0] - forward_direction[1] * side_offset_pixels
                + forward_direction[0] * forward_offset_pixels,
            snout_position[1]
                + forward_direction[0] * side_offset_pixels
                + forward_direction[1] * forward_offset_pixels,
        ];
        let whisker_angle = heading_radians + sway_radians + body_side * SPREAD_RADIANS;
        for step_pixels in 1..=whisker_length_pixels {
            whisker_points.push([
                root_position[0] + whisker_angle.cos() * f64::from(step_pixels),
                root_position[1] + whisker_angle.sin() * f64::from(step_pixels),
            ]);
        }
    }
    whisker_points
}

/// Whiskers grow with the koi, from three to five pixels.
fn calculate_whisker_length(koi_anatomy: &KoiAnatomy) -> u32 {
    const BODY_PIXELS_PER_WHISKER_PIXEL: f64 = 34.0;
    (koi_anatomy.body_length_pixels / BODY_PIXELS_PER_WHISKER_PIXEL)
        .round()
        .clamp(3.0, 5.0) as u32
}

/// The lap circle's radius: as wide as the frame allows, tighter for longer koi so the
/// whole fish stays inside, and never so tight that the body covers more than its
/// share of the circle.
fn calculate_lap_radius(body_length_pixels: f64) -> f64 {
    const FRAME_REACH_PIXELS: f64 = 116.0;
    const OUTER_REACH_FRACTION: f64 = 0.33;
    const MAX_BODY_SHARE_OF_LAP: f64 = 0.6;
    (FRAME_REACH_PIXELS - OUTER_REACH_FRACTION * body_length_pixels)
        .min(MAX_LAP_RADIUS_PIXELS)
        .max(body_length_pixels / (TAU * MAX_BODY_SHARE_OF_LAP))
}

/// The corners of the box that holds every curve, including its half widths.
fn measure_curve_bounds(
    body_spine: &[CurvePoint],
    fin_curves: &[Vec<CurvePoint>],
) -> [[f64; 2]; 2] {
    let mut min_corner = [f64::INFINITY; 2];
    let mut max_corner = [f64::NEG_INFINITY; 2];
    for curve_point in body_spine.iter().chain(fin_curves.iter().flatten()) {
        for axis_index in 0..2 {
            min_corner[axis_index] = min_corner[axis_index]
                .min(curve_point.position[axis_index] - curve_point.half_width_pixels);
            max_corner[axis_index] = max_corner[axis_index]
                .max(curve_point.position[axis_index] + curve_point.half_width_pixels);
        }
    }
    [min_corner, max_corner]
}

fn sample_koi(
    body_spine: &[CurvePoint],
    fin_curves: &[Vec<CurvePoint>],
    sample_position: [f64; 2],
    pixel_position: [u32; 2],
    koi_appearance: &KoiAppearance,
) -> [u8; 4] {
    if let Some(body_sample) = sample_curve(body_spine, sample_position) {
        return shade_body(&body_sample, pixel_position, koi_appearance);
    }
    for fin_curve in fin_curves {
        if let Some(fin_sample) = sample_curve(fin_curve, sample_position) {
            return shade_fin(&fin_sample, pixel_position, koi_appearance.koi_coat);
        }
    }
    [0; 4]
}

// Constructed curves contain at least two points and no zero-length segments.
fn sample_curve(curve_points: &[CurvePoint], sample_position: [f64; 2]) -> Option<CurveSample> {
    let mut closest_sample = None;
    let mut closest_distance_pixels = f64::INFINITY;
    for (segment_index, end_points) in curve_points.windows(2).enumerate() {
        let [start_point, end_point] = [end_points[0], end_points[1]];
        let segment_x = end_point.position[0] - start_point.position[0];
        let segment_y = end_point.position[1] - start_point.position[1];
        let offset_x = sample_position[0] - start_point.position[0];
        let offset_y = sample_position[1] - start_point.position[1];
        let segment_length_pixels = segment_x.hypot(segment_y);
        let segment_progress = ((offset_x * segment_x + offset_y * segment_y)
            / segment_length_pixels.powi(2))
        .clamp(0.0, 1.0);
        let distance_pixels = (offset_x - segment_progress * segment_x)
            .hypot(offset_y - segment_progress * segment_y);
        if distance_pixels >= closest_distance_pixels {
            continue;
        }
        closest_distance_pixels = distance_pixels;
        closest_sample = Some(CurveSample {
            along_fraction: (segment_index as f64 + segment_progress)
                / (curve_points.len() - 1) as f64,
            lateral_pixels: (offset_x * segment_y - offset_y * segment_x) / segment_length_pixels,
            distance_pixels,
            half_width_pixels: start_point.half_width_pixels
                + segment_progress * (end_point.half_width_pixels - start_point.half_width_pixels),
            normal_direction: [
                segment_y / segment_length_pixels,
                -segment_x / segment_length_pixels,
            ],
        });
    }
    closest_sample
        .filter(|curve_sample| curve_sample.distance_pixels <= curve_sample.half_width_pixels)
}

fn shade_body(
    body_sample: &CurveSample,
    pixel_position: [u32; 2],
    koi_appearance: &KoiAppearance,
) -> [u8; 4] {
    let koi_anatomy = koi_appearance.koi_anatomy;
    let body_length_pixels = koi_anatomy.body_length_pixels;
    let lateral_fraction = body_sample.lateral_pixels / body_sample.half_width_pixels;
    let body_paint = koi_appearance.koi_coat.select_body_paint(
        body_sample.along_fraction,
        lateral_fraction,
        body_length_pixels,
        body_sample.half_width_pixels,
        calculate_dither_threshold(pixel_position),
    );
    const INNER_RIM_PIXELS: f64 = 0.8;
    if body_sample.distance_pixels > body_sample.half_width_pixels - INNER_RIM_PIXELS {
        return body_paint.shade_ramp[0];
    }
    let eye_lateral_fraction = 0.55;
    let eye_half_thickness_pixels = 0.8 * koi_anatomy.eye_scale;
    let eye_half_span_fraction = 0.25 * koi_anatomy.eye_scale.sqrt();
    if ((body_sample.along_fraction - koi_anatomy.eye_along_fraction) * body_length_pixels).abs()
        < eye_half_thickness_pixels
        && (lateral_fraction.abs() - eye_lateral_fraction).abs() < eye_half_span_fraction
    {
        return OUTLINE_COLOR;
    }
    if let Some(glint_color) = body_paint.glint_color {
        return glint_color;
    }
    let surface_brightness = calculate_body_brightness(body_sample, lateral_fraction, koi_anatomy)
        + body_paint.brightness_shift;
    adjust_saturation(
        choose_shade(body_paint.shade_ramp, surface_brightness, pixel_position),
        body_paint.saturation,
    )
}

fn calculate_body_brightness(
    body_sample: &CurveSample,
    lateral_fraction: f64,
    koi_anatomy: &KoiAnatomy,
) -> f64 {
    const AMBIENT_BRIGHTNESS: f64 = 0.24;
    const DIFFUSE_STRENGTH: f64 = 0.7;
    const LIGHT_DIRECTION: [f64; 3] = [-0.32, -0.5, 0.8];
    let surface_normal_z = (1.0 - lateral_fraction.powi(2)).max(0.0).sqrt();
    let incident_light = lateral_fraction
        * (body_sample.normal_direction[0] * LIGHT_DIRECTION[0]
            + body_sample.normal_direction[1] * LIGHT_DIRECTION[1])
        + surface_normal_z * LIGHT_DIRECTION[2];
    let mut surface_brightness = AMBIENT_BRIGHTNESS + DIFFUSE_STRENGTH * incident_light.max(0.0);
    let dorsal_span = 0.27..0.62;
    let dorsal_half_width_fraction = 0.1;
    let dorsal_shadow = 0.18;
    if dorsal_span.contains(&body_sample.along_fraction)
        && lateral_fraction.abs() < dorsal_half_width_fraction
    {
        surface_brightness -= dorsal_shadow;
    }
    // The gill line sits near the back of the head's slant.
    let gill_along_fraction = koi_anatomy.head_slant_fraction * 0.72;
    let gill_inner_fraction = 0.3;
    let gill_shadow = 0.22;
    if ((body_sample.along_fraction - gill_along_fraction) * koi_anatomy.body_length_pixels).abs()
        < 0.5
        && lateral_fraction.abs() > gill_inner_fraction
    {
        surface_brightness -= gill_shadow;
    }
    surface_brightness
}

fn shade_fin(fin_sample: &CurveSample, pixel_position: [u32; 2], koi_coat: &KoiCoat) -> [u8; 4] {
    const FIN_RIM_PIXELS: f64 = 0.6;
    const FIN_RAY_SPACING_PIXELS: f64 = 1.8;
    const FIN_ROOT_SHADOW: f64 = 0.12;
    let is_on_rim = fin_sample.distance_pixels > fin_sample.half_width_pixels - FIN_RIM_PIXELS;
    let fin_paint = koi_coat.select_fin_paint(
        fin_sample.along_fraction,
        fin_sample.lateral_pixels / fin_sample.half_width_pixels,
        is_on_rim,
        calculate_dither_threshold(pixel_position),
    );
    if is_on_rim {
        return adjust_saturation(fin_paint.shade_ramp[1], fin_paint.saturation);
    }
    let ray_phase = (fin_sample.lateral_pixels / FIN_RAY_SPACING_PIXELS).rem_euclid(1.0);
    let ray_brightness = 0.68;
    let membrane_brightness = 0.94;
    let surface_brightness = if ray_phase < 0.5 {
        ray_brightness
    } else {
        membrane_brightness
    };
    let root_shadow = FIN_ROOT_SHADOW * (1.0 - fin_sample.along_fraction).powi(2);
    adjust_saturation(
        choose_shade(
            fin_paint.shade_ramp,
            surface_brightness - root_shadow,
            pixel_position,
        ),
        fin_paint.saturation,
    )
}

/// The body is drawn as a chain of round segments. The head widens from a round nose
/// along a gentle slant, so no segment outgrows the one in front of it: a steeper rise
/// leaves a knob at the tip, and a narrow head behind a wide body leaves a pinch.
fn calculate_body_half_width(along_fraction: f64, koi_anatomy: &KoiAnatomy) -> f64 {
    const MIN_SHOULDER_FRACTION: f64 = 0.3;
    const SHOULDER_GAP_FRACTION: f64 = 0.04;
    const TAPER_END_FRACTION: f64 = 0.92;
    const TAIL_WIDTH_FRACTION: f64 = 0.26;
    let head_slant_fraction = koi_anatomy.head_slant_fraction;
    let width_fraction = if along_fraction < head_slant_fraction {
        let nose_width_fraction = koi_anatomy.nose_width_fraction;
        nose_width_fraction
            + (1.0 - nose_width_fraction) * (along_fraction / head_slant_fraction * PI / 2.0).sin()
    } else {
        let shoulder_fraction =
            (head_slant_fraction + SHOULDER_GAP_FRACTION).max(MIN_SHOULDER_FRACTION);
        let taper_length_fraction = TAPER_END_FRACTION - shoulder_fraction;
        (1.0 - ((along_fraction - shoulder_fraction).max(0.0) / taper_length_fraction)
            .powf(koi_anatomy.tail_taper_exponent))
        .max(TAIL_WIDTH_FRACTION)
    };
    koi_anatomy.body_width_pixels / 2.0 * width_fraction
}

fn calculate_forward_direction(curve_points: &[CurvePoint], point_index: usize) -> [f64; 2] {
    const DIRECTION_SPAN_SAMPLES: usize = 2;
    let earlier_point = curve_points[point_index.saturating_sub(DIRECTION_SPAN_SAMPLES)].position;
    let later_point =
        curve_points[(point_index + DIRECTION_SPAN_SAMPLES).min(curve_points.len() - 1)].position;
    let tangent_x = earlier_point[0] - later_point[0];
    let tangent_y = earlier_point[1] - later_point[1];
    let tangent_length_pixels = tangent_x.hypot(tangent_y);
    [
        tangent_x / tangent_length_pixels,
        tangent_y / tangent_length_pixels,
    ]
}

fn paint_outline(rgba_pixels: &mut [[u8; 4]]) {
    let original_pixels = rgba_pixels.to_vec();
    let frame_width = FRAME_WIDTH_PIXELS as usize;
    for (pixel_index, pixel_channels) in rgba_pixels.iter_mut().enumerate() {
        if original_pixels[pixel_index][3] != 0 {
            continue;
        }
        let pixel_x = pixel_index % frame_width;
        let pixel_y = pixel_index / frame_width;
        let touches_shape = (pixel_x > 0 && original_pixels[pixel_index - 1][3] != 0)
            || (pixel_x + 1 < frame_width && original_pixels[pixel_index + 1][3] != 0)
            || (pixel_y > 0 && original_pixels[pixel_index - frame_width][3] != 0)
            || (pixel_y + 1 < FRAME_HEIGHT_PIXELS as usize
                && original_pixels[pixel_index + frame_width][3] != 0);
        if touches_shape {
            *pixel_channels = OUTLINE_COLOR;
        }
    }
}

fn choose_shade(
    shade_ramp: &[[u8; 4]],
    surface_brightness: f64,
    pixel_position: [u32; 2],
) -> [u8; 4] {
    let shade_position = surface_brightness.clamp(0.0, 1.0) * (shade_ramp.len() - 1) as f64;
    let base_index = shade_position.floor() as usize;
    let shade_index = base_index
        + usize::from(shade_position.fract() > calculate_dither_threshold(pixel_position));
    shade_ramp[shade_index.min(shade_ramp.len() - 1)]
}

/// Scale a color's distance from its own gray; one leaves it unchanged.
fn adjust_saturation(pixel_color: [u8; 4], saturation: f64) -> [u8; 4] {
    let [red, green, blue, alpha] = pixel_color.map(f64::from);
    let gray = 0.3 * red + 0.59 * green + 0.11 * blue;
    let saturate = |channel: f64| {
        (gray + (channel - gray) * saturation)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    [saturate(red), saturate(green), saturate(blue), alpha as u8]
}

/// The ordered-dither threshold at a pixel, from zero through one.
fn calculate_dither_threshold(pixel_position: [u32; 2]) -> f64 {
    let dither_x = pixel_position[0] as usize % DITHER_RANKS[0].len();
    let dither_y = pixel_position[1] as usize % DITHER_RANKS.len();
    (f64::from(DITHER_RANKS[dither_y][dither_x]) + 0.5)
        / (DITHER_RANKS.len() * DITHER_RANKS[0].len()) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Paint only the mouth on an empty canvas centered on it.
    fn paint_mouth_alone(koi_body: KoiBody, mouth_opening: f64) -> Vec<[u8; 4]> {
        let koi_anatomy = calculate_anatomy(1_500);
        let body_spine = build_spine(&koi_anatomy, &STILL_POSE);
        let mut rgba_pixels = vec![[0; 4]; (FRAME_WIDTH_PIXELS * FRAME_HEIGHT_PIXELS) as usize];
        let mouth_position = calculate_mouth_position(&body_spine);
        paint_mouth(
            &mut rgba_pixels,
            &body_spine,
            koi_body,
            mouth_position,
            mouth_opening,
        );
        rgba_pixels
    }

    fn center_pixel(rgba_pixels: &[[u8; 4]]) -> [u8; 4] {
        rgba_pixels
            [(FRAME_HEIGHT_PIXELS / 2 * FRAME_WIDTH_PIXELS + FRAME_WIDTH_PIXELS / 2) as usize]
    }

    fn count_painted(rgba_pixels: &[[u8; 4]]) -> usize {
        rgba_pixels
            .iter()
            .filter(|pixel_channels| pixel_channels[3] != 0)
            .count()
    }

    #[test]
    fn the_mouth_opens_as_a_body_colored_rim_around_a_dark_opening() {
        assert_eq!(count_painted(&paint_mouth_alone(KoiBody::White, 0.1)), 0);
        for (koi_body, rim_color) in [
            (KoiBody::White, [192, 203, 220, 255]),
            (KoiBody::Black, [58, 68, 102, 255]),
        ] {
            let open_mouth = paint_mouth_alone(koi_body, 1.0);
            assert_eq!(center_pixel(&open_mouth), OUTLINE_COLOR);
            assert!(open_mouth.contains(&rim_color), "{koi_body:?}");
            assert!(
                open_mouth
                    .iter()
                    .all(|&pixel| pixel == [0; 4] || pixel == OUTLINE_COLOR || pixel == rim_color)
            );
        }
        let half_open = count_painted(&paint_mouth_alone(KoiBody::White, 0.5));
        let wide_open = count_painted(&paint_mouth_alone(KoiBody::White, 1.0));
        assert!(0 < half_open && half_open < wide_open);
    }
}
