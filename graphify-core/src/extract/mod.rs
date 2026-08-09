use crate::types::ExtractionResult;
use anyhow::{Result, anyhow};
use std::fs;
use std::path::Path;

pub mod c;
pub mod cpp;
pub mod go;
pub mod java;
pub mod javascript;
pub mod php;
pub mod python;
pub mod rust;
pub mod swift;

#[cfg(test)]
mod tests;

pub fn extract_file(path: &Path) -> Result<ExtractionResult> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let content = fs::read_to_string(path)
        .map_err(|e| anyhow!("Failed to read file {}: {}", path.display(), e))?;

    let file_path = path.to_string_lossy().to_string();

    match ext.as_str() {
        "py" => python::extract(&content, &file_path),
        "rs" => rust::extract(&content, &file_path),
        "go" => go::extract(&content, &file_path),
        "js" | "jsx" | "mjs" | "ts" | "tsx" | "mts" => javascript::extract(&content, &file_path),
        "c" | "h" => c::extract(&content, &file_path),
        "cpp" | "cc" | "cxx" | "hpp" | "h++" | "hh" => cpp::extract(&content, &file_path),
        "php" => php::extract(&content, &file_path),
        "java" => java::extract(&content, &file_path),
        "swift" => swift::extract(&content, &file_path),
        _ => anyhow::bail!("Unsupported file extension: {}", ext),
    }
}
