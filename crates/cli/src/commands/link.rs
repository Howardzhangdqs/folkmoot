//! link / unlink：安装三字母短命令 flm（~/.local/bin 符号链接，§5.2）

use std::path::PathBuf;

use anyhow::{bail, Context, Result};

pub fn link(name: &str) -> Result<()> {
    let exe = std::env::current_exe().context("current_exe")?;
    let dir = local_bin();
    std::fs::create_dir_all(&dir)?;
    let link_path = dir.join(name);

    // PATH 同名冲突检测（§5.2）
    if let Some(existing) = which_in_path(name) {
        if existing != link_path {
            bail!("'{name}' already exists in PATH at {}", existing.display());
        }
    }
    if link_path.exists() || link_path.symlink_metadata().is_ok() {
        // 已存在：仅当已指向本二进制时视为幂等
        if std::fs::read_link(&link_path).map(|t| t == exe).unwrap_or(false) {
            println!("{name} already linked to {}", exe.display());
            return Ok(());
        }
        bail!("{} exists and does not point to this binary", link_path.display());
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(&exe, &link_path).context("create symlink")?;
    #[cfg(not(unix))]
    bail!("link is only supported on Unix; on Windows, copy the binary as {name}.exe");

    println!("linked {} -> {}", link_path.display(), exe.display());
    if which_in_path(name).is_none() {
        eprintln!("note: {} is not in PATH; add it to use `{name}`", dir.display());
    }
    Ok(())
}

pub fn unlink(name: &str) -> Result<()> {
    let link_path = local_bin().join(name);
    let exe = std::env::current_exe()?;
    if std::fs::read_link(&link_path).map(|t| t == exe).unwrap_or(false) {
        std::fs::remove_file(&link_path)?;
        println!("removed {}", link_path.display());
    } else {
        bail!("{} does not point to this binary; refusing to remove", link_path.display());
    }
    Ok(())
}

fn local_bin() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".local/bin")
}

fn which_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let p = dir.join(name);
        if p.is_file() || p.symlink_metadata().is_ok() {
            return Some(p);
        }
    }
    None
}
