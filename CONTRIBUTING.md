# Contributing to CareerTwin AI

## Getting Started

1. Fork the repository.
2. Create a branch: `git checkout -b feature/your-feature`
3. Install dependencies: `uv sync --extra dev --extra rendering`
4. Make your changes.
5. Run tests: `uv run pytest tests/ -m "not integration"`
6. Run linting: `uv run ruff check src/ tests/`
7. Commit: `git commit -m "feat: your change"`
8. Push: `git push origin feature/your-feature`
9. Open a Pull Request.

## Development

```bash
make api       # FastAPI backend with reload on http://127.0.0.1:8000
make test      # unit tests
make lint      # ruff
make docker    # backend + SearXNG in containers
```

## Code Style

- Python: PEP 8, type hints required, formatted by Ruff
- Absolute imports from the `careertwin` package; no `sys.path` changes
- No inline comments in source files
- Conventional commits: `feat:`, `fix:`, `refactor:`, `chore:`, `docs:`, `test:`

## Releases

Releases are automatic. Every push to `master` runs *Auto release* (`.github/workflows/autorelease.yml`), which reads the commit titles since the last `vX.Y.Z` tag with `scripts/next_release.py`:

| Since the last release | Result |
|---|---|
| a `feat:` commit | minor release (`0.1.2` → `0.2.0`) |
| only `fix:` or `perf:` | patch release (`0.1.2` → `0.1.3`) |
| a breaking change (`feat!:`, or `BREAKING CHANGE:` in the body) | minor while the major version is 0, major after |
| only `docs:`, `chore:`, `test:`, `ci:`, `style:`, `refactor:`, `build:` | no release |

When there is something to release the workflow writes the version into `pyproject.toml` and `uv.lock` in a `chore: release vX.Y.Z` commit, tags it and starts *Release* on the tag. Run `python3 scripts/next_release.py` to see what the next push would do (it changes nothing without `--apply`). To skip one release put `[skip release]` in a commit message; to stop them altogether set the repository variable `AUTO_RELEASE` to `false` (Settings → Secrets and variables → Actions → Variables). Raising the version in `pyproject.toml` by hand and merging it also releases exactly that version. The workflow pushes the version commit to `master` with the workflow token, so it needs a `master` that the token may push to (a branch protection rule must let GitHub Actions bypass it).

*Release* builds the portable Windows zip, a Windows setup program (`.exe`, Inno Setup, built on a Windows runner) and a macOS disk image (`.dmg`, built on a macOS runner), and attaches them to a GitHub release together with `SHA256SUMS.txt`. Before publishing, it installs the setup on its Windows machine, imports the packages and draws the window from the installed program, then uninstalls it; on the macOS machine it mounts the dmg, checks the signature and does the same from the app inside. Each build takes about ten minutes and the macOS minutes cost ten times as much on a private repository, so a flood of `fix:` commits is a flood of releases: batch them, or use `[skip release]`.

You can still release by hand: set the version in `pyproject.toml`, merge it, then `git tag v0.2.0 && git push origin v0.2.0`. The build fails if the tag and the `pyproject.toml` version differ. A tag with a hyphen (`v0.2.0-rc1`) is marked as a pre-release and is never the base of an automatic version. To get the files without releasing, run *Release* by hand from the Actions tab and download the artifacts `windows-zip`, `windows-setup` and `macos-dmg` (kept for 14 days). A change to `packaging/` or the build scripts runs the same build on a pull request without publishing. Locally, `make windows` makes the zip and `./scripts/build_macos.sh` the Mac app and dmg (on a Mac; `SKIP_NATIVE=1` stops before the icon, signing and dmg, which only macOS can do). Neither installer is signed, which is why Windows and macOS warn on the first start.

## Project Structure

All code lives in the `careertwin` package under `src/`:

- `api/` — FastAPI app (`app.py`), request models (`schemas.py`), one module per feature in `routes/`
- `automation/` — four-agent application workflow: search and rank, prepare documents, apply, track
- `jobs/` — job discovery: ATS adapters (`boards/`), scanners, career-page scrapers, `data/companies.json`
- `profile/` — candidate context (`ABOUT.md`), career graph building, profile names
- `services/` — resume tailoring and cover-letter service used by the API
- `agents/`, `ats/`, `collectors/`, `graph/`, `rendering/`, `storage/`, `templates/`, `vision/` — the document-building library
- `settings.py`, `db.py`, `llm.py`, `browser.py`, `net.py` — configuration, SQLite, LLM providers, agent-browser helpers, URL safety
- `cli.py` — the `careertwin` command
