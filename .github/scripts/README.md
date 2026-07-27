# CI helper scripts

Shell helpers invoked by the workflows and composite actions under
`.github/`. Scripts here may use GitHub Actions workflow commands (e.g.
`::error::`, `::warning::`) and assume they run on a GitHub-hosted runner
with the repository checked out at `$GITHUB_WORKSPACE`.

- `apt-retry.sh` — `apt-get update` + `install` wrapped in a bounded retry, so
  transient Ubuntu mirror failures don't fail an otherwise-healthy job.
