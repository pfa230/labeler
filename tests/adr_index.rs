//! `docs/adr/README.md` carries the ADR catalog. Adding a row for each record was a convention, and
//! it was silently skipped for twenty consecutive records (#160), which is what an unenforced
//! convention does. The directory is frozen now (#285), so nothing new arrives, and this guards the
//! archive instead: a file added without a row, or a row left behind by a deleted file. Checked in
//! both directions: an unlisted record makes the index incomplete, a listed one that does not exist
//! makes it a liar.

use std::collections::HashSet;

const ADR_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/docs/adr");

/// The four-digit prefix of an ADR filename (`0042-remove-printer-enabled.md` -> `0042`).
fn adr_number(file_name: &str) -> Option<&str> {
    let (number, _) = file_name.split_once('-')?;
    (number.len() == 4 && number.bytes().all(|b| b.is_ascii_digit())).then_some(number)
}

/// A `| [NNNN](NNNN-slug.md) | …` row, as (number, link target). Header and separator rows have no
/// bracketed four-digit label and yield `None`.
fn index_row(line: &str) -> Option<(&str, &str)> {
    let cell = line.strip_prefix('|')?.split('|').next()?.trim();
    let (number, rest) = cell.strip_prefix('[')?.split_once(']')?;
    if number.len() != 4 || !number.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let href = rest.strip_prefix('(')?.split_once(')')?.0;
    Some((number, href))
}

#[test]
fn adr_readme_indexes_every_record_and_invents_none() {
    let readme =
        std::fs::read_to_string(format!("{ADR_DIR}/README.md")).expect("read adr/README.md");
    let index = readme
        .split("## Index")
        .nth(1)
        .expect("adr/README.md must have an Index section");
    let index = index.split("\n## ").next().unwrap_or(index);

    let files: HashSet<String> = std::fs::read_dir(ADR_DIR)
        .expect("read adr dir")
        .filter_map(|entry| {
            let name = entry
                .expect("adr dir entry")
                .file_name()
                .into_string()
                .ok()?;
            adr_number(&name)?;
            Some(name)
        })
        .collect();
    assert!(
        !files.is_empty(),
        "found no ADR files in {ADR_DIR}; the test is not looking where it thinks it is"
    );

    let rows: Vec<(&str, &str)> = index.lines().filter_map(index_row).collect();

    // A row that links the wrong file is as misleading as a missing row, so the href has to name a
    // record that exists and carry the number the row claims to be.
    for (number, href) in &rows {
        assert_eq!(
            adr_number(href),
            Some(*number),
            "docs/adr/README.md row [{number}] links to {href}"
        );
        assert!(
            files.contains(*href),
            "docs/adr/README.md row [{number}] links to {href}, which does not exist"
        );
    }

    let listed: HashSet<&str> = rows.iter().map(|(number, _)| *number).collect();
    assert_eq!(
        listed.len(),
        rows.len(),
        "docs/adr/README.md lists an ADR more than once"
    );

    let on_disk: HashSet<&str> = files.iter().filter_map(|name| adr_number(name)).collect();

    let mut unlisted: Vec<_> = on_disk.difference(&listed).collect();
    unlisted.sort_unstable();
    assert!(
        unlisted.is_empty(),
        "ADRs missing from the docs/adr/README.md index: {unlisted:?}"
    );

    let mut phantom: Vec<_> = listed.difference(&on_disk).collect();
    phantom.sort_unstable();
    assert!(
        phantom.is_empty(),
        "docs/adr/README.md indexes ADRs that do not exist: {phantom:?}"
    );
}
