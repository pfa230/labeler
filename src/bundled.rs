use include_dir::{include_dir, Dir};
use std::path::Path;

pub static BUNDLED_TEMPLATES: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/templates");

/// Seed the embedded bundled templates into `templates_dir` ONCE (first run), gated by the DB flag.
/// After first run the dir is the user's; deletes are permanent. The flag write MUST propagate.
pub async fn seed_templates_once(
    store: &crate::store::Store,
    templates_dir: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    const FLAG: &str = "templates_seeded";
    if store.get_setting(FLAG).await?.is_some() {
        return Ok(());
    }
    std::fs::create_dir_all(templates_dir)?;
    for file in BUNDLED_TEMPLATES.files() {
        let Some(name) = file.path().file_name() else {
            continue;
        };
        let is_yaml = Path::new(name)
            .extension()
            .is_some_and(|e| e == "yaml" || e == "yml");
        if is_yaml {
            std::fs::write(templates_dir.join(name), file.contents())?;
        }
    }
    store.set_setting(FLAG, "1").await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    fn temp() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "labeler-seed-{}-{}",
            std::process::id(),
            rand_suffix()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn rand_suffix() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64
    }

    #[tokio::test]
    async fn seeds_once_then_idempotent_and_no_resurrect() {
        let dir = temp();
        let store = Store::open_in_memory().expect("store");
        // first run: seeds the bundled yamls + sets the flag
        seed_templates_once(&store, &dir).await.expect("seed");
        let n = std::fs::read_dir(&dir).unwrap().count();
        assert!(n > 0, "bundled templates seeded");
        // the seeded set validates
        crate::templates::TemplateRegistry::load_from_dir(&dir).expect("seeded templates valid");
        // delete one, run again -> NOT re-created (flag set)
        let first = std::fs::read_dir(&dir)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        std::fs::remove_file(&first).unwrap();
        seed_templates_once(&store, &dir).await.expect("seed 2");
        assert_eq!(
            std::fs::read_dir(&dir).unwrap().count(),
            n - 1,
            "deleted template not resurrected"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
