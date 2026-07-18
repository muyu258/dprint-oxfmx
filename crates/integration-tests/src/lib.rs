//! Integration-test support for the dprint Oxfmt process plugin.

#[cfg(test)]
mod tests {
    #[test]
    fn test_harness_is_wired_into_the_workspace() {
        assert_eq!(
            env!("CARGO_PKG_NAME"),
            "dprint-plugin-oxfmt-integration-tests"
        );
    }
}
