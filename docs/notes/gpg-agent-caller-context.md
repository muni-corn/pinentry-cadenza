# How gpg-agent passes caller context to pinentry

Verified against GnuPG 2.4.9 (`agent/call-pinentry.c`, `agent/gpg-agent.c`, `agent/command.c`,
`common/session-env.c`, `common/asshelp.c`).

pinentry-cadenza must decide between its GTK dialog and a terminal pinentry (`pinentry-curses` or
`pinentry-tty`). The decision needs to know whether the _caller_ (for example `gpg` run by
`git commit -S` in an SSH session) can see the graphical session. gpg-agent conveys much less about
the caller than it may seem.

## What the pinentry process receives

### Before the assuan greeting

| Channel       | Contents                                                                   |
| ------------- | -------------------------------------------------------------------------- |
| argv          | `--display <value>` **only** if the caller's session env has `DISPLAY`     |
| environment   | gpg-agent's own environment, plus caller values layered on top (see below) |
| stdin, stdout | the assuan pipe                                                            |

gpg-agent never passes `--ttyname` or `--ttytype` on argv.

### After the greeting (assuan `OPTION` lines)

| Option                            | Source                                          |
| --------------------------------- | ----------------------------------------------- |
| `OPTION ttyname=<path>`           | caller's `GPG_TTY`                              |
| `OPTION ttytype=<type>`           | caller's `TERM`                                 |
| `OPTION owner=<pid>/<uid> <host>` | pid of the connected client (for example `gpg`) |
| `OPTION pinentry-user-data=...`   | caller's `PINENTRY_USER_DATA`                   |
| `OPTION lc-ctype`, `lc-messages`  | caller's locale                                 |

`OPTION owner` only names the client. The hostname in it is the agent's own node name, not the
client's.

## Environment inheritance quirk

In `atfork_core`, gpg-agent calls `gnupg_setenv` for each standard variable that the caller
supplied. It **never unsets** a variable the caller did not supply. The pinentry therefore inherits
whatever gpg-agent itself was started with.

For an agent started by a graphical session (for example socket-activated by the systemd user
manager), that includes `WAYLAND_DISPLAY`, `DISPLAY` and `XDG_SESSION_TYPE=wayland`. An SSH caller
has none of those, so nothing overwrites them, and the pinentry sees a graphical environment.

`SSH_CONNECTION`, `SSH_CLIENT` and `SSH_TTY` are not in the list of standard variables
(`stdenvnames` in `common/session-env.c`), so they never reach the pinentry. They only appear if the
agent itself was started inside an SSH session.

The standard variables that the caller can supply are: `GPG_TTY`, `TERM`, `DISPLAY`, `XAUTHORITY`,
`XMODIFIERS`, `WAYLAND_DISPLAY`, `XDG_SESSION_TYPE`, `QT_QPA_PLATFORM`, `GTK_IM_MODULE`,
`DBUS_SESSION_BUS_ADDRESS`, `QT_IM_MODULE`, `INSIDE_EMACS`, `PINENTRY_USER_DATA` and
`PINENTRY_GEOM_HINT`.

Because `XDG_SESSION_TYPE` is in that list, a value of `tty` can only come from the caller (assuming
the agent itself runs in a graphical session). It is a reliable "this caller is a terminal session"
signal when `pam_systemd` sets it.

## The caller must export `GPG_TTY`

`git commit -S` runs `gpg` with stdin, stdout and stderr connected to pipes, so `gpg` cannot
discover a tty by itself. Without `GPG_TTY` in the caller's environment, no `OPTION ttyname` is
sent, and `pinentry-curses` fails with `Inappropriate ioctl for device`. Shells should run
`export GPG_TTY=$(tty)`.

## What this means for the fallback decision

- Anything derived from `--ttyname` or `--ttytype` on argv is dead code when the caller is
  gpg-agent. The options are kept for manual invocation.
- The pre-greeting environment cannot distinguish "agent started in a graphical session, caller over
  SSH" from "agent and caller both local" without help from the caller (`XDG_SESSION_TYPE`,
  `PINENTRY_USER_DATA`).
- After the greeting, `OPTION owner` provides the client pid. Reading `/proc/<pid>/environ` gives
  the caller's real environment, including `SSH_*` and the absence of display variables.
- Once the greeting has been sent, `exec()` is no longer possible. A terminal pinentry has to be
  started as a child process and the already-received commands replayed into it.
