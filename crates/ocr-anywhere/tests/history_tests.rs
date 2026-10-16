use ocr_anywhere::history::storage::HistoryStore;
use std::env::temp_dir;

#[test]
fn test_history_add_and_prune() {
    let mut temp_path = temp_dir();
    temp_path.push(format!(
        "ocr_test_history_{}.dat",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let mut store = HistoryStore::new(temp_path.clone(), 3);

    store.add_entry("Clip 1".to_string()).unwrap();
    store.add_entry("Clip 2".to_string()).unwrap();
    store.add_entry("Clip 3".to_string()).unwrap();

    assert_eq!(store.entries.len(), 3);
    assert_eq!(store.entries[0].text, "Clip 3"); // Most recent first
    assert_eq!(store.entries[2].text, "Clip 1");

    // Add 4th clip -> should prune oldest
    store.add_entry("Clip 4".to_string()).unwrap();
    assert_eq!(store.entries.len(), 3);
    assert_eq!(store.entries[0].text, "Clip 4");
    assert_eq!(store.entries[1].text, "Clip 3");
    assert_eq!(store.entries[2].text, "Clip 2");

    // Clean up
    let _ = std::fs::remove_file(&temp_path);
}

#[test]
fn test_history_delete_and_clear() {
    let mut temp_path = temp_dir();
    temp_path.push(format!(
        "ocr_test_history_del_{}.dat",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let mut store = HistoryStore::new(temp_path.clone(), 10);
    let e1 = store.add_entry("Entry to keep".to_string()).unwrap();
    let e2 = store.add_entry("Entry to delete".to_string()).unwrap();

    assert_eq!(store.entries.len(), 2);

    let deleted = store.delete_entry(&e2.id);
    assert!(deleted);
    assert_eq!(store.entries.len(), 1);
    assert_eq!(store.entries[0].id, e1.id);

    store.clear().unwrap();
    assert_eq!(store.entries.len(), 0);

    let _ = std::fs::remove_file(&temp_path);
}

#[test]
fn test_history_persistence_roundtrip() {
    let mut temp_path = temp_dir();
    temp_path.push(format!(
        "ocr_test_persist_{}.dat",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    {
        let mut store = HistoryStore::new(temp_path.clone(), 10);
        store
            .add_entry("Persisted sample text".to_string())
            .unwrap();
    }

    // Reload from disk
    let mut reloaded = HistoryStore::new(temp_path.clone(), 10);
    reloaded.load().unwrap();

    assert_eq!(reloaded.entries.len(), 1);
    assert_eq!(reloaded.entries[0].text, "Persisted sample text");

    let _ = std::fs::remove_file(&temp_path);
}
