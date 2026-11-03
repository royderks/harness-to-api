use harness_to_api::harness::{Adapter, Config, Error, Provider, Request, Usage};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn all_adapters_run_and_validate_terminal_and_cancellation() {
    let Ok(binary) = std::env::var("H2A_HARNESS_FIXTURE") else {
        return;
    };
    let extension = tempfile::tempdir().unwrap();
    std::fs::create_dir(extension.path().join("extensions")).unwrap();
    std::fs::write(extension.path().join("extensions/bob.ts"), "").unwrap();
    for provider in Provider::ALL {
        let adapter = Adapter::new(Config {
            provider,
            binary: binary.clone().into(),
            bob_extension: Some(extension.path().into()),
        })
        .unwrap();
        for model in ["default", "custom-model"] {
            let (tx, mut rx) = mpsc::channel(8);
            let result = adapter
                .run(
                    Request {
                        model: model.into(),
                        prompt: "hello".into(),
                    },
                    CancellationToken::new(),
                    tx,
                )
                .await
                .unwrap();
            assert_eq!(rx.recv().await.as_deref(), Some("Hello 🌍"), "{provider:?}");
            assert_eq!(
                result,
                Some(Usage {
                    prompt_tokens: 5,
                    completion_tokens: 2,
                    total_tokens: 7
                }),
                "{provider:?}"
            );
        }
        for model in ["missing-result", "bad-json", "exit-error"] {
            let (tx, _rx) = mpsc::channel(8);
            assert!(
                adapter
                    .run(
                        Request {
                            model: model.into(),
                            prompt: "hello".into()
                        },
                        CancellationToken::new(),
                        tx
                    )
                    .await
                    .is_err(),
                "{provider:?} {model}"
            );
        }
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        assert!(matches!(
            adapter
                .run(
                    Request {
                        model: "default".into(),
                        prompt: "hello".into()
                    },
                    CancellationToken::new(),
                    tx
                )
                .await,
            Err(Error::Canceled)
        ));
        let (tx, _rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();
        let c = cancel.clone();
        let a = adapter.clone();
        let task = tokio::spawn(async move {
            a.run(
                Request {
                    model: "hang".into(),
                    prompt: "hello".into(),
                },
                c,
                tx,
            )
            .await
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        cancel.cancel();
        assert!(
            matches!(
                tokio::time::timeout(std::time::Duration::from_secs(3), task)
                    .await
                    .unwrap()
                    .unwrap(),
                Err(Error::Canceled)
            ),
            "{provider:?}"
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn dropped_run_reaps_child_and_cleans_workdir() {
    let Ok(binary) = std::env::var("H2A_HARNESS_FIXTURE") else {
        return;
    };
    let adapter = Adapter::new(Config {
        provider: Provider::Codex,
        binary: binary.into(),
        bob_extension: None,
    })
    .unwrap();
    let (tx, mut rx) = mpsc::channel(1);
    let task = tokio::spawn(async move {
        adapter
            .run(
                Request {
                    model: "drop-guard".into(),
                    prompt: "hello".into(),
                },
                CancellationToken::new(),
                tx,
            )
            .await
    });
    let marker = tokio::time::timeout(std::time::Duration::from_secs(3), rx.recv())
        .await
        .unwrap()
        .unwrap();
    let (dir, pid) = marker.split_once('|').unwrap();
    let pid: i32 = pid.parse().unwrap();
    assert!(std::path::Path::new(dir).exists());
    task.abort();
    let _ = task.await;
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while std::path::Path::new(dir).exists() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        unsafe { libc::kill(pid, 0) },
        -1,
        "child must be reaped before cleanup"
    );
}

#[tokio::test]
async fn receiver_close_during_run_cancels_child() {
    let Ok(binary) = std::env::var("H2A_HARNESS_FIXTURE") else {
        return;
    };
    let adapter = Adapter::new(Config {
        provider: Provider::Codex,
        binary: binary.into(),
        bob_extension: None,
    })
    .unwrap();
    let (tx, mut rx) = mpsc::channel(1);
    let task = tokio::spawn(async move {
        adapter
            .run(
                Request {
                    model: "emit-hang".into(),
                    prompt: "hello".into(),
                },
                CancellationToken::new(),
                tx,
            )
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(3), rx.recv())
        .await
        .unwrap()
        .unwrap();
    drop(rx);
    assert!(matches!(
        tokio::time::timeout(std::time::Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap(),
        Err(Error::Canceled)
    ));
}
