use core::fmt;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::{env, io, path};

use flate2::read::GzDecoder;
use reqwest;
use semver::Version;
use serde::{Deserialize, Serialize};
use tar::Archive;
use tempfile::NamedTempFile;
use which::which;

use crate::error::Error;
use crate::regex101;
use crate::utils::vale_arch;

const RELEASES: &str = "https://github.com/vale-cli/vale/releases/download";
const LATEST: &str = "https://api.github.com/repos/vale-cli/vale/releases/latest";

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct ValeConfig {
    /// Every directory Vale searches for styles, ordered from most general
    /// (e.g., the global styles directory) to most specific (the project's
    /// own `StylesPath`).
    #[serde(default)]
    pub paths: Vec<PathBuf>,
    /// The project's `StylesPath`, as reported by older versions of Vale that
    /// predate `Paths`.
    #[serde(default)]
    pub styles_path: Option<PathBuf>,
    /// The vocabularies the project has active, in the order they're listed.
    #[serde(default)]
    pub vocab: Option<Vec<String>>,
}

impl ValeConfig {
    /// The directories to search for styles, vocabularies, and rules.
    pub fn styles_paths(&self) -> Vec<PathBuf> {
        if !self.paths.is_empty() {
            return self.paths.clone();
        }
        self.styles_path.clone().into_iter().collect()
    }

    /// The active vocabularies, which a term can be added to.
    pub fn vocabs(&self) -> Vec<String> {
        self.vocab.clone().unwrap_or_default()
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct CompiledRule {
    pub pattern: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct ValeError {
    pub path: String,
    pub text: String,
    pub line: u32,
    pub span: u32,
}

impl fmt::Display for ValeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}: {}",
            self.path, self.line, self.span, self.text
        )
    }
}

#[derive(Deserialize, Debug)]
pub(crate) struct Release {
    tag_name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct ValeAction {
    #[serde(rename = "Name")]
    pub name: Option<String>,
    #[serde(rename = "Params")]
    pub params: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ValeFix {
    pub suggestions: Vec<String>,
    pub error: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct ValeAlert {
    #[serde(rename = "Action")]
    pub action: ValeAction,
    #[serde(rename = "Check")]
    pub check: String,
    #[serde(rename = "Match")]
    pub matched: String,
    #[serde(rename = "Description")]
    pub description: String,
    #[serde(rename = "Link")]
    pub link: String,
    #[serde(rename = "Line")]
    pub line: usize,
    #[serde(rename = "Span")]
    pub span: (usize, usize),
    #[serde(rename = "Severity")]
    pub severity: String,
    #[serde(rename = "Message")]
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct ValeManager {
    pub managed_exe: PathBuf,
    pub managed_bin: PathBuf,

    pub args: Vec<String>,
    pub arch: String,

    pub fallback_exe: PathBuf,
    pub custom_exe: Option<PathBuf>,
}

// ValeManager manages the installation and execution of Vale.
//
// ValeManager is responsible for downloading and installing Vale, as well as
// running Vale and parsing its output.
impl ValeManager {
    // `new` creates a new ValeManager.
    //
    // The ValeManager will attempt to use the managed version of Vale, but
    // will fall back to the system version if it's not available.
    pub fn new() -> ValeManager {
        ValeManager::with_custom_exe(None)
    }

    /// `with_custom_exe` pins the manager to a specific Vale binary.
    ///
    /// A pinned binary is never substituted: a caller that asks for its own
    /// installation needs a missing binary to be an error rather than a
    /// silent fall back to a downloaded or `PATH` copy.
    pub fn with_custom_exe(custom_exe: Option<PathBuf>) -> ValeManager {
        let arch = vale_arch();

        let fallback = which("vale").unwrap_or(PathBuf::from(""));
        let mut bin_dir = match env::current_exe() {
            Ok(exe_path) => exe_path.parent().unwrap().to_path_buf(),
            Err(_) => PathBuf::from(""),
        };

        let mut exe = "vale".to_string();
        if arch.to_lowercase().contains("windows") {
            exe += ".exe";
        }

        bin_dir.push(path::Path::new("vale_bin"));
        ValeManager {
            managed_bin: bin_dir.clone(),
            managed_exe: bin_dir.join(path::Path::new(&exe)),
            args: vec!["--output=JSON".to_string()],
            arch,
            fallback_exe: fallback,
            custom_exe,
        }
    }

    /// `pinned_to` returns a copy of this manager bound to `custom_exe`.
    ///
    /// The client can change the configured binary at any point, and probing
    /// `PATH` again on every request to rebuild the manager isn't worth it.
    pub fn pinned_to(&self, custom_exe: Option<PathBuf>) -> ValeManager {
        ValeManager {
            custom_exe,
            ..self.clone()
        }
    }

    pub(crate) fn is_installed(&self) -> bool {
        match &self.custom_exe {
            Some(exe) => exe.is_file(),
            None => self.managed_exe.exists() || self.fallback_exe.exists(),
        }
    }

    /// `resolve_cwd` returns the directory to run Vale from, if we have one.
    ///
    /// LSP clients aren't required to send `rootUri`, so our idea of the
    /// workspace root may be empty. Handing that to `current_dir` fails the
    /// spawn with `ENOENT` before Vale ever runs, so we inherit our own
    /// working directory instead.
    fn resolve_cwd(cwd: &str) -> Option<PathBuf> {
        let path = PathBuf::from(cwd);
        if path.is_dir() {
            Some(path)
        } else {
            None
        }
    }

    /// `install_or_update` checks if Vale is installed and, if so, checks if it's
    /// the latest version.
    pub(crate) fn install_or_update(&self) -> Result<String, Error> {
        if let Some(exe) = &self.custom_exe {
            // Managing a binary we've been told not to use would download it
            // for nothing -- and, worse, imply we might run it.
            return Ok(format!("Using {}; skipping install.", exe.display()));
        }

        let newer = self.newer_version()?;
        if newer.is_some() {
            let v = newer.unwrap();
            self.install(&self.managed_bin, &v, &self.arch)?;
            Ok(format!("Vale v{} installed.", v))
        } else {
            Ok("Vale is up to date.".to_string())
        }
    }

    /// `run_buffer` lints text that hasn't been written to disk yet.
    ///
    /// Vale reads the buffer from stdin but still resolves configuration and
    /// syntax from `--path`, so the results match what linting the saved file
    /// would produce.
    pub(crate) fn run_buffer(
        &self,
        text: String,
        fp: PathBuf,
        config_path: String,
        filter: String,
    ) -> Result<HashMap<String, Vec<ValeAlert>>, Error> {
        let mut args = self.args.clone();

        if config_path != "" {
            args.push(format!("--config={}", config_path));
        }
        if filter != "" {
            args.push(format!("--filter={}", filter));
        }
        args.push(format!("--path={}", fp.as_path().display()));

        let exe = self.exe_path(false)?;
        let mut cmd = Command::new(exe.as_os_str());
        if let Some(dir) = fp
            .parent()
            .and_then(|p| Self::resolve_cwd(&p.to_string_lossy()))
        {
            cmd.current_dir(dir);
        }

        let mut child = cmd
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        child
            .stdin
            .take()
            .ok_or_else(|| Error::from("Failed to open Vale's stdin."))?
            .write_all(text.as_bytes())?;

        self.parse_output(child.wait_with_output()?)
    }

    /// `run` executes Vale over a file on disk.
    ///
    /// If `filter` is not empty, it will be passed to Vale as `--filter`.
    pub(crate) fn run(
        &self,
        fp: PathBuf,
        config_path: String,
        filter: String,
    ) -> Result<HashMap<String, Vec<ValeAlert>>, Error> {
        let mut args = self.args.clone();
        let cwd = fp.parent().unwrap();

        if config_path != "" {
            args.push(format!("--config={}", config_path));
        }
        if filter != "" {
            args.push(format!("--filter={}", filter));
        }
        args.push(fp.as_path().display().to_string());

        let exe = self.exe_path(false)?;
        let out = Command::new(exe.as_os_str())
            .current_dir(cwd)
            .args(args)
            .output()?;

        self.parse_output(out)
    }

    pub(crate) fn version(&self, managed: bool) -> Result<String, Error> {
        let exe = self.exe_path(managed)?;
        let out = Command::new(exe.as_os_str()).arg("-v").output()?;
        let buf = String::from_utf8(out.stdout)?;

        let v = buf
            .trim()
            .strip_prefix("vale version ")
            .unwrap()
            .to_string();

        Ok(v)
    }

    pub(crate) fn sync(&self, config_path: String, cwd: String) -> Result<(), Error> {
        let mut args = vec![];
        if config_path != "" {
            args.push(format!("--config={}", config_path));
        }
        args.push("sync".to_string());

        let exe = self.exe_path(false)?;
        let mut cmd = Command::new(exe.as_os_str());
        if let Some(dir) = Self::resolve_cwd(&cwd) {
            cmd.current_dir(dir);
        }

        // NOTE: Calling `status` causes the server to crash?
        let _ = cmd.args(args).output()?;

        Ok(())
    }

    pub(crate) fn config(&self, config_path: String, cwd: String) -> Result<ValeConfig, Error> {
        let mut args = vec![];
        if config_path != "" {
            args.push(format!("--config={}", config_path));
        }
        args.push("ls-config".to_string());

        let exe = self.exe_path(false)?;
        let mut cmd = Command::new(exe.as_os_str());
        if let Some(dir) = Self::resolve_cwd(&cwd) {
            cmd.current_dir(dir);
        }

        let out = cmd.args(args).output()?;

        let config: ValeConfig = serde_json::from_slice(&out.stdout)?;
        Ok(config)
    }

    /// `metrics` reports the given file's internal metrics -- word count,
    /// sentence count, and the counts the readability formulas are built on.
    pub(crate) fn metrics(
        &self,
        fp: PathBuf,
        config_path: String,
    ) -> Result<serde_json::Map<String, serde_json::Value>, Error> {
        let mut args = vec![];
        if config_path != "" {
            args.push(format!("--config={}", config_path));
        }
        args.push("ls-metrics".to_string());
        args.push(fp.as_path().display().to_string());

        let exe = self.exe_path(false)?;
        let mut cmd = Command::new(exe.as_os_str());
        if let Some(dir) = fp
            .parent()
            .and_then(|p| Self::resolve_cwd(&p.to_string_lossy()))
        {
            cmd.current_dir(dir);
        }

        let out = cmd.args(args).output()?;
        let metrics = serde_json::from_slice(&out.stdout)?;

        Ok(metrics)
    }

    pub(crate) fn fix(&self, alert: &str) -> Result<ValeFix, Error> {
        let mut file = NamedTempFile::new()?;
        file.write_all(alert.as_bytes())?;

        let exe = self.exe_path(false)?;
        let out = Command::new(exe.as_os_str())
            .arg("fix")
            .arg(file.path())
            .output()?;
        let buf = String::from_utf8(out.stdout)?;

        let fix: ValeFix = serde_json::from_str(&buf)?;
        Ok(fix)
    }

    pub(crate) fn upload_rule(
        &self,
        config_path: String,
        cwd: String,
        rule: String,
    ) -> Result<regex101::Regex101Session, Error> {
        let rule = self.compile(config_path, cwd.clone(), rule)?;
        let session = regex101::upload(rule.pattern)?;
        Ok(session)
    }

    fn compile(
        &self,
        config_path: String,
        cwd: String,
        rule: String,
    ) -> Result<CompiledRule, Error> {
        let mut args = vec![];

        if config_path != "" {
            args.push(format!("--config={}", config_path));
        }

        args.push("compile".to_string());
        args.push(rule);

        let exe = self.exe_path(false)?;
        let mut cmd = Command::new(exe.as_os_str());
        if let Some(dir) = Self::resolve_cwd(&cwd) {
            cmd.current_dir(dir);
        }

        let compiled = cmd.args(args).output()?;

        let buf = String::from_utf8(compiled.stdout)?;
        let rule: CompiledRule = serde_json::from_str(&buf)?;

        Ok(rule)
    }

    fn exe_path(&self, managed: bool) -> Result<PathBuf, Error> {
        if let Some(exe) = &self.custom_exe {
            if exe.is_file() {
                return Ok(exe.clone());
            }
            return Err(Error::from(format!(
                "Configured Vale binary not found: {}",
                exe.display()
            )));
        }

        if self.managed_exe.exists() {
            return Ok(self.managed_exe.clone());
        } else if self.fallback_exe.exists() && !managed {
            return Ok(self.fallback_exe.clone());
        }
        Err(Error::from("Vale is not installed."))
    }

    fn newer_version(&self) -> Result<Option<String>, Error> {
        let latest = self.fetch_version()?;
        match self.version(true) {
            Ok(current) => {
                let v1 = Version::parse(&current)?;
                let v2 = Version::parse(&latest)?;
                if v2 != v1 {
                    Ok(Some(latest))
                } else {
                    Ok(None)
                }
            }
            Err(_) => Ok(Some(latest)),
        }
    }

    /// `parse_output` takes the output of Vale and returns a `HashMap` of
    /// `ValeAlert`s.
    fn parse_output(&self, output: Output) -> Result<HashMap<String, Vec<ValeAlert>>, Error> {
        let stdout = String::from_utf8(output.stdout)?;
        let stderr = String::from_utf8(output.stderr)?;

        if !stdout.is_empty() {
            let results: HashMap<String, Vec<ValeAlert>> = serde_json::from_str(&stdout)?;
            return Ok(results);
        }

        Err(Error::Msg(stderr))
    }

    /// `fetch_version` returns the latest version of Vale.
    fn fetch_version(&self) -> Result<String, Error> {
        let client = reqwest::blocking::Client::builder()
            .user_agent("vale-ls")
            .build()?;

        let resp = client.get(LATEST).send()?;
        let info: Release = resp.json()?;

        let tag = info.tag_name.strip_prefix("v").unwrap().to_string();
        Ok(tag)
    }

    /// `install` downloads the latest version of Vale and extracts it to the
    /// specified path.
    ///
    /// # Arguments
    ///
    /// * `path` - A path to the directory where Vale should be installed.
    /// * `version` - A string representing the version to be installed.
    /// * `arch` - A string representing the architecture to be installed.
    fn install(&self, path: &Path, v: &str, arch: &str) -> Result<(), Error> {
        let mut asset = format!("/v{}/vale_{}_{}.tar.gz", v, v, arch);
        if arch.to_lowercase().contains("windows") {
            asset = format!("/v{}/vale_{}_{}.zip", v, v, arch);
        }
        let url = format!("{}{}", RELEASES, asset);

        let resp = reqwest::blocking::get(url)?.bytes()?;
        let archive = resp.to_vec();

        let buf = io::Cursor::new(archive);
        if asset.ends_with(".zip") {
            zip_extract::extract(buf, path, true)?;
        } else {
            Archive::new(GzDecoder::new(buf)).unpack(path)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version() {
        let mgr = ValeManager::new();

        let out = mgr.newer_version().unwrap();
        assert!(out.is_some());

        let v1 = Version::parse(&out.unwrap()).unwrap();
        assert!(v1 >= Version::parse("2.0.0").unwrap());

        let v2 = Version::parse(&mgr.fetch_version().unwrap()).unwrap();
        assert!(v2 >= Version::parse("2.0.0").unwrap());
    }

    #[test]
    fn custom_exe_is_never_substituted() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("vale");
        std::fs::write(&exe, "").unwrap();

        let mgr = ValeManager::new().pinned_to(Some(exe.clone()));
        assert!(mgr.is_installed());
        assert_eq!(mgr.exe_path(false).unwrap(), exe);

        // A pinned binary that isn't there is an error, not a fall back to
        // the managed or `PATH` copy.
        let missing = dir.path().join("nope");
        let mgr = ValeManager::new().pinned_to(Some(missing.clone()));

        assert!(!mgr.is_installed());
        assert!(mgr.exe_path(false).is_err());
        assert!(mgr
            .exe_path(false)
            .unwrap_err()
            .to_string()
            .contains(missing.to_str().unwrap()));

        // ...and we don't download one it was told not to use.
        assert!(mgr
            .install_or_update()
            .unwrap()
            .contains("skipping install"));
    }

    #[test]
    fn cwd_falls_back_to_ours() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(
            ValeManager::resolve_cwd(dir.path().to_str().unwrap()),
            Some(dir.path().to_path_buf())
        );

        // A client that doesn't send `rootUri` leaves us without a root.
        assert_eq!(ValeManager::resolve_cwd(""), None);
        assert_eq!(ValeManager::resolve_cwd("does-not-exist"), None);
    }

    #[test]
    fn config_paths() {
        let cfg: ValeConfig = serde_json::from_str(
            r#"{"RootINI": "/p/.vale.ini", "Paths": ["/global", "/p/styles"]}"#,
        )
        .unwrap();

        assert_eq!(
            cfg.styles_paths(),
            vec![PathBuf::from("/global"), PathBuf::from("/p/styles")]
        );
    }

    #[test]
    fn config_legacy_styles_path() {
        let cfg: ValeConfig = serde_json::from_str(r#"{"StylesPath": "/p/styles"}"#).unwrap();
        assert_eq!(cfg.styles_paths(), vec![PathBuf::from("/p/styles")]);
    }

    #[test]
    fn config_without_styles() {
        let cfg: ValeConfig = serde_json::from_str("{}").unwrap();
        assert!(cfg.styles_paths().is_empty());
    }
}
