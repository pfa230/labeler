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

/// Check sibling entries in `dir_fd` for exact match.
pub fn check_sibling_name(dir_fd: BorrowedFd<'_>, segment: &str) -> Result<bool, AppError> {
    let entries = list_dir_entries(dir_fd)?;
    Ok(entries.iter().any(|entry| entry == segment))
}

pub fn open_exact_segment_dir(
    parent_fd: BorrowedFd<'_>,
    segment: &str,
    caller_supplied: bool,
) -> Result<OwnedFd, AppError> {
    match rustix::fs::openat(
        parent_fd,
        segment,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(opened_fd) => Ok(opened_fd),
        Err(rustix::io::Errno::LOOP) | Err(rustix::io::Errno::NOTDIR) => {
            let msg = if let Ok(stat) =
                rustix::fs::statat(parent_fd, segment, AtFlags::SYMLINK_NOFOLLOW)
            {
                let ft = rustix::fs::FileType::from_raw_mode(stat.st_mode);
                if ft.is_symlink() {
                    format!("group directory '{segment}' is a symbolic link")
                } else {
                    format!("group directory '{segment}' is not a directory")
                }
            } else {
                format!("group directory '{segment}' is not a directory")
            };
            if caller_supplied {
                Err(AppError::template_invalid(
                    Reason::TemplateGroupUnsafePath,
                    msg,
                ))
            } else {
                Err(AppError::render_failed(
                    Reason::TemplateGroupUnsafePath,
                    msg,
                ))
            }
        }
        Err(err) => Err(AppError::render_failed(
            Reason::TemplateRegistryIo,
            format!("failed to open group directory '{segment}': {err}"),
        )),
    }
}

enum EexistClassification {
    ExactDir(OwnedFd),
    Unsafe(AppError),
    CaseConflict(AppError),
    Vanished,
}

fn classify_eexist(
    parent_fd: BorrowedFd<'_>,
    segment: &str,
    caller_supplied: bool,
    current_path: &Path,
) -> Result<EexistClassification, AppError> {
    let entries = list_dir_entries(parent_fd)?;
    if entries.iter().any(|e| e == segment) {
        match open_exact_segment_dir(parent_fd, segment, caller_supplied) {
            Ok(fd) => Ok(EexistClassification::ExactDir(fd)),
            Err(err) if err.status() == axum::http::StatusCode::UNPROCESSABLE_ENTITY => {
                Ok(EexistClassification::Unsafe(err))
            }
            Err(err) => {
                if let Ok(_stat) = rustix::fs::statat(parent_fd, segment, AtFlags::SYMLINK_NOFOLLOW)
                {
                    Err(err)
                } else {
                    Ok(EexistClassification::Vanished)
                }
            }
        }
    } else {
        // No exact entry in listing: resolve requested spelling without following symlinks
        match rustix::fs::statat(parent_fd, segment, AtFlags::SYMLINK_NOFOLLOW) {
            Err(rustix::io::Errno::NOENT) => Ok(EexistClassification::Vanished),
            Err(err) => Err(AppError::render_failed(
                Reason::TemplateRegistryIo,
                format!("failed to stat group directory '{segment}': {err}"),
            )),
            Ok(stat) => {
                let file_type = rustix::fs::FileType::from_raw_mode(stat.st_mode);
                if file_type.is_symlink() {
                    let msg = format!("group directory '{segment}' is a symbolic link");
                    let err = if caller_supplied {
                        AppError::template_invalid(Reason::TemplateGroupUnsafePath, msg)
                    } else {
                        AppError::render_failed(Reason::TemplateGroupUnsafePath, msg)
                    };
                    Ok(EexistClassification::Unsafe(err))
                } else if !file_type.is_dir() {
                    let msg = format!("group directory '{segment}' is not a directory");
                    let err = if caller_supplied {
                        AppError::template_invalid(Reason::TemplateGroupUnsafePath, msg)
                    } else {
                        AppError::render_failed(Reason::TemplateGroupUnsafePath, msg)
                    };
                    Ok(EexistClassification::Unsafe(err))
                } else {
                    // Non-exact directory alias. Find stored spelling by (st_dev, st_ino).
                    // This comparison feeds the message only and authorizes no reuse, rename, or other mutation.
                    let mut stored_spelling = None;
                    for entry_name in &entries {
                        if let Ok(s) =
                            rustix::fs::statat(parent_fd, entry_name, AtFlags::SYMLINK_NOFOLLOW)
                        {
                            if s.st_dev == stat.st_dev && s.st_ino == stat.st_ino {
                                stored_spelling = Some(entry_name.clone());
                                break;
                            }
                        }
                    }
                    let actual_name = stored_spelling.unwrap_or_else(|| segment.to_string());
                    let full_existing_path = if current_path.as_os_str().is_empty() {
                        actual_name
                    } else {
                        current_path
                            .join(&actual_name)
                            .to_string_lossy()
                            .replace('\\', "/")
                    };
                    let err = AppError::template_group_case_conflict(format!(
                        "group path segment '{segment}' clashes with existing group '{full_existing_path}' differing only by case"
                    ));
                    Ok(EexistClassification::CaseConflict(err))
                }
            }
        }
    }
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
            let opened_fd = open_exact_segment_dir(current_fd.as_fd(), segment, caller_supplied)?;
            current_fd = opened_fd;
            current_path.push(segment);
        } else {
            // Create directory exclusively
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

                    let opened_fd =
                        open_exact_segment_dir(current_fd.as_fd(), segment, caller_supplied)?;
                    current_fd = opened_fd;
                    current_path.push(segment);
                }
                Err(rustix::io::Errno::EXIST) => {
                    match classify_eexist(
                        current_fd.as_fd(),
                        segment,
                        caller_supplied,
                        &current_path,
                    )? {
                        EexistClassification::ExactDir(opened_fd) => {
                            current_fd = opened_fd;
                            current_path.push(segment);
                        }
                        EexistClassification::Unsafe(err) => return Err(err),
                        EexistClassification::CaseConflict(err) => return Err(err),
                        EexistClassification::Vanished => {
                            // Retry exclusive create once
                            match rustix::fs::mkdirat(
                                current_fd.as_fd(),
                                segment,
                                Mode::from_raw_mode(0o777),
                            ) {
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

                                    let opened_fd = open_exact_segment_dir(
                                        current_fd.as_fd(),
                                        segment,
                                        caller_supplied,
                                    )?;
                                    current_fd = opened_fd;
                                    current_path.push(segment);
                                }
                                Err(rustix::io::Errno::EXIST) => {
                                    // Final classification
                                    match classify_eexist(
                                        current_fd.as_fd(),
                                        segment,
                                        caller_supplied,
                                        &current_path,
                                    )? {
                                        EexistClassification::ExactDir(opened_fd) => {
                                            current_fd = opened_fd;
                                            current_path.push(segment);
                                        }
                                        EexistClassification::Unsafe(err) => return Err(err),
                                        EexistClassification::CaseConflict(err) => return Err(err),
                                        EexistClassification::Vanished => {
                                            return Err(AppError::render_failed(
                                                Reason::TemplateRegistryIo,
                                                format!("unstable concurrent race creating group directory '{segment}'"),
                                            ));
                                        }
                                    }
                                }
                                Err(err) => {
                                    return Err(AppError::render_failed(
                                        Reason::TemplateRegistryIo,
                                        format!(
                                            "failed to create group directory '{segment}': {err}"
                                        ),
                                    ));
                                }
                            }
                        }
                    }
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

/// Resolve an existing group path for rename.
/// Returns `(parent_dir_fd, old_segment_name, group_dir_fd)`.
/// Every component must match exact entry name (404 on mismatch).
/// A symbolic link or regular file component produces `422 TemplateGroupUnsafePath`.
pub fn resolve_group_for_rename(
    root_fd: BorrowedFd<'_>,
    group_path: &str,
) -> Result<(OwnedFd, String, OwnedFd), AppError> {
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

        match rustix::fs::openat(
            current_fd.as_fd(),
            *segment,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            Ok(next_fd) => {
                if is_last {
                    return Ok((current_fd, segment.to_string(), next_fd));
                } else {
                    current_fd = next_fd;
                }
            }
            Err(rustix::io::Errno::LOOP) | Err(rustix::io::Errno::NOTDIR) => {
                let msg = if let Ok(stat) =
                    rustix::fs::statat(current_fd.as_fd(), *segment, AtFlags::SYMLINK_NOFOLLOW)
                {
                    let ft = rustix::fs::FileType::from_raw_mode(stat.st_mode);
                    if ft.is_symlink() {
                        format!("group directory '{segment}' is a symbolic link")
                    } else {
                        format!("group directory '{segment}' is not a directory")
                    }
                } else {
                    format!("group directory '{segment}' is not a directory")
                };
                return Err(AppError::template_invalid(
                    Reason::TemplateGroupUnsafePath,
                    msg,
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

    Err(AppError::not_found(group_path))
}

/// Recursively collect relative subgroup paths within a directory descriptor.
/// Does not follow symlinks, skips dot-directories and invalid group segment names.
pub fn collect_subgroup_rel_paths_fd(
    dir_fd: BorrowedFd<'_>,
    current_rel: &str,
    out: &mut Vec<String>,
) -> Result<(), AppError> {
    let entries = list_dir_entries(dir_fd)?;
    for entry in entries {
        if entry.starts_with('.') {
            continue;
        }
        if crate::templates::validate_group_segment(&entry).is_err() {
            continue;
        }

        match rustix::fs::openat(
            dir_fd,
            &entry,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            Ok(sub_fd) => {
                let rel_path = if current_rel.is_empty() {
                    entry.clone()
                } else {
                    format!("{current_rel}/{entry}")
                };
                out.push(rel_path.clone());
                collect_subgroup_rel_paths_fd(sub_fd.as_fd(), &rel_path, out)?;
            }
            Err(rustix::io::Errno::LOOP) | Err(rustix::io::Errno::NOTDIR) => {
                continue;
            }
            Err(err) => {
                return Err(AppError::render_failed(
                    Reason::TemplateRegistryIo,
                    format!("failed to open subdirectory '{entry}': {err}"),
                ));
            }
        }
    }
    Ok(())
}

/// Atomically rename a group directory using NOREPLACE.
pub fn rename_group_dir(
    parent_fd: BorrowedFd<'_>,
    old_name: &str,
    new_name: &str,
) -> Result<(), AppError> {
    if old_name == new_name {
        return Ok(());
    }

    match rustix::fs::renameat_with(
        parent_fd,
        old_name,
        parent_fd,
        new_name,
        RenameFlags::NOREPLACE,
    ) {
        Ok(()) => Ok(()),
        Err(rustix::io::Errno::EXIST) => Err(AppError::conflict(format!(
            "destination group directory '{new_name}' already exists"
        ))),
        Err(rustix::io::Errno::NOSYS) | Err(rustix::io::Errno::INVAL) => {
            Err(AppError::render_failed(
                Reason::TemplateRegistryIo,
                "platform does not support atomic no-replace directory rename",
            ))
        }
        Err(rustix::io::Errno::NOENT) => Err(AppError::not_found(old_name)),
        Err(err) => Err(AppError::render_failed(
            Reason::TemplateRegistryIo,
            format!("failed to rename group directory '{old_name}' to '{new_name}': {err}"),
        )),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "labeler_fs_safe_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn exact_reuse_and_sibling_creation() {
        let dir = temp_dir();
        fs::create_dir_all(dir.join("Warehouse")).unwrap();
        let root_fd = open_dir_handle(&dir).unwrap();

        // Exact match finds Warehouse
        assert!(check_sibling_name(root_fd.as_fd(), "Warehouse").unwrap());
        // Exact match does not find warehouse on case-sensitive filesystem
        assert!(!check_sibling_name(root_fd.as_fd(), "warehouse").unwrap());

        // resolve_or_create_group reuses Warehouse
        let res = resolve_or_create_group(root_fd.as_fd(), Some("Warehouse"), false).unwrap();
        assert!(res.created_dirs.is_empty());

        // resolve_or_create_group creates warehouse as distinct sibling
        let res2 = resolve_or_create_group(root_fd.as_fd(), Some("warehouse"), true).unwrap();
        assert_eq!(res2.created_dirs.len(), 1);
        assert!(dir.join("Warehouse").is_dir());
        assert!(dir.join("warehouse").is_dir());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolve_for_rename_symlink_and_file_distinguished() {
        let dir = temp_dir();
        fs::create_dir_all(dir.join("real_dir")).unwrap();
        fs::write(dir.join("real_file"), b"hello").unwrap();
        let ext = temp_dir();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&ext, dir.join("sym_dir")).unwrap();

        let root_fd = open_dir_handle(&dir).unwrap();

        // Real dir succeeds
        let (parent, seg, _fd) = resolve_group_for_rename(root_fd.as_fd(), "real_dir").unwrap();
        assert_eq!(seg, "real_dir");
        drop(parent);

        // File produces 422 with not a directory message
        let err = resolve_group_for_rename(root_fd.as_fd(), "real_file").unwrap_err();
        assert_eq!(err.status(), axum::http::StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(err.reason(), Some("template_group_unsafe_path"));
        assert!(err.message_text().contains("is not a directory"));

        #[cfg(unix)]
        {
            // Symlink produces 422 with symbolic link message
            let err = resolve_group_for_rename(root_fd.as_fd(), "sym_dir").unwrap_err();
            assert_eq!(err.status(), axum::http::StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(err.reason(), Some("template_group_unsafe_path"));
            assert!(err.message_text().contains("is a symbolic link"));
        }

        fs::remove_dir_all(&dir).ok();
        fs::remove_dir_all(&ext).ok();
    }

    #[test]
    fn rename_group_dir_atomic_no_replace() {
        let dir = temp_dir();
        fs::create_dir_all(dir.join("src_group")).unwrap();
        fs::create_dir_all(dir.join("existing_dest")).unwrap();
        let root_fd = open_dir_handle(&dir).unwrap();

        // Idempotent rename to same name
        rename_group_dir(root_fd.as_fd(), "src_group", "src_group").unwrap();
        assert!(dir.join("src_group").is_dir());

        // Rename onto existing empty destination fails 409
        let err = rename_group_dir(root_fd.as_fd(), "src_group", "existing_dest").unwrap_err();
        assert_eq!(err.status(), axum::http::StatusCode::CONFLICT);
        assert!(dir.join("src_group").is_dir());
        assert!(dir.join("existing_dest").is_dir());

        // Rename to free name succeeds
        rename_group_dir(root_fd.as_fd(), "src_group", "new_group").unwrap();
        assert!(!dir.join("src_group").exists());
        assert!(dir.join("new_group").is_dir());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn classify_eexist_state_machine_and_inode_comparison() {
        let dir = temp_dir();
        fs::create_dir_all(dir.join("Exact")).unwrap();
        fs::write(dir.join("ExactFile"), b"file").unwrap();
        let root_fd = open_dir_handle(&dir).unwrap();

        // 1. Exact directory present
        let c1 = classify_eexist(root_fd.as_fd(), "Exact", true, Path::new("")).unwrap();
        assert!(matches!(c1, EexistClassification::ExactDir(_)));

        // 2. Exact file present (unsafe)
        let c2 = classify_eexist(root_fd.as_fd(), "ExactFile", true, Path::new("")).unwrap();
        assert!(matches!(c2, EexistClassification::Unsafe(_)));

        // 3. Vanished entry
        let c3 = classify_eexist(root_fd.as_fd(), "Ghost", true, Path::new("")).unwrap();
        assert!(matches!(c3, EexistClassification::Vanished));

        // 4. Inode comparison selects stored spelling for error message without authorising mutation
        let entries = list_dir_entries(root_fd.as_fd()).unwrap();
        let exact_stat =
            rustix::fs::statat(root_fd.as_fd(), "Exact", AtFlags::SYMLINK_NOFOLLOW).unwrap();
        let mut matched = None;
        for entry in &entries {
            if let Ok(s) = rustix::fs::statat(root_fd.as_fd(), entry, AtFlags::SYMLINK_NOFOLLOW) {
                if s.st_dev == exact_stat.st_dev && s.st_ino == exact_stat.st_ino {
                    matched = Some(entry.clone());
                    break;
                }
            }
        }
        assert_eq!(matched.as_deref(), Some("Exact"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn collect_subgroup_rel_paths_fd_traversal() {
        let dir = temp_dir();
        fs::create_dir_all(dir.join("a/b/c")).unwrap();
        fs::create_dir_all(dir.join(".hidden/sub")).unwrap();
        fs::create_dir_all(dir.join("a/invalid:name")).unwrap();
        fs::write(dir.join("a/b/file.txt"), b"hello").unwrap();

        let root_fd = open_dir_handle(&dir).unwrap();
        let mut out = Vec::new();
        collect_subgroup_rel_paths_fd(root_fd.as_fd(), "", &mut out).unwrap();
        out.sort();

        assert_eq!(out, vec!["a", "a/b", "a/b/c"]);

        fs::remove_dir_all(&dir).ok();
    }
}
