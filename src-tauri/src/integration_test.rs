//! Integration-style tests on shipped Host helpers (no GUI).
//! Covers project store + independent data root — the path the UI uses.

#[cfg(test)]
mod integration {
    use crate::paths::{app_data_root, ensure_app_dirs};
    use crate::store::{
        add_project, load_projects, load_settings, relocate_project, remove_project, save_settings,
        trust_project, AppSettings,
    };
    use std::fs;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Host store uses a single on-disk projects index. Serialize tests that
    /// mutate it so parallel `cargo test` workers cannot clobber each other
    /// (macOS CI saw `project_relocate` lose its row mid-test).
    static PROJECTS_STORE_LOCK: Mutex<()> = Mutex::new(());

    fn unique_dir() -> std::path::PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("grok-app-itest-{n}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn project_add_trust_remove_roundtrip() {
        let _guard = PROJECTS_STORE_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = unique_dir();
        let path = dir.display().to_string();
        let p = add_project(path.clone(), false).expect("add");
        assert!(!p.trusted);
        assert_eq!(p.path, path);
        assert!(p.path_ok);
        let trusted = trust_project(&p.id).expect("trust");
        assert!(trusted.trusted);
        let list = load_projects();
        assert!(list.iter().any(|x| x.id == p.id && x.trusted));
        remove_project(&p.id).expect("remove");
        let list2 = load_projects();
        assert!(!list2.iter().any(|x| x.id == p.id));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn project_relocate_updates_path_and_path_ok() {
        let _guard = PROJECTS_STORE_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let old_dir = unique_dir();
        let new_dir = unique_dir();
        let old_path = old_dir.display().to_string();
        let new_path = new_dir.display().to_string();
        let p = add_project(old_path.clone(), true).expect("add");
        assert_eq!(p.path, old_path);
        assert!(p.path_ok);

        // Simulate deleted folder: list re-check marks path_ok false.
        let _ = fs::remove_dir_all(&old_dir);
        assert!(
            !std::path::Path::new(&old_path).is_dir(),
            "old path should be gone"
        );
        let listed = load_projects();
        let stale = listed
            .iter()
            .find(|x| x.id == p.id)
            .expect("project row must remain after folder delete");
        assert!(!stale.path_ok);

        let moved = relocate_project(&p.id, new_path.clone()).expect("relocate");
        assert_eq!(moved.path, new_path);
        assert!(moved.path_ok);

        let listed2 = load_projects();
        let again = listed2.iter().find(|x| x.id == p.id).expect("listed");
        assert_eq!(again.path, new_path);
        assert!(again.path_ok);

        assert!(relocate_project(&p.id, "/no/such/dir-xyz".into()).is_err());
        remove_project(&p.id).expect("remove");
        let _ = fs::remove_dir_all(&new_dir);
    }

    #[test]
    fn settings_default_factory_and_disk_roundtrip() {
        let _ = ensure_app_dirs();
        // Factory defaults (not necessarily what's already on disk)
        let factory = AppSettings::default();
        assert_eq!(factory.session_data_mode, "independent");
        assert_eq!(factory.permission_policy, "ask");
        assert_eq!(factory.sandbox_profile, "off");
        assert!(!factory.reopen_last_session);
        assert!(factory.last_session_id.is_none());
        assert!(factory.sidebar_collapsed_project_ids.is_empty());
        // Disk load + save same content must not corrupt
        let s = load_settings();
        save_settings(&s).expect("save");
        let s2 = load_settings();
        assert_eq!(s2.session_data_mode, s.session_data_mode);
        assert_eq!(s2.permission_policy, s.permission_policy);
        assert_eq!(s2.sandbox_profile, s.sandbox_profile);
        assert_eq!(s2.reopen_last_session, s.reopen_last_session);
        let root = app_data_root();
        assert!(!root.as_os_str().is_empty());
    }

    #[test]
    fn command_surface_has_no_grok_product_commands() {
        let commands = crate::registered_command_names();

        // Meta-check: `registered_command_names()` is a manual mirror of the
        // `generate_handler!` list in `lib.rs`. Assert the count so future
        // drift is caught here rather than silently making the absence checks
        // below vacuous. If you add/remove a command in `generate_handler!`,
        // update `registered_command_names()` AND this expected count.
        //
        // Verify at any time with:
        //   sed -n '/invoke_handler(tauri::generate_handler!/,/^\s*])/p' \
        //     src-tauri/src/lib.rs | grep -cE '(commands|updater|tray|mirror|voice_host|remote_im)::'
        const EXPECTED_REGISTERED_COMMAND_COUNT: usize = 171;
        assert_eq!(
            commands.len(),
            EXPECTED_REGISTERED_COMMAND_COUNT,
            "registered_command_names() is out of sync with generate_handler! \
             (expected {EXPECTED_REGISTERED_COMMAND_COUNT}, got {}). Update \
             registered_command_names() in lib.rs to match the current \
             generate_handler! list.",
            commands.len()
        );

        // Every Tauri command removed by the runtime / account backend
        // deletion must stay removed. These names are the actual
        // `commands::<name>` identifiers from the pre-deletion `main` branch's
        // `generate_handler!` block (enumerated via `comm -23` between main and
        // this branch), not module names or guesswork.
        for removed in [
            // CLI install / update lifecycle
            "probe_cli",
            "cli_install_latest",
            "cli_install_commands",
            "cli_update_check",
            "cli_update_install",
            // CLI session import
            "cli_session_import",
            "cli_sessions_import_all",
            "cli_sessions_list",
            // Grok account backend
            "account_status",
            "account_login",
            "account_login_cancel",
            "account_logout",
            "account_switch",
            "account_remove",
            "account_rename",
            "account_save_current",
            "account_open_subscribe",
            "account_open_usage",
            "accounts_list",
        ] {
            assert!(
                !commands.contains(&removed),
                "legacy command remained: {removed}"
            );
        }
    }
}
