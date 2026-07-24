pub(super) fn parse_usize_arg(args: &[String], index: usize, name: &str) -> Result<usize, String> {
    crate::adapters::required_arg(args, index, name)?
        .parse::<usize>()
        .map_err(|error| format!("{name} must be an integer: {error}"))
}
