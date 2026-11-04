//! Account snapshots collected from GitHub through the GitHub CLI, separate from
//! interpretation. Only public data is read, so a workflow's built-in token is enough.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::io::ErrorKind;
use std::process::Command;

use chrono::{DateTime, Days, Months, NaiveDate, Utc};
use serde_json::Value;

use crate::history::{
    AccountSnapshot, ConflictingRepository, ContributionDay, ContributionHistory,
    ContributionHistoryError, LanguageColor, PublicActivityEvent, PublicActivityKind,
    RepositoryObservation, RepositoryVisibility, calculate_language_shares,
    summarize_contributions,
};

/// Runs one GitHub GraphQL query with string variables and returns its `data` object.
pub trait GraphqlClient {
    /// Run `query` with the given string variables and return the response's `data`
    /// object. Errors carry the reason `gh` or GitHub gave.
    fn run_query(&self, query: &str, variables: &[(&str, &str)]) -> Result<Value, CollectionError>;
}

/// Collect public push, issue, pull request, and review events from the last 30 days.
/// GitHub's event endpoint is bounded to 300 recent entries and 30 days.
pub fn collect_public_activity(
    login: &str,
    today: NaiveDate,
) -> Result<Vec<PublicActivityEvent>, CollectionError> {
    let endpoint = format!("users/{login}/events/public?per_page=100");
    let gh_output = Command::new("gh")
        .args(["api", "--paginate", "--slurp"])
        .arg(endpoint)
        .output()
        .map_err(|spawn_error| {
            if spawn_error.kind() == ErrorKind::NotFound {
                CollectionError::GhNotInstalled
            } else {
                CollectionError::GhFailed(spawn_error.to_string())
            }
        })?;
    if !gh_output.status.success() {
        return Err(CollectionError::GhFailed(
            String::from_utf8_lossy(&gh_output.stderr).trim().to_owned(),
        ));
    }
    let response: Value = serde_json::from_slice(&gh_output.stdout).map_err(|parse_error| {
        CollectionError::UnexpectedResponse(format!(
            "gh public events output is not JSON: {parse_error}"
        ))
    })?;
    let pages = response.as_array().ok_or_else(|| {
        CollectionError::UnexpectedResponse("public events response is not a list".to_owned())
    })?;
    let first_day = today
        .checked_sub_days(Days::new(29))
        .expect("event window stays within supported dates");
    let mut public_activity = Vec::new();
    for page in pages {
        let events = page.as_array().ok_or_else(|| {
            CollectionError::UnexpectedResponse("public events page is not a list".to_owned())
        })?;
        for event in events {
            let kind = match read_str(event, "type")? {
                "PushEvent" => PublicActivityKind::Push,
                "IssuesEvent" => PublicActivityKind::Issue,
                "PullRequestEvent" => PublicActivityKind::PullRequest,
                "PullRequestReviewEvent" => PublicActivityKind::PullRequestReview,
                _ => continue,
            };
            let happened_at = DateTime::parse_from_rfc3339(read_str(event, "created_at")?)
                .map_err(|parse_error| {
                    CollectionError::UnexpectedResponse(format!(
                        "public event timestamp is invalid: {parse_error}"
                    ))
                })?
                .with_timezone(&Utc);
            if happened_at.date_naive() < first_day || happened_at.date_naive() > today {
                continue;
            }
            let repository = read_str(read_field(event, "repo")?, "name")?.to_owned();
            let commit_count = if kind == PublicActivityKind::Push {
                let payload = read_field(event, "payload")?;
                let count = payload
                    .get("size")
                    .and_then(Value::as_u64)
                    .or_else(|| {
                        payload
                            .get("commits")
                            .and_then(Value::as_array)
                            .map(|commits| commits.len() as u64)
                    })
                    .unwrap_or(1);
                u32::try_from(count).map_err(|_| {
                    CollectionError::UnexpectedResponse(format!(
                        "push commit count for {repository} is out of range"
                    ))
                })?
            } else {
                0
            };
            public_activity.push(PublicActivityEvent {
                kind,
                happened_at,
                repository,
                commit_count,
                primary_language: None,
                language_color: None,
            });
        }
    }
    Ok(public_activity)
}

/// Add GitHub's primary language and color to the five most frequently pushed repos.
/// One GraphQL request covers the whole trail rather than one request per repository.
pub fn collect_repository_languages(
    github: &impl GraphqlClient,
    public_activity: &mut [PublicActivityEvent],
) -> Result<(), CollectionError> {
    let mut push_counts = BTreeMap::<String, u32>::new();
    for event in public_activity.iter() {
        if event.kind == PublicActivityKind::Push {
            *push_counts.entry(event.repository.clone()).or_default() += 1;
        }
    }
    let mut repositories: Vec<_> = push_counts.into_iter().collect();
    repositories.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    repositories.truncate(5);
    if repositories.is_empty() {
        return Ok(());
    }

    let mut query = String::from("query(");
    let mut variables = Vec::with_capacity(repositories.len() * 2);
    for (index, (repository, _)) in repositories.iter().enumerate() {
        let (owner, name) = repository.split_once('/').ok_or_else(|| {
            CollectionError::UnexpectedResponse(format!(
                "public repository name {repository} has no owner"
            ))
        })?;
        if index > 0 {
            query.push_str(", ");
        }
        query.push_str(&format!("$owner{index}: String!, $name{index}: String!"));
        variables.push((format!("owner{index}"), owner.to_owned()));
        variables.push((format!("name{index}"), name.to_owned()));
    }
    query.push_str(") {");
    for (index, _) in repositories.iter().enumerate() {
        query.push_str(&format!(
            " repo{index}: repository(owner: $owner{index}, name: $name{index}) {{ primaryLanguage {{ name color }} }}"
        ));
    }
    query.push_str(" }");
    let variable_refs: Vec<_> = variables
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    let response = github.run_query(&query, &variable_refs)?;

    for (index, (repository, _)) in repositories.iter().enumerate() {
        let Some(repository_data) = response
            .get(format!("repo{index}"))
            .filter(|repository_data| !repository_data.is_null())
        else {
            continue;
        };
        let language = repository_data
            .get("primaryLanguage")
            .filter(|value| !value.is_null());
        let Some(language) = language else {
            continue;
        };
        let language_name = read_str(language, "name")?.to_owned();
        let language_color = match language.get("color") {
            Some(Value::String(color)) => Some(
                LanguageColor::try_from(color.as_str())
                    .map_err(CollectionError::UnexpectedResponse)?,
            ),
            Some(Value::Null) | None => None,
            Some(_) => {
                return Err(CollectionError::UnexpectedResponse(format!(
                    "{language_name} color is not text"
                )));
            }
        };
        for event in public_activity.iter_mut().filter(|event| {
            event.kind == PublicActivityKind::Push && event.repository == *repository
        }) {
            event.primary_language = Some(language_name.clone());
            event.language_color = language_color;
        }
    }
    Ok(())
}

/// Runs queries through `gh api graphql` with whatever authentication `gh` has,
/// including `GH_TOKEN` in GitHub Actions.
pub struct GhCli;

impl GraphqlClient for GhCli {
    fn run_query(&self, query: &str, variables: &[(&str, &str)]) -> Result<Value, CollectionError> {
        let mut gh_command = Command::new("gh");
        gh_command
            .args(["api", "graphql", "-f"])
            .arg(format!("query={query}"));
        for (variable_name, variable_value) in variables {
            // -f sends the value as a string; -F would turn a numeric login into a number.
            gh_command
                .arg("-f")
                .arg(format!("{variable_name}={variable_value}"));
        }
        let gh_output = gh_command.output().map_err(|spawn_error| {
            if spawn_error.kind() == ErrorKind::NotFound {
                CollectionError::GhNotInstalled
            } else {
                CollectionError::GhFailed(spawn_error.to_string())
            }
        })?;
        if !gh_output.status.success() {
            return Err(CollectionError::GhFailed(
                String::from_utf8_lossy(&gh_output.stderr).trim().to_owned(),
            ));
        }
        let response: Value = serde_json::from_slice(&gh_output.stdout).map_err(|parse_error| {
            CollectionError::UnexpectedResponse(format!("gh output is not JSON: {parse_error}"))
        })?;
        Ok(read_field(&response, "data")?.clone())
    }
}

const ACCOUNT_QUERY: &str =
    "query($login: String!) { user(login: $login) { databaseId createdAt } }";
const REPOSITORY_FIELDS: &str = "nodes { databaseId visibility isFork primaryLanguage { name color } } pageInfo { hasNextPage endCursor }";
const CALENDAR_QUERY: &str = "query($login: String!, $from: DateTime!, $to: DateTime!) { \
    user(login: $login) { contributionsCollection(from: $from, to: $to) { \
    contributionCalendar { weeks { contributionDays { date contributionCount } } } } } }";

/// Collect one account's public snapshot: its public owned repositories, the public
/// repositories GitHub counts it as contributing to, GitHub's color for each of their
/// languages, and daily contribution counts from the day it was created through the
/// latest date GitHub reports.
///
/// GitHub limits each calendar request to a year, so the calendar is requested a year at
/// a time, and any days GitHub leaves out of an answer are requested again.
/// `today` is the current UTC date and its day is still in progress, so the
/// requests end the day before and a run just after midnight observes the day that just
/// ended. Within that bound the observation date comes from GitHub. An account created
/// today has no complete day yet and gets an empty history observed through yesterday.
/// The snapshot is checked against the history rules before it is returned, so a gap,
/// a conflict, or a calendar that starts after the account was created fails instead of
/// producing a partial snapshot.
pub fn collect_account_snapshot(
    github: &impl GraphqlClient,
    login: &str,
    today: NaiveDate,
) -> Result<AccountSnapshot, CollectionError> {
    let account_data = github.run_query(ACCOUNT_QUERY, &[("login", login)])?;
    let user = account_data
        .get("user")
        .filter(|user| !user.is_null())
        .ok_or_else(|| CollectionError::AccountNotFound(login.to_owned()))?;
    let account_id = read_u64(user, "databaseId")?;
    let created_on = DateTime::parse_from_rfc3339(read_str(user, "createdAt")?)
        .map_err(|parse_error| {
            CollectionError::UnexpectedResponse(format!(
                "createdAt is not a timestamp: {parse_error}"
            ))
        })?
        .date_naive();
    let mut language_colors = BTreeMap::new();
    let mut repositories = collect_repositories(
        github,
        login,
        &mut language_colors,
        "repositories",
        "repositories(first: 100, after: $cursor, ownerAffiliations: [OWNER], privacy: PUBLIC)",
    )?;
    repositories.extend(collect_repositories(
        github,
        login,
        &mut language_colors,
        "repositoriesContributedTo",
        "repositoriesContributedTo(first: 100, after: $cursor, privacy: PUBLIC, \
         includeUserRepositories: false, \
         contributionTypes: [COMMIT, ISSUE, PULL_REQUEST, PULL_REQUEST_REVIEW, REPOSITORY])",
    )?);
    let account_snapshot = AccountSnapshot {
        account_id,
        repositories,
        contribution_history: Some(collect_contribution_history(
            github, login, created_on, today,
        )?),
        language_colors: Some(language_colors),
    };
    summarize_contributions(&account_snapshot).map_err(CollectionError::InvalidHistory)?;
    calculate_language_shares(&account_snapshot).map_err(CollectionError::ConflictingRepository)?;
    Ok(account_snapshot)
}

/// Follow every page of one repository connection on the user, recording each primary
/// language's color as it goes.
fn collect_repositories(
    github: &impl GraphqlClient,
    login: &str,
    language_colors: &mut BTreeMap<String, Option<LanguageColor>>,
    connection_name: &str,
    connection_call: &str,
) -> Result<Vec<RepositoryObservation>, CollectionError> {
    let query = format!(
        "query($login: String!, $cursor: String) {{ user(login: $login) {{ {connection_call} {{ {REPOSITORY_FIELDS} }} }} }}"
    );
    let mut repositories = Vec::new();
    let mut page_cursor: Option<String> = None;
    loop {
        let mut variables = vec![("login", login)];
        if let Some(page_cursor) = &page_cursor {
            variables.push(("cursor", page_cursor.as_str()));
        }
        let page_data = github.run_query(&query, &variables)?;
        let connection = read_field(read_field(&page_data, "user")?, connection_name)?;
        for repository_node in read_array(connection, "nodes")? {
            repositories.push(read_repository(repository_node, language_colors)?);
        }
        let page_info = read_field(connection, "pageInfo")?;
        if !read_bool(page_info, "hasNextPage")? {
            return Ok(repositories);
        }
        let next_cursor = read_str(page_info, "endCursor")?.to_owned();
        if page_cursor.as_deref() == Some(next_cursor.as_str()) {
            return Err(CollectionError::UnexpectedResponse(format!(
                "{connection_name} repeated page cursor {next_cursor}"
            )));
        }
        page_cursor = Some(next_cursor);
    }
}

fn read_repository(
    repository_node: &Value,
    language_colors: &mut BTreeMap<String, Option<LanguageColor>>,
) -> Result<RepositoryObservation, CollectionError> {
    let visibility = match read_str(repository_node, "visibility")? {
        "PUBLIC" => RepositoryVisibility::Public,
        "PRIVATE" => RepositoryVisibility::Private,
        "INTERNAL" => RepositoryVisibility::Internal,
        other => {
            return Err(CollectionError::UnexpectedResponse(format!(
                "unknown repository visibility {other}"
            )));
        }
    };
    let primary_language = match read_field(repository_node, "primaryLanguage")? {
        Value::Null => None,
        language => {
            let language_name = read_str(language, "name")?.to_owned();
            let language_color = match read_field(language, "color")? {
                Value::Null => None,
                color => Some(
                    LanguageColor::try_from(color.as_str().ok_or_else(|| {
                        CollectionError::UnexpectedResponse(format!(
                            "{language_name} color is not text"
                        ))
                    })?)
                    .map_err(CollectionError::UnexpectedResponse)?,
                ),
            };
            if let Some(recorded_color) =
                language_colors.insert(language_name.clone(), language_color)
                && recorded_color != language_color
            {
                return Err(CollectionError::ConflictingLanguageColor(language_name));
            }
            Some(language_name)
        }
    };
    Ok(RepositoryObservation {
        repository_id: read_u64(repository_node, "databaseId")?,
        visibility,
        is_fork: read_bool(repository_node, "isFork")?,
        primary_language,
    })
}

/// Collect the calendar in year-long windows from the account's creation date through the
/// last complete day.
fn collect_contribution_history(
    github: &impl GraphqlClient,
    login: &str,
    created_on: NaiveDate,
    today: NaiveDate,
) -> Result<ContributionHistory, CollectionError> {
    let last_complete_day = today
        .pred_opt()
        .expect("calendar windows stay within supported dates");
    if created_on > last_complete_day {
        return Ok(ContributionHistory {
            observed_on: last_complete_day,
            days: Vec::new(),
        });
    }
    let mut daily_counts = BTreeMap::new();
    let mut window_start = created_on;
    while window_start <= last_complete_day {
        let window_end = window_start
            .checked_add_months(Months::new(12))
            .and_then(|next_year| next_year.pred_opt())
            .expect("calendar windows stay within supported dates")
            .min(last_complete_day);
        daily_counts.extend(collect_calendar_window(
            github,
            login,
            window_start,
            window_end,
        )?);
        window_start = window_end
            .checked_add_days(Days::new(1))
            .expect("calendar windows stay within supported dates");
    }
    let first_reported_on = daily_counts.keys().next().copied();
    let observed_on = match (first_reported_on, daily_counts.keys().next_back()) {
        (Some(first_date), Some(&last_date)) if first_date == created_on => last_date,
        _ => {
            return Err(CollectionError::IncompleteCalendar {
                created_on,
                first_reported_on,
            });
        }
    };
    Ok(ContributionHistory {
        observed_on,
        days: daily_counts
            .into_iter()
            .map(|(date, contribution_count)| ContributionDay {
                date,
                contribution_count,
            })
            .collect(),
    })
}

/// Collect one window of the calendar. GitHub can answer a request with fewer days than it
/// covers, leaving out the earliest, so while the window's first day is missing the days
/// before the earliest one reported are requested again. Each request ends before the
/// previous one, so this stops. A request that returns no days stops it with the window
/// incomplete, and the checks on the whole history report the missing days.
fn collect_calendar_window(
    github: &impl GraphqlClient,
    login: &str,
    window_start: NaiveDate,
    window_end: NaiveDate,
) -> Result<BTreeMap<NaiveDate, u32>, CollectionError> {
    let mut window_daily_counts = BTreeMap::new();
    let mut request_end = window_end;
    loop {
        let request_daily_counts = request_calendar_days(github, login, window_start, request_end)?;
        let Some(&earliest_reported_on) = request_daily_counts.keys().next() else {
            return Ok(window_daily_counts);
        };
        window_daily_counts.extend(request_daily_counts);
        if earliest_reported_on == window_start {
            return Ok(window_daily_counts);
        }
        request_end = earliest_reported_on
            .pred_opt()
            .expect("calendar windows stay within supported dates");
    }
}

/// Make one calendar request and return the daily counts GitHub reported from
/// `request_start` through `request_end`. GitHub answers in whole weeks, so days outside
/// that range are dropped. A date reported twice with different counts fails.
fn request_calendar_days(
    github: &impl GraphqlClient,
    login: &str,
    request_start: NaiveDate,
    request_end: NaiveDate,
) -> Result<BTreeMap<NaiveDate, u32>, CollectionError> {
    let from = format!("{request_start}T00:00:00Z");
    let to = format!("{request_end}T23:59:59Z");
    let calendar_data = github.run_query(
        CALENDAR_QUERY,
        &[("login", login), ("from", &from), ("to", &to)],
    )?;
    let calendar = read_field(
        read_field(
            read_field(&calendar_data, "user")?,
            "contributionsCollection",
        )?,
        "contributionCalendar",
    )?;
    let mut daily_counts = BTreeMap::new();
    for calendar_week in read_array(calendar, "weeks")? {
        for calendar_day in read_array(calendar_week, "contributionDays")? {
            let contribution_date =
                NaiveDate::parse_from_str(read_str(calendar_day, "date")?, "%Y-%m-%d").map_err(
                    |parse_error| {
                        CollectionError::UnexpectedResponse(format!(
                            "calendar date is invalid: {parse_error}"
                        ))
                    },
                )?;
            if contribution_date < request_start || contribution_date > request_end {
                continue;
            }
            let contribution_count = u32::try_from(read_u64(calendar_day, "contributionCount")?)
                .map_err(|_| {
                    CollectionError::UnexpectedResponse(format!(
                        "contribution count for {contribution_date} is out of range"
                    ))
                })?;
            if let Some(previous_count) = daily_counts.insert(contribution_date, contribution_count)
                && previous_count != contribution_count
            {
                return Err(CollectionError::InvalidHistory(
                    ContributionHistoryError::ConflictingDate(contribution_date),
                ));
            }
        }
    }
    Ok(daily_counts)
}

fn read_field<'a>(object: &'a Value, field_name: &str) -> Result<&'a Value, CollectionError> {
    object.get(field_name).ok_or_else(|| {
        CollectionError::UnexpectedResponse(format!("response is missing {field_name}"))
    })
}

fn read_array<'a>(object: &'a Value, field_name: &str) -> Result<&'a Vec<Value>, CollectionError> {
    read_field(object, field_name)?
        .as_array()
        .ok_or_else(|| CollectionError::UnexpectedResponse(format!("{field_name} is not a list")))
}

fn read_str<'a>(object: &'a Value, field_name: &str) -> Result<&'a str, CollectionError> {
    read_field(object, field_name)?
        .as_str()
        .ok_or_else(|| CollectionError::UnexpectedResponse(format!("{field_name} is not text")))
}

fn read_u64(object: &Value, field_name: &str) -> Result<u64, CollectionError> {
    read_field(object, field_name)?.as_u64().ok_or_else(|| {
        CollectionError::UnexpectedResponse(format!("{field_name} is not a whole number"))
    })
}

fn read_bool(object: &Value, field_name: &str) -> Result<bool, CollectionError> {
    read_field(object, field_name)?.as_bool().ok_or_else(|| {
        CollectionError::UnexpectedResponse(format!("{field_name} is not true or false"))
    })
}

/// Why a collection produced no snapshot. Each message names the operator repair.
#[derive(Debug, Eq, PartialEq)]
pub enum CollectionError {
    /// `gh` is not on `PATH`.
    GhNotInstalled,
    /// `gh` ran and failed; carries its own message (authentication, rate limits, errors).
    GhFailed(String),
    /// GitHub has no account with this login.
    AccountNotFound(String),
    /// GitHub answered in a shape this version does not understand; carries the detail.
    UnexpectedResponse(String),
    /// GitHub's contribution calendar does not start on the account's creation date.
    IncompleteCalendar {
        /// The day GitHub says the account was created.
        created_on: NaiveDate,
        /// The first day the calendar reported, or `None` when it reported none.
        first_reported_on: Option<NaiveDate>,
    },
    /// The collected days break the history rules; carries the rule.
    InvalidHistory(ContributionHistoryError),
    /// Two observations of one repository disagree.
    ConflictingRepository(ConflictingRepository),
    /// GitHub reported two colors for this language.
    ConflictingLanguageColor(String),
}

impl fmt::Display for CollectionError {
    fn fmt(&self, output_formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GhNotInstalled => write!(
                output_formatter,
                "gh is not installed or not on PATH; install the GitHub CLI from https://cli.github.com"
            ),
            Self::GhFailed(gh_message) => write!(
                output_formatter,
                "gh api failed: {gh_message}; check `gh auth status`, or set GH_TOKEN in CI"
            ),
            Self::AccountNotFound(login) => {
                write!(output_formatter, "GitHub has no account named {login}")
            }
            Self::UnexpectedResponse(detail) => write!(
                output_formatter,
                "unexpected GitHub response: {detail}; try again or report the response"
            ),
            Self::IncompleteCalendar {
                created_on,
                first_reported_on: Some(first_reported_on),
            } => write!(
                output_formatter,
                "GitHub's contribution calendar starts on {first_reported_on}, but the account was created on {created_on}; try again"
            ),
            Self::IncompleteCalendar {
                created_on,
                first_reported_on: None,
            } => write!(
                output_formatter,
                "GitHub reported no contribution days since the account was created on {created_on}; try again"
            ),
            Self::InvalidHistory(history_error) => {
                write!(
                    output_formatter,
                    "collected history is invalid: {history_error}"
                )
            }
            Self::ConflictingRepository(repository_error) => {
                write!(output_formatter, "{repository_error}")
            }
            Self::ConflictingLanguageColor(language_name) => write!(
                output_formatter,
                "GitHub reported two colors for {language_name}; try again"
            ),
        }
    }
}

impl Error for CollectionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidHistory(history_error) => Some(history_error),
            Self::ConflictingRepository(repository_error) => Some(repository_error),
            _ => None,
        }
    }
}
