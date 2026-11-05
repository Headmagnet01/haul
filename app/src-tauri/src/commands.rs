//! Tauri commands: thin calls on `AppCore`. Errors reach the UI as strings.
//!
//! Every command is async and runs its store work on a blocking thread, so a slow SQLite
//! query (or the store lock held by a transfer saving segments) never freezes the window.

use crate::core::{AppCore, FolderTarget};
use crate::dto::{AddResult, Snapshot, SpeedLimit};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

type R<T> = Result<T, String>;

async fn blocking<T: Send + 'static>(
    core: &State<'_, AppCore>,
    f: impl FnOnce(AppCore) -> R<T> + Send + 'static,
) -> R<T> {
    let c = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || f(c)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn snapshot(core: State<'_, AppCore>) -> R<Snapshot> {
    blocking(&core, |c| c.snapshot().map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn add_links(core: State<'_, AppCore>, text: String) -> R<AddResult> {
    blocking(&core, move |c| c.add_links(&text).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn pause(core: State<'_, AppCore>, ids: Vec<i64>) -> R<()> {
    blocking(&core, move |c| {
        c.pause(&ids);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn resume(core: State<'_, AppCore>, ids: Vec<i64>) -> R<()> {
    blocking(&core, move |c| {
        c.resume(&ids);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn remove(core: State<'_, AppCore>, ids: Vec<i64>) -> R<()> {
    blocking(&core, move |c| {
        c.remove(&ids);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn pause_all(core: State<'_, AppCore>) -> R<()> {
    blocking(&core, |c| {
        c.pause_all();
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn set_priority(core: State<'_, AppCore>, id: i64, priority: String) -> R<()> {
    blocking(&core, move |c| c.set_priority(id, &priority).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn reorder(core: State<'_, AppCore>, ids: Vec<i64>) -> R<()> {
    blocking(&core, move |c| {
        c.reorder(&ids);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn set_speed_limit(core: State<'_, AppCore>, enabled: bool, bps: u64) -> R<()> {
    blocking(&core, move |c| c.set_speed_limit(SpeedLimit { enabled, bps }).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn open_folder(app: AppHandle, core: State<'_, AppCore>, id: i64) -> R<()> {
    match blocking(&core, move |c| c.folder_target(id).map_err(|e| e.to_string())).await? {
        FolderTarget::RevealFile(p) => app.opener().reveal_item_in_dir(p),
        FolderTarget::OpenDir(d) => app.opener().open_path(d.to_string_lossy(), None::<&str>),
    }
    .map_err(|e| e.to_string())
}
