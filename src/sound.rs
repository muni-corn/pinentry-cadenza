use std::process::Command;

/// Plays the authentication dialog sound in the background.
///
/// Spawns `canberra-gtk-play` with the `dialog-question-authentication` sound
/// event ID, which is not standard in the freedesktop sound theme, but will
/// fallback to `dialog-question` or `dialog` if
/// `dialog-question-authentication` doesn't exist. The child process is not
/// waited on — if the binary is missing or no sound theme is installed the
/// error is silently ignored.
pub fn play_dialog_sound() {
    // fire-and-forget: errors are benign (missing binary, no sound theme, etc.)
    if let Err(e) = Command::new("canberra-gtk-play")
        .args([
            "--id",
            "dialog-question-authentication",
            "--description",
            "Authentication required",
        ])
        .spawn()
    {
        eprintln!("couldn't play dialog sound: {e}")
    }
}

/// Plays the authentication error sound in the background.
///
/// Spawns `canberra-gtk-play` with the `dialog-error-authentication` sound
/// event ID, which is not standard in the freedesktop sound theme, but will
/// fallback to `dialog-error` or `dialog` if `dialog-error-authentication`
/// doesn't exist. The child process is not waited on — if the binary is missing
/// or no sound theme is installed the error is silently ignored.
#[allow(dead_code)]
pub fn play_error_sound() {
    // fire-and-forget: errors are benign (missing binary, no sound theme, etc.)
    if let Err(e) = Command::new("canberra-gtk-play")
        .args([
            "--id",
            "dialog-error-authentication",
            "--description",
            "Incorrect password",
        ])
        .spawn()
    {
        eprintln!("couldn't play dialog error sound: {e}")
    }
}
