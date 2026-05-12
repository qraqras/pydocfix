use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CliArgs {
    pub(crate) fix: bool,
    pub(crate) diff: bool,
    pub(crate) unsafe_fixes: bool,
    pub(crate) jobs: Option<usize>,
    pub(crate) ignore: Vec<String>,
    pub(crate) exclude: Vec<String>,
    pub(crate) paths: Vec<PathBuf>,
}

pub(crate) fn parse_cli_args(args: impl IntoIterator<Item = String>) -> Result<Option<CliArgs>, String> {
    let mut args = args.into_iter().collect::<Vec<_>>();
    if args.is_empty() || matches!(args.first().map(String::as_str), Some("-h" | "--help")) {
        return Ok(None);
    }

    let fix = remove_flag(&mut args, "--fix");
    let diff = remove_flag(&mut args, "--diff");
    let unsafe_fixes = remove_flag(&mut args, "--unsafe-fixes");
    let jobs = remove_option(&mut args, "--jobs")?
        .map(|value| parse_jobs(&value))
        .transpose()?;
    let ignore = parse_rule_filter(remove_option(&mut args, "--ignore")?);
    let exclude = parse_rule_filter(remove_option(&mut args, "--exclude")?);

    if args.is_empty() {
        return Ok(None);
    }

    Ok(Some(CliArgs {
        fix,
        diff,
        unsafe_fixes,
        jobs,
        ignore,
        exclude,
        paths: args.into_iter().map(PathBuf::from).collect(),
    }))
}

pub(crate) fn print_help() {
    println!("pydocsync");
    println!();
    println!("Usage: pydocsync [OPTIONS] <PATH>...");
    println!();
    println!("Keeps Python signatures and docstrings in sync.");
    println!();
    println!("Options:");
    println!("  --fix");
    println!("  --diff");
    println!("  --unsafe-fixes");
    println!("  --ignore <RULE[,RULE...]>");
    println!("  --exclude <PATTERN[,PATTERN...]>");
    println!("  --jobs <N>");
}

fn parse_jobs(value: &str) -> Result<usize, String> {
    let jobs = value
        .parse::<usize>()
        .map_err(|_| format!("invalid --jobs value {value:?}; expected a positive integer"))?;
    if jobs == 0 {
        return Err("invalid --jobs value 0; expected a positive integer".to_string());
    }
    Ok(jobs)
}

fn remove_flag(args: &mut Vec<String>, flag: &str) -> bool {
    let original_len = args.len();
    args.retain(|arg| arg != flag);
    args.len() != original_len
}

fn remove_option(args: &mut Vec<String>, flag: &str) -> Result<Option<String>, String> {
    if let Some(index) = args.iter().position(|arg| arg == flag) {
        args.remove(index);
        if index >= args.len() {
            return Err(format!("missing value for {flag}"));
        }
        return Ok(Some(args.remove(index)));
    }

    let prefix = format!("{flag}=");
    if let Some(index) = args.iter().position(|arg| arg.starts_with(&prefix)) {
        let value = args.remove(index);
        return Ok(Some(value[prefix.len()..].to_string()));
    }

    Ok(None)
}

fn parse_rule_filter(value: Option<String>) -> Vec<String> {
    value
        .into_iter()
        .flat_map(|value| value.split(',').map(str::trim).map(str::to_string).collect::<Vec<_>>())
        .filter(|value| !value.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_none_for_help() {
        assert!(parse_cli_args(["--help".to_string()]).unwrap().is_none());
    }

    #[test]
    fn parses_paths_and_options() {
        let args = parse_cli_args([
            "--fix".to_string(),
            "--unsafe-fixes".to_string(),
            "--ignore".to_string(),
            "yields,args-param-out-of-order".to_string(),
            "--jobs=4".to_string(),
            "src".to_string(),
        ])
        .unwrap()
        .unwrap();

        assert!(args.fix);
        assert!(args.unsafe_fixes);
        assert_eq!(args.ignore, vec!["yields", "args-param-out-of-order"]);
        assert_eq!(args.jobs, Some(4));
        assert_eq!(args.paths, vec![PathBuf::from("src")]);
    }

    #[test]
    fn reports_missing_option_value() {
        let error = parse_cli_args(["--ignore".to_string()]).unwrap_err();

        assert_eq!(error, "missing value for --ignore");
    }

    #[test]
    fn reports_invalid_jobs() {
        let error = parse_cli_args(["--jobs=0".to_string(), "src".to_string()]).unwrap_err();

        assert_eq!(error, "invalid --jobs value 0; expected a positive integer");
    }
}
