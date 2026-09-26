# Homebrew formula for the ioscpy fork

This formula builds the macOS host from the public
[dtrukr/ioscpy](https://github.com/dtrukr/ioscpy) fork. It is pinned to a tested
protocol 5 commit and uses a verified source archive checksum. The matching
device package must also come from this fork; the upstream tap and Sileo package
use protocol 4.

Install the formula from a clone of this repository:

```bash
brew install --build-from-source ./packaging/homebrew/ioscpy.rb
ioscpy --version
```

The main [README](../../README.md) also documents a direct `make install-host`
installation. Do not install both into the same Homebrew prefix.

To update this formula, first push and test a fork commit. Replace the commit
in `url`, set `version`, and compute the checksum of the exact archive:

```bash
curl -fLsS https://github.com/dtrukr/ioscpy/archive/<commit>.tar.gz \
  | shasum -a 256
```

The `head` entry builds the current `main` branch when installed with Homebrew's
`--HEAD` option.
