use std::path::PathBuf;

use dprint_core::async_runtime::LocalBoxFuture;
use dprint_core::async_runtime::async_trait;
use dprint_core::configuration::ConfigKeyMap;
use dprint_core::configuration::GlobalConfiguration;
use dprint_core::plugins::AsyncPluginHandler;
use dprint_core::plugins::FormatError;
use dprint_core::plugins::FormatRequest;
use dprint_core::plugins::FormatResult;
use dprint_core::plugins::HostFormatRequest;
use dprint_core::plugins::PluginInfo;
use dprint_core::plugins::PluginResolveConfigurationResult;

use crate::configuration::ResolvedConfiguration;
use crate::configuration::resolve_configuration;
use crate::mock_worker::MockFormatRequest;
use crate::mock_worker::MockWorker;

pub struct OxfmtPluginHandler {
    worker: MockWorker,
}

impl OxfmtPluginHandler {
    pub fn new() -> Self {
        Self {
            worker: MockWorker::new(),
        }
    }
}

#[async_trait(?Send)]
impl AsyncPluginHandler for OxfmtPluginHandler {
    type Configuration = ResolvedConfiguration;

    fn plugin_info(&self) -> PluginInfo {
        PluginInfo {
            name: env!("CARGO_PKG_NAME").to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            config_key: "oxfmt".to_owned(),
            help_url: "https://github.com/dprint/dprint-plugin-oxfmt".to_owned(),
            config_schema_url:
                "https://github.com/dprint/dprint-plugin-oxfmt/blob/main/schema/plugin.schema.json"
                    .to_owned(),
            update_url: None,
        }
    }

    fn license_text(&self) -> String {
        include_str!("../../../LICENSE").to_owned()
    }

    async fn resolve_config(
        &self,
        config: ConfigKeyMap,
        global_config: GlobalConfiguration,
    ) -> PluginResolveConfigurationResult<Self::Configuration> {
        resolve_configuration(config, global_config)
    }

    async fn format(
        &self,
        request: FormatRequest<Self::Configuration>,
        _format_with_host: impl FnMut(HostFormatRequest) -> LocalBoxFuture<'static, FormatResult>
        + 'static,
    ) -> FormatResult {
        if request.range.is_some() || request.token.is_cancelled() {
            return Ok(None);
        }

        let source_text = String::from_utf8(request.file_bytes)?;
        let file_path = absolute_path(request.file_path)?;
        let mock_request = MockFormatRequest {
            file_name: &file_path,
            source_text: &source_text,
            options: &request.config.options,
        };
        let output = self.worker.format(&mock_request);

        if output == source_text {
            Ok(None)
        } else {
            Ok(Some(output.into_bytes()))
        }
    }
}

fn absolute_path(file_path: PathBuf) -> Result<PathBuf, FormatError> {
    if file_path.is_absolute() {
        Ok(file_path)
    } else {
        Ok(std::env::current_dir()?.join(file_path))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use dprint_core::plugins::FormatConfigId;
    use dprint_core::plugins::NullCancellationToken;

    use super::*;

    fn test_request(file_bytes: Vec<u8>) -> FormatRequest<ResolvedConfiguration> {
        FormatRequest {
            file_path: PathBuf::from("src/example.ts"),
            file_bytes,
            config_id: FormatConfigId::from_raw(1),
            config: Arc::new(ResolvedConfiguration {
                options: serde_json::json!({ "lineWidth": 100 }),
                project_root: None,
            }),
            range: None,
            token: Arc::new(NullCancellationToken),
        }
    }

    #[test]
    fn forwards_an_absolute_path_source_and_options_to_the_mock_worker() {
        let handler = OxfmtPluginHandler::new();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("failed creating test runtime");

        let result = runtime
            .block_on(
                handler.format(test_request(b"const value=1;\n".to_vec()), |_request| {
                    Box::pin(async { Ok(None) })
                }),
            )
            .expect("mock formatting should succeed");

        assert!(result.is_none());
        let forwarded = handler
            .worker
            .last_request()
            .expect("request was not forwarded");
        assert!(forwarded.file_name.is_absolute());
        assert_eq!(forwarded.source_text, "const value=1;\n");
        assert_eq!(forwarded.options, serde_json::json!({ "lineWidth": 100 }));
    }

    #[test]
    fn rejects_invalid_utf8() {
        let handler = OxfmtPluginHandler::new();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("failed creating test runtime");

        let error = runtime
            .block_on(handler.format(test_request(vec![0xFF]), |_request| {
                Box::pin(async { Ok(None) })
            }))
            .expect_err("invalid UTF-8 should fail");

        assert!(error.to_string().contains("invalid utf-8"));
    }

    #[test]
    fn range_formatting_returns_no_change() {
        let handler = OxfmtPluginHandler::new();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("failed creating test runtime");
        let mut request = test_request(b"const value = 1;\n".to_vec());
        request.range = Some(0..5);

        let result = runtime
            .block_on(handler.format(request, |_request| Box::pin(async { Ok(None) })))
            .expect("range formatting should not fail");

        assert!(result.is_none());
        assert!(handler.worker.last_request().is_none());
    }
}
