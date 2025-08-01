use super::{deserialize_delete, serialize_delete};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(transparent)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
pub struct ExtraHeadersConfig {
    pub data: Vec<ExtraHeader>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
pub struct ExtraHeader {
    pub name: String,
    #[serde(flatten)]
    pub kind: ExtraHeaderKind,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
pub enum ExtraHeaderKind {
    Value(String),
    // We only allow `"delete": true` to be set - deserializing `"delete": false` will error
    #[serde(
        serialize_with = "serialize_delete",
        deserialize_with = "deserialize_delete"
    )]
    Delete,
}

/// The 'InferenceExtraHeaders' options provided directly in an inference request
/// These have not yet been filtered by variant name
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(transparent)]
pub struct UnfilteredInferenceExtraHeaders {
    pub headers: Vec<InferenceExtraHeader>,
}

impl UnfilteredInferenceExtraHeaders {
    pub fn is_empty(&self) -> bool {
        self.headers.is_empty()
    }

    /// Filter the 'InferenceExtraHeader' options by variant name
    /// If the variant name is `None`, then all variant-specific extra header options are removed
    pub fn filter(self, variant_name: &str) -> FilteredInferenceExtraHeaders {
        FilteredInferenceExtraHeaders {
            data: self
                .headers
                .into_iter()
                .filter(|config| config.should_apply_variant(variant_name))
                .collect(),
        }
    }
}

/// The result of filtering `InferenceExtraHeader` by variant name.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(transparent)]
pub struct FilteredInferenceExtraHeaders {
    pub data: Vec<InferenceExtraHeader>,
}

/// Holds the config-level and inference-level extra headers options
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct FullExtraHeadersConfig {
    pub variant_extra_headers: Option<ExtraHeadersConfig>,
    pub inference_extra_headers: FilteredInferenceExtraHeaders,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub enum InferenceExtraHeader {
    Provider {
        model_provider_name: String,
        name: String,
        #[serde(flatten)]
        kind: ExtraHeaderKind,
    },
    Variant {
        variant_name: String,
        name: String,
        #[serde(flatten)]
        kind: ExtraHeaderKind,
    },
}

impl InferenceExtraHeader {
    pub fn should_apply_variant(&self, variant_name: &str) -> bool {
        match self {
            InferenceExtraHeader::Provider { .. } => true,
            InferenceExtraHeader::Variant {
                variant_name: v, ..
            } => v == variant_name,
        }
    }
}
