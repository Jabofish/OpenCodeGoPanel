use serde::{de::DeserializeOwned, Serialize};
use std::path::PathBuf;

/// Atomically (under the caller's lock) swap a store's backing file:
/// 1. Persist `current` to `*path`.
/// 2. Load `new_path` into `*current` (or `T::default()` if absent).
/// 3. Set `*path = new_path`.
///
/// The caller holds its own `RwLock` write-guard across this call so that
/// no concurrent `record`/`update` can interleave.
///
/// If the target file exists but cannot be parsed, the old state remains
/// active and the error is returned to the caller instead of silently showing
/// an empty store.
pub fn swap_store_file<T>(
    current: &mut T,
    path: &mut PathBuf,
    new_path: PathBuf,
) -> Result<(), String>
where
    T: Serialize + DeserializeOwned + Default,
{
    // 1. Flush current state to the OLD path.
    persist(current, path)?;

    // 2. Load the NEW path (default if missing). If the file is absent,
    //    materialize it with the default so the new path is a stable,
    //    writable location for subsequent `record`/`update` calls.
    let existed = new_path.exists();
    let next = load_or_default(&new_path)?;
    *current = next;
    if !existed {
        persist(current, &new_path)?;
    }

    // 3. Adopt the new path.
    *path = new_path;
    Ok(())
}

fn persist<T: Serialize>(value: &T, path: &PathBuf) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create_dir_all {}: {}", parent.display(), e))?;
    }
    let content = serde_json::to_string_pretty(value).map_err(|e| format!("serialize: {}", e))?;
    std::fs::write(path, content).map_err(|e| format!("write {}: {}", path.display(), e))
}

fn load_or_default<T: DeserializeOwned + Default>(path: &PathBuf) -> Result<T, String> {
    match std::fs::read_to_string(path) {
        Ok(content) => {
            serde_json::from_str(&content).map_err(|e| format!("parse {}: {}", path.display(), e))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(format!("read {}: {}", path.display(), e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    fn temp_dir() -> PathBuf {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("ocp-io-{}-{}", pid, nanos))
    }

    #[derive(Debug, Serialize, Deserialize, Default, PartialEq, Clone)]
    struct Sample {
        items: Vec<String>,
    }

    #[test]
    fn swap_persists_current_then_loads_target() {
        let dir = temp_dir();
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut current = Sample {
            items: vec!["a".into()],
        };
        let mut path = dir.join("old.json");
        let new_path = dir.join("new.json");

        // Pre-seed the new file with different content.
        std::fs::write(
            &new_path,
            serde_json::to_string_pretty(&Sample {
                items: vec!["b".into(), "c".into()],
            })
            .unwrap(),
        )
        .unwrap();

        swap_store_file(&mut current, &mut path, new_path).unwrap();

        // Old file now holds the old content.
        let old_str = std::fs::read_to_string(dir.join("old.json")).unwrap();
        assert!(old_str.contains("\"a\""));
        // In-memory is now the new content.
        assert_eq!(current.items, vec!["b".to_string(), "c".to_string()]);
        // Path swapped.
        assert_eq!(path, dir.join("new.json"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn swap_to_missing_file_uses_default() {
        let dir = temp_dir();
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut current = Sample {
            items: vec!["a".into()],
        };
        let mut path = dir.join("old.json");
        let new_path = dir.join("does-not-exist.json");

        swap_store_file(&mut current, &mut path, new_path).unwrap();

        // Old content persisted.
        assert!(std::fs::read_to_string(dir.join("old.json"))
            .unwrap()
            .contains("\"a\""));
        // In-memory is default (empty vec).
        assert_eq!(current.items, Vec::<String>::new());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn swap_creates_parent_dirs_for_new_path() {
        let dir = temp_dir();
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut current = Sample {
            items: vec!["a".into()],
        };
        let mut path = dir.join("old.json");
        let new_path = dir.join("nested").join("deep").join("new.json");

        swap_store_file(&mut current, &mut path, new_path.clone()).unwrap();
        assert!(new_path.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn swap_with_corrupt_new_file_returns_error_and_keeps_current() {
        let dir = temp_dir();
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut current = Sample {
            items: vec!["a".into()],
        };
        let mut path = dir.join("old.json");
        let new_path = dir.join("corrupt.json");
        std::fs::write(&new_path, "{ this is not json").unwrap();

        let err = swap_store_file(&mut current, &mut path, new_path).unwrap_err();
        assert!(err.contains("parse"));
        assert_eq!(current.items, vec!["a".to_string()]);
        assert_eq!(path, dir.join("old.json"));
        assert!(std::fs::read_to_string(dir.join("old.json"))
            .unwrap()
            .contains("\"a\""));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
