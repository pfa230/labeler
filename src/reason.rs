//! The `details.reason` vocabulary (ADR-0052).
//!
//! Each slug is API: clients switch on it, so renaming one is a breaking change. The macro keeps
//! `ALL` structurally complete — a variant cannot be added without appearing in it, which is what
//! makes the completeness test in `errors.rs` meaningful. Slugs are written out beside their
//! variants rather than derived from them, so renaming a variant does not silently move the wire
//! value.

macro_rules! reasons {
    ($($variant:ident => $slug:literal,)+) => {
        /// A stable, machine-readable cause, serialized as `details.reason`.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Reason {
            $($variant,)+
        }

        impl Reason {
            /// Every reason, in declaration order.
            pub const ALL: &'static [Reason] = &[$(Reason::$variant,)+];

            /// The wire slug. Part of the API contract; see SPEC §10.1.
            pub fn as_slug(self) -> &'static str {
                match self {
                    $(Reason::$variant => $slug,)+
                }
            }
        }
    };
}

reasons! {
    // TemplateInvalid
    TemplateParseFailed => "template_parse_failed",
    TemplateValidationFailed => "template_validation_failed",
    TemplateDuplicateId => "template_duplicate_id",

    // UnsupportedLayoutItem
    CoordOutOfFrame => "coord_out_of_frame",
    ItemOutOfFrame => "item_out_of_frame",
    LineEndpointOutOfFrame => "line_endpoint_out_of_frame",
    LineDegenerate => "line_degenerate",
    EdgeRectInverted => "edge_rect_inverted",
    SizeInvalid => "size_invalid",
    SizeAutoWithoutMax => "size_auto_without_max",
    MaxSizeInvalid => "max_size_invalid",
    ImageSourceMissing => "image_source_missing",
    ImageFormatUnsupported => "image_format_unsupported",
    ImageDataInvalid => "image_data_invalid",
    ImageAssetMissing => "image_asset_missing",
    ImageAssetUnreadable => "image_asset_unreadable",
    ImageAssetPathEscapes => "image_asset_path_escapes",
    AssetsDirUnavailable => "assets_dir_unavailable",
    QrErrorCorrectionInvalid => "qr_error_correction_invalid",
}

#[cfg(test)]
mod tests {
    use super::Reason;
    use std::collections::HashSet;

    #[test]
    fn slugs_are_unique() {
        let mut seen = HashSet::new();
        for reason in Reason::ALL {
            assert!(
                seen.insert(reason.as_slug()),
                "duplicate slug '{}'",
                reason.as_slug()
            );
        }
    }

    #[test]
    fn slugs_are_snake_case_and_non_empty() {
        for reason in Reason::ALL {
            let slug = reason.as_slug();
            assert!(!slug.is_empty(), "{reason:?} has an empty slug");
            assert!(
                slug.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "slug '{slug}' is not snake_case"
            );
        }
    }

    #[test]
    fn template_invalid_slugs_are_exact() {
        assert_eq!(
            Reason::TemplateParseFailed.as_slug(),
            "template_parse_failed"
        );
        assert_eq!(
            Reason::TemplateValidationFailed.as_slug(),
            "template_validation_failed"
        );
        assert_eq!(
            Reason::TemplateDuplicateId.as_slug(),
            "template_duplicate_id"
        );
    }
}
