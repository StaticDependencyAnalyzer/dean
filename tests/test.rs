use dean::npm;
use expects::equal::be_ok;
use expects::iter::consist_of;
use expects::Subject;
use mockall::mock;
use mockall::predicate::eq;
use rspec::{describe, run};

mock! {
    Retriever{}
    impl npm::InfoRetriever for Retriever{
        fn latest_version(&self, package_name: &str) -> Result<String, String>;
    }
}

#[test]
fn test() {
    run(&describe("NPM Dependency Retriever", false, |c| {
        c.when("retrieving the dependencies of a project", |c| {
            c.it("retrieves all the dependencies", |_c| {
                let mut retriever = Box::new(MockRetriever::new());
                retriever
                    .expect_latest_version()
                    .with(eq("colors"))
                    .return_once(|_| Ok("1.4.1".into()));
                retriever
                    .expect_latest_version()
                    .with(eq("faker"))
                    .return_once(|_| Ok("5.5.3".into()));

                let dependency_reader = npm::DependencyReader::new(retriever);

                let dependencies =
                    dependency_reader.retrieve_from_reader(npm_package_lock().as_bytes());

                dependencies.should(be_ok(consist_of(&[
                    npm::Dependency {
                        name: "colors".into(),
                        version: "1.4.0".into(),
                        latest_version: "1.4.1".into(),
                    },
                    npm::Dependency {
                        name: "faker".into(),
                        version: "5.5.3".into(),
                        latest_version: "5.5.3".into(),
                    },
                ])));
            });
        })
    }))
}

fn npm_package_lock() -> String {
    String::from(
        r#"{
  "name": "foo",
  "version": "1.0.0",
  "lockfileVersion": 2,
  "requires": true,
  "packages": {
    "": {
      "name": "foo",
      "version": "1.0.0",
      "license": "ISC",
      "dependencies": {
        "colors": "^1.4.0",
        "faker": "^5.5.3"
      }
    },
    "node_modules/colors": {
      "version": "1.4.0",
      "resolved": "https://registry.npmjs.org/colors/-/colors-1.4.0.tgz",
      "integrity": "sha512-a+UqTh4kgZg/SlGvfbzDHpgRu7AAQOmmqRHJnxhRZICKFUT91brVhNNt58CMWU9PsBbv3PDCZUHbVxuDiH2mtA==",
      "engines": {
        "node": ">=0.1.90"
      }
    },
    "node_modules/faker": {
      "version": "5.5.3",
      "resolved": "https://registry.npmjs.org/faker/-/faker-5.5.3.tgz",
      "integrity": "sha512-wLTv2a28wjUyWkbnX7u/ABZBkUkIF2fCd73V6P2oFqEGEktDfzWx4UxrSqtPRw0xPRAcjeAOIiJWqZm3pP4u3g=="
    }
  },
  "dependencies": {
    "colors": {
      "version": "1.4.0",
      "resolved": "https://registry.npmjs.org/colors/-/colors-1.4.0.tgz",
      "integrity": "sha512-a+UqTh4kgZg/SlGvfbzDHpgRu7AAQOmmqRHJnxhRZICKFUT91brVhNNt58CMWU9PsBbv3PDCZUHbVxuDiH2mtA=="
    },
    "faker": {
      "version": "5.5.3",
      "resolved": "https://registry.npmjs.org/faker/-/faker-5.5.3.tgz",
      "integrity": "sha512-wLTv2a28wjUyWkbnX7u/ABZBkUkIF2fCd73V6P2oFqEGEktDfzWx4UxrSqtPRw0xPRAcjeAOIiJWqZm3pP4u3g=="
    }
  }
}"#,
    )
}
