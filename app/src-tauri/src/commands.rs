//! Tauri commands: thin calls on `AppCore`. Errors reach the UI as strings.
//!
//! Every command is async and runs its store work on a blocking thread, so a slow SQLite
//! query (or the store lock held by a transfer saving segments) never freezes the window.

use crate::core::{AppCore, FolderTarget};
use crate::dto::{
    Category, CollectorFilters, CollectorSnapshot, Destination, IntakeResult, PostProcSettings, Rule, RulesConfig, Run,
    Snapshot, SpeedLimit, StartResult,
};
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
pub async fn add_links(core: State<'_, AppCore>, text: String, source: String) -> R<IntakeResult> {
    blocking(&core, move |c| c.add_links(&text, &source).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn collector_snapshot(core: State<'_, AppCore>) -> R<CollectorSnapshot> {
    blocking(&core, |c| c.collector_snapshot().map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn collector_start(core: State<'_, AppCore>, ids: Vec<i64>) -> R<StartResult> {
    blocking(&core, move |c| c.collector_start(&ids).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn collector_remove(core: State<'_, AppCore>, ids: Vec<i64>) -> R<()> {
    blocking(&core, move |c| c.collector_remove(&ids).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn collector_recheck(core: State<'_, AppCore>, ids: Vec<i64>) -> R<()> {
    blocking(&core, move |c| c.collector_recheck(&ids).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn set_collector_filters(core: State<'_, AppCore>, filters: CollectorFilters) -> R<()> {
    blocking(&core, move |c| c.set_collector_filters(filters).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn set_watch_clipboard(core: State<'_, AppCore>, on: bool) -> R<()> {
    blocking(&core, move |c| c.set_watch_clipboard(on).map_err(|e| e.to_string())).await
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

#[tauri::command]
pub async fn postproc_runs(core: State<'_, AppCore>) -> R<Vec<Run>> {
    blocking(&core, |c| c.postproc_runs().map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn postproc_process(core: State<'_, AppCore>, package_id: Option<i64>, download_id: Option<i64>) -> R<i64> {
    blocking(&core, move |c| c.postproc_process(package_id, download_id).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn postproc_retry(core: State<'_, AppCore>, run_id: i64) -> R<()> {
    blocking(&core, move |c| c.postproc_retry(run_id).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn postproc_password(core: State<'_, AppCore>, run_id: i64, password: String, remember: bool) -> R<()> {
    blocking(&core, move |c| c.postproc_password(run_id, password, remember).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn set_expected_hash(core: State<'_, AppCore>, id: i64, hash: Option<String>) -> R<()> {
    blocking(&core, move |c| c.set_expected_hash(id, hash.as_deref()).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn rules_config(core: State<'_, AppCore>) -> R<RulesConfig> {
    blocking(&core, |c| c.rules_config().map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn set_postproc_settings(core: State<'_, AppCore>, settings: PostProcSettings) -> R<()> {
    blocking(&core, move |c| c.set_postproc_settings(settings).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn set_categories(core: State<'_, AppCore>, categories: Vec<Category>) -> R<()> {
    blocking(&core, move |c| c.set_categories(categories).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn set_rules(core: State<'_, AppCore>, rules: Vec<Rule>) -> R<()> {
    blocking(&core, move |c| c.set_rules(rules).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn try_rules(
    core: State<'_, AppCore>,
    name: String,
    host: String,
    size: Option<u64>,
    rules: Vec<Rule>,
    categories: Vec<Category>,
    sort: bool,
) -> R<Destination> {
    blocking(&core, move |c| c.try_rules(&name, &host, size, &rules, &categories, sort).map_err(|e| e.to_string()))
        .await
}

#[tauri::command]
pub async fn add_password(core: State<'_, AppCore>, password: String) -> R<i64> {
    blocking(&core, move |c| c.add_password(&password).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn remove_password(core: State<'_, AppCore>, id: i64) -> R<()> {
    blocking(&core, move |c| c.remove_password(id).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn order_passwords(core: State<'_, AppCore>, ids: Vec<i64>) -> R<()> {
    blocking(&core, move |c| c.order_passwords(&ids).map_err(|e| e.to_string())).await
}
