use super::*;
#[test]
fn duration_roundtrip() {
    for (s, n, render) in [
        ("1h2m3.000000004s", 3_723_000_000_004, "1h2m3.000000004s"),
        ("1.5us", 1500, "1.5µs"),
        ("-0.5s", -500_000_000, "-500ms"),
        ("0", 0, "0s"),
    ] {
        let d = DurationValue::parse(s).unwrap();
        assert_eq!(d.0, n);
        assert_eq!(d.to_string(), render)
    }
    assert!(DurationValue::parse("1x").is_err());
}
#[test]
fn lifecycle() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::new(dir.path());
    let k = s
        .create(KeySpec {
            name: "test".into(),
            models: vec!["codex/*".into()],
            policy: Policy::default(),
        })
        .unwrap();
    assert_eq!(s.authenticate(&k.token).unwrap(), k.principal);
    assert!(s.insert_prepared(&k).is_err());
    assert_eq!(s.list().unwrap().len(), 1);
    assert_eq!(s.inspect(&k.principal.id).unwrap().accepted, 0);
    s.revoke(&k.principal.id).unwrap();
    assert!(matches!(s.authenticate(&k.token), Err(Error::InvalidKey)));
}
#[test]
fn rejects_unknown_record_fields() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::new(dir.path());
    let k = s
        .create(KeySpec {
            name: "test".into(),
            models: vec!["*".into()],
            policy: Policy::default(),
        })
        .unwrap();
    let path = dir
        .path()
        .join("keys")
        .join(format!("{}.json", k.principal.id));
    let mut v: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    v["unknown"] = serde_json::json!(true);
    fs::write(path, serde_json::to_vec(&v).unwrap()).unwrap();
    assert!(matches!(s.inspect(&k.principal.id), Err(Error::Storage)));
}
#[cfg(unix)]
#[test]
fn separate_process_owner_lock() {
    if let Ok(dir) = std::env::var("H2A_TEST_LOCK_CHILD") {
        let store = Store::new(dir);
        assert!(matches!(
            store.open_admission(Duration::ZERO, 0),
            Err(Error::OwnerBusy)
        ));
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    let _owner = store.open_admission(Duration::ZERO, 0).unwrap();
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "auth::tests::separate_process_owner_lock",
            "--nocapture",
        ])
        .env("H2A_TEST_LOCK_CHILD", dir.path())
        .status()
        .unwrap();
    assert!(status.success());
}
#[tokio::test]
async fn admission_quota_and_revoke() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::new(dir.path());
    let k = s
        .create(KeySpec {
            name: "test".into(),
            models: vec!["*".into()],
            policy: Policy {
                max_requests: 2,
                max_concurrent: 1,
                ..Default::default()
            },
        })
        .unwrap();
    let a = s.open_admission(Duration::from_secs(10), 4).unwrap();
    let l = a
        .admit(
            k.token.clone(),
            "codex/test".into(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert!(matches!(
        a.admit(
            k.token.clone(),
            "codex/test".into(),
            CancellationToken::new()
        )
        .await,
        Err(Error::Limit {
            code: "key_concurrency_exceeded",
            ..
        })
    ));
    drop(l);
    let l = a
        .admit(
            k.token.clone(),
            "codex/test".into(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(s.inspect(&k.principal.id).unwrap().accepted, 2);
    s.revoke(&k.principal.id).unwrap();
    tokio::time::timeout(Duration::from_secs(2), l.cancellation().cancelled_owned())
        .await
        .unwrap();
    assert!(matches!(l.cause(), Some(Error::RevokedKey)));
    drop(l);
    a.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn transaction_contention_does_not_suspend_deadline_or_shutdown() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    let key = store
        .create(KeySpec {
            name: "contention".into(),
            models: vec!["*".into()],
            policy: Policy::default(),
        })
        .unwrap();
    let admission = store.open_admission(Duration::from_millis(250), 3).unwrap();
    let lease = admission
        .admit(
            key.token.clone(),
            "codex/default".into(),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    let lock = store.lock(".transaction.lock", true).unwrap();
    let a = admission.clone();
    let waiting = tokio::spawn(async move {
        a.admit(key.token, "codex/default".into(), CancellationToken::new())
            .await
    });
    tokio::time::timeout(
        Duration::from_secs(1),
        lease.cancellation().cancelled_owned(),
    )
    .await
    .unwrap();
    assert!(matches!(lease.cause(), Some(Error::Timeout)));
    drop(lease);
    tokio::time::timeout(Duration::from_secs(1), admission.close())
        .await
        .unwrap();
    drop(lock);
    assert!(matches!(waiting.await.unwrap(), Err(Error::Canceled)));
}

#[test]
fn zero_timestamps_fail_closed_at_record_boundaries() {
    for field in ["created_at", "last_used_at", "recent"] {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let key = store
            .create(KeySpec {
                name: "zero".into(),
                models: vec!["*".into()],
                policy: Policy {
                    requests_per_minute: 2,
                    ..Default::default()
                },
            })
            .unwrap();
        let path = dir
            .path()
            .join("keys")
            .join(format!("{}.json", key.principal.id));
        let mut record: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let zero = serde_json::json!("0001-01-01T00:00:00Z");
        record["usage"] = serde_json::json!({"accepted":1,"last_used_at":Utc::now()});
        match field {
            "created_at" => record[field] = zero,
            "last_used_at" => record["usage"][field] = zero,
            _ => record["usage"][field] = serde_json::json!([zero]),
        }
        fs::write(path, serde_json::to_vec(&record).unwrap()).unwrap();
        assert!(
            matches!(store.inspect(&key.principal.id), Err(Error::Storage)),
            "{field}"
        );
        assert!(
            matches!(store.authenticate(&key.token), Err(Error::Storage)),
            "{field}"
        );
    }
}

#[test]
fn runtime_shutdown_with_transaction_lock_held() {
    for mode in [
        "timeout",
        "caller",
        "close",
        "pending_caller",
        "pending_drop",
        "pending_close",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let key = store
            .create(KeySpec {
                name: "shutdown".into(),
                models: vec!["*".into()],
                policy: Policy::default(),
            })
            .unwrap();
        let key_id = key.principal.id.clone();
        let timeout = if mode == "timeout" {
            Duration::from_millis(250)
        } else {
            Duration::from_secs(30)
        };
        let admission = store.open_admission(timeout, 0).unwrap();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let lock_store = store.clone();
        let thread = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                let caller = CancellationToken::new();
                let lease = if mode.starts_with("pending") {
                    None
                } else {
                    Some(
                        admission
                            .admit(key.token.clone(), "codex/default".into(), caller.clone())
                            .await
                            .unwrap(),
                    )
                };
                let lock = lock_store.lock(".transaction.lock", true).unwrap();
                ready_tx.send(lock).unwrap();
                if let Some(lease) = lease {
                    // Let the watcher attempt revalidation while the lock is held.
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    match mode {
                        "caller" => caller.cancel(),
                        "close" => {
                            let a = admission.clone();
                            tokio::spawn(async move { a.close().await });
                        }
                        _ => {}
                    }
                    tokio::time::timeout(
                        Duration::from_secs(2),
                        lease.cancellation().cancelled_owned(),
                    )
                    .await
                    .unwrap();
                    assert!(matches!(
                        (mode, lease.cause()),
                        ("timeout", Some(Error::Timeout))
                            | ("caller" | "close", Some(Error::Canceled))
                    ));
                    drop(lease);
                } else {
                    let a = admission.clone();
                    let c = caller.clone();
                    let waiting =
                        tokio::spawn(
                            async move { a.admit(key.token, "codex/default".into(), c).await },
                        );
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    match mode {
                        "pending_caller" => caller.cancel(),
                        "pending_drop" => waiting.abort(),
                        _ => admission.close().await,
                    }
                    if mode != "pending_drop" {
                        assert!(matches!(
                            tokio::time::timeout(Duration::from_secs(2), waiting)
                                .await
                                .unwrap()
                                .unwrap(),
                            Err(Error::Canceled)
                        ));
                    }
                }
                tokio::time::timeout(Duration::from_secs(2), admission.close())
                    .await
                    .unwrap();
            });
            drop(runtime);
            done_tx.send(()).unwrap();
        });
        let lock = ready_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        let result = done_rx.recv_timeout(Duration::from_secs(3));
        drop(lock);
        thread.join().unwrap();
        assert_eq!(
            store.inspect(&key_id).unwrap().accepted,
            u64::from(!mode.starts_with("pending"))
        );
        assert!(
            result.is_ok(),
            "runtime did not shut down with lock held: {mode}"
        );
    }
}

#[test]
fn transaction_releases_lock_before_executor_resumes() {
    const CHILD: &str = "H2A_TRANSACTION_HANDOFF_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let key = store
            .create(KeySpec {
                name: "handoff".into(),
                models: vec!["*".into()],
                policy: Policy::default(),
            })
            .unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (tx, rx) = std::sync::mpsc::channel();
            let worker_store = store.clone();
            let task = tokio::spawn(async move {
                let cancel = CancellationToken::new();
                transaction(&worker_store, &cancel, &cancel, move || {
                    tx.send(()).unwrap();
                    Ok(())
                })
                .await
            });
            tokio::task::yield_now().await;
            rx.recv_timeout(Duration::from_secs(2)).unwrap();
            // The executor cannot poll the join result while inspection waits for the lock.
            assert_eq!(store.inspect(&key.principal.id).unwrap().accepted, 0);
            task.await.unwrap().unwrap();
        });
        return;
    }
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "auth::tests::transaction_releases_lock_before_executor_resumes",
        ])
        .env(CHILD, "1")
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("transaction handoff deadlocked");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[tokio::test]
async fn cancellable_authentication_preserves_results() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    let key = store
        .create(KeySpec {
            name: "preauth-results".into(),
            models: vec!["*".into()],
            policy: Policy::default(),
        })
        .unwrap();
    assert_eq!(
        store
            .authenticate_cancellable(key.token, CancellationToken::new())
            .await
            .unwrap(),
        key.principal
    );
    assert!(matches!(
        store
            .authenticate_cancellable("invalid".into(), CancellationToken::new())
            .await,
        Err(Error::InvalidKey)
    ));
    assert_eq!(store.inspect(&key.principal.id).unwrap().accepted, 0);
}

#[test]
fn cancellable_authentication_shuts_down_with_lock_held() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    let key = store
        .create(KeySpec {
            name: "preauth".into(),
            models: vec!["*".into()],
            policy: Policy::default(),
        })
        .unwrap();
    let lock = store.lock(".transaction.lock", true).unwrap();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let worker_store = store.clone();
    let thread = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let cancel = CancellationToken::new();
            let c = cancel.clone();
            let task =
                tokio::spawn(
                    async move { worker_store.authenticate_cancellable(key.token, c).await },
                );
            tokio::time::sleep(Duration::from_millis(100)).await;
            cancel.cancel();
            assert!(matches!(
                tokio::time::timeout(Duration::from_secs(1), task)
                    .await
                    .unwrap()
                    .unwrap(),
                Err(Error::Canceled)
            ));
        });
        drop(runtime);
        done_tx.send(()).unwrap();
    });
    let result = done_rx.recv_timeout(Duration::from_secs(3));
    drop(lock);
    thread.join().unwrap();
    assert!(
        result.is_ok(),
        "authentication left a blocking lock waiter at runtime shutdown"
    );
    assert_eq!(store.inspect(&key.principal.id).unwrap().accepted, 0);
}
