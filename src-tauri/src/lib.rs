//! OMP Desktop Host — Tauri application entrypoint.

mod acp_client;
mod agent_memory;
mod agents_catalog;
mod agent_prefs;
mod app_update;
mod updater;
mod agent_subagents;
mod extensions;
mod hooks;
mod commands;
mod support_bundle;
mod editors;
mod error;
mod fs_browser;
mod media_protocol;
mod path_scope;
mod portability;
mod mirror;
mod mock_acp;
mod models_catalog;
mod omp_desktop_v1;
mod paths;
mod process_util;
mod process_limits;
mod proxy;
mod journal_throttle;
mod logging;
mod stream_emit;
mod stream_stall;
mod tool_heartbeat;
mod trace;
mod turn_complete;
mod store_lock;
mod permission;
mod project_rules;
mod permission_rules;
mod providers;
mod secrets;
mod session_import;
mod session_content_search;
mod session_title;
#[cfg(test)]
mod permission_host_test;
#[cfg(test)]
mod integration_test;
#[cfg(test)]
mod acp_golden_test;
mod session_fsm;
mod session_manager;
mod store;
mod tray;
mod tray_i18n;
mod dialog_i18n;
#[cfg(windows)]
mod win_shell;
mod voice_host;
mod voice_stt;
mod voice_tools;
mod remote_im;
mod runtime_availability;
mod supervisor;
mod event_journal;

use std::sync::Arc;

use mirror::MirrorHost;
use omp_desktop_v1::OmpExtension;
use session_manager::SessionManager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = paths::ensure_app_dirs();
    logging::init();
    // §8.2: idempotent credential migration into the OS secure store. A store
    // outage sets the Safe Mode flag in the ledger and leaves plaintext
    // untouched; per-entry failures roll back and retry next launch.
    {
        let store = secrets::store::default_store();
        secrets::migration::run_startup_migration(store.as_ref());
    }
    // Windows: AppUserModelID before window/taskbar so Show Desktop / jump lists
    // treat us as a normal app (matches NSIS shortcut AUMID).
    #[cfg(windows)]
    win_shell::set_process_app_user_model_id();

    let session_mgr = Arc::new(SessionManager::new());
    let mirror_host = Arc::new(MirrorHost::from_env());
    let omp_extension = Arc::new(OmpExtension::new());
    let voice_host = Arc::new(voice_host::VoiceHost::new());
    let remote_im_state = Arc::new(remote_im::RemoteImState {
        inner: tokio::sync::Mutex::new(remote_im::BridgeRuntime::default()),
    });

    // Attach `tauri-plugin-updater` only when release CI injected OMP_DESKTOP_UPDATER_*
    // (build.rs → cfg) and this is a non-debug binary. Crate is always linked for ACL.
    fn maybe_register_updater(
        builder: tauri::Builder<tauri::Wry>,
    ) -> tauri::Builder<tauri::Wry> {
        #[cfg(omp_desktop_updater_enabled)]
        {
            if !cfg!(debug_assertions) {
                return builder.plugin(tauri_plugin_updater::Builder::new().build());
            }
        }
        builder
    }

    let builder = tauri::Builder::default()
        // Must be registered first so a second process exits and focuses the primary window.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // Same restore path as tray Open — taskbar + shell styles included.
            tray::show_main_window(app);
        }))
        .plugin(tauri_plugin_store::Builder::new().build())
        // Always register process so release builds can relaunch after install.
        .plugin(tauri_plugin_process::init());

    // Register the updater only in configured release builds; omit it locally.
    // Requires OMP_DESKTOP_UPDATER_* env at compile time (build.rs) + non-debug binary.
    let builder = maybe_register_updater(builder);

    builder
        .manage(session_mgr)
        .manage(mirror_host)
        .manage(omp_extension)
        .manage(voice_host)
        .manage(remote_im_state)
        // Range-capable media streaming (video/audio/pdf) — never loads multi‑GB into RAM.
        .register_asynchronous_uri_scheme_protocol("media", |_ctx, request, responder| {
            std::thread::spawn(move || {
                let response = media_protocol::handle_request(request);
                responder.respond(response);
            });
        })
        // Close button / Alt+F4: hide to tray (default) or quit — Settings → General.
        // Full exit always available via tray "Quit OMP Desktop" or Cmd+Q.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                use tauri::Manager;
                let close_to_tray = store::load_settings().close_to_tray;
                if close_to_tray {
                    api.prevent_close();
                    tray::hide_to_tray(window.app_handle());
                }
                // else: allow default close → process exit
            }
        })
        .setup(|app| {
            crate::path_scope::refresh_from_store();
            use tauri::Manager;
            if let Some(window) = app.get_webview_window("main") {
                #[cfg(target_os = "macos")]
                {
                    // Transparent layers so CSS backdrop-filter / native vibrancy show through.
                    let _ = window.set_background_color(Some(tauri::window::Color(0, 0, 0, 0)));
                    // Frosted glass under transparent regions (sidebar). Solid main CSS covers the rest.
                    use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial};
                    if let Err(e) = apply_vibrancy(
                        &window,
                        NSVisualEffectMaterial::Sidebar,
                        None,
                        Some(16.0),
                    ) {
                        tracing::warn!("window vibrancy: {e}");
                    }
                }
                // Windows / others: solid base matching dark theme (avoids white flash / WebView2 glitches).
                #[cfg(not(target_os = "macos"))]
                {
                    let _ = window.set_background_color(Some(tauri::window::Color(13, 13, 13, 255)));
                }
                // Windows: frameless + tray skip_taskbar can leave the HWND out of
                // Explorer's Show Desktop set when it is the only visible window.
                #[cfg(windows)]
                win_shell::ensure_main_window_shell_integration(&window);
            }
            // Menu-bar / system tray — logo.svg tray icon (not dock app icon)
            if let Err(e) = tray::setup_tray(app.handle()) {
                tracing::warn!("tray setup: {e}");
            }
            // I03: recycle idle agent processes; session metadata stays on disk.
            // I06: surface cancel UI when a stream is pure-silent for too long.
            {
                use tauri::Manager;
                let mgr = app.state::<Arc<SessionManager>>().inner().clone();
                mgr.start_idle_watchdog(app.handle().clone());
                mgr.start_stream_stall_watchdog(app.handle().clone());
            }
            // Remote IM: restore Feishu/Weixin connectors after App restart so
            // already-bound channels keep receiving messages without a manual Start.
            {
                use tauri::Manager;
                remote_im::set_app_handle(app.handle().clone());
                let rim = app.state::<Arc<remote_im::RemoteImState>>().inner().clone();
                tauri::async_runtime::spawn(async move {
                    remote_im::try_autostart(&rim).await;
                });
            }
            // Headless mirror auto-start (GROK_MIRROR_HEADLESS=1) — off by default.
            {
                use tauri::Manager;
                let host = app.state::<Arc<MirrorHost>>().inner().clone();
                let mgr = app.state::<Arc<SessionManager>>().inner().clone();
                mirror::maybe_autostart(host, app.handle().clone(), mgr);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::session_get_state,
            commands::session_connect,
            commands::session_send,
            commands::session_interject,
            commands::session_stop,
            commands::session_disconnect,
            commands::session_reattach,
            commands::session_resolve_permission,
            commands::session_resolve_plan,
            commands::session_resolve_ask_user,
            commands::acp_test_connection,
            commands::pick_cli_binary,
            commands::open_external_url,
            commands::app_check_update,
            updater::is_auto_update_supported,
            updater::is_updater_plugin_enabled,
            updater::updater_status,
            updater::prepare_for_app_update,
            commands::voice_status,
            commands::voice_transcribe,
            commands::projects_list,
            commands::project_add,
            commands::project_add_dialog,
            commands::project_remove,
            commands::project_relocate,
            commands::project_trust,
            commands::project_set_permission_policy,
            commands::project_rename,
            commands::project_set_pinned,
            commands::project_reveal,
            commands::project_rules_list,
            commands::project_rules_ensure_template,
            commands::project_archive_sessions,
            commands::sessions_list,
            commands::sessions_search,
            commands::session_create,
            commands::session_delete,
            commands::session_rename,
            commands::session_set_archived,
            commands::session_set_pinned,
            commands::session_set_project,
            commands::session_set_scheduled,
            commands::session_messages,
            commands::session_media_root,
            commands::session_resolve_relative_media,
            commands::settings_get,
            commands::store_take_quarantine,
            commands::settings_set,
            commands::memory_clear,
            commands::settings_remember_last_session,
            commands::models_list_available,
            commands::agents_catalog,
            commands::composer_prefs_resolve,
            commands::composer_prefs_set,
            commands::session_set_policy,
            commands::permission_rules_get,
            commands::permission_rules_set,
            commands::session_set_model,
            commands::session_rewind_drop_last_user,
            commands::session_rewind_points,
            commands::session_rewind_execute,
            commands::session_fork,
            commands::secrets_get_masked,
            commands::secrets_set,
            commands::provider_ping,
            commands::import_grok_cli_config,
            commands::import_grok_go_config,
            commands::doctor_report,
            commands::network_probe,
            commands::agents_recycle_all,
            commands::cli_doctor_fix,
            commands::export_support_bundle,
            commands::export_session_bundle,
            commands::session_trace_export,
            commands::reset_app_data,
            commands::skills_list,
            commands::agents_list,
            commands::inspect_mcp,
            commands::project_inspect,
            commands::omp_desktop_v1_capability,
            commands::extensions_get,
            commands::extensions_set_mcp,
            commands::extensions_set_skill,
            commands::extensions_enable_all_mcp,
            commands::extensions_enable_all_skills,
            commands::mcp_add,
            commands::mcp_remove,
            commands::mcp_doctor,
            commands::plugins_list,
            commands::plugin_enable,
            commands::plugin_disable,
            commands::plugin_uninstall,
            commands::plugin_details,
            commands::plugin_install,
            commands::plugin_update,
            commands::hooks_list,
            commands::hooks_reveal,
            commands::hooks_open_dir,
            commands::hooks_ensure_dir,
            commands::setup_preview,
            commands::setup_install,
            commands::marketplace_list,
            commands::marketplace_available,
            commands::marketplace_add,
            commands::marketplace_remove,
            commands::marketplace_update,
            commands::leader_list,
            commands::leader_kill_all,
            commands::pick_directory,
            commands::pick_attach_files,
            commands::pick_attach_folder,
            commands::save_temp_attachment,
            commands::clipboard_paste_image,
            commands::paths_classify,
            commands::path_open,
            commands::path_reveal,
            commands::git_file_diff,
            commands::git_status,
            commands::git_worktrees_list,
            commands::git_worktree_add,
            commands::git_worktree_remove,
            commands::git_worktree_gc,
            commands::git_show_file,
            commands::fs_list_dir,
            commands::fs_read_file,
            commands::fs_write_file,
            commands::fs_write_absolute,
            tray::tray_refresh,
            commands::fs_read_absolute,
            commands::fs_open_path,
            commands::session_auto_title,
            commands::automations_list,
            commands::automation_create,
            commands::automation_update,
            commands::automation_set_enabled,
            commands::automation_mark_run,
            commands::automation_delete,
            commands::session_import_transcript,
            commands::session_import_transcript_file,
            commands::session_export_portable,
            commands::session_import_portable,
            commands::providers_list,
            commands::providers_upsert,
            commands::providers_remove,
            commands::providers_set_default,
            commands::providers_activate,
            commands::providers_ping,
            commands::providers_list_models,
            commands::editors_list,
            commands::open_in_editor,
            mirror::mirror_status,
            mirror::mirror_rotate_token,
            mirror::mirror_set_read_only,
            mirror::mirror_start,
            mirror::mirror_stop,
            voice_host::voice_state,
            voice_host::voice_start,
            voice_host::voice_stop,
            voice_host::voice_push_pcm,
            voice_host::voice_invoke_tool,
            voice_host::voice_dictation_transcribe,
            remote_im::remote_im_bridge_status,
            remote_im::remote_im_bridge_start,
            remote_im::remote_im_bridge_stop,
            remote_im::remote_im_bridge_set_config,
            remote_im::remote_im_bridge_reload,
            remote_im::remote_im_test_connection,
            remote_im::remote_im_scan_begin,
            remote_im::remote_im_scan_poll,
            remote_im::remote_im_list_instances,
            remote_im::remote_im_save_instance,
            remote_im::remote_im_delete_instance,
            remote_im::remote_im_doctor,
            runtime_availability::runtime_availability,
        ])
        .build(tauri::generate_context!())
        .expect("error while building OMP Desktop")
        .run(|app, event| {
            // macOS: click Dock icon when all windows hidden → show main window again.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen {
                has_visible_windows,
                ..
            } = event
            {
                if !has_visible_windows {
                    tray::show_main_window(app);
                }
            }
            // Full exit (tray Quit / Cmd+Q): tear down mirror host + cloudflared group.
            if let tauri::RunEvent::Exit = event {
                use tauri::Manager;
                if let Some(host) = app.try_state::<Arc<MirrorHost>>() {
                    host.inner().stop_sync();
                }
            }
            let _ = (app, &event);
        });
}

/// Test-only registry of all Tauri command names registered in [`run`].
///
/// Kept in sync with the `generate_handler!` list above. The command-surface
/// regression test uses this to assert that legacy CLI/account/quota
/// commands have been removed.
#[cfg(test)]
fn registered_command_names() -> &'static [&'static str] {
    &[
        // session
        "session_get_state",
        "session_connect",
        "session_send",
        "session_interject",
        "session_stop",
        "session_disconnect",
        "session_reattach",
        "session_resolve_permission",
        "session_resolve_plan",
        "session_resolve_ask_user",
        "acp_test_connection",
        "pick_cli_binary",
        "open_external_url",
        "app_check_update",
        "is_auto_update_supported",
        "is_updater_plugin_enabled",
        "updater_status",
        "prepare_for_app_update",
        "voice_status",
        "voice_transcribe",
        // projects
        "projects_list",
        "project_add",
        "project_add_dialog",
        "project_remove",
        "project_relocate",
        "project_trust",
        "project_set_permission_policy",
        "project_rename",
        "project_set_pinned",
        "project_reveal",
        "project_rules_list",
        "project_rules_ensure_template",
        "project_archive_sessions",
        "sessions_list",
        "sessions_search",
        "session_create",
        "session_delete",
        "session_rename",
        "session_set_archived",
        "session_set_pinned",
        "session_set_project",
        "session_set_scheduled",
        "session_messages",
        "session_media_root",
        "session_resolve_relative_media",
        "settings_get",
        "store_take_quarantine",
        "settings_set",
        "memory_clear",
        "settings_remember_last_session",
        "models_list_available",
        "agents_catalog",
        "composer_prefs_resolve",
        "composer_prefs_set",
        "session_set_policy",
        "permission_rules_get",
        "permission_rules_set",
        "session_set_model",
        "session_rewind_drop_last_user",
        "session_rewind_points",
        "session_rewind_execute",
        "session_fork",
        "secrets_get_masked",
        "secrets_set",
        "provider_ping",
        "import_grok_cli_config",
        "import_grok_go_config",
        "doctor_report",
        "network_probe",
        "agents_recycle_all",
        "cli_doctor_fix",
        "export_support_bundle",
        "export_session_bundle",
        "session_trace_export",
        "reset_app_data",
        "skills_list",
        "agents_list",
        "inspect_mcp",
        "project_inspect",
        "omp_desktop_v1_capability",
        "extensions_get",
        "extensions_set_mcp",
        "extensions_set_skill",
        "extensions_enable_all_mcp",
        "extensions_enable_all_skills",
        "mcp_add",
        "mcp_remove",
        "mcp_doctor",
        "plugins_list",
        "plugin_enable",
        "plugin_disable",
        "plugin_uninstall",
        "plugin_details",
        "plugin_install",
        "plugin_update",
        "hooks_list",
        "hooks_reveal",
        "hooks_open_dir",
        "hooks_ensure_dir",
        "setup_preview",
        "setup_install",
        "marketplace_list",
        "marketplace_available",
        "marketplace_add",
        "marketplace_remove",
        "marketplace_update",
        "leader_list",
        "leader_kill_all",
        "pick_directory",
        "pick_attach_files",
        "pick_attach_folder",
        "save_temp_attachment",
        "clipboard_paste_image",
        "paths_classify",
        "path_open",
        "path_reveal",
        "git_file_diff",
        "git_status",
        "git_worktrees_list",
        "git_worktree_add",
        "git_worktree_remove",
        "git_worktree_gc",
        "git_show_file",
        "fs_list_dir",
        "fs_read_file",
        "fs_write_file",
        "fs_write_absolute",
        "tray_refresh",
        "fs_read_absolute",
        "fs_open_path",
        "session_auto_title",
        "automations_list",
        "automation_create",
        "automation_update",
        "automation_set_enabled",
        "automation_mark_run",
        "automation_delete",
        "session_import_transcript",
        "session_import_transcript_file",
        "providers_list",
        "providers_upsert",
        "providers_remove",
        "providers_set_default",
        "providers_activate",
        "providers_ping",
        "providers_list_models",
        "editors_list",
        "open_in_editor",
        "mirror_status",
        "mirror_rotate_token",
        "mirror_set_read_only",
        "mirror_start",
        "mirror_stop",
        "voice_state",
        "voice_start",
        "voice_stop",
        "voice_push_pcm",
        "voice_invoke_tool",
        "voice_dictation_transcribe",
        "remote_im_bridge_status",
        "remote_im_bridge_start",
        "remote_im_bridge_stop",
        "remote_im_bridge_set_config",
        "remote_im_bridge_reload",
        "remote_im_test_connection",
        "remote_im_scan_begin",
        "remote_im_scan_poll",
        "remote_im_list_instances",
        "remote_im_save_instance",
        "remote_im_delete_instance",
        "remote_im_doctor",
        "runtime_availability",
    ]
}
