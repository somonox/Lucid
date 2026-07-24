use anyhow::{Result, anyhow};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use crate::config::{get_session, get_csrf_token};

const DREAMHACK_BASE_URL: &str = "https://dreamhack.io";
const USER_DETAIL_ENDPOINT: &str = "/api/v1/user/detail/";
// CloudFront in front of dreamhack.io 403s any request with no User-Agent
// at all; reqwest sends none by default. See oauth.rs for how this was
// diagnosed.
const LUCID_USER_AGENT: &str = concat!("Lucid-CLI/", env!("CARGO_PKG_VERSION"));

fn client() -> Result<Client> {
    Ok(Client::builder().user_agent(LUCID_USER_AGENT).build()?)
}

// Real schema confirmed against the live API (curl to /api/v1/user/detail/):
// an earlier draft of this struct (id/username/email/nickname/points/rank)
// was guessed and wrong the same way Challenge originally was - there's no
// top-level "points", and stats live under nested wargame/contributions
// blocks instead.
#[derive(Debug, Deserialize)]
pub struct CategoryStat {
    pub score: i64,
    pub solved_cnt: i64,
    pub rank: i64,
}

#[derive(Debug, Deserialize)]
pub struct CategoryBreakdown {
    pub pwnable: CategoryStat,
    pub reversing: CategoryStat,
    pub web: CategoryStat,
    pub crypto: CategoryStat,
    pub others: CategoryStat,
}

#[derive(Debug, Deserialize)]
pub struct WargameStats {
    pub rank: i64,
    pub score: i64,
    pub category: CategoryBreakdown,
    pub solved: i64,
}

#[derive(Debug, Deserialize)]
pub struct Contributions {
    pub level: i64,
    pub exp: i64,
    pub total_exp: i64,
    pub exp_needed: i64,
}

#[derive(Debug, Deserialize)]
pub struct UserInfo {
    pub id: i64,
    pub nickname: String,
    pub wargame: WargameStats,
    pub contributions: Contributions,
}

impl std::fmt::Display for UserInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} - Rank #{}, Score {} ({} solved)",
            self.nickname, self.wargame.rank, self.wargame.score, self.wargame.solved
        )
    }
}

pub async fn get_user_info() -> Result<UserInfo> {
    let session = get_session()?
        .ok_or_else(|| anyhow!("Not logged in. Please run 'lucid login' first."))?;
    
    let csrf_token = get_csrf_token()?
        .unwrap_or_default();

    let client = client()?;
    let response = client
        .get(format!("{}{}", DREAMHACK_BASE_URL, USER_DETAIL_ENDPOINT))
        .header("Accept", "application/json")
        .header("Cookie", session)
        .header("X-CSRFToken", csrf_token)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow!("Failed to get user info: {}", response.status()));
    }

    let user_info: UserInfo = response.json().await?;
    Ok(user_info)
}

#[derive(Debug, Deserialize)]
pub struct Author {
    pub nickname: String,
}

// Real schema confirmed against the live API (search.har):
// GET /api/v1/wargame/challenges/?page=&search=&ordering=&scope=&category=&status=&difficulty=&type=&page_size=
// No login required - this is public. `tags` replaces the singular
// "category" field an earlier draft of this struct guessed at, and there's
// no `points` in the list view at all. `description` is only present on the
// single-challenge detail endpoint (confirmed via curl), so it's Option -
// this same struct is shared across list/search/detail responses.
#[derive(Debug, Deserialize)]
pub struct Challenge {
    pub id: i64,
    pub title: String,
    pub author: Author,
    pub tags: Vec<String>,
    pub tier: i32,
    pub tier_display: String,
    pub cnt_solvers: i32,
    pub is_completed: bool,
    pub is_attempted: bool,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ChallengePage {
    pub count: i64,
    pub next: Option<String>,
    pub previous: Option<String>,
    pub results: Vec<Challenge>,
}

// `is_completed`/`is_attempted` are per-user - the server only knows to set
// them if it can identify who's asking. These list/detail/search endpoints
// work fine anonymously (no login required), but without the session
// cookie they'd always come back false regardless of login state. So we
// attach it when available, best-effort, without requiring login.
fn with_optional_session(request: reqwest::RequestBuilder) -> Result<reqwest::RequestBuilder> {
    Ok(match get_session()? {
        Some(session) => request.header("Cookie", session),
        None => request,
    })
}

pub async fn get_challenges(search: Option<&str>, category: Option<&str>) -> Result<ChallengePage> {
    let client = client()?;

    let query = [
        ("page", "1"),
        ("page_size", "20"),
        ("ordering", "cnt_solvers"),
        ("search", search.unwrap_or("")),
        ("category", category.unwrap_or("")),
        ("scope", ""),
        ("status", ""),
        ("difficulty", ""),
        ("type", ""),
    ];

    let request = client
        .get(format!("{}/api/v1/wargame/challenges/", DREAMHACK_BASE_URL))
        .header("Accept", "application/json")
        .query(&query);

    let response = with_optional_session(request)?.send().await?;

    if !response.status().is_success() {
        return Err(anyhow!("Failed to get challenges: {}", response.status()));
    }

    Ok(response.json().await?)
}

// GET /api/v1/wargame/challenges/{id}/ - confirmed public (no auth) via curl.
// Same shape as the list's Challenge, plus extra fields (description,
// exported_from, ...) we don't need - serde ignores them.
pub async fn get_challenge(challenge_id: i64) -> Result<Challenge> {
    let client = client()?;

    let request = client
        .get(format!(
            "{}/api/v1/wargame/challenges/{}/",
            DREAMHACK_BASE_URL, challenge_id
        ))
        .header("Accept", "application/json");

    let response = with_optional_session(request)?.send().await?;

    if !response.status().is_success() {
        return Err(anyhow!("Failed to get challenge {}: {}", challenge_id, response.status()));
    }

    Ok(response.json().await?)
}

// Real schema confirmed against the live API (search.har):
// GET /api/v1/services/suggestion/?keyword=&section=
// Public, no login required. Leaving `section` empty returns all six
// categories at once (what DreamHack's own search bar does), each capped
// at 5 results. Nested author/user stat blocks are dropped here - serde
// ignores unlisted JSON fields, so only what's useful for CLI output is
// modeled.
#[derive(Debug, Deserialize)]
pub struct SearchSection<T> {
    pub count: i64,
    pub results: Vec<T>,
}

#[derive(Debug, Deserialize)]
pub struct SearchResults {
    pub wargame: SearchSection<Challenge>,
    pub users: SearchSection<UserResult>,
    pub paths: SearchSection<PathResult>,
    pub units: SearchSection<UnitResult>,
    pub questions: SearchSection<QuestionResult>,
    pub community: SearchSection<CommunityResult>,
}

#[derive(Debug, Deserialize)]
pub struct UserResult {
    pub id: i64,
    pub nickname: String,
    pub country: Option<String>,
    pub wargame: WargameStats,
}

#[derive(Debug, Deserialize)]
pub struct PathResult {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub tier: i32,
    pub cnt_units: i32,
}

#[derive(Debug, Deserialize)]
pub struct UnitResult {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub tier: i32,
}

#[derive(Debug, Deserialize)]
pub struct QuestionResult {
    pub id: i64,
    pub title: String,
    pub scope: String,
}

#[derive(Debug, Deserialize)]
pub struct CommunityResult {
    pub id: i64,
    pub slug: String,
    pub title: String,
}

pub async fn search(keyword: &str) -> Result<SearchResults> {
    let client = client()?;

    let request = client
        .get(format!("{}/api/v1/services/suggestion/", DREAMHACK_BASE_URL))
        .header("Accept", "application/json")
        .query(&[("keyword", keyword), ("section", "")]);

    let response = with_optional_session(request)?.send().await?;

    if !response.status().is_success() {
        return Err(anyhow!("Search failed: {}", response.status()));
    }

    Ok(response.json().await?)
}

// GET /api/v1/wargame/challenges/complete/?user_id= - confirmed public (no
// auth) for any user_id via curl (stat.har was captured from /mypage/, but
// the endpoint itself doesn't check that user_id matches the session).
#[derive(Debug, Deserialize)]
pub struct TierCount {
    pub tier: i32,
    pub cnt_by_tier: i32,
    pub cnt_solved: i32,
}

#[derive(Debug, Deserialize)]
pub struct TierProgress {
    pub challenges: Vec<TierCount>,
}

pub async fn get_tier_progress(user_id: i64) -> Result<TierProgress> {
    let client = client()?;

    let response = client
        .get(format!(
            "{}/api/v1/wargame/challenges/complete/",
            DREAMHACK_BASE_URL
        ))
        .header("Accept", "application/json")
        .query(&[("user_id", user_id.to_string())])
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "Failed to get tier progress for user {}: {}",
            user_id,
            response.status()
        ));
    }

    Ok(response.json().await?)
}

// Real schema confirmed against the live API (class.har, captured on the
// class-tracker page). GET /api/v1/wargame/classes/tracked/ works without
// auth (returns all-zero progress anonymously) but personalizes with the
// session cookie, same pattern as challenge listings. `tracked_class` is
// the next incomplete level in that category - None once every level is
// finished.
#[derive(Debug, Deserialize)]
pub struct ClassTierBreakdown {
    pub tier: i32,
    pub completed: i32,
    pub total: i32,
}

#[derive(Debug, Deserialize)]
pub struct TrackedClass {
    pub category: String,
    pub level: i32,
    pub description: String,
    pub cnt_challenges: i32,
    pub cnt_completed: i32,
    pub tier_breakdown: Vec<ClassTierBreakdown>,
}

#[derive(Debug, Deserialize)]
pub struct CategoryTrack {
    pub category: String,
    pub owned_level: i32,
    pub tracked_class: Option<TrackedClass>,
}

#[derive(Debug, Deserialize)]
pub struct TrackedClasses {
    pub categories: Vec<CategoryTrack>,
}

pub async fn get_tracked_classes() -> Result<TrackedClasses> {
    let client = client()?;

    let request = client
        .get(format!("{}/api/v1/wargame/classes/tracked/", DREAMHACK_BASE_URL))
        .header("Accept", "application/json");

    let response = with_optional_session(request)?.send().await?;

    if !response.status().is_success() {
        return Err(anyhow!("Failed to get class progress: {}", response.status()));
    }

    Ok(response.json().await?)
}

// GET /api/v1/wargame/classes/{category}-{level}/challenges/ - confirmed
// public (no auth) via curl, same Challenge shape as the main wargame list.
pub async fn get_class_challenges(category: &str, level: i32) -> Result<Vec<Challenge>> {
    let client = client()?;

    let request = client
        .get(format!(
            "{}/api/v1/wargame/classes/{}-{}/challenges/",
            DREAMHACK_BASE_URL, category, level
        ))
        .header("Accept", "application/json");

    let response = with_optional_session(request)?.send().await?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "Failed to get challenges for class {}-{}: {}",
            category,
            level,
            response.status()
        ));
    }

    Ok(response.json().await?)
}

#[derive(Debug, Deserialize)]
struct ErrorDetail {
    detail: String,
}

// Real behavior confirmed against the live API (download.har):
// POST /api/v1/wargame/challenges/{id}/download/ requires an authenticated
// session (401 "자격 인증 데이터가 제공되지 않았습니다." with no cookie at all)
// and sends X-CSRFToken, unlike the anonymous login endpoints. On success
// the body is a bare JSON string - a presigned object-storage URL valid for
// 24h - which we then fetch directly to get the actual challenge zip.
pub async fn download_challenge(challenge_id: i64) -> Result<Vec<u8>> {
    let session = get_session()?
        .ok_or_else(|| anyhow!("Not logged in. Please run 'lucid login' first."))?;
    let csrf_token = get_csrf_token()?
        .ok_or_else(|| anyhow!("No CSRF token saved. Please run 'lucid login' again."))?;

    let client = client()?;
    let response = client
        .post(format!(
            "{}/api/v1/wargame/challenges/{}/download/",
            DREAMHACK_BASE_URL, challenge_id
        ))
        .header("Accept", "application/json")
        .header("Cookie", &session)
        .header("X-CSRFToken", &csrf_token)
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let detail = response
            .json::<ErrorDetail>()
            .await
            .map(|e| e.detail)
            .unwrap_or_else(|_| status.to_string());
        return Err(anyhow!("Failed to get download link: {detail}"));
    }

    let download_url: String = response.json().await?;

    let file_response = client.get(&download_url).send().await?;
    if !file_response.status().is_success() {
        return Err(anyhow!(
            "Failed to download challenge file: {}",
            file_response.status()
        ));
    }

    Ok(file_response.bytes().await?.to_vec())
}

#[derive(Debug, Serialize)]
struct FlagSubmission<'a> {
    flag: &'a str,
}

// Real behavior confirmed via curl against the live API:
// POST /api/v1/wargame/challenges/{id}/auth/ {"flag": "..."} requires
// session + X-CSRFToken like download. A wrong flag comes back as DRF's
// standard field-error shape: 400 {"flag": ["Wrong flag."]}. The success
// shape wasn't verified (would require actually solving a challenge to
// check), so success is judged purely by HTTP status - any 2xx is treated
// as correct.
#[derive(Debug, Deserialize)]
struct FlagErrorDetail {
    flag: Option<Vec<String>>,
    detail: Option<String>,
}

pub async fn submit_flag(challenge_id: i64, flag: &str) -> Result<()> {
    let session = get_session()?
        .ok_or_else(|| anyhow!("Not logged in. Please run 'lucid login' first."))?;
    let csrf_token = get_csrf_token()?
        .ok_or_else(|| anyhow!("No CSRF token saved. Please run 'lucid login' again."))?;

    let client = client()?;
    let response = client
        .post(format!(
            "{}/api/v1/wargame/challenges/{}/auth/",
            DREAMHACK_BASE_URL, challenge_id
        ))
        .header("Accept", "application/json")
        .header("Cookie", &session)
        .header("X-CSRFToken", &csrf_token)
        .json(&FlagSubmission { flag })
        .send()
        .await?;

    let status = response.status();
    if status.is_success() {
        return Ok(());
    }

    let message = response
        .json::<FlagErrorDetail>()
        .await
        .ok()
        .and_then(|e| e.flag.map(|v| v.join(" ")).or(e.detail))
        .unwrap_or_else(|| status.to_string());
    Err(anyhow!("{message}"))
}

// Real schema confirmed against the live API (vm_gen.har + follow-up
// read-only/teardown curl checks - no VM was ever created by this
// exploration, only inspected/torn down, so no credits were spent
// verifying this):
// GET /api/v1/user/vm-credits/ -> {"balance": N, "has_unlimited_credits": bool}
#[derive(Debug, Deserialize)]
pub struct VmCredits {
    pub balance: i64,
    pub has_unlimited_credits: bool,
}

pub async fn get_vm_credits() -> Result<VmCredits> {
    let session = get_session()?
        .ok_or_else(|| anyhow!("Not logged in. Please run 'lucid login' first."))?;
    let csrf_token = get_csrf_token()?.unwrap_or_default();

    let client = client()?;
    let response = client
        .get(format!("{}/api/v1/user/vm-credits/", DREAMHACK_BASE_URL))
        .header("Accept", "application/json")
        .header("Cookie", &session)
        .header("X-CSRFToken", &csrf_token)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow!("Failed to get VM credits: {}", response.status()));
    }

    Ok(response.json().await?)
}

// GET /api/v1/wargame/challenges/{id}/live/ -> full status object if a VM
// is running, or a bare `{}` if not (confirmed via curl after tearing one
// down) - so an empty object means "no VM", not an error.
#[derive(Debug, Deserialize)]
pub struct VmStatus {
    pub id: String,
    pub state: String,
    pub host: String,
    pub starttime: String,
    pub endtime: String,
    /// Each entry is (protocol, port, port) exactly as the API returns it -
    /// which number is the externally-reachable one wasn't confirmed, so
    /// it's left unlabeled rather than guessed at.
    pub port_mappings: Vec<(String, i32, i32)>,
}

pub async fn get_vm_status(challenge_id: i64) -> Result<Option<VmStatus>> {
    let session = get_session()?
        .ok_or_else(|| anyhow!("Not logged in. Please run 'lucid login' first."))?;
    let csrf_token = get_csrf_token()?.unwrap_or_default();

    let client = client()?;
    let response = client
        .get(format!(
            "{}/api/v1/wargame/challenges/{}/live/",
            DREAMHACK_BASE_URL, challenge_id
        ))
        .header("Accept", "application/json")
        .header("Cookie", &session)
        .header("X-CSRFToken", &csrf_token)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow!("Failed to get VM status: {}", response.status()));
    }

    // Confirmed via curl: challenges with no VM feature at all (e.g. a
    // plain reversing challenge, not needs_vm) return 200 with a
    // completely empty body here - not even `{}` - which isn't valid JSON
    // on its own, so that has to be checked before attempting to parse.
    let text = response.text().await?;
    if text.trim().is_empty() {
        return Ok(None);
    }

    let value: serde_json::Value = serde_json::from_str(&text)?;
    if value.as_object().is_some_and(|o| o.is_empty()) {
        return Ok(None);
    }
    Ok(Some(serde_json::from_value(value)?))
}

#[derive(Debug, Deserialize)]
struct VmCreateResponse {
    vmid: String,
}

/// Spins up a real remote VM and **spends one VM credit** (confirmed via
/// vm_gen.har: POST returns 201 `{"vmid": "..."}`, and the account's
/// vm-credits balance is what it's billed against). Only call this in
/// direct response to an explicit user request - never speculatively, and
/// never from a test/verification path.
pub async fn create_vm(challenge_id: i64) -> Result<String> {
    let session = get_session()?
        .ok_or_else(|| anyhow!("Not logged in. Please run 'lucid login' first."))?;
    let csrf_token = get_csrf_token()?
        .ok_or_else(|| anyhow!("No CSRF token saved. Please run 'lucid login' again."))?;

    let client = client()?;
    let response = client
        .post(format!(
            "{}/api/v1/wargame/challenges/{}/live/",
            DREAMHACK_BASE_URL, challenge_id
        ))
        .header("Accept", "application/json")
        .header("Cookie", &session)
        .header("X-CSRFToken", &csrf_token)
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let detail = response
            .json::<ErrorDetail>()
            .await
            .map(|e| e.detail)
            .unwrap_or_else(|_| status.to_string());
        return Err(anyhow!("Failed to start VM: {detail}"));
    }

    Ok(response.json::<VmCreateResponse>().await?.vmid)
}

// DELETE /api/v1/wargame/challenges/{id}/live/ -> 204, confirmed free
// (tearing down doesn't cost a credit - only creating does).
pub async fn stop_vm(challenge_id: i64) -> Result<()> {
    let session = get_session()?
        .ok_or_else(|| anyhow!("Not logged in. Please run 'lucid login' first."))?;
    let csrf_token = get_csrf_token()?
        .ok_or_else(|| anyhow!("No CSRF token saved. Please run 'lucid login' again."))?;

    let client = client()?;
    let response = client
        .delete(format!(
            "{}/api/v1/wargame/challenges/{}/live/",
            DREAMHACK_BASE_URL, challenge_id
        ))
        .header("Accept", "application/json")
        .header("Cookie", &session)
        .header("X-CSRFToken", &csrf_token)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow!("Failed to stop VM: {}", response.status()));
    }

    Ok(())
}