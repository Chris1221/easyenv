# Changelog

## [0.3.0](https://github.com/Chris1221/easyenv/compare/v0.2.0...v0.3.0) (2026-08-01)


### Features

* **bench:** add shadowenv, mise, and zsh-autoenv to the comparison ([3cd341f](https://github.com/Chris1221/easyenv/commit/3cd341f36b8949e12ba5bb222072d9843b922f5e))
* **release:** allow manually re-uploading assets to an existing tag ([df77243](https://github.com/Chris1221/easyenv/commit/df77243640ca6446100d4a79af6d8f292394ee13))
* **security:** add config-driven denylist/skip-list engine (P0-1) ([96f6286](https://github.com/Chris1221/easyenv/commit/96f6286194fc9294be14c59d7e5b9a979039fbb9))


### Bug Fixes

* make the readme a bit simpler ([a9dab85](https://github.com/Chris1221/easyenv/commit/a9dab85a452cd3067e8c7d2b6047ccf483ad9d96))
* **release:** drop component name from release tags ([5e4f8f0](https://github.com/Chris1221/easyenv/commit/5e4f8f0339d9266714ea34fb28dff7d3a8fd3cc5))
* **release:** pass upload-rust-binary-action a fully-formed tag ref ([2b054fb](https://github.com/Chris1221/easyenv/commit/2b054fb7d823a41c03ebecd6b209f84b7fc6d96d))
* **security:** call easyenv by absolute path from the hook (P0-2) ([90d0224](https://github.com/Chris1221/easyenv/commit/90d0224cca656f1540e28caef0a2d601a886bbf4))
* **security:** make the /tmp-skip test portable to macOS CI ([f516c0c](https://github.com/Chris1221/easyenv/commit/f516c0c96450ab7eafcc023705231fb66baff163))

## [0.2.0](https://github.com/Chris1221/easyenv/compare/easyenv-v0.1.0...easyenv-v0.2.0) (2026-07-31)


### Features

* **bench:** add nesting-depth benchmark vs direnv and autoenv ([2ed71b1](https://github.com/Chris1221/easyenv/commit/2ed71b1031faf3113e6b0f9eb6d8f686d5344005))
* **install:** add curl|bash installer with confirmed rc-file setup ([650cacf](https://github.com/Chris1221/easyenv/commit/650cacf3b7f47bbd1a9d77b776c37e9cbcb1ee77))
* **install:** default Linux installs to the static musl build ([65f93e4](https://github.com/Chris1221/easyenv/commit/65f93e4ef296b37c09f8e5280fabf96fa72044f2))
* **release:** cross-compile and publish binaries on tagged releases ([b93b375](https://github.com/Chris1221/easyenv/commit/b93b375ba9291b66069c660ab3d25ca46fd8c1b4))


### Bug Fixes

* **release:** use upload-rust-binary-action's own $tag placeholder ([3d098b7](https://github.com/Chris1221/easyenv/commit/3d098b7b652a5ea42bf95f1c771e2f14102068af))
