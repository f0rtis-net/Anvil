use std::{
    env,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    let arch = env::var("CARGO_CFG_TARGET_ARCH")
        .expect("CARGO_CFG_TARGET_ARCH not set");

    // --- linker script ------------------------------------------------------
    let linker_script = format!("linker-{arch}.ld");
    println!("cargo:rustc-link-arg=-T{linker_script}");
    println!("cargo:rerun-if-changed={linker_script}");

    // --- paths --------------------------------------------------------------
    let src_dir = Path::new("src");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // --- find & compile asm -------------------------------------------------
    let mut object_files = Vec::new();

    visit_dir(src_dir, &mut |path| {
        if path.extension().and_then(|e| e.to_str()) == Some("asm") {
            let obj = compile_nasm(path, &out_dir);
            object_files.push(obj);
        }
    });

    // --- archive ------------------------------------------------------------
    if !object_files.is_empty() {
        let lib_path = out_dir.join("libasm.a");
        archive_objects(&lib_path, &object_files);

        println!("cargo:rustc-link-search={}", out_dir.display());
        println!("cargo:rustc-link-lib=static=asm");
    }
}

// ==========================================================================
// helpers
// ==========================================================================

fn visit_dir(dir: &Path, f: &mut impl FnMut(&Path)) {
    for entry in fs::read_dir(dir).expect("read_dir failed") {
        let entry = entry.expect("bad dir entry");
        let path = entry.path();

        println!("cargo:rerun-if-changed={}", path.display());

        if path.is_dir() {
            visit_dir(&path, f);
        } else {
            f(&path);
        }
    }
}

fn compile_nasm(src: &Path, out_dir: &Path) -> PathBuf {
    let obj = out_dir.join(
        src.file_stem()
            .expect("bad asm filename")
            .to_str()
            .unwrap()
            .to_owned() + ".o",
    );

    let status = Command::new("nasm")
        .args([
            "-f", "elf64",
            "-g", "-F", "dwarf", // debug symbols (можно убрать)
            src.to_str().unwrap(),
            "-o",
            obj.to_str().unwrap(),
        ])
        .status()
        .expect("failed to run nasm");

    assert!(
        status.success(),
        "NASM failed on {}",
        src.display()
    );

    obj
}

fn archive_objects(lib: &Path, objects: &[PathBuf]) {
    let mut cmd = Command::new("ar");
    cmd.args(["crs", lib.to_str().unwrap()]);

    for obj in objects {
        cmd.arg(obj);
    }

    let status = cmd.status().expect("failed to run ar");
    assert!(status.success(), "ar failed");
}
