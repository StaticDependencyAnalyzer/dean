#[derive(Copy, Clone)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub enum PackageManager {
    Npm,
    Cargo,
    Yarn,
}

impl PackageManager {
    pub fn from_filename(package_file: &str) -> Option<PackageManager> {
        if package_file.ends_with("package-lock.json") {
            Some(Self::Npm)
        } else if package_file.ends_with("Cargo.lock") {
            Some(Self::Cargo)
        } else if package_file.ends_with("yarn.lock") {
            Some(Self::Yarn)
        } else {
            None
        }
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
        PackageManager::from_filename("package-lock.json")
            .should(be_some(equal(PackageManager::Npm)));
    }

    #[test]
    fn it_recognizes_the_npm_package_lock_file_even_with_full_path() {
        PackageManager::from_filename("/path/to/package-lock.json")
            .should(be_some(equal(PackageManager::Npm)));
    }

    #[test]
    fn it_recognizes_the_cargo_package_lock_file() {
        PackageManager::from_filename("Cargo.lock").should(be_some(equal(PackageManager::Cargo)));
    }

    #[test]
    fn it_recognizes_the_cargo_package_lock_file_even_with_full_path() {
        PackageManager::from_filename("/path/to/Cargo.lock")
            .should(be_some(equal(PackageManager::Cargo)));
    }

    #[test]
    fn it_recognizes_the_yarn_package_lock_file() {
        PackageManager::from_filename("yarn.lock").should(be_some(equal(PackageManager::Yarn)));
    }

    #[test]
    fn it_recognizes_the_yarn_package_lock_file_even_with_full_path() {
        PackageManager::from_filename("/path/to/yarn.lock")
            .should(be_some(equal(PackageManager::Yarn)));
    }

    #[test]
    fn if_it_doesnt_recognize_the_package_manager_returns_none() {
        PackageManager::from_filename("some-file-name").should(be_none());
    }
}
