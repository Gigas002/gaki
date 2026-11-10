use super::*;

#[test]
fn safe_join_keeps_paths_inside_and_takes_both_separators() {
    let dir = Path::new("/cache/620");
    assert_eq!(
        safe_join(dir, "fozpipelinesv6/steam_pipeline_cache.foz").unwrap(),
        dir.join("fozpipelinesv6/steam_pipeline_cache.foz")
    );
    assert_eq!(
        safe_join(dir, "fozpipelinesv6\\steam_pipeline_cache.foz").unwrap(),
        dir.join("fozpipelinesv6/steam_pipeline_cache.foz")
    );
    for bad in ["", "../x", "a/../b", "/abs", "a//b", "./a"] {
        assert!(safe_join(dir, bad).is_err(), "{bad:?} must be refused");
    }
}

#[test]
fn compiled_counts_take_the_last_progress_report() {
    let output = "\
Fossilize INFO:    Compile graphics 10 / 65537, skipped 3, cached 0
Fossilize INFO:    Compile compute 2 / 56774, skipped 1, cached 0
Fossilize INFO:    Compile graphics 7316 / 65537, skipped 58221, cached 0
Fossilize INFO:    Compile compute 1353 / 56774, skipped 55421, cached 0
";
    assert_eq!(compiled_counts(output), (7316, 1353));
    assert_eq!(compiled_counts("nothing here"), (0, 0));
}

#[test]
fn replay_env_matches_the_launch_and_lets_the_user_win() {
    let dir = Path::new("/cache/620");
    let user = BTreeMap::from([("DXVK_STATE_CACHE_PATH".to_owned(), "/mine".to_owned())]);
    let env: BTreeMap<String, OsString> = replay_env(dir, |k| k == "MESA_SHADER_CACHE_DIR", &user)
        .into_iter()
        .collect();
    // gaki's own environment has it: inherited, not overridden.
    assert!(!env.contains_key("MESA_SHADER_CACHE_DIR"));
    assert_eq!(env["DXVK_STATE_CACHE_PATH"], OsString::from("/mine"));
    assert_eq!(
        env["__GL_SHADER_DISK_CACHE_PATH"],
        dir.join("nvidiav1").into_os_string()
    );
    assert_eq!(
        env["AMD_VK_PIPELINE_CACHE_PATH"],
        dir.join("AMDv1").into_os_string()
    );
}

#[test]
fn state_round_trips_and_knows_when_files_are_current() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = State::default();
    state.buckets.insert(
        "pipelines".into(),
        BucketState {
            manifest_id: 7,
            files: vec!["fozpipelinesv6/steam_pipeline_cache.foz".into()],
            replayed: Some(7),
        },
    );
    state.save(dir.path()).unwrap();
    let back = State::load(dir.path());
    assert_eq!(back, state);

    // The file isn't there yet.
    assert!(!back.is_current(BucketKind::Pipelines, 7, dir.path()));
    std::fs::create_dir_all(dir.path().join("fozpipelinesv6")).unwrap();
    std::fs::write(
        dir.path().join("fozpipelinesv6/steam_pipeline_cache.foz"),
        b"x",
    )
    .unwrap();
    assert!(back.is_current(BucketKind::Pipelines, 7, dir.path()));
    // Another manifest, or another kind, isn't.
    assert!(!back.is_current(BucketKind::Pipelines, 8, dir.path()));
    assert!(!back.is_current(BucketKind::Video, 7, dir.path()));
}

#[test]
fn a_broken_state_file_reads_as_empty() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(STATE_FILE), "not = [toml").unwrap();
    assert_eq!(State::load(dir.path()), State::default());
}

#[test]
fn reports_serialize_with_a_status_tag() {
    let report = Report {
        app_id: 620,
        dir: "/cache/620".into(),
        buckets: vec![
            BucketResult {
                kind: BucketKind::Pipelines,
                manifest_id: Some(7),
                status: BucketStatus::Downloaded { files: 1, bytes: 9 },
            },
            BucketResult {
                kind: BucketKind::Video,
                manifest_id: None,
                status: BucketStatus::NotOffered,
            },
        ],
        replay: Replay::Compiled {
            graphics: 3,
            compute: 1,
        },
    };
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["buckets"][0]["kind"], "pipelines");
    assert_eq!(json["buckets"][0]["status"], "downloaded");
    assert_eq!(json["buckets"][0]["bytes"], 9);
    assert_eq!(json["buckets"][1]["status"], "not_offered");
    assert_eq!(json["replay"]["status"], "compiled");
    assert_eq!(json["replay"]["graphics"], 3);
    assert_eq!(
        report.summary(),
        "Shaders: pipelines: downloaded (9 B), video: none on Steam; \
         compiled 3 graphics and 1 compute pipelines."
    );
    assert_eq!(report.short(), "shaders compiled");
}

#[test]
fn a_failed_download_shows_in_the_short_form() {
    let report = Report {
        app_id: 620,
        dir: "/cache/620".into(),
        buckets: vec![BucketResult {
            kind: BucketKind::Video,
            manifest_id: None,
            status: BucketStatus::Failed {
                error: "AccessDenied".into(),
            },
        }],
        replay: Replay::NoPipelines,
    };
    assert_eq!(report.short(), "shader download failed");
    assert!(report.summary().contains("video: failed: AccessDenied"));
}
