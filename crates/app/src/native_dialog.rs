//! Native dialogs on Linux, without rfd.
//!
//! rfd 0.15 (and 0.17) pass `--no-markup` as the first argument of every
//! `zenity` call, unconditionally. That flag was removed in zenity 4.x,
//! so on a system with zenity 4 every dialog fails to start and rfd
//! reports it as "the user cancelled" — file pickers that silently do
//! nothing, message boxes that never appear (Q27). This module calls
//! `zenity` directly, with the same arguments rfd used minus that one
//! flag, so the dialogs work on zenity 3.x and 4.x alike.
//!
//! The API mirrors the exact rfd surface the app uses (see `winfx.rs`),
//! so the call sites do not change: `FileDialog` with `set_title`,
//! `set_directory`, `set_file_name`, `add_filter`, `pick_file`,
//! `save_file`, `pick_folder`; `MessageDialog` with `set_title`,
//! `set_level`, `set_description`, `set_buttons`, `show`.
//!
//! Two small differences from rfd, both improvements:
//! * `set_directory` is honoured (rfd's zenity backend ignored it):
//!   the dialog opens in that folder, or in that folder with the
//!   given file name pre-filled.
//! * message text is escaped for Pango markup (`--no-markup` used to
//!   do that job): a `<` in an error message shows as `<`, it does not
//!   break the dialog.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The result of a message box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageDialogResult {
    Ok,
    Cancel,
    Yes,
    No,
}

/// How serious a message box is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MessageLevel {
    #[default]
    Info,
    Warning,
    Error,
}

/// Which buttons a message box shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MessageButtons {
    #[default]
    Ok,
    YesNo,
}

/// A file / folder picker.
#[derive(Debug, Default)]
pub struct FileDialog {
    title: Option<String>,
    starting_directory: Option<PathBuf>,
    file_name: Option<String>,
    filters: Vec<(String, Vec<String>)>,
}

impl FileDialog {
    pub fn new() -> Self {
        Self::default()
    }

    /// No call site uses it today (rfd has it, so it stays for parity).
    #[allow(dead_code)]
    pub fn set_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn set_directory(mut self, path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        if path.as_os_str().is_empty() {
            self.starting_directory = None;
        } else {
            self.starting_directory = Some(path.to_path_buf());
        }
        self
    }

    pub fn set_file_name(mut self, name: impl Into<String>) -> Self {
        self.file_name = Some(name.into());
        self
    }

    pub fn add_filter(mut self, name: impl Into<String>, extensions: &[impl ToString]) -> Self {
        self.filters.push((
            name.into(),
            extensions.iter().map(|e| e.to_string()).collect(),
        ));
        self
    }

    /// The file the picker starts in / with, if any: the directory
    /// plus the name when both are set, the directory alone (with a
    /// trailing slash, so zenity opens there) or the name alone.
    fn filename_arg(&self) -> Option<String> {
        match (&self.starting_directory, &self.file_name) {
            (Some(dir), Some(name)) => Some(dir.join(name).to_string_lossy().into_owned()),
            (Some(dir), None) => {
                let mut s = dir.to_string_lossy().into_owned();
                if !s.ends_with('/') {
                    s.push('/');
                }
                Some(s)
            }
            (None, Some(name)) => Some(name.clone()),
            (None, None) => None,
        }
    }

    /// The full command for one kind of picker, without the program
    /// name: kept pure so tests can check the arguments (in particular
    /// that `--no-markup` is never among them).
    fn command_args(&self, kind: FileKind) -> Vec<String> {
        let mut args = vec!["--file-selection".to_string()];
        match kind {
            FileKind::Pick => {}
            FileKind::Save => {
                args.push("--save".into());
                args.push("--confirm-overwrite".into());
            }
            FileKind::Folder => {
                args.push("--directory".into());
            }
        }
        if let Some(title) = &self.title {
            args.push("--title".into());
            args.push(title.clone());
        }
        for (name, exts) in &self.filters {
            args.push("--file-filter".into());
            let patterns: Vec<String> = exts.iter().map(|e| format!("*.{e}")).collect();
            args.push(format!("{} | {}", name, patterns.join(" ")));
        }
        if let Some(name) = self.filename_arg() {
            args.push("--filename".into());
            args.push(name);
        }
        args
    }

    fn command(&self, kind: FileKind) -> Command {
        let mut cmd = Command::new("zenity");
        cmd.args(self.command_args(kind));
        cmd
    }

    pub fn pick_file(self) -> Option<PathBuf> {
        run_file(self.command(FileKind::Pick))
    }

    pub fn save_file(self) -> Option<PathBuf> {
        run_file(self.command(FileKind::Save))
    }

    pub fn pick_folder(self) -> Option<PathBuf> {
        run_file(self.command(FileKind::Folder))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileKind {
    Pick,
    Save,
    Folder,
}

/// Run a picker: the chosen path on success, `None` when the user
/// cancelled, closed the dialog, or zenity is missing (that last case
/// is written to stderr, so it is not a silent cancel).
fn run_file(cmd: Command) -> Option<PathBuf> {
    let mut cmd = cmd;
    match cmd.output() {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout);
            let picked = text.trim();
            if !picked.is_empty() {
                Some(PathBuf::from(picked))
            } else {
                None
            }
        }
        Err(e) => {
            eprintln!("zenity is missing or could not start: {e}");
            None
        }
    }
}

/// A message box.
#[derive(Debug, Default)]
pub struct MessageDialog {
    title: Option<String>,
    level: MessageLevel,
    description: Option<String>,
    buttons: MessageButtons,
}

impl MessageDialog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn set_level(mut self, level: MessageLevel) -> Self {
        self.level = level;
        self
    }

    pub fn set_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn set_buttons(mut self, buttons: MessageButtons) -> Self {
        self.buttons = buttons;
        self
    }

    /// The full command, without the program name (see `FileDialog`).
    fn command_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        match self.buttons {
            MessageButtons::YesNo => args.push("--question".into()),
            MessageButtons::Ok => match self.level {
                MessageLevel::Info => args.push("--info".into()),
                MessageLevel::Warning => args.push("--warning".into()),
                MessageLevel::Error => args.push("--error".into()),
            },
        }
        if let Some(title) = &self.title {
            args.push("--title".into());
            args.push(escape_markup(title));
        }
        args.push("--text".into());
        args.push(escape_markup(self.description.as_deref().unwrap_or("")));
        args
    }

    fn command(&self) -> Command {
        let mut cmd = Command::new("zenity");
        cmd.args(self.command_args());
        cmd
    }

    pub fn show(self) -> MessageDialogResult {
        run_message(self.command(), self.buttons)
    }
}

/// Run a message box: `Ok`/`Yes` when zenity exits 0, `Cancel`/`No`
/// when it exits 1 or is missing (that last case is written to stderr,
/// so it is not a silent cancel).
fn run_message(cmd: Command, buttons: MessageButtons) -> MessageDialogResult {
    let mut cmd = cmd;
    match cmd.output() {
        Ok(out) => {
            if out.status.success() {
                match buttons {
                    MessageButtons::Ok => MessageDialogResult::Ok,
                    MessageButtons::YesNo => MessageDialogResult::Yes,
                }
            } else {
                match buttons {
                    MessageButtons::Ok => MessageDialogResult::Cancel,
                    MessageButtons::YesNo => MessageDialogResult::No,
                }
            }
        }
        Err(e) => {
            eprintln!("zenity is missing or could not start: {e}");
            match buttons {
                MessageButtons::Ok => MessageDialogResult::Cancel,
                MessageButtons::YesNo => MessageDialogResult::No,
            }
        }
    }
}

/// Escape text for zenity's `--text` / `--title`: without
/// `--no-markup` zenity reads Pango markup there, so a `<` in an
/// error message would otherwise break the dialog or vanish.
fn escape_markup(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn file_commands_never_pass_no_markup() {
        let dlg = FileDialog::new()
            .set_title("Pick one")
            .set_directory("/tmp/dl")
            .set_file_name("a.bin")
            .add_filter("Videos", &["mp4", "mkv"]);
        for kind in [FileKind::Pick, FileKind::Save, FileKind::Folder] {
            let args = dlg.command_args(kind);
            assert!(
                !args.iter().any(|a| a == "--no-markup"),
                "{kind:?} passes --no-markup: {args:?}"
            );
            assert!(args.contains(&"--file-selection".to_string()));
        }
        let pick = dlg.command_args(FileKind::Pick);
        assert!(pick.contains(&"--title".to_string()));
        assert!(pick.contains(&"Pick one".to_string()));
        assert!(pick.contains(&"--file-filter".to_string()));
        assert!(pick.contains(&"Videos | *.mp4 *.mkv".to_string()));
        assert!(pick.contains(&"--filename".to_string()));
        assert!(pick.contains(&"/tmp/dl/a.bin".to_string()));
        let save = dlg.command_args(FileKind::Save);
        assert!(save.contains(&"--save".to_string()));
        assert!(save.contains(&"--confirm-overwrite".to_string()));
        let folder = dlg.command_args(FileKind::Folder);
        assert!(folder.contains(&"--directory".to_string()));
    }

    #[test]
    fn directory_alone_opens_in_that_folder() {
        let dlg = FileDialog::new().set_directory("/tmp/dl");
        let args = dlg.command_args(FileKind::Pick);
        assert!(args.contains(&"/tmp/dl/".to_string()));
        let dlg = FileDialog::new().set_file_name("a.bin");
        assert!(dlg
            .command_args(FileKind::Pick)
            .contains(&"a.bin".to_string()));
    }

    #[test]
    fn message_commands_never_pass_no_markup() {
        let info = MessageDialog::new()
            .set_title("Ezoo Download Manager")
            .set_description("a <b> & \"c\"")
            .command_args();
        assert!(!info.iter().any(|a| a == "--no-markup"));
        assert!(info.contains(&"--info".to_string()));
        assert!(info.contains(&"a &lt;b&gt; &amp; &quot;c&quot;".to_string()));
        let question = MessageDialog::new()
            .set_title("Ezoo Download Manager")
            .set_description("sure?")
            .set_buttons(MessageButtons::YesNo)
            .command_args();
        assert!(question.contains(&"--question".to_string()));
        assert!(!question.iter().any(|a| a == "--info"));
        let warning = MessageDialog::new()
            .set_level(MessageLevel::Warning)
            .command_args();
        assert!(warning.contains(&"--warning".to_string()));
        let error = MessageDialog::new()
            .set_level(MessageLevel::Error)
            .command_args();
        assert!(error.contains(&"--error".to_string()));
    }

    /// A fake `zenity` for the runtime tests: it records its arguments
    /// and either prints a path and exits 0, or exits 1 (cancelled).
    fn fake_zenity(dir: &Path, exit_zero: bool) -> (PathBuf, PathBuf) {
        let script = dir.join("zenity");
        let log = dir.join("args.log");
        let mut f = std::fs::File::create(&script).unwrap();
        writeln!(f, "#!/bin/sh").unwrap();
        writeln!(f, "printf '%s\\n' \"$@\" >> {}", log.display()).unwrap();
        if exit_zero {
            writeln!(f, "printf '%s\\n' \"$FAKE_OUT\"").unwrap();
            writeln!(f, "exit 0").unwrap();
        } else {
            writeln!(f, "exit 1").unwrap();
        }
        drop(f);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        (script, log)
    }

    fn run_with_fake(cmd_args: Vec<String>, fake: &Path, fake_out: &str) -> std::process::Output {
        Command::new(fake)
            .args(&cmd_args)
            .env("FAKE_OUT", fake_out)
            .output()
            .unwrap()
    }

    #[test]
    fn fake_zenity_answers_like_the_real_one() {
        let dir = std::env::temp_dir().join(format!("ezoo-zenity-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // pick a file: the fake prints a path, exit 0
        let (fake, log) = fake_zenity(&dir, true);
        let dlg = FileDialog::new()
            .set_title("Pick one")
            .set_directory("/tmp/dl")
            .add_filter("Videos", &["mp4"]);
        let args = dlg.command_args(FileKind::Pick);
        let out = run_with_fake(args, &fake, "/tmp/dl/chosen.mp4");
        assert!(out.status.success());
        let picked = String::from_utf8_lossy(&out.stdout);
        assert_eq!(picked.trim(), "/tmp/dl/chosen.mp4");
        let recorded = std::fs::read_to_string(&log).unwrap();
        assert!(recorded.contains("--file-selection"));
        assert!(
            !recorded.contains("--no-markup"),
            "the fake saw --no-markup"
        );

        // cancel: exit 1, no output
        let (fake, _) = fake_zenity(&dir, false);
        let out = run_with_fake(dlg.command_args(FileKind::Pick), &fake, "");
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());

        // a question: exit 0 is Yes, exit 1 is No
        let q = MessageDialog::new()
            .set_title("Ezoo Download Manager")
            .set_description("sure?")
            .set_buttons(MessageButtons::YesNo);
        let (fake, _) = fake_zenity(&dir, true);
        assert!(run_with_fake(q.command_args(), &fake, "").status.success());
        let (fake, _) = fake_zenity(&dir, false);
        assert!(!run_with_fake(q.command_args(), &fake, "").status.success());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_zenity_is_a_cancel_not_a_crash() {
        // a program that does not exist: the picker answers None, and
        // a message box answers Cancel / No — never a panic
        let mut cmd = Command::new("/nonexistent/zenity-for-test");
        cmd.arg("--file-selection");
        assert_eq!(run_file(cmd), None);
        let mut cmd = Command::new("/nonexistent/zenity-for-test");
        cmd.args(["--info", "--title", "t", "--text", "x"]);
        assert_eq!(
            run_message(cmd, MessageButtons::Ok),
            MessageDialogResult::Cancel
        );
        let mut cmd = Command::new("/nonexistent/zenity-for-test");
        cmd.args(["--question", "--title", "t", "--text", "x"]);
        assert_eq!(
            run_message(cmd, MessageButtons::YesNo),
            MessageDialogResult::No
        );
    }
}
