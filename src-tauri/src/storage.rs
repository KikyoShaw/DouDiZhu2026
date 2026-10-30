use std::{fs, path::Path};
use serde::{Deserialize, Serialize};
use crate::session::Settings;

#[derive(Serialize, Deserialize)]
struct SavedSettings { version: u32, settings: Settings }

pub fn load(directory: &Path) -> Settings {
    fs::read(directory.join("settings.json")).ok()
        .and_then(|bytes| serde_json::from_slice::<SavedSettings>(&bytes).ok())
        .filter(|saved| saved.version == 1 && saved.settings.validate().is_ok())
        .map(|saved| saved.settings).unwrap_or_default()
}

pub fn save(directory: &Path, settings: &Settings) -> Result<(), String> {
    settings.validate()?;
    fs::create_dir_all(directory).map_err(|error| format!("无法创建配置目录：{error}"))?;
    let content = serde_json::to_vec_pretty(&SavedSettings { version: 1, settings: settings.clone() })
        .map_err(|error| format!("无法保存设置：{error}"))?;
    let temporary = directory.join("settings.json.tmp");
    fs::write(&temporary, content).map_err(|error| format!("无法写入设置：{error}"))?;
    fs::rename(temporary, directory.join("settings.json"))
        .map_err(|error| format!("无法更新设置：{error}"))
}
