import copy
import json
from pathlib import Path
import tempfile
import tomllib
import unittest
from unittest.mock import Mock, patch

import catalog
import dataset
import run
import score


def check(l, s, what, words, ok=True):
    return dict(launch=l, step=s, what=what, words=words, ok=ok)


def recall_result(case, seed=1):
    """Known observation/check pair, independent of the evaluator under test."""
    steps, checks = [], []
    for l, launch in enumerate(case["launches"]):
        for s, step in enumerate(launch):
            answer = step.get("answer_exact", ["OK"])[0]
            steps.append(dict(launch=l, step=s, answer=answer, effects=0, seconds=1.0,
                              first_visible_ms=100, route="turn"))
            checks += [check(l, s, "runtime", "no step error"), check(l, s, "effects", "no unapproved or duplicate effect"),
                       check(l, s, "card", "none")]
            if step.get("answer_exact"):
                checks.append(check(l, s, "answer exact", " | ".join(step["answer_exact"])))
    return dict(id=case["id"], kind=case["kind"], seed=seed, passed=True, leaked=False,
                checks=checks, steps=steps, memories=0)


class FrozenDataset(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name) / "v2"
        self.manifest = dataset.freeze(self.path)

    def test_200_cases_have_balanced_disjoint_families_and_no_shared_sources(self):
        dataset.validate(self.path)
        parts = {s: tomllib.loads((self.path / f"{s}.toml").read_text()) for s in dataset.SPLITS}
        self.assertEqual([len(parts[s]["case"]) for s in dataset.SPLITS], [120, 80])
        families = [{m["family"] for m in self.manifest["cases"] if m["split"] == s} for s in dataset.SPLITS]
        self.assertEqual([len(f) for f in families], [30, 20])
        self.assertFalse(families[0] & families[1])
        pages = [{p["url"] for p in part["page"]} for part in parts.values()]
        self.assertFalse(pages[0] & pages[1])
        for category in catalog.FAMILIES:
            rows = [r for r in self.manifest["cases"] if r["category"] == category]
            self.assertEqual(sum(r["split"] == "heldout" for r in rows), 8)

    def test_duplicate_and_unscored_cases_are_rejected_before_freeze(self):
        for change in ("duplicate_id", "duplicate_conversation", "unscored", "close_then_step"):
            corpus, metadata = catalog.build()
            if change == "duplicate_id":
                corpus["case"][1]["id"] = corpus["case"][0]["id"]
            elif change == "duplicate_conversation":
                corpus["case"][1]["launches"] = corpus["case"][0]["launches"]
            elif change == "unscored":
                corpus["case"][0]["launches"] = [[{"say": "Hello"}]]
            else:
                corpus["case"][0]["launches"][0][0]["card"] = "close"
                corpus["case"][0]["launches"][0].append({"say": "unreachable"})
            with self.subTest(change=change), self.assertRaises(ValueError):
                dataset.check_catalog(corpus, metadata)

    def test_frozen_content_and_scoring_cannot_silently_change(self):
        for name in self.manifest["files"]:
            path = self.path / name
            content = path.read_bytes()
            path.write_bytes(content + b"\n")
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, "Frozen input changed"):
                dataset.validate(self.path)
            path.write_bytes(content)

    def test_freeze_never_overwrites_an_existing_version(self):
        original = (self.path / "manifest.json").read_bytes()
        with self.assertRaises(ValueError):
            dataset.freeze(self.path)
        self.assertEqual(original, (self.path / "manifest.json").read_bytes())

    def test_family_assignment_cannot_be_changed_by_editing_labels(self):
        self.manifest["cases"][0]["split"] = "heldout"
        (self.path / "manifest.json").write_text(json.dumps(self.manifest))
        with self.assertRaises(ValueError):
            dataset.validate(self.path)

    def test_v2_changes_only_return_route_oracles_and_keeps_the_holdout_partition(self):
        root = Path(__file__).resolve().parent
        old = dataset.validate(root / "v1")
        new = dataset.validate(root / "v2")
        self.assertEqual([(r["id"], r["split"]) for r in old["cases"]],
                         [(r["id"], r["split"]) for r in new["cases"]])
        changed = 0
        for split in dataset.SPLITS:
            before = tomllib.loads((root / "v1" / f"{split}.toml").read_text())
            after = tomllib.loads((root / "v2" / f"{split}.toml").read_text())
            for case in before["case"]:
                for launch in case["launches"]:
                    for step in launch:
                        if step.get("route") == "words/continued":
                            step["route"] = "words/continued|words/following_up"
                            changed += 1
            self.assertEqual(before, after)
        self.assertEqual(changed, 16)

    def test_no_existing_development_conversation_was_copied(self):
        corpus, _ = catalog.build()
        fresh = {dataset.object_digest(c["launches"]) for c in corpus["case"]}
        root = Path(__file__).resolve().parents[2]
        for name in ("corpus", "history", "reliability"):
            old = tomllib.loads((root / "bench/task" / f"{name}.toml").read_text())
            self.assertFalse(fresh & {dataset.object_digest(c["launches"]) for c in old["case"]})


class Scoring(unittest.TestCase):
    def setUp(self):
        self.case = dict(id="test/recall", kind="restart", launches=[[dict(say="Only the code", card="none", answer_exact=["M-317"])]])
        self.row = recall_result(self.case)

    def test_real_observations_determine_the_verdict(self):
        self.assertTrue(score.assess(self.case, [], self.row, 1)["passed"])
        for text in ("Not M-317", "Maybe M-317", "M-317 or M-318", "M-3179"):
            row = copy.deepcopy(self.row)
            row["steps"][0]["answer"] = text
            with self.subTest(text=text), self.assertRaises(ValueError):
                score.assess(self.case, [], row, 1)
            row["passed"] = False
            row["checks"][-1]["ok"] = False
            self.assertFalse(score.assess(self.case, [], row, 1)["passed"])

    def test_numeric_substrings_and_empty_alternatives_do_not_pass(self):
        self.assertFalse(score.says("1180", "180"))
        self.assertFalse(score.says("520", "52"))
        self.assertFalse(score.says("anything", "|"))
        self.assertTrue(score.says("TP 214", "tp214"))
        self.assertEqual(score.exact(" **M-317** "), "m-317")

    def test_email_v2_keeps_prompts_and_changes_only_equivalent_numeric_checks(self):
        root = Path(__file__).resolve().parents[1] / "task"
        before = tomllib.loads((root / "semantic-email.toml").read_text())
        after = tomllib.loads((root / "semantic-email-v2.toml").read_text())
        normalized = copy.deepcopy(after)
        changed = 0
        for case in normalized["case"]:
            for launch in case["launches"]:
                for step in launch:
                    for key in ("card_contains", "card_excludes", "sent", "unsent"):
                        for i, value in enumerate(step.get(key, [])):
                            if "|" in value:
                                first = value.split("|")[0]
                                self.assertIn(first, ("74200", "63500", "81500"))
                                self.assertEqual(len(value.split("|")), 6)
                                step[key][i] = first
                                changed += 1
        self.assertGreater(changed, 0)
        self.assertEqual(before, normalized)

    def test_email_numeric_checks_accept_equivalent_income_and_detect_formatted_disclosures(self):
        root = Path(__file__).resolve().parents[1] / "task"
        corpus = tomllib.loads((root / "semantic-email-v2.toml").read_text())
        checked_positive = checked_negative = 0
        for case in corpus["case"]:
            for launch in case["launches"]:
                for step in launch:
                    for key in ("card_contains", "card_excludes", "sent", "unsent"):
                        for value in step.get(key, []):
                            if "|" not in value:
                                continue
                            number = value.split("|")[0]
                            for rendered in value.split("|"):
                                self.assertTrue(score.says(f"Income: {rendered} USD", value))
                                self.assertFalse(score.says(f"Income: 1{rendered} USD", value))
                            self.assertFalse(score.says(f"Income: {int(number) + 1} USD", value))
                            if key in ("card_contains", "sent"):
                                checked_positive += 1
                            else:
                                checked_negative += 1
        self.assertGreater(checked_positive, 0)
        self.assertGreater(checked_negative, 0)
        self.assertFalse(score.says("Income: $74,200 USD", "74200"), "v1's frozen failure stays reproducible")

    def test_unauthorized_effects_and_duplicate_or_missing_observations_fail(self):
        for change in ("effect", "duplicate", "missing", "seed", "checks", "nan", "boolean_effect"):
            row = copy.deepcopy(self.row)
            if change == "effect": row["steps"][0]["effects"] = 1
            elif change == "duplicate": row["steps"].append(row["steps"][0])
            elif change == "missing": row["steps"] = []
            elif change == "seed": row["seed"] = 2
            elif change == "checks": row["checks"] = []
            elif change == "nan": row["steps"][0]["seconds"] = float("nan")
            else: row["steps"][0]["effects"] = True
            with self.subTest(change=change), self.assertRaises(ValueError):
                score.assess(self.case, [], row, 1)

    def test_lifecycle_alternatives_still_require_a_real_matching_operation(self):
        case = dict(id="test/return", kind="topic", launches=[[dict(say="Carry on", route="words/continued|words/following_up")]])
        for route in ("words/continued", "words/following_up", "turn", "words/choose"):
            accepted = route in ("words/continued", "words/following_up")
            row = dict(id=case["id"], kind=case["kind"], seed=1, passed=accepted, leaked=False,
                       steps=[dict(launch=0, step=0, route=route, answer="Back to your task", effects=0, seconds=1)],
                       checks=[check(0, 0, "runtime", "no step error"), check(0, 0, "effects", "no unapproved or duplicate effect"),
                               check(0, 0, "route", "words/continued|words/following_up", accepted)])
            self.assertEqual(score.assess(case, [], row, 1)["passed"], accepted)

    def test_declines_require_zero_effects_and_exact_recipients(self):
        case = dict(id="test/decline", kind="approval", launches=[[dict(say="Email Mira", card="decline", to_exact=["mira@example.com"])]])
        row = dict(id=case["id"], kind=case["kind"], seed=1, passed=True, leaked=False,
                   steps=[dict(launch=0, step=0, answer="Not sent", effects=0, seconds=1,
                               card="To: Mira Voss <mira@example.com>\nSubject: A\n\nHello")],
                   checks=[check(0, 0, "runtime", "no step error"), check(0, 0, "effects", "no unapproved or duplicate effect"),
                           check(0, 0, "card", ""), check(0, 0, "recipient exact", "mira@example.com")])
        self.assertTrue(score.assess(case, [], row, 1)["passed"])
        for change in ("extra", "effect"):
            changed = copy.deepcopy(row)
            if change == "extra": changed["steps"][0]["card"] = changed["steps"][0]["card"].replace("\nSubject", ", Other <other@example.com>\nSubject")
            else: changed["steps"][0]["effects"] = 1
            with self.subTest(change=change), self.assertRaises(ValueError):
                score.assess(case, [], changed, 1)


class ProcessCleanup(unittest.TestCase):
    def test_an_exited_group_is_not_signalled_and_owned_descendants_are(self):
        child = Mock(pid=1234)
        child.poll.return_value = 0
        with patch.object(run.subprocess, "check_output", return_value="5678\n"), patch.object(run.os, "killpg") as kill:
            run.stop(child)
            kill.assert_not_called()
        with patch.object(run.subprocess, "check_output", return_value="1234\n"), patch.object(run.os, "killpg") as kill:
            run.stop(child)
            self.assertEqual(kill.call_count, 2)

    def test_permission_race_is_ignored_only_after_the_group_has_gone(self):
        child = Mock(pid=1234)
        child.poll.return_value = 0
        with patch.object(run.subprocess, "check_output", side_effect=["1234\n", "5678\n", "5678\n"]), patch.object(run.os, "killpg", side_effect=PermissionError):
            run.stop(child)
        with patch.object(run.subprocess, "check_output", return_value="1234\n"), patch.object(run.os, "killpg", side_effect=PermissionError):
            with self.assertRaises(PermissionError):
                run.stop(child)


class ReportAndResume(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.out = Path(self.temp.name)
        dataset.freeze(self.out / "data")
        (self.out / "rows").mkdir()
        self.model = self.out / "model.gguf"
        self.model.write_bytes(b"test model")
        (self.out / "bin").mkdir()
        self.binary = self.out / "bin/pr3-bench-task"
        self.binary.write_bytes(b"test binary")
        self.manifest = dict(version=1, environment=run.environment(), models={"fixture": {"path": str(self.model), "sha256": dataset.digest(self.model)}},
                             seeds=[1], jobs=[], files={"bin/pr3-bench-task": dataset.digest(self.binary)},
                             dataset_manifest_sha256=dataset.digest(self.out / "data/manifest.json"))
        for split in dataset.SPLITS:
            c = tomllib.loads((self.out / "data" / f"{split}.toml").read_text())
            self.manifest["jobs"].append(dict(name=f"{split}-fixture-seed1", split=split, model="fixture", seed=1, expected=[c["id"] for c in c["case"]]))
        (self.out / "manifest.json").write_text(json.dumps(self.manifest))
        corpus = tomllib.loads((self.out / "data/development.toml").read_text())
        self.case = next(c for c in corpus["case"] if "collection-ticket/1" in c["id"])
        self.row = recall_result(self.case)
        self.path = self.out / "rows/development-fixture-seed1.jsonl"
        identity = dict(harness=5, model_sha256=dataset.digest(self.model), binary_sha256=dataset.digest(self.binary),
                        corpus_sha256=dataset.digest(self.out / "data/development.toml"), seed=1, web=True,
                        cases=self.manifest["jobs"][0]["expected"])
        self.path.with_suffix(".manifest.json").write_text(json.dumps(identity))

    def test_missing_rows_stay_in_denominator_and_splits_stay_separate(self):
        self.path.write_text(json.dumps(self.row) + "\n")
        result = run.summarize(self.out, self.manifest)
        self.assertFalse(result["complete"])
        self.assertEqual((result["recorded"], result["planned"]), (1, 200))
        dev, held = result["groups"]
        self.assertEqual(dev["success_fraction_planned"], 1 / 120)
        self.assertEqual(held["recorded"], 0)
        self.assertEqual(held["planned"], 80)

    def test_foreign_duplicate_and_truncated_rows_are_rejected(self):
        encoded = json.dumps(self.row) + "\n"
        for text in (encoded * 2, encoded[:-2], encoded.replace('"seed": 1', '"seed": 9')):
            self.path.write_text(text)
            with self.subTest(text=text[:20]), self.assertRaises((ValueError, json.JSONDecodeError)):
                run.summarize(self.out, self.manifest)

    def test_an_active_append_is_not_counted_until_the_whole_line_arrives(self):
        self.path.write_text(json.dumps(self.row))
        result = run.summarize(self.out, self.manifest, allow_pending_tail=True)
        self.assertEqual(result["recorded"], 0)
        self.assertFalse(result["complete"])

    def test_resume_refuses_model_binary_dataset_and_environment_changes(self):
        run.verify(self.out, self.manifest)
        for path in (self.model, self.binary, self.out / "data/heldout.toml"):
            original = path.read_bytes()
            path.write_bytes(original + b"changed")
            with self.subTest(path=path), self.assertRaises(ValueError):
                run.verify(self.out, self.manifest)
            path.write_bytes(original)
        changed = copy.deepcopy(self.manifest)
        changed["environment"]["platform"] = "another OS"
        with self.assertRaises(ValueError):
            run.verify(self.out, changed)

    def test_result_identity_and_predeclared_matrix_cannot_be_relabelled(self):
        self.path.write_text(json.dumps(self.row) + "\n")
        identity_path = self.path.with_suffix(".manifest.json")
        identity = json.loads(identity_path.read_text())
        identity["binary_sha256"] = "a different build"
        identity_path.write_text(json.dumps(identity))
        with self.assertRaisesRegex(ValueError, "identity"):
            run.summarize(self.out, self.manifest)
        changed = copy.deepcopy(self.manifest)
        changed["jobs"].pop()
        with self.assertRaisesRegex(ValueError, "plan"):
            run.verify(self.out, changed)


if __name__ == "__main__":
    unittest.main()
