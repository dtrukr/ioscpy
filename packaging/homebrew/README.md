# Homebrew formula for the ioscpy fork

This formula builds the macOS host from the public
[dtrukr/ioscpy](https://github.com/dtrukr/ioscpy) fork. It is pinned to a tested
protocol 5 commit and uses a verified source archive checksum. The matching
device package must also come from this fork; the upstream tap and Sileo package
use protocol 4.

Install from the published tap:

```bash
brew tap dtrukr/ioscpy
brew install dtrukr/ioscpy/ioscpy
ioscpy --version
```

Updates use `brew upgrade dtrukr/ioscpy/ioscpy`. The main
[README](../../README.md) also documents a direct `make install-host`
installation. Choose one installation method per Homebrew prefix.

The same formula is published in
[dtrukr/homebrew-ioscpy](https://github.com/dtrukr/homebrew-ioscpy) and must be
updated there whenever this copy changes.

To update this formula, first push and test a fork commit. Replace the commit
in `url`, set `version`, and compute the checksum of the exact archive:

```bash
curl -fLsS https://github.com/dtrukr/ioscpy/archive/<commit>.tar.gz \
  | shasum -a 256
```

The `head` entry builds the current `main` branch when installed with Homebrew's
`--HEAD` option.
