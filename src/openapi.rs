use utoipa::OpenApi;

use crate::{
    api,
    models::{
        AutoSize, Dimension, ErrorBody, ErrorResponse, Fit, FontSize, HealthResponse,
        HorizontalAlign, LabelInput, Layout, LayoutItem, Options, Placement, Point, Position,
        PrintRequest, QrParams, ReloadResponse, RenderBatchRequest, RenderLabelRequest,
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
        api::replace_template,
        api::delete_template,
        api::list_printers,
        api::create_printer,
        api::get_printer,
        api::replace_printer,
        api::delete_printer,
        api::render_label,
        api::render_batch,
        api::print
    ),
    components(
        schemas(
            HealthResponse,
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
            RenderBatchRequest,
            PrintRequest,
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
