//! Explicit, optional registration of the bundled CLI on the user's PATH.
//! Merely checking availability never changes a profile or the registry.

use std::env;
use std::io::Write as _;
use std::path::{Path, PathBuf};
#[cfg(not(target_os = "linux"))]
use std::sync::Mutex;

#[cfg(any(windows, test))]
#[path = "cli_path_windows.rs"]
mod windows;

#[cfg(not(target_os = "linux"))]
static INSTALL_LOCK: Mutex<()> = Mutex::new(());
const CHOICE_FILE: &str = "cli-path-choice.json";
#[cfg(not(target_os = "linux"))]
const WINDOWS_INSTALLER_MARKER: &str = ".sniper-installed";
#[cfg(target_os = "linux")]
const LINUX_CLI_PATH_INSTRUCTIONS: &str = "Install the Debian package to provide /usr/bin/sniper-cli, or use the portable archive's bin/sniper-cli by its full path and add that bin directory to your terminal's PATH yourself. Sniper does not edit shell profiles on Linux.";

#[derive(Debug, Default, serde::Serialize)]
pub struct CliPathInstall {
    pub updated: Vec<String>,
    pub unchanged: Vec<String>,
    pub warnings: Vec<String>,
    pub message: String,
}

#[derive(Debug, serde::Serialize)]
pub struct CliPathStatus {
    pub platform: &'static str,
    pub supported: bool,
    pub registered: bool,
    pub can_install: bool,
    pub should_prompt: bool,
    pub directory: Option<String>,
    pub message: String,
}

/// The choice belongs to the app, not a webview origin (whose port can change).
/// An interrupted or failed registration is retried explicitly from Settings.
pub fn defer_cli_path(data_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(data_dir).map_err(|e| format!("Could not save CLI choice: {e}"))?;
    let path = data_dir.join(CHOICE_FILE);
    let mut file = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Ok(()),
        Err(e) => return Err(format!("Could not save CLI choice: {e}")),
    };
    file.write_all(b"{\"version\":1,\"prompt_dismissed\":true}\n")
        .and_then(|()| file.sync_all())
        .and_then(|()| crate::platform::sync_directory(data_dir))
        .map_err(|e| format!("Could not save CLI choice: {e}"))
}

#[cfg(target_os = "linux")]
pub fn cli_path_status(_data_dir: &Path) -> CliPathStatus {
    let executable =
        env::current_exe().map_err(|e| format!("Could not locate the running app: {e}"));
    let search_dirs = env::var_os("PATH")
        .map(|path| env::split_paths(&path).collect())
        .unwrap_or_default();
    linux_cli_path_status(executable, search_dirs)
}

#[cfg(target_os = "linux")]
fn linux_cli_path_status(
    executable: Result<PathBuf, String>,
    search_dirs: Vec<PathBuf>,
) -> CliPathStatus {
    use std::os::unix::fs::MetadataExt;

    let mut status = CliPathStatus {
        platform: "linux",
        supported: true,
        registered: false,
        can_install: false,
        should_prompt: false,
        directory: None,
        message: String::new(),
    };
    let result = (|| {
        let executable = executable?;
        let directory = executable
            .parent()
            .ok_or("Could not locate the app directory.")?;
        status.directory = Some(directory.display().to_string());
        let cli = directory.join("sniper-cli");
        let Some(cli_metadata) = linux_executable_metadata(&cli) else {
            return Err(format!(
                "The sibling sniper-cli is missing or not executable at {}.",
                cli.display()
            ));
        };
        // PATH is ordered: a different executable earlier in it masks this
        // copy. Do not inspect shell profiles or assume a login shell's PATH.
        for directory in search_dirs {
            let candidate = directory.join("sniper-cli");
            let Some(metadata) = linux_executable_metadata(&candidate) else {
                continue;
            };
            if (metadata.dev(), metadata.ino()) == (cli_metadata.dev(), cli_metadata.ino()) {
                status.registered = true;
                status.message = "This app's sniper-cli is available on the running app's PATH. A terminal may have a different PATH.".to_string();
                return Ok(());
            }
            return Err(format!("Another sniper-cli at {} takes precedence on the running app's PATH. Resolve that PATH conflict to use this copy.", candidate.display()));
        }
        Err("This app's sniper-cli is not on the running app's PATH. A terminal may have a different PATH.".to_string())
    })();
    if let Err(error) = result {
        status.message = format!("{error} {LINUX_CLI_PATH_INSTRUCTIONS}");
    }
    status
}

#[cfg(target_os = "linux")]
fn linux_executable_metadata(path: &Path) -> Option<std::fs::Metadata> {
    use std::os::unix::ffi::OsStrExt;

    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    // Match execution permission for this process, including ACLs and noexec
    // mounts, without executing the candidate. The CString lives through the call.
    let executable = unsafe {
        libc::faccessat(libc::AT_FDCWD, path.as_ptr(), libc::X_OK, libc::AT_EACCESS) == 0
    };
    executable.then_some(metadata)
}

#[cfg(not(target_os = "linux"))]
pub fn cli_path_status(data_dir: &Path) -> CliPathStatus {
    let platform = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(windows) {
        "windows"
    } else {
        "unsupported"
    };
    let mut status = CliPathStatus {
        platform,
        supported: platform != "unsupported",
        registered: false,
        can_install: false,
        should_prompt: false,
        directory: None,
        message: String::new(),
    };
    let result = (|| {
        let directory = bundled_cli_directory()?;
        status.directory = Some(directory.display().to_string());
        #[cfg(windows)]
        let (registered, conflict) = {
            let found = windows::inspect(&directory)?;
            (found.registered, found.conflict)
        };
        #[cfg(not(windows))]
        let (registered, conflict) =
            inspect_macos(&directory, &shell_targets()?, macos_search_dirs())?;
        status.registered = registered;
        status.can_install = conflict.is_none();
        status.message = conflict.unwrap_or_else(|| if registered {
            "sniper-cli is already registered. Open a new terminal to use it.".to_string()
        } else {
            "Add the bundled sniper-cli for this user. Existing PATH entries are preserved. Open a new terminal afterwards.".to_string()
        });
        status.should_prompt = should_offer_cli_path(
            status.can_install,
            registered,
            data_dir.join(CHOICE_FILE).exists(),
            cfg!(windows) && directory.join(WINDOWS_INSTALLER_MARKER).is_file(),
        );
        Ok::<(), String>(())
    })();
    if let Err(error) = result {
        status.message = error;
    }
    status
}

#[cfg(any(not(target_os = "linux"), test))]
fn should_offer_cli_path(
    can_install: bool,
    registered: bool,
    decided: bool,
    installed_by_setup: bool,
) -> bool {
    // Setup already offered an unchecked choice. Its installed copy must not
    // immediately ask again; Settings still allows a later explicit opt-in.
    can_install && !registered && !decided && !installed_by_setup
}

#[cfg(target_os = "linux")]
pub fn install_cli_path() -> Result<CliPathInstall, String> {
    Err(LINUX_CLI_PATH_INSTRUCTIONS.to_string())
}

#[cfg(not(target_os = "linux"))]
pub fn install_cli_path() -> Result<CliPathInstall, String> {
    let _guard = INSTALL_LOCK
        .lock()
        .map_err(|_| "CLI PATH setup is unavailable. Restart Sniper and retry.".to_string())?;
    let directory = bundled_cli_directory()?;
    #[cfg(windows)]
    {
        windows::install(&directory)
    }
    #[cfg(not(windows))]
    {
        let home = home_dir()?;
        let _profile_lock = lock_profiles(&home)?;
        install_macos(&directory, &shell_targets()?, macos_search_dirs())
    }
}

#[cfg(not(target_os = "linux"))]
fn bundled_cli_directory() -> Result<PathBuf, String> {
    if !cfg!(any(target_os = "macos", windows)) {
        return Err(
            "Automatic CLI PATH setup is available in the macOS and Windows desktop apps."
                .to_string(),
        );
    }
    let exe = env::current_exe().map_err(|e| format!("Could not locate the running app: {e}"))?;
    let directory = exe
        .parent()
        .ok_or("Could not locate the app directory.")?
        .to_path_buf();
    #[cfg(windows)]
    {
        if !exe.file_name().is_some_and(|name| {
            name.to_string_lossy()
                .eq_ignore_ascii_case("sniper-desktop.exe")
        }) || !directory.join("sniper-cli.exe").is_file()
        {
            return Err("Run sniper-desktop.exe from the installed app or an extracted portable ZIP containing sniper-cli.exe.".to_string());
        }
    }
    #[cfg(not(windows))]
    {
        if !should_install_cli_path(&directory) {
            return Err("Move Sniper.app to Applications before adding the CLI to PATH. Mounted disk images and translocated apps are not supported.".to_string());
        }
        if !is_executable_file(&directory.join("sniper-cli")) {
            return Err("sniper-cli is missing or not executable in this app bundle.".to_string());
        }
    }
    Ok(directory)
}

#[cfg(all(not(windows), not(target_os = "linux")))]
fn home_dir() -> Result<PathBuf, String> {
    env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is not set.".to_string())
}

#[cfg(all(not(windows), not(target_os = "linux")))]
fn shell_targets() -> Result<Vec<PathBuf>, String> {
    let home = home_dir()?;
    let zsh_dir = env::var_os("ZDOTDIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.clone());
    if !zsh_dir.is_absolute() {
        return Err("ZDOTDIR must be an absolute directory for automatic PATH setup.".to_string());
    }
    let mut targets = vec![zsh_dir.join(".zshrc")];
    for name in [".bash_profile", ".bashrc"] {
        let target = home.join(name);
        if target.exists() {
            targets.push(target);
        }
    }
    Ok(targets)
}

#[cfg(all(not(windows), not(target_os = "linux")))]
fn macos_search_dirs() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = env::var_os("PATH")
        .map(|path| env::split_paths(&path).collect())
        .unwrap_or_default();
    // Finder-launched apps do not necessarily inherit Homebrew's shell PATH.
    paths.extend([
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ]);
    paths
}

#[cfg(any(all(not(windows), not(target_os = "linux")), test))]
fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(any(all(not(windows), not(target_os = "linux")), test))]
fn managed_path_line(directory: &Path) -> Result<String, String> {
    let value = directory
        .to_str()
        .ok_or("The app path is not valid Unicode.")?;
    if value.contains([':', '\n', '\r']) {
        return Err(
            "The app path contains characters that cannot safely be added to PATH.".to_string(),
        );
    }
    Ok(format!(
        "case \":$PATH:\" in *{}*) ;; *) export PATH=\"$PATH\":{} ;; esac # Added by Sniper.app",
        shell_single_quote(&format!(":{value}:")),
        shell_single_quote(value)
    ))
}

#[cfg(any(all(not(windows), not(target_os = "linux")), test))]
fn inspect_macos(
    directory: &Path,
    targets: &[PathBuf],
    search_dirs: Vec<PathBuf>,
) -> Result<(bool, Option<String>), String> {
    let cli = std::fs::canonicalize(directory.join("sniper-cli"))
        .map_err(|e| format!("Could not read bundled CLI: {e}"))?;
    for path in search_dirs {
        let candidate = path.join("sniper-cli");
        if !is_executable_file(&candidate) {
            continue;
        }
        if std::fs::canonicalize(&candidate).ok().as_ref() == Some(&cli) {
            return Ok((true, None));
        }
        return Ok((false, Some(format!("Another sniper-cli already exists at {}. Its PATH entry was left unchanged. Resolve that conflict before adding this copy.", candidate.display()))));
    }
    let expected = managed_path_line(directory)?;
    let legacy = format!(
        "export PATH={}:$PATH # Added by Sniper.app",
        shell_single_quote(&directory.to_string_lossy())
    );
    let mut registered = !targets.is_empty();
    for target in targets {
        let contents =
            load_shell_rc_contents(target).map_err(|e| format!("{}: {e}", target.display()))?;
        registered &= contents.split_inclusive('\n').any(|line| {
            let line = normalize_managed_path_line(line);
            line == expected || line == legacy
        });
    }
    Ok((registered, None))
}

#[cfg(any(all(not(windows), not(target_os = "linux")), test))]
fn install_macos(
    directory: &Path,
    targets: &[PathBuf],
    search_dirs: Vec<PathBuf>,
) -> Result<CliPathInstall, String> {
    let (registered, conflict) = inspect_macos(directory, targets, search_dirs)?;
    if let Some(conflict) = conflict {
        return Err(conflict);
    }
    if registered {
        return Ok(CliPathInstall {
            unchanged: vec!["Existing CLI PATH registration".to_string()],
            message: "sniper-cli is already registered. Open a new terminal to use it.".to_string(),
            ..Default::default()
        });
    }
    let export_line = managed_path_line(directory)?;
    let mut result = CliPathInstall::default();
    for rc_path in targets {
        let update = (|| {
            let contents = load_shell_rc_contents(rc_path)?;
            let Some(updated) = upsert_managed_path_line(&contents, &export_line)? else {
                return Ok(false);
            };
            // Do not knowingly overwrite an edit made while the setup was preparing.
            if load_shell_rc_contents(rc_path)? != contents {
                return Err(std::io::Error::other(
                    "The profile changed during setup; retry from Settings.",
                ));
            }
            PreparedShellRc::new(rc_path, &updated, Some(&contents))?.commit()?;
            Ok::<bool, std::io::Error>(true)
        })();
        match update {
            Ok(true) => result.updated.push(rc_path.display().to_string()),
            Ok(false) => result.unchanged.push(rc_path.display().to_string()),
            Err(e) => result.warnings.push(format!("{}: {e}", rc_path.display())),
        }
    }
    if result.updated.is_empty() && result.unchanged.is_empty() {
        return Err(result.warnings.join("; "));
    }
    result.message = if result.warnings.is_empty() {
        format!(
            "Registered sniper-cli in {}. Open a new terminal to use it.",
            result
                .updated
                .iter()
                .chain(&result.unchanged)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        "CLI PATH setup only partially completed. Review the warnings and retry from Settings."
            .to_string()
    };
    Ok(result)
}

#[cfg(any(all(not(windows), not(target_os = "linux")), all(test, unix)))]
fn lock_profiles(home: &Path) -> Result<std::fs::File, String> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(home.join(".sniper-cli-path.lock"))
        .map_err(|e| format!("Could not lock CLI PATH setup: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        // The file descriptor stays open until registration finishes; close releases the lock.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err("Another CLI PATH setup is running. Retry from Settings.".to_string());
        }
    }
    Ok(file)
}

#[cfg(all(not(windows), not(target_os = "linux")))]
pub(crate) fn should_install_cli_path(macos_dir: &std::path::Path) -> bool {
    let path = macos_dir.to_string_lossy();
    if macos_dir.starts_with("/Volumes") || path.contains("/AppTranslocation/") {
        return false;
    }

    let Some(app_contents_dir) = macos_dir.parent() else {
        return false;
    };
    if macos_dir.file_name().and_then(|name| name.to_str()) != Some("MacOS")
        || app_contents_dir.file_name().and_then(|name| name.to_str()) != Some("Contents")
    {
        return false;
    }

    let Some(app_bundle) = app_contents_dir.parent() else {
        return false;
    };
    let Some(app_bundle_name) = app_bundle.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if !app_bundle_name.ends_with(".app") {
        return false;
    }

    let Some(install_dir) = app_bundle.parent() else {
        return false;
    };
    if install_dir == std::path::Path::new("/Applications") {
        return true;
    }
    match env::var("HOME") {
        Ok(home) => {
            let home = std::path::PathBuf::from(home);
            install_dir == home.join("Applications")
                || app_bundle == home.join("Desktop").join("Sniper.app")
        }
        Err(_) => false,
    }
}

#[cfg(any(all(not(windows), not(target_os = "linux")), test))]
pub(crate) fn load_shell_rc_contents(rc_path: &Path) -> std::io::Result<String> {
    match std::fs::metadata(rc_path) {
        Ok(metadata) if !metadata.is_file() => {
            return Err(std::io::Error::other(
                "Shell profile is not a regular file.",
            ))
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(error) => return Err(error),
    }
    std::fs::read_to_string(rc_path)
}

#[cfg(test)]
pub(crate) fn write_shell_rc_atomically(rc_path: &Path, contents: &str) -> std::io::Result<()> {
    PreparedShellRc::new(rc_path, contents, None)?.commit()
}

#[cfg(any(all(not(windows), not(target_os = "linux")), test))]
struct PreparedShellRc {
    rc_path: PathBuf,
    write_path: PathBuf,
    tmp_path: PathBuf,
    expected: Option<String>,
}

#[cfg(any(all(not(windows), not(target_os = "linux")), test))]
impl PreparedShellRc {
    fn new(rc_path: &Path, contents: &str, expected: Option<&str>) -> std::io::Result<Self> {
        let write_path = resolve_shell_rc_write_path(rc_path)?;
        let parent = write_path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent)?;
        let existing_permissions = match std::fs::metadata(&write_path) {
            Ok(metadata) if metadata.is_file() => Some(metadata.permissions()),
            Ok(_) => {
                return Err(std::io::Error::other(
                    "Shell profile is not a regular file.",
                ))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        let name = write_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("shellrc");
        let tmp_path = parent.join(format!(".{name}.{}.tmp", uuid::Uuid::new_v4()));
        let prepared = Self {
            rc_path: rc_path.to_path_buf(),
            write_path,
            tmp_path,
            expected: expected.map(str::to_owned),
        };
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&prepared.tmp_path)?;
        file.write_all(contents.as_bytes())?;
        if let Some(permissions) = existing_permissions {
            file.set_permissions(permissions)?;
        }
        file.sync_all()?;
        Ok(prepared)
    }

    fn commit(self) -> std::io::Result<()> {
        // Recheck after the slow write/fsync, immediately before rename. Other
        // editors do not share our flock; no portable filesystem CAS exists,
        // but a known concurrent edit or symlink move must never be overwritten.
        if resolve_shell_rc_write_path(&self.rc_path)? != self.write_path {
            return Err(std::io::Error::other(
                "The profile target changed during setup; retry from Settings.",
            ));
        }
        let current = load_shell_rc_contents(&self.rc_path)?;
        if self
            .expected
            .as_ref()
            .is_some_and(|expected| *expected != current)
        {
            return Err(std::io::Error::other(
                "The profile changed during setup; retry from Settings.",
            ));
        }
        crate::platform::rename(&self.tmp_path, &self.write_path)?;
        crate::platform::sync_directory(self.write_path.parent().unwrap_or_else(|| Path::new(".")))
    }
}

#[cfg(any(all(not(windows), not(target_os = "linux")), test))]
impl Drop for PreparedShellRc {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.tmp_path);
    }
}

#[cfg(any(all(not(windows), not(target_os = "linux")), test))]
pub(crate) fn resolve_shell_rc_write_path(rc_path: &std::path::Path) -> std::io::Result<PathBuf> {
    match std::fs::symlink_metadata(rc_path) {
        Ok(metadata) if metadata.file_type().is_symlink() => std::fs::canonicalize(rc_path),
        Ok(_) => Ok(rc_path.to_path_buf()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(rc_path.to_path_buf()),
        Err(error) => Err(error),
    }
}

#[cfg(any(all(not(windows), not(target_os = "linux")), test))]
fn decode_shell_single_quote(value: &str) -> Option<String> {
    let inner = value.strip_prefix('\'')?.strip_suffix('\'')?;
    if inner.contains('\0') {
        return None;
    }
    let decoded = inner.replace("'\\''", "\0");
    if decoded.contains('\'') {
        return None;
    }
    Some(decoded.replace('\0', "'"))
}

#[cfg(any(all(not(windows), not(target_os = "linux")), test))]
fn normalize_managed_path_line(line: &str) -> &str {
    // Only horizontal ASCII padding is ignored for ownership. A bare CR,
    // vertical whitespace, and Unicode whitespace remain meaningful bytes.
    let line = line
        .strip_suffix("\r\n")
        .or_else(|| line.strip_suffix('\n'))
        .unwrap_or(line);
    line.trim_matches([' ', '\t'])
}

#[cfg(any(all(not(windows), not(target_os = "linux")), test))]
fn is_managed_path_line(line: &str) -> bool {
    let line = normalize_managed_path_line(line);
    if let Some(quoted) = line
        .strip_prefix("export PATH=")
        .and_then(|s| s.strip_suffix(":$PATH # Added by Sniper.app"))
    {
        return decode_shell_single_quote(quoted).is_some();
    }
    let Some(quoted) = line
        .split("*) ;; *) export PATH=\"$PATH\":")
        .nth(1)
        .and_then(|s| s.strip_suffix(" ;; esac # Added by Sniper.app"))
    else {
        return false;
    };
    let Some(directory) = decode_shell_single_quote(quoted) else {
        return false;
    };
    managed_path_line(Path::new(&directory)).is_ok_and(|expected| expected == line)
}

#[cfg(any(all(not(windows), not(target_os = "linux")), test))]
pub(crate) fn upsert_managed_path_line(
    contents: &str,
    export_line: &str,
) -> std::io::Result<Option<String>> {
    let mut changed = false;
    let mut found_managed = false;
    let mut updated = String::new();
    for line in contents.split_inclusive('\n') {
        if is_managed_path_line(line) {
            if found_managed {
                changed = true;
                continue;
            }
            found_managed = true;
            changed |= normalize_managed_path_line(line) != export_line;
            updated.push_str(export_line);
            updated.push('\n');
        } else {
            // Keep unrelated profile bytes, including CRLF and comments, intact.
            updated.push_str(line);
        }
    }
    if !found_managed {
        // Appending LF would turn a previously unowned terminal CR into CRLF,
        // allowing a later update to claim and replace that original line.
        if updated.ends_with('\r') {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "The profile was left unchanged because its final line ends with a lone carriage return. Review that line manually before retrying PATH setup.",
            ));
        }
        if !updated.is_empty() {
            if !updated.ends_with('\n') {
                updated.push('\n');
            }
            updated.push('\n');
        }
        updated.push_str(export_line);
        updated.push('\n');
        changed = true;
    }
    Ok(changed.then_some(updated))
}

#[cfg(any(all(not(windows), not(target_os = "linux")), test))]
pub(crate) fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new() -> Self {
            let path = env::temp_dir().join(format!("sniper-cli-path-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn cli(&self, name: &str) -> PathBuf {
            let directory = self.0.join(name);
            std::fs::create_dir_all(&directory).unwrap();
            let path = directory.join("sniper-cli");
            std::fs::write(&path, "test fixture only").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            directory
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[cfg(target_os = "linux")]
    mod linux {
        use super::*;
        use std::os::unix::{ffi::OsStringExt, fs::PermissionsExt};

        fn status(directory: &Path, search_dirs: Vec<PathBuf>) -> CliPathStatus {
            let status = linux_cli_path_status(Ok(directory.join("sniper-desktop")), search_dirs);
            assert_eq!(status.platform, "linux");
            assert!(status.supported);
            assert!(!status.can_install);
            assert!(!status.should_prompt);
            status
        }

        #[test]
        fn linux_package_sibling_on_path_is_available() {
            let root = TempRoot::new();
            let directory = root.cli("usr/bin");
            let result = status(&directory, vec![directory.clone()]);
            assert!(result.registered);
            assert_eq!(result.directory, Some(directory.display().to_string()));
            assert!(result.message.contains("running app's PATH"));
            assert!(result
                .message
                .contains("terminal may have a different PATH"));
        }

        #[test]
        fn linux_portable_sibling_off_path_gets_manual_instructions() {
            let root = TempRoot::new();
            let directory = root.cli("portable folder/bin");
            let result = status(&directory, vec![]);
            assert!(!result.registered);
            assert!(result.message.contains("not on the running app's PATH"));
            assert!(result.message.contains("Debian package"));
            assert!(result.message.contains("portable archive"));
            assert!(result.message.contains("does not edit shell profiles"));
        }

        #[test]
        fn linux_path_precedence_reports_foreign_cli_without_hiding_it() {
            let root = TempRoot::new();
            let directory = root.cli("bundle/bin");
            let foreign = root.cli("other/bin");
            let result = status(&directory, vec![foreign.clone(), directory.clone()]);
            assert!(!result.registered);
            assert!(result.message.contains("takes precedence"));
            assert!(result.message.contains(&foreign.display().to_string()));
            assert!(status(&directory, vec![directory.clone(), foreign]).registered);
        }

        #[test]
        fn linux_path_symlinks_and_hard_links_to_sibling_are_available() {
            let root = TempRoot::new();
            let directory = root.cli("bundle/bin");
            for (name, hard_link) in [("symlink/bin", false), ("hardlink/bin", true)] {
                let link_directory = root.0.join(name);
                std::fs::create_dir_all(&link_directory).unwrap();
                let link = link_directory.join("sniper-cli");
                if hard_link {
                    std::fs::hard_link(directory.join("sniper-cli"), link).unwrap();
                } else {
                    std::os::unix::fs::symlink(directory.join("sniper-cli"), link).unwrap();
                }
                assert!(status(&directory, vec![link_directory]).registered);
            }
        }

        #[test]
        fn linux_path_ignores_missing_nonexecutable_and_nonregular_candidates() {
            let root = TempRoot::new();
            let directory = root.cli("bundle/bin");
            let nonexecutable = root.cli("nonexecutable");
            std::fs::set_permissions(
                nonexecutable.join("sniper-cli"),
                std::fs::Permissions::from_mode(0o644),
            )
            .unwrap();
            let nonregular = root.0.join("nonregular");
            std::fs::create_dir_all(nonregular.join("sniper-cli")).unwrap();
            let dangling = root.0.join("dangling");
            std::fs::create_dir_all(&dangling).unwrap();
            std::os::unix::fs::symlink(root.0.join("absent"), dangling.join("sniper-cli")).unwrap();
            let mut search_dirs = vec![root.0.join("missing"), nonexecutable, nonregular, dangling];
            assert!(!status(&directory, search_dirs.clone()).registered);
            search_dirs.push(directory.clone());
            assert!(status(&directory, search_dirs).registered);
        }

        #[test]
        fn linux_missing_or_nonexecutable_sibling_is_not_registered_by_foreign_cli() {
            let root = TempRoot::new();
            let foreign = root.cli("other/bin");
            let missing = root.0.join("missing/bin");
            let nonexecutable = root.cli("nonexecutable/bin");
            std::fs::set_permissions(
                nonexecutable.join("sniper-cli"),
                std::fs::Permissions::from_mode(0o644),
            )
            .unwrap();
            for directory in [missing, nonexecutable] {
                let result = status(&directory, vec![foreign.clone()]);
                assert!(!result.registered);
                assert!(result.message.contains("missing or not executable"));
            }
        }

        #[test]
        fn linux_nonunicode_path_is_checked_without_shell_quoting() {
            let root = TempRoot::new();
            let directory = root.cli("bundle/bin");
            let link_directory = root
                .0
                .join(std::ffi::OsString::from_vec(b"path-\xff".to_vec()));
            std::fs::create_dir(&link_directory).unwrap();
            std::os::unix::fs::symlink(
                directory.join("sniper-cli"),
                link_directory.join("sniper-cli"),
            )
            .unwrap();
            assert!(status(&directory, vec![link_directory]).registered);
        }

        #[test]
        fn linux_app_lookup_failure_is_actionable_and_never_installable() {
            let result = linux_cli_path_status(Err("fixture lookup failure".to_string()), vec![]);
            assert_eq!(result.platform, "linux");
            assert!(!result.registered);
            assert!(!result.can_install);
            assert!(!result.should_prompt);
            assert_eq!(result.directory, None);
            assert!(result.message.contains("fixture lookup failure"));
            assert!(result.message.contains("Debian package"));
        }

        #[test]
        fn linux_status_and_install_do_not_create_choice_or_profile_files() {
            let root = TempRoot::new();
            let data_dir = root.0.join("unused-data");
            let result = cli_path_status(&data_dir);
            assert_eq!(result.platform, "linux");
            assert!(!result.can_install);
            assert!(!result.should_prompt);
            assert!(install_cli_path()
                .unwrap_err()
                .contains("does not edit shell profiles"));
            assert!(!data_dir.exists());
            assert_eq!(std::fs::read_dir(&root.0).unwrap().count(), 0);
        }
    }

    #[test]
    fn profile_edit_during_preparation_is_not_overwritten() {
        let root = TempRoot::new();
        let profile = root.0.join(".zshrc");
        std::fs::write(&profile, "original").unwrap();
        let prepared = PreparedShellRc::new(&profile, "our change", Some("original")).unwrap();
        std::fs::write(&profile, "user's newer edit").unwrap();
        assert!(prepared.commit().is_err());
        assert_eq!(
            std::fs::read_to_string(&profile).unwrap(),
            "user's newer edit"
        );
        assert_eq!(std::fs::read_dir(&root.0).unwrap().count(), 1);
    }

    #[test]
    #[cfg(unix)]
    fn profile_symlink_move_during_preparation_is_not_overwritten() {
        let root = TempRoot::new();
        let profile = root.0.join(".zshrc");
        let first = root.0.join("first");
        let second = root.0.join("second");
        std::fs::write(&first, "original").unwrap();
        std::fs::write(&second, "original").unwrap();
        std::os::unix::fs::symlink(&first, &profile).unwrap();
        let prepared = PreparedShellRc::new(&profile, "our change", Some("original")).unwrap();
        std::fs::remove_file(&profile).unwrap();
        std::os::unix::fs::symlink(&second, &profile).unwrap();
        assert!(prepared.commit().is_err());
        assert_eq!(std::fs::read_to_string(first).unwrap(), "original");
        assert_eq!(std::fs::read_to_string(second).unwrap(), "original");
    }

    #[test]
    #[cfg(unix)]
    fn nonregular_profiles_are_rejected_without_reading_or_replacing() {
        use std::os::unix::ffi::OsStrExt;
        let root = TempRoot::new();
        let profile = root.0.join(".zshrc");
        let path = std::ffi::CString::new(profile.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
        assert!(load_shell_rc_contents(&profile).is_err());
        assert!(write_shell_rc_atomically(&profile, "new").is_err());
        assert!(!std::fs::metadata(profile).unwrap().is_file());
    }

    #[test]
    fn first_run_prompt_respects_setup_choice_and_registration() {
        assert!(should_offer_cli_path(true, false, false, false));
        assert!(!should_offer_cli_path(true, false, false, true));
        assert!(!should_offer_cli_path(true, false, true, false));
        assert!(!should_offer_cli_path(true, true, false, false));
        assert!(!should_offer_cli_path(false, false, false, false));
    }

    #[test]
    fn deferral_is_persistent_and_idempotent_without_shell_mutation() {
        let root = TempRoot::new();
        defer_cli_path(&root.0).unwrap();
        let first = std::fs::read(root.0.join(CHOICE_FILE)).unwrap();
        defer_cli_path(&root.0).unwrap();
        assert_eq!(std::fs::read(root.0.join(CHOICE_FILE)).unwrap(), first);
        assert_eq!(std::fs::read_dir(&root.0).unwrap().count(), 1);
    }

    #[test]
    fn deferral_failure_is_reported() {
        let root = TempRoot::new();
        let file = root.0.join("not-a-directory");
        std::fs::write(&file, "unchanged").unwrap();
        assert!(defer_cli_path(&file).is_err());
        assert_eq!(std::fs::read_to_string(file).unwrap(), "unchanged");
    }

    #[test]
    #[cfg(unix)]
    fn profile_registration_repeats_without_duplicates_and_updates_owned_paths() {
        let root = TempRoot::new();
        let directory = root.cli("Sniper's first.app");
        let target = root.0.join(".zshrc");
        std::fs::write(&target, "# keep this\r\nexport PATH=/my/bin:$PATH\r\n").unwrap();
        let first = install_macos(&directory, &[target.clone()], vec![]).unwrap();
        assert_eq!(first.updated.len(), 1);
        assert!(first.warnings.is_empty());
        let contents = std::fs::read_to_string(&target).unwrap();
        assert!(contents.starts_with("# keep this\r\nexport PATH=/my/bin:$PATH\r\n"));
        let second = install_macos(&directory, &[target.clone()], vec![]).unwrap();
        assert!(second.updated.is_empty());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), contents);
        let new_directory = root.cli("Sniper next.app");
        install_macos(&new_directory, &[target.clone()], vec![]).unwrap();
        let updated = std::fs::read_to_string(&target).unwrap();
        assert_eq!(updated.matches("# Added by Sniper.app").count(), 1);
        assert!(!updated.contains("first.app"));
        assert!(updated.contains("next.app"));
    }

    #[test]
    fn foreign_cli_blocks_registration_without_changing_profiles() {
        let root = TempRoot::new();
        let directory = root.cli("bundle");
        let foreign = root.cli("unrelated");
        let target = root.0.join(".zshrc");
        std::fs::write(&target, "original").unwrap();
        assert!(install_macos(&directory, &[target.clone()], vec![foreign]).is_err());
        assert_eq!(std::fs::read_to_string(target).unwrap(), "original");
    }

    #[test]
    fn existing_path_registration_does_not_create_a_profile() {
        let root = TempRoot::new();
        let directory = root.cli("bundle");
        let target = root.0.join(".zshrc");
        let result = install_macos(&directory, &[target.clone()], vec![directory.clone()]).unwrap();
        assert!(result.updated.is_empty());
        assert!(!target.exists());
    }

    #[test]
    fn unrelated_marker_comments_and_modified_commands_are_preserved() {
        let line = managed_path_line(Path::new("/Applications/Sniper.app/Contents/MacOS")).unwrap();
        let original = "# Added by Sniper.app is just a note\nexport PATH='x'; echo example # Added by Sniper.app\n";
        let updated = upsert_managed_path_line(original, &line).unwrap().unwrap();
        assert!(updated.starts_with(original));
        assert!(upsert_managed_path_line(&updated, &line).unwrap().is_none());
    }

    fn managed_path_line_variants(directory: &Path) -> [String; 2] {
        [
            managed_path_line(directory).unwrap(),
            format!(
                "export PATH={}:$PATH # Added by Sniper.app",
                shell_single_quote(directory.to_str().unwrap())
            ),
        ]
    }

    fn unowned_whitespace_lines(directory: &Path) -> Vec<String> {
        let mut lines = Vec::new();
        for line in managed_path_line_variants(directory) {
            for padding in ['\u{00a0}', '\u{2002}', '\u{3000}', '\u{000b}', '\u{000c}'] {
                lines.push(format!("{padding}{line}\n"));
                lines.push(format!("{line}{padding}\r\n"));
            }
            lines.extend([
                format!("\r{line}\n"),
                format!("{line}\r"),
                format!("{line}\r \n"),
                format!("{line}\r\r\n"),
            ]);
        }
        lines
    }

    #[test]
    fn profile_whitespace_recognizer_preserves_non_horizontal_padding() {
        let directory = Path::new("/Applications/Sniper '\u{00a0}\u{2002}\u{3000}'.app");
        for line in unowned_whitespace_lines(directory) {
            assert!(!is_managed_path_line(&line), "Incorrectly owned: {line:?}");
        }
        for line in managed_path_line_variants(directory) {
            for ending in ["", "\n", "\r\n"] {
                assert!(is_managed_path_line(&format!(" \t{line}\t {ending}")));
            }
            let terminal_cr = format!("{line}\r");
            let replacement = managed_path_line(Path::new("/new/Sniper.app")).unwrap();
            assert_eq!(
                upsert_managed_path_line(&terminal_cr, &replacement)
                    .unwrap_err()
                    .kind(),
                std::io::ErrorKind::InvalidData
            );
        }
    }

    #[test]
    #[cfg(unix)]
    fn profile_whitespace_status_rejects_unowned_padding() {
        let root = TempRoot::new();
        let directory = root.0.join("bundle");
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(
            directory.join("sniper-cli"),
            "inert fixture, never executed",
        )
        .unwrap();
        let target = root.0.join(".zshrc");
        let mut failures = Vec::new();
        for original in unowned_whitespace_lines(&directory) {
            std::fs::write(&target, &original).unwrap();
            let (registered, conflict) =
                inspect_macos(&directory, &[target.clone()], vec![]).unwrap();
            assert!(conflict.is_none());
            assert_eq!(std::fs::read_to_string(&target).unwrap(), original);
            if registered || is_managed_path_line(&original) {
                failures.push(format!("{original:?}"));
            }
        }
        assert!(failures.is_empty(), "Incorrectly owned: {failures:#?}");
    }

    #[test]
    #[cfg(unix)]
    fn profile_whitespace_install_preserves_unowned_padding() {
        let root = TempRoot::new();
        let directory = root.0.join("bundle");
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(
            directory.join("sniper-cli"),
            "inert fixture, never executed",
        )
        .unwrap();
        let target = root.0.join(".zshrc");
        let expected_line = managed_path_line(&directory).unwrap();
        let mut failures = Vec::new();
        for original in unowned_whitespace_lines(&root.0.join("old bundle"))
            .into_iter()
            .filter(|line| !line.ends_with('\r'))
        {
            std::fs::write(&target, &original).unwrap();
            let separator = if original.ends_with('\n') {
                "\n"
            } else {
                "\n\n"
            };
            let expected = format!("{original}{separator}{expected_line}\n");
            let updated = upsert_managed_path_line(&original, &expected_line)
                .unwrap()
                .unwrap();
            let installed = install_macos(&directory, &[target.clone()], vec![]).unwrap();
            assert_eq!(installed.updated.len(), 1);
            assert!(installed.warnings.is_empty());
            let after = std::fs::read_to_string(&target).unwrap();
            if updated != expected || after != expected {
                failures.push(format!("{original:?}"));
            }
            for _ in 0..2 {
                assert_eq!(
                    inspect_macos(&directory, &[target.clone()], vec![]).unwrap(),
                    (true, None)
                );
                let repeated = install_macos(&directory, &[target.clone()], vec![]).unwrap();
                assert!(repeated.updated.is_empty());
                assert!(repeated.warnings.is_empty());
                assert_eq!(std::fs::read_to_string(&target).unwrap(), after);
            }
        }
        assert!(
            failures.is_empty(),
            "Rewritten unowned lines: {failures:#?}"
        );
    }

    #[test]
    #[cfg(unix)]
    fn profile_whitespace_accepts_ascii_padding_and_physical_line_endings() {
        let root = TempRoot::new();
        let directory = root.0.join("Sniper 'quoted'\u{00a0}\u{2002}\u{3000}.app");
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(
            directory.join("sniper-cli"),
            "inert fixture, never executed",
        )
        .unwrap();
        let target = root.0.join(".zshrc");
        for line in managed_path_line_variants(&directory) {
            for (prefix, suffix) in [
                ("", ""),
                ("", "\n"),
                ("", "\r\n"),
                (" \t", "\t "),
                (" \t", "\t \n"),
                (" \t", "\t \r\n"),
            ] {
                let original = format!("{prefix}{line}{suffix}");
                assert!(is_managed_path_line(&original));
                assert!(upsert_managed_path_line(&original, &line)
                    .unwrap()
                    .is_none());
                std::fs::write(&target, &original).unwrap();
                for _ in 0..2 {
                    assert_eq!(
                        inspect_macos(&directory, &[target.clone()], vec![]).unwrap(),
                        (true, None)
                    );
                    let result = install_macos(&directory, &[target.clone()], vec![]).unwrap();
                    assert!(result.updated.is_empty());
                    assert!(result.warnings.is_empty());
                    assert_eq!(std::fs::read_to_string(&target).unwrap(), original);
                }
            }
        }
    }

    #[test]
    #[cfg(unix)]
    fn profile_whitespace_terminal_cr_append_is_refused_without_mutation() {
        let root = TempRoot::new();
        let current = root.0.join("current bundle");
        let next = root.0.join("next bundle");
        for directory in [&current, &next] {
            std::fs::create_dir(directory).unwrap();
            std::fs::write(directory.join("sniper-cli"), "inert fixture").unwrap();
        }
        let target = root.0.join(".zshrc");
        let mut originals: Vec<_> = managed_path_line_variants(&root.0.join("old bundle"))
            .into_iter()
            .map(|line| format!("{line}\r"))
            .collect();
        originals.push("# unrelated terminal CR\r".to_string());
        for original in originals {
            std::fs::write(&target, &original).unwrap();
            for directory in [&current, &current, &next] {
                assert_eq!(
                    inspect_macos(directory, &[target.clone()], vec![]).unwrap(),
                    (false, None)
                );
                let line = managed_path_line(directory).unwrap();
                let error = upsert_managed_path_line(&original, &line).unwrap_err();
                assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
                let error = install_macos(directory, &[target.clone()], vec![]).unwrap_err();
                assert!(error.contains("lone carriage return"));
                assert_eq!(std::fs::read_to_string(&target).unwrap(), original);
            }
        }
    }

    #[test]
    #[cfg(unix)]
    fn profile_whitespace_terminal_cr_append_refusal_is_profile_local() {
        let root = TempRoot::new();
        let directory = root.0.join("bundle");
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(directory.join("sniper-cli"), "inert fixture").unwrap();
        let supported = root.0.join(".zshrc");
        let refused = root.0.join(".bashrc");
        let original = "# unrelated terminal CR\r";
        std::fs::write(&refused, original).unwrap();
        let targets = [supported.clone(), refused.clone()];
        let installed = install_macos(&directory, &targets, vec![]).unwrap();
        assert_eq!(installed.updated, [supported.display().to_string()]);
        assert_eq!(installed.warnings.len(), 1);
        assert!(installed.warnings[0].contains("lone carriage return"));
        assert_eq!(std::fs::read_to_string(&refused).unwrap(), original);
        let supported_contents = std::fs::read_to_string(&supported).unwrap();
        let repeated = install_macos(&directory, &targets, vec![]).unwrap();
        assert!(repeated.updated.is_empty());
        assert_eq!(repeated.unchanged, [supported.display().to_string()]);
        assert_eq!(repeated.warnings.len(), 1);
        assert_eq!(std::fs::read_to_string(&refused).unwrap(), original);
        assert_eq!(
            std::fs::read_to_string(&supported).unwrap(),
            supported_contents
        );
    }

    #[test]
    fn profile_whitespace_terminal_cr_does_not_block_owned_line_updates() {
        let current = managed_path_line(Path::new("/current/Sniper.app")).unwrap();
        let tail = "# unrelated terminal CR\r";
        let unchanged = format!("{current}\n{tail}");
        assert!(upsert_managed_path_line(&unchanged, &current)
            .unwrap()
            .is_none());
        for old in managed_path_line_variants(Path::new("/old/Sniper.app")) {
            let original = format!("{old}\n{tail}");
            let updated = upsert_managed_path_line(&original, &current)
                .unwrap()
                .unwrap();
            assert_eq!(updated, unchanged);
            assert!(upsert_managed_path_line(&updated, &current)
                .unwrap()
                .is_none());
        }
    }

    #[test]
    fn profile_whitespace_mixed_forms_only_replace_owned_lines() {
        let [old_canonical, old_legacy] =
            managed_path_line_variants(Path::new("/old/Sniper '\u{00a0}\u{2002}\u{3000}'.app"));
        let line =
            managed_path_line(Path::new("/new/Sniper '\u{00a0}\u{2002}\u{3000}'.app")).unwrap();
        let unowned = format!("\u{00a0}{old_canonical}\r\n\u{2002}{old_legacy}\n");
        let original = format!(
            "# keep Unicode\u{3000}comment\r\n{unowned} \t{old_canonical}\t \r\n# middle\r\n\t{old_legacy} \n# no final newline\u{00a0}"
        );
        let updated = upsert_managed_path_line(&original, &line).unwrap().unwrap();
        assert_eq!(
            updated,
            format!(
                "# keep Unicode\u{3000}comment\r\n{unowned}{line}\n# middle\r\n# no final newline\u{00a0}"
            )
        );
        assert!(upsert_managed_path_line(&updated, &line).unwrap().is_none());
    }

    #[test]
    fn malformed_paths_cannot_be_written_as_shell_path_entries() {
        for path in ["/tmp/a:b", "/tmp/a\nb", "/tmp/a\rb"] {
            assert!(managed_path_line(Path::new(path)).is_err());
        }
    }

    #[test]
    #[cfg(unix)]
    fn generated_shell_line_preserves_precedence_and_is_safe_to_source_twice() {
        let directory = Path::new("/tmp/Sniper 'quoted' $(literal).app/Contents/MacOS");
        let line = managed_path_line(directory).unwrap();
        let output = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(format!(
                "PATH=/existing/bin; {line}\n{line}\nprintf '%s' \"$PATH\""
            ))
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!("/existing/bin:{}", directory.display())
        );
    }

    #[test]
    #[cfg(unix)]
    fn profile_lock_rejects_concurrent_registration_then_releases() {
        let root = TempRoot::new();
        let first = lock_profiles(&root.0).unwrap();
        assert!(lock_profiles(&root.0).is_err());
        drop(first);
        assert!(lock_profiles(&root.0).is_ok());
    }

    #[test]
    #[cfg(unix)]
    fn symlink_chains_and_permissions_are_preserved() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let root = TempRoot::new();
        let target = root.0.join("real-profile");
        let intermediate = root.0.join("intermediate");
        let profile = root.0.join(".zshrc");
        std::fs::write(&target, "old").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640)).unwrap();
        symlink(&target, &intermediate).unwrap();
        symlink(&intermediate, &profile).unwrap();
        write_shell_rc_atomically(&profile, "new").unwrap();
        assert!(std::fs::symlink_metadata(&profile).unwrap().is_symlink());
        assert!(std::fs::symlink_metadata(&intermediate)
            .unwrap()
            .is_symlink());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
        assert_eq!(
            std::fs::metadata(target).unwrap().permissions().mode() & 0o777,
            0o640
        );
    }

    #[test]
    #[cfg(unix)]
    fn dangling_profile_links_fail_without_replacing_the_link() {
        let root = TempRoot::new();
        let profile = root.0.join(".zshrc");
        std::os::unix::fs::symlink("missing", &profile).unwrap();
        assert!(write_shell_rc_atomically(&profile, "new").is_err());
        assert!(std::fs::symlink_metadata(&profile).unwrap().is_symlink());
        assert!(!root.0.join("missing").exists());
    }

    #[test]
    fn shell_single_quote_escapes_embedded_quotes() {
        assert_eq!(
            shell_single_quote("/Applications/Sniper 'Beta'.app/Contents/MacOS"),
            "'/Applications/Sniper '\\''Beta'\\''.app/Contents/MacOS'"
        );
    }

    #[test]
    fn upsert_managed_path_line_replaces_old_managed_line() {
        let updated = upsert_managed_path_line(
            "export PATH='/old/Sniper.app/Contents/MacOS':$PATH # Added by Sniper.app\n",
            "export PATH='/new/Sniper.app/Contents/MacOS':$PATH # Added by Sniper.app",
        )
        .unwrap()
        .unwrap();
        assert!(updated.contains("/new/Sniper.app"));
        assert!(!updated.contains("/old/Sniper.app"));
    }

    #[test]
    fn upsert_managed_path_line_collapses_duplicate_managed_lines() {
        let updated = upsert_managed_path_line(
            "before\nexport PATH='/old/Sniper.app/Contents/MacOS':$PATH # Added by Sniper.app\nmiddle\nexport PATH='/older/Sniper.app/Contents/MacOS':$PATH # Added by Sniper.app\nafter\n",
            "export PATH='/new/Sniper.app/Contents/MacOS':$PATH # Added by Sniper.app",
        )
        .unwrap()
        .unwrap();

        assert_eq!(updated.matches("# Added by Sniper.app").count(), 1);
        assert!(updated.contains("before\n"));
        assert!(updated.contains("middle\n"));
        assert!(updated.contains("after\n"));
        assert!(updated.contains("/new/Sniper.app"));
        assert!(!updated.contains("/old/Sniper.app"));
        assert!(!updated.contains("/older/Sniper.app"));
    }

    #[test]
    fn shell_rc_loader_only_defaults_missing_files() {
        let root = std::env::temp_dir().join(format!("sniper-rc-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(load_shell_rc_contents(&root.join(".zshrc")).unwrap(), "");

        let invalid_utf8 = root.join(".bashrc");
        std::fs::write(&invalid_utf8, [0xff, 0xfe]).unwrap();
        assert!(load_shell_rc_contents(&invalid_utf8).is_err());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn shell_rc_writer_replaces_file_atomically() {
        let root =
            std::env::temp_dir().join(format!("sniper-rc-write-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let rc_path = root.join(".zshrc");
        std::fs::write(&rc_path, "old\n").unwrap();

        write_shell_rc_atomically(&rc_path, "new\n").unwrap();

        assert_eq!(std::fs::read_to_string(&rc_path).unwrap(), "new\n");
        assert!(!std::fs::read_dir(&root).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".tmp")));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    #[cfg(unix)]
    fn shell_rc_writer_preserves_symlinked_rc_files() {
        let root =
            std::env::temp_dir().join(format!("sniper-rc-symlink-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let target_path = root.join("dotfiles").join("zshrc");
        std::fs::create_dir_all(target_path.parent().unwrap()).unwrap();
        std::fs::write(&target_path, "old\n").unwrap();
        let rc_path = root.join(".zshrc");
        std::os::unix::fs::symlink("dotfiles/zshrc", &rc_path).unwrap();

        write_shell_rc_atomically(&rc_path, "new\n").unwrap();

        assert!(std::fs::symlink_metadata(&rc_path)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(std::fs::read_to_string(&target_path).unwrap(), "new\n");

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn cli_path_install_skips_transient_dmg_mounts() {
        assert!(!should_install_cli_path(std::path::Path::new(
            "/Volumes/Sniper/Sniper.app/Contents/MacOS",
        )));
        assert!(!should_install_cli_path(std::path::Path::new(
            "/private/var/folders/xx/AppTranslocation/123/Sniper.app/Contents/MacOS",
        )));
        assert!(!should_install_cli_path(std::path::Path::new(
            "/Users/kakao/Desktop/git/Sniper/target/release",
        )));
        assert!(!should_install_cli_path(std::path::Path::new(
            "/tmp/sniper-build/release/Sniper.app/Contents/MacOS",
        )));
        assert!(!should_install_cli_path(std::path::Path::new(
            "/Users/kakao/Desktop/git/Sniper/dist/Sniper.app/Contents/MacOS",
        )));
        assert!(should_install_cli_path(
            &std::path::PathBuf::from(std::env::var("HOME").unwrap())
                .join("Desktop")
                .join("Sniper.app")
                .join("Contents")
                .join("MacOS")
        ));
        assert!(should_install_cli_path(std::path::Path::new(
            "/Applications/Sniper.app/Contents/MacOS",
        )));
        let user_app = std::path::PathBuf::from(std::env::var("HOME").unwrap())
            .join("Applications")
            .join("Sniper.app")
            .join("Contents")
            .join("MacOS");
        assert!(should_install_cli_path(&user_app));
    }
}
