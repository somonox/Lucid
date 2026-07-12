use anyhow::{Result, anyhow};
use dialoguer::{Input, theme::ColorfulTheme};
use colored::*;
use webbrowser;
use crate::config::{Config, save_session};

const DREAMHACK_LOGIN_URL: &str = "https://dreamhack.io/users/login";

/// DreamHack's Google OAuth callback page consumes the authorization code
/// itself (confirmed via HAR timing: ~1ms between landing on the callback
/// URL and its own login_finish call), so a CLI-driven code exchange can
/// never win that race. The session does get created successfully in the
/// user's real browser though — we just need to read that cookie back out.
pub async fn login() -> Result<()> {
    println!("{}", "Lucid Authentication".bold().cyan());
    println!("{}", "====================".cyan());
    println!();

    println!("Opening your browser to log in to DreamHack...");
    webbrowser::open(DREAMHACK_LOGIN_URL)?;

    println!("\n{}", "Please finish signing in with Google in the browser.".green());
    println!("Once you're logged in (you'll see your DreamHack dashboard):");
    println!("  1. Open Developer Tools (F12)");
    println!("  2. Go to Application (Chrome) or Storage (Firefox) → Cookies → https://dreamhack.io");
    println!("  3. Copy the value of the 'sessionid' cookie");
    println!();

    let sessionid: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Paste 'sessionid' cookie value")
        .interact_text()?;

    let sessionid = sessionid.trim();
    if sessionid.is_empty() {
        return Err(anyhow!("sessionid cannot be empty"));
    }

    // Logging in itself needs no CSRF token (verified: login_init/
    // login_finish accept requests with no csrf_token at all). But write
    // requests made *after* login (submitting flags, downloading challenge
    // files, etc.) need an X-CSRFToken header matched against this cookie,
    // so grab it now while we're already in DevTools. Note: despite the
    // header being named X-CSRFToken, DreamHack's cookie itself is named
    // 'csrf_token' (with an underscore) — not Django's textbook default
    // 'csrftoken' — confirmed by testing both against the live download
    // endpoint (csrf_token succeeds, csrftoken 403s "CSRF cookie not set").
    println!();
    println!("  4. In the same cookie list, look for 'csrf_token' too");
    println!("     (needed later for actions like submitting flags — leave blank if you don't see one)");
    println!();

    let csrf_token: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Paste 'csrf_token' cookie value (optional)")
        .allow_empty(true)
        .interact_text()?;

    let cookie_string = if csrf_token.trim().is_empty() {
        format!("sessionid={}", sessionid)
    } else {
        format!("sessionid={}; csrf_token={}", sessionid, csrf_token.trim())
    };

    save_session(&cookie_string)?;

    println!("\n{}", "✓ Login successful! Session saved.".green().bold());
    println!("You can now use other Lucid commands.");

    Ok(())
}

pub fn logout() -> Result<()> {
    Config::clear_session()?;
    Ok(())
}
