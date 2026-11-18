//! Build-time facts.

/// `versionName` on Android; bump it (crates/app/Cargo.toml) for every APK release.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Identifies a web build (the git commit, set by the Dockerfile). `version.json` next to the
/// page carries the deployed one; a tab running another build offers to reload. `dev` never does.
pub const BUILD: &str = match option_env!("STOWARZYSZ_BUILD") {
    Some(build) => build,
    None => "dev",
};

/// Where the Android app finds the API (`scripts/build-apk.sh` sets it for releases). The
/// default is the development machine as seen from the Android emulator.
pub const ANDROID_ORIGIN: &str = match option_env!("STOWARZYSZ_ORIGIN") {
    Some(origin) => origin,
    None => "http://10.0.2.2:8000",
};

/// Android's `versionCode`: `major * 10000 + minor * 100 + patch` (strictly increasing as long
/// as minor and patch stay below 100). `scripts/build-apk.sh` computes the same.
pub fn version_code() -> u32 {
    let mut parts = VERSION.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    let mut next = || parts.next().unwrap_or(0);
    next() * 10_000 + next() * 100 + next()
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_code_follows_the_version() {
        let (major, rest) = super::VERSION.split_once('.').expect("semver");
        let (minor, patch) = rest.split_once('.').expect("semver");
        let expected = major.parse::<u32>().expect("major") * 10_000
            + minor.parse::<u32>().expect("minor") * 100
            + patch.parse::<u32>().expect("patch");
        assert_eq!(super::version_code(), expected);
    }
}
