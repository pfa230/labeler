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
    fn loaded_inventory_retains_removed_bytes() {
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
        std::fs::write(custom.join("unusable.ttf"), b"not a font").unwrap();
        let inventory = FontInventory::load(Path::new("fonts"), &custom);
        for family in ["Inter", "Source Sans 3", "JetBrains Mono", "Roboto"] {
            assert_eq!(
                inventory.select(family, 400),
                inventory.select(&family.to_uppercase(), 400)
            );
            assert!(
                inventory.select(family, 400).is_some(),
                "{family}: {:?}",
                inventory
                    .fonts
                    .iter()
                    .map(|font| &font.info().family)
                    .collect::<Vec<_>>()
            );
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

    #[test]
    fn bundled_family_excludes_a_closer_custom_weight() {
        let root = std::path::PathBuf::from(format!(
            ".agent-runs/font-precedence-{}",
            std::process::id()
        ));
        let bundled = root.join("bundled");
        let custom = root.join("custom");
        std::fs::create_dir_all(&bundled).unwrap();
        std::fs::create_dir_all(&custom).unwrap();
        let face = |weight| {
            typst_assets::fonts()
                .flat_map(|bytes| Font::iter(Bytes::new(bytes)))
                .find(|font| {
                    font.info().family == "Libertinus Serif"
                        && font.info().variant.style == typst::text::FontStyle::Normal
                        && font.info().variant.weight.to_number() == weight
                })
                .unwrap()
        };
        let bold = face(700);
        let regular = face(400);
        for font in [&bold, &regular] {
            assert_eq!(font.info().variant.stretch, FontStretch::NORMAL);
            let face = ttf_parser::Face::parse(font.data(), font.index()).unwrap();
            assert!(face.variation_axes().is_empty());
            assert_eq!(face.width().to_number(), 5);
        }
        let competing_book = FontBook::from_fonts([&bold, &regular]);
        assert_eq!(
            competing_book.select("libertinus serif", FontVariant::default()),
            Some(1),
            "ordering alone selects the closer custom weight"
        );
        std::fs::write(bundled.join("LibertinusSerif-Bold.otf"), bold.data()).unwrap();
        std::fs::write(custom.join("LibertinusSerif-Regular.otf"), regular.data()).unwrap();
        let inventory = FontInventory::load(&bundled, &custom);
        assert_eq!(inventory.fonts, [bold.clone()]);
        assert_eq!(inventory.select("lIbErTiNuS sErIf", 400), Some(&bold));
        let engine = typst_as_lib::TypstEngine::builder()
            .fonts(inventory.fonts.clone())
            .main_file("#set text(font: \"Libertinus Serif\", weight: 400)\nWidget")
            .build();
        let result = engine.compile::<typst_layout::PagedDocument>();
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        let doc = result.output.unwrap();
        assert!(assert_emitted_font(&doc.pages()[0].frame, &bold) > 0);
        std::fs::remove_dir_all(root).unwrap();
    }
}
