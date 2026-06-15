use utoipa::OpenApi;

use crate::{
    api,
    models::{
        AutoSize, Dimension, ErrorBody, ErrorResponse, Fit, FontSize, HealthResponse,
        HorizontalAlign, LabelInput, Layout, LayoutItem, Options, Placement, Point, Position,
        QrParams, RenderBatchRequest, RenderLabelRequest, SheetPosition, Size, SizeValue,
        TemplateDetail, TemplateFormat, TemplateList, TemplateSummary, VerticalAlign,
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
