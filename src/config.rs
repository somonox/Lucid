use anyhow::{Result, Context};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub session: Option<String>,
    pub csrf_token: Option<String>,
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = get_config_path()?;
        
        if !path.exists() {
            return Ok(Self {
                session: None,
                csrf_token: None,
            });
        }

        let content = fs::read_to_string(&path)
            .context("Failed to read config file")?;
        
        serde_json::from_str(&content)
            .context("Failed to parse config file")
    }

    pub fn save(&self) -> Result<()> {
        let path = get_config_path()?;
        
        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .context("Failed to create config directory")?;
        }

        let content = serde_json::to_string_pretty(self)
            .context("Failed to serialize config")?;
        
        fs::write(&path, content)
            .context("Failed to write config file")?;
        
        Ok(())
    }

    pub fn clear_session() -> Result<()> {
        let mut config = Self::load()?;
        config.session = None;
        config.csrf_token = None;
        config.save()?;
        Ok(())
    }
}

fn get_config_path() -> Result<PathBuf> {
    let proj_dirs = ProjectDirs::from("com", "dreamhack", "lucid")
        .context("Failed to determine config directory")?;
    
    let config_dir = proj_dirs.config_dir();
    Ok(config_dir.join("config.json"))
}

pub fn save_session(session: &str) -> Result<()> {
    let mut config = Config::load()?;
    config.session = Some(session.to_string());
    
    // Extract CSRF token if present in session. DreamHack's cookie is
    // named 'csrf_token' (underscore) - not Django's default 'csrftoken' -
    // confirmed against the live download endpoint.
    if let Some(csrf_match) = session.split(';')
        .find(|s| s.trim().starts_with("csrf_token=")) {
        let csrf_token = csrf_match.trim()
            .strip_prefix("csrf_token=")
            .unwrap_or("")
            .to_string();
        config.csrf_token = Some(csrf_token);
    }
    
    config.save()?;
    Ok(())
}

pub fn get_session() -> Result<Option<String>> {
    let config = Config::load()?;
    Ok(config.session)
}

pub fn get_csrf_token() -> Result<Option<String>> {
    let config = Config::load()?;
    Ok(config.csrf_token)
}