mod auth;
mod api;
mod config;

use clap::{Parser, Subcommand};
use anyhow::Result;
use colored::*;

#[derive(Parser)]
#[command(name = "lucid")]
#[command(about = "Lucid - A powerful CLI tool for DreamHack", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Login to DreamHack
    Login,
    /// Logout from DreamHack
    Logout,
    /// Get current user information
    Me,
    /// List available wargame challenges
    Challenges {
        #[arg(short, long, help = "Filter by category (web, pwnable, reversing, crypto, misc, ...)")]
        category: Option<String>,
        #[arg(short, long, help = "Filter by title keyword")]
        search: Option<String>,
    },
    /// Search DreamHack (wargame, users, learning paths, units, Q&A, community)
    Search {
        keyword: String,
    },
    /// Download and extract a wargame challenge's files (requires login)
    Download {
        challenge_id: i64,
        #[arg(short, long, help = "Output folder (default: the challenge's title)")]
        output: Option<String>,
    },
    /// Show wargame stats for yourself or another user by nickname
    Stat {
        /// Nickname to look up (defaults to your own account if omitted, requires login)
        user: Option<String>,
    },
    /// List unsolved challenges in your current class for a category (requires login)
    Class {
        /// pwnable, reversing, web, or crypto
        category: String,
    },
    /// Submit a flag for a challenge (requires login). Stops any container
    /// 'lucid download' started for it if the flag is correct.
    Submit {
        challenge_id: i64,
        flag: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Login => {
            println!("{}", "Starting Lucid authentication...".green());
            auth::login().await?;
        }
        Commands::Logout => {
            auth::logout()?;
            println!("{}", "Logged out successfully!".green());
        }
        Commands::Me => {
            print_me().await?;
        }
        Commands::Challenges { category, search } => {
            print_challenges(category.as_deref(), search.as_deref()).await?;
        }
        Commands::Search { keyword } => {
            print_search(&keyword).await?;
        }
        Commands::Download { challenge_id, output } => {
            download_challenge(challenge_id, output.as_deref()).await?;
        }
        Commands::Stat { user } => {
            print_stat(user.as_deref()).await?;
        }
        Commands::Class { category } => {
            print_class(&category).await?;
        }
        Commands::Submit { challenge_id, flag } => {
            submit_flag(challenge_id, &flag).await?;
        }
    }

    Ok(())
}

async fn submit_flag(challenge_id: i64, flag: &str) -> Result<()> {
    println!("Submitting flag for challenge #{}...", challenge_id);
    api::submit_flag(challenge_id, flag).await?;
    println!("{}", "✓ Correct flag!".green().bold());

    let challenge = api::get_challenge(challenge_id).await?;
    stop_docker_container(&docker_tag_for_challenge(challenge_id, &challenge.title));

    Ok(())
}

/// Best-effort teardown of whatever container `lucid download` started for
/// this challenge. Silent no-op if docker isn't installed or no such
/// container exists - a solved flag shouldn't fail just because there was
/// nothing running to stop.
fn stop_docker_container(tag: &str) {
    if docker_unavailable_reason().is_some() {
        return;
    }

    if let Ok(output) = std::process::Command::new("docker").args(["rm", "-f", tag]).output() {
        if output.status.success() {
            println!("{}", format!("✓ Stopped container '{}'", tag).green());
        }
    }
}

const CLASS_CATEGORIES: [&str; 4] = ["pwnable", "reversing", "web", "crypto"];

async fn print_me() -> Result<()> {
    let user_info = api::get_user_info().await?;
    println!("{}", format!("User: {}", user_info).cyan());

    let tracked = api::get_tracked_classes().await?;
    println!("\n{}", "Class progress".yellow().bold());
    for c in &tracked.categories {
        match &c.tracked_class {
            Some(tc) => println!(
                "  {:<10} level {}  ({}/{} solved) - {}",
                c.category, tc.level, tc.cnt_completed, tc.cnt_challenges, tc.description
            ),
            None => println!("  {:<10} all levels completed", c.category),
        }
    }

    Ok(())
}

async fn print_class(category: &str) -> Result<()> {
    if !CLASS_CATEGORIES.contains(&category) {
        return Err(anyhow::anyhow!(
            "Unknown category '{}' - expected one of: {}",
            category,
            CLASS_CATEGORIES.join(", ")
        ));
    }

    let tracked = api::get_tracked_classes().await?;
    let track = tracked
        .categories
        .into_iter()
        .find(|c| c.category == category)
        .ok_or_else(|| anyhow::anyhow!("No class track found for '{}'", category))?;

    let Some(tc) = track.tracked_class else {
        println!(
            "{}",
            format!("✓ You've completed every class level in {}!", category).green().bold()
        );
        return Ok(());
    };

    println!(
        "{}",
        format!("{} level {} - {}", category, tc.level, tc.description).cyan().bold()
    );
    println!("{}\n", format!("{}/{} solved so far", tc.cnt_completed, tc.cnt_challenges).dimmed());

    let challenges = api::get_class_challenges(category, tc.level).await?;
    let unsolved: Vec<_> = challenges.iter().filter(|c| !c.is_completed).collect();

    if unsolved.is_empty() {
        println!("{}", "No unsolved challenges left in this class!".green());
        return Ok(());
    }

    println!("{}", format!("Unsolved ({}):", unsolved.len()).yellow().bold());
    for c in unsolved {
        println!(
            "  {} {}  [{}]  {}",
            format!("#{}", c.id).dimmed(),
            c.title,
            c.tier_display,
            format!("solved by {}", c.cnt_solvers).dimmed()
        );
    }

    Ok(())
}

async fn download_challenge(challenge_id: i64, output: Option<&str>) -> Result<()> {
    let challenge = api::get_challenge(challenge_id).await?;
    println!("Requesting download link for '{}' (#{})...", challenge.title, challenge_id);

    let bytes = api::download_challenge(challenge_id).await?;

    let folder_name = output
        .map(String::from)
        .unwrap_or_else(|| sanitize_folder_name(&challenge.title));
    let folder = std::path::Path::new(&folder_name);
    std::fs::create_dir_all(folder)?;

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&bytes[..]))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(relative_path) = entry.enclosed_name() else {
            continue;
        };
        let out_path = folder.join(relative_path);

        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)?;
            continue;
        }

        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Buffered (rather than streamed via io::copy) so we can sniff the
        // first bytes for an ELF/shebang executable before writing - the
        // zip crate doesn't restore the original Unix permission bits, so
        // extracted binaries otherwise land as non-executable (confirmed:
        // a downloaded ELF challenge binary came out `-rw-r--r--`).
        let mut contents = Vec::with_capacity(entry.size() as usize);
        std::io::Read::read_to_end(&mut entry, &mut contents)?;
        std::fs::write(&out_path, &contents)?;
        if is_executable(&contents) {
            make_executable(&out_path)?;
        }
    }

    std::fs::write(folder.join("README.md"), challenge_readme(&challenge))?;

    println!(
        "{}",
        format!(
            "✓ Extracted '{}' (with README.md) into {}/",
            challenge.title, folder_name
        )
        .green()
        .bold()
    );

    if let Some(dockerfile) = find_dockerfile(folder) {
        if let Err(e) = build_and_run_docker(&dockerfile, challenge.id, &challenge.title) {
            println!("{}", format!("⚠ Dockerfile found but container setup failed: {e}").yellow());
        }
    }

    Ok(())
}

fn is_executable(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x7fELF") || bytes.starts_with(b"#!")
}

#[cfg(unix)]
fn make_executable(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(perms.mode() | 0o111);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &std::path::Path) -> Result<()> {
    Ok(())
}

/// Depth-first search for a file literally named `Dockerfile`, checking
/// each directory's own files before descending (matches the common case
/// of it sitting at the extracted folder's top level).
fn find_dockerfile(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.file_name().is_some_and(|n| n == "Dockerfile") {
            return Some(path);
        }
        if path.is_dir() {
            subdirs.push(path);
        }
    }
    subdirs.into_iter().find_map(|d| find_dockerfile(&d))
}

/// `docker --version` succeeds even when the daemon socket is unreachable
/// (it only talks to the client binary), so it can't predict whether
/// `docker build`/`run` will actually work. `docker info` requires a live
/// daemon connection, so it surfaces permission/daemon-down problems
/// upfront instead of mid-build behind a wall of unrelated tar/pipe
/// errors - confirmed against a real run: `docker --version` passed, then
/// `docker build` failed deep into tar streaming with "permission denied
/// ... docker.sock" buried in the middle of the noise.
fn docker_unavailable_reason() -> Option<String> {
    match std::process::Command::new("docker").arg("info").output() {
        Err(_) => Some("'docker' isn't installed".to_string()),
        Ok(output) if output.status.success() => None,
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("permission denied") && stderr.contains("docker.sock") {
                Some(
                    "permission denied talking to the Docker daemon - your user likely isn't \
                     in the 'docker' group. Try: sudo usermod -aG docker $USER, then log out \
                     and back in (or run `newgrp docker`)."
                        .to_string(),
                )
            } else {
                Some(format!("docker daemon not reachable: {}", stderr.trim()))
            }
        }
    }
}

/// Echoes docker's own stdout/stderr so nothing is hidden even though we
/// captured it (needed to check exit status ourselves rather than just
/// inheriting the child's stdio).
fn print_docker_output(output: &std::process::Output) {
    use std::io::Write;
    let _ = std::io::stdout().write_all(&output.stdout);
    let _ = std::io::stderr().write_all(&output.stderr);
}

fn build_and_run_docker(dockerfile: &std::path::Path, challenge_id: i64, title: &str) -> Result<()> {
    let build_context = dockerfile
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Dockerfile has no parent directory"))?;

    if let Some(reason) = docker_unavailable_reason() {
        println!(
            "{}",
            format!("Dockerfile found but Docker isn't usable ({reason}) - skipping container build.")
                .yellow()
        );
        return Ok(());
    }

    let tag = docker_tag_for_challenge(challenge_id, title);

    println!("\nDockerfile found - building image '{}'...", tag);
    let build_output = std::process::Command::new("docker")
        .args(["build", "-t", &tag, "."])
        .current_dir(build_context)
        .output()?;
    print_docker_output(&build_output);
    if !build_output.status.success() {
        return Err(anyhow::anyhow!("docker build exited with {}", build_output.status));
    }

    // Ignore failures here - there's just nothing to remove on a first run.
    let _ = std::process::Command::new("docker")
        .args(["rm", "-f", &tag])
        .output();

    println!("Starting container '{}'...", tag);
    let run_output = std::process::Command::new("docker")
        .args(["run", "-d", "-P", "--name", &tag, &tag])
        .output()?;
    print_docker_output(&run_output);
    if !run_output.status.success() {
        return Err(anyhow::anyhow!("docker run exited with {}", run_output.status));
    }

    let ports = std::process::Command::new("docker")
        .args(["port", &tag])
        .output()?;
    let ports = String::from_utf8_lossy(&ports.stdout);

    println!("{}", format!("✓ Container '{}' is running", tag).green().bold());
    if ports.trim().is_empty() {
        println!("  (no ports exposed)");
    } else {
        for line in ports.lines() {
            println!("  {}", line);
        }
    }

    Ok(())
}

/// Docker image/container names must be lowercase alphanumerics plus
/// `.`/`_`/`-`.
fn docker_slug(name: &str) -> String {
    let lower: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '-' })
        .collect();
    lower.trim_matches(|c: char| matches!(c, '-' | '.' | '_')).to_string()
}

/// Derived from `challenge_id` (not the local download folder name, which
/// `--output` can override) so `lucid submit` can independently recompute
/// the same tag later to tear the container down, without needing to know
/// where - or under what name - it was downloaded to. Prefixed with
/// `lucid-` to namespace what this tool creates (`docker ps --filter
/// name=lucid-`).
fn docker_tag_for_challenge(challenge_id: i64, title: &str) -> String {
    let slug = docker_slug(title);
    if slug.is_empty() {
        format!("lucid-{challenge_id}")
    } else {
        format!("lucid-{challenge_id}-{slug}")
    }
}

/// The challenge's own `description` is already markdown (confirmed via
/// curl - e.g. "## Reference" headers, `[text](url)` links), so it's
/// embedded as-is rather than escaped or reformatted.
fn challenge_readme(challenge: &api::Challenge) -> String {
    format!(
        "# {title}\n\n\
        - **Tier:** {tier}\n\
        - **Tags:** {tags}\n\
        - **Author:** {author}\n\
        - **Solvers:** {solvers}\n\n\
        ## Description\n\n\
        {description}\n",
        title = challenge.title,
        tier = challenge.tier_display,
        tags = challenge.tags.join(", "),
        author = challenge.author.nickname,
        solvers = challenge.cnt_solvers,
        description = challenge.description.as_deref().unwrap_or("_No description provided._"),
    )
}

/// Filesystem-unsafe characters (Windows-forbidden set covers Linux/macOS too)
/// replaced with '_' so a challenge title can be used directly as a folder name.
fn sanitize_folder_name(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches('.');
    if trimmed.is_empty() {
        "challenge".to_string()
    } else {
        trimmed.to_string()
    }
}

async fn print_challenges(category: Option<&str>, search: Option<&str>) -> Result<()> {
    let page = api::get_challenges(search, category).await?;

    println!(
        "{}",
        format!("{} challenges found (showing {})", page.count, page.results.len()).cyan()
    );
    println!();

    for c in &page.results {
        let mark = if c.is_completed {
            "✓".green()
        } else if c.is_attempted {
            "~".yellow()
        } else {
            " ".normal()
        };
        println!(
            "{} [{}] {}  {}  {}",
            mark,
            c.tier_display.bold(),
            c.title,
            format!("({})", c.tags.join(", ")).dimmed(),
            format!("solved by {}", c.cnt_solvers).dimmed()
        );
    }

    Ok(())
}

async fn print_search(keyword: &str) -> Result<()> {
    let results = api::search(keyword).await?;

    println!("{}", format!("Search results for '{}'", keyword).cyan().bold());

    if results.wargame.count > 0 {
        println!("\n{} ({})", "Wargame".yellow().bold(), results.wargame.count);
        for c in &results.wargame.results {
            let title = if c.is_completed {
                c.title.green()
            } else {
                c.title.normal()
            };
            println!(
                "  {} {} [{}]  {}",
                format!("#{}", c.id).dimmed(),
                title,
                c.tier_display,
                format!("solved by {}", c.cnt_solvers).dimmed()
            );
        }
    }

    if results.users.count > 0 {
        println!("\n{} ({})", "Users".yellow().bold(), results.users.count);
        for u in &results.users.results {
            let country = u.country.as_deref().unwrap_or("?");
            println!("  {} ({})", u.nickname, country);
        }
    }

    if results.paths.count > 0 {
        println!("\n{} ({})", "Learning Paths".yellow().bold(), results.paths.count);
        for p in &results.paths.results {
            println!("  {}  {}", p.title, format!("tier {}, {} units", p.tier, p.cnt_units).dimmed());
        }
    }

    if results.units.count > 0 {
        println!("\n{} ({})", "Units".yellow().bold(), results.units.count);
        for u in &results.units.results {
            println!("  {}  {}", u.title, format!("tier {}", u.tier).dimmed());
        }
    }

    if results.questions.count > 0 {
        println!("\n{} ({})", "Q&A".yellow().bold(), results.questions.count);
        for q in &results.questions.results {
            println!("  {}", q.title);
        }
    }

    if results.community.count > 0 {
        println!("\n{} ({})", "Community".yellow().bold(), results.community.count);
        for p in &results.community.results {
            println!("  {}", p.title);
        }
    }

    Ok(())
}

async fn print_stat(user: Option<&str>) -> Result<()> {
    let (id, nickname, wargame) = match user {
        None => {
            let info = api::get_user_info().await?;
            (info.id, info.nickname, info.wargame)
        }
        Some(query) => {
            let results = api::search(query).await?;
            let matched = results
                .users
                .results
                .into_iter()
                .next()
                .ok_or_else(|| anyhow::anyhow!("No user found matching '{}'", query))?;
            (matched.id, matched.nickname, matched.wargame)
        }
    };

    let progress = api::get_tier_progress(id).await?;

    println!("{}", format!("{}'s Wargame Stats", nickname).cyan().bold());
    println!(
        "Rank {}  Score {}  Solved {}",
        format!("#{}", wargame.rank).bold(),
        wargame.score,
        wargame.solved
    );

    println!("\n{}", "By category".yellow().bold());
    for (name, stat) in [
        ("pwnable", &wargame.category.pwnable),
        ("reversing", &wargame.category.reversing),
        ("web", &wargame.category.web),
        ("crypto", &wargame.category.crypto),
        ("others", &wargame.category.others),
    ] {
        println!(
            "  {:<10} solved {:<4} score {:<6} rank #{}",
            name, stat.solved_cnt, stat.score, stat.rank
        );
    }

    println!("\n{}", "By tier".yellow().bold());
    for t in &progress.challenges {
        if t.cnt_by_tier == 0 {
            continue;
        }
        println!("  Tier {:<3} {}/{}", t.tier, t.cnt_solved, t.cnt_by_tier);
    }

    Ok(())
}
