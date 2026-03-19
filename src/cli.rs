use clap::Parser;

/// Wayland pinentry dialog for GnuPG.
#[derive(Parser, Debug)]
#[command(version, about)]
pub struct Args {
    /// X display to use (X11 only; ignored on Wayland).
    #[arg(short = 'd', long)]
    pub display: Option<String>,

    /// Terminal device name (e.g. /dev/pts/1).
    #[arg(short = 'T', long)]
    pub ttyname: Option<String>,

    /// Terminal type (e.g. vt100).
    #[arg(short = 't', long)]
    pub ttytype: Option<String>,

    /// Locale for character types (LC_CTYPE).
    #[arg(short = 'C', long)]
    pub lc_ctype: Option<String>,

    /// Locale for messages (LC_MESSAGES).
    #[arg(short = 'M', long)]
    pub lc_messages: Option<String>,

    /// Parent window ID for transient-for window hints.
    #[arg(short = 'W', long)]
    pub parent_wid: Option<String>,

    /// File to touch after each user interaction.
    #[arg(long)]
    pub touch_file: Option<String>,

    /// Dialog timeout in seconds (0 means no timeout).
    #[arg(long)]
    pub timeout: Option<u32>,

    /// Do not grab keyboard and mouse globally.
    #[arg(short = 'g', long)]
    pub no_global_grab: bool,

    /// Allow an external program to cache the passphrase.
    #[arg(long)]
    pub allow_external_password_cache: bool,

    /// Enable debug output.
    #[arg(long)]
    pub debug: bool,

    /// Purge outdated entries from the key cache on startup.
    #[arg(long)]
    pub purge_keys: bool,

    /// Unknown options and GTK-specific flags (e.g. --gdk-debug) are passed
    /// through to GTK4 unchanged. Use -- to separate them unambiguously.
    #[arg(allow_hyphen_values = true, trailing_var_arg = true)]
    pub gtk_options: Vec<String>,
}
