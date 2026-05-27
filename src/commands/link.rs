/// Link files into the dotling repository.
///
/// Moves the target file (or directory contents) into the repo using
/// [`dest_to_src_path`], deploys a symlink/copy back, and updates the config.
/// Stages changes in git unless `--no-commit` is set.
use std::{fs, path::Path};

use walkdir::WalkDir;

use crate::{
    config::{Config, LinkEntry, LinkMethod},
    error::{DotlingError, Result, io_err},
    git::Git,
    linker::Linker,
    platform::Platform,
    printer::Printer,
    repo,
};

/// Runs the `link` command.
pub fn run(
    printer: &Printer,
    path: &Path,
    as_dir: bool,
    copy: bool,
    no_commit: bool,
    os: Platform,
) -> Result<()> {
    let repo_root = repo::get_repo_root()?;
    let abs_path = repo::resolve_path(path)?;

    if !abs_path.exists() {
        return Err(DotlingError::PathNotFound(abs_path));
    }

    if !repo::is_inside_home(&abs_path) {
        return Err(DotlingError::PathOutsideHome(abs_path));
    }

    // Check if it's already a symlink
    if abs_path.is_symlink() {
        return Err(DotlingError::AlreadySymlink(abs_path));
    }

    let method = if copy {
        LinkMethod::Copy
    } else {
        LinkMethod::Symlink
    };

    let mut config = Config::load(&repo_root)?;
    let git = Git::new(repo_root.clone());
    let linker = Linker::new(repo_root.clone());

    if abs_path.is_dir() && !as_dir {
        link_directory(printer, &repo_root, &abs_path, method, os, &mut config, &linker);
    } else {
        link_single(printer, &repo_root, &abs_path, method, os, &mut config, &linker)?;
    }

    config.save()?;

    if !no_commit {
        git.stage_all()?;
        git.commit("dotling: link files")?;
    }

    Ok(())
}

/// Links a single file or directory-as-unit.
#[allow(clippy::too_many_arguments)]
fn link_single(
    printer: &Printer,
    repo_root: &Path,
    abs_path: &Path,
    method: LinkMethod,
    os: Platform,
    config: &mut Config,
    linker: &Linker,
) -> Result<()> {
    let src_rel = repo::dest_to_src_path(abs_path)?;
    let src_abs = repo_root.join(&src_rel);

    let dest_str = repo::path_with_tilde(abs_path);

    // Check if already tracked
    if config.find_by_dest(&dest_str).is_some() {
        return Err(DotlingError::AlreadyTracked(abs_path.to_path_buf()));
    }

    // Create parent directories in repo
    if let Some(parent) = src_abs.parent() {
        fs::create_dir_all(parent).map_err(io_err(parent))?;
    }

    // Move file into repo
    printer.arrow("move", abs_path, &src_abs);
    fs::rename(abs_path, &src_abs).map_err(io_err(abs_path))?;

    // Add config entry
    let entry = LinkEntry {
        src: src_rel,
        dest: dest_str,
        method,
        os,
    };
    config.add_entry(entry.clone())?;

    // Deploy the link/copy back
    linker.deploy_entry(&entry, false)?;
    printer.ok("linked", abs_path);

    Ok(())
}

/// Links all files within a directory (walking recursively).
#[allow(clippy::too_many_arguments)]
fn link_directory(
    printer: &Printer,
    repo_root: &Path,
    dir_path: &Path,
    method: LinkMethod,
    os: Platform,
    config: &mut Config,
    linker: &Linker,
) {
    let entries: Vec<_> = WalkDir::new(dir_path)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_type().is_file() || e.file_type().is_symlink())
        .collect();

    for entry in entries {
        let file_path = entry.path();

        // Skip symlinks during directory walks (warn, don't error)
        if file_path.is_symlink() {
            printer.skipped("skip", file_path);
            printer.hint("  already a symlink, skipping");
            continue;
        }

        if let Err(e) = link_single(printer, repo_root, file_path, method, os, config, linker) {
            printer.warn(
                "warn",
                &format!("{file_path}: {e}", file_path = file_path.display()),
            );
        }
    }
}
