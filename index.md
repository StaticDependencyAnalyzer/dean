<h1 align="center" style="border-bottom: none">
    <b>
        DEAN<br>
    </b>
    Static (DE)pendency (AN)alyzer <br>
</h1>

<p align="center">
<a href="https://github.com/StaticDependencyAnalyzer/dean/actions/workflows/ci.yml"><img src=https://github.com/StaticDependencyAnalyzer/dean/actions/workflows/ci.yml/badge.svg?branch=master" /></a>
<a href="https://opensource.org/licenses/AGPL-3.0"><img src="https://img.shields.io/badge/license-AGPL--3.0-blue" alt="License: AGPL-3.0"></a>
</p>

## Why dean?

Auditing and keeping the security of the supply chain in software projects is a challenging task, because the
dependencies are chained, and when you import a library, you are also importing all its dependencies.

This application allows you to audit the dependencies of your projects with simple policies.

## Install

### Install from source

It requires you to have Rust installed with the compiler toolchains required for your system. <br>See https://rustup.rs
for instructions of how to install it.

In order to install dean from the source, just run the following command

```
cargo install --force --git https://github.com/StaticDependencyAnalyzer/dean
```

This will install the `dean` executable in your `~/.cargo/bin`.
Make sure to add `~/.cargo/bin` directory to your `PATH` variable.

## Policy implementation roadmap

- [x] Contributor ratio
  > If the contributor ratio is high, the project is at risk of being sabotaged or abandoned, because is only maintained
  by a few people.
  > If the contributor ratio is low, the project is maintained by multiple people and has lower risk.
- [x] Minimum number of releases required
  > If the number of releases is very low, it can be potentially abandoned.
- [x] Issue life span in GitHub projects
  > Shows the activity of the project when issues are reported.
- [x] Pull Request life span in GitHub projects
  > Shows the activity of the project when new PRs are submitted.
- [ ] Number of stars of a project in GitHub
  > A high number of stars in a project shows interest by the community.
- [ ] Number of forks of a project
  > A high number of forks can be considered with lower risk of suddenly disappearing, and the number of contributors
  can be higher.
- [ ] Number of dependants
  > The higher number of dependants of the project, the higher the risk it implies if a vulnerability is found, or the
  project is abandoned.
- [ ] Number and score of the vulnerabilities affecting the project
  > If there's a vulnerability that affects the project, it should be updated ASAP or there is a risk on impacting the
  security of the applications using it.
- [ ] Version deprecation warnings
  > A version marked as deprecated should be updated ASAP, or will be at risk of vulnerabilities or bugs.
- [ ] Older major versions being used
  > A dependency that has a newer major version can be at risk of being deprecated over time.
- [ ] Licenses being used are compatible
  > The usage of incompatible licenses can deal to legal problems and could potentially mean that the dependencies can't
  be used, thus, affecting the application.
- [ ] Follows SemVer
  > If the dependency does not use Semantic Versioning, there's no way to update it in a safe way. An update of the
  dependency can potentially break the API or the behavior with harmful outcomes.
- [ ] Is using a stable version
  > The usage of 0.x.y versions is unsafe. An update of the dependency can potentially break the API or the behavior
  with harmful outcomes.
- [ ] Is using an alpha, beta or RC version
  > The usage of x.y.z-[alpha|beta|rc] is unsafe. The product may not be completely finished and waiting until a final
  release is desirable.
- [ ] Is using the latest minor/patch version
  > Using the latest version would ensure the best support for the application. If there are any bugs present in older
  versions, these may be automatically solved by using the latest major-compatible version.
- [ ] The maintainer is considered malicious
  > Maintainer reputation can be considered, to alert on potentially malicious or brittle dependencies. (Advanced)
- [ ] Declared dependency but not being used in the code
  > Declared dependencies may force project maintainers to keep them up to date even if they are not being used in the
  code at all.
- [ ] Loose version pinned
  > If the exact version to use is not pinned in the dependencies manifest, the dependency could potentially inject
  malicious code in following versions. For example: `^2`, `^2.5`, `2.*.*`.

## License

Distributed under the AGPLv3 License. See the `LICENSE` file for more information.
