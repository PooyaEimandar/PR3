"""Immutable run identity; refuse to resume a different model, seed or harness."""
import hashlib
import json
import os
from pathlib import Path


def sha256(path: Path) -> str:
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def identity(config: dict) -> str:
    return hashlib.sha256(json.dumps(config, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def ensure(path: Path, config: dict) -> str:
    """An output without a matching manifest is never silently adopted."""
    manifest = path.with_suffix(path.suffix + ".manifest.json")
    wanted = json.loads(json.dumps({"run_id": identity(config), "config": config}))
    if manifest.exists():
        if json.loads(manifest.read_text()) != wanted:
            raise ValueError(f"Incompatible resume: {manifest}; use a new output path")
    else:
        if path.exists() and path.stat().st_size:
            raise ValueError(f"Unidentified existing results: {path}; use a new output path")
        # Exclusive creation: do not replace a manifest another runner established.
        with manifest.open("x") as f:
            json.dump(wanted, f, indent=2, sort_keys=True)
            f.write("\n")
            f.flush()
            os.fsync(f.fileno())
    return wanted["run_id"]


def completed(path: Path, run_id: str, expected: set[tuple]) -> set[tuple]:
    done = set()
    if path.exists():
        for line in path.read_text().splitlines():
            row = json.loads(line)  # A truncated record needs explicit repair, never hiding.
            key = (row["mode"], row["user_task"], row["injection_task"])
            if row.get("run_id") != run_id or key not in expected or key in done:
                raise ValueError(f"Foreign, unexpected or duplicate result in {path}: {key}")
            done.add(key)
    return done
