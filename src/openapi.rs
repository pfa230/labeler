use utoipa::OpenApi;

use crate::{
    api::{
        auth::{
            AuthStatus, Credentials, OkResponse, PasswordChange, TokenCreate, TokenCreated,
            TokenSummary, UserSummary,
        },
        printers::{CutResponse, ProbeCapabilities, ProbeResponse},
        settings::{
            DatetimePreviewRequest, DatetimePreviewResponse, ResolvedSetting, SettingValue,
        },
    },
    models::{
        ActivityEntry, ActivityPage, BatchRowError, BatchSummary, ErrorBody, ErrorResponse,
        HealthResponse, JobRecord, LabelInput, NewPrinter, ParamControl, ParamEntry, ParamSpec,
        ParamType, ParamValue, PrintRequest, Printer, PrinterConnection, PrinterUpdate,
        ReloadResponse, RenderLabelRequest, RenderProfile, RenderRequest, SheetPosition,
        TemplateDetail, TemplateFormat, TemplateList, TemplateSummary, VariableValue,
    },
};

#[derive(OpenApi)]
#[openapi(
    servers((url = "/api")),
    components(
        schemas(
            ActivityEntry,
            ActivityPage,
            HealthResponse,
            VariableValue,
            ResolvedSetting,
            SettingValue,
            DatetimePreviewRequest,
            DatetimePreviewResponse,
            ReloadResponse,
            Printer,
            NewPrinter,
            PrinterUpdate,
            PrinterConnection,
            RenderProfile,
            ProbeResponse,
            ProbeCapabilities,
            CutResponse,
            TemplateList,
            TemplateSummary,
            TemplateFormat,
            TemplateDetail,
            ParamControl,
            ParamEntry,
            ParamSpec,
            ParamType,
            ParamValue,
            SheetPosition,
            RenderLabelRequest,
            RenderRequest,
            PrintRequest,
            BatchSummary,
            BatchRowError,
            JobRecord,
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
        (name = "auth", description = "Authentication, users, and API tokens"),
        (name = "settings", description = "Typed application configuration"),
        (name = "favorites", description = "Per-user favorites and recent templates"),
        (name = "printing", description = "Print dispatch and recorded jobs")
    )
)]
pub(crate) struct ApiMetadata;

pub fn document() -> utoipa::openapi::OpenApi {
    crate::api::documented_router().into_openapi()
}
