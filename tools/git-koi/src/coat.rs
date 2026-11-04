//! Koi coats colored by language pigments on a chosen body, sampled in body
//! coordinates.
//!
//! Each language's GitHub color becomes the koi pigment nearest in hue: red (beni),
//! orange (orenji), yellow (ki), or blue (ai). Grays and languages without a color
//! become black (sumi). Languages on the same pigment add their shares, and equal
//! shares rank red, black, yellow, orange, then blue. Organic patches seeded by the
//! account give each pigment body area in proportion to its share, pulled partway
//! toward the top pigment's classic variety: red plates, black lightning, a netted blue
//! back, or a netted solid yellow or orange coat. The second pigment adds spots, or
//! red, orange, or yellow sides under a blue back; the third tints the fin roots. Young
//! koi show their pigments pale until [`crate::koi::FULL_COLOR_CONTRIBUTIONS`]: red,
//! orange, yellow, and blue come in first, and black last.
//!
//! The caller chooses one of six [`KoiBody`] values. Each body redraws the pigments in
//! its own palette, and a pigment that would match the ground swaps so it stays
//! visible: black becomes white on black, blue becomes white on Asagi, yellow becomes
//! platinum on gold and gold on platinum, and tea draws every pigment in earthy tones.
//! Black and Asagi also brighten yellow, and black turns blue sky blue. Gold and
//! platinum catch a metallic sheen on the bare body, never on the color patches. A koi
//! without counted languages shows its plain body.
//!
//! The streak marks from [`crate::markings`] show here. The current streak fills a
//! dithered share of the fin rims and tips with the body's streak color: gold on white,
//! black, tea, and platinum, and red on Asagi and gold. The longest streak changes only
//! the language colors, from dull through normal to extra bright and vivid, with black
//! never lightening, and then gin-rin glints build on the color patches.

use std::collections::BTreeMap;
use std::ops::Range;

use crate::history::LanguageColor;
use crate::koi::{FULL_COLOR_CONTRIBUTIONS, KoiDevelopment, calculate_growth_fraction};
use crate::markings::{StreakMarks, calculate_streak_marks};

pub(crate) const WHITE_RAMP: [[u8; 4]; 4] = [
    [90, 105, 136, 255],
    [139, 155, 180, 255],
    [192, 203, 220, 255],
    [255, 255, 255, 255],
];
const RED_RAMP: [[u8; 4]; 4] = [
    [162, 38, 51, 255],
    [228, 59, 68, 255],
    [240, 96, 79, 255],
    [246, 117, 122, 255],
];
const BLACK_RAMP: [[u8; 4]; 3] = [[24, 20, 37, 255], [38, 43, 68, 255], [58, 68, 102, 255]];
const YELLOW_RAMP: [[u8; 4]; 3] = [
    [215, 118, 67, 255],
    [254, 174, 52, 255],
    [254, 231, 97, 255],
];
const ORANGE_RAMP: [[u8; 4]; 3] = [[190, 74, 47, 255], [247, 118, 34, 255], [254, 174, 52, 255]];
const BLUE_RAMP: [[u8; 4]; 3] = [[58, 68, 102, 255], [18, 78, 137, 255], [0, 153, 219, 255]];
const BLUE_NET_RAMP: [[u8; 4]; 1] = [[38, 43, 68, 255]];
const PALE_RED_RAMP: [[u8; 4]; 3] = [
    [194, 133, 105, 255],
    [232, 183, 150, 255],
    [234, 212, 170, 255],
];
const PALE_BLACK_RAMP: [[u8; 4]; 3] = [
    [90, 105, 136, 255],
    [139, 155, 180, 255],
    [192, 203, 220, 255],
];
const PALE_YELLOW_RAMP: [[u8; 4]; 3] = [
    [232, 183, 150, 255],
    [234, 212, 170, 255],
    [255, 255, 255, 255],
];
const PALE_BLUE_RAMP: [[u8; 4]; 3] = [
    [139, 155, 180, 255],
    [192, 203, 220, 255],
    [255, 255, 255, 255],
];
const PALE_NET_RAMP: [[u8; 4]; 1] = [[139, 155, 180, 255]];
const STREAK_GOLD_RAMP: [[u8; 4]; 3] = [
    [247, 118, 34, 255],
    [254, 174, 52, 255],
    [254, 231, 97, 255],
];
const GLINT_COLOR: [u8; 4] = [255, 255, 255, 255];
const ASAGI_RAMP: [[u8; 4]; 4] = [
    [38, 43, 68, 255],
    [58, 68, 102, 255],
    [90, 105, 136, 255],
    [139, 155, 180, 255],
];
const ASAGI_NET_RAMP: [[u8; 4]; 1] = [[58, 68, 102, 255]];
const TEA_RAMP: [[u8; 4]; 4] = [
    [62, 39, 49, 255],
    [115, 62, 57, 255],
    [184, 111, 80, 255],
    [228, 166, 114, 255],
];
const GOLD_RAMP: [[u8; 4]; 5] = [
    [115, 62, 57, 255],
    [215, 118, 67, 255],
    [254, 174, 52, 255],
    [254, 231, 97, 255],
    [255, 255, 255, 255],
];
const PLATINUM_RAMP: [[u8; 4]; 5] = [
    [90, 105, 136, 255],
    [139, 155, 180, 255],
    [192, 203, 220, 255],
    [244, 247, 251, 255],
    [255, 255, 255, 255],
];
const BRIGHT_YELLOW_RAMP: [[u8; 4]; 4] = [
    [215, 118, 67, 255],
    [254, 174, 52, 255],
    [254, 231, 97, 255],
    [255, 243, 176, 255],
];
const SKY_RAMP: [[u8; 4]; 3] = [[18, 78, 137, 255], [0, 153, 219, 255], [44, 232, 245, 255]];
const RUST_RAMP: [[u8; 4]; 3] = [[115, 62, 57, 255], [190, 74, 47, 255], [215, 118, 67, 255]];
const UMBER_RAMP: [[u8; 4]; 3] = [[24, 20, 37, 255], [62, 39, 49, 255], [115, 62, 57, 255]];
const SAND_RAMP: [[u8; 4]; 3] = [
    [184, 111, 80, 255],
    [228, 166, 114, 255],
    [234, 212, 170, 255],
];
const CLAY_RAMP: [[u8; 4]; 3] = [
    [190, 74, 47, 255],
    [215, 118, 67, 255],
    [228, 166, 114, 255],
];
const SLATE_RAMP: [[u8; 4]; 3] = [[38, 43, 68, 255], [58, 68, 102, 255], [90, 105, 136, 255]];
const BRONZE_RAMP: [[u8; 4]; 4] = [
    [62, 39, 49, 255],
    [115, 62, 57, 255],
    [160, 106, 58, 255],
    [201, 154, 90, 255],
];
const METAL_RED_RAMP: [[u8; 4]; 4] = [
    [162, 38, 51, 255],
    [247, 118, 34, 255],
    [254, 174, 52, 255],
    [255, 255, 255, 255],
];
const METAL_ORANGE_RAMP: [[u8; 4]; 4] = [
    [190, 74, 47, 255],
    [247, 118, 34, 255],
    [254, 174, 52, 255],
    [255, 255, 255, 255],
];
const COPPER_RAMP: [[u8; 4]; 4] = [
    [115, 62, 57, 255],
    [184, 111, 80, 255],
    [228, 166, 114, 255],
    [255, 255, 255, 255],
];
const METAL_BLUE_RAMP: [[u8; 4]; 4] = [
    [18, 78, 137, 255],
    [0, 153, 219, 255],
    [139, 155, 180, 255],
    [255, 255, 255, 255],
];
const STEEL_RAMP: [[u8; 4]; 4] = [
    [38, 43, 68, 255],
    [58, 68, 102, 255],
    [90, 105, 136, 255],
    [192, 203, 220, 255],
];

/// Patch scores must beat the white ground's score to place a pigment.
const WHITE_PATCH_SCORE: f64 = 0.5;
/// Share of the patch span that pigments cover together, divided by their shares.
const PATCH_COVERAGE: f64 = 0.58;
const PATCH_SPAN: Range<f64> = 0.02..0.86;
/// Patches move this far from organic shapes toward the top pigment's variety layout.
const VARIETY_PULL: f64 = 0.25;
// The variety layout votes with a hard step, so it carries little weight against the
// smooth patch scores.
const VARIETY_VOTE_WEIGHT: f64 = 0.16 * VARIETY_PULL;
const SPOT_SPAN: Range<f64> = 0.12..0.8;
const SPOT_LATERAL_LIMIT: f64 = 0.8;
/// The full variety layout tints 40% of each fin; the pull shortens the tint with it.
const FIN_ROOT_TINT_FRACTION: f64 = 0.4 * VARIETY_PULL;
/// Streak color fills the fin rims and each fin beyond this fraction of its length.
const STREAK_FIN_START_FRACTION: f64 = 0.55;
const DULL_SATURATION: f64 = 0.3;
const VIVID_SATURATION_GAIN: f64 = 0.3;
const DULL_DARKENING: f64 = 0.15;
const VIVID_BRIGHTENING: f64 = 0.12;

/// The koi's ground color, chosen by the profile owner. Each body redraws the language
/// pigments in its own palette, and a pigment that would match the ground swaps to one
/// that stays visible.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum KoiBody {
    /// Shiro: white.
    #[default]
    White,
    /// Karasu: black.
    Black,
    /// Blue-gray with a scale net, a white head, and orange fins.
    Asagi,
    /// Cha: tea, with earthy colors.
    Tea,
    /// Ogon: metallic gold.
    Gold,
    /// Metallic platinum.
    Platinum,
}

impl KoiBody {
    fn select_ground_ramp(self) -> &'static [[u8; 4]] {
        match self {
            Self::White => &WHITE_RAMP,
            Self::Black => &BLACK_RAMP,
            Self::Asagi => &ASAGI_RAMP,
            Self::Tea => &TEA_RAMP,
            Self::Gold => &GOLD_RAMP,
            Self::Platinum => &PLATINUM_RAMP,
        }
    }

    fn select_fin_ramp(self) -> &'static [[u8; 4]] {
        match self {
            Self::White | Self::Black => &WHITE_RAMP,
            Self::Asagi => &ORANGE_RAMP,
            Self::Tea | Self::Gold | Self::Platinum => self.select_ground_ramp(),
        }
    }

    /// The mouth's rim: a light shade of the body's own color, or on the black body its
    /// lightest shade.
    pub(crate) fn select_mouth_rim_color(self) -> [u8; 4] {
        let ground_ramp = self.select_ground_ramp();
        match self {
            Self::Black => ground_ramp[ground_ramp.len() - 1],
            _ => ground_ramp[ground_ramp.len() - 2],
        }
    }

    /// The current-streak fin color, chosen to stand out against this body's fins.
    fn select_streak_fin_ramp(self) -> &'static [[u8; 4]] {
        match self {
            Self::Asagi | Self::Gold => &RED_RAMP,
            Self::White | Self::Black | Self::Tea | Self::Platinum => &STREAK_GOLD_RAMP,
        }
    }

    /// Gold and platinum: a soft glow along the back and a bright band behind the head.
    fn calculate_metallic_sheen(self, along_fraction: f64, lateral_fraction: f64) -> f64 {
        const BACK_GLOW: f64 = 0.1;
        const BAND_ALONG_FRACTION: f64 = 0.35;
        const BAND_HALF_WIDTH_FRACTION: f64 = 0.06;
        const BAND_BRIGHTNESS: f64 = 0.3;
        if !matches!(self, Self::Gold | Self::Platinum) {
            return 0.0;
        }
        let band_brightness =
            if (along_fraction - BAND_ALONG_FRACTION).abs() < BAND_HALF_WIDTH_FRACTION {
                BAND_BRIGHTNESS
            } else {
                0.0
            };
        BACK_GLOW * (1.0 - lateral_fraction.abs()) + band_brightness
    }

    /// The body's version of a pigment: black turns white on black, blue turns white on
    /// Asagi, yellow turns platinum on gold and gold on platinum, and tea draws every
    /// pigment in earthy tones. Black and Asagi also brighten yellow, and black turns
    /// blue sky blue.
    fn select_pigment_ramp(self, pigment: Pigment) -> &'static [[u8; 4]] {
        match (self, pigment) {
            (Self::Black, Pigment::Sumi) | (Self::Asagi, Pigment::Ai) => &WHITE_RAMP,
            (Self::Black | Self::Asagi, Pigment::Ki) => &BRIGHT_YELLOW_RAMP,
            (Self::Black, Pigment::Ai) => &SKY_RAMP,
            (Self::Tea, Pigment::Beni) => &RUST_RAMP,
            (Self::Tea, Pigment::Sumi) => &UMBER_RAMP,
            (Self::Tea, Pigment::Ki) => &SAND_RAMP,
            (Self::Tea, Pigment::Orenji) => &CLAY_RAMP,
            (Self::Tea, Pigment::Ai) => &SLATE_RAMP,
            (Self::Gold, Pigment::Beni) => &METAL_RED_RAMP,
            (Self::Gold, Pigment::Sumi) => &BRONZE_RAMP,
            (Self::Gold, Pigment::Ki) => &PLATINUM_RAMP,
            (Self::Platinum, Pigment::Beni) => &METAL_ORANGE_RAMP,
            (Self::Platinum, Pigment::Sumi) => &STEEL_RAMP,
            (Self::Platinum, Pigment::Ki) => &GOLD_RAMP,
            (Self::Gold | Self::Platinum, Pigment::Orenji) => &COPPER_RAMP,
            (Self::Gold | Self::Platinum, Pigment::Ai) => &METAL_BLUE_RAMP,
            (_, Pigment::Beni) => &RED_RAMP,
            (_, Pigment::Sumi) => &BLACK_RAMP,
            (_, Pigment::Ki) => &YELLOW_RAMP,
            (_, Pigment::Orenji) => &ORANGE_RAMP,
            (_, Pigment::Ai) => &BLUE_RAMP,
        }
    }
}

/// Koi pigments; each language color becomes the one nearest in hue.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Pigment {
    /// Red.
    Beni,
    /// Black.
    Sumi,
    /// Yellow.
    Ki,
    /// Orange.
    Orenji,
    /// Blue.
    Ai,
}

/// A koi's language pigments laid out on one body shape.
///
/// Languages on the same pigment add their shares. Organic patches give each pigment
/// body area in proportion to its share, pulled toward the top pigment's classic
/// variety layout. The second pigment adds spots, or Asagi sides under blue; the third
/// tints the fin roots. Young koi show pigments pale until FULL_COLOR_CONTRIBUTIONS:
/// red, orange, yellow, and blue come in first, and black last. Streak marks color
/// the fins and change only the language colors.
pub(crate) struct KoiCoat {
    identity_seed: u64,
    koi_body: KoiBody,
    /// Largest share first; equal shares keep pigment order.
    pigment_shares: Vec<(Pigment, f64)>,
    patch_biases: Vec<f64>,
    spot_threshold: f64,
    color_fraction: f64,
    streak_marks: StreakMarks,
}

/// How to color one pixel of the coat.
pub(crate) struct CoatPaint {
    pub(crate) shade_ramp: &'static [[u8; 4]],
    /// Added to the lighting before a shade is chosen.
    pub(crate) brightness_shift: f64,
    /// Scales a shaded color's distance from its own gray; one leaves it unchanged.
    pub(crate) saturation: f64,
    /// Replaces the shaded color, as on a glinting scale.
    pub(crate) glint_color: Option<[u8; 4]>,
}

impl CoatPaint {
    fn plain(shade_ramp: &'static [[u8; 4]]) -> Self {
        Self {
            shade_ramp,
            brightness_shift: 0.0,
            saturation: 1.0,
            glint_color: None,
        }
    }
}

struct BodySample {
    along_fraction: f64,
    lateral_fraction: f64,
    area_weight: f64,
}

impl KoiCoat {
    /// `calculate_half_width` gives the body's half width at an along fraction, so
    /// pigment areas are measured on the fish being drawn. A language without a
    /// recorded color counts as black.
    pub(crate) fn new(
        koi_development: &KoiDevelopment,
        koi_body: KoiBody,
        calculate_half_width: impl Fn(f64) -> f64,
    ) -> Self {
        let identity_seed = koi_development.identity_seed;
        let pigment_shares = calculate_pigment_shares(koi_development);
        let body_samples = sample_body(calculate_half_width);
        Self {
            identity_seed,
            koi_body,
            patch_biases: fit_patch_biases(&pigment_shares, identity_seed, &body_samples),
            spot_threshold: find_spot_threshold(&pigment_shares, identity_seed, &body_samples),
            pigment_shares,
            color_fraction: (calculate_growth_fraction(koi_development.total_contributions)
                / calculate_growth_fraction(FULL_COLOR_CONTRIBUTIONS))
            .min(1.0),
            streak_marks: calculate_streak_marks(koi_development),
        }
    }

    pub(crate) fn select_body_paint(
        &self,
        along_fraction: f64,
        lateral_fraction: f64,
        body_length_pixels: f64,
        half_width_pixels: f64,
        dither_threshold: f64,
    ) -> CoatPaint {
        const ASAGI_HEAD_FRACTION: f64 = 0.11;
        let is_on_net = || {
            is_on_scale_net(
                along_fraction,
                lateral_fraction,
                body_length_pixels,
                half_width_pixels,
            )
        };
        let Some(pigment) = self.select_body_pigment(along_fraction, lateral_fraction) else {
            let shade_ramp: &'static [[u8; 4]] = match self.koi_body {
                KoiBody::Asagi if along_fraction <= ASAGI_HEAD_FRACTION => &WHITE_RAMP,
                KoiBody::Asagi if is_on_net() => &ASAGI_NET_RAMP,
                koi_body => koi_body.select_ground_ramp(),
            };
            return CoatPaint {
                brightness_shift: self
                    .koi_body
                    .calculate_metallic_sheen(along_fraction, lateral_fraction),
                ..CoatPaint::plain(shade_ramp)
            };
        };
        let pigment_ramp = self.koi_body.select_pigment_ramp(pigment);
        let base_pigment = self.pigment_shares.first().map(|&(pigment, _)| pigment);
        let shade_ramp: &'static [[u8; 4]] = match pigment {
            // Only the standard blue carries its dark net.
            Pigment::Ai if pigment_ramp == BLUE_RAMP && is_on_net() => &BLUE_NET_RAMP,
            Pigment::Ki | Pigment::Orenji if base_pigment == Some(pigment) && is_on_net() => {
                &pigment_ramp[..1]
            }
            _ => pigment_ramp,
        };
        let is_glinting = self.is_glinting(
            along_fraction,
            lateral_fraction,
            body_length_pixels,
            half_width_pixels,
        );
        CoatPaint {
            shade_ramp: self.develop_ramp(pigment, shade_ramp, dither_threshold),
            brightness_shift: self.calculate_vibrancy_shift(pigment),
            saturation: self.calculate_vibrancy_saturation(),
            glint_color: is_glinting.then_some(GLINT_COLOR),
        }
    }

    /// The current streak fills a dithered share of the fin rims and tips with the
    /// body's streak color. Elsewhere, a fin-root tint is a language color, unless it
    /// matches the plain fins and so never shows.
    pub(crate) fn select_fin_paint(
        &self,
        along_fraction: f64,
        lateral_fraction: f64,
        is_on_rim: bool,
        dither_threshold: f64,
    ) -> CoatPaint {
        if dither_threshold < self.streak_marks.streak_fin_strength
            && (is_on_rim || along_fraction > STREAK_FIN_START_FRACTION)
        {
            return CoatPaint::plain(self.koi_body.select_streak_fin_ramp());
        }
        let plain_fin_ramp = self.koi_body.select_fin_ramp();
        match self.pigment_shares.get(2) {
            Some(&(tertiary_pigment, _))
                if along_fraction < FIN_ROOT_TINT_FRACTION && lateral_fraction.abs() < 0.85 =>
            {
                let tint_ramp = self.develop_ramp(
                    tertiary_pigment,
                    self.koi_body.select_pigment_ramp(tertiary_pigment),
                    dither_threshold,
                );
                if tint_ramp == plain_fin_ramp {
                    CoatPaint::plain(tint_ramp)
                } else {
                    CoatPaint {
                        saturation: self.calculate_vibrancy_saturation(),
                        ..CoatPaint::plain(tint_ramp)
                    }
                }
            }
            _ => CoatPaint::plain(plain_fin_ramp),
        }
    }

    /// Dull language colors darken and vivid ones brighten, except black, which never
    /// lightens.
    fn calculate_vibrancy_shift(&self, pigment: Pigment) -> f64 {
        let color_vibrancy = self.streak_marks.color_vibrancy;
        if color_vibrancy < 1.0 {
            -DULL_DARKENING * (1.0 - color_vibrancy)
        } else if pigment == Pigment::Sumi {
            0.0
        } else {
            VIVID_BRIGHTENING * (color_vibrancy - 1.0)
        }
    }

    /// Dull language colors pull toward their own gray; vivid ones push away from it.
    fn calculate_vibrancy_saturation(&self) -> f64 {
        let color_vibrancy = self.streak_marks.color_vibrancy;
        if color_vibrancy < 1.0 {
            DULL_SATURATION + (1.0 - DULL_SATURATION) * color_vibrancy
        } else {
            1.0 + VIVID_SATURATION_GAIN * (color_vibrancy - 1.0)
        }
    }

    /// Gin-rin: scattered scales on the color patches glint, more as glimmer builds.
    fn is_glinting(
        &self,
        along_fraction: f64,
        lateral_fraction: f64,
        body_length_pixels: f64,
        half_width_pixels: f64,
    ) -> bool {
        const GLINT_SPAN: Range<f64> = 0.14..0.74;
        const GLINT_LATERAL_LIMIT: f64 = 0.86;
        // Share of scales that glint at full glimmer.
        const FULL_GLINT_SHARE: f64 = 0.36;
        let glimmer_strength = self.streak_marks.glimmer_strength;
        if glimmer_strength <= 0.0
            || !GLINT_SPAN.contains(&along_fraction)
            || lateral_fraction.abs() >= GLINT_LATERAL_LIMIT
        {
            return false;
        }
        let scale_row = (lateral_fraction + 1.0) * half_width_pixels / 2.0;
        let scale_column =
            along_fraction * body_length_pixels / 2.4 + scale_row.floor().rem_euclid(2.0) * 0.5;
        hash_lattice_point(
            self.identity_seed ^ 0x6119,
            scale_column.floor() as i64,
            scale_row.floor() as i64,
        ) < glimmer_strength * FULL_GLINT_SHARE
            && scale_row.fract() < 0.5
            && scale_column.fract() < 0.42
    }

    /// Blend the organic patch scores with the variety layout's vote; the white ground
    /// competes like a pigment.
    fn select_body_pigment(&self, along_fraction: f64, lateral_fraction: f64) -> Option<Pigment> {
        let variety_pigment = self.select_variety_pigment(along_fraction, lateral_fraction);
        let is_in_patch_span = PATCH_SPAN.contains(&along_fraction);
        let mut selected_pigment = None;
        let mut best_score = (1.0 - VARIETY_VOTE_WEIGHT) * WHITE_PATCH_SCORE
            + VARIETY_VOTE_WEIGHT * f64::from(u8::from(variety_pigment.is_none()));
        for (pigment_index, (&(pigment, _), patch_bias)) in self
            .pigment_shares
            .iter()
            .zip(&self.patch_biases)
            .enumerate()
        {
            let patch_score = if is_in_patch_span {
                sample_patch_score(
                    self.identity_seed,
                    pigment_index,
                    along_fraction,
                    lateral_fraction,
                ) + patch_bias
            } else {
                0.0
            };
            let blended_score = (1.0 - VARIETY_VOTE_WEIGHT) * patch_score
                + VARIETY_VOTE_WEIGHT * f64::from(u8::from(variety_pigment == Some(pigment)));
            if blended_score > best_score {
                best_score = blended_score;
                selected_pigment = Some(pigment);
            }
        }
        selected_pigment
    }

    /// The top pigment's classic layout with the second pigment's spots: red plates
    /// (Kohaku), black lightning and patches (Shiro Utsuri), a blue back with red,
    /// orange, or yellow sides (Asagi), or a solid yellow or orange coat.
    fn select_variety_pigment(
        &self,
        along_fraction: f64,
        lateral_fraction: f64,
    ) -> Option<Pigment> {
        let identity_seed = self.identity_seed;
        let &(base_pigment, _) = self.pigment_shares.first()?;
        let secondary_pigment = self.pigment_shares.get(1).map(|&(pigment, _)| pigment);
        let has_asagi_sides = base_pigment == Pigment::Ai
            && matches!(
                secondary_pigment,
                Some(Pigment::Beni | Pigment::Orenji | Pigment::Ki)
            );
        if let Some(secondary_pigment) = secondary_pigment
            && !has_asagi_sides
            && SPOT_SPAN.contains(&along_fraction)
            && lateral_fraction.abs() < SPOT_LATERAL_LIMIT
            && sample_spot_noise(identity_seed, along_fraction, lateral_fraction)
                > self.spot_threshold
        {
            return Some(secondary_pigment);
        }
        match base_pigment {
            Pigment::Beni => is_in_red_plate(identity_seed, along_fraction, lateral_fraction)
                .then_some(Pigment::Beni),
            Pigment::Sumi => {
                let is_in_lightning = along_fraction < 0.13
                    && (lateral_fraction - 0.25 * (along_fraction * 55.0).sin()).abs() < 0.14;
                (is_in_lightning
                    || sample_fractal_noise(
                        identity_seed,
                        along_fraction * 4.0 + 60.0,
                        lateral_fraction * 1.2,
                    ) > 0.5)
                    .then_some(Pigment::Sumi)
            }
            Pigment::Ai if along_fraction < 0.11 => None,
            Pigment::Ai if lateral_fraction.abs() > 0.64 => {
                secondary_pigment.filter(|_| has_asagi_sides)
            }
            Pigment::Ai | Pigment::Ki | Pigment::Orenji => Some(base_pigment),
        }
    }

    /// Swap in the pale ramp on the dithered share of pixels whose pigment has not
    /// yet come in.
    fn develop_ramp(
        &self,
        pigment: Pigment,
        shade_ramp: &'static [[u8; 4]],
        dither_threshold: f64,
    ) -> &'static [[u8; 4]] {
        if dither_threshold <= calculate_pigment_development(pigment, self.color_fraction) {
            return shade_ramp;
        }
        if shade_ramp.len() == 1 {
            return &PALE_NET_RAMP;
        }
        match pigment {
            Pigment::Beni | Pigment::Orenji => &PALE_RED_RAMP,
            Pigment::Sumi => &PALE_BLACK_RAMP,
            Pigment::Ki => &PALE_YELLOW_RAMP,
            Pigment::Ai => &PALE_BLUE_RAMP,
        }
    }
}

/// Sum each pigment's language shares, largest first; equal shares keep pigment order.
fn calculate_pigment_shares(koi_development: &KoiDevelopment) -> Vec<(Pigment, f64)> {
    let total_share: f64 = koi_development.language_shares.values().sum();
    let mut summed_shares = BTreeMap::new();
    for (language_name, language_share) in &koi_development.language_shares {
        let language_color = koi_development
            .language_colors
            .get(language_name)
            .copied()
            .flatten();
        *summed_shares
            .entry(select_pigment(language_color))
            .or_insert(0.0) += language_share / total_share;
    }
    // Sums of equal shares can differ in their last bits, so order by rounded shares.
    let rank_share = |pigment_share: f64| (pigment_share * 1e9).round();
    let mut pigment_shares: Vec<_> = summed_shares.into_iter().collect();
    pigment_shares.sort_by(|left, right| {
        rank_share(right.1)
            .total_cmp(&rank_share(left.1))
            .then(left.0.cmp(&right.0))
    });
    pigment_shares
}

/// Map a GitHub language color to the koi pigment nearest in hue. Grays and missing
/// colors become black.
fn select_pigment(language_color: Option<LanguageColor>) -> Pigment {
    const MIN_CHROMA: f64 = 0.12;
    const MIN_SATURATION: f64 = 0.2;
    let Some(LanguageColor(color_channels)) = language_color else {
        return Pigment::Sumi;
    };
    let [red, green, blue] = color_channels.map(|color_channel| f64::from(color_channel) / 255.0);
    let max_channel = red.max(green).max(blue);
    let min_channel = red.min(green).min(blue);
    let chroma = max_channel - min_channel;
    let lightness = (max_channel + min_channel) / 2.0;
    if chroma < MIN_CHROMA || chroma / (1.0 - (2.0 * lightness - 1.0).abs()) < MIN_SATURATION {
        return Pigment::Sumi;
    }
    let hue_degrees = if max_channel == red {
        60.0 * ((green - blue) / chroma).rem_euclid(6.0)
    } else if max_channel == green {
        60.0 * ((blue - red) / chroma + 2.0)
    } else {
        60.0 * ((red - green) / chroma + 4.0)
    };
    match hue_degrees {
        hue_degrees if !(18.0..330.0).contains(&hue_degrees) => Pigment::Beni,
        hue_degrees if hue_degrees < 45.0 => Pigment::Orenji,
        hue_degrees if hue_degrees < 160.0 => Pigment::Ki,
        _ => Pigment::Ai,
    }
}

/// How far a pigment has come in, from pale (0) to full color (1).
fn calculate_pigment_development(pigment: Pigment, color_fraction: f64) -> f64 {
    let (start_fraction, span_fraction) = match pigment {
        Pigment::Sumi => (0.35, 0.6),
        _ => (0.05, 0.55),
    };
    let fraction = ((color_fraction - start_fraction) / span_fraction).clamp(0.0, 1.0);
    fraction * fraction * (3.0 - 2.0 * fraction)
}

/// A grid over the patch span, weighted by half width so each sample stands for its
/// share of body area.
fn sample_body(calculate_half_width: impl Fn(f64) -> f64) -> Vec<BodySample> {
    const ALONG_STEPS: usize = 170;
    const ALONG_STEP_FRACTION: f64 = 0.005;
    const LATERAL_STEPS: usize = 41;
    const LATERAL_STEP_FRACTION: f64 = 0.05;
    let mut body_samples = Vec::with_capacity(ALONG_STEPS * LATERAL_STEPS);
    for along_step in 0..ALONG_STEPS {
        let along_fraction = PATCH_SPAN.start + along_step as f64 * ALONG_STEP_FRACTION;
        let area_weight = calculate_half_width(along_fraction);
        for lateral_step in 0..LATERAL_STEPS {
            body_samples.push(BodySample {
                along_fraction,
                lateral_fraction: -1.0 + lateral_step as f64 * LATERAL_STEP_FRACTION,
                area_weight,
            });
        }
    }
    body_samples
}

/// Raise or lower each pigment's patch scores until its area matches its share.
fn fit_patch_biases(
    pigment_shares: &[(Pigment, f64)],
    identity_seed: u64,
    body_samples: &[BodySample],
) -> Vec<f64> {
    const FIT_PASSES: usize = 120;
    const FIT_RATE: f64 = 0.6;
    let patch_scores: Vec<Vec<f64>> = body_samples
        .iter()
        .map(|body_sample| {
            (0..pigment_shares.len())
                .map(|pigment_index| {
                    sample_patch_score(
                        identity_seed,
                        pigment_index,
                        body_sample.along_fraction,
                        body_sample.lateral_fraction,
                    )
                })
                .collect()
        })
        .collect();
    let total_weight: f64 = body_samples
        .iter()
        .map(|body_sample| body_sample.area_weight)
        .sum();
    let mut patch_biases = vec![0.0; pigment_shares.len()];
    for _ in 0..FIT_PASSES {
        let mut covered_fractions = vec![0.0; pigment_shares.len()];
        for (body_sample, sample_scores) in body_samples.iter().zip(&patch_scores) {
            let mut covering_index = None;
            let mut best_score = WHITE_PATCH_SCORE;
            for (pigment_index, (patch_score, patch_bias)) in
                sample_scores.iter().zip(&patch_biases).enumerate()
            {
                if patch_score + patch_bias > best_score {
                    best_score = patch_score + patch_bias;
                    covering_index = Some(pigment_index);
                }
            }
            if let Some(pigment_index) = covering_index {
                covered_fractions[pigment_index] += body_sample.area_weight / total_weight;
            }
        }
        for ((patch_bias, &(_, pigment_share)), covered_fraction) in patch_biases
            .iter_mut()
            .zip(pigment_shares)
            .zip(covered_fractions)
        {
            *patch_bias += FIT_RATE * (PATCH_COVERAGE * pigment_share - covered_fraction);
        }
    }
    patch_biases
}

/// The spot noise level above which the second pigment's spots cover their target
/// share of the spot span; no second pigment means no spots.
fn find_spot_threshold(
    pigment_shares: &[(Pigment, f64)],
    identity_seed: u64,
    body_samples: &[BodySample],
) -> f64 {
    const SPOTS_PER_SHARE: f64 = 0.75;
    const MAX_SPOT_COVERAGE: f64 = 0.32;
    let Some(&(_, secondary_share)) = pigment_shares.get(1) else {
        return f64::INFINITY;
    };
    let spot_coverage = (secondary_share * SPOTS_PER_SHARE).min(MAX_SPOT_COVERAGE);
    let mut spot_noises: Vec<f64> = body_samples
        .iter()
        .filter(|body_sample| SPOT_SPAN.contains(&body_sample.along_fraction))
        .map(|body_sample| {
            sample_spot_noise(
                identity_seed,
                body_sample.along_fraction,
                body_sample.lateral_fraction,
            )
        })
        .collect();
    spot_noises.sort_by(f64::total_cmp);
    spot_noises[((1.0 - spot_coverage) * (spot_noises.len() - 1) as f64) as usize]
}

fn sample_patch_score(
    identity_seed: u64,
    pigment_index: usize,
    along_fraction: f64,
    lateral_fraction: f64,
) -> f64 {
    let pigment_seed = identity_seed ^ (pigment_index as u64 + 1).wrapping_mul(0x9e3779b97f4a7c15);
    sample_fractal_noise(
        pigment_seed,
        along_fraction * 3.0 + pigment_index as f64 * 13.0,
        lateral_fraction * 0.95 + 7.0,
    )
}

fn sample_spot_noise(identity_seed: u64, along_fraction: f64, lateral_fraction: f64) -> f64 {
    sample_fractal_noise(
        identity_seed ^ 0x5b07,
        along_fraction * 9.0 + 30.0,
        lateral_fraction * 2.6,
    )
}

/// Kohaku's three red plates from head to tail, each with a seeded lean, width, and
/// edge.
fn is_in_red_plate(identity_seed: u64, along_fraction: f64, lateral_fraction: f64) -> bool {
    const PLATE_SHARES: [f64; 3] = [0.42, 0.33, 0.25];
    const PLATES_START_FRACTION: f64 = 0.035;
    const PLATES_END_FRACTION: f64 = 0.8;
    const PLATE_GAP_FRACTION: f64 = 0.05;
    let plated_fraction = PLATES_END_FRACTION
        - PLATES_START_FRACTION
        - PLATE_GAP_FRACTION * (PLATE_SHARES.len() - 1) as f64;
    let mut plate_start_fraction = PLATES_START_FRACTION;
    for (plate_index, plate_share) in PLATE_SHARES.into_iter().enumerate() {
        let plate_length_fraction = plated_fraction * plate_share;
        let plate_center_fraction = plate_start_fraction + plate_length_fraction / 2.0;
        plate_start_fraction += plate_length_fraction + PLATE_GAP_FRACTION;
        let lattice_index = plate_index as i64;
        let plate_lean = (hash_lattice_point(identity_seed, lattice_index, 1) - 0.5) * 0.55;
        let plate_half_width = 0.62 + hash_lattice_point(identity_seed, lattice_index, 2) * 0.24;
        let edge_wobble = (sample_fractal_noise(
            identity_seed ^ 0xa11ce,
            along_fraction * 16.0 + plate_index as f64 * 31.0,
            lateral_fraction * 2.4,
        ) - 0.5)
            * 0.9;
        let plate_distance = ((along_fraction - plate_center_fraction)
            / (plate_length_fraction / 2.0))
            .abs()
            .powi(4)
            + ((lateral_fraction - plate_lean) / plate_half_width)
                .abs()
                .powi(4);
        if plate_distance < 1.0 + edge_wobble {
            return true;
        }
    }
    false
}

/// Offset rows of scale outlines for blue backs and solid yellow or orange coats.
fn is_on_scale_net(
    along_fraction: f64,
    lateral_fraction: f64,
    body_length_pixels: f64,
    half_width_pixels: f64,
) -> bool {
    let scale_row = (lateral_fraction + 1.0) * half_width_pixels / 2.4;
    let scale_column =
        along_fraction * body_length_pixels / 2.8 + scale_row.floor().rem_euclid(2.0) * 0.5;
    scale_row.fract().min(scale_column.fract()) < 0.17
}

fn sample_fractal_noise(identity_seed: u64, coordinate_x: f64, coordinate_y: f64) -> f64 {
    let mut noise_sum = 0.0;
    let mut total_weight = 0.0;
    for octave_index in 0..3 {
        let frequency = 2.0_f64.powi(octave_index);
        let noise_weight = 1.0 / frequency;
        noise_sum += sample_smooth_noise(
            identity_seed,
            coordinate_x * frequency,
            coordinate_y * frequency,
        ) * noise_weight;
        total_weight += noise_weight;
    }
    noise_sum / total_weight
}

fn sample_smooth_noise(identity_seed: u64, coordinate_x: f64, coordinate_y: f64) -> f64 {
    let lattice_x = coordinate_x.floor() as i64;
    let lattice_y = coordinate_y.floor() as i64;
    let fraction_x = coordinate_x - coordinate_x.floor();
    let fraction_y = coordinate_y - coordinate_y.floor();
    let blend_x = fraction_x * fraction_x * (3.0 - 2.0 * fraction_x);
    let blend_y = fraction_y * fraction_y * (3.0 - 2.0 * fraction_y);
    let top_left = hash_lattice_point(identity_seed, lattice_x, lattice_y);
    let top_right = hash_lattice_point(identity_seed, lattice_x + 1, lattice_y);
    let bottom_left = hash_lattice_point(identity_seed, lattice_x, lattice_y + 1);
    let bottom_right = hash_lattice_point(identity_seed, lattice_x + 1, lattice_y + 1);
    let top_noise = top_left + (top_right - top_left) * blend_x;
    let bottom_noise = bottom_left + (bottom_right - bottom_left) * blend_x;
    top_noise + (bottom_noise - top_noise) * blend_y
}

fn hash_lattice_point(identity_seed: u64, lattice_x: i64, lattice_y: i64) -> f64 {
    let mut hash_bits = identity_seed
        ^ (lattice_x as u64).wrapping_mul(0x9e3779b97f4a7c15)
        ^ (lattice_y as u64).wrapping_mul(0xbf58476d1ce4e5b9);
    hash_bits = (hash_bits ^ (hash_bits >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    hash_bits = (hash_bits ^ (hash_bits >> 27)).wrapping_mul(0x94d049bb133111eb);
    hash_bits ^= hash_bits >> 31;
    (hash_bits >> 11) as f64 / (1_u64 << 53) as f64
}
