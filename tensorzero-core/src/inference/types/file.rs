//! File Handling for TensorZero Inference Inputs
//!
//! This module defines file types for different stages of the file lifecycle:
//! from client requests through async processing to database storage and retrieval.
//!
//! # File Type Categories
//!
//! ## 1. `File` - Input from Client Requests
//! The primary enum representing files as they arrive from API requests.
//! Four variants handle different input formats:
//! - **`Url`**: A file URL that needs to be fetched
//! - **`Base64`**: A base64-encoded file with data already present
//! - **`ObjectStoragePointer`**: A pointer to an object storage location (metadata only, no data)
//! - **`ObjectStorage`**: A complete object storage file (metadata + data)
//!
//! ## 2. `LazyFile` - Async Processing
//! An intermediate type used during inference processing. Enables lazy evaluation
//! of file operations using shared futures for concurrent access.
//! Four variants mirror the `File` variants:
//! - **`Url`**: Contains a `FileFuture` that will fetch and store the file
//! - **`Base64`**: Contains a `PendingObjectStoreFile` (base64 data + pending object storage path)
//! - **`ObjectStoragePointer`**: Contains a `FileFuture` that will fetch from object storage
//! - **`ObjectStorage`**: Contains resolved `ObjectStorageFile` (no async work needed)
//!
//! The shared futures allow multiple model providers to await the same file resolution
//! without redundant fetches or computations.
//!
//! ## 3. `StoredFile` - Database Persistence
//! A newtype wrapper around `ObjectStoragePointer` used for ClickHouse storage.
//! Only stores metadata (source URL, mime type, storage path) without the actual file data.
//! The storage path is content-addressed, allowing files to be re-fetched when needed.
//!
//! # Flow Between Types
//!
//! ```text
//! Client Request → File
//!                   ↓
//!              LazyFile (with futures for async operations)
//!                   ↓ .resolve()
//!           ObjectStorageFile (metadata + data)
//!                   ↓ .into_stored_file()
//!              StoredFile (metadata only, persisted to ClickHouse)
//!                   ↓ .reresolve() (fetch data from storage)
//!           ObjectStorageFile (metadata + data)
//! ```

use std::borrow::Cow;

use futures::FutureExt;
use mime::MediaType;
use scoped_tls::scoped_thread_local;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use super::{ContentBlock, RequestMessage};
use crate::{
    error::{Error, ErrorDetails, IMPOSSIBLE_ERROR_MESSAGE},
    http::TensorzeroHttpClient,
    inference::types::{resolved_input::LazyFile, storage::StoragePath, stored_input::StoredFile},
};
use aws_smithy_types::base64;
#[cfg(feature = "pyo3")]
use pyo3::prelude::*;

scoped_thread_local!(static SERIALIZE_FILE_DATA: ());

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileEncoding {
    Base64,
    Url,
}

pub fn require_image(mime_type: &MediaType, provider_type: &str) -> Result<(), Error> {
    if mime_type.type_() != mime::IMAGE {
        return Err(Error::new(ErrorDetails::UnsupportedContentBlockType {
            content_block_type: format!("file: {mime_type}"),
            provider_type: provider_type.to_string(),
        }));
    }
    Ok(())
}

/// A file already encoded as base64
#[derive(Clone, Debug, PartialEq, Serialize, ts_rs::TS)]
#[cfg_attr(feature = "pyo3", pyclass)]
#[ts(export)]
pub struct Base64File {
    // The original url we used to download the file
    #[serde(alias = "url")] // DEPRECATED
    #[ts(optional)]
    pub source_url: Option<Url>,
    #[ts(type = "string")]
    pub mime_type: MediaType,
    // TODO: should we add a wrapper type to enforce base64?
    pub data: String,
}

/// Implement a custom deserializer for Base64File to show a deprecation warning for the `url` field
impl<'de> Deserialize<'de> for Base64File {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Base64FileHelper {
            #[serde(alias = "url")]
            source_url: Option<Url>,
            mime_type: MediaType,
            data: String,
        }

        let value = serde_json::Value::deserialize(deserializer)?;

        // Check if the deprecated "url" field is present
        if value.get("url").is_some() && value.get("source_url").is_none() {
            tracing::warn!(
                "Deprecation Warning: `url` is deprecated for `Base64File`. \
                Please use `source_url` instead."
            );
        }

        let helper: Base64FileHelper =
            serde_json::from_value(value).map_err(serde::de::Error::custom)?;

        Ok(Base64File {
            source_url: helper.source_url,
            mime_type: helper.mime_type,
            data: helper.data,
        })
    }
}

/// Like `Base64File`, but without the data field.
#[derive(ts_rs::TS, Clone, Debug, Serialize, PartialEq)]
#[ts(export)]
#[cfg_attr(feature = "pyo3", pyclass)]
pub struct Base64FileMetadata {
    // The original url we used to download the file
    #[serde(alias = "url")] // DEPRECATED
    #[ts(optional)]
    pub source_url: Option<Url>,
    #[ts(type = "string")]
    pub mime_type: MediaType,
}

/// Implement a custom deserializer for Base64FileMetadata to show a deprecation warning for the `url` field
impl<'de> Deserialize<'de> for Base64FileMetadata {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Base64FileMetadataHelper {
            #[serde(alias = "url")]
            source_url: Option<Url>,
            mime_type: MediaType,
        }

        let value = serde_json::Value::deserialize(deserializer)?;

        // Check if the deprecated "url" field is present
        if value.get("url").is_some() && value.get("source_url").is_none() {
            tracing::warn!(
                "Deprecation Warning: `url` is deprecated for `Base64FileMetadata`. \
                Please use `source_url` instead."
            );
        }

        let helper: Base64FileMetadataHelper =
            serde_json::from_value(value).map_err(serde::de::Error::custom)?;

        Ok(Base64FileMetadata {
            source_url: helper.source_url,
            mime_type: helper.mime_type,
        })
    }
}

impl std::fmt::Display for Base64File {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let json = serde_json::to_string_pretty(self).map_err(|_| std::fmt::Error)?;
        write!(f, "{json}")
    }
}

impl Base64File {
    pub fn data(&self) -> Result<&String, Error> {
        Ok(&self.data)
    }
}
#[cfg(feature = "pyo3")]
#[pymethods]
impl Base64File {
    #[getter(url)]
    pub fn url_string(&self) -> Option<String> {
        self.source_url.as_ref().map(Url::to_string)
    }

    #[getter(mime_type)]
    pub fn mime_type_string(&self) -> String {
        self.mime_type.to_string()
    }

    pub fn __repr__(&self) -> String {
        self.to_string()
    }
}

pub fn serialize_with_file_data<T: Serialize>(value: &T) -> Result<Value, Error> {
    SERIALIZE_FILE_DATA.set(&(), || {
        serde_json::to_value(value).map_err(|e| {
            Error::new(ErrorDetails::Serialization {
                message: format!("Error serializing value: {e}"),
            })
        })
    })
}

/// A file that can be located at a URL
#[derive(Clone, Debug, PartialEq, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct UrlFile {
    pub url: Url,
    #[ts(type = "string | null")]
    pub mime_type: Option<MediaType>,
}

/// A file stored in an object storage backend, without data.
/// This struct can be stored in the database. It's used by `StoredFile` (`StoredInput`).
/// Note: `File` supports both `ObjectStorageFilePointer` and `ObjectStorageFile`.
#[derive(Clone, Debug, PartialEq, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct ObjectStoragePointer {
    #[serde(alias = "url")] // DEPRECATED (SEE IMPORTANT NOTE BELOW)
    #[ts(optional)]
    pub source_url: Option<Url>,
    #[ts(type = "string")]
    pub mime_type: MediaType,
    pub storage_path: StoragePath,
}

/// A file stored in an object storage backend, with data.
/// This struct can NOT be stored in the database.
/// Note: `File` supports both `ObjectStorageFilePointer` and `ObjectStorageFile`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct ObjectStorageFile {
    #[serde(flatten)]
    pub file: ObjectStoragePointer,
    // TODO (GabrielBianconi): in the future this should be an Option<String> so we can handle failures more gracefully (or alternatively, another variant for `File`)
    // TODO: should we add a wrapper type to enforce base64?
    pub data: String,
}

/// A newtype wrapper around `ObjectStorageFile` that represents file data
/// from a base64 input that needs to be written to object storage.
/// The `storage_path` inside is content-addressed (computed from data) and represents
/// where the file WILL be written, not where it currently exists.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PendingObjectStoreFile(pub ObjectStorageFile);

impl std::ops::Deref for PendingObjectStoreFile {
    type Target = ObjectStorageFile;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Implement a custom deserializer for ObjectStorageFile to show a deprecation warning for the `url` field
///
/// IMPORTANT: This deserializer can't be fully removed. Eventually, we'll want to move it from `ObjectStoragePointer`
/// to `StoredFile`, but ClickHouse will still have legacy data with `url`.
impl<'de> Deserialize<'de> for ObjectStoragePointer {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct ObjectStorageFileHelper {
            #[serde(alias = "url")]
            source_url: Option<Url>,
            mime_type: MediaType,
            storage_path: StoragePath,
        }

        let value = serde_json::Value::deserialize(deserializer)?;

        // Check if the deprecated "url" field is present
        if value.get("url").is_some() && value.get("source_url").is_none() {
            tracing::warn!(
                "Deprecation Warning: `url` is deprecated for `ObjectStorageFile`. \
                Please use `source_url` instead."
            );
        }

        let helper: ObjectStorageFileHelper =
            serde_json::from_value(value).map_err(serde::de::Error::custom)?;

        Ok(ObjectStoragePointer {
            source_url: helper.source_url,
            mime_type: helper.mime_type,
            storage_path: helper.storage_path,
        })
    }
}

/// A file for an inference or a datapoint.
#[derive(Clone, Debug, PartialEq, Serialize, ts_rs::TS)]
#[serde(tag = "file_type", rename_all = "snake_case")]
#[ts(export)]
// NOTE(shuyangli, 2025-10-21): we're manually implementing Serialize and Deserialize for a while until we're confident
// that clients are sending us the correct tagged versions. Serialization always produces tagged format, but
// deserialization accepts both tagged and untagged formats for backwards compatibility.
// TODO(#4107): Remove this once we're confident that clients are sending us the tagged version.
pub enum File {
    Url(UrlFile),                               // a file URL
    Base64(Base64File),                         // a base64-encoded file
    ObjectStoragePointer(ObjectStoragePointer), // a pointer to an object storage file (metadata only)
    ObjectStorage(ObjectStorageFile),           // a file from object storage (metadata + data)
}

// Allow deserializing File as either tagged or untagged format.
// This is a backwards compatibility feature for a while until we're confident that clients are sending us
// the correct tagged versions. Switching from `#[serde(untagged)]` to `#[serde(tag = "file_type")]` is a breaking change.
impl<'de> Deserialize<'de> for File {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Helper structs that match the tagged and untagged formats for deserialization purposes.
        #[derive(Deserialize)]
        #[serde(tag = "file_type", rename_all = "snake_case")]
        enum TaggedFile {
            Url {
                url: Url,
                mime_type: Option<MediaType>,
            },
            Base64 {
                mime_type: MediaType,
                data: String,
            },
            ObjectStoragePointer {
                source_url: Option<Url>,
                mime_type: MediaType,
                storage_path: StoragePath,
            },
            ObjectStorage {
                #[serde(flatten)]
                file: ObjectStoragePointer,
                data: String,
            },
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum LegacyUntaggedFile {
            Url {
                url: Url,
                #[serde(default)]
                mime_type: Option<MediaType>,
            },
            Base64 {
                mime_type: MediaType,
                data: String,
            },
            ObjectStoragePointer {
                source_url: Option<Url>,
                mime_type: MediaType,
                storage_path: StoragePath,
            },
            ObjectStorage {
                #[serde(flatten)]
                file: ObjectStoragePointer,
                data: String,
            },
        }

        // Try both formats during deserialization - tagged first, then untagged
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum FileTaggedOrUntagged {
            Tagged(TaggedFile),
            Untagged(LegacyUntaggedFile),
        }

        match FileTaggedOrUntagged::deserialize(deserializer)? {
            FileTaggedOrUntagged::Tagged(TaggedFile::Url { url, mime_type }) => {
                Ok(File::Url(UrlFile { url, mime_type }))
            }
            FileTaggedOrUntagged::Tagged(TaggedFile::Base64 { mime_type, data }) => {
                Ok(File::Base64(Base64File {
                    mime_type,
                    data,
                    source_url: None,
                }))
            }
            FileTaggedOrUntagged::Tagged(TaggedFile::ObjectStoragePointer {
                source_url,
                mime_type,
                storage_path,
            }) => Ok(File::ObjectStoragePointer(ObjectStoragePointer {
                source_url,
                mime_type,
                storage_path,
            })),
            FileTaggedOrUntagged::Tagged(TaggedFile::ObjectStorage { file, data }) => {
                Ok(File::ObjectStorage(ObjectStorageFile { file, data }))
            }
            FileTaggedOrUntagged::Untagged(LegacyUntaggedFile::Url { url, mime_type }) => {
                Ok(File::Url(UrlFile { url, mime_type }))
            }
            FileTaggedOrUntagged::Untagged(LegacyUntaggedFile::Base64 { mime_type, data }) => {
                Ok(File::Base64(Base64File {
                    mime_type,
                    data,
                    source_url: None,
                }))
            }
            FileTaggedOrUntagged::Untagged(LegacyUntaggedFile::ObjectStoragePointer {
                source_url,
                mime_type,
                storage_path,
            }) => Ok(File::ObjectStoragePointer(ObjectStoragePointer {
                source_url,
                mime_type,
                storage_path,
            })),
            FileTaggedOrUntagged::Untagged(LegacyUntaggedFile::ObjectStorage { file, data }) => {
                Ok(File::ObjectStorage(ObjectStorageFile { file, data }))
            }
        }
    }
}

impl File {
    pub async fn take_or_fetch(self, client: &TensorzeroHttpClient) -> Result<Base64File, Error> {
        match self {
            File::Url(url_file) => {
                let UrlFile { url, mime_type } = url_file;
                let response = client.get(url.clone()).send().await.map_err(|e| {
                    Error::new(ErrorDetails::BadFileFetch {
                        url: url.clone(),
                        message: format!("Error fetching image: {e:?}"),
                    })
                })?;

                // Check status code
                let status = response.status();
                if !status.is_success() {
                    let error_body = response.text().await.unwrap_or_else(|_| String::from("(unable to read response body)"));
                    return Err(Error::new(ErrorDetails::BadFileFetch {
                        url: url.clone(),
                        message: format!("HTTP error {status}: {error_body}"),
                    }));
                }

                // Extract headers before consuming response
                let content_type_header =
                    response.headers().get(http::header::CONTENT_TYPE).cloned();

                let bytes = response.bytes().await.map_err(|e| {
                    Error::new(ErrorDetails::BadFileFetch {
                        url: url.clone(),
                        message: format!("Error reading image bytes: {e}"),
                    })
                })?;

                let mime_type = if let Some(mime_type) = mime_type {
                    // Priority 1: Explicitly provided mime_type
                    mime_type
                } else {
                    // Priority 2: Infer from file content
                    let inferred = infer::get(&bytes);

                    if let Some(inferred_type) = inferred {
                        let inferred_mime = inferred_type
                            .mime_type()
                            .parse::<MediaType>()
                            .map_err(|e| {
                                Error::new(ErrorDetails::BadFileFetch {
                                    url: url.clone(),
                                    message: format!("Inferred mime type is not valid: {e}"),
                                })
                            })?;

                        // Check if Content-Type header differs and log warning
                        if let Some(content_type) = &content_type_header {
                            if let Ok(content_type_str) = content_type.to_str() {
                                if let Ok(header_mime) = content_type_str.parse::<MediaType>() {
                                    if header_mime != inferred_mime {
                                        tracing::warn!(
                                            "Inferred MIME type `{}` differs from Content-Type header `{}` for URL {}. The gateway will send `{}` to the model provider",
                                            inferred_mime,
                                            header_mime,
                                            url,
                                            inferred_mime,
                                        );
                                    }
                                } else {
                                    tracing::warn!("Content-Type header is not a valid mime type: `{content_type_str}`");
                                }
                            }
                        }

                        inferred_mime
                    } else if let Some(content_type) = &content_type_header {
                        // Priority 3: Content-Type header
                        content_type
                            .to_str()
                            .map_err(|e| {
                                Error::new(ErrorDetails::BadFileFetch {
                                    url: url.clone(),
                                    message: format!(
                                        "Content-Type header is not a valid string: {e}"
                                    ),
                                })
                            })?
                            .parse::<MediaType>()
                            .map_err(|e| {
                                Error::new(ErrorDetails::BadFileFetch {
                                    url: url.clone(),
                                    message: format!(
                                        "Content-Type header is not a valid mime type: {e}"
                                    ),
                                })
                            })?
                    } else {
                        return Err(Error::new(ErrorDetails::BadFileFetch {
                            url: url.clone(),
                            message:
                                "`mime_type` not provided, and unable to infer from file content or Content-Type header"
                                    .to_string(),
                        }));
                    }
                };

                let data = base64::encode(bytes);
                Ok(Base64File {
                    source_url: Some(url.clone()),
                    mime_type,
                    data,
                })
            }
            File::Base64(base64_file) => {
                Ok(Base64File { source_url: None, ..base64_file })
            }
            File::ObjectStoragePointer(_) => Err(Error::new(ErrorDetails::InternalError {
                // This path gets called from `InputMessageContent::into_lazy_resolved_input_message`, and only
                // the base File::Url type calls this method.
                message: format!("File::ObjectStorage::take_or_fetch should be unreachable! {IMPOSSIBLE_ERROR_MESSAGE}"),
            })),
            File::ObjectStorage(_) => Err(Error::new(ErrorDetails::InternalError {
                // This path gets called from `InputMessageContent::into_lazy_resolved_input_message`, and only
                // the base File::Url type calls this method.
                message: format!("File::ObjectStorage::take_or_fetch should be unreachable! {IMPOSSIBLE_ERROR_MESSAGE}"),
            })),
        }
    }

    pub fn into_stored_file(self) -> Result<StoredFile, Error> {
        match self {
            File::ObjectStorage(ObjectStorageFile { file, data: _ }) => {
                Ok(StoredFile(file))
            }
            File::Url(_) | File::Base64(_) | File::ObjectStoragePointer(_) => {
                Err(Error::new(ErrorDetails::InternalError {
                    message: format!(
                        "File::into_stored_file should only be called on ObjectStorage! {IMPOSSIBLE_ERROR_MESSAGE}"
                    ),
                }))
            }
        }
    }
}

/// Strips out image data from the raw request, replacing it with a placeholder.
/// This is a best-effort attempt to avoid filling up ClickHouse with image data.
pub fn sanitize_raw_request(input_messages: &[RequestMessage], mut raw_request: String) -> String {
    let mut i = 0;
    for message in input_messages {
        for content in &message.content {
            if let ContentBlock::File(file) = content {
                let file_with_path = match &**file {
                    LazyFile::Url {
                        future,
                        file_url: _,
                    } => {
                        // If we actually sent the file bytes to some model provider, then the
                        // Shared future must be ready, so we'll get a file from `now_or_never`.
                        // Otherwise, the file cannot have been sent to a model provider (since the
                        // future was never `.await`ed before we constructed `raw_request`), so
                        // there's nothing to strip from the message.
                        // We ignore errors here, since an error during file resolution means that
                        // we cannot have included the file bytes in `raw_request`.
                        if let Some(Ok(resolved)) = future.clone().now_or_never() {
                            Some(Cow::Owned(File::ObjectStorage(resolved)))
                        } else {
                            None
                        }
                    }
                    LazyFile::Base64(pending) => {
                        Some(Cow::Owned(File::ObjectStorage(pending.0.clone())))
                    }
                    LazyFile::ObjectStorage(resolved) => {
                        Some(Cow::Owned(File::ObjectStorage(resolved.clone())))
                    }
                    LazyFile::ObjectStoragePointer { future, .. } => {
                        // If we actually sent the file bytes to some model provider, then the
                        // Shared future must be ready, so we'll get a file from `now_or_never`.
                        // Otherwise, the file cannot have been sent to a model provider (since the
                        // future was never `.await`ed before we constructed `raw_request`), so
                        // there's nothing to strip from the message.
                        // We ignore errors here, since an error during file resolution means that
                        // we cannot have included the file bytes in `raw_request`.
                        if let Some(Ok(resolved)) = future.clone().now_or_never() {
                            Some(Cow::Owned(File::ObjectStorage(resolved)))
                        } else {
                            None
                        }
                    }
                };
                if let Some(file) = file_with_path {
                    let data = match &*file {
                        File::ObjectStorage(resolved) => &resolved.data,
                        File::Base64(base64) => &base64.data,
                        File::Url(_) | File::ObjectStoragePointer(_) => {
                            // These variants should not occur in resolved files
                            continue;
                        }
                    };
                    raw_request = raw_request.replace(data, &format!("<TENSORZERO_FILE_{i}>"));
                    i += 1;
                }
            }
        }
    }
    raw_request
}

/// Tries to convert a mime type to a file extension, picking an arbitrary extension if there are multiple
/// extensions for the mime type.
/// This is used when writing a file input to object storage, and when determining the file name
/// to provide to OpenAI (which doesn't accept mime types for file input)
pub fn mime_type_to_ext(mime_type: &MediaType) -> Result<Option<&'static str>, Error> {
    Ok(match mime_type {
        _ if mime_type == &mime::IMAGE_JPEG => Some("jpg"),
        _ if mime_type == &mime::IMAGE_PNG => Some("png"),
        _ if mime_type == &mime::IMAGE_GIF => Some("gif"),
        _ if mime_type == &mime::APPLICATION_PDF => Some("pdf"),
        _ if mime_type == "image/webp" => Some("webp"),
        _ if mime_type == "text/plain" => Some("txt"),
        _ => {
            let guess = mime_guess::get_mime_extensions_str(mime_type.as_ref())
                .and_then(|types| types.last());
            if guess.is_some() {
                tracing::warn!("Guessed file extension {guess:?} for mime-type {mime_type} - this may not be correct");
            }
            guess.copied()
        }
    })
}

/// Tries to convert a filename to a mime type, based on the file extension.
/// This picks an arbitrary mime type if there are multiple mime types for the extension.
///
/// This is used by the openai-compatible endpoint to determine the mime type for
/// a file input (the OpenAI chat-completions endpoint doesn't provide a mime type for
/// file inputs)
pub fn filename_to_mime_type(filename: &str) -> Result<MediaType, Error> {
    let ext = filename.split('.').next_back().ok_or_else(|| {
        Error::new(ErrorDetails::InvalidOpenAICompatibleRequest {
            message: "File name must contain a file extension".to_string(),
        })
    })?;

    Ok(match ext {
        "jpeg" | "jpg" => mime::IMAGE_JPEG,
        "png" => mime::IMAGE_PNG,
        "gif" => mime::IMAGE_GIF,
        "pdf" => mime::APPLICATION_PDF,
        "webp" => "image/webp".parse::<MediaType>().map_err(|_| {
            Error::new(ErrorDetails::InternalError {
                message: "Unknown mime-type `image/webp`. This should never happen. Please file a bug report at https://github.com/tensorzero/tensorzero/discussions/new?category=bug-reports.".to_string(),
            })
        })?,
        _ => {
            let mime_type = mime_guess::from_ext(ext).first().ok_or_else(|| {
                Error::new(ErrorDetails::InvalidOpenAICompatibleRequest {
                    message: format!("Unknown file extension `{ext}`"),
                })
            })?;
            tracing::warn!("Guessed mime-type `{mime_type}` for file with extension `{ext}` - this may not be correct");
            // Reparse to handle different `mime` crate versions
            mime_type.to_string().parse::<MediaType>().map_err(|_| {
                Error::new(ErrorDetails::InvalidOpenAICompatibleRequest {
                    message: format!("Unknown mime-type `{mime_type}`"),
                })
            })?
        }
    })
}

#[cfg(test)]
mod tests {
    use tracing_test::traced_test;

    use crate::inference::types::{
        file::{
            filename_to_mime_type, sanitize_raw_request, ObjectStorageFile, ObjectStoragePointer,
        },
        resolved_input::LazyFile,
        storage::{StorageKind, StoragePath},
        ContentBlock, RequestMessage, Role,
    };

    #[test]
    fn test_sanitize_input() {
        assert_eq!(
            sanitize_raw_request(&[], "my-fake-input".to_string()),
            "my-fake-input"
        );

        assert_eq!(
            sanitize_raw_request(
                &[
                    RequestMessage {
                        role: Role::User,
                        content: vec![
                            ContentBlock::File(Box::new(LazyFile::ObjectStorage(
                                ObjectStorageFile {
                                    file: ObjectStoragePointer {
                                        source_url: None,
                                        mime_type: mime::IMAGE_JPEG,
                                        storage_path: StoragePath {
                                            kind: StorageKind::Disabled,
                                            path: object_store::path::Path::parse(
                                                "my-image-1-path"
                                            )
                                            .unwrap(),
                                        },
                                    },
                                    data: "my-image-1-data".to_string(),
                                }
                            ))),
                            ContentBlock::File(Box::new(LazyFile::ObjectStorage(
                                ObjectStorageFile {
                                    file: ObjectStoragePointer {
                                        source_url: None,
                                        mime_type: mime::IMAGE_JPEG,
                                        storage_path: StoragePath {
                                            kind: StorageKind::Disabled,
                                            path: object_store::path::Path::parse(
                                                "my-image-2-path"
                                            )
                                            .unwrap(),
                                        },
                                    },
                                    data: "my-image-2-data".to_string(),
                                }
                            ))),
                            ContentBlock::File(Box::new(LazyFile::ObjectStorage(
                                ObjectStorageFile {
                                    file: ObjectStoragePointer {
                                        source_url: None,
                                        mime_type: mime::IMAGE_JPEG,
                                        storage_path: StoragePath {
                                            kind: StorageKind::Disabled,
                                            path: object_store::path::Path::parse(
                                                "my-image-1-path"
                                            )
                                            .unwrap(),
                                        },
                                    },
                                    data: "my-image-1-data".to_string(),
                                }
                            ))),
                        ],
                    },
                    RequestMessage {
                        role: Role::User,
                        content: vec![
                            ContentBlock::File(Box::new(LazyFile::ObjectStorage(
                                ObjectStorageFile {
                                    file: ObjectStoragePointer {
                                        source_url: None,
                                        mime_type: mime::IMAGE_JPEG,
                                        storage_path: StoragePath {
                                            kind: StorageKind::Disabled,
                                            path: object_store::path::Path::parse(
                                                "my-image-3-path"
                                            )
                                            .unwrap(),
                                        },
                                    },
                                    data: "my-image-3-data".to_string(),
                                }
                            ))),
                            ContentBlock::File(Box::new(LazyFile::ObjectStorage(
                                ObjectStorageFile {
                                    file: ObjectStoragePointer {
                                        source_url: None,
                                        mime_type: mime::IMAGE_JPEG,
                                        storage_path: StoragePath {
                                            kind: StorageKind::Disabled,
                                            path: object_store::path::Path::parse(
                                                "my-image-1-path"
                                            )
                                            .unwrap(),
                                        },
                                    },
                                    data: "my-image-1-data".to_string(),
                                }
                            )))
                        ],
                    }
                ],
                "First my-image-1-data then my-image-2-data then my-image-3-data".to_string()
            ),
            // Each occurrence of the file data should be replaced with the first matching file content block
            "First <TENSORZERO_FILE_0> then <TENSORZERO_FILE_1> then <TENSORZERO_FILE_3>"
                .to_string()
        );
    }

    #[test]
    #[traced_test]
    fn test_filename_to_mime_type() {
        assert_eq!(filename_to_mime_type("test.png").unwrap(), mime::IMAGE_PNG);
        assert_eq!(filename_to_mime_type("test.jpg").unwrap(), mime::IMAGE_JPEG);
        assert_eq!(
            filename_to_mime_type("test.jpeg").unwrap(),
            mime::IMAGE_JPEG
        );
        assert_eq!(filename_to_mime_type("test.gif").unwrap(), mime::IMAGE_GIF);
        assert_eq!(filename_to_mime_type("test.webp").unwrap(), "image/webp");
        assert_eq!(
            filename_to_mime_type("test.pdf").unwrap(),
            mime::APPLICATION_PDF
        );
        assert!(!logs_contain("Guessed"));
    }

    #[test]
    #[traced_test]
    fn test_guessed_mime_type_warning() {
        assert_eq!(
            filename_to_mime_type("my_file.txt").unwrap(),
            mime::TEXT_PLAIN
        );
        assert!(logs_contain("Guessed"));
    }

    #[test]
    fn test_infer_mime_type_from_bytes() {
        // Test JPEG detection (magic bytes: FF D8 FF)
        let jpeg_bytes = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        let inferred = infer::get(&jpeg_bytes).expect("Should detect JPEG");
        assert_eq!(inferred.mime_type(), "image/jpeg");
        assert_eq!(inferred.extension(), "jpg");

        // Test PNG detection (magic bytes: 89 50 4E 47 0D 0A 1A 0A)
        let png_bytes = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        let inferred = infer::get(&png_bytes).expect("Should detect PNG");
        assert_eq!(inferred.mime_type(), "image/png");
        assert_eq!(inferred.extension(), "png");

        // Test GIF detection (magic bytes: 47 49 46 38 39 61)
        let gif_bytes = [0x47, 0x49, 0x46, 0x38, 0x39, 0x61];
        let inferred = infer::get(&gif_bytes).expect("Should detect GIF");
        assert_eq!(inferred.mime_type(), "image/gif");
        assert_eq!(inferred.extension(), "gif");

        // Test PDF detection (magic bytes: %PDF)
        let pdf_bytes = b"%PDF-1.4\n";
        let inferred = infer::get(pdf_bytes).expect("Should detect PDF");
        assert_eq!(inferred.mime_type(), "application/pdf");
        assert_eq!(inferred.extension(), "pdf");

        // Test WebP detection (magic bytes: RIFF....WEBP)
        let webp_bytes = b"RIFF\x00\x00\x00\x00WEBPVP8 ";
        let inferred = infer::get(webp_bytes).expect("Should detect WebP");
        assert_eq!(inferred.mime_type(), "image/webp");
        assert_eq!(inferred.extension(), "webp");

        // Test that unknown bytes return None
        let unknown_bytes = [0x00, 0x01, 0x02, 0x03];
        assert!(infer::get(&unknown_bytes).is_none());
    }

    mod file_serde_tests {
        use crate::inference::types::{
            file::{Base64File, ObjectStoragePointer, UrlFile},
            storage::{StorageKind, StoragePath},
            File,
        };

        #[test]
        fn test_file_url_serialize_always_tagged() {
            // Serialization should always produce tagged format
            let file = File::Url(UrlFile {
                url: "https://example.com/image.png".parse().unwrap(),
                mime_type: Some(mime::IMAGE_PNG),
            });

            let serialized = serde_json::to_value(&file).unwrap();
            assert_eq!(serialized["file_type"], "url");
            assert_eq!(serialized["url"], "https://example.com/image.png");
            assert_eq!(serialized["mime_type"], "image/png");
        }

        #[test]
        fn test_file_url_serialize_tagged_without_mime_type() {
            let file = File::Url(UrlFile {
                url: "https://example.com/image.png".parse().unwrap(),
                mime_type: None,
            });

            let serialized = serde_json::to_value(&file).unwrap();
            assert_eq!(serialized["file_type"], "url");
            assert_eq!(serialized["url"], "https://example.com/image.png");
            assert_eq!(serialized["mime_type"], serde_json::Value::Null);
        }

        #[test]
        fn test_file_base64_serialize_always_tagged() {
            let file = File::Base64(Base64File {
                source_url: None,
                mime_type: mime::IMAGE_PNG,
                data: "iVBORw0KGgo=".to_string(),
            });

            let serialized = serde_json::to_value(&file).unwrap();
            assert_eq!(serialized["file_type"], "base64");
            assert_eq!(serialized["mime_type"], "image/png");
            assert_eq!(serialized["data"], "iVBORw0KGgo=");
        }

        #[test]
        fn test_file_object_storage_serialize_always_tagged() {
            let file = File::ObjectStoragePointer(ObjectStoragePointer {
                source_url: Some("https://example.com/image.png".parse().unwrap()),
                mime_type: mime::IMAGE_PNG,
                storage_path: StoragePath {
                    kind: StorageKind::Disabled,
                    path: object_store::path::Path::parse("test/path.png").unwrap(),
                },
            });

            let serialized = serde_json::to_value(&file).unwrap();
            let file_type = serialized["file_type"].as_str().unwrap();
            assert_eq!(file_type, "object_storage_pointer");
            assert!(serialized.get("source_url").is_some());
            assert!(serialized.get("mime_type").is_some());
            assert!(serialized.get("storage_path").is_some());
        }

        #[test]
        fn test_file_url_deserialize_tagged() {
            // Deserialization should accept tagged format
            let json = serde_json::json!({
                "file_type": "url",
                "url": "https://example.com/image.png",
                "mime_type": "image/png"
            });

            let file: File = serde_json::from_value(json).unwrap();
            assert!(matches!(file, File::Url(_)));
            if let File::Url(UrlFile { url, mime_type }) = file {
                assert_eq!(url.as_str(), "https://example.com/image.png");
                assert_eq!(mime_type, Some(mime::IMAGE_PNG));
            }
        }

        #[test]
        fn test_file_url_deserialize_untagged() {
            // Deserialization should still accept untagged format for backwards compatibility
            let json = serde_json::json!({
                "url": "https://example.com/image.png",
                "mime_type": "image/png"
            });

            let file: File = serde_json::from_value(json).unwrap();
            assert!(matches!(file, File::Url(_)));
            if let File::Url(UrlFile { url, mime_type }) = file {
                assert_eq!(url.as_str(), "https://example.com/image.png");
                assert_eq!(mime_type, Some(mime::IMAGE_PNG));
            }
        }

        #[test]
        fn test_file_base64_deserialize_tagged() {
            let json = serde_json::json!({
                "file_type": "base64",
                "mime_type": "image/png",
                "data": "iVBORw0KGgo="
            });

            let file: File = serde_json::from_value(json).unwrap();
            assert!(matches!(file, File::Base64(_)));
            if let File::Base64(base64_file) = file {
                let Base64File {
                    source_url: _,
                    mime_type,
                    data,
                } = base64_file;
                assert_eq!(mime_type, mime::IMAGE_PNG);
                assert_eq!(data, "iVBORw0KGgo=");
            }
        }

        #[test]
        fn test_file_base64_deserialize_untagged() {
            let json = serde_json::json!({
                "mime_type": "image/png",
                "data": "iVBORw0KGgo="
            });

            let file: File = serde_json::from_value(json).unwrap();
            assert!(matches!(file, File::Base64(_)));
            if let File::Base64(base64_file) = file {
                let Base64File {
                    source_url: _,
                    mime_type,
                    data,
                } = base64_file;
                assert_eq!(mime_type, mime::IMAGE_PNG);
                assert_eq!(data, "iVBORw0KGgo=");
            }
        }

        #[test]
        fn test_file_object_storage_deserialize_tagged() {
            let json = serde_json::json!({
                "file_type": "object_storage_pointer",
                "source_url": "https://example.com/image.png",
                "mime_type": "image/png",
                "storage_path": {
                    "kind": {
                        "type": "disabled"
                    },
                    "path": "test/path.png"
                }
            });

            let file: File = serde_json::from_value(json).unwrap();
            assert!(matches!(file, File::ObjectStoragePointer(_)));
        }

        #[test]
        fn test_file_object_storage_deserialize_untagged() {
            let json = serde_json::json!({
                "source_url": "https://example.com/image.png",
                "mime_type": "image/png",
                "storage_path": {
                    "kind": {
                        "type": "disabled"
                    },
                    "path": "test/path.png"
                }
            });

            let file: File = serde_json::from_value(json).unwrap();
            assert!(matches!(file, File::ObjectStoragePointer(_)));
        }

        #[test]
        fn test_roundtrip_serialization() {
            // Test that serialize -> deserialize maintains data integrity
            let original = File::Base64(Base64File {
                source_url: None,
                mime_type: mime::IMAGE_JPEG,
                data: "base64data".to_string(),
            });

            let serialized = serde_json::to_string(&original).unwrap();
            let deserialized: File = serde_json::from_str(&serialized).unwrap();

            assert_eq!(original, deserialized);
        }
    }
}
