use std::path::PathBuf;

pub const VERSION: &str = "0.1.0";

pub fn data_dir() -> anyhow::Result<PathBuf> {
    let path = match std::env::var_os("STUDYPILOT_DATA_DIR") {
        Some(value) => PathBuf::from(value),
        None => std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .ok_or_else(|| anyhow::anyhow!("无法确定用户主目录"))?
            .join(".studypilot"),
    };
    Ok(path)
}

pub fn repository_root() -> Option<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .map(PathBuf::from)
        .filter(|path| path.join("studypilot/adapters/__init__.py").is_file())
        .or_else(|| {
            let executable = std::env::current_exe().ok()?;
            let directory = executable.parent()?;
            [
                directory.join("python"),
                directory.join("resources"),
                directory.join("resources").join("python"),
                directory.to_owned(),
                directory.join(".."),
            ]
            .into_iter()
            .find(|path| path.join("studypilot/adapters/__init__.py").is_file())
        })
}
