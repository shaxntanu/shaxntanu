use std::{
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

fn main() -> ExitCode {
    let mut args = env::args_os().skip(1);
    let readme = args.next().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("README.md"));

    if args.next().is_some() {
        eprintln!("usage: readme-asset-check [README.md]");
        return ExitCode::from(2);
    }

    let contents = match fs::read_to_string(&readme) {
        Ok(contents) => contents,
        Err(error) => {
            eprintln!("{}: {error}", readme.display());
            return ExitCode::FAILURE;
        }
    };

    let root = readme.parent().unwrap_or_else(|| Path::new("."));
    let mut checked = 0usize;
    let mut missing = Vec::new();

    for (line, value) in local_src_references(&contents) {
        let path_part = value.split(['?', '#']).next().unwrap_or("");
        if path_part.is_empty() {
            continue;
        }

        let relative = path_part.trim_start_matches('/');
        let target = root.join(relative);
        checked += 1;

        if !target.is_file() {
            missing.push((line, value, target));
        }
    }

    if missing.is_empty() {
        println!("Checked {checked} local image reference(s) in {}.", readme.display());
        return ExitCode::SUCCESS;
    }

    for (line, reference, target) in &missing {
        eprintln!(
            "{}:{line}: image reference {reference:?} does not exist ({})",
            readme.display(),
            target.display()
        );
    }

    eprintln!("Found {} missing image reference(s).", missing.len());
    ExitCode::FAILURE
}

fn local_src_references(markdown: &str) -> Vec<(usize, String)> {
    let mut references = Vec::new();

    for (line_index, line) in markdown.lines().enumerate() {
        let mut offset = 0;

        while let Some(relative_start) = line[offset..].find("src=") {
            let start = offset + relative_start;
            let before = line[..start].chars().next_back();
            let is_attribute = before.is_some_and(|character| {
                character.is_whitespace() || character == '<'
            });

            if !is_attribute {
                offset = start + 4;
                continue;
            }

            let value_start = start + 4;
            let Some(quote) = line[value_start..].chars().next() else {
                break;
            };
            if quote != '"' && quote != '\'' {
                offset = value_start;
                continue;
            }

            let content_start = value_start + quote.len_utf8();
            let Some(relative_end) = line[content_start..].find(quote) else {
                break;
            };
            let value = &line[content_start..content_start + relative_end];

            if is_local_reference(value) {
                references.push((line_index + 1, value.to_string()));
            }

            offset = content_start + relative_end + quote.len_utf8();
        }
    }

    references
}

fn is_local_reference(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && !value.starts_with("//")
        && !value.starts_with('#')
        && !value.starts_with("data:")
        && !value.starts_with("http://")
        && !value.starts_with("https://")
        && !value.starts_with("mailto:")
}
