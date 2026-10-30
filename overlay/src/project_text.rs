//! Generated from the Python overlay's project.py: the text is copied, not retyped.

pub const DOSSIER_SYSTEM: &str = r#"You study a software project so its author can defend it in a job interview. You get the file tree and the most important files. Write a DOSSIER in Markdown, at most 1,500 words, with exactly these sections:

## Overview
what it is, who uses it and the problem it solves, in three sentences
## Tech stack
each technology and what it is used for here
## Architecture
the main parts and how data moves between them, naming real files and modules
## Key modules
one line per module: path, then what it is responsible for
## How to run and test
the real commands found in the files
## Decisions and trade-offs
what the code chose (frameworks, structure, data model) and what that costs
## Weak spots and improvements
real limits visible in the code and what to do next
## Likely defense questions
twelve questions an interviewer would ask about this project, each with a two-sentence answer in the first person, grounded in the code

Rules
- Use only what the files show. Never invent features, numbers, users, deadlines or history.
- Where the code does not say why something was done, write the most likely reason and mark it (inferred).
- Name real files, classes and functions. No preamble and no closing remarks.
"#;

pub const KEY_FILE: &str = r#"(?:^|/)(?:readme[^/]*|pyproject\.toml|package\.json|pom\.xml|build\.gradle(?:\.kts)?|settings\.gradle(?:\.kts)?|cargo\.toml|go\.mod|requirements[^/]*\.txt|dockerfile|docker-compose[^/]*\.ya?ml|makefile|application\.(?:ya?ml|properties))$"#;

pub const ENTRY_NAMES: &[&str] = &["app", "application", "cli", "index", "main", "manage", "program", "server", "startup"];

pub const ROLE_WORDS: &[&str] = &["api", "route", "controller", "service", "model", "schema", "entity", "repository", "handler", "config", "middleware", "core", "domain", "auth", "store", "db", "view", "component"];

pub const SKIP_DIRS: &[&str] = &[".cache", ".git", ".gradle", ".hg", ".idea", ".mypy_cache", ".next", ".nuxt", ".pytest_cache", ".ruff_cache", ".svn", ".terraform", ".tox", ".venv", ".vscode", "__pycache__", "build", "coverage", "dist", "env", "node_modules", "obj", "out", "site-packages", "target", "vendor", "venv"];

pub const SKIP_SUFFIXES: &[&str] = &[".7z", ".a", ".avi", ".bin", ".bmp", ".class", ".dat", ".db", ".dll", ".dylib", ".eot", ".exe", ".gif", ".gz", ".h5", ".ico", ".jar", ".jpeg", ".jpg", ".lock", ".map", ".mov", ".mp3", ".mp4", ".npy", ".npz", ".o", ".ogg", ".onnx", ".otf", ".parquet", ".pdf", ".pkl", ".png", ".pt", ".pyc", ".pyo", ".rar", ".so", ".sqlite", ".sqlite3", ".svg", ".tar", ".tgz", ".tiff", ".ttf", ".war", ".wav", ".webm", ".webp", ".whl", ".woff", ".woff2", ".zip"];

pub const SKIP_NAMES: &[&str] = &[".ds_store", "cargo.lock", "composer.lock", "gemfile.lock", "package-lock.json", "pnpm-lock.yaml", "poetry.lock", "thumbs.db", "uv.lock", "yarn.lock"];

pub const SECRET_SUFFIXES: &[&str] = &[".jks", ".key", ".keystore", ".p12", ".pem", ".pfx"];

pub const SECRET_NAMES: &[&str] = &["credentials.json", "id_ed25519", "id_rsa", "secrets.json", "secrets.yaml", "secrets.yml"];

pub const ENV_EXAMPLES: &[&str] = &[".env.example", ".env.sample", ".env.template", ".env.dist"];

pub const STOP_WORDS: &[&str] = &["a", "about", "an", "and", "are", "as", "at", "be", "been", "by", "can", "could", "did", "do", "does", "for", "from", "had", "has", "have", "here", "how", "in", "into", "is", "it", "its", "no", "not", "of", "on", "or", "should", "than", "that", "the", "then", "there", "these", "this", "those", "to", "was", "were", "what", "when", "where", "which", "who", "why", "with", "would", "yes", "you", "your"];
