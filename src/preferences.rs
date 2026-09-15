//! Small non-authoritative UI preferences, deliberately outside editor history.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{ErrorKind, Read},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Preferences {
    pub guide_dismissed: bool,
    pub guide_started: bool,
    pub guide_step: u8,
    pub hide_tooltips: bool,
    pub hide_demos: bool,
    pub strip_placed: bool,
    pub setup_complete: bool,
    pub verified_address: Option<std::net::Ipv4Addr>,
}

impl Preferences {
    pub fn path_for(config: &Path) -> Result<PathBuf> {
        let mut name = config
            .file_name()
            .context("Setup needs a filename")?
            .to_os_string();
        name.push(".ui.json");
        Ok(config.with_file_name(name))
    }

    pub fn load(path: &Path) -> Result<Self> {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(error).context("Cannot read guide preferences"),
        };
        let mut bytes = Vec::new();
        file.take(4097)
            .read_to_end(&mut bytes)
            .context("Cannot read guide preferences")?;
        ensure!(bytes.len() <= 4096, "Guide preferences exceed 4 KiB");
        let preferences: Self =
            serde_json::from_slice(&bytes).context("Invalid guide preferences")?;
        ensure!(preferences.guide_step < 4, "Invalid guide step");
        Ok(preferences)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        ensure!(self.guide_step < 4, "Invalid guide step");
        crate::config::write_atomic(path, &serde_json::to_vec_pretty(self)?)
    }
}
