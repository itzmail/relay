use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

pub fn detect_installed() -> Vec<&'static str> {
    ["claude", "codex", "copilot"]
        .iter()
        .copied()
        .filter(|bin| which(bin))
        .collect()
}

pub fn install_claude(url: &str, path: &Path, dry_run: bool) -> Result<String> {
    let mut root: Value = if path.exists() {
        serde_json::from_str(&fs::read_to_string(path)?)
            .context("invalid JSON in existing .mcp.json")?
    } else {
        json!({})
    };
    let obj = root
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!(".mcp.json root must be object"))?;
    let servers = obj.entry("mcpServers").or_insert_with(|| json!({}));
    servers["relay"] = json!({ "url": url });

    let pretty = serde_json::to_string_pretty(&root)?;
    if !dry_run {
        write_atomic(path, &pretty)?;
    }
    Ok(pretty)
}

pub fn install_codex(url: &str, path: &Path, dry_run: bool) -> Result<String> {
    use toml_edit::{DocumentMut, Item, Table, value};

    let mut doc: DocumentMut = if path.exists() {
        fs::read_to_string(path)?
            .parse()
            .context("invalid TOML in ~/.codex/config.toml")?
    } else {
        DocumentMut::new()
    };

    let mcp = doc
        .entry("mcp_servers")
        .or_insert(Item::Table(Table::new()));
    let mcp_tbl = mcp
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("[mcp_servers] is not a table"))?;
    mcp_tbl.set_implicit(true);

    let mut relay = Table::new();
    relay["url"] = value(url);
    mcp_tbl.insert("relay", Item::Table(relay));

    let rendered = doc.to_string();
    if !dry_run {
        write_atomic(path, &rendered)?;
    }
    Ok(rendered)
}

pub fn install_copilot(url: &str, path: &Path, dry_run: bool) -> Result<String> {
    let mut root: Value = if path.exists() {
        serde_json::from_str(&fs::read_to_string(path)?)
            .context("invalid JSON in ~/.copilot/mcp-config.json")?
    } else {
        json!({})
    };
    let obj = root
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("root must be object"))?;
    let servers = obj.entry("mcpServers").or_insert_with(|| json!({}));
    servers["relay"] = json!({
        "type": "http",
        "url": url,
        "tools": ["*"]
    });

    let pretty = serde_json::to_string_pretty(&root)?;
    if !dry_run {
        write_atomic(path, &pretty)?;
    }
    Ok(pretty)
}

pub fn uninstall_claude(path: &Path, dry_run: bool) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let mut root: Value = serde_json::from_str(&fs::read_to_string(path)?)
        .context("invalid JSON in existing .mcp.json")?;
    let removed = root
        .as_object_mut()
        .and_then(|obj| obj.get_mut("mcpServers"))
        .and_then(|s| s.as_object_mut())
        .map(|servers| servers.remove("relay").is_some())
        .unwrap_or(false);
    if removed && !dry_run {
        write_atomic(path, &serde_json::to_string_pretty(&root)?)?;
    }
    Ok(removed)
}

pub fn uninstall_codex(path: &Path, dry_run: bool) -> Result<bool> {
    use toml_edit::DocumentMut;
    if !path.exists() {
        return Ok(false);
    }
    let mut doc: DocumentMut = fs::read_to_string(path)?
        .parse()
        .context("invalid TOML in ~/.codex/config.toml")?;
    let removed = doc
        .get_mut("mcp_servers")
        .and_then(|t| t.as_table_mut())
        .map(|t| t.remove("relay").is_some())
        .unwrap_or(false);
    if removed && !dry_run {
        write_atomic(path, &doc.to_string())?;
    }
    Ok(removed)
}

pub fn uninstall_copilot(path: &Path, dry_run: bool) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let mut root: Value = serde_json::from_str(&fs::read_to_string(path)?)
        .context("invalid JSON in ~/.copilot/mcp-config.json")?;
    let removed = root
        .as_object_mut()
        .and_then(|obj| obj.get_mut("mcpServers"))
        .and_then(|s| s.as_object_mut())
        .map(|servers| servers.remove("relay").is_some())
        .unwrap_or(false);
    if removed && !dry_run {
        write_atomic(path, &serde_json::to_string_pretty(&root)?)?;
    }
    Ok(removed)
}

fn write_atomic(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, content)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn which(bin: &str) -> bool {
    std::process::Command::new("which")
        .arg(bin)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn install_claude_fresh() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(".mcp.json");
        install_claude("http://localhost:7777/mcp", &path, false).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["mcpServers"]["relay"]["url"], "http://localhost:7777/mcp");
    }

    #[test]
    fn install_claude_merge_preserves_other() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(".mcp.json");
        std::fs::write(&path, r#"{"mcpServers":{"github":{"url":"https://mcp.github.com"}}}"#).unwrap();
        install_claude("http://localhost:7777/mcp", &path, false).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["mcpServers"]["github"]["url"], "https://mcp.github.com");
        assert_eq!(v["mcpServers"]["relay"]["url"], "http://localhost:7777/mcp");
    }

    #[test]
    fn install_claude_idempotent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(".mcp.json");
        install_claude("http://localhost:7777/mcp", &path, false).unwrap();
        install_claude("http://localhost:7777/mcp", &path, false).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let count = v["mcpServers"].as_object().unwrap().len();
        assert_eq!(count, 1);
    }

    #[test]
    fn install_codex_fresh() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        install_codex("http://localhost:7777/mcp", &path, false).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("[mcp_servers.relay]"));
        assert!(content.contains("http://localhost:7777/mcp"));
    }

    #[test]
    fn install_codex_preserves_comments() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "# my comment\nmodel = \"o4-mini\"\n").unwrap();
        install_codex("http://localhost:7777/mcp", &path, false).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("# my comment"));
        assert!(content.contains("[mcp_servers.relay]"));
    }

    #[test]
    fn install_copilot_fresh() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("mcp-config.json");
        install_copilot("http://localhost:7777/mcp", &path, false).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["mcpServers"]["relay"]["type"], "http");
        assert_eq!(v["mcpServers"]["relay"]["url"], "http://localhost:7777/mcp");
    }

    #[test]
    fn dry_run_no_write() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(".mcp.json");
        install_claude("http://localhost:7777/mcp", &path, true).unwrap();
        assert!(!path.exists());
    }
}
