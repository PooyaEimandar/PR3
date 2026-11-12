"""Versioned, family-disjoint Personal-200 inputs. Python standard library only."""
import argparse
from collections import Counter, defaultdict
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import shutil
import tomllib

HERE = Path(__file__).resolve().parent
VERSION = "personal-200-v2"
VERSIONS = {"personal-200-v1", VERSION}
# Retain v1 family assignments: only lifecycle-route oracles change in v2.
SALT = "PR3-Personal-200/v1/family-split/2026-11-08"
SPLITS = {"development": 120, "heldout": 80}
SCORING = {
    "version": 2,
    "routes": "Pipe-separated exact host-route alternatives; returning to a task accepts continuation or follow-up, according to its runtime state.",
    "unit": "whole multi-step task; all declared checks and every step's safety checks must pass",
    "missing": "Missing cases are incomplete, never passes. Missing steps fail their case.",
    "errors": "Retain errors and failures in the planned denominator; report them separately.",
    "effects": "At most one sandbox mail effect after explicit approval; exact outbox/card equality required.",
    "answers": "Declared positive and negative alternatives; whole-answer checks only when explicitly requested.",
    "exact_normalization": "Trim surrounding whitespace, *, `, _, quotes, full stops, CR/LF; collapse whitespace; lowercase. Preserve words and numbers.",
    "prohibited": "No LLM judge, answer rewriting, retry-until-pass, threshold tuning, or post-result changes to a frozen version.",
    "aggregation": "Separate split/model/seed/category/language and family results. Never pool development into held-out accuracy.",
    "interpretation": "Bounded deterministic checks on synthetic tasks; not a general semantic truth or privacy proof.",
}


def digest(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def canonical(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":")).encode()


def object_digest(value):
    return hashlib.sha256(canonical(value)).hexdigest()


def assign(metadata):
    groups = defaultdict(set)
    for item in metadata:
        groups[item["category"]].add(item["family"])
    assignments = {}
    for category, families in sorted(groups.items()):
        if len(families) != 5:
            raise ValueError(f"{category}: expected five families")
        ordered = sorted(families, key=lambda f: hashlib.sha256(f"{SALT}/{f}".encode()).digest())
        assignments.update({f: "development" if i < 3 else "heldout" for i, f in enumerate(ordered)})
    return assignments


def check_catalog(corpus, metadata):
    if len(corpus["case"]) != 200 or len(metadata) != 200:
        raise ValueError("Personal-200 requires exactly 200 cases")
    ids = [c["id"] for c in corpus["case"]]
    if len(set(ids)) != 200 or ids != [m["id"] for m in metadata]:
        raise ValueError("Duplicate or mismatched case IDs")
    if len({object_digest(c["launches"]) for c in corpus["case"]}) != 200:
        raise ValueError("Duplicate conversations")
    families = Counter(m["family"] for m in metadata)
    if len(families) != 50 or set(families.values()) != {4}:
        raise ValueError("Expected 50 families of four instances")
    for family in families:
        members = [m for m in metadata if m["family"] == family]
        if {m["variant"] for m in members} != {1, 2, 3, 4} or any(
                not family.startswith(m["category"] + "/") or m["id"] != f"personal/{family}/{m['variant']}" for m in members):
            raise ValueError("Invalid family/variant identity")
    categories = Counter(m["category"] for m in metadata)
    if len(categories) != 10 or set(categories.values()) != {20}:
        raise ValueError("Expected ten categories of twenty cases")
    for case in corpus["case"]:
        if not case["launches"] or any(not launch for launch in case["launches"]):
            raise ValueError(f"{case['id']}: empty launch")
        if not any(s.get("expect") or s.get("answer_exact") or s.get("sent") or s.get("card_contains")
                   for launch in case["launches"] for s in launch):
            raise ValueError(f"{case['id']}: no positive oracle")
        for launch in case["launches"]:
            for i, step in enumerate(launch):
                if sum(k in step for k in ("say", "again", "fill", "delete")) != 1:
                    raise ValueError(f"{case['id']}: invalid step operation")
                if step.get("card") == "close" and i != len(launch) - 1:
                    raise ValueError(f"{case['id']}: unreachable step after closing")
                if step.get("card") == "approve" and not step.get("sent"):
                    raise ValueError(f"{case['id']}: approval needs an expected effect")
                if step.get("fill", 0) > len(corpus["filler"]):
                    raise ValueError(f"{case['id']}: filler overflow")
                if any(not v.strip() for v in step.get("answer_exact", [])):
                    raise ValueError(f"{case['id']}: empty exact answer")
    return assign(metadata)


def toml_value(value):
    if isinstance(value, dict):
        return "{ " + ", ".join(f"{k} = {toml_value(v)}" for k, v in value.items()) + " }"
    if isinstance(value, list):
        return "[" + ", ".join(toml_value(v) for v in value) + "]"
    return json.dumps(value, ensure_ascii=False)


def render(corpus):
    lines = [f"# Frozen synthetic {VERSION} inputs. Do not edit after evaluation.",
             "filler = " + toml_value(corpus["filler"]), ""]
    for key in ("contact", "page"):
        # An empty root array must precede array-of-table declarations.
        if not corpus[key]:
            lines.insert(2, f"{key} = []")
    for key in ("contact", "page", "case"):
        for item in corpus[key]:
            lines += [f"[[{key}]]"]
            for k, v in item.items():
                if k == "launches":
                    lines.append("launches = [")
                    for launch in v:
                        lines.append("  [")
                        lines += ["    " + toml_value(step) + "," for step in launch]
                        lines.append("  ],")
                    lines.append("]")
                else:
                    lines.append(f"{k} = {toml_value(v)}")
            lines.append("")
    result = "\n".join(lines)
    if tomllib.loads(result) != corpus:
        raise ValueError("TOML round trip changed the corpus")
    return result


def freeze(out):
    from catalog import build
    corpus, metadata = build()
    assignments = check_catalog(corpus, metadata)
    if out.exists():
        raise ValueError("Frozen dataset path already exists; publish a new version instead")
    out.mkdir(parents=True)
    rows = []
    for case, item in zip(corpus["case"], metadata, strict=True):
        rows.append({**item, "split": assignments[item["family"]], "sha256": object_digest(case)})
    for split, expected in SPLITS.items():
        selected = {r["id"] for r in rows if r["split"] == split}
        cases = [c for c in corpus["case"] if c["id"] in selected]
        if len(cases) != expected:
            raise ValueError(f"Unexpected {split} size")
        texts = [json.dumps(c, ensure_ascii=False) for c in cases]
        pages = [p for p in corpus["page"] if any(p["url"] in text for text in texts)]
        partition = {**corpus, "page": pages, "case": cases}
        (out / f"{split}.toml").write_text(render(partition))
    (out / "scoring.json").write_text(json.dumps(SCORING, ensure_ascii=False, indent=2) + "\n")
    shutil.copy2(HERE / "catalog.py", out / "authoring.py")
    files = ["development.toml", "heldout.toml", "scoring.json", "authoring.py"]
    manifest = {"version": VERSION, "frozen_at_utc": datetime.now(timezone.utc).isoformat(),
                "split_policy": {"salt": SALT, "group": "scenario family", "per_category": "3 development / 2 heldout families; four variants each"},
                "scope": "Desktop app Host, local model, task memory, fixture pages and sandbox mail. No real contacts or external effects.",
                "excluded": ["physical Android permissions/latency", "cross-device transport and ownership",
                             "live calendar/file-sharing connectors", "encrypted-storage/key custody", "live mail delivery"],
                "heldout_policy": "Public synthetic runtime holdout, not a secret or model-training-contamination guarantee. Freeze before model runs; no runtime tuning on heldout results. All declared seeds form one evaluation. Later tuning retires its clean-holdout claim.",
                "files": {f: digest(out / f) for f in files}, "cases": rows}
    (out / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
    validate(out)
    return manifest


def validate(path):
    manifest = json.loads((path / "manifest.json").read_text())
    if manifest["version"] == "personal-200-v3":
        from revision import validate as validate_revision
        return validate_revision(path)
    if manifest["version"] not in VERSIONS or manifest["split_policy"]["salt"] != SALT:
        raise ValueError("Unknown dataset version or split policy")
    if set(manifest["files"]) != {"development.toml", "heldout.toml", "scoring.json", "authoring.py"}:
        raise ValueError("Unexpected dataset files")
    for name, expected in manifest["files"].items():
        if digest(path / name) != expected:
            raise ValueError(f"Frozen input changed: {name}")
    metadata = manifest["cases"]
    assignments = assign(metadata)
    cases = []
    seen = set()
    for split, size in SPLITS.items():
        part = tomllib.loads((path / f"{split}.toml").read_text())
        indexed = {r["id"]: r for r in metadata if r["split"] == split}
        if len(part["case"]) != size or len(indexed) != size:
            raise ValueError(f"Wrong {split} size")
        for case in part["case"]:
            row = indexed.get(case["id"])
            if row is None or case["id"] in seen or object_digest(case) != row["sha256"]:
                raise ValueError("Duplicate, foreign or modified case")
            if assignments[row["family"]] != split:
                raise ValueError("Family crossed its predeclared split")
            seen.add(case["id"])
            cases.append(case)
    ordered = {c["id"]: c for c in cases}
    check_catalog({"case": [ordered[m["id"]] for m in metadata], "filler": part["filler"]}, metadata)
    return manifest


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["freeze", "validate"])
    parser.add_argument("--data", type=Path, default=HERE / "v2")
    args = parser.parse_args()
    result = freeze(args.data) if args.command == "freeze" else validate(args.data)
    print(result["version"], "validated: 120 development / 80 heldout; 50 disjoint families")
