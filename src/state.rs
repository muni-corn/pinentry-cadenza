use secrecy::SecretString;

/// Accumulated state from Assuan SET* commands and OPTION configuration.
#[derive(Debug, Default)]
pub struct PinentryState {
    // per-dialog fields (cleared by reset_per_request)
    pub desc: Option<String>,
    pub prompt: Option<String>,
    pub title: Option<String>,
    pub ok_label: Option<String>,
    pub cancel_label: Option<String>,
    pub notok_label: Option<String>,
    pub error: Option<String>,
    pub timeout: Option<u32>,
    pub keyinfo: Option<String>,
    pub repeat_prompt: Option<String>,
    pub repeat_error: Option<String>,
    /// Non-None when SETQUALITYBAR was called; empty string means use default
    /// label.
    pub quality_bar: Option<String>,
    pub quality_bar_tt: Option<String>,
    // session-scoped fields (survive RESET, set via OPTION)
    pub ttyname: Option<String>,
    pub ttytype: Option<String>,
    pub lc_ctype: Option<String>,
    pub lc_messages: Option<String>,
    pub display: Option<String>,
    pub grab: bool,
    pub invisible_char: Option<char>,
    pub formatted_passphrase: bool,
}

impl PinentryState {
    /// Clears per-request fields after a dialog completes or on RESET.
    ///
    /// Session-scoped fields (`ttyname`, `display`, `grab`, etc.) are
    /// preserved.
    pub fn reset_per_request(&mut self) {
        self.desc = None;
        self.prompt = None;
        self.title = None;
        self.ok_label = None;
        self.cancel_label = None;
        self.notok_label = None;
        self.error = None;
        self.timeout = None;
        self.keyinfo = None;
        self.repeat_prompt = None;
        self.repeat_error = None;
        self.quality_bar = None;
        self.quality_bar_tt = None;
    }
}

/// The three interactive dialog types a pinentry must support.
#[derive(Debug)]
#[allow(dead_code)]
pub enum DialogRequest {
    GetPin,
    Confirm { one_button: bool },
    Message,
}

/// The result returned from a completed dialog interaction.
#[derive(Debug)]
#[allow(dead_code)]
pub enum DialogResult {
    /// The user submitted a passphrase.
    Pin(SecretString),
    /// The user confirmed (OK).
    Confirmed,
    /// The user chose the "Not OK" option.
    NotConfirmed,
    /// The user cancelled or dismissed the dialog.
    Cancelled,
}
