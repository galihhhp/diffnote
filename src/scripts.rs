use std::path::PathBuf;

const GIT_UPDATE: &str = include_str!("../scripts/git_update.sh");
const BRANCHUP: &str = include_str!("../scripts/branchup.sh");

pub fn dir() -> PathBuf {
    std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".local/share")
        })
        .join("diffnote/scripts")
}

pub fn ensure() -> std::io::Result<PathBuf> {
    let dir = dir();
    std::fs::create_dir_all(&dir)?;
    for (name, content) in [("git_update.sh", GIT_UPDATE), ("branchup.sh", BRANCHUP)] {
        let path = dir.join(name);
        let current = std::fs::read_to_string(&path).unwrap_or_default();
        if current != content {
            std::fs::write(&path, content)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
            }
        }
    }
    Ok(dir)
}

pub fn command_path(token: &str) -> Option<PathBuf> {
    let dir = ensure().ok()?;
    let name = match token {
        "{update_script}" => "git_update.sh",
        "{branchup_script}" => "branchup.sh",
        _ => return None,
    };
    Some(dir.join(name))
}
