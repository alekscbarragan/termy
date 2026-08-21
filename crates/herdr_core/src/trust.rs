use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HerdrServiceLocation {
    HomeRelative(&'static str),
    BundleRelative(&'static str),
}

pub const DOCUMENTED_HERDR_INSTALL_LOCATIONS: &[HerdrServiceLocation] =
    &[HerdrServiceLocation::HomeRelative(".local/bin/herdr")];
pub const BUNDLED_HERDR_SERVICE_LOCATION: HerdrServiceLocation =
    HerdrServiceLocation::BundleRelative("Contents/MacOS/herdr");

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrustedLocation(PathBuf);

impl TrustedLocation {
    pub fn vet(candidate: &Path, trusted_set: &[PathBuf]) -> Result<Self, TrustRejection> {
        if !candidate.is_absolute() {
            return Err(TrustRejection::new(
                candidate,
                TrustRejectionReason::NotAbsolute,
            ));
        }

        let canonical_candidate = fs::canonicalize(candidate).map_err(|error| {
            let reason = if error.kind() == io::ErrorKind::NotFound {
                TrustRejectionReason::Missing
            } else {
                TrustRejectionReason::Unresolvable
            };
            TrustRejection::new(candidate, reason)
        })?;

        let metadata = fs::metadata(&canonical_candidate)
            .map_err(|_| TrustRejection::new(candidate, TrustRejectionReason::Missing))?;
        if !metadata.is_file() {
            return Err(TrustRejection::new(
                candidate,
                TrustRejectionReason::NotAFile,
            ));
        }
        if !is_executable(&canonical_candidate, &metadata) {
            return Err(TrustRejection::new(
                candidate,
                TrustRejectionReason::NotExecutable,
            ));
        }

        let trusted = trusted_set.iter().any(|anchor| {
            if !anchor.is_absolute() {
                return false;
            }
            let Ok(canonical_anchor) = fs::canonicalize(anchor) else {
                return false;
            };
            let Ok(anchor_metadata) = fs::metadata(&canonical_anchor) else {
                return false;
            };

            if anchor_metadata.is_file() {
                canonical_candidate == canonical_anchor
            } else if anchor_metadata.is_dir() {
                canonical_candidate.starts_with(canonical_anchor)
            } else {
                false
            }
        });

        if !trusted {
            return Err(TrustRejection::new(
                candidate,
                TrustRejectionReason::OutsideTrustedSet,
            ));
        }

        Ok(Self(canonical_candidate))
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrustRejectionReason {
    NotAbsolute,
    Missing,
    Unresolvable,
    NotAFile,
    NotExecutable,
    OutsideTrustedSet,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrustRejection {
    rejected_path: PathBuf,
    reason: TrustRejectionReason,
}

impl TrustRejection {
    fn new(path: &Path, reason: TrustRejectionReason) -> Self {
        Self {
            rejected_path: path.to_path_buf(),
            reason,
        }
    }

    pub fn rejected_path(&self) -> &Path {
        &self.rejected_path
    }

    pub fn reason(&self) -> TrustRejectionReason {
        self.reason
    }
}

#[cfg(unix)]
fn is_executable(_path: &Path, metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;

    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(windows)]
fn is_executable(path: &Path, _metadata: &fs::Metadata) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
}

#[cfg(not(any(unix, windows)))]
fn is_executable(_path: &Path, _metadata: &fs::Metadata) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use std::fs;

    #[cfg(unix)]
    use std::os::unix::fs::{PermissionsExt, symlink};

    use tempfile::TempDir;

    use super::*;

    fn candidate_path(directory: &Path) -> PathBuf {
        #[cfg(windows)]
        {
            directory.join("herdr.exe")
        }
        #[cfg(not(windows))]
        {
            directory.join("herdr")
        }
    }

    fn executable(path: &Path) {
        fs::write(path, b"#!/bin/sh\n").expect("write executable fixture");
        #[cfg(unix)]
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))
            .expect("mark fixture executable");
    }

    #[test]
    fn trust_accepts_canonical_executable_inside_trusted_directory() {
        let temp = TempDir::new().expect("create temp directory");
        let candidate = candidate_path(temp.path());
        executable(&candidate);

        let trusted = TrustedLocation::vet(&candidate, &[temp.path().to_path_buf()])
            .expect("trusted executable should be accepted");

        assert_eq!(
            trusted.as_path(),
            candidate.canonicalize().expect("canonical candidate")
        );
    }

    #[test]
    fn trust_rejects_relative_candidate() {
        let rejection = TrustedLocation::vet(Path::new("bin/herdr"), &[])
            .expect_err("relative candidate must be rejected");
        assert_eq!(rejection.rejected_path(), Path::new("bin/herdr"));
        assert_eq!(rejection.reason(), TrustRejectionReason::NotAbsolute);
    }

    #[test]
    fn trust_rejects_inherited_path_only_candidate() {
        let rejection = TrustedLocation::vet(Path::new("herdr"), &[])
            .expect_err("bare PATH candidate must be rejected");
        assert_eq!(rejection.rejected_path(), Path::new("herdr"));
        assert_eq!(rejection.reason(), TrustRejectionReason::NotAbsolute);
    }

    #[test]
    fn trust_rejects_missing_candidate() {
        let temp = TempDir::new().expect("create temp directory");
        let candidate = temp.path().join("missing-herdr");
        let rejection = TrustedLocation::vet(&candidate, &[temp.path().to_path_buf()])
            .expect_err("missing candidate must be rejected");
        assert_eq!(rejection.rejected_path(), candidate);
        assert_eq!(rejection.reason(), TrustRejectionReason::Missing);
    }

    #[test]
    fn trust_rejects_non_executable_candidate() {
        let temp = TempDir::new().expect("create temp directory");
        let candidate = temp.path().join("herdr.txt");
        fs::write(&candidate, b"not executable").expect("write fixture");
        #[cfg(unix)]
        fs::set_permissions(&candidate, fs::Permissions::from_mode(0o644))
            .expect("set fixture permissions");

        let rejection = TrustedLocation::vet(&candidate, &[temp.path().to_path_buf()])
            .expect_err("non-executable candidate must be rejected");
        assert_eq!(rejection.rejected_path(), candidate);
        assert_eq!(rejection.reason(), TrustRejectionReason::NotExecutable);
    }

    #[cfg(unix)]
    #[test]
    fn trust_rejects_symlink_escape() {
        let trusted = TempDir::new().expect("create trusted directory");
        let outside = TempDir::new().expect("create outside directory");
        let target = outside.path().join("herdr");
        executable(&target);
        let candidate = trusted.path().join("herdr");
        symlink(&target, &candidate).expect("create symlink fixture");

        let rejection = TrustedLocation::vet(&candidate, &[trusted.path().to_path_buf()])
            .expect_err("symlink escape must be rejected");
        assert_eq!(rejection.rejected_path(), candidate);
        assert_eq!(rejection.reason(), TrustRejectionReason::OutsideTrustedSet);
    }

    #[test]
    fn trust_rejects_absolute_candidate_outside_trusted_set() {
        let trusted = TempDir::new().expect("create trusted directory");
        let outside = TempDir::new().expect("create outside directory");
        let candidate = candidate_path(outside.path());
        executable(&candidate);

        let rejection = TrustedLocation::vet(&candidate, &[trusted.path().to_path_buf()])
            .expect_err("outside candidate must be rejected");
        assert_eq!(rejection.rejected_path(), candidate);
        assert_eq!(rejection.reason(), TrustRejectionReason::OutsideTrustedSet);
    }

    #[test]
    fn trust_install_locations_match_herdr_install_documentation() {
        // https://herdr.dev/docs/install/ documents the manual ~/.local/bin/herdr location.
        assert_eq!(
            DOCUMENTED_HERDR_INSTALL_LOCATIONS,
            [HerdrServiceLocation::HomeRelative(".local/bin/herdr")]
        );
        assert_eq!(
            BUNDLED_HERDR_SERVICE_LOCATION,
            HerdrServiceLocation::BundleRelative("Contents/MacOS/herdr")
        );
    }
}
