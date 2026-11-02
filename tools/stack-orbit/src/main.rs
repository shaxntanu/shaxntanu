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
    const REPO_RADIUS: f64 = 220.0;
    const REPO_WIDTH: f64 = 190.0;
    const REPO_HEIGHT: f64 = 78.0;
    const LANGUAGE_WIDTH: f64 = 152.0;
    const LANGUAGE_HEIGHT: f64 = 72.0;

    let language_radius = if languages.is_empty() {
        420.0
    } else {
        420.0_f64.max((LANGUAGE_WIDTH + 28.0) * languages.len() as f64 / std::f64::consts::TAU)
    };
    let extent = language_radius + 125.0;
    let width = 1400.0_f64.max(extent * 2.0).ceil();
    let height = 1040.0_f64.max(extent * 2.0).ceil();
    let center = Point {
        x: width / 2.0,
        y: height / 2.0 + 18.0,
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
<title id="title">Stack Orbit: randomly sampled GitHub repositories and their languages</title>
<desc id="desc">GitHub connects to {} public repositories from shaxntanu and VIGIL-IoT. Repository connections merge at shared language nodes.</desc>
<defs>
  <radialGradient id="background"><stop offset="0" stop-color="#101c33"/><stop offset="1" stop-color="#070b14"/></radialGradient>
  <linearGradient id="hub"><stop stop-color="#182c50"/><stop offset="1" stop-color="#0b1220"/></linearGradient>
  <filter id="glow" x="-40%" y="-40%" width="180%" height="180%"><feGaussianBlur stdDeviation="5" result="blur"/><feMerge><feMergeNode in="blur"/><feMergeNode in="SourceGraphic"/></feMerge></filter>
  <style>
    .flow {{ stroke-dasharray: 8 12; animation: dash 2.8s linear infinite; }}
    @keyframes dash {{ to {{ stroke-dashoffset: -40; }} }}
    text {{ font-family: Arial, Helvetica, sans-serif; }}
  </style>
</defs>
<rect width="100%" height="100%" rx="30" fill="url(#background)"/>
<text x="{:.1}" y="48" text-anchor="middle" fill="#f0f6fc" font-size="24" font-weight="700" letter-spacing="2">RANDOM {} PUBLIC REPOS  →  SHARED LANGUAGES</text>
<text x="{:.1}" y="75" text-anchor="middle" fill="#8b9bb4" font-size="13">daily sample · at least one from each source when available</text>
"##,
        repositories.len(),
        center.x,
        repositories.len(),
        center.x
    );

    for star in 0..62usize {
        let x = (star.wrapping_mul(193).wrapping_add(47)) as f64 % width;
        let y = (star.wrapping_mul(127).wrapping_add(113)) as f64 % height;
        let opacity = 0.12 + (star % 5) as f64 * 0.07;
        let _ = write!(
            svg,
            r##"<circle cx="{x:.1}" cy="{y:.1}" r="1.3" fill="#9dbdff" opacity="{opacity:.2}"/>"##
        );
    }

    for (index, point) in repository_points.iter().enumerate() {
        let path = quadratic_path(center, *point, (index as f64 - 2.0) * 14.0);
        animated_path(&mut svg, &path, "#7AA2F7", 2.2, 3.0 + index as f64 * 0.25, index as f64 * 0.4);
    }

    for (language_index, language) in languages.iter().enumerate() {
        let target = language_points[language_index];
        for (connection_index, repository_index) in language.repositories.iter().enumerate() {
            let source = repository_points[*repository_index];
            let bend = (language_index as f64 - connection_index as f64) * 12.0;
            let path = quadratic_path(source, target, bend);
            animated_path(
                &mut svg,
                &path,
                &language.color,
                1.8,
                3.8 + (language_index % 5) as f64 * 0.35,
                connection_index as f64 * 0.45,
            );
        }
    }

    for (repository, point) in repositories.iter().zip(&repository_points) {
        let x = point.x - REPO_WIDTH / 2.0;
        let y = point.y - REPO_HEIGHT / 2.0;
        let border = if repository.repository.source.eq_ignore_ascii_case("VIGIL-IoT") {
            "#9B8AFB"
        } else {
            "#62A7FF"
        };
        let repo_name = truncate(&repository.repository.name, 21);
        let source = xml_escape(&format!("@{}", repository.repository.source));
        let url = xml_escape(&repository.repository.url);
        let _ = write!(
            svg,
            r##"<a href="{url}"><g class="repo-node">
  <rect x="{x:.1}" y="{y:.1}" width="{REPO_WIDTH:.1}" height="{REPO_HEIGHT:.1}" rx="16" fill="#101a2a" stroke="{border}" stroke-width="2" filter="url(#glow)"/>
  <text x="{:.1}" y="{:.1}" text-anchor="middle" fill="#f4f7fb" font-size="15" font-weight="700">{}</text>
  <text x="{:.1}" y="{:.1}" text-anchor="middle" fill="{border}" font-size="11">{}</text>
  <title>{}</title>
</g></a>"##,
            point.x,
            point.y - 4.0,
            xml_escape(&repo_name),
            point.x,
            point.y + 20.0,
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
  <rect x="{x:.1}" y="{y:.1}" width="{LANGUAGE_WIDTH:.1}" height="{LANGUAGE_HEIGHT:.1}" rx="18" fill="#111a27" stroke="{}" stroke-width="2"/>
"##,
            language.color
        );
        if let Some(icon) = &language.icon {
            let icon_x = x + 12.0;
            let icon_y = y + 23.0;
            let _ = write!(
                svg,
                r##"<svg x="{icon_x:.1}" y="{icon_y:.1}" width="26" height="26" viewBox="0 0 24 24" aria-hidden="true"><path d="{}" fill="{}"/></svg>"##,
                icon,
                language.color
            );
        } else {
            let letters = xml_escape(&initials(&language.name));
            let _ = write!(
                svg,
                r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" fill="{}" font-size="14" font-weight="700">{letters}</text>"##,
                x + 25.0,
                y + 43.0,
                language.color
            );
        }
        let _ = write!(
            svg,
            r##"<text x="{:.1}" y="{:.1}" fill="#f0f6fc" font-size="13" font-weight="700">{}</text>
  <text x="{:.1}" y="{:.1}" fill="#8b9bb4" font-size="10">{} repo{}</text>
</g>"##,
            x + 46.0,
            y + 33.0,
            xml_escape(&truncate(&language.name, 13)),
            x + 46.0,
            y + 51.0,
            language.repositories.len(),
            if language.repositories.len() == 1 { "" } else { "s" }
        );
    }

    let _ = write!(
        svg,
        r##"<g class="github-hub">
  <circle cx="{:.1}" cy="{:.1}" r="88" fill="url(#hub)" stroke="#5c9dff" stroke-width="3" filter="url(#glow)"/>
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
        r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" fill="#ffffff" font-size="19" font-weight="700" letter-spacing="1">GITHUB</text>
  <text x="{:.1}" y="{:.1}" text-anchor="middle" fill="#9dbdff" font-size="11">{} repos · {} languages</text>
</g>
<text x="{:.1}" y="{:.1}" text-anchor="middle" fill="#70809a" font-size="11">shared language nodes merge connections from every selected repo</text>
</svg>"##,
        center.x,
        center.y + 19.0,
        center.x,
        center.y + 43.0,
        repositories.len(),
        languages.len(),
        center.x,
        height - 24.0
    );

    svg
}

fn animated_path(svg: &mut String, path: &str, color: &str, width: f64, duration: f64, delay: f64) {
    let _ = write!(
        svg,
        r##"<path d="{path}" fill="none" stroke="{color}" stroke-width="{width:.1}" opacity=".72" class="flow"><animate attributeName="stroke-dashoffset" from="0" to="-40" dur="{duration:.2}s" repeatCount="indefinite"/></path>
<circle r="3.2" fill="{color}" filter="url(#glow)"><animateMotion dur="{duration:.2}s" begin="{delay:.2}s" repeatCount="indefinite" path="{path}"/></circle>
"##
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
