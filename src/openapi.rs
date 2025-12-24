use utoipa::OpenApi;

use crate::{
    api,
    models::{
        BatchLabel, Box, Dimension, ErrorBody, ErrorResponse, FontSize, HealthResponse,
        HorizontalAlign, Layout, LayoutItem, Options, OutputOptions, Point, QrParams,
        RenderBatchRequest, VerticalAlign,
        RenderLabelRequest, TemplateDetail, TemplateFormat, TemplateList, TemplateSummary,
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
            Options,
            LayoutItem,
            Layout,
            Box,
            Point,
            Dimension,
            FontSize,
            QrParams,
            HorizontalAlign,
            VerticalAlign,
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
