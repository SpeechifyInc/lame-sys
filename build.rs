use std::{
    env, fs,
    io::BufReader,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::ensure;
use flate2::bufread::GzDecoder;
use tar::Archive;

// todo: [don't use canonicalize](https://kornel.ski/rust-sys-crate)

fn version() -> anyhow::Result<String> {
    let major = env::var("CARGO_PKG_VERSION_MAJOR")?;
    let minor = env::var("CARGO_PKG_VERSION_MINOR")?;

    Ok(format!("{major}.{minor}"))
}

fn dir_name() -> anyhow::Result<String> {
    let version = version()?;
    Ok(format!("lame-{version}"))
}

fn file_name() -> anyhow::Result<String> {
    let version = version()?;
    Ok(format!("lame-{version}.tar.gz"))
}

/// URL of the file to download
fn url() -> anyhow::Result<String> {
    let version = version()?;
    let file_name = file_name()?;
    Ok(format!(
        "https://downloads.sourceforge.net/project/lame/lame/{version}/{file_name}"
    ))
}

fn lame_dir(install_dir: &Path) -> anyhow::Result<PathBuf> {
    let dir_name = dir_name()?;
    Ok(install_dir.join(dir_name))
}

/// Returns the path to the lame dir
fn clone_to(install_dir: &Path) -> anyhow::Result<()> {
    let url = url()?;
    let response = reqwest::blocking::get(url)?.error_for_status()?;

    let reader = BufReader::new(response);

    let tar = GzDecoder::new(reader);
    let mut archive = Archive::new(tar);
    archive.unpack(install_dir)?;

    let lame_dir = lame_dir(install_dir)?;

    // make sure dir exists
    ensure!(lame_dir.exists(), "lame_dir does not exist");

    Ok(())
}

fn apply_patches(lame_dir: &Path) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    apply_patches_macos(lame_dir)?;

    Ok(())
}

fn apply_patches_macos(lame_dir: &Path) -> anyhow::Result<()> {
    // Fix undefined symbol error _lame_init_old
    // https://sourceforge.net/p/lame/mailman/message/36081038/

    let path = lame_dir.join("include").join("libmp3lame.sym");

    let content = fs::read_to_string(&path)?;
    let new_content = content.replace("lame_init_old\n", "");
    fs::write(&path, new_content)?;

    Ok(())
}

// todo: make sure works twice. specifically file replace. should this be moved to `install`?
fn install(lame_dir: &Path, prefix: &str) -> anyhow::Result<()> {
    apply_patches(lame_dir)?;

    // TODO: see if I need to "--enable-shared"
    let flags = &["--enable-nasm", "--enable-static", "--with-pic"];

    let configure_path = lame_dir.join("configure");

    // Execute the configure command
    let status = Command::new(configure_path)
        .current_dir(lame_dir)
        .args(flags)
        .arg(format!("--prefix={prefix}"))
        .status()?;

    ensure!(status.success(), "configure failed");

    // Execute the make command
    let status = Command::new("make").current_dir(lame_dir).status()?;

    ensure!(status.success(), "make failed");

    // Install the library
    let status = Command::new("make")
        .current_dir(lame_dir)
        .arg("install")
        .status()?;

    ensure!(status.success(), "make failed");

    Ok(())
}

fn prefix_dir(out_dir: &Path) -> PathBuf {
    out_dir.join("lame-install")
}

fn init() -> anyhow::Result<()> {
    let out_dir = env::var("OUT_DIR").unwrap();
    let out_dir = Path::new(&out_dir);
    let out_dir = out_dir.canonicalize()?;

    let install_dir = out_dir.join("local_build");

    // [A] only install if DNE
    if !install_dir.exists() {
        let attempt = clone_to(&install_dir);

        if let Err(e) = attempt {
            let _ = fs::remove_dir_all(&install_dir);
            return Err(e);
        }

        if attempt.is_err() {
            // remove dir so that [A] does not cache incorrectly
        }

        attempt?;
    }

    let lame_dir = lame_dir(&install_dir)?;
    let prefix_dir = prefix_dir(&out_dir);

    let prefix = prefix_dir.to_str().unwrap();

    install(&lame_dir, prefix)?;

    let lib_dir = prefix_dir.join("lib");
    let include_dir = prefix_dir.join("include");

    // add to path
    println!("cargo:rustc-link-search=native={}", lib_dir.display());

    // static link
    println!("cargo:rustc-link-lib=static=mp3lame");
    println!("cargo:include={}", include_dir.display());

    Ok(())
}

fn main() {
    init().unwrap();
}
