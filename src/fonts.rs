use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;
use typst::foundations::Bytes;
use typst::text::{Font, FontBook, FontVariant, FontWeight};

/// Loaded bytes and the matcher shared by validation, measurement and Typst compilation.
pub(crate) struct FontInventory {
    pub fonts: Vec<Font>,
    book: FontBook,
}

impl FontInventory {
    pub fn load(bundled: &Path, custom: &Path) -> Self {
        let mut fonts = load_dir(bundled);
        let bundled_families: HashSet<_> = fonts
            .iter()
            .map(|font| font.info().family.to_lowercase())
            .collect();
        fonts.extend(
            load_dir(custom)
                .into_iter()
                .filter(|font| !bundled_families.contains(&font.info().family.to_lowercase())),
        );
        let book = FontBook::from_fonts(&fonts);
        Self { fonts, book }
    }

    pub fn select(&self, family: &str, weight: u16) -> Option<&Font> {
        let variant = FontVariant {
            weight: FontWeight::from_number(weight),
            ..FontVariant::default()
        };
        self.book
            .select(&family.to_lowercase(), variant)
            .map(|index| &self.fonts[index])
    }
}

fn load_dir(dir: &Path) -> Vec<Font> {
    let mut fonts = Vec::new();
    // A missing custom directory and unusable files contribute no families.
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if let Ok(bytes) = std::fs::read(entry.path()) {
            fonts.extend(Font::iter(Bytes::new(bytes)));
        }
    }
    fonts
}

pub(crate) fn inventory() -> &'static FontInventory {
    static INVENTORY: OnceLock<FontInventory> = OnceLock::new();
    INVENTORY.get_or_init(|| {
        let bundled = crate::resolve_dir(std::env::var_os("LABELER_FONTS_DIR"), "fonts");
        let config = crate::resolve_dir(std::env::var_os("LABELER_CONFIG_DIR"), "/config");
        FontInventory::load(&bundled, &config.join("fonts"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use typst::layout::{Frame, FrameItem};
    use typst::text::FontStretch;

    fn assert_emitted_font(frame: &Frame, expected: &Font) -> usize {
        frame
            .items()
            .map(|(_, item)| match item {
                FrameItem::Group(group) => assert_emitted_font(&group.frame, expected),
                FrameItem::Text(text) => {
                    assert_eq!(text.font.font(), expected);
                    1
                }
                _ => 0,
            })
            .sum()
    }

    #[test]
    fn loaded_inventory_excludes_collisions_and_retains_removed_bytes() {
        let custom =
            std::path::PathBuf::from(format!(".agent-runs/font-inventory-{}", std::process::id()));
        std::fs::create_dir_all(&custom).unwrap();
        for font in typst_assets::fonts().flat_map(|bytes| Font::iter(Bytes::new(bytes))) {
            if font.info().family == "Libertinus Serif"
                && font.info().variant.style == typst::text::FontStyle::Normal
                && [400, 700].contains(&font.info().variant.weight.to_number())
            {
                std::fs::write(
                    custom.join(format!("{}.otf", font.info().variant.weight.to_number())),
                    font.data(),
                )
                .unwrap();
            }
        }
        let collision = Font::new(
            Bytes::new(include_bytes!("../tests/fixtures/fonts/Roboto.ttf").as_slice()),
            0,
        )
        .unwrap();
        assert_eq!(collision.info().family, "Roboto");
        assert_eq!(collision.info().variant.stretch, FontStretch::NORMAL);
        std::fs::write(custom.join("Roboto.ttf"), collision.data()).unwrap();
        std::fs::write(custom.join("unusable.ttf"), b"not a font").unwrap();
        let inventory = FontInventory::load(Path::new("fonts"), &custom);
        let bundled = inventory.select("Roboto", 400).unwrap();
        assert_ne!(bundled.info().variant.stretch, FontStretch::NORMAL);
        let competing_book = FontBook::from_fonts([bundled, &collision]);
        assert_eq!(
            competing_book.select("roboto", FontVariant::default()),
            Some(1),
            "ordering alone would select the custom font"
        );
        assert!(!inventory.fonts.contains(&collision));
        assert_eq!(inventory.book.select_family("roboto").count(), 1);
        for family in ["Inter", "Source Sans 3", "JetBrains Mono", "Roboto"] {
            assert_eq!(
                inventory.select(family, 400),
                inventory.select(&family.to_uppercase(), 400)
            );
            assert!(inventory.select(family, 400).is_some(), "{family}");
        }
        for weight in [400, 700] {
            let font = inventory.select("lIbErTiNuS sErIf", weight).unwrap();
            assert_eq!(font.info().variant.weight.to_number(), weight);
            let face = ttf_parser::Face::parse(font.data(), font.index()).unwrap();
            assert!(face.variation_axes().is_empty());
        }
        std::fs::remove_dir_all(&custom).unwrap();
        // A fresh engine must still emit the original loaded face after disk removal.
        for (family, weight) in [
            ("Libertinus Serif", 400),
            ("Libertinus Serif", 700),
            ("Roboto", 400),
        ] {
            let engine = typst_as_lib::TypstEngine::builder()
                .fonts(inventory.fonts.clone())
                .main_file(format!(
                    "#set text(font: \"{family}\", weight: {weight})\nWidget"
                ))
                .build();
            let result = engine.compile::<typst_layout::PagedDocument>();
            assert!(result.warnings.is_empty(), "{:?}", result.warnings);
            let doc = result.output.unwrap();
            assert!(
                assert_emitted_font(
                    &doc.pages()[0].frame,
                    inventory.select(family, weight).unwrap()
                ) > 0
            );
        }
        let restarted = FontInventory::load(Path::new("fonts"), &custom);
        assert!(restarted.select("Libertinus Serif", 400).is_none());
        assert_eq!(restarted.book.families().count(), 4);
    }
}
