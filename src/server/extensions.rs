use bsp_server::{types::BuildTargetIdentifier, Message, Notification, RequestId, Response};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tap::Pipe;
use url::Url;

/// Request to register or unregister changes in file options or dependencies.
#[derive(Debug, Deserialize, Serialize)]
pub struct OptionsChangedRequest {
    /// The URI of the document to get options for.
    pub uri: Url,
    /// Whether to register or unregister for the file.
    pub action: RegisterAction,
}

impl OptionsChangedRequest {
    pub const METHOD: &'static str = "textDocument/registerForChanges";
    pub fn new(uri: Url, action: RegisterAction) -> Self {
        Self { uri, action }
    }
}

impl Into<OptionsChangedRequest> for Value {
    fn into(self) -> OptionsChangedRequest {
        serde_json::from_value(self).unwrap()
    }
}

/// RegisterForChangesRequest Action variants
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RegisterAction {
    Register,
    Unregister,
}

/// Request to Query for the list of compiler options necessary to compile a given file.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionsRequest {
    /// The URI of the document to get options for
    pub uri: Url,
}

impl OptionsRequest {
    pub const METHOD: &'static str = "textDocument/sourceKitOptions";
}

impl Into<OptionsRequest> for Value {
    fn into(self) -> OptionsRequest {
        serde_json::from_value(self).unwrap()
    }
}

/// A Response containing compiler options necessary to compile a given file.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionsResponse {
    /// The compiler options required for the requested file.
    pub options: Vec<String>,
    /// The working directory for the compile command.
    pub working_directory: Option<Url>,
}

impl OptionsResponse {
    pub fn new(options: Vec<String>, working_directory: Option<Url>) -> Self {
        Self {
            options,
            working_directory,
        }
    }

    pub fn as_response(self, id: RequestId) -> Response {
        Response::ok(id, self)
    }
}

/// A Notification sent to SourceKit-lsp when changes happen to a registered files build settings.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionsChangedNotification {
    /// The URI of the document that has changed settings.
    pub uri: Url,
    /// The updated options for the registered file.
    pub updated_options: OptionsResponse,
}

impl OptionsChangedNotification {
    pub fn new(uri: Url, options: Vec<String>, working_directory: Option<Url>) -> Self {
        OptionsResponse {
            options,
            working_directory,
        }
        .pipe(|updated_options| Self {
            uri,
            updated_options,
        })
    }
}

impl From<OptionsChangedNotification> for Message {
    fn from(not: OptionsChangedNotification) -> Message {
        Message::Notification(Notification::Custom(
            "build/sourceKitOptionsChanged",
            // WARN: Force Unwrap
            serde_json::to_value(not).unwrap(),
        ))
    }
}

/// Request to query for the list of compilation output paths for a targets sources.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildTargetOutputPathsRequest {
    pub targets: Vec<BuildTargetIdentifier>,
}

impl Into<BuildTargetOutputPathsRequest> for Value {
    fn into(self) -> BuildTargetOutputPathsRequest {
        serde_json::from_value(self).unwrap()
    }
}

impl BuildTargetOutputPathsRequest {
    pub const METHOD: &'static str = "buildTarget/outputPaths";
}

/// Request containing the list of [`BuildTargetOutputPaths`]
#[derive(Debug, Deserialize, Serialize)]
pub struct BuildTargetOutputPathsResponse {
    pub items: Vec<BuildTargetOutputPaths>,
}

impl BuildTargetOutputPathsResponse {
    pub fn new(items: Vec<BuildTargetOutputPaths>) -> Self {
        Self { items }
    }

    pub fn as_response(self, id: RequestId) -> Response {
        Response::ok(id, self)
    }
}
/// Compilation output paths for a [`BuildTargetIdentifier`]
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildTargetOutputPaths {
    pub target: BuildTargetIdentifier,
    /// The output paths for sources that belong to this build target.
    pub output_paths: Vec<Url>,
}

impl BuildTargetOutputPaths {
    pub fn new(target: BuildTargetIdentifier, output_paths: Vec<Url>) -> Self {
        Self {
            target,
            output_paths,
        }
    }
}