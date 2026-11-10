// SPDX-License-Identifier: GPL-3.0-or-later
//! Starts fefw.exe in the run folder, watches it, and reports how it ended.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter, Manager};

/// Files the launcher may open for the player, by name, inside the run folder (the runtime writes the reports next to the log).
pub const REPORT_FILES: [&str; 4] = ["fefw_crash.txt", "fefw_freeze.txt", "fefw_fatal.txt", "fefw.log"];

#[derive(Serialize, Clone, Default)]
pub struct ReportFile {
    pub name: String,
    /// written since this run started
    pub fresh: bool,
}

#[derive(Serialize, Clone)]
pub struct Status {
    /// "idle", "running" or "exited"
    pub state: String,
    pub code: Option<i32>,
    pub crashed: bool,
    pub files: Vec<ReportFile>,
    pub message: String,
    pub modded: bool,
}

impl Default for Status {
    fn default() -> Self {
        Status { state: "idle".into(), code: None, crashed: false, files: vec![], message: String::new(), modded: false }
    }
}

#[derive(Default)]
pub struct PlayState(pub Mutex<Status>, pub Mutex<Option<PathBuf>>);

fn set_status(app: &AppHandle, s: Status) {
    *app.state::<PlayState>().0.lock().unwrap() = s.clone();
    let _ = app.emit("game-status", s);
}

fn report_files(dir: &Path, started: SystemTime) -> Vec<ReportFile> {
    REPORT_FILES
        .iter()
        .filter_map(|n| {
            let md = std::fs::metadata(dir.join(n)).ok()?;
            let fresh = md.modified().map(|m| m + Duration::from_secs(2) >= started).unwrap_or(false);
            Some(ReportFile { name: n.to_string(), fresh })
        })
        .collect()
}

pub fn start(app: &AppHandle, paths: &crate::setup::Paths, local: Option<crate::code_mod::Launch>, minimize: bool) -> Result<(), String> {
    let (exe, dir) = match &local {
        Some(local) => (local.exe.clone(), local.dir.clone()),
        None => (paths.exe(), paths.game_dir.clone()),
    };
    if !exe.is_file() {
        return Err(format!("fefw.exe was not found at {}. Run Set up first.", crate::dump::fwd(&exe)));
    }
    let modded = local.is_some();
    let initial = Status { state: "running".into(), message: "Starting fefw.exe...".into(), modded, ..Default::default() };
    let state = app.state::<PlayState>();
    {
        let mut status = state.0.lock().unwrap();
        if status.state == "running" {
            return Err("The game is already running.".into());
        }
        *state.1.lock().unwrap() = Some(dir.clone());
        *status = initial.clone();
    }
    let _ = app.emit("game-status", initial);
    let app2 = app.clone();
    let paths = paths.clone();
    std::thread::spawn(move || {
        let mut cmd = Command::new(&exe);
        // fefw.exe reads fefw.ini from its working directory: every path comes from there
        cmd.current_dir(&dir);
        if let Some(local) = local {
            local.configure(&mut cmd, &paths);
        }
        // the upscaler DLLs ship next to the launcher (upscalers/fsr, upscalers/streamline), which need not be the run folder
        if let Some(d) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
            cmd.env("NXRT_LAUNCHER_DIR", d);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no stray console
        }
        let started = SystemTime::now();
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                set_status(&app2, Status { state: "exited".into(), crashed: true, message: format!("Could not start fefw.exe: {e}"), modded, ..Default::default() });
                return;
            }
        };
        set_status(&app2, Status { state: "running".into(), message: format!("fefw.exe started (process {})", child.id()), modded, ..Default::default() });
        if minimize {
            if let Some(w) = app2.get_webview_window("main") {
                let _ = w.minimize();
            }
        }
        let (code, wait_err) = match child.wait() {
            Ok(s) => (s.code(), None),
            Err(e) => (None, Some(e.to_string())),
        };
        let files = report_files(&dir, started);
        let crash_file = files.iter().any(|f| f.fresh && f.name != "fefw.log");
        let crashed = crash_file || code.map_or(true, |c| c != 0);
        let message = match (&wait_err, code) {
            (Some(e), _) => format!("Lost track of fefw.exe: {e}"),
            (None, Some(0)) => "fefw.exe exited normally.".to_string(),
            (None, Some(4)) => "fefw.exe stopped before the game started: this processor lacks FMA3, AVX or CMPXCHG16B, which the game needs (an Intel Haswell or AMD Piledriver, 2012-2013, or newer). fefw_fatal.txt says which.".to_string(),
            (None, Some(c)) => format!("fefw.exe exited with code {} (0x{:08X}).", c, c as u32),
            (None, None) => "fefw.exe was ended by the system.".to_string(),
        };
        set_status(&app2, Status { state: "exited".into(), code, crashed, files, message, modded });
        if minimize {
            if let Some(w) = app2.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }
    });
    Ok(())
}

pub fn open_with_shell(path: &Path) -> Result<(), String> {
    Command::new("explorer.exe").arg(path).spawn().map(|_| ()).map_err(|e| format!("Could not open {}: {}", path.display(), e))
}
