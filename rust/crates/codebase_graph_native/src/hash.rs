use crate::legacy;

pub fn sha256_file(path: &std::path::Path) -> Result<String, std::io::Error> {
    let bytes = std::fs::read(path)?;
    Ok(legacy::stable_sha256_hex(&bytes))
}

pub fn partition_id(path: &str) -> String {
    legacy::stable_partition_id(path)
}
