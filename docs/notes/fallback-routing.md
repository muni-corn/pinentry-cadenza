# GUI or terminal: how pinentry-cadenza routes a request

See `gpg-agent-caller-context.md` for what gpg-agent actually sends.

## Decision points

### 1. Before the greeting (`exec()` is still possible)

`fallback::should_use_curses` replaces the process with `pinentry-curses` or `pinentry-tty` if any
of these hold, checked in this order:

| Signal                                                 | Meaning                                 |
| ------------------------------------------------------ | --------------------------------------- |
| `PINENTRY_USER_DATA` has `USE_TTY=1` or `USE_CURSES=1` | the caller asked for a terminal         |
| `SSH_CONNECTION`, `SSH_CLIENT` or `SSH_TTY` in our env | the agent itself runs in an SSH session |
| `XDG_SESSION_TYPE=tty`                                 | forwarded by the caller, so a terminal  |
| no `WAYLAND_DISPLAY` or `DISPLAY` (env or `--display`) | nothing to draw on                      |

### 2. At the first dialog request (the greeting has been sent)

The assuan preamble (greeting, `OPTION`, `SET*`, `GETINFO`) runs before GTK starts. At the first
`GETPIN`, `CONFIRM` or `MESSAGE`, `main` reads the environment of the client process named by
`OPTION owner` from `/proc/<pid>/environ`, and `fallback::should_hand_off` decides:

| Caller environment                                  | `OPTION ttyname` | Result   |
| --------------------------------------------------- | ---------------- | -------- |
| has `SSH_CONNECTION`, `SSH_CLIENT` or `SSH_TTY`     | any              | terminal |
| no `WAYLAND_DISPLAY` and no `DISPLAY`               | present          | terminal |
| no `WAYLAND_DISPLAY` and no `DISPLAY`               | absent           | GUI      |
| has a display                                       | any              | GUI      |
| environment unreadable (no `owner`, gone, no /proc) | any              | GUI      |

An SSH caller without a tty is still sent to the terminal. `pinentry-curses` then fails with
`Inappropriate ioctl for device`, which is the standard sign that `GPG_TTY` isn't exported. That is
better than a hidden GUI dialog that hangs the caller.

The terminal binary is chosen by `fallback::terminal_candidates`: an explicit hint wins, otherwise
`pinentry-curses` is preferred when `ttytype` is a capable terminal and `pinentry-tty` otherwise.
The other one is the fallback.

## The handoff

`fallback::hand_off_to_terminal` starts the terminal pinentry as a child process, then:

1. reads its greeting;
2. replays every `OPTION`, `SET*` and `RESET` line received so far (answers are discarded; gpg-agent
   already got ours);
3. sends the pending dialog request;
4. relays bytes in both directions until the child exits, and exits with its status.

If the child can't be started or set up, the pending request is answered with
`ERR 83886165 no pinentry: ...`.

## Known limitations

- A tmux or zellij session started locally and attached over SSH has a graphical environment, so it
  gets the GUI. Set `PINENTRY_USER_DATA=USE_CURSES=1` (or `USE_TTY=1`) in that session.
- With agent forwarding, the client pid belongs to sshd, whose environment is usually unreadable, so
  the GUI is used. That is right: the person forwarding the agent is at this machine.
- Reading `/proc` only works on Linux. Elsewhere only the pre-greeting checks apply.
- GTK starts only when the first dialog is requested, so the dialog appears a few tens of
  milliseconds later than before.

## Manual test recipe

Don't use your real agent. Build, then start a throwaway one from a graphical terminal so that it
inherits `WAYLAND_DISPLAY` (this reproduces the original bug):

```sh
nix build
export GNUPGHOME=$(mktemp -d)
gpg-agent --daemon --pinentry-program "$PWD/result/bin/pinentry-cadenza"
gpg --batch --passphrase '' --quick-generate-key test@example.invalid
gpg --passwd test@example.invalid   # set a passphrase through the pinentry
```

Then, with the same `GNUPGHOME` and `GPG_TTY=$(tty)` exported:

- from an SSH session: `echo hi | gpg --clearsign` prompts in curses;
- from a local graphical terminal: the same command shows the GUI;
- from SSH, enter a wrong passphrase once: the retry prompt appears in curses;
- `PINENTRY_USER_DATA=USE_TTY=1` in a local terminal gives `pinentry-tty`;
- from SSH with `GPG_TTY` unset: gpg reports `Inappropriate ioctl for device`.

To exercise the handoff without SSH, give the protocol a client pid whose environment has
`SSH_CONNECTION`:

```sh
env SSH_CONNECTION=x sleep 30 & owner=$!
printf 'OPTION ttyname=%s\nOPTION owner=%s/%s host\nGETPIN\nBYE\n' \
  "$(tty)" "$owner" "$(id -u)" | target/debug/pinentry-cadenza
```
