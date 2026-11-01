#!/usr/bin/env python3
"""Color each 3D contribution day by its dominant repository language."""

from __future__ import annotations

import collections
import datetime as dt
import json
import os
import re
import urllib.request
import xml.etree.ElementTree as ET
from pathlib import Path


GRAPHQL_URL = "https://api.github.com/graphql"
SVG_NS = "http://www.w3.org/2000/svg"
ET.register_namespace("", SVG_NS)

CALENDAR_QUERY = """
query($login: String!) {
  user(login: $login) {
    contributionsCollection {
      contributionCalendar {
        weeks {
          contributionDays { date }
        }
      }
    }
  }
}
"""

# Query one month at a time: a repository can have at most 31 daily entries in
# each response, so 40 nodes per repository is enough and keeps query cost low.
MONTH_QUERY = """
query($login: String!, $from: DateTime!, $to: DateTime!) {
  user(login: $login) {
    contributionsCollection(from: $from, to: $to) {
      commitContributionsByRepository(maxRepositories: 100) {
        repository {
          primaryLanguage { name color }
        }
        contributions(first: 40) {
          nodes { occurredAt commitCount }
        }
      }
    }
  }
}
"""

FALLBACK_LANGUAGE = "Other"
FALLBACK_COLOR = "#8b949e"
NO_COMMIT_COLOR = "#30363d"


def graphql(token: str, query: str, variables: dict) -> dict:
    payload = json.dumps({"query": query, "variables": variables}).encode()
    request = urllib.request.Request(
        GRAPHQL_URL,
        data=payload,
        headers={
            "Authorization": f"bearer {token}",
            "Accept": "application/json",
            "Content-Type": "application/json",
        },
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        body = json.loads(response.read().decode())
    if body.get("errors"):
        messages = "; ".join(error.get("message", "GraphQL error") for error in body["errors"])
        raise RuntimeError(messages)
    user = body.get("data", {}).get("user")
    if user is None:
        raise RuntimeError("GitHub returned no profile data for the configured username")
    return user


def month_chunks(start: dt.date, end: dt.date):
    cursor = start
    while cursor <= end:
        if cursor.month == 12:
            next_month = dt.date(cursor.year + 1, 1, 1)
        else:
            next_month = dt.date(cursor.year, cursor.month + 1, 1)
        chunk_end = min(end, next_month - dt.timedelta(days=1))
        yield cursor, chunk_end
        cursor = chunk_end + dt.timedelta(days=1)


def contribution_dates(token: str, username: str) -> list[str]:
    user = graphql(token, CALENDAR_QUERY, {"login": username})
    weeks = user["contributionsCollection"]["contributionCalendar"]["weeks"]
    return [day["date"][:10] for week in weeks for day in week["contributionDays"]]


def language_commits_by_date(token: str, username: str, dates: list[str]):
    counts = collections.defaultdict(collections.Counter)
    colors = {FALLBACK_LANGUAGE: FALLBACK_COLOR}
    start = dt.date.fromisoformat(dates[0])
    end = dt.date.fromisoformat(dates[-1])

    for chunk_start, chunk_end in month_chunks(start, end):
        user = graphql(
            token,
            MONTH_QUERY,
            {
                "login": username,
                "from": f"{chunk_start.isoformat()}T00:00:00Z",
                "to": f"{chunk_end.isoformat()}T23:59:59Z",
            },
        )
        repositories = user["contributionsCollection"]["commitContributionsByRepository"]
        for item in repositories:
            language = item["repository"].get("primaryLanguage") or {}
            name = language.get("name") or FALLBACK_LANGUAGE
            color = language.get("color") or FALLBACK_COLOR
            if re.fullmatch(r"#[0-9a-fA-F]{6}", color):
                colors.setdefault(name, color)
            else:
                colors.setdefault(name, FALLBACK_COLOR)

            for node in item["contributions"]["nodes"]:
                occurred_at = node["occurredAt"].replace("Z", "+00:00")
                day = dt.datetime.fromisoformat(occurred_at).astimezone(dt.timezone.utc).date().isoformat()
                counts[day][name] += node["commitCount"]

    return counts, colors


def make_visible(color: str) -> str:
    values = [int(color[i : i + 2], 16) for i in (1, 3, 5)]
    brightest = max(values)
    if brightest and brightest < 200:
        scale = 200 / brightest
        values = [min(255, round(value * scale)) for value in values]
    return "#" + "".join(f"{value:02x}" for value in values)


def shade(color: str, factor: float) -> str:
    values = [int(color[i : i + 2], 16) for i in (1, 3, 5)]
    return "#" + "".join(f"{round(value * factor):02x}" for value in values)


def find_daily_bars(root: ET.Element) -> list[ET.Element]:
    group_tag = f"{{{SVG_NS}}}g"
    rect_tag = f"{{{SVG_NS}}}rect"
    for group in root.findall(group_tag):
        bars = [
            child
            for child in list(group)
            if child.tag == group_tag
            and len([face for face in list(child) if face.tag == rect_tag]) >= 3
        ]
        if len(bars) > 200:
            return bars
    raise RuntimeError("Could not locate the 3D daily contribution bars in the SVG")


def main() -> None:
    token = os.environ.get("GITHUB_TOKEN")
    username = os.environ.get("USERNAME")
    if not token or not username:
        raise RuntimeError("GITHUB_TOKEN and USERNAME must be set")

    repo_root = Path(os.environ.get("GITHUB_WORKSPACE", Path(__file__).resolve().parents[2]))
    svg_path = repo_root / "profile-3d-contrib" / "profile-night-green.svg"
    if not svg_path.exists():
        raise RuntimeError(f"Expected generated graph not found: {svg_path}")

    dates = contribution_dates(token, username)
    if not dates:
        raise RuntimeError("GitHub returned an empty contribution calendar")
    counts, language_colors = language_commits_by_date(token, username, dates)
    if not counts:
        raise RuntimeError("No repository-level commit language data was returned")

    tree = ET.parse(svg_path)
    root = tree.getroot()
    bars = find_daily_bars(root)
    if len(bars) != len(dates):
        raise RuntimeError(
            f"The SVG has {len(bars)} daily bars, but GitHub returned {len(dates)} calendar days"
        )

    colored_days = 0
    for day, bar in zip(dates, bars):
        day_commits = counts.get(day, {})
        if day_commits:
            language = min(day_commits, key=lambda item: (-day_commits[item], item))
            color = make_visible(language_colors.get(language, FALLBACK_COLOR))
            colored_days += 1
            commit_count = sum(day_commits.values())
            title_text = f"{day}: {language} ({commit_count} commits)"
        else:
            language = "No commits"
            color = NO_COMMIT_COLOR
            title_text = f"{day}: no commits"

        bar.set("data-language", language)
        title = next((child for child in list(bar) if child.tag == f"{{{SVG_NS}}}title"), None)
        if title is None:
            title = ET.Element(f"{{{SVG_NS}}}title")
            bar.insert(0, title)
        title.text = title_text

        faces = [child for child in list(bar) if child.tag == f"{{{SVG_NS}}}rect"]
        for face, face_color in zip(faces[:3], (color, shade(color, 0.72), shade(color, 0.54))):
            face.set("fill", face_color)

    tree.write(svg_path, encoding="utf-8", xml_declaration=False)
    print(
        f"Colored {colored_days}/{len(dates)} days by dominant repository language; "
        f"found {len(language_colors) - 1} language colors. Updated {svg_path}"
    )


if __name__ == "__main__":
    main()
