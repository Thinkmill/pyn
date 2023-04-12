# pyn

Neat little package manager helper for JavaScript projects.

@emmatown and @JedWatson's rust experiment, generally undocumented,
pretty handy in your terminal.

## Roadmap

### Add `upgrade {packages}`

- [ ] Find usages of package
  - Show current version vs. latest version
  - Upgrade everywhere to latest version
  - Warn about lockfiles (no action, just reminder)
- [ ] Remove entries for that dependency from lockfile

### Add `upgrade --all | --interactive`

- [ ] Find new versions of ALL the packages
- [ ] With `--interactive` offer to upgrade all or selected
- [ ] Upgrade all or selected packages everywhere

### Improve `add {packages}`

- [ ] Find existing usage of the package, and offer to
  - Use existing version
  - Upgrade existing version

### Improve `remove {packages}`

- [ ] Remove everywhere
  - List usage of dependency in packages
  - Replace flag with prompt if the package exists elsewhere
