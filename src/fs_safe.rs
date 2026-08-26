//! Safe fd-relative filesystem operations using `rustix` (ADR-0073).
//!
//! Enforces structural containment under `templates/`, exact case matching,
//! no-symlink policies, and pre-publication cleanup.

use rustix::fd::{AsFd, BorrowedFd, OwnedFd};
use rustix::fs::{AtFlags, Mode, OFlags, RenameFlags};
use std::path::{Path, PathBuf};

use crate::errors::AppError;
use crate::reason::Reason;
use crate::templates::validate_group_name;

pub struct ResolvedGroup {
    pub target_fd: OwnedFd,
    pub target_path: PathBuf,
    pub created_dirs: Vec<(OwnedFd, String)>,
}

pub enum PublishResult {
    Published,
    AlreadyExists,
}

pub fn open_dir_handle(path: &Path) -> Result<OwnedFd, AppError> {
    rustix::fs::open(
        path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|err| {
        AppError::render_failed(
            Reason::TemplateRegistryIo,
            format!("failed to open directory '{}': {err}", path.display()),
        )
    })
}

/// List entry names in a directory handle.
pub fn list_dir_entries(dir_fd: BorrowedFd<'_>) -> Result<Vec<String>, AppError> {
    let fd_dup = rustix::fs::openat(
        dir_fd,
        ".",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|err| {
        AppError::render_failed(
            Reason::TemplateRegistryIo,
            format!("failed to re-open directory for listing: {err}"),
        )
    })?;

    let proc_path = format!("/proc/self/fd/{}", rustix::fd::AsRawFd::as_raw_fd(&fd_dup));
    let mut names = Vec::new();
    match std::fs::read_dir(&proc_path) {
        Ok(read_dir) => {
            for entry in read_dir.flatten() {
                if let Ok(name) = entry.file_name().into_string() {
                    names.push(name);
                }
            }
        }
        Err(err) => {
            return Err(AppError::render_failed(
                Reason::TemplateRegistryIo,
                format!("failed to read directory entries: {err}"),
            ));
        }
    }
    Ok(names)
}

/// Check sibling entries in `dir_fd` for exact match and case conflicts.
pub fn check_sibling_name(dir_fd: BorrowedFd<'_>, segment: &str) -> Result<bool, AppError> {
    let entries = list_dir_entries(dir_fd)?;
    let mut exact_found = false;
    let segment_lower = segment.to_lowercase();

    for entry in &entries {
        if entry == segment {
            exact_found = true;
        } else if entry.to_lowercase() == segment_lower {
            return Err(AppError::template_group_case_conflict(format!(
                "group path segment '{segment}' clashes with existing entry '{entry}' differing only by case"
            )));
        }
    }

    Ok(exact_found)
}

/// Component-wise directory resolution and creation.
pub fn resolve_or_create_group(
    root_fd: BorrowedFd<'_>,
    group: Option<&str>,
    caller_supplied: bool,
) -> Result<ResolvedGroup, AppError> {
    let root_owned = rustix::fs::openat(
        root_fd,
        ".",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|err| {
        AppError::render_failed(
            Reason::TemplateRegistryIo,
            format!("failed to clone root fd: {err}"),
        )
    })?;

    let Some(raw_group) = group else {
        return Ok(ResolvedGroup {
            target_fd: root_owned,
            target_path: PathBuf::new(),
            created_dirs: Vec::new(),
        });
    };

    let group_str = validate_group_name(raw_group).map_err(AppError::template_group_invalid)?;
    if group_str.is_empty() {
        return Ok(ResolvedGroup {
            target_fd: root_owned,
            target_path: PathBuf::new(),
            created_dirs: Vec::new(),
        });
    }

    let mut current_fd = root_owned;
    let mut current_path = PathBuf::new();
    let mut created_dirs = Vec::new();

    for segment in group_str.split('/') {
        let exact_exists = check_sibling_name(current_fd.as_fd(), segment)?;

        if exact_exists {
            // Must not be a symlink
            match rustix::fs::openat(
                current_fd.as_fd(),
                segment,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            ) {
                Ok(opened_fd) => {
                    current_fd = opened_fd;
                    current_path.push(segment);
                }
                Err(rustix::io::Errno::LOOP) | Err(rustix::io::Errno::NOTDIR) => {
                    let msg = format!("group directory '{segment}' is a symbolic link");
                    return if caller_supplied {
                        Err(AppError::template_invalid(
                            Reason::TemplateGroupUnsafePath,
                            msg,
                        ))
                    } else {
                        Err(AppError::render_failed(
                            Reason::TemplateGroupUnsafePath,
                            msg,
                        ))
                    };
                }
                Err(err) => {
                    return Err(AppError::render_failed(
                        Reason::TemplateRegistryIo,
                        format!("failed to open group directory '{segment}': {err}"),
                    ));
                }
            }
        } else {
            // Create directory
            match rustix::fs::mkdirat(current_fd.as_fd(), segment, Mode::from_raw_mode(0o777)) {
                Ok(()) => {
                    let parent_clone = rustix::fs::openat(
                        current_fd.as_fd(),
                        ".",
                        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
                        Mode::empty(),
                    )
                    .map_err(|err| {
                        AppError::render_failed(
                            Reason::TemplateRegistryIo,
                            format!("failed to clone parent fd: {err}"),
                        )
                    })?;

                    created_dirs.push((parent_clone, segment.to_string()));

                    let opened_fd = rustix::fs::openat(
                        current_fd.as_fd(),
                        segment,
                        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                        Mode::empty(),
                    )
                    .map_err(|err| {
                        AppError::render_failed(
                            Reason::TemplateRegistryIo,
                            format!("failed to open newly created directory '{segment}': {err}"),
                        )
                    })?;

                    current_fd = opened_fd;
                    current_path.push(segment);
                }
                Err(rustix::io::Errno::EXIST) => {
                    return Err(AppError::template_group_case_conflict(format!(
                        "group directory '{segment}' cannot be created because a case-variant exists"
                    )));
                }
                Err(err) => {
                    return Err(AppError::render_failed(
                        Reason::TemplateRegistryIo,
                        format!("failed to create group directory '{segment}': {err}"),
                    ));
                }
            }
        }
    }

    Ok(ResolvedGroup {
        target_fd: current_fd,
        target_path: current_path,
        created_dirs,
    })
}

/// Cleanup newly created directories in reverse order (innermost first) if request failed.
pub fn cleanup_created_dirs(created_dirs: Vec<(OwnedFd, String)>) {
    for (parent_fd, name) in created_dirs.into_iter().rev() {
        if let Err(err) = rustix::fs::unlinkat(parent_fd.as_fd(), &name, AtFlags::REMOVEDIR) {
            tracing::debug!(name = %name, %err, "stopped directory cleanup");
            break;
        }
    }
}

/// Resolve an existing group path for deletion.
/// Every component must match exact entry name (404 on mismatch), and no symlinks (400).
pub fn resolve_group_for_delete(
    root_fd: BorrowedFd<'_>,
    group_path: &str,
) -> Result<(OwnedFd, String), AppError> {
    let segments: Vec<&str> = group_path.split('/').collect();
    if segments.is_empty() {
        return Err(AppError::not_found(group_path));
    }

    let mut current_fd = rustix::fs::openat(
        root_fd,
        ".",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|err| {
        AppError::render_failed(
            Reason::TemplateRegistryIo,
            format!("failed to clone root fd: {err}"),
        )
    })?;

    for (idx, segment) in segments.iter().enumerate() {
        let is_last = idx == segments.len() - 1;
        let entries = list_dir_entries(current_fd.as_fd())?;

        let exact_found = entries.iter().any(|e| e == *segment);
        if !exact_found {
            return Err(AppError::not_found(group_path));
        }

        if is_last {
            // Check that the last component is not a symlink
            match rustix::fs::openat(
                current_fd.as_fd(),
                *segment,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            ) {
                Ok(_) => return Ok((current_fd, segment.to_string())),
                Err(rustix::io::Errno::LOOP) | Err(rustix::io::Errno::NOTDIR) => {
                    return Err(AppError::invalid_request(
                        Reason::TemplateGroupUnsafePath,
                        format!("group directory '{segment}' is a symbolic link"),
                    ));
                }
                Err(err) if err == rustix::io::Errno::NOENT => {
                    return Err(AppError::not_found(group_path));
                }
                Err(err) => {
                    return Err(AppError::render_failed(
                        Reason::TemplateRegistryIo,
                        format!("failed to open directory '{segment}': {err}"),
                    ));
                }
            }
        } else {
            // Open intermediate directory
            match rustix::fs::openat(
                current_fd.as_fd(),
                *segment,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            ) {
                Ok(next_fd) => {
                    current_fd = next_fd;
                }
                Err(rustix::io::Errno::LOOP) | Err(rustix::io::Errno::NOTDIR) => {
                    return Err(AppError::invalid_request(
                        Reason::TemplateGroupUnsafePath,
                        format!("group directory '{segment}' is a symbolic link"),
                    ));
                }
                Err(err) if err == rustix::io::Errno::NOENT => {
                    return Err(AppError::not_found(group_path));
                }
                Err(err) => {
                    return Err(AppError::render_failed(
                        Reason::TemplateRegistryIo,
                        format!("failed to open directory '{segment}': {err}"),
                    ));
                }
            }
        }
    }

    Err(AppError::not_found(group_path))
}

/// Stage `body` in `dest_fd` and publish exclusively (create).
pub fn stage_and_publish_new(
    dest_fd: BorrowedFd<'_>,
    filename: &str,
    body: &str,
) -> Result<PublishResult, AppError> {
    let (staging_name, staging_fd) = stage_file_in_dir(dest_fd, filename, body)?;
    drop(staging_fd);

    let link_res = rustix::fs::linkat(dest_fd, &staging_name, dest_fd, filename, AtFlags::empty());

    let published = match link_res {
        Ok(()) => PublishResult::Published,
        Err(rustix::io::Errno::EXIST) => PublishResult::AlreadyExists,
        Err(rustix::io::Errno::NOSYS) | Err(rustix::io::Errno::XDEV) => {
            // Fallback to renameat_with NOREPLACE
            match rustix::fs::renameat_with(
                dest_fd,
                &staging_name,
                dest_fd,
                filename,
                RenameFlags::NOREPLACE,
            ) {
                Ok(()) => return Ok(PublishResult::Published),
                Err(rustix::io::Errno::EXIST) => PublishResult::AlreadyExists,
                Err(err) => {
                    let _ = rustix::fs::unlinkat(dest_fd, &staging_name, AtFlags::empty());
                    return Err(AppError::render_failed(
                        Reason::TemplateWriteFailed,
                        format!("failed to persist template: {err}"),
                    ));
                }
            }
        }
        Err(err) => {
            let _ = rustix::fs::unlinkat(dest_fd, &staging_name, AtFlags::empty());
            return Err(AppError::render_failed(
                Reason::TemplateWriteFailed,
                format!("failed to persist template: {err}"),
            ));
        }
    };

    let _ = rustix::fs::unlinkat(dest_fd, &staging_name, AtFlags::empty());
    Ok(published)
}

/// Stage `body` in `dest_fd` and replace `filename`.
pub fn stage_and_replace(
    dest_fd: BorrowedFd<'_>,
    filename: &str,
    body: &str,
) -> Result<(), AppError> {
    let (staging_name, staging_fd) = stage_file_in_dir(dest_fd, filename, body)?;
    drop(staging_fd);

    match rustix::fs::renameat(dest_fd, &staging_name, dest_fd, filename) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = rustix::fs::unlinkat(dest_fd, &staging_name, AtFlags::empty());
            Err(AppError::render_failed(
                Reason::TemplateWriteFailed,
                format!("failed to persist template: {err}"),
            ))
        }
    }
}

/// Move a file fd-relativly between source and destination directories.
pub fn move_template_file(
    src_dir_fd: BorrowedFd<'_>,
    src_filename: &str,
    dest_dir_fd: BorrowedFd<'_>,
    dest_filename: &str,
) -> Result<(), AppError> {
    // Assert source is not a symlink
    match rustix::fs::openat(
        src_dir_fd,
        src_filename,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(_) => {}
        Err(rustix::io::Errno::LOOP) => {
            return Err(AppError::render_failed(
                Reason::TemplateGroupUnsafePath,
                format!("source file '{src_filename}' is a symbolic link"),
            ));
        }
        Err(err) => {
            return Err(AppError::render_failed(
                Reason::TemplateRegistryIo,
                format!("failed to open source file '{src_filename}': {err}"),
            ));
        }
    }

    // Attempt atomic NOREPLACE move
    match rustix::fs::renameat_with(
        src_dir_fd,
        src_filename,
        dest_dir_fd,
        dest_filename,
        RenameFlags::NOREPLACE,
    ) {
        Ok(()) => Ok(()),
        Err(rustix::io::Errno::EXIST) => Err(AppError::template_id_collision(
            dest_filename
                .strip_suffix(".yaml")
                .or_else(|| dest_filename.strip_suffix(".yml"))
                .unwrap_or(dest_filename),
            vec![dest_filename.to_string()],
            format!("destination file '{dest_filename}' already exists"),
        )),
        Err(rustix::io::Errno::NOSYS) | Err(rustix::io::Errno::INVAL) => {
            // Fallback to linkat + unlinkat
            match rustix::fs::linkat(
                src_dir_fd,
                src_filename,
                dest_dir_fd,
                dest_filename,
                AtFlags::empty(),
            ) {
                Ok(()) => {
                    if let Err(err) =
                        rustix::fs::unlinkat(src_dir_fd, src_filename, AtFlags::empty())
                    {
                        tracing::warn!(%err, "failed to unlink source after hard link during move");
                    }
                    Ok(())
                }
                Err(rustix::io::Errno::EXIST) => Err(AppError::template_id_collision(
                    dest_filename
                        .strip_suffix(".yaml")
                        .or_else(|| dest_filename.strip_suffix(".yml"))
                        .unwrap_or(dest_filename),
                    vec![dest_filename.to_string()],
                    format!("destination file '{dest_filename}' already exists"),
                )),
                Err(err) => Err(AppError::render_failed(
                    Reason::TemplateWriteFailed,
                    format!("failed to move template: {err}"),
                )),
            }
        }
        Err(err) => Err(AppError::render_failed(
            Reason::TemplateWriteFailed,
            format!("failed to move template: {err}"),
        )),
    }
}

/// Unlink a file in `dir_fd`.
pub fn unlink_file(dir_fd: BorrowedFd<'_>, filename: &str) -> Result<(), AppError> {
    match rustix::fs::unlinkat(dir_fd, filename, AtFlags::empty()) {
        Ok(()) => Ok(()),
        Err(rustix::io::Errno::NOENT) => Ok(()),
        Err(err) => Err(AppError::render_failed(
            Reason::TemplateDeleteFailed,
            format!("failed to delete template '{filename}': {err}"),
        )),
    }
}

/// Helper to stage body into a nonce-named file in `dest_fd`.
fn stage_file_in_dir(
    dest_fd: BorrowedFd<'_>,
    filename: &str,
    body: &str,
) -> Result<(String, OwnedFd), AppError> {
    use std::io::Write;

    let mut last_err = None;
    for attempt in 0..8 {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
            .wrapping_add(attempt);
        let tmp_name = format!(".{filename}.{nonce}.tmp");

        match rustix::fs::openat(
            dest_fd,
            &tmp_name,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o666),
        ) {
            Ok(fd) => {
                let mut file = std::fs::File::from(fd);
                if let Err(err) = file
                    .write_all(body.as_bytes())
                    .and_then(|()| file.sync_all())
                {
                    let _ = rustix::fs::unlinkat(dest_fd, &tmp_name, AtFlags::empty());
                    return Err(AppError::render_failed(
                        Reason::TemplateWriteFailed,
                        format!("failed to write staging file: {err}"),
                    ));
                }
                return Ok((tmp_name, file.into()));
            }
            Err(rustix::io::Errno::EXIST) => {
                last_err = Some(rustix::io::Errno::EXIST);
            }
            Err(rustix::io::Errno::LOOP) => {
                return Err(AppError::render_failed(
                    Reason::TemplateGroupUnsafePath,
                    format!("staging path for '{filename}' is a symbolic link"),
                ));
            }
            Err(err) => {
                return Err(AppError::render_failed(
                    Reason::TemplateWriteFailed,
                    format!("failed to open staging file: {err}"),
                ));
            }
        }
    }

    Err(AppError::render_failed(
        Reason::TemplateWriteFailed,
        format!(
            "failed to write template: no free staging name for '{filename}': {}",
            last_err
                .map(|e| e.to_string())
                .unwrap_or_else(|| "unknown".into())
        ),
    ))
}
