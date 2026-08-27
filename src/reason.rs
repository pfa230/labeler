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
    TemplateGroupInvalid => "template_group_invalid",
    TemplateGroupCaseConflict => "template_group_case_conflict",
    TemplateGroupUnsafePath => "template_group_unsafe_path",

    // UnsupportedLayoutItem
    CoordOutOfFrame => "coord_out_of_frame",
    ItemOutOfFrame => "item_out_of_frame",
    LineEndpointOutOfFrame => "line_endpoint_out_of_frame",
    LineDegenerate => "line_degenerate",
    EdgeRectInverted => "edge_rect_inverted",
    SizeInvalid => "size_invalid",
    MaxSizeInvalid => "max_size_invalid",
    IntrinsicSizeUndefined => "intrinsic_size_undefined",
    TextDoesNotFit => "text_does_not_fit",
    ImageSourceMissing => "image_source_missing",
    ImageFormatUnsupported => "image_format_unsupported",
    ImageDataInvalid => "image_data_invalid",
    ImageAssetMissing => "image_asset_missing",
    ImageAssetUnreadable => "image_asset_unreadable",
    ImageAssetPathEscapes => "image_asset_path_escapes",
    AssetsDirUnavailable => "assets_dir_unavailable",
    QrErrorCorrectionInvalid => "qr_error_correction_invalid",
    DimensionExceedsLimit => "dimension_exceeds_limit",

    // InvalidRequest
    JsonMalformed => "json_malformed",
    RequestBodyInvalid => "request_body_invalid",
    PathParamInvalid => "path_param_invalid",
    StartSlotOutOfRange => "start_slot_out_of_range",
    StartSlotNotApplicable => "start_slot_not_applicable",
    OptionsNotSupported => "options_not_supported",
    BatchEmpty => "batch_empty",
    FormatUnknown => "format_unknown",
    FormatNotApplicable => "format_not_applicable",
    InterpolationSyntax => "interpolation_syntax",
    TemplateIdInvalid => "template_id_invalid",
    TemplateIdMismatch => "template_id_mismatch",
    TemplateGroupMismatch => "template_group_mismatch",
    UnsupportedPrecondition => "unsupported_precondition",
    PrinterIdInvalid => "printer_id_invalid",
    PrinterIdMismatch => "printer_id_mismatch",
    VariableKeyInvalid => "variable_key_invalid",
    SettingValueInvalid => "setting_value_invalid",
    DatetimePatternInvalid => "datetime_pattern_invalid",
    DatetimeParamInvalid => "datetime_param_invalid",
    ConnectorUnknown => "connector_unknown",
    ConnectionConnectorMissing => "connection_connector_missing",
    ConnectionTransformInvalid => "connection_transform_invalid",
    CredentialRequired => "credential_required",
    BaseUrlInvalid => "base_url_invalid",
    PublicUrlInvalid => "public_url_invalid",
    CsvHeaderInvalid => "csv_header_invalid",
    CsvRowInvalid => "csv_row_invalid",
    CsvEmpty => "csv_empty",
    CsvOptionColumnUnknown => "csv_option_column_unknown",
    ModeUnknown => "mode_unknown",
    PrinterRequired => "printer_required",
    CopiesInvalid => "copies_invalid",
    ColorModeUnknown => "color_mode_unknown",
    ResolutionInvalid => "resolution_invalid",
    BilevelRequiresPng => "bilevel_requires_png",
    UsernameEmpty => "username_empty",
    PasswordEmpty => "password_empty",

    // RenderFailed
    TypstCompileFailed => "typst_compile_failed",
    TypstSourceBuildFailed => "typst_source_build_failed",
    TypstNoPages => "typst_no_pages",
    PngEncodeFailed => "png_encode_failed",
    PdfEncodeFailed => "pdf_encode_failed",
    ItemHasNoSource => "item_has_no_source",
    QrGenerationFailed => "qr_generation_failed",
    FontReadFailed => "font_read_failed",
    FontParseFailed => "font_parse_failed",
    FontAxisMissing => "font_axis_missing",
    TemplatePathInvalid => "template_path_invalid",
    TemplateWriteFailed => "template_write_failed",
    TemplateMissingAfterWrite => "template_missing_after_write",
    TemplateDeleteFailed => "template_delete_failed",
    TemplateRegistryIo => "template_registry_io",
    ZipWriteFailed => "zip_write_failed",
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
