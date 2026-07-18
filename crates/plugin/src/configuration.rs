use dprint_core::configuration::ConfigKeyMap;
use dprint_core::configuration::GlobalConfiguration;
use dprint_core::plugins::FileMatchingInfo;
use dprint_core::plugins::PluginResolveConfigurationResult;
use serde::Deserialize;
use serde::Serialize;

const JAVASCRIPT_AND_TYPESCRIPT_EXTENSIONS: &[&str] =
    &["js", "jsx", "ts", "tsx", "mjs", "cjs", "mts", "cts"];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResolvedConfiguration {
    pub options: serde_json::Value,
}

pub fn resolve_configuration(
    config: ConfigKeyMap,
    _global_config: GlobalConfiguration,
) -> PluginResolveConfigurationResult<ResolvedConfiguration> {
    let options = serde_json::to_value(config).expect("dprint configuration should serialize");

    PluginResolveConfigurationResult {
        file_matching: FileMatchingInfo {
            file_extensions: JAVASCRIPT_AND_TYPESCRIPT_EXTENSIONS
                .iter()
                .map(|extension| (*extension).to_owned())
                .collect(),
            file_names: Vec::new(),
        },
        diagnostics: Vec::new(),
        config: ResolvedConfiguration { options },
    }
}

#[cfg(test)]
mod tests {
    use dprint_core::configuration::ConfigKeyValue;

    use super::*;

    #[test]
    fn preserves_raw_oxfmt_options() {
        let mut config = ConfigKeyMap::new();
        config.insert("lineWidth".to_owned(), ConfigKeyValue::Number(100));
        config.insert(
            "quoteStyle".to_owned(),
            ConfigKeyValue::String("single".to_owned()),
        );

        let result = resolve_configuration(config, GlobalConfiguration::default());

        assert_eq!(
            result.config.options,
            serde_json::json!({
                "lineWidth": 100,
                "quoteStyle": "single"
            })
        );
        assert_eq!(
            result.file_matching.file_extensions,
            ["js", "jsx", "ts", "tsx", "mjs", "cjs", "mts", "cts"]
        );
        assert!(result.diagnostics.is_empty());
    }
}
