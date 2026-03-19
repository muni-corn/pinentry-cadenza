/// Standard pinentry CLI options and whether each takes a value argument.
///
/// gpg-agent passes these flags when spawning a pinentry. GTK4 does not
/// recognize them, so they must be stripped before the GTK application
/// initializes. The values are also sent via Assuan `OPTION` commands at the
/// start of each session, so discarding them at the CLI level is safe.
const PINENTRY_OPTIONS: &[(&str, bool)] = &[
    // informational flags (no value)
    ("--version", false),
    ("-V", false),
    ("--help", false),
    ("-h", false),
    // display and terminal configuration (take a value)
    ("--display", true),
    ("-d", true),
    ("--ttyname", true),
    ("-T", true),
    ("--ttytype", true),
    ("-t", true),
    ("--lc-ctype", true),
    ("-C", true),
    ("--lc-messages", true),
    ("-M", true),
    // window and session options (take a value)
    ("--parent-wid", true),
    ("-W", true),
    ("--touch-file", true),
    ("--timeout", true),
    // boolean flags (no value)
    ("--no-global-grab", false),
    ("-g", false),
    ("--allow-external-password-cache", false),
    ("--debug", false),
    ("--purge-keys", false),
];

/// Strips known pinentry-specific CLI options from `args`, returning the
/// remaining arguments suitable for passing to GTK4.
///
/// GTK4 does not recognize pinentry options (such as `--display`). Any unknown
/// option causes GTK4 to print an error and exit, preventing the dialog from
/// showing. gpg-agent passes these options when spawning the pinentry and also
/// sends them as Assuan `OPTION` commands, so they can safely be dropped here.
///
/// Both `--option VALUE` (space-separated) and `--option=VALUE` (equals-sign)
/// forms are handled.
pub fn filter_gtk_args(args: &[String]) -> Vec<String> {
    let mut result = Vec::new();
    let mut i = 0;

    while i < args.len() {
        let arg = &args[i];

        // check for the `--option=value` form first
        if let Some(opt) = PINENTRY_OPTIONS
            .iter()
            .find(|(opt, has_value)| *has_value && arg.starts_with(&format!("{opt}=")))
            .map(|(opt, _)| *opt)
        {
            // the value is embedded; verify the prefix matches exactly
            // (e.g. "--display=" must not match "--displays=...")
            let expected_prefix = format!("{opt}=");
            if arg.starts_with(&expected_prefix) {
                i += 1;
                continue;
            }
        }

        // check for the space-separated form `--option VALUE` or bare `--flag`
        if let Some((_, has_value)) = PINENTRY_OPTIONS.iter().find(|(opt, _)| *arg == **opt) {
            i += 1;
            // skip the following value token if this option takes one
            if *has_value && i < args.len() {
                i += 1;
            }
            continue;
        }

        result.push(arg.clone());
        i += 1;
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn strips_display_space_form() {
        let input = args(&["--display", ":0", "--other"]);
        assert_eq!(filter_gtk_args(&input), args(&["--other"]));
    }

    #[test]
    fn strips_display_equals_form() {
        let input = args(&["--display=:0", "--other"]);
        assert_eq!(filter_gtk_args(&input), args(&["--other"]));
    }

    #[test]
    fn strips_boolean_flag() {
        let input = args(&["--no-global-grab", "--other"]);
        assert_eq!(filter_gtk_args(&input), args(&["--other"]));
    }

    #[test]
    fn strips_short_option_with_value() {
        let input = args(&["-T", "/dev/pts/1", "--other"]);
        assert_eq!(filter_gtk_args(&input), args(&["--other"]));
    }

    #[test]
    fn strips_multiple_options() {
        let input = args(&[
            "--display",
            ":0",
            "--ttyname",
            "/dev/pts/1",
            "--lc-ctype",
            "en_US.UTF-8",
            "--lc-messages",
            "en_US.UTF-8",
        ]);
        assert_eq!(filter_gtk_args(&input), args(&[]));
    }

    #[test]
    fn preserves_unknown_args() {
        let input = args(&["--gdk-debug=events", "--display", ":0"]);
        assert_eq!(filter_gtk_args(&input), args(&["--gdk-debug=events"]));
    }

    #[test]
    fn empty_input() {
        assert_eq!(filter_gtk_args(&[]), Vec::<String>::new());
    }
}
