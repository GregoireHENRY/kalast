use std::path::{Path, PathBuf};

#[derive(Debug)]
pub(crate) struct NativeFile {
    path: PathBuf,
}

impl From<PathBuf> for NativeFile {
    fn from(path: PathBuf) -> Self {
        Self { path }
    }
}

impl egui::DroppedFile for NativeFile {
    fn path(&self) -> &Path {
        &self.path
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.path).map_err(|err| err.to_string())
    }

    // kalast: what egui's trait asks for on wasm32 instead, which 0.36.1 left
    // out. Browsers hand winit no paths, so no file arrives this way there.
    #[cfg(target_arch = "wasm32")]
    fn bytes_async(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>, String>> + '_>> {
        Box::pin(std::future::ready(Err("no file system in the browser".to_owned())))
    }
}
