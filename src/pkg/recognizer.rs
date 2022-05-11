#[cfg_attr(test, derive(Copy, Clone, Debug, PartialEq))]
pub enum PackageManager {
    Npm,
    Cargo,
}

pub fn package_manager_from_filename(package_file: &str) -> Option<PackageManager> {
    if package_file.ends_with("package-lock.json") {
        Some(PackageManager::Npm)
    } else if package_file.ends_with("Cargo.lock") {
        Some(PackageManager::Cargo)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use expects::{
        matcher::{be_none, be_some, equal},
        Subject,
    };

    use super::*;

    #[test]
    fn it_recognizes_the_npm_package_lock_file() {
        package_manager_from_filename("package-lock.json")
            .should(be_some(equal(PackageManager::Npm)));
    }

    #[test]
    fn it_recognizes_the_npm_package_lock_file_even_with_full_path() {
        package_manager_from_filename("/path/to/package-lock.json")
            .should(be_some(equal(PackageManager::Npm)));
    }

    #[test]
    fn it_recognizes_the_cargo_package_lock_file() {
        package_manager_from_filename("Cargo.lock").should(be_some(equal(PackageManager::Cargo)));
    }

    #[test]
    fn it_recognizes_the_cargo_package_lock_file_even_with_full_path() {
        package_manager_from_filename("/path/to/Cargo.lock")
            .should(be_some(equal(PackageManager::Cargo)));
    }

    #[test]
    fn if_it_doesnt_recognize_the_package_manager_returns_none() {
        package_manager_from_filename("some-file-name").should(be_none());
    }
}
