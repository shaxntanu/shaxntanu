use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};
use reqwest::blocking::Client;
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashSet},
    env, error::Error, fmt::Write as FmtWrite, fs,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const REPO_COUNT: usize = 5;
const SIMPLE_ICONS: &str = "https://cdn.simpleicons.org";
const DEFAULT_OUTPUT: &str = "profile/stack-orbit.svg";

#[derive(Clone)]
struct Repository {
    name: String,
    full_name: String,
    url: String,
    source: String,
}

struct RepositoryNode {
    repository: Repository,
}

#[derive(Default)]
struct LanguageUsage {
    repositories: Vec<usize>,
}

struct LanguageNode {
    name: String,
    repositories: Vec<usize>,
    color: String,
    icon: Option<String>,
}

#[derive(Clone, Copy)]
struct Point {
    x: f64,
    y: f64,
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let output = PathBuf::from(args.next().unwrap_or_else(|| DEFAULT_OUTPUT.to_string()));
    if args.next().is_some() {
        return Err("usage: stack-orbit [output.svg]".into());
    }

    let username = env::var("GITHUB_USER").unwrap_or_else(|_| "shaxntanu".to_string());
    let organization = env::var("GITHUB_ORG").unwrap_or_else(|_| "VIGIL-IoT".to_string());
    let token = env::var("GITHUB_TOKEN").ok().filter(|value| !value.is_empty());

    let client = Client::builder()
        .user_agent("shaxntanu-stack-orbit/0.1")
        .timeout(Duration::from_secs(20))
        .build()?;

    let mut personal = list_repositories(
        &client,
        &format!("https://api.github.com/users/{username}/repos?type=owner"),
        &username,
        token.as_deref(),
    )?;
    let mut organization_repositories = list_repositories(
        &client,
        &format!("https://api.github.com/orgs/{organization}/repos?type=public"),
        &organization,
        token.as_deref(),
    )?;

    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        / 86_400;
    let selected = choose_repositories(&mut personal, &mut organization_repositories, seed);
    if selected.is_empty() {
        return Err("no eligible public repositories were found".into());
    }

    let mut language_usage: BTreeMap<String, LanguageUsage> = BTreeMap::new();
    let mut repositories = Vec::with_capacity(selected.len());

    for (repository_index, repository) in selected.into_iter().enumerate() {
        println!("Including {}", repository.full_name);
        let languages = match repository_languages(&client, &repository, token.as_deref()) {
            Ok(languages) => languages,
            Err(error) => {
                eprintln!("Could not read languages for {}: {error}", repository.full_name);
                BTreeMap::new()
            }
        };

        for (name, _bytes) in languages {
            let usage = language_usage.entry(name).or_default();
            usage.repositories.push(repository_index);
        }

        repositories.push(RepositoryNode { repository });
    }

    let icon_client = Client::builder()
        .user_agent("shaxntanu-stack-orbit/0.1")
        .timeout(Duration::from_secs(12))
        .build()?;
    let mut icon_cache = BTreeMap::new();
    for name in language_usage.keys() {
        if let Some((slug, icon_color)) = simple_icon(name) {
            let path = fetch_icon_path(&icon_client, slug, icon_color);
            icon_cache.insert(name.clone(), path);
        }
    }
    let github_icon = fetch_icon_path(&icon_client, "github", "FFFFFF");

    let languages: Vec<LanguageNode> = language_usage
        .into_iter()
        .map(|(name, usage)| LanguageNode {
            color: language_color(&name),
            icon: icon_cache.get(&name).cloned().flatten(),
            name,
            repositories: usage.repositories,
        })
        .collect();

    let svg = render_svg(&repositories, &languages, github_icon.as_deref());
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output, svg)?;
    println!(
        "Wrote {} with {} repositories and {} detected languages.",
        output.display(),
        repositories.len(),
        languages.len()
    );
    Ok(())
}

fn list_repositories(
    client: &Client,
    base_url: &str,
    source: &str,
    token: Option<&str>,
) -> Result<Vec<Repository>, Box<dyn Error>> {
    let mut repositories = Vec::new();
    let mut seen = HashSet::new();

    for page in 1..=10 {
        let separator = if base_url.contains('?') { '&' } else { '?' };
        let url = format!("{base_url}{separator}per_page=100&page={page}&sort=pushed");
        let response = github_json(client, &url, token)?;
        let Some(items) = response.as_array() else {
            return Err(format!("GitHub returned an invalid repository list for {source}").into());
        };
        if items.is_empty() {
            break;
        }

        for item in items {
            if item["fork"].as_bool().unwrap_or(false)
                || item["archived"].as_bool().unwrap_or(false)
                || item["private"].as_bool().unwrap_or(false)
                || item["size"].as_u64().unwrap_or(0) == 0
                || item["name"].as_str() == Some(".github")
            {
                continue;
            }

            let Some(full_name) = item["full_name"].as_str() else {
                continue;
            };
            if !seen.insert(full_name.to_ascii_lowercase()) {
                continue;
            }

            repositories.push(Repository {
                name: item["name"].as_str().unwrap_or(full_name).to_string(),
                full_name: full_name.to_string(),
                url: item["html_url"].as_str().unwrap_or("").to_string(),
                source: source.to_string(),
            });
        }

        if items.len() < 100 {
            break;
        }
    }
    Ok(repositories)
}

fn choose_repositories(
    personal: &mut Vec<Repository>,
    organization: &mut Vec<Repository>,
    seed: u64,
) -> Vec<Repository> {
    let mut rng = StdRng::seed_from_u64(seed);
    personal.shuffle(&mut rng);
    organization.shuffle(&mut rng);

    let mut selected = Vec::new();
    if !personal.is_empty() {
        selected.push(personal.remove(0));
    }
    if !organization.is_empty() {
        selected.push(organization.remove(0));
    }

    let mut remaining: Vec<Repository> = personal
        .drain(..)
        .chain(organization.drain(..))
        .collect();
    remaining.shuffle(&mut rng);
    selected.extend(remaining.into_iter().take(REPO_COUNT.saturating_sub(selected.len())));
    selected.shuffle(&mut rng);
    selected
}

fn repository_languages(
    client: &Client,
    repository: &Repository,
    token: Option<&str>,
) -> Result<BTreeMap<String, u64>, Box<dyn Error>> {
    let url = format!("https://api.github.com/repos/{}/languages", repository.full_name);
    let response = github_json(client, &url, token)?;
    let Some(object) = response.as_object() else {
        return Err(format!("GitHub returned invalid language data for {}", repository.full_name).into());
    };

    Ok(object
        .iter()
        .filter_map(|(name, bytes)| bytes.as_u64().map(|bytes| (name.clone(), bytes)))
        .filter(|(_, bytes)| *bytes > 0)
        .collect())
}

fn github_json(
    client: &Client,
    url: &str,
    token: Option<&str>,
) -> Result<Value, Box<dyn Error>> {
    let request = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");
    let request = if let Some(token) = token {
        request.bearer_auth(token)
    } else {
        request
    };
    Ok(request.send()?.error_for_status()?.json()?)
}

fn simple_icon(language: &str) -> Option<(&'static str, &'static str)> {
    match language {
        "Rust" => Some(("rust", "DEA584")),
        "C++" => Some(("cplusplus", "00599C")),
        "C" => Some(("c", "A8B9CC")),
        "Python" => Some(("python", "3776AB")),
        "JavaScript" => Some(("javascript", "F7DF1E")),
        "TypeScript" => Some(("typescript", "3178C6")),
        "HTML" => Some(("html5", "E34F26")),
        "CSS" => Some(("css3", "1572B6")),
        "Java" => Some(("openjdk", "ED8B00")),
        "Go" => Some(("go", "00ADD8")),
        "Kotlin" => Some(("kotlin", "7F52FF")),
        "Swift" => Some(("swift", "F05138")),
        "Ruby" => Some(("ruby", "CC342D")),
        "PHP" => Some(("php", "777BB4")),
        "PowerShell" => Some(("powershell", "5391FE")),
        "Dockerfile" => Some(("docker", "2496ED")),
        "Jupyter Notebook" => Some(("jupyter", "F37626")),
        "MATLAB" => Some(("mathworks", "0076A8")),
        "Shell" => Some(("gnubash", "4EAA25")),
        "C#" => Some(("csharp", "239120")),
        "Dart" => Some(("dart", "0175C2")),
        "Lua" => Some(("lua", "2C2D72")),
        "R" => Some(("r", "276DC3")),
        "SCSS" => Some(("sass", "CC6699")),
        "CMake" => Some(("cmake", "064F8C")),
        "Markdown" => Some(("markdown", "FFFFFF")),
        "JSON" => Some(("json", "000000")),
        "YAML" => Some(("yaml", "CB171E")),
        _ => None,
    }
}

fn fetch_icon_path(client: &Client, slug: &str, color: &str) -> Option<String> {
    let url = format!("{SIMPLE_ICONS}/{slug}/{color}");
    let svg = client.get(url).send().ok()?.error_for_status().ok()?.text().ok()?;
    let path_start = svg.find("<path")?;
    let path_tag = &svg[path_start..];
    let data_start = path_tag.find("d=\"")? + 3;
    let data = &path_tag[data_start..];
    let data_end = data.find('"')?;
    Some(data[..data_end].to_string())
}

fn language_color(language: &str) -> String {
    if let Some((_, color)) = simple_icon(language) {
        return format!("#{color}");
    }
    let palette = [
        "#7AA2F7", "#56B6C2", "#C678DD", "#E5C07B", "#98C379", "#E06C75",
        "#61AFEF", "#D19A66",
    ];
    let hash = language.bytes().fold(0usize, |value, byte| {
        value.wrapping_mul(31).wrapping_add(byte as usize)
    });
    palette[hash % palette.len()].to_string()
}

fn render_svg(
    repositories: &[RepositoryNode],
    languages: &[LanguageNode],
    github_icon: Option<&str>,
) -> String {
    const REPO_RADIUS: f64 = 228.0;
    const REPO_WIDTH: f64 = 184.0;
    const REPO_HEIGHT: f64 = 74.0;
    const LANGUAGE_WIDTH: f64 = 148.0;
    const LANGUAGE_HEIGHT: f64 = 62.0;
    const HUB_RADIUS: f64 = 74.0;
    const HUB_TRACK_RADIUS: f64 = 96.0;

    let language_radius = if languages.is_empty() {
        420.0
    } else {
        420.0_f64.max((LANGUAGE_WIDTH + 34.0) * languages.len() as f64 / std::f64::consts::TAU)
    };
    let extent = language_radius + LANGUAGE_WIDTH / 2.0 + 58.0;
    let width = 1400.0_f64.max(extent * 2.0).ceil();
    let height = 1080.0_f64.max(extent * 2.0).ceil();
    let center = Point {
        x: width / 2.0,
        y: height / 2.0 + 12.0,
    };

    let repository_points: Vec<Point> = (0..repositories.len())
        .map(|index| {
            let angle = -std::f64::consts::FRAC_PI_2
                + std::f64::consts::TAU * index as f64 / repositories.len().max(1) as f64;
            Point {
                x: center.x + REPO_RADIUS * angle.cos(),
                y: center.y + REPO_RADIUS * angle.sin(),
            }
        })
        .collect();

    let language_points: Vec<Point> = (0..languages.len())
        .map(|index| {
            let angle = -std::f64::consts::FRAC_PI_2
                + std::f64::consts::TAU * index as f64 / languages.len().max(1) as f64;
            Point {
                x: center.x + language_radius * angle.cos(),
                y: center.y + language_radius * angle.sin(),
            }
        })
        .collect();

    let mut svg = String::new();
    let _ = write!(
        svg,
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width:.0} {height:.0}" role="img" aria-labelledby="title desc">
<title id="title">Stack Orbit</title>
<desc id="desc">A daily map of {} public repositories from shaxntanu and VIGIL-IoT, joined to {} language nodes. Each language node connects to every selected repository that uses it.</desc>
<defs>
  <style>
    text {{ font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; }}
    @media (prefers-reduced-motion: reduce) {{ .orbiting-dot {{ display: none; }} }}
  </style>
</defs>
<rect width="100%" height="100%" fill="#0d1117"/>
<text x="{:.1}" y="44" text-anchor="middle" fill="#e6edf3" font-size="23" font-weight="650">Stack Orbit</text>
<text x="{:.1}" y="68" text-anchor="middle" fill="#8b949e" font-size="12">{} repositories · {} languages · daily sample</text>
<circle cx="{:.1}" cy="{:.1}" r="{REPO_RADIUS:.1}" fill="none" stroke="#27313d" stroke-width="1"/>
<circle cx="{:.1}" cy="{:.1}" r="{language_radius:.1}" fill="none" stroke="#27313d" stroke-width="1"/>
<circle cx="{:.1}" cy="{:.1}" r="{HUB_TRACK_RADIUS:.1}" fill="none" stroke="#344251" stroke-width="1" stroke-dasharray="1 7"/>
"##,
        repositories.len(),
        languages.len(),
        center.x,
        center.x,
        repositories.len(),
        languages.len(),
        center.x,
        center.y,
        center.x,
        center.y,
        center.x,
        center.y
    );

    for (index, point) in repository_points.iter().enumerate() {
        let offset = index as f64 - (repositories.len().saturating_sub(1) as f64 / 2.0);
        let path = quadratic_path(center, *point, offset * 7.0);
        connection_path(&mut svg, &path, "#526171", 1.25, 0.74);
    }

    for (language_index, language) in languages.iter().enumerate() {
        let target = language_points[language_index];
        for (connection_index, repository_index) in language.repositories.iter().enumerate() {
            let source = repository_points[*repository_index];
            let bend = (language_index as f64 - connection_index as f64) * 8.0;
            let path = quadratic_path(source, target, bend);
            connection_path(
                &mut svg,
                &path,
                &language.color,
                1.05,
                0.42,
            );
        }
    }

    for (repository, point) in repositories.iter().zip(&repository_points) {
        let x = point.x - REPO_WIDTH / 2.0;
        let y = point.y - REPO_HEIGHT / 2.0;
        let border = if repository.repository.source.eq_ignore_ascii_case("VIGIL-IoT") {
            "#56B6C2"
        } else {
            "#7AA2F7"
        };
        let repo_name = truncate(&repository.repository.name, 18);
        let source = xml_escape(&repository.repository.source);
        let url = xml_escape(&repository.repository.url);
        let _ = write!(
            svg,
            r##"<a href="{url}"><g class="repo-node">
  <rect x="{x:.1}" y="{y:.1}" width="{REPO_WIDTH:.1}" height="{REPO_HEIGHT:.1}" rx="7" fill="#111820" stroke="#303b48" stroke-width="1"/>
  <rect x="{x:.1}" y="{y:.1}" width="3" height="{REPO_HEIGHT:.1}" rx="1.5" fill="{border}"/>
  <text x="{:.1}" y="{:.1}" fill="#e6edf3" font-size="14" font-weight="600">{}</text>
  <text x="{:.1}" y="{:.1}" fill="{border}" font-size="10">{}</text>
  <title>{}</title>
</g></a>"##,
            point.x - REPO_WIDTH / 2.0 + 14.0,
            point.y - 4.0,
            xml_escape(&repo_name),
            point.x - REPO_WIDTH / 2.0 + 14.0,
            point.y + 18.0,
            source,
            xml_escape(&repository.repository.full_name)
        );
    }

    for (language, point) in languages.iter().zip(&language_points) {
        let x = point.x - LANGUAGE_WIDTH / 2.0;
        let y = point.y - LANGUAGE_HEIGHT / 2.0;
        let _ = write!(
            svg,
            r##"<g class="language-node">
  <rect x="{x:.1}" y="{y:.1}" width="{LANGUAGE_WIDTH:.1}" height="{LANGUAGE_HEIGHT:.1}" rx="7" fill="#111820" stroke="#303b48" stroke-width="1"/>
  <rect x="{x:.1}" y="{y:.1}" width="3" height="{LANGUAGE_HEIGHT:.1}" rx="1.5" fill="{}"/>
"##,
            language.color
        );
        if let Some(icon) = &language.icon {
            let icon_x = x + 13.0;
            let icon_y = y + 21.0;
            let _ = write!(
                svg,
                r##"<svg x="{icon_x:.1}" y="{icon_y:.1}" width="20" height="20" viewBox="0 0 24 24" aria-hidden="true"><path d="{}" fill="{}"/></svg>"##,
                icon,
                language.color
            );
        } else {
            let letters = xml_escape(&initials(&language.name));
            let _ = write!(
                svg,
                r##"<circle cx="{:.1}" cy="{:.1}" r="10" fill="{}" opacity=".18"/><text x="{:.1}" y="{:.1}" text-anchor="middle" fill="{}" font-size="10" font-weight="700">{letters}</text>"##,
                x + 23.0,
                y + 31.0,
                language.color,
                x + 23.0,
                y + 35.0,
                language.color
            );
        }
        let _ = write!(
            svg,
            r##"<text x="{:.1}" y="{:.1}" fill="#e6edf3" font-size="12" font-weight="600">{}</text>
  <text x="{:.1}" y="{:.1}" fill="#8b949e" font-size="10">{} repo{}</text>
  <title>{} appears in {} selected repositories</title>
</g>"##,
            x + 42.0,
            y + 27.0,
            xml_escape(&truncate(&language.name, 14)),
            x + 42.0,
            y + 45.0,
            language.repositories.len(),
            if language.repositories.len() == 1 { "" } else { "s" },
            xml_escape(&language.name),
            language.repositories.len()
        );
    }

    let _ = write!(
        svg,
        r##"<g class="github-hub">
  <circle cx="{:.1}" cy="{:.1}" r="{HUB_RADIUS:.1}" fill="#111820" stroke="#7aa2f7" stroke-width="1.5"/>
"##,
        center.x,
        center.y
    );
    if let Some(icon) = github_icon {
        let _ = write!(
            svg,
            r##"<svg x="{:.1}" y="{:.1}" width="42" height="42" viewBox="0 0 24 24" aria-hidden="true"><path d="{icon}" fill="#ffffff"/></svg>"##,
            center.x - 21.0,
            center.y - 42.0
        );
    }
    let _ = write!(
        svg,
        r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" fill="#ffffff" font-size="18" font-weight="650">GitHub</text>
  <text x="{:.1}" y="{:.1}" text-anchor="middle" fill="#8b949e" font-size="10">{} repositories</text>
</g>
<circle class="orbiting-dot" cx="{:.1}" cy="{:.1}" r="2.5" fill="#7aa2f7">
  <animateTransform attributeName="transform" type="rotate" from="0 {:.1} {:.1}" to="360 {:.1} {:.1}" dur="24s" repeatCount="indefinite"/>
</circle>
<text x="{:.1}" y="{:.1}" text-anchor="middle" fill="#8b949e" font-size="11">One node per language, shared by every selected repository that uses it.</text>
</svg>"##,
        center.x,
        center.y + 19.0,
        center.x,
        center.y + 43.0,
        repositories.len(),
        center.x + HUB_TRACK_RADIUS,
        center.y,
        center.x,
        center.y,
        center.x,
        center.y,
        center.x,
        height - 24.0
    );

    svg
}

fn connection_path(svg: &mut String, path: &str, color: &str, width: f64, opacity: f64) {
    let _ = write!(
        svg,
        r##"<path d="{path}" fill="none" stroke="{color}" stroke-width="{width:.2}" opacity="{opacity:.2}" stroke-linecap="round"/>"##
    );
}

fn quadratic_path(start: Point, end: Point, bend: f64) -> String {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length = (dx * dx + dy * dy).sqrt().max(1.0);
    let control_x = (start.x + end.x) / 2.0 - dy / length * bend;
    let control_y = (start.y + end.y) / 2.0 + dx / length * bend;
    format!(
        "M {:.1} {:.1} Q {:.1} {:.1} {:.1} {:.1}",
        start.x, start.y, control_x, control_y, end.x, end.y
    )
}

fn initials(text: &str) -> String {
    let letters: String = text
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter_map(|part| part.chars().next())
        .take(2)
        .collect();
    if letters.is_empty() {
        text.chars().take(2).collect::<String>().to_uppercase()
    } else {
        letters.to_uppercase()
    }
}

fn truncate(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    let mut shortened: String = chars.into_iter().take(max_chars.saturating_sub(1)).collect();
    shortened.push('…');
    shortened
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
