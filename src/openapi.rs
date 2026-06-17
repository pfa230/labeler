use utoipa::OpenApi;

use crate::{
    api,
    api::{
        AuthStatus, Credentials, OkResponse, PasswordChange, TokenCreate, TokenCreated,
        TokenSummary, UserSummary,
    },
    models::{
        AutoSize, BatchRequest, BatchRowError, BatchSummary, Dimension, ErrorBody, ErrorResponse,
        Fit, FontSize, HealthResponse, HorizontalAlign, LabelInput, Layout, LayoutItem, Options,
        Placement, Point, Position, QrParams, ReloadResponse, RenderLabelRequest, SettingValue,
        SheetPosition, Size, SizeValue, TemplateDetail, TemplateFormat, TemplateList,
        TemplateSummary, VerticalAlign,
    },
    store::Printer,
};

#[derive(OpenApi)]
#[openapi(
    paths(
        api::health,
        api::list_templates,
        api::create_template,
        api::reload_templates,
        api::get_template,
        api::template_source,
        api::replace_template,
        api::delete_template,
        api::list_printers,
        api::create_printer,
        api::get_printer,
        api::replace_printer,
        api::delete_printer,
        api::get_settings,
        api::put_setting,
        api::render_label,
        api::batch,
        api::import_csv,
        api::setup,
        api::login,
        api::logout,
        api::me,
        api::change_password,
        api::list_users,
        api::create_user_h,
        api::delete_user_h,
        api::list_tokens,
        api::create_token_h,
        api::delete_token_h
    ),
    servers((url = "/api")),
    components(
        schemas(
            HealthResponse,
            SettingValue,
            ReloadResponse,
            Printer,
            TemplateList,
            TemplateSummary,
            TemplateFormat,
            TemplateDetail,
            Options,
            LayoutItem,
            Layout,
            Point,
            Position,
            Placement,
            SheetPosition,
            Dimension,
            Size,
            SizeValue,
            AutoSize,
            FontSize,
            Fit,
            QrParams,
            HorizontalAlign,
            VerticalAlign,
            RenderLabelRequest,
            BatchRequest,
            BatchSummary,
            BatchRowError,
            LabelInput,
            ErrorResponse,
            ErrorBody,
            Credentials,
            PasswordChange,
            TokenCreate,
            AuthStatus,
            UserSummary,
            TokenSummary,
            TokenCreated,
            OkResponse
        )
    ),
    tags(
        (name = "labeler", description = "Label rendering service"),
        (name = "auth", description = "Authentication, users, and API tokens")
    )
)]
pub struct ApiDoc;
