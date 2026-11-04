//! Account observations, language shares, and weekly participation, independent of collection.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use chrono::{DateTime, Datelike, IsoWeek, NaiveDate, Utc};
use serde::{Deserialize, Deserializer, Serialize};

/// GitHub observations supplied by the caller, readable and writable through Serde.
///
/// The caller supplies the owned and GitHub-recognized contributed repositories; only
/// public, non-fork ones affect language shares, so a public-only collection is enough.
/// Parsing verifies the format, not collection completeness.
/// Older snapshots remain usable for language shares but have no contribution history
/// or language colors. Language colors require contribution history (version 3).
/// Serialization selects the format from the available observations. Unknown fields,
/// unsupported versions, and missing fields within a supported format are rejected.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "SnapshotDocument", into = "SnapshotDocument")]
pub struct AccountSnapshot {
    /// GitHub's stable numeric ID for the account.
    pub account_id: u64,
    /// Every repository observation the collector supplied, owned or contributed to.
    pub repositories: Vec<RepositoryObservation>,
    /// Daily contributions through an observation date; `None` before snapshot version 2.
    pub contribution_history: Option<ContributionHistory>,
    /// GitHub's color for each observed language name; `None` means GitHub has no color.
    pub language_colors: Option<BTreeMap<String, Option<LanguageColor>>>,
}

/// A public GitHub event used to animate the koi's recent activity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicActivityEvent {
    /// Activity category retained by the profile visualization.
    pub kind: PublicActivityKind,
    /// Event creation time, normalized to UTC.
    pub happened_at: DateTime<Utc>,
    /// Repository's `owner/name` path from the event.
    pub repository: String,
    /// Number of commits in a push event, zero for other event kinds.
    pub commit_count: u32,
    /// Repository primary language, collected for the most active pushed repositories.
    pub primary_language: Option<String>,
    /// GitHub's color for `primary_language`; absent when GitHub has no color.
    pub language_color: Option<LanguageColor>,
}

/// A collected account snapshot plus the public events used by animated behavior.
/// Existing [`AccountSnapshot`] literals remain source compatible for library users.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(try_from = "SnapshotDocument", into = "SnapshotDocument")]
pub struct AccountActivitySnapshot {
    /// Stable account and contribution observations used to draw the koi.
    pub account: AccountSnapshot,
    /// Recent public activity; `None` in snapshots created before event collection.
    pub public_activity: Option<Vec<PublicActivityEvent>>,
}

/// A public GitHub event category with a distinct visual cue.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicActivityKind {
    /// Commits were pushed to a public repository.
    Push,
    /// An issue was created or changed.
    Issue,
    /// A pull request was created or changed.
    PullRequest,
    /// A pull request review was submitted.
    PullRequestReview,
}

/// A GitHub language color, written as `#rrggbb`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LanguageColor(pub [u8; 3]);

impl TryFrom<&str> for LanguageColor {
    type Error = String;

    fn try_from(color_text: &str) -> Result<Self, Self::Error> {
        let hex_digits = color_text
            .strip_prefix('#')
            .filter(|hex_digits| {
                hex_digits.len() == 6 && hex_digits.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
            .ok_or_else(|| format!("language color {color_text} is not #rrggbb"))?;
        let channel = |start: usize| {
            u8::from_str_radix(&hex_digits[start..start + 2], 16).expect("validated hex digits")
        };
        Ok(Self([channel(0), channel(2), channel(4)]))
    }
}

impl Serialize for LanguageColor {
    fn serialize<ColorSerializer: serde::Serializer>(
        &self,
        color_serializer: ColorSerializer,
    ) -> Result<ColorSerializer::Ok, ColorSerializer::Error> {
        let [red, green, blue] = self.0;
        color_serializer.serialize_str(&format!("#{red:02x}{green:02x}{blue:02x}"))
    }
}

impl<'de> Deserialize<'de> for LanguageColor {
    fn deserialize<ColorDeserializer: Deserializer<'de>>(
        color_deserializer: ColorDeserializer,
    ) -> Result<Self, ColorDeserializer::Error> {
        let color_text = String::deserialize(color_deserializer)?;
        Self::try_from(color_text.as_str()).map_err(serde::de::Error::custom)
    }
}

/// Daily observations through an explicit date, without consulting the system clock.
///
/// A nonempty collection must include every date from its first observation through
/// `observed_on`, including zero-activity dates. An empty collection explicitly
/// confirms no observations. The collector owns completeness before the first date.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContributionHistory {
    /// The last day the history covers, inclusive.
    pub observed_on: NaiveDate,
    /// One entry per observed day, complete from the first entry through `observed_on`.
    pub days: Vec<ContributionDay>,
}

/// GitHub's contribution count for one day.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContributionDay {
    /// The day, in GitHub's UTC calendar.
    pub date: NaiveDate,
    /// Contributions GitHub counted on that day.
    pub contribution_count: u32,
}

/// Derived measurements; these are not stored alongside source observations.
#[derive(Debug, Eq, PartialEq)]
pub struct ContributionSummary {
    /// The observation date of the summarized history.
    pub observed_on: NaiveDate,
    /// Every counted contribution, unweighted.
    pub total_contributions: u64,
    /// Chronological Monday-Sunday weeks, including observed weeks with no activity.
    pub weeks: Vec<ContributionWeek>,
    /// One count per date, in order, from the first observation through the
    /// observation date, with identical duplicates merged.
    pub daily_counts: BTreeMap<NaiveDate, u32>,
}

/// Participation in one Monday-Sunday week, including explicitly inactive weeks.
/// `observed_day_count` distinguishes partial calendar coverage from a full week.
/// A week matching the summary's observation week is still in progress.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContributionWeek {
    /// The ISO week the participation falls in.
    pub iso_week: IsoWeek,
    /// Observed days in the week with at least one contribution.
    pub active_day_count: u8,
    /// Days of the week the history covers; seven for a complete week.
    pub observed_day_count: u8,
}

/// Current repository metadata identified by its stable GitHub numeric ID.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryObservation {
    /// GitHub's stable numeric ID for the repository.
    pub repository_id: u64,
    /// Who can see the repository; only public ones count toward language shares.
    pub visibility: RepositoryVisibility,
    /// Whether the repository is a fork, which keeps it out of language shares.
    pub is_fork: bool,
    /// GitHub's language name unchanged; null means a known absence.
    /// Omitting the field is an error because absence of data is not an observation.
    #[serde(deserialize_with = "Option::deserialize")]
    pub primary_language: Option<String>,
}

/// A repository's visibility, as GitHub's API names it.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RepositoryVisibility {
    /// Visible to everyone.
    Public,
    /// Visible to its collaborators.
    Private,
    /// Visible within an enterprise.
    Internal,
}

/// Calculate fractions of eligible projects, keyed by unchanged language names.
///
/// Each distinct public, non-fork repository with a known primary language has
/// one vote. Identical observations count once; conflicting observations for the
/// same ID fail rather than let input order determine the answer. No additional
/// contribution or age rules apply. The input is borrowed without mutation.
///
/// Shares are ordered by language name and range from zero to one. An empty
/// eligible collection returns an empty map. Collection and file access belong
/// to the caller; this operation performs no I/O.
pub fn calculate_language_shares(
    account_snapshot: &AccountSnapshot,
) -> Result<BTreeMap<String, f64>, ConflictingRepository> {
    let mut distinct_repositories = BTreeMap::new();
    for repository_observation in &account_snapshot.repositories {
        if let Some(previous_observation) = distinct_repositories
            .insert(repository_observation.repository_id, repository_observation)
            && previous_observation != repository_observation
        {
            return Err(ConflictingRepository {
                repository_id: repository_observation.repository_id,
            });
        }
    }

    let mut language_repository_counts = BTreeMap::new();
    for repository_observation in distinct_repositories.values() {
        if repository_observation.visibility != RepositoryVisibility::Public
            || repository_observation.is_fork
        {
            continue;
        }
        if let Some(language_name) = &repository_observation.primary_language {
            *language_repository_counts
                .entry(language_name.clone())
                .or_insert(0_usize) += 1;
        }
    }

    let eligible_repository_count: usize = language_repository_counts.values().sum();
    Ok(language_repository_counts
        .into_iter()
        .map(|(language_name, repository_count)| {
            (
                language_name,
                repository_count as f64 / eligible_repository_count as f64,
            )
        })
        .collect())
}

/// Summarize one account's dated contributions into chronological calendar weeks.
///
/// Counts are GitHub-recognized contributions without additional weighting. Each
/// date counts once; identical duplicates and input order do not affect results.
/// Missing history, gaps, conflicting dates, and dates after the observation date
/// fail rather than silently changing participation. Inputs are not mutated.
/// A confirmed empty history produces zero contributions and no observed weeks.
pub fn summarize_contributions(
    account_snapshot: &AccountSnapshot,
) -> Result<ContributionSummary, ContributionHistoryError> {
    let contribution_history = account_snapshot
        .contribution_history
        .as_ref()
        .ok_or(ContributionHistoryError::MissingHistory)?;
    let daily_counts = normalize_contribution_days(contribution_history)?;
    let mut weekly_contributions = BTreeMap::new();
    let mut total_contributions = 0;
    for (&contribution_date, &contribution_count) in &daily_counts {
        total_contributions += u64::from(contribution_count);
        let iso_week = contribution_date.iso_week();
        let contribution_week = weekly_contributions
            .entry(iso_week)
            .or_insert(ContributionWeek {
                iso_week,
                active_day_count: 0,
                observed_day_count: 0,
            });
        contribution_week.active_day_count += u8::from(contribution_count > 0);
        contribution_week.observed_day_count += 1;
    }
    Ok(ContributionSummary {
        observed_on: contribution_history.observed_on,
        total_contributions,
        weeks: weekly_contributions.into_values().collect(),
        daily_counts,
    })
}

/// Two observations disagree about the same repository; no shares were produced.
#[derive(Debug, Eq, PartialEq)]
pub struct ConflictingRepository {
    /// The repository the observations disagree about.
    pub repository_id: u64,
}

impl fmt::Display for ConflictingRepository {
    fn fmt(&self, output_formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            output_formatter,
            "conflicting observations for repository {}; collect a consistent snapshot",
            self.repository_id
        )
    }
}

impl Error for ConflictingRepository {}

/// Why a history could not be summarized. Each message names the repair.
#[derive(Debug, Eq, PartialEq)]
pub enum ContributionHistoryError {
    /// The snapshot has no contribution history.
    MissingHistory,
    /// Two entries for this date carry different counts.
    ConflictingDate(NaiveDate),
    /// An entry is dated after the observation date.
    AfterObservationDate {
        /// The offending entry's date.
        contribution_date: NaiveDate,
        /// The history's observation date.
        observed_on: NaiveDate,
    },
    /// The history skips this date.
    MissingDate(NaiveDate),
}

impl fmt::Display for ContributionHistoryError {
    fn fmt(&self, output_formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingHistory => write!(
                output_formatter,
                "contribution history is unavailable; supply a snapshot with dated contributions"
            ),
            Self::ConflictingDate(contribution_date) => write!(
                output_formatter,
                "conflicting contribution counts for {contribution_date}; collect a consistent history"
            ),
            Self::AfterObservationDate {
                contribution_date,
                observed_on,
            } => write!(
                output_formatter,
                "contribution date {contribution_date} is after observation date {observed_on}; supply observations through the stated date"
            ),
            Self::MissingDate(contribution_date) => write!(
                output_formatter,
                "missing contribution date {contribution_date}; include observations for every date, including zero-activity days"
            ),
        }
    }
}

impl Error for ContributionHistoryError {}

fn normalize_contribution_days(
    contribution_history: &ContributionHistory,
) -> Result<BTreeMap<NaiveDate, u32>, ContributionHistoryError> {
    let mut daily_counts = BTreeMap::new();
    for contribution_day in &contribution_history.days {
        if contribution_day.date > contribution_history.observed_on {
            return Err(ContributionHistoryError::AfterObservationDate {
                contribution_date: contribution_day.date,
                observed_on: contribution_history.observed_on,
            });
        }
        if let Some(previous_count) =
            daily_counts.insert(contribution_day.date, contribution_day.contribution_count)
            && previous_count != contribution_day.contribution_count
        {
            return Err(ContributionHistoryError::ConflictingDate(
                contribution_day.date,
            ));
        }
    }

    let mut next_expected_date = None;
    for contribution_date in daily_counts.keys() {
        if let Some(expected_date) = next_expected_date
            && *contribution_date != expected_date
        {
            return Err(ContributionHistoryError::MissingDate(expected_date));
        }
        next_expected_date = contribution_date.succ_opt();
    }
    if let Some(expected_date) = next_expected_date
        && expected_date <= contribution_history.observed_on
    {
        return Err(ContributionHistoryError::MissingDate(expected_date));
    }
    Ok(daily_counts)
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SnapshotDocument {
    schema_version: SnapshotVersion,
    account_id: u64,
    repositories: Vec<RepositoryObservation>,
    #[serde(
        default,
        deserialize_with = "deserialize_present_history",
        skip_serializing_if = "Option::is_none"
    )]
    contribution_history: Option<ContributionHistory>,
    #[serde(
        default,
        deserialize_with = "deserialize_present_colors",
        skip_serializing_if = "Option::is_none"
    )]
    language_colors: Option<BTreeMap<String, Option<LanguageColor>>>,
    #[serde(
        default,
        deserialize_with = "deserialize_present_activity",
        skip_serializing_if = "Option::is_none"
    )]
    public_activity: Option<Vec<PublicActivityEvent>>,
}

impl TryFrom<SnapshotDocument> for AccountActivitySnapshot {
    type Error = String;

    fn try_from(snapshot_document: SnapshotDocument) -> Result<Self, Self::Error> {
        let has_history = snapshot_document.contribution_history.is_some();
        let has_colors = snapshot_document.language_colors.is_some();
        let has_public_activity = snapshot_document.public_activity.is_some();
        match snapshot_document.schema_version {
            SnapshotVersion::V1 if has_history || has_public_activity => {
                return Err(
                    "contribution_history and public_activity are not fields in snapshot version 1; \
                     use a newer snapshot version"
                        .to_owned(),
                );
            }
            SnapshotVersion::V2
            | SnapshotVersion::V3
            | SnapshotVersion::V4
            | SnapshotVersion::V5
                if !has_history =>
            {
                return Err(
                    "missing contribution_history in snapshot versions 2 through 5; \
                     supply dated observations"
                        .to_owned(),
                );
            }
            SnapshotVersion::V1 | SnapshotVersion::V2 if has_colors => {
                return Err("language_colors is not a field before snapshot version 3; \
                     use version 3 for language colors"
                    .to_owned());
            }
            SnapshotVersion::V3 | SnapshotVersion::V4 | SnapshotVersion::V5 if !has_colors => {
                return Err("missing language_colors in snapshot versions 3 through 5; \
                     collect the snapshot again"
                    .to_owned());
            }
            SnapshotVersion::V1 | SnapshotVersion::V2 | SnapshotVersion::V3
                if has_public_activity =>
            {
                return Err("public_activity requires snapshot version 4".to_owned());
            }
            SnapshotVersion::V4 | SnapshotVersion::V5 if !has_public_activity => {
                return Err(
                    "missing public_activity in snapshot version 4 or 5; collect the snapshot again"
                        .to_owned(),
                );
            }
            _ => {}
        }
        Ok(Self {
            account: AccountSnapshot {
                account_id: snapshot_document.account_id,
                repositories: snapshot_document.repositories,
                contribution_history: snapshot_document.contribution_history,
                language_colors: snapshot_document.language_colors,
            },
            public_activity: snapshot_document.public_activity,
        })
    }
}

impl From<AccountActivitySnapshot> for SnapshotDocument {
    fn from(activity_snapshot: AccountActivitySnapshot) -> Self {
        let schema_version = if activity_snapshot.public_activity.is_some() {
            SnapshotVersion::V5
        } else if activity_snapshot.account.language_colors.is_some() {
            SnapshotVersion::V3
        } else if activity_snapshot.account.contribution_history.is_some() {
            SnapshotVersion::V2
        } else {
            SnapshotVersion::V1
        };
        Self {
            schema_version,
            account_id: activity_snapshot.account.account_id,
            repositories: activity_snapshot.account.repositories,
            contribution_history: activity_snapshot.account.contribution_history,
            language_colors: activity_snapshot.account.language_colors,
            public_activity: activity_snapshot.public_activity,
        }
    }
}

impl TryFrom<SnapshotDocument> for AccountSnapshot {
    type Error = String;

    fn try_from(snapshot_document: SnapshotDocument) -> Result<Self, Self::Error> {
        Ok(AccountActivitySnapshot::try_from(snapshot_document)?.account)
    }
}

impl From<AccountSnapshot> for SnapshotDocument {
    fn from(account_snapshot: AccountSnapshot) -> Self {
        AccountActivitySnapshot {
            account: account_snapshot,
            public_activity: None,
        }
        .into()
    }
}

// An explicit history field must be an object; absence is reserved for older snapshots.
fn deserialize_present_history<'de, HistoryDeserializer>(
    history_deserializer: HistoryDeserializer,
) -> Result<Option<ContributionHistory>, HistoryDeserializer::Error>
where
    HistoryDeserializer: Deserializer<'de>,
{
    ContributionHistory::deserialize(history_deserializer).map(Some)
}

// An explicit colors field must be an object; absence is reserved for older snapshots.
fn deserialize_present_colors<'de, ColorsDeserializer>(
    colors_deserializer: ColorsDeserializer,
) -> Result<Option<BTreeMap<String, Option<LanguageColor>>>, ColorsDeserializer::Error>
where
    ColorsDeserializer: Deserializer<'de>,
{
    BTreeMap::deserialize(colors_deserializer).map(Some)
}

// An explicit activity field must be a list; absence is reserved for older snapshots.
fn deserialize_present_activity<'de, ActivityDeserializer>(
    activity_deserializer: ActivityDeserializer,
) -> Result<Option<Vec<PublicActivityEvent>>, ActivityDeserializer::Error>
where
    ActivityDeserializer: Deserializer<'de>,
{
    Vec::deserialize(activity_deserializer).map(Some)
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(try_from = "u32", into = "u32")]
enum SnapshotVersion {
    V1 = 1,
    V2 = 2,
    V3 = 3,
    V4 = 4,
    V5 = 5,
}

impl TryFrom<u32> for SnapshotVersion {
    type Error = String;

    fn try_from(schema_version: u32) -> Result<Self, Self::Error> {
        if schema_version == Self::V1 as u32 {
            Ok(Self::V1)
        } else if schema_version == Self::V2 as u32 {
            Ok(Self::V2)
        } else if schema_version == Self::V3 as u32 {
            Ok(Self::V3)
        } else if schema_version == Self::V4 as u32 {
            Ok(Self::V4)
        } else if schema_version == Self::V5 as u32 {
            Ok(Self::V5)
        } else {
            Err(format!(
                "unsupported snapshot schema version {schema_version}; use a reader supporting that version"
            ))
        }
    }
}

impl From<SnapshotVersion> for u32 {
    fn from(schema_version: SnapshotVersion) -> Self {
        schema_version as Self
    }
}
