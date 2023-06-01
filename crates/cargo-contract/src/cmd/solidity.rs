use anyhow::{Error, Result};
use std::{
    ffi::OsStr,
    fs::canonicalize,
    path::Path,
    path::PathBuf,
    process::Command,
    str::from_utf8,
};

fn get_extension_from_filename(filename: &str) -> Option<&str> {
    Path::new(filename)
        .extension()
        .and_then(OsStr::to_str)
}

pub fn build_solidity_contract(solidity_filename: String, compiler_target: &String) -> Result<PathBuf, Error> {
    let solidity_file_relative_path = format!("./{solidity_filename}");
    let solidity_file_dir = PathBuf::from(solidity_file_relative_path);
    println!("solidity_file_dir {:?}", solidity_file_dir);
    let canonical_solidity_file_dir = canonicalize(&solidity_file_dir)?;
    println!("canonical_solidity_file_dir: {:?}", canonical_solidity_file_dir);
    let exists_solidity_file = std::path::Path::new(&canonical_solidity_file_dir).exists();
    println!("exists_solidity_file {:?}", &exists_solidity_file);

    let project_root_relative_path = format!("./");
    let project_root_dir = PathBuf::from(project_root_relative_path);
    let canonical_project_root_dir = canonicalize(&project_root_dir)?;
    println!("canonical_project_root_dir {:?}", &canonical_project_root_dir);

    let solang_shell_script_relative_path = format!("./solang.sh");
    let solang_shell_script_file_dir = PathBuf::from(solang_shell_script_relative_path);
    println!("solang_shell_script_file_dir {:?}", solang_shell_script_file_dir);
    let canonical_solang_shell_script_file_dir = canonicalize(&solang_shell_script_file_dir)?;
    println!("canonical_solang_shell_script_file_dir: {:?}", canonical_solang_shell_script_file_dir);

    if get_extension_from_filename(&solidity_filename) == Some("sol") && exists_solidity_file {
        println!("Found file {:?} with Solidity file extension in the project root", solidity_filename);

        let output = if cfg!(target_os = "windows") {
            println!("Detected Windows OS");
            Command::new("cmd")
                // project root directory
                .current_dir(canonical_project_root_dir.clone())
                // .args(["/C", "echo hello"])
                .arg(format!("{:?} {:?} {:?} {:?}",
                    canonical_solang_shell_script_file_dir.display(), &solidity_filename, &canonical_solidity_file_dir, compiler_target.to_string()))
                .output()
                .expect("failed to execute process")
        } else {
            Command::new("sh")
                // project root directory
                .current_dir(canonical_project_root_dir.clone())
                .arg("-c")
                // .arg("echo hello")
                .arg(format!("{:?} {:?} {:?} {:?}",
                    canonical_solang_shell_script_file_dir.display(), &solidity_filename, &canonical_solidity_file_dir, compiler_target.to_string()))
                .output()
                .expect("failed to execute process")
        };
        let output = output.stdout;
        println!("output: {:#?}", from_utf8(&output).unwrap());
        return Ok(canonical_project_root_dir)
    } else {
        anyhow::bail!("Unable to find a filename {:?} with .sol file extension in the project root folder", solidity_filename);
    }
}
