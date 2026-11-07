use super::*;

fn done<T>(result: Result<T, String>) -> Load<T> {
    Load::Loading(Arc::new(Mutex::new(Some(result))))
}

fn pending<T>() -> Load<T> {
    Load::Loading(Arc::new(Mutex::new(None)))
}

#[test]
fn a_load_is_adopted_once() {
    let mut load = done(Ok(3));
    assert!(load.is_loading());
    assert!(load.poll());
    assert_eq!(load.ready(), Some(&3));
    assert!(!load.poll());

    let mut failed: Load<i32> = done(Err("offline".into()));
    assert!(failed.poll());
    assert_eq!(failed.error(), Some("offline"));

    let mut waiting: Load<i32> = pending();
    assert!(!waiting.poll());
    assert!(waiting.is_loading());
}

#[test]
fn spawned_jobs_land_in_their_slot() {
    let slot = spawn(|| Ok::<_, TuiError>(7));
    let result = loop {
        if let Some(r) = take(&slot) {
            break r;
        }
        std::thread::yield_now();
    };
    assert_eq!(result, Ok(7));
}
