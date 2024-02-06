use std::fs;

pub(crate) fn create_dir(path: &str) {
    if let Err(e) = fs::create_dir_all(path) {
        println!("Error creating directories: {}", e);
    } else {
        println!("Directories created: {}", path);
    }
}