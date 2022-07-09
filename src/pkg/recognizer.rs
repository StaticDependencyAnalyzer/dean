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
    use super::*;

    #[test]
    fn it_recognizes_the_npm_package_lock_file() {
        assert_eq!(
            PackageManager::from_filename("package-lock.json").unwrap(),
            PackageManager::Npm
        );
    }

    #[test]
    fn it_recognizes_the_npm_package_lock_file_even_with_full_path() {
        assert_eq!(
            PackageManager::from_filename("/path/to/package-lock.json").unwrap(),
            PackageManager::Npm
        );
    }

    #[test]
    fn it_recognizes_the_cargo_package_lock_file() {
        assert_eq!(
            PackageManager::from_filename("Cargo.lock").unwrap(),
            PackageManager::Cargo
        );
    }

    #[test]
    fn it_recognizes_the_cargo_package_lock_file_even_with_full_path() {
        assert_eq!(
            PackageManager::from_filename("/path/to/Cargo.lock").unwrap(),
            PackageManager::Cargo
        );
    }

    #[test]
    fn it_recognizes_the_yarn_package_lock_file() {
        assert_eq!(
            PackageManager::from_filename("yarn.lock").unwrap(),
            PackageManager::Yarn
        );
    }

    #[test]
    fn it_recognizes_the_yarn_package_lock_file_even_with_full_path() {
        assert_eq!(
            PackageManager::from_filename("/path/to/yarn.lock").unwrap(),
            PackageManager::Yarn
        );
    }

    #[test]
    fn if_it_doesnt_recognize_the_package_manager_returns_none() {
        assert!(PackageManager::from_filename("some-file-name").is_none());
    }
}
