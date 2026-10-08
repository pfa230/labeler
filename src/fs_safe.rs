//! Atomic, fd-relative template writes using `rustix` (#227).
//!
//! Every write stages the body in a dot-named temp file beside its destination, which the registry
//! skips, and then publishes it in one step: an exclusive link for a create, a rename for a replace.

use rustix::fd::{BorrowedFd, OwnedFd};
use rustix::fs::{AtFlags, Mode, OFlags, RenameFlags};
use std::path::Path;

use crate::errors::{AppError, NotFoundKind};

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
        AppError::internal(format!(
            "failed to open directory '{}': {err}",
            path.display()
        ))
    })
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
                    return Err(AppError::internal(format!(
                        "failed to persist template: {err}"
                    )));
                }
            }
        }
        Err(err) => {
            let _ = rustix::fs::unlinkat(dest_fd, &staging_name, AtFlags::empty());
            return Err(AppError::internal(format!(
                "failed to persist template: {err}"
            )));
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
            Err(AppError::internal(format!(
                "failed to persist template: {err}"
            )))
        }
    }
}

/// Unlink template `id`'s file, `{id}.yaml`, in `dir_fd`; a missing file is the template's `404`.
pub fn unlink_file(dir_fd: BorrowedFd<'_>, id: &str) -> Result<(), AppError> {
    let filename = format!("{id}.yaml");
    match rustix::fs::unlinkat(dir_fd, &filename, AtFlags::empty()) {
        Ok(()) => Ok(()),
        Err(rustix::io::Errno::NOENT) => Err(AppError::not_found(NotFoundKind::Template, id)),
        Err(err) => Err(AppError::internal(format!(
            "failed to delete template '{filename}': {err}"
        ))),
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
                    return Err(AppError::internal(format!(
                        "failed to write staging file: {err}"
                    )));
                }
                return Ok((tmp_name, file.into()));
            }
            Err(rustix::io::Errno::EXIST) => {
                last_err = Some(rustix::io::Errno::EXIST);
            }
            Err(rustix::io::Errno::LOOP) => {
                return Err(AppError::internal(format!(
                    "staging path for '{filename}' is a symbolic link"
                )));
            }
            Err(err) => {
                return Err(AppError::internal(format!(
                    "failed to open staging file: {err}"
                )));
            }
        }
    }

    Err(AppError::internal(format!(
        "failed to write template: no free staging name for '{filename}': {}",
        last_err
            .map(|e| e.to_string())
            .unwrap_or_else(|| "unknown".into())
    )))
}
