use utoipa::OpenApi;

use crate::{
    api,
    models::{
        BatchLabel, ErrorBody, ErrorResponse, FieldSpec, HealthResponse, OptionDetail,
        OutputOptions, RenderBatchRequest, RenderLabelRequest, TemplateDetail, TemplateFormat,
        TemplateList, TemplateSummary,
    },
};

#[derive(OpenApi)]
#[openapi(
    paths(
        api::health,
        api::list_templates,
        api::get_template,
        api::render_label,
        api::render_batch
    ),
    components(
        schemas(
            HealthResponse,
            TemplateList,
            TemplateSummary,
            TemplateFormat,
            TemplateDetail,
            OptionDetail,
            FieldSpec,
            RenderLabelRequest,
            RenderBatchRequest,
            BatchLabel,
            OutputOptions,
            ErrorResponse,
            ErrorBody
        )
    ),
    tags(
        (name = "labeler", description = "Label rendering service")
    )
)]
pub struct ApiDoc;
