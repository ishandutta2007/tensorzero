use std::collections::HashMap;
use std::time::Duration;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use serde::{Serialize, Serializer};
use serde_json::{json, Value};
use std::fmt::{Debug, Display};
use thiserror::Error;
use tokio::sync::OnceCell;
use url::Url;
use uuid::Uuid;

use crate::inference::types::storage::StoragePath;
use crate::inference::types::Thought;

/// Controls whether to include raw request/response details in error output
///
/// When true:
/// - Raw request/response details are logged for inference provider errors
/// - Raw details are included in error response bodies
/// - Most commonly affects errors from provider API requests/responses
///
/// WARNING: Setting this to true will expose potentially sensitive request/response
/// data in logs and error responses. Use with caution.
static DEBUG: OnceCell<bool> =
    if cfg!(feature = "e2e_tests") || cfg!(feature = "optimization_tests") {
        OnceCell::const_new_with(true)
    } else {
        OnceCell::const_new()
    };

pub fn set_debug(debug: bool) -> Result<(), Error> {
    // We already initialized `DEBUG`, so do nothing
    if cfg!(feature = "e2e_tests") {
        return Ok(());
    }
    DEBUG.set(debug).map_err(|_| {
        Error::new(ErrorDetails::Config {
            message: "Failed to set debug mode".to_string(),
        })
    })
}

static UNSTABLE_ERROR_JSON: OnceCell<bool> = OnceCell::const_new();

pub fn set_unstable_error_json(unstable_error_json: bool) -> Result<(), Error> {
    UNSTABLE_ERROR_JSON.set(unstable_error_json).map_err(|_| {
        Error::new(ErrorDetails::Config {
            message: "Failed to set unstable error JSON".to_string(),
        })
    })
}

pub fn warn_discarded_cache_write(raw_response: &str) {
    if *DEBUG.get().unwrap_or(&false) {
        tracing::warn!("Skipping cache write due to invalid output:\nRaw response: {raw_response}");
    } else {
        tracing::warn!("Skipping cache write due to invalid output");
    }
}

pub fn warn_discarded_thought_block(provider_type: &str, thought: &Thought) {
    if *DEBUG.get().unwrap_or(&false) {
        tracing::warn!("Provider type `{provider_type}` does not support input thought blocks, discarding: {thought:?}");
    } else {
        tracing::warn!(
            "Provider type `{provider_type}` does not support input thought blocks, discarding"
        );
    }
}

pub fn warn_discarded_unknown_chunk(provider_type: &str, part: &str) {
    if *DEBUG.get().unwrap_or(&false) {
        tracing::warn!("Discarding unknown chunk in {provider_type} response: {part}");
    } else {
        tracing::warn!("Discarding unknown chunk in {provider_type} response");
    }
}

pub const IMPOSSIBLE_ERROR_MESSAGE: &str = "This should never happen, please file a bug report at https://github.com/tensorzero/tensorzero/discussions/new?category=bug-reports";

/// Chooses between a `Debug` or `Display` representation based on the gateway-level `DEBUG` flag.
pub struct DisplayOrDebugGateway<T: Debug + Display> {
    val: T,
}

impl<T: Debug + Display> DisplayOrDebugGateway<T> {
    pub fn new(val: T) -> Self {
        Self { val }
    }
}

impl<T: Debug + Display> Display for DisplayOrDebugGateway<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if *DEBUG.get().unwrap_or(&false) {
            write!(f, "{:?}", self.val)
        } else {
            write!(f, "{}", self.val)
        }
    }
}

#[derive(Debug, Error, Serialize)]
#[cfg_attr(any(test, feature = "e2e_tests"), derive(PartialEq))]
#[error(transparent)]
// As long as the struct member is private, we force people to use the `new` method and log the error.
// We box `ErrorDetails` per the `clippy::result_large_err` lint
pub struct Error(Box<ErrorDetails>);

impl Error {
    pub fn new(details: ErrorDetails) -> Self {
        details.log();
        Error(Box::new(details))
    }

    pub fn new_without_logging(details: ErrorDetails) -> Self {
        Error(Box::new(details))
    }

    pub fn status_code(&self) -> StatusCode {
        self.0.status_code()
    }

    pub fn get_details(&self) -> &ErrorDetails {
        &self.0
    }

    pub fn get_owned_details(self) -> ErrorDetails {
        *self.0
    }

    pub fn log(&self) {
        self.0.log();
    }
}

// Expect for derive Serialize
#[expect(clippy::trivially_copy_pass_by_ref)]
fn serialize_status<S>(code: &Option<StatusCode>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match code {
        Some(c) => serializer.serialize_u16(c.as_u16()),
        None => serializer.serialize_none(),
    }
}

fn serialize_if_debug<T, S>(data: T, serializer: S) -> Result<S::Ok, S::Error>
where
    T: Serialize,
    S: Serializer,
{
    if *DEBUG.get().unwrap_or(&false) {
        return data.serialize(serializer);
    }
    serializer.serialize_none()
}

impl From<ErrorDetails> for Error {
    fn from(details: ErrorDetails) -> Self {
        Error::new(details)
    }
}

#[derive(Debug, Error, Serialize)]
#[cfg_attr(any(test, feature = "e2e_tests"), derive(PartialEq))]
pub enum ErrorDetails {
    AllVariantsFailed {
        errors: HashMap<String, Error>,
    },
    InvalidInferenceTarget {
        message: String,
    },
    ApiKeyMissing {
        provider_name: String,
    },
    AppState {
        message: String,
    },
    BadCredentialsPreInference {
        provider_name: String,
    },
    BatchInputValidation {
        index: usize,
        message: String,
    },
    BatchNotFound {
        id: Uuid,
    },
    BadImageFetch {
        url: Url,
        message: String,
    },
    Cache {
        message: String,
    },
    ChannelWrite {
        message: String,
    },
    ClickHouseConnection {
        message: String,
    },
    ClickHouseDeserialization {
        message: String,
    },
    ClickHouseMigration {
        id: String,
        message: String,
    },
    ClickHouseQuery {
        message: String,
    },
    Config {
        message: String,
    },
    ObjectStoreUnconfigured {
        block_type: String,
    },
    DatapointNotFound {
        dataset_name: String,
        datapoint_id: Uuid,
    },
    DuplicateTool {
        name: String,
    },
    DynamicJsonSchema {
        message: String,
    },
    FileRead {
        message: String,
        file_path: String,
    },
    GCPCredentials {
        message: String,
    },
    Inference {
        message: String,
    },
    InferenceClient {
        message: String,
        #[serde(serialize_with = "serialize_status")]
        status_code: Option<StatusCode>,
        provider_type: String,
        #[serde(serialize_with = "serialize_if_debug")]
        raw_request: Option<String>,
        #[serde(serialize_with = "serialize_if_debug")]
        raw_response: Option<String>,
    },
    InferenceNotFound {
        inference_id: Uuid,
    },
    InferenceServer {
        message: String,
        provider_type: String,
        #[serde(serialize_with = "serialize_if_debug")]
        raw_request: Option<String>,
        #[serde(serialize_with = "serialize_if_debug")]
        raw_response: Option<String>,
    },
    InvalidClientMode {
        mode: String,
        message: String,
    },
    InvalidDynamicTemplatePath {
        name: String,
    },
    InvalidEncodedJobHandle,
    InvalidJobHandle {
        message: String,
    },
    InvalidInferenceOutputSource {
        source_kind: String,
    },
    ObjectStoreWrite {
        message: String,
        path: StoragePath,
    },
    InternalError {
        message: String,
    },
    InferenceTimeout {
        variant_name: String,
    },
    VariantTimeout {
        variant_name: String,
        timeout: Duration,
        streaming: bool,
    },
    ModelTimeout {
        model_name: String,
        timeout: Duration,
        streaming: bool,
    },
    ModelProviderTimeout {
        provider_name: String,
        timeout: Duration,
        streaming: bool,
    },
    InputValidation {
        source: Box<Error>,
    },
    InvalidBatchParams {
        message: String,
    },
    InvalidBaseUrl {
        message: String,
    },
    InvalidCandidate {
        variant_name: String,
        message: String,
    },
    InvalidDatasetName {
        dataset_name: String,
    },
    InvalidDiclConfig {
        message: String,
    },
    InvalidDynamicEvaluationRun {
        episode_id: Uuid,
    },
    InvalidTensorzeroUuid {
        kind: String,
        message: String,
    },
    InvalidFunctionVariants {
        message: String,
    },
    InvalidMetricName {
        metric_name: String,
    },
    InvalidMessage {
        message: String,
    },
    InvalidModel {
        model_name: String,
    },
    InvalidModelProvider {
        model_name: String,
        provider_name: String,
    },
    InvalidOpenAICompatibleRequest {
        message: String,
    },
    InvalidProviderConfig {
        message: String,
    },
    InvalidRenderedStoredInference {
        message: String,
    },
    InvalidRequest {
        message: String,
    },
    InvalidTemplatePath,
    InvalidTool {
        message: String,
    },
    InvalidVariantForOptimization {
        function_name: String,
        variant_name: String,
    },
    InvalidValFraction {
        val_fraction: f64,
    },
    InvalidUuid {
        raw_uuid: String,
    },
    JsonRequest {
        message: String,
    },
    JsonSchema {
        message: String,
    },
    JsonSchemaValidation {
        messages: Vec<String>,
        data: Box<Value>,
        schema: Box<Value>,
    },
    MissingFunctionInVariants {
        function_name: String,
    },
    MiniJinjaEnvironment {
        message: String,
    },
    MiniJinjaTemplate {
        template_name: String,
        message: String,
    },
    MiniJinjaTemplateMissing {
        template_name: String,
    },
    MiniJinjaTemplateRender {
        template_name: String,
        message: String,
    },
    MissingBatchInferenceResponse {
        inference_id: Option<Uuid>,
    },
    MissingFileExtension {
        file_name: String,
    },
    ModelProvidersExhausted {
        provider_errors: HashMap<String, Error>,
    },
    ModelValidation {
        message: String,
    },
    Observability {
        message: String,
    },
    OutputParsing {
        message: String,
        raw_output: String,
    },
    OptimizationResponse {
        message: String,
        provider_type: String,
    },
    OutputValidation {
        source: Box<Error>,
    },
    ProviderNotFound {
        provider_name: String,
    },
    Serialization {
        message: String,
    },
    ExtraBodyReplacement {
        message: String,
        pointer: String,
    },
    StreamError {
        source: Box<Error>,
    },
    ToolNotFound {
        name: String,
    },
    ToolNotLoaded {
        name: String,
    },
    TypeConversion {
        message: String,
    },
    UnknownCandidate {
        name: String,
    },
    UnknownEvaluation {
        name: String,
    },
    UnknownFunction {
        name: String,
    },
    UnknownModel {
        name: String,
    },
    UnknownTool {
        name: String,
    },
    UnknownVariant {
        name: String,
    },
    UnknownMetric {
        name: String,
    },
    UnsupportedModelProviderForBatchInference {
        provider_type: String,
    },
    UnsupportedVariantForBatchInference {
        variant_name: Option<String>,
    },
    UnsupportedVariantForStreamingInference {
        variant_type: String,
        issue_link: Option<String>,
    },
    UnsupportedVariantForFunctionType {
        function_name: String,
        variant_name: String,
        function_type: String,
        variant_type: String,
    },
    UnsupportedContentBlockType {
        content_block_type: String,
        provider_type: String,
    },
    UuidInFuture {
        raw_uuid: String,
    },
    UnsupportedFileExtension {
        extension: String,
    },
    RouteNotFound {
        path: String,
        method: String,
    },
}

impl ErrorDetails {
    /// Defines the error level for logging this error
    fn level(&self) -> tracing::Level {
        match self {
            ErrorDetails::AllVariantsFailed { .. } => tracing::Level::ERROR,
            ErrorDetails::ApiKeyMissing { .. } => tracing::Level::ERROR,
            ErrorDetails::AppState { .. } => tracing::Level::ERROR,
            ErrorDetails::ObjectStoreUnconfigured { .. } => tracing::Level::ERROR,
            ErrorDetails::ExtraBodyReplacement { .. } => tracing::Level::ERROR,
            ErrorDetails::InvalidInferenceTarget { .. } => tracing::Level::WARN,
            ErrorDetails::BadCredentialsPreInference { .. } => tracing::Level::ERROR,
            ErrorDetails::UnsupportedContentBlockType { .. } => tracing::Level::WARN,
            ErrorDetails::BatchInputValidation { .. } => tracing::Level::WARN,
            ErrorDetails::BatchNotFound { .. } => tracing::Level::WARN,
            ErrorDetails::Cache { .. } => tracing::Level::WARN,
            ErrorDetails::ChannelWrite { .. } => tracing::Level::ERROR,
            ErrorDetails::ClickHouseConnection { .. } => tracing::Level::ERROR,
            ErrorDetails::BadImageFetch { .. } => tracing::Level::ERROR,
            ErrorDetails::ClickHouseDeserialization { .. } => tracing::Level::ERROR,
            ErrorDetails::ClickHouseMigration { .. } => tracing::Level::ERROR,
            ErrorDetails::ClickHouseQuery { .. } => tracing::Level::ERROR,
            ErrorDetails::ObjectStoreWrite { .. } => tracing::Level::ERROR,
            ErrorDetails::Config { .. } => tracing::Level::ERROR,
            ErrorDetails::DatapointNotFound { .. } => tracing::Level::WARN,
            ErrorDetails::DuplicateTool { .. } => tracing::Level::WARN,
            ErrorDetails::DynamicJsonSchema { .. } => tracing::Level::WARN,
            ErrorDetails::FileRead { .. } => tracing::Level::ERROR,
            ErrorDetails::GCPCredentials { .. } => tracing::Level::ERROR,
            ErrorDetails::Inference { .. } => tracing::Level::ERROR,
            ErrorDetails::InferenceClient { .. } => tracing::Level::ERROR,
            ErrorDetails::InferenceNotFound { .. } => tracing::Level::WARN,
            ErrorDetails::InferenceServer { .. } => tracing::Level::ERROR,
            ErrorDetails::InferenceTimeout { .. } => tracing::Level::WARN,
            ErrorDetails::ModelProviderTimeout { .. } => tracing::Level::WARN,
            ErrorDetails::ModelTimeout { .. } => tracing::Level::WARN,
            ErrorDetails::VariantTimeout { .. } => tracing::Level::WARN,
            ErrorDetails::InputValidation { .. } => tracing::Level::WARN,
            ErrorDetails::InternalError { .. } => tracing::Level::ERROR,
            ErrorDetails::InvalidBaseUrl { .. } => tracing::Level::ERROR,
            ErrorDetails::InvalidBatchParams { .. } => tracing::Level::ERROR,
            ErrorDetails::InvalidCandidate { .. } => tracing::Level::ERROR,
            ErrorDetails::InvalidClientMode { .. } => tracing::Level::ERROR,
            ErrorDetails::InvalidDiclConfig { .. } => tracing::Level::ERROR,
            ErrorDetails::InvalidDatasetName { .. } => tracing::Level::WARN,
            ErrorDetails::InvalidDynamicEvaluationRun { .. } => tracing::Level::ERROR,
            ErrorDetails::InvalidInferenceOutputSource { .. } => tracing::Level::WARN,
            ErrorDetails::InvalidTensorzeroUuid { .. } => tracing::Level::WARN,
            ErrorDetails::InvalidFunctionVariants { .. } => tracing::Level::ERROR,
            ErrorDetails::InvalidVariantForOptimization { .. } => tracing::Level::WARN,
            ErrorDetails::InvalidDynamicTemplatePath { .. } => tracing::Level::WARN,
            ErrorDetails::InvalidEncodedJobHandle => tracing::Level::WARN,
            ErrorDetails::InvalidJobHandle { .. } => tracing::Level::WARN,
            ErrorDetails::InvalidRenderedStoredInference { .. } => tracing::Level::ERROR,
            ErrorDetails::InvalidMetricName { .. } => tracing::Level::WARN,
            ErrorDetails::InvalidMessage { .. } => tracing::Level::WARN,
            ErrorDetails::InvalidModel { .. } => tracing::Level::ERROR,
            ErrorDetails::InvalidModelProvider { .. } => tracing::Level::ERROR,
            ErrorDetails::InvalidOpenAICompatibleRequest { .. } => tracing::Level::ERROR,
            ErrorDetails::InvalidProviderConfig { .. } => tracing::Level::ERROR,
            ErrorDetails::InvalidRequest { .. } => tracing::Level::WARN,
            ErrorDetails::InvalidTemplatePath => tracing::Level::ERROR,
            ErrorDetails::InvalidTool { .. } => tracing::Level::ERROR,
            ErrorDetails::InvalidUuid { .. } => tracing::Level::ERROR,
            ErrorDetails::InvalidValFraction { .. } => tracing::Level::WARN,
            ErrorDetails::JsonRequest { .. } => tracing::Level::WARN,
            ErrorDetails::JsonSchema { .. } => tracing::Level::ERROR,
            ErrorDetails::JsonSchemaValidation { .. } => tracing::Level::ERROR,
            ErrorDetails::MiniJinjaEnvironment { .. } => tracing::Level::ERROR,
            ErrorDetails::MiniJinjaTemplate { .. } => tracing::Level::ERROR,
            ErrorDetails::MiniJinjaTemplateMissing { .. } => tracing::Level::ERROR,
            ErrorDetails::MiniJinjaTemplateRender { .. } => tracing::Level::ERROR,
            ErrorDetails::MissingFunctionInVariants { .. } => tracing::Level::ERROR,
            ErrorDetails::MissingBatchInferenceResponse { .. } => tracing::Level::WARN,
            ErrorDetails::MissingFileExtension { .. } => tracing::Level::WARN,
            ErrorDetails::ModelProvidersExhausted { .. } => tracing::Level::ERROR,
            ErrorDetails::ModelValidation { .. } => tracing::Level::ERROR,
            ErrorDetails::Observability { .. } => tracing::Level::WARN,
            ErrorDetails::OutputParsing { .. } => tracing::Level::WARN,
            ErrorDetails::OutputValidation { .. } => tracing::Level::WARN,
            ErrorDetails::OptimizationResponse { .. } => tracing::Level::ERROR,
            ErrorDetails::ProviderNotFound { .. } => tracing::Level::ERROR,
            ErrorDetails::Serialization { .. } => tracing::Level::ERROR,
            ErrorDetails::StreamError { .. } => tracing::Level::ERROR,
            ErrorDetails::ToolNotFound { .. } => tracing::Level::WARN,
            ErrorDetails::ToolNotLoaded { .. } => tracing::Level::ERROR,
            ErrorDetails::TypeConversion { .. } => tracing::Level::ERROR,
            ErrorDetails::UnknownCandidate { .. } => tracing::Level::ERROR,
            ErrorDetails::UnknownFunction { .. } => tracing::Level::WARN,
            ErrorDetails::UnknownEvaluation { .. } => tracing::Level::WARN,
            ErrorDetails::UnknownModel { .. } => tracing::Level::ERROR,
            ErrorDetails::UnknownTool { .. } => tracing::Level::ERROR,
            ErrorDetails::UnknownVariant { .. } => tracing::Level::WARN,
            ErrorDetails::UnknownMetric { .. } => tracing::Level::WARN,
            ErrorDetails::UnsupportedFileExtension { .. } => tracing::Level::WARN,
            ErrorDetails::UnsupportedModelProviderForBatchInference { .. } => tracing::Level::WARN,
            ErrorDetails::UnsupportedVariantForBatchInference { .. } => tracing::Level::WARN,
            ErrorDetails::UnsupportedVariantForFunctionType { .. } => tracing::Level::ERROR,
            ErrorDetails::UnsupportedVariantForStreamingInference { .. } => tracing::Level::WARN,
            ErrorDetails::UuidInFuture { .. } => tracing::Level::WARN,
            ErrorDetails::RouteNotFound { .. } => tracing::Level::WARN,
        }
    }

    /// Defines the HTTP status code for responses involving this error
    fn status_code(&self) -> StatusCode {
        match self {
            ErrorDetails::AllVariantsFailed { .. } => StatusCode::BAD_GATEWAY,
            ErrorDetails::ApiKeyMissing { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::ExtraBodyReplacement { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::AppState { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::BadCredentialsPreInference { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::BatchInputValidation { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::BatchNotFound { .. } => StatusCode::NOT_FOUND,
            ErrorDetails::Cache { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::ChannelWrite { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::ClickHouseConnection { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::ClickHouseDeserialization { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::ClickHouseMigration { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::ClickHouseQuery { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::ObjectStoreUnconfigured { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::DatapointNotFound { .. } => StatusCode::NOT_FOUND,
            ErrorDetails::Config { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::DuplicateTool { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::DynamicJsonSchema { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::FileRead { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::GCPCredentials { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::InvalidInferenceTarget { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::Inference { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::ObjectStoreWrite { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::InferenceClient { status_code, .. } => {
                status_code.unwrap_or_else(|| StatusCode::INTERNAL_SERVER_ERROR)
            }
            ErrorDetails::BadImageFetch { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::InferenceNotFound { .. } => StatusCode::NOT_FOUND,
            ErrorDetails::InferenceServer { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::InferenceTimeout { .. } => StatusCode::REQUEST_TIMEOUT,
            ErrorDetails::ModelProviderTimeout { .. } => StatusCode::REQUEST_TIMEOUT,
            ErrorDetails::ModelTimeout { .. } => StatusCode::REQUEST_TIMEOUT,
            ErrorDetails::VariantTimeout { .. } => StatusCode::REQUEST_TIMEOUT,
            ErrorDetails::InvalidClientMode { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::InvalidEncodedJobHandle => StatusCode::BAD_REQUEST,
            ErrorDetails::InvalidJobHandle { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::InvalidTensorzeroUuid { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::InvalidUuid { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::InputValidation { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::InternalError { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::InvalidBaseUrl { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::InvalidValFraction { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::UnsupportedContentBlockType { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::InvalidBatchParams { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::InvalidCandidate { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::InvalidDiclConfig { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::InvalidDatasetName { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::InvalidDynamicEvaluationRun { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::InvalidDynamicTemplatePath { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::InvalidFunctionVariants { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::InvalidInferenceOutputSource { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::InvalidMessage { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::InvalidMetricName { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::InvalidModel { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::InvalidModelProvider { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::InvalidOpenAICompatibleRequest { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::InvalidProviderConfig { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::InvalidRequest { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::InvalidRenderedStoredInference { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::InvalidTemplatePath => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::InvalidTool { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::InvalidVariantForOptimization { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::JsonRequest { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::JsonSchema { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::JsonSchemaValidation { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::MiniJinjaEnvironment { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::MiniJinjaTemplate { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::MiniJinjaTemplateMissing { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::MiniJinjaTemplateRender { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::MissingBatchInferenceResponse { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::MissingFunctionInVariants { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::MissingFileExtension { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::ModelProvidersExhausted { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::ModelValidation { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::Observability { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::OptimizationResponse { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::OutputParsing { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::OutputValidation { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::ProviderNotFound { .. } => StatusCode::NOT_FOUND,
            ErrorDetails::Serialization { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::StreamError { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::ToolNotFound { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::ToolNotLoaded { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::TypeConversion { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::UnknownCandidate { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::UnknownFunction { .. } => StatusCode::NOT_FOUND,
            ErrorDetails::UnknownEvaluation { .. } => StatusCode::NOT_FOUND,
            ErrorDetails::UnknownModel { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::UnknownTool { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorDetails::UnknownVariant { .. } => StatusCode::NOT_FOUND,
            ErrorDetails::UnknownMetric { .. } => StatusCode::NOT_FOUND,
            ErrorDetails::UnsupportedFileExtension { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::UnsupportedModelProviderForBatchInference { .. } => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            ErrorDetails::UnsupportedVariantForBatchInference { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::UnsupportedVariantForStreamingInference { .. } => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            ErrorDetails::UnsupportedVariantForFunctionType { .. } => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            ErrorDetails::UuidInFuture { .. } => StatusCode::BAD_REQUEST,
            ErrorDetails::RouteNotFound { .. } => StatusCode::NOT_FOUND,
        }
    }

    /// Log the error using the `tracing` library
    pub fn log(&self) {
        match self.level() {
            tracing::Level::ERROR => tracing::error!("{self}"),
            tracing::Level::WARN => tracing::warn!("{self}"),
            tracing::Level::INFO => tracing::info!("{self}"),
            tracing::Level::DEBUG => tracing::debug!("{self}"),
            tracing::Level::TRACE => tracing::trace!("{self}"),
        }
    }
}

impl std::fmt::Display for ErrorDetails {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorDetails::AllVariantsFailed { errors } => {
                write!(
                    f,
                    "All variants failed with errors: {}",
                    errors
                        .iter()
                        .map(|(variant_name, error)| format!("{variant_name}: {error}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            }
            ErrorDetails::ModelProviderTimeout {
                provider_name,
                timeout,
                streaming,
            } => {
                if *streaming {
                    write!(
                        f,
                        "Model provider {provider_name} timed out due to configured `streaming.ttft_ms` timeout ({timeout:?})"
                    )
                } else {
                    write!(
                        f,
                        "Model provider {provider_name} timed out due to configured `non_streaming.total_ms` timeout ({timeout:?})"
                    )
                }
            }
            ErrorDetails::ModelTimeout {
                model_name,
                timeout,
                streaming,
            } => {
                if *streaming {
                    write!(f, "Model {model_name} timed out due to configured `streaming.ttft_ms` timeout ({timeout:?})")
                } else {
                    write!(f, "Model {model_name} timed out due to configured `non_streaming.total_ms` timeout ({timeout:?})")
                }
            }
            ErrorDetails::VariantTimeout {
                variant_name,
                timeout,
                streaming,
            } => {
                let variant_description = format!("Variant `{variant_name}`");
                if *streaming {
                    write!(f, "{variant_description} timed out due to configured `streaming.ttft_ms` timeout ({timeout:?})")
                } else {
                    write!(f, "{variant_description} timed out due to configured `non_streaming.total_ms` timeout ({timeout:?})")
                }
            }
            ErrorDetails::ObjectStoreWrite { message, path } => {
                write!(
                    f,
                    "Error writing to object store: `{message}`. Path: {path:?}"
                )
            }
            ErrorDetails::InvalidInferenceTarget { message } => {
                write!(f, "Invalid inference target: {message}")
            }
            ErrorDetails::BadImageFetch { url, message } => {
                write!(f, "Error fetching image from {url}: {message}")
            }
            ErrorDetails::ObjectStoreUnconfigured { block_type } => {
                write!(f, "Object storage is not configured. You must configure `[object_storage]` before making requests containing a `{block_type}` content block. If you don't want to use object storage, you can explicitly set `object_storage.type = \"disabled\"` in your configuration.")
            }
            ErrorDetails::UnsupportedContentBlockType {
                content_block_type,
                provider_type,
            } => {
                write!(
                    f,
                    "Unsupported content block type `{content_block_type}` for provider `{provider_type}`",
                )
            }
            ErrorDetails::ExtraBodyReplacement { message, pointer } => {
                write!(
                    f,
                    "Error replacing extra body: `{message}` with pointer: `{pointer}`"
                )
            }
            ErrorDetails::ApiKeyMissing { provider_name } => {
                write!(f, "API key missing for provider: {provider_name}")
            }
            ErrorDetails::AppState { message } => {
                write!(f, "Error initializing AppState: {message}")
            }
            ErrorDetails::BadCredentialsPreInference { provider_name } => {
                write!(
                    f,
                    "Bad credentials at inference time for provider: {provider_name}. This should never happen. Please file a bug report: https://github.com/tensorzero/tensorzero/issues/new"
                )
            }
            ErrorDetails::BatchInputValidation { index, message } => {
                write!(f, "Input at index {index} failed validation: {message}",)
            }
            ErrorDetails::BatchNotFound { id } => {
                write!(f, "Batch request not found for id: {id}")
            }
            ErrorDetails::Cache { message } => {
                write!(f, "Error in cache: {message}")
            }
            ErrorDetails::ChannelWrite { message } => {
                write!(f, "Error writing to channel: {message}")
            }
            ErrorDetails::ClickHouseConnection { message } => {
                write!(f, "Error connecting to ClickHouse: {message}")
            }
            ErrorDetails::ClickHouseDeserialization { message } => {
                write!(f, "Error deserializing ClickHouse response: {message}")
            }
            ErrorDetails::ClickHouseMigration { id, message } => {
                write!(f, "Error running ClickHouse migration {id}: {message}")
            }
            ErrorDetails::ClickHouseQuery { message } => {
                write!(f, "Failed to run ClickHouse query: {message}")
            }
            ErrorDetails::Config { message } => {
                write!(f, "{message}")
            }
            ErrorDetails::DatapointNotFound {
                dataset_name,
                datapoint_id,
            } => {
                write!(
                    f,
                    "Datapoint not found for dataset: {dataset_name} and id: {datapoint_id}"
                )
            }
            ErrorDetails::DuplicateTool { name } => {
                write!(f, "Duplicate tool name: {name}. Tool names must be unique.")
            }
            ErrorDetails::DynamicJsonSchema { message } => {
                write!(
                    f,
                    "Error in compiling client-provided JSON schema: {message}"
                )
            }
            ErrorDetails::FileRead { message, file_path } => {
                write!(f, "Error reading file {file_path}: {message}")
            }
            ErrorDetails::GCPCredentials { message } => {
                write!(f, "Error in acquiring GCP credentials: {message}")
            }
            ErrorDetails::Inference { message } => write!(f, "{message}"),
            ErrorDetails::InferenceClient {
                message,
                provider_type,
                raw_request,
                raw_response,
                status_code,
            } => {
                // `debug` defaults to false so we don't log raw request and response by default
                if *DEBUG.get().unwrap_or(&false) {
                    write!(
                        f,
                        "Error from {} client: {}{}{}",
                        provider_type,
                        message,
                        raw_request
                            .as_ref()
                            .map_or(String::new(), |r| format!("\nRaw request: {r}")),
                        raw_response
                            .as_ref()
                            .map_or(String::new(), |r| format!("\nRaw response: {r}"))
                    )
                } else {
                    write!(
                        f,
                        "Error{} from {} client: {}",
                        status_code.map_or(String::new(), |s| format!(" {s}")),
                        provider_type,
                        message
                    )
                }
            }
            ErrorDetails::InferenceNotFound { inference_id } => {
                write!(f, "Inference not found for id: {inference_id}")
            }
            ErrorDetails::InferenceServer {
                message,
                provider_type,
                raw_request,
                raw_response,
            } => {
                // `debug` defaults to false so we don't log raw request and response by default
                if *DEBUG.get().unwrap_or(&false) {
                    write!(
                        f,
                        "Error from {} server: {}{}{}",
                        provider_type,
                        message,
                        raw_request
                            .as_ref()
                            .map_or(String::new(), |r| format!("\nRaw request: {r}")),
                        raw_response
                            .as_ref()
                            .map_or(String::new(), |r| format!("\nRaw response: {r}"))
                    )
                } else {
                    write!(f, "Error from {provider_type} server: {message}")
                }
            }
            ErrorDetails::InferenceTimeout { variant_name } => {
                write!(f, "Inference timed out for variant: {variant_name}")
            }
            ErrorDetails::InputValidation { source } => {
                write!(f, "Input validation failed with messages: {source}")
            }
            ErrorDetails::InternalError { message } => {
                write!(f, "Internal error: {message}")
            }
            ErrorDetails::InvalidBaseUrl { message } => {
                write!(f, "Invalid batch params retrieved from database: {message}")
            }
            ErrorDetails::InvalidBatchParams { message } => write!(f, "{message}"),
            ErrorDetails::InvalidCandidate {
                variant_name,
                message,
            } => {
                write!(
                    f,
                    "Invalid candidate variant as a component of variant {variant_name}: {message}"
                )
            }
            ErrorDetails::InvalidClientMode { mode, message } => {
                write!(f, "Invalid client mode: {mode}. {message}")
            }
            ErrorDetails::InvalidDiclConfig { message } => {
                write!(f, "Invalid dynamic in-context learning config: {message}. This should never happen. Please file a bug report: https://github.com/tensorzero/tensorzero/issues/new")
            }
            ErrorDetails::InvalidDatasetName { dataset_name } => {
                write!(f, "Invalid dataset name: {dataset_name}. Datasets cannot be named \"builder\" or begin with \"tensorzero::\"")
            }
            ErrorDetails::InvalidDynamicEvaluationRun { episode_id } => {
                write!(
                    f,
                    "Dynamic evaluation run not found for episode id: {episode_id}",
                )
            }
            ErrorDetails::InvalidDynamicTemplatePath { name } => {
                write!(f, "Invalid dynamic template path: {name}. There is likely a duplicate template in the config.")
            }
            ErrorDetails::InvalidEncodedJobHandle => {
                write!(
                    f,
                    "Invalid encoded job handle. Failed to decode using URL-safe Base64."
                )
            }
            ErrorDetails::InvalidJobHandle { message } => {
                write!(f, "Failed to deserialize job handle: {message}")
            }
            ErrorDetails::InvalidFunctionVariants { message } => write!(f, "{message}"),
            ErrorDetails::InvalidTensorzeroUuid { message, kind } => {
                write!(f, "Invalid {kind} ID: {message}")
            }
            ErrorDetails::InvalidInferenceOutputSource { source_kind } => {
                write!(f, "Invalid inference output source: {source_kind}. Should be one of: \"inference\" or \"demonstration\".")
            }
            ErrorDetails::InvalidMetricName { metric_name } => {
                write!(f, "Invalid metric name: {metric_name}")
            }
            ErrorDetails::InvalidMessage { message } => write!(f, "{message}"),
            ErrorDetails::InvalidModel { model_name } => {
                write!(f, "Invalid model: {model_name}")
            }
            ErrorDetails::InvalidModelProvider {
                model_name,
                provider_name,
            } => {
                write!(
                    f,
                    "Invalid model provider: {provider_name} for model: {model_name}"
                )
            }
            ErrorDetails::InvalidValFraction { val_fraction } => {
                write!(
                    f,
                    "Invalid val fraction: {val_fraction}. Must be between 0 and 1."
                )
            }
            ErrorDetails::InvalidOpenAICompatibleRequest { message } => write!(
                f,
                "Invalid request to OpenAI-compatible endpoint: {message}"
            ),
            ErrorDetails::InvalidProviderConfig { message } => write!(f, "{message}"),
            ErrorDetails::InvalidRequest { message } => write!(f, "{message}"),
            ErrorDetails::InvalidRenderedStoredInference { message } => {
                write!(f, "Invalid rendered stored inference: {message}")
            }
            ErrorDetails::InvalidTemplatePath => {
                write!(f, "Template path failed to convert to Rust string")
            }
            ErrorDetails::InvalidTool { message } => write!(f, "{message}"),
            ErrorDetails::InvalidUuid { raw_uuid } => {
                write!(f, "Failed to parse UUID as v7: {raw_uuid}")
            }
            ErrorDetails::InvalidVariantForOptimization {
                function_name,
                variant_name,
            } => {
                write!(f, "Invalid variant for optimization: {variant_name} for function: {function_name}")
            }
            ErrorDetails::JsonRequest { message } => write!(f, "{message}"),
            ErrorDetails::JsonSchema { message } => write!(f, "{message}"),
            ErrorDetails::JsonSchemaValidation {
                messages,
                data,
                schema,
            } => {
                write!(f, "JSON Schema validation failed:\n{}", messages.join("\n"))?;
                // `debug` defaults to false so we don't log data by default
                if *DEBUG.get().unwrap_or(&false) {
                    write!(
                        f,
                        "\n\nData:\n{}",
                        serde_json::to_string(data).map_err(|_| std::fmt::Error)?
                    )?;
                }
                write!(
                    f,
                    "\n\nSchema:\n{}",
                    serde_json::to_string(schema).map_err(|_| std::fmt::Error)?
                )
            }
            ErrorDetails::MiniJinjaEnvironment { message } => {
                write!(f, "Error initializing MiniJinja environment: {message}")
            }
            ErrorDetails::MiniJinjaTemplate {
                template_name,
                message,
            } => {
                write!(f, "Error rendering template {template_name}: {message}")
            }
            ErrorDetails::MiniJinjaTemplateMissing { template_name } => {
                write!(f, "Template not found: {template_name}")
            }
            ErrorDetails::MiniJinjaTemplateRender {
                template_name,
                message,
            } => {
                write!(f, "Error rendering template {template_name}: {message}")
            }
            ErrorDetails::MissingBatchInferenceResponse { inference_id } => match inference_id {
                Some(inference_id) => write!(
                    f,
                    "Missing batch inference response for inference id: {inference_id}"
                ),
                None => write!(f, "Missing batch inference response"),
            },
            ErrorDetails::MissingFunctionInVariants { function_name } => {
                write!(f, "Missing function in variants: {function_name}")
            }
            ErrorDetails::MissingFileExtension { file_name } => {
                write!(
                    f,
                    "Could not determine file extension for file: {file_name}"
                )
            }
            ErrorDetails::ModelProvidersExhausted { provider_errors } => {
                write!(
                    f,
                    "All model providers failed to infer with errors: {}",
                    provider_errors
                        .iter()
                        .map(|(provider_name, error)| format!("{provider_name}: {error}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            ErrorDetails::ModelValidation { message } => {
                write!(f, "Failed to validate model: {message}")
            }
            ErrorDetails::Observability { message } => {
                write!(f, "{message}")
            }
            ErrorDetails::OptimizationResponse {
                message,
                provider_type,
            } => {
                write!(
                    f,
                    "Error from {provider_type} optimization response: {message}"
                )
            }
            ErrorDetails::OutputParsing {
                raw_output,
                message,
            } => {
                write!(
                    f,
                    "Error parsing output as JSON with message: {message}: {raw_output}"
                )
            }
            ErrorDetails::OutputValidation { source } => {
                write!(f, "Output validation failed with messages: {source}")
            }
            ErrorDetails::ProviderNotFound { provider_name } => {
                write!(f, "Provider not found: {provider_name}")
            }
            ErrorDetails::StreamError { source } => {
                write!(f, "Error in streaming response: {source}")
            }
            ErrorDetails::Serialization { message } => write!(f, "{message}"),
            ErrorDetails::TypeConversion { message } => write!(f, "{message}"),
            ErrorDetails::ToolNotFound { name } => write!(f, "Tool not found: {name}"),
            ErrorDetails::ToolNotLoaded { name } => write!(f, "Tool not loaded: {name}"),
            ErrorDetails::UnknownCandidate { name } => {
                write!(f, "Unknown candidate variant: {name}")
            }
            ErrorDetails::UnknownEvaluation { name } => write!(f, "Unknown evaluation: {name}"),
            ErrorDetails::UnknownFunction { name } => write!(f, "Unknown function: {name}"),
            ErrorDetails::UnknownModel { name } => write!(f, "Unknown model: {name}"),
            ErrorDetails::UnknownTool { name } => write!(f, "Unknown tool: {name}"),
            ErrorDetails::UnknownVariant { name } => write!(f, "Unknown variant: {name}"),
            ErrorDetails::UnknownMetric { name } => write!(f, "Unknown metric: {name}"),
            ErrorDetails::UnsupportedModelProviderForBatchInference { provider_type } => {
                write!(
                    f,
                    "Unsupported model provider for batch inference: {provider_type}"
                )
            }
            ErrorDetails::UnsupportedFileExtension { extension } => {
                write!(f, "Unsupported file extension: {extension}")
            }
            ErrorDetails::UnsupportedVariantForBatchInference { variant_name } => {
                match variant_name {
                    Some(variant_name) => {
                        write!(f, "Unsupported variant for batch inference: {variant_name}")
                    }
                    None => write!(f, "Unsupported variant for batch inference"),
                }
            }
            ErrorDetails::UnsupportedVariantForStreamingInference {
                variant_type,
                issue_link,
            } => {
                if let Some(link) = issue_link {
                    write!(
                        f,
                        "Unsupported variant for streaming inference of type {variant_type}. For more information, see: {link}"
                    )
                } else {
                    write!(
                        f,
                        "Unsupported variant for streaming inference of type {variant_type}"
                    )
                }
            }
            ErrorDetails::UnsupportedVariantForFunctionType {
                function_name,
                variant_name,
                function_type,
                variant_type,
            } => {
                write!(f, "Unsupported variant `{variant_name}` of type `{variant_type}` for function `{function_name}` of type `{function_type}`")
            }
            ErrorDetails::UuidInFuture { raw_uuid } => {
                write!(f, "UUID is in the future: {raw_uuid}")
            }
            ErrorDetails::RouteNotFound { path, method } => {
                write!(f, "Route not found: {method} {path}")
            }
        }
    }
}

impl IntoResponse for Error {
    /// Log the error and convert it into an Axum response
    fn into_response(self) -> Response {
        let mut body = json!({
            "error": self.to_string(),
        });
        if *UNSTABLE_ERROR_JSON.get().unwrap_or(&false) {
            body["error_json"] =
                serde_json::to_value(self.get_details()).unwrap_or_else(|e| json!(e.to_string()));
        }
        (self.status_code(), Json(body)).into_response()
    }
}
