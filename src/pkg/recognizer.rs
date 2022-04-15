#[cfg_attr(test, derive(Copy, Clone, Debug, PartialEq))]
pub enum PackageManager {
    Npm,
}

fn package_manager_from_filename(package_file: &str) -> Option<PackageManager> {
    match package_file {
        "package-lock.json" => Some(PackageManager::Npm),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use expects::{
        matcher::{be_none, be_some, equal},
        Subject,
    };

    #[test]
    fn it_recognizes_the_npm_package_lock_file() {
        package_manager_from_filename("package-lock.json")
            .should(be_some(equal(PackageManager::Npm)));
    }

    #[test]
    fn if_it_doesnt_recognize_the_package_manager_returns_none() {
        package_manager_from_filename("some-file-name").should(be_none());
    }
}
