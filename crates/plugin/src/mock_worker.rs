use std::path::Path;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

pub struct MockFormatRequest<'a> {
    pub file_name: &'a Path,
    pub source_text: &'a str,
    pub options: &'a serde_json::Value,
}

pub struct MockWorker {
    request_count: AtomicU64,
    #[cfg(test)]
    last_request: std::sync::Mutex<Option<OwnedMockFormatRequest>>,
}

impl MockWorker {
    pub fn new() -> Self {
        Self {
            request_count: AtomicU64::new(0),
            #[cfg(test)]
            last_request: std::sync::Mutex::new(None),
        }
    }

    pub fn format(&self, request: &MockFormatRequest<'_>) -> String {
        self.request_count.fetch_add(1, Ordering::Relaxed);

        #[cfg(test)]
        {
            let owned_request = OwnedMockFormatRequest {
                file_name: request.file_name.to_path_buf(),
                source_text: request.source_text.to_owned(),
                options: request.options.clone(),
            };
            *self
                .last_request
                .lock()
                .expect("mock worker mutex poisoned") = Some(owned_request);
        }

        let _ = (request.file_name, request.options);
        request.source_text.to_owned()
    }

    #[cfg(test)]
    pub fn last_request(&self) -> Option<OwnedMockFormatRequest> {
        self.last_request
            .lock()
            .expect("mock worker mutex poisoned")
            .clone()
    }
}

#[cfg(test)]
#[derive(Clone)]
pub struct OwnedMockFormatRequest {
    pub file_name: std::path::PathBuf,
    pub source_text: String,
    pub options: serde_json::Value,
}
