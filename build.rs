// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/*!
Build script to integrate PyOxidizer.

The goal of this build script is to configure a Rust application that embeds
Python. It keys off `build-mode-*` features / environment variables to determine
how to do this.

The following strategies exist for obtaining the build artifacts needed
by this crate:

1. Call `pyoxidizer run-build-script` and use its output verbatim.
2. Call into the PyOxidizer library directly to perform the equivalent
   of `pyoxidizer run-build-script`. (See commented out section of file for
   an example.)
3. Build artifacts out-of-band and consume them manually in this script
   (e.g. by calling `pyoxidizer build` and then reading the generated files.)
*/

use {
    embed_resource,
    std::path::{Path, PathBuf},
};

/// Filename of artifact containing the default PythonInterpreterConfig definition.
const DEFAULT_PYTHON_CONFIG_FILENAME: &str = "default_python_config.rs";

const DEFAULT_PYTHON_CONFIG: &str = "\
pub fn default_python_config<'a>() -> pyembed::OxidizedPythonInterpreterConfig<'a> {
    pyembed::OxidizedPythonInterpreterConfig::default()
}
";

/// Build with PyOxidizer artifacts in a directory.
fn build_with_artifacts_in_dir(path: &Path) {
    println!("using pre-built artifacts from {}", path.display());

    let config_path = path.join(DEFAULT_PYTHON_CONFIG_FILENAME);
    if !config_path.exists() {
        panic!(
            "{} does not exist; is {} a valid artifacts directory?",
            config_path.display(),
            path.display()
        );
    }

    println!(
        "cargo:rustc-env=DEFAULT_PYTHON_CONFIG_RS={}",
        config_path.display()
    );
}

/// Build by calling a `pyoxidizer` executable to generate build artifacts.
fn build_with_pyoxidizer_exe(exe: Option<String>, resolve_target: Option<&str>) {
    let pyoxidizer_exe = if let Some(path) = exe {
        path
    } else {
        "pyoxidizer".to_string()
    };

    let mut args = vec!["run-build-script", "build.rs"];
    if let Some(target) = resolve_target {
        args.push("--target");
        args.push(target);
    }

    match std::process::Command::new(pyoxidizer_exe)
        .args(args)
        .status()
    {
        Ok(status) => {
            if !status.success() {
                panic!("`pyoxidizer run-build-script` failed");
            }
        }
        Err(e) => panic!("`pyoxidizer run-build-script` failed: {}", e.to_string()),
    }
}

#[allow(clippy::if_same_then_else)]
fn main() {
    if std::env::var("CARGO_FEATURE_BUILD_MODE_STANDALONE").is_ok() {
        let path = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR not defined"));
        let path = path.join(DEFAULT_PYTHON_CONFIG_FILENAME);

        std::fs::write(&path, DEFAULT_PYTHON_CONFIG.as_bytes())
            .expect("failed to write default python config");
        println!(
            "cargo:rustc-env=DEFAULT_PYTHON_CONFIG_RS={}",
            path.display()
        );
    } else if std::env::var("CARGO_FEATURE_BUILD_MODE_PYOXIDIZER_EXE").is_ok() {
        let target = if let Ok(target) = std::env::var("PYOXIDIZER_BUILD_TARGET") {
            Some(target)
        } else {
            None
        };

        build_with_pyoxidizer_exe(
            std::env::var("PYOXIDIZER_EXE").ok(),
            target.as_ref().map(|target| target.as_ref()),
        );
    } else if std::env::var("CARGO_FEATURE_BUILD_MODE_PREBUILT_ARTIFACTS").is_ok() {
        let artifact_dir_env = std::env::var("PYOXIDIZER_ARTIFACT_DIR");

        let artifact_dir_path = match artifact_dir_env {
            Ok(ref v) => PathBuf::from(v),
            Err(_) => {
                let out_dir = std::env::var("OUT_DIR").unwrap();
                PathBuf::from(&out_dir)
            }
        };

        println!("cargo:rerun-if-env-changed=PYOXIDIZER_ARTIFACT_DIR");
        build_with_artifacts_in_dir(&artifact_dir_path);
    } else {
        panic!("build-mode-* feature not set");
    }

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").expect("CARGO_CFG_TARGET_OS not defined");

    // Export symbols from built binaries. This is needed to ensure libpython's
    // symbols are exported. Without those symbols being exported, loaded extension
    // modules won't find the libpython symbols and won't be able to run.
    match target_os.as_str() {
        "linux" => {
            println!("cargo:rustc-link-arg=-Wl,-export-dynamic");
        }
        "macos" => {
            println!("cargo:rustc-link-arg=-rdynamic");
        }
        _ => {}
    }

    let target_family =
        std::env::var("CARGO_CFG_TARGET_FAMILY").expect("CARGO_CFG_TARGET_FAMILY not defined");

    let global_allocator_jemalloc =
        std::env::var("CARGO_FEATURE_GLOBAL_ALLOCATOR_JEMALLOC").is_ok();
    let global_allocator_mimalloc =
        std::env::var("CARGO_FEATURE_GLOBAL_ALLOCATOR_MIMALLOC").is_ok();
    let global_allocator_snmalloc =
        std::env::var("CARGO_FEATURE_GLOBAL_ALLOCATOR_SNMALLOC").is_ok();

    let global_allocator_count = vec![
        global_allocator_jemalloc,
        global_allocator_mimalloc,
        global_allocator_snmalloc,
    ]
    .into_iter()
    .filter(|x| *x)
    .count();

    if global_allocator_count > 1 {
        panic!(
            "at most 1 global-allocator-* feature must be defined; got {}",
            global_allocator_count
        );
    }

    // Embed the XML manifest enabling long paths into the binary.
    //
    // This isn't needed on Windows 10 version 1607 and above, as long paths are
    // enabled by default. But being explicit provides maximum compatibility.
    if target_family == "windows" {
        embed_resource::compile("oside-tests-manifest.rc");
    }
}
