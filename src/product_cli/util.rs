use super::*;

pub(super) fn snapshot_file(path: &Path) -> Result<Option<String>, String> {
    if !path.exists() {
        return Ok(None);
    }
    fs::read_to_string(path)
        .map(Some)
        .map_err(|error| format!("failed to snapshot {}: {error}", path.display()))
}

pub(super) fn restore_file(path: &Path, previous: Option<&str>) -> Result<(), String> {
    match previous {
        Some(text) => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    format!("failed to restore directory {}: {error}", parent.display())
                })?;
            }
            fs::write(path, text)
                .map_err(|error| format!("failed to restore {}: {error}", path.display()))
        }
        None => match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("failed to remove {}: {error}", path.display())),
        },
    }
}

pub(super) fn read_json_file(path: &Path) -> Result<serde_json::Value, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read JSON file {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("failed to parse JSON file {}: {error}", path.display()))
}
pub(super) fn required_arg<'a>(
    args: &'a [String],
    index: usize,
    name: &str,
) -> Result<&'a str, String> {
    args.get(index + 1)
        .map(String::as_str)
        .ok_or_else(|| format!("{name} requires a value"))
}

pub(super) fn parse_usize_arg(args: &[String], index: usize, name: &str) -> Result<usize, String> {
    required_arg(args, index, name)?
        .parse::<usize>()
        .map_err(|error| format!("{name} must be an integer: {error}"))
}
