// SPDX-License-Identifier: GPL-3.0-or-later
//! Select a locally built alternative executable without replacing the stock game.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

pub const FOLDER: &str = "设置补正";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: u32,
    profile: String,
    main_build_id: String,
    runtime_sha256: String,
}

pub struct Launch {
    pub exe: PathBuf,
    pub dir: PathBuf,
    pub exefs: PathBuf,
}

impl Launch {
    pub fn configure(&self, cmd: &mut Command, paths: &crate::setup::Paths) {
        cmd.env("NXRT_FS_EXEFS", &self.exefs)
            .env("NXRT_FS_ROMFS", &paths.romfs)
            .env("NXRT_FS_MODS", paths.mods())
            .env("NXRT_FS_SAVE_DIR", self.dir.join("save"))
            .env("NXRT_NVN_SHADER_CACHE", self.dir.join("shader_cache/spirv"))
            .env("NXRT_NVN_BC7_CACHE", self.dir.join("shader_cache/bc7"));
    }
}

fn sha256(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut sha = Sha256::new();
    io::copy(&mut file, &mut sha)?;
    Ok(format!("{:x}", sha.finalize()))
}

pub fn resolve(game: &Path, stock_main: &Path, stock_runtime: &Path) -> Result<Option<Launch>, String> {
    resolve_inner(game, stock_main, stock_runtime)
        .map_err(|error| format!("Settings MOD: {error}"))
}

fn resolve_inner(game: &Path, stock_main: &Path, stock_runtime: &Path) -> Result<Option<Launch>, String> {
    let bundle = game.join("mods").join(FOLDER);
    match fs::symlink_metadata(&bundle) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
        Ok(metadata) if !metadata.is_dir() => return Err("expected a folder under mods.".into()),
        Ok(_) => {}
    }
    let manifest_path = bundle.join("local-build.json");
    let mut json = Vec::new();
    fs::File::open(&manifest_path).and_then(|file| file.take(4097).read_to_end(&mut json))
        .map_err(|e| format!("could not read local-build.json: {e}"))?;
    if json.len() > 4096 {
        return Err("local-build.json exceeds 4 KiB.".into());
    }
    let manifest: Manifest = serde_json::from_slice(&json).map_err(|e| format!("invalid local-build.json: {e}"))?;
    if manifest.schema != 1 {
        return Err("unsupported local-build.json schema.".into());
    }
    let profile = Path::new(&manifest.profile);
    if !matches!(profile.components().collect::<Vec<_>>().as_slice(), [Component::Normal(_)])
        || ["mods", "save"].iter().any(|reserved| manifest.profile.eq_ignore_ascii_case(reserved))
        || manifest.profile.ends_with([' ', '.'])
    {
        return Err("profile must be a separate directory directly under the game folder.".into());
    }

    let dir = game.join(profile);
    let root = fs::canonicalize(game).map_err(|e| e.to_string())?;
    let profile_path = fs::canonicalize(&dir).map_err(|e| format!("missing profile: {e}"))?;
    let save = fs::canonicalize(dir.join("save")).map_err(|e| format!("missing profile save folder: {e}"))?;
    if profile_path != root.join(profile) || save != profile_path.join("save")
        || !save.is_dir() || !dir.join("fefw.ini").is_file()
    {
        return Err("profile must have its own fefw.ini and save directory.".into());
    }
    let exefs = bundle.join("exefs");
    let exe = bundle.join("fefw.exe");
    if !exe.is_file() || !crate::setup::MODULES.iter().all(|name| exefs.join(name).is_file()) {
        return Err("the local build is incomplete (fefw.exe or ExeFS files are missing).".into());
    }
    let build_id = crate::dump::build_id(stock_main).ok_or("could not read the stock game Build ID.")?;
    let mod_id = crate::dump::build_id(&exefs.join("main")).ok_or("could not read the MOD game Build ID.")?;
    let stock_hash = sha256(stock_runtime).map_err(|e| format!("could not read stock fefw_rt.dll: {e}"))?;
    let mod_hash = sha256(&bundle.join("fefw_rt.dll")).map_err(|e| format!("could not read MOD fefw_rt.dll: {e}"))?;
    if !manifest.main_build_id.eq_ignore_ascii_case(&build_id) || mod_id != build_id
        || !manifest.runtime_sha256.eq_ignore_ascii_case(&stock_hash) || stock_hash != mod_hash
    {
        return Err("this build targets a different game or runtime. Move mods/设置补正 out of mods to play the stock version, or rebuild the MOD locally.".into());
    }
    Ok(Some(Launch { exe, dir, exefs }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_local_build_and_returns_to_stock_after_removal() {
        let root = std::env::temp_dir().join(format!("fefw-code-mod-{}", std::process::id()));
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
        }
        let _cleanup = Cleanup(root.clone());
        fs::create_dir_all(&root).unwrap();
        let main = root.join("main");
        let rt = root.join("fefw_rt.dll");
        assert!(resolve(&root, &main, &rt).unwrap().is_none());
        let bundle = root.join("mods").join(FOLDER);
        fs::create_dir_all(bundle.join("exefs")).unwrap();
        assert!(resolve(&root, &main, &rt).err().unwrap().contains("local-build.json"));
        let profile = root.join("mod-profile");
        fs::create_dir_all(profile.join("save")).unwrap();
        fs::write(profile.join("fefw.ini"), b"").unwrap();
        fs::write(&rt, b"runtime").unwrap();
        fs::copy(&rt, bundle.join("fefw_rt.dll")).unwrap();
        fs::write(bundle.join("fefw.exe"), b"test").unwrap();
        let mut nso = [0u8; 0x54];
        nso[..4].copy_from_slice(b"NSO0");
        fs::write(&main, nso).unwrap();
        for name in crate::setup::MODULES {
            fs::write(bundle.join("exefs").join(name), nso).unwrap();
        }
        let write_manifest = |profile: &str| {
            fs::write(bundle.join("local-build.json"), serde_json::json!({
                "schema": 1, "profile": profile, "main_build_id": "00".repeat(20),
                "runtime_sha256": sha256(&rt).unwrap(),
            }).to_string()).unwrap();
        };
        write_manifest("../save");
        assert!(resolve(&root, &main, &rt).err().unwrap().contains("separate directory"));
        write_manifest("mod-profile");
        let selected = resolve(&root, &main, &rt).unwrap().unwrap();
        assert_eq!(selected.exe, bundle.join("fefw.exe"));
        assert_eq!(selected.dir, profile);
        fs::write(&rt, b"updated runtime").unwrap();
        assert!(resolve(&root, &main, &rt).err().unwrap().contains("different game or runtime"));
        fs::rename(&bundle, root.join("disabled")).unwrap();
        assert!(resolve(&root, &main, &rt).unwrap().is_none());
        assert!(profile.join("save").is_dir());
    }
}
