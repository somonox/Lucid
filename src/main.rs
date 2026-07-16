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
    }

    Ok(())
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
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out_file = std::fs::File::create(&out_path)?;
            std::io::copy(&mut entry, &mut out_file)?;
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

    Ok(())
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
