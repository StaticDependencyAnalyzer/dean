use dean::{NpmDependency, NpmDependencyRetriever};
use expects::equal::be_ok;

use expects::iter::contain_element;
use expects::Subject;
use rspec::{describe, run};

#[test]
fn test() {
    run(&describe("NPM Dependency Retriever", false, |c| {
        c.when("retrieving the dependencies of a project", |c| {
            c.it("retrieves all the dependencies", |_c| {
                let dependency_retriever = NpmDependencyRetriever::new();

                let dependencies =
                    dependency_retriever.retrieve_from_reader(npm_package_lock().as_bytes());

                dependencies.should(be_ok(contain_element(NpmDependency {
                    name: "colors".into(),
                })));
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
