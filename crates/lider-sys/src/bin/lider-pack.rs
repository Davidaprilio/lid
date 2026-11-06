//! Build helper for `packaging/build.sh` (not installed; the packages run the real `lider`).
//!
//!     lider-pack extensions DIST WORK            the extension zips and firefox-updates.json
//!     lider-pack system-files ROOT HOST LIDER    host script, system host manifests, exported extensions
//!     lider-pack licenses OUT                    licences of the Rust crates and npm packages built into `lider`
//!     lider-pack version                         the version of LID (the workspace Cargo.toml)
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use lider_sys::browser;
use serde_json::Value;

const USAGE: &str = "usage: lider-pack extensions DIST WORK | system-files ROOT HOST_PATH LIDER | licenses OUT | version";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match a.as_slice() {
        ["extensions", dist, work] => fs::create_dir_all(dist)
            .and_then(|_| browser::build_extensions(Path::new(dist), Path::new(work))),
        ["system-files", root, host, lider] => {
            browser::write_system_files(Path::new(root), host, lider)
        }
        ["licenses", out] => licenses(Path::new(out)),
        ["version"] => {
            println!("{}", lider_core::VERSION);
            Ok(())
        }
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(1);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("lider-pack: {e}");
            ExitCode::FAILURE
        }
    }
}

fn other(e: impl std::fmt::Display) -> io::Error {
    io::Error::other(e.to_string())
}

/// Licence texts in a package folder: LICENSE*, LICENCE*, COPYING*, NOTICE*, UNLICENSE*, COPYRIGHT*.
fn licence_files(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let n = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_ascii_uppercase();
            p.is_file()
                && [
                    "LICENSE",
                    "LICENCE",
                    "COPYING",
                    "NOTICE",
                    "UNLICENSE",
                    "COPYRIGHT",
                ]
                .iter()
                .any(|k| n.starts_with(k))
        })
        .collect();
    out.sort();
    out
}

struct Entry {
    name: String,
    version: String,
    license: String,
    source: String,
    files: Vec<PathBuf>,
}

/// The crates linked into the `lider` binary: normal dependencies of the package `lider` on this target
/// (build scripts and proc macros run at build time and are not shipped).
fn rust_crates() -> io::Result<Vec<Entry>> {
    let out = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--filter-platform",
            "x86_64-unknown-linux-gnu",
        ])
        .output()?;
    if !out.status.success() {
        return Err(other(String::from_utf8_lossy(&out.stderr)));
    }
    let meta: Value = serde_json::from_slice(&out.stdout).map_err(other)?;
    let packages: HashMap<&str, &Value> = meta["packages"]
        .as_array()
        .ok_or_else(|| other("no packages"))?
        .iter()
        .map(|p| (p["id"].as_str().unwrap_or(""), p))
        .collect();
    let nodes: HashMap<&str, &Value> = meta["resolve"]["nodes"]
        .as_array()
        .ok_or_else(|| other("no resolve graph"))?
        .iter()
        .map(|n| (n["id"].as_str().unwrap_or(""), n))
        .collect();
    let root = packages
        .values()
        .find(|p| p["name"] == "lider" && p["source"].is_null())
        .and_then(|p| p["id"].as_str())
        .ok_or_else(|| other("package lider not found"))?;
    let mut seen = BTreeSet::new();
    let mut todo = vec![root];
    while let Some(id) = todo.pop() {
        if !seen.insert(id) {
            continue;
        }
        let is_proc_macro = packages[id]["targets"].as_array().is_some_and(|t| {
            t.iter().any(|t| {
                t["kind"]
                    .as_array()
                    .is_some_and(|k| k.iter().any(|k| k == "proc-macro"))
            })
        });
        if is_proc_macro && id != root {
            continue;
        }
        for d in nodes[id]["deps"].as_array().into_iter().flatten() {
            let normal = d["dep_kinds"]
                .as_array()
                .is_some_and(|k| k.iter().any(|k| k["kind"].is_null()));
            if normal && let Some(pkg) = d["pkg"].as_str() {
                todo.push(pkg);
            }
        }
    }
    let mut out = vec![];
    for id in seen {
        let p = packages[id];
        if p["source"].is_null() {
            continue; // LID's own crates
        }
        let is_proc_macro = p["targets"].as_array().is_some_and(|t| {
            t.iter().any(|t| {
                t["kind"]
                    .as_array()
                    .is_some_and(|k| k.iter().any(|k| k == "proc-macro"))
            })
        });
        if is_proc_macro {
            continue;
        }
        let dir = Path::new(p["manifest_path"].as_str().unwrap_or(""))
            .parent()
            .unwrap_or(Path::new(""))
            .to_path_buf();
        let mut files = licence_files(&dir);
        if let Some(f) = p["license_file"].as_str() {
            let f = dir.join(f);
            if f.is_file() && !files.contains(&f) {
                files.push(f);
            }
        }
        out.push(Entry {
            name: p["name"].as_str().unwrap_or("").into(),
            version: p["version"].as_str().unwrap_or("").into(),
            license: p["license"]
                .as_str()
                .unwrap_or("see the licence files")
                .into(),
            source: p["repository"].as_str().unwrap_or("crates.io").into(),
            files,
        });
    }
    Ok(out)
}

/// The npm packages whose code is in the built frontend: the dependencies of ui/package.json and the
/// Svelte runtime.
fn npm_packages(ui: &Path) -> io::Result<Vec<Entry>> {
    let pkg: Value =
        serde_json::from_str(&fs::read_to_string(ui.join("package.json"))?).map_err(other)?;
    let mut names: BTreeSet<String> = pkg["dependencies"]
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    names.insert("svelte".into());
    let mut out = vec![];
    for name in names {
        let dir = ui.join("node_modules").join(&name);
        let meta: Value =
            serde_json::from_str(&fs::read_to_string(dir.join("package.json"))?).map_err(other)?;
        out.push(Entry {
            version: meta["version"].as_str().unwrap_or("").into(),
            license: meta["license"]
                .as_str()
                .unwrap_or("see the licence files")
                .into(),
            source: "npm".into(),
            files: licence_files(&dir),
            name,
        });
    }
    Ok(out)
}

/// OUT/THIRD-PARTY.md lists every package with its licence; OUT/<name>-<version>/ holds its licence texts.
fn licenses(out: &Path) -> io::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let rust = rust_crates()?;
    let npm = npm_packages(&root.join("ui"))?;
    fs::create_dir_all(out)?;
    let mut by_licence: BTreeMap<String, usize> = BTreeMap::new();
    let mut md = String::from(
        "# Third-party software in LID\n\nThe `lider` program is built from these Rust crates and npm packages. Each folder next to \
         this file holds the licence texts that came with that package.\n\nSystem libraries (WebKitGTK, GTK, GLib, \
         libayatana-appindicator) are not part of the program; the packages depend on them, and the AppImage carries \
         copies of them under their own licences (see `system-libraries.md`).\n",
    );
    for (title, list) in [("Rust crates", &rust), ("npm packages", &npm)] {
        md.push_str(&format!(
            "\n## {title}\n\n| Package | Version | Licence |\n|---|---|---|\n"
        ));
        for e in list.iter() {
            *by_licence.entry(e.license.clone()).or_default() += 1;
            md.push_str(&format!("| {} | {} | {} |\n", e.name, e.version, e.license));
            let dir = out.join(format!("{}-{}", e.name.replace('/', "_"), e.version));
            fs::create_dir_all(&dir)?;
            for f in &e.files {
                fs::copy(f, dir.join(f.file_name().unwrap()))?;
            }
            if e.files.is_empty() {
                fs::write(
                    dir.join("LICENSE-NOTE.txt"),
                    format!(
                        "{} {} is licensed under {}. The package ships no licence file; source: {}.\n",
                        e.name, e.version, e.license, e.source
                    ),
                )?;
            }
        }
    }
    md.push_str("\n## Licences in use\n\n");
    for (l, n) in by_licence {
        md.push_str(&format!("- {l}: {n}\n"));
    }
    fs::write(out.join("THIRD-PARTY.md"), md)?;
    println!(
        "{} Rust crates and {} npm packages in {}",
        rust.len(),
        npm.len(),
        out.display()
    );
    Ok(())
}
