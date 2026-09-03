//! Guards the facts that must agree before a version is published.
//!
//! hilvan ships to several places - GitHub, the container registry, the docs
//! site - and each renders its own copy of the README or carries its own
//! manifest. Drift is only visible after publishing, when it is too late to
//! take back, so these checks run in CI instead.

use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: impl AsRef<Path>) -> String {
    let path = repo_root().join(path);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// Reads a top-level `key = "value"` from the `[package]` block of Cargo.toml.
///
/// Deliberately naive: it stops at the next section, which is all these checks
/// need, and avoids a TOML parser as a dev-dependency.
fn cargo_field(key: &str) -> String {
    let manifest = read("Cargo.toml");
    for line in manifest.lines() {
        let line = line.trim();
        // `version` also appears under [dependencies] and in every dependency.
        if line.starts_with('[') && line != "[package]" {
            break;
        }
        let Some((name, value)) = line.split_once('=') else { continue };
        // Exact match, so `rust-version` cannot answer a lookup for `version`.
        if name.trim() != key {
            continue;
        }
        return value.trim().trim_matches('"').to_string();
    }
    panic!("`{key}` not found in the [package] block of Cargo.toml");
}

/// Pulls `"version": "x.y.z"` out of a package.json without a JSON dependency:
/// the field is the only thing these checks care about.
fn package_json_version(path: &str) -> String {
    let manifest = read(path);
    let key = "\"version\"";
    let at = manifest.find(key).unwrap_or_else(|| panic!("{path} has no version field"));
    let rest = &manifest[at + key.len()..];
    let open = rest.find('"').unwrap_or_else(|| panic!("{path}: malformed version field"));
    let rest = &rest[open + 1..];
    let close = rest.find('"').unwrap_or_else(|| panic!("{path}: unterminated version string"));
    rest[..close].to_string()
}

#[test]
fn readme_links_resolve_off_github() {
    // The same file is rendered wherever the package lands - the container
    // registry, the docs site - and a relative path has no repository to
    // resolve against there: the banner turns into a broken image and the
    // links 404.
    let readme = read("README.md");

    for (line_no, line) in readme.lines().enumerate() {
        for (marker, kind) in [("src=\"", "image"), ("](", "link")] {
            let mut rest = line;
            while let Some(at) = rest.find(marker) {
                let target = &rest[at + marker.len()..];
                let end = if marker == "](" { ')' } else { '"' };
                let target = &target[..target.find(end).unwrap_or(target.len())];

                let relative = !target.starts_with("http") && !target.starts_with('#') && !target.is_empty();
                assert!(
                    !relative,
                    "README line {}: relative {kind} `{target}` breaks off GitHub; use an absolute URL",
                    line_no + 1
                );

                rest = &rest[at + marker.len()..];
            }
        }
    }
}

#[test]
fn readme_is_not_duplicated() {
    // One README for every storefront. A second copy is where descriptions
    // start to drift; the SPA and the docs site must reuse the root file
    // rather than fork it.
    for candidate in ["web/README.md", "docs/README.md", "docs/site/README.md"] {
        let duplicate = repo_root().join(candidate);
        assert!(
            !duplicate.exists(),
            "{candidate} exists; it will drift from the root README, which is the single source"
        );
    }
}

#[test]
fn every_manifest_carries_the_same_version() {
    // Three manifests, one release. A stale number in web/ or docs/ ships a
    // build that claims a version it is not.
    let crate_version = cargo_field("version");
    assert_eq!(crate_version, env!("CARGO_PKG_VERSION"), "the manifest reader disagrees with cargo");

    for manifest in ["web/package.json", "docs/site/package.json"] {
        assert_eq!(
            package_json_version(manifest),
            crate_version,
            "{manifest} disagrees with Cargo.toml about the version"
        );
    }
}

#[test]
fn the_changelog_covers_the_version_being_shipped() {
    // The tag drives a release, and the release notes are cut from the
    // changelog. A manifest bumped without a changelog entry ships a version
    // nobody can read the changes of.
    //
    // 0.0.0 is the one version that never gets a heading of its own: it is
    // what the manifests carry before anything has been tagged, and its
    // changes belong in `## [Unreleased]` until the first release cuts it.
    let version = cargo_field("version");
    let changelog = read("CHANGELOG.md");

    if version == "0.0.0" {
        assert!(changelog.contains("## [Unreleased]"), "CHANGELOG.md has no `## [Unreleased]` section");
        return;
    }

    let heading = format!("## [{version}]");
    assert!(
        changelog.contains(&heading),
        "CHANGELOG.md has no `{heading}` section; run `git-cliff --tag v{version}` before tagging"
    );
}

#[test]
fn the_readme_documents_every_environment_variable() {
    // The configuration table is the only place an operator learns these
    // exist. A variable added to the code and not to the table is invisible
    // until someone reads the source, which is not what a self-hosted product
    // can ask of them.
    let config = read("src/config.rs");
    let readme = read("README.md");
    let example = read(".env.example");

    let mut found = 0;
    for line in config.lines() {
        let Some(start) = line.find("lookup(\"HILVAN_") else { continue };
        let rest = &line[start + "lookup(\"".len()..];
        let Some(end) = rest.find('"') else { continue };
        let variable = &rest[..end];
        found += 1;

        assert!(
            readme.contains(variable),
            "{variable} is read by the server but missing from the README's configuration table"
        );
        assert!(example.contains(variable), "{variable} is read by the server but missing from .env.example");
    }
    assert!(found > 0, "no variables were found in src/config.rs; the check is looking in the wrong place");
}

#[test]
fn the_compose_files_name_the_image_this_repository_publishes() {
    // The stand builds from source; the publish workflow pushes an image
    // under the repository's own name. If the two drift, `docker compose
    // pull` on someone else's machine fails while ours keeps working.
    let workflow = read(".github/workflows/publish.yml");
    assert!(
        workflow.contains("ghcr.io/${{ github.repository }}"),
        "the publish workflow must push under the repository's own name"
    );

    let prod = read("docker-compose.prod.yml");
    assert!(prod.contains("build:"), "the stand compose must build from source");

    // The install compose is the one a machine with no source checkout uses,
    // so it is the one that breaks silently when the image name drifts: the
    // stand keeps working because it builds its own.
    let install = read("docker-compose.install.yml");
    assert!(
        install.contains("ghcr.io/lacodda/hilvan:"),
        "the install compose must pull the image the publish workflow pushes"
    );
    assert!(
        !install.contains("build:"),
        "the install compose must not build; that is what the prod one is for"
    );

    // Both files carry the same variables, or locking one stand and not the
    // other becomes a thing someone discovers the hard way.
    for variable in ["HILVAN_PASSWORD_HASH", "HILVAN_PORT"] {
        assert!(install.contains(variable), "docker-compose.install.yml does not pass {variable}");
        assert!(prod.contains(variable), "docker-compose.prod.yml does not pass {variable}");
    }
}
