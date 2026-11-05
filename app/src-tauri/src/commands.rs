//! Tauri commands: one-line calls on `AppCore`. Errors reach the UI as strings.

use crate::core::{AppCore, FolderTarget};
use crate::dto::{AddResult, Snapshot, SpeedLimit};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

type R<T> = Result<T, String>;

#[tauri::command]
pub fn snapshot(core: State<'_, AppCore>) -> R<Snapshot> {
    core.snapshot().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_links(core: State<'_, AppCore>, text: String) -> R<AddResult> {
    core.add_links(&text).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn pause(core: State<'_, AppCore>, ids: Vec<i64>) -> R<()> {
    core.pause(&ids);
    Ok(())
}

#[tauri::command]
pub fn resume(core: State<'_, AppCore>, ids: Vec<i64>) -> R<()> {
    core.resume(&ids);
    Ok(())
}

#[tauri::command]
pub fn remove(core: State<'_, AppCore>, ids: Vec<i64>) -> R<()> {
    core.remove(&ids);
    Ok(())
}

#[tauri::command]
pub fn pause_all(core: State<'_, AppCore>) -> R<()> {
    core.pause_all();
    Ok(())
}

#[tauri::command]
pub fn set_priority(core: State<'_, AppCore>, id: i64, priority: String) -> R<()> {
    core.set_priority(id, &priority).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reorder(core: State<'_, AppCore>, ids: Vec<i64>) -> R<()> {
    core.reorder(&ids);
    Ok(())
}

#[tauri::command]
pub fn set_speed_limit(core: State<'_, AppCore>, enabled: bool, bps: u64) -> R<()> {
    core.set_speed_limit(SpeedLimit { enabled, bps }).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_folder(app: AppHandle, core: State<'_, AppCore>, id: i64) -> R<()> {
    match core.folder_target(id).map_err(|e| e.to_string())? {
        FolderTarget::RevealFile(p) => app.opener().reveal_item_in_dir(p),
        FolderTarget::OpenDir(d) => app.opener().open_path(d.to_string_lossy(), None::<&str>),
    }
    .map_err(|e| e.to_string())
}
