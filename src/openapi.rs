use utoipa::OpenApi;

use crate::{
    api,
    models::{
        Box, Dimension, ErrorBody, ErrorResponse, FontSize, HealthResponse, HorizontalAlign,
        LabelInput, Layout, LayoutItem, Margins, Options, Point, QrParams, RenderBatchRequest,
        RenderLabelRequest, SheetPosition, TemplateDetail, TemplateFormat, TemplateList,
        TemplateSummary, VerticalAlign,
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
            Margins,
            SheetPosition,
            Dimension,
            FontSize,
            QrParams,
            HorizontalAlign,
            VerticalAlign,
            RenderLabelRequest,
            RenderBatchRequest,
            LabelInput,
            ErrorResponse,
            ErrorBody
        )
    ),
    tags(
        (name = "labeler", description = "Label rendering service")
    )
)]
pub struct ApiDoc;
