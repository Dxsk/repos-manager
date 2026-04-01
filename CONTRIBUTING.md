# Contributing

Thanks for your interest in contributing to repos-manager!

For the full contributing guide (setup, testing, adding providers, PR rules), see the documentation:

https://repos-manager.dxscloud.fr/docs/contributing/

## Quick start

```bash
git clone git@github.com:Dxsk/repos-manager.git
cd repos-manager
bats tests/*.bats        # Run tests
shellcheck -x repos-manager.sh lib/*.sh sourceme.bash  # Lint
```

Branch from `develop`, one feature per PR, tests must pass.
