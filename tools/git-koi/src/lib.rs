//! Draw a pixel-art koi from a GitHub account's public activity.
//!
//! The pipeline runs one way, and each module owns one stage of it:
//!
//! 1. [`collect`] gathers an [`history::AccountSnapshot`] through the GitHub CLI.
//! 2. [`history`] reads and writes snapshots and derives language shares and weekly
//!    participation from them.
//! 3. [`koi`] turns a snapshot into a [`koi::KoiDevelopment`]: identity, language colors,
//!    growth, streaks, and recent activity.
//! 4. [`behavior`] chooses the day's [`behavior::Behavior`] from that activity, and
//!    [`motion`] gives the pose for each frame of the behavior's loop.
//! 5. [`markings`] measures how strongly the streaks show, and [`coat`] names the bodies
//!    the koi can wear and colors it from its languages.
//! 6. [`render`] draws a [`render::KoiFrame`], still or swimming, and [`output`] encodes
//!    it as PNG, SVG, or a looping GIF, with a preview page.
//!
//! Everything after collection is deterministic: it reads no clock, performs no I/O, and
//! mutates no input. Callers own files and publication. The `git-koi` command composes
//! these stages; the repository's getting-started guide covers the command and the GitHub
//! Action. Breaking changes to this API bump the major version.
#![warn(missing_docs)]

pub mod behavior;
pub mod coat;
pub mod collect;
pub mod history;
pub mod koi;
pub mod markings;
pub mod motion;
pub mod output;
pub mod render;
