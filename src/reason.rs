//! The `details.reason` vocabulary.
//!
//! Each slug is API: clients switch on it, so renaming one is a breaking change. Slugs are written
//! out beside their variants rather than derived from them, so renaming a variant does not silently
//! move the wire value.

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

            /// The wire slug. Part of the API contract; see the `errors` spec.
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
    TemplateValidationFailed => "template_validation_failed",
    ReferenceUnresolved => "reference_unresolved",

    // UnsupportedLayoutItem
    CoordOutOfFrame => "coord_out_of_frame",
    ItemOutOfFrame => "item_out_of_frame",
    LineEndpointOutOfFrame => "line_endpoint_out_of_frame",
    LineDegenerate => "line_degenerate",
    EdgeRectInverted => "edge_rect_inverted",
    SizeInvalid => "size_invalid",
    TextDoesNotFit => "text_does_not_fit",
    ImageFormatUnsupported => "image_format_unsupported",
    ImageDataInvalid => "image_data_invalid",
    ImageAssetMissing => "image_asset_missing",
    ImageAssetUnreadable => "image_asset_unreadable",
    ImageAssetPathEscapes => "image_asset_path_escapes",
    AssetsDirUnavailable => "assets_dir_unavailable",
    DimensionExceedsLimit => "dimension_exceeds_limit",
    FieldValueNotScalar => "field_value_not_scalar",
    QrPayloadInvalid => "qr_payload_invalid",

    // InvalidRequest
    JsonMalformed => "json_malformed",
    RequestBodyInvalid => "request_body_invalid",
    PathParamInvalid => "path_param_invalid",
    ParamValueInvalid => "param_value_invalid",
    FieldNotApplicable => "field_not_applicable",
    FormatUnsupported => "format_unsupported",
    PrinterInvalid => "printer_invalid",
    FilterInvalid => "filter_invalid",
    RowKeyInvalid => "row_key_invalid",
    RowLimitExceeded => "row_limit_exceeded",
    StartSlotOutOfRange => "start_slot_out_of_range",
    BatchEmpty => "batch_empty",
    FormatUnknown => "format_unknown",
    TemplateIdInvalid => "template_id_invalid",
    PrinterIdInvalid => "printer_id_invalid",
    VariableKeyInvalid => "variable_key_invalid",
    SettingValueInvalid => "setting_value_invalid",
    DatetimePatternInvalid => "datetime_pattern_invalid",
    WidthBoundsInverted => "width_bounds_inverted",
    ConnectorUnknown => "connector_unknown",
    ConnectionConnectorMissing => "connection_connector_missing",
    CredentialRequired => "credential_required",
    BaseUrlInvalid => "base_url_invalid",
    PublicUrlInvalid => "public_url_invalid",
    DataKeyUnknown => "data_key_unknown",
    ColorModeUnknown => "color_mode_unknown",
    ResolutionInvalid => "resolution_invalid",
    BilevelRequiresPng => "bilevel_requires_png",
    UsernameEmpty => "username_empty",
    PasswordEmpty => "password_empty",

    // Upstream
    Auth => "auth",
    Unreachable => "unreachable",
    RateLimited => "rate_limited",
    BadResponse => "bad_response",
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
}
