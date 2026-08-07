use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

fn setup_test_files(temp_dir: &TempDir) {
    // Create example tape file
    let tape_content = r#"Set Engine Auto
Set Output "test.webm"
Type "echo hello"
Enter
"#;
    fs::write(temp_dir.path().join("test.tape"), tape_content).unwrap();
}

#[test]
fn test_cli_help() {
    Command::cargo_bin("tapedeck")
        .unwrap()
        .assert()
        .success()
        .stdout(predicates::str::contains("tapedeck"));
}

#[test]
fn test_cli_version() {
    Command::cargo_bin("tapedeck")
        .unwrap()
        .arg("--version")
        .assert()
        .success();
}

#[test]
fn test_run_command_not_found() {
    let temp_dir = TempDir::new().unwrap();
    setup_test_files(&temp_dir);
    
    Command::cargo_bin("tapedeck")
        .unwrap()
        .arg("run")
        .arg("nonexistent.tape")
        .assert()
        .failure();
}

#[test]
fn test_link_output() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.gif");
    fs::write(&file_path, "fake gif content").unwrap();
    
    Command::cargo_bin("tapedeck")
        .unwrap()
        .arg("link")
        .arg(&file_path)
        .assert()
        .success()
        .stdout(predicates::str::contains("![]"));
}