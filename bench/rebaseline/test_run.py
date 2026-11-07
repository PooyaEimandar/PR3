import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("rebaseline", Path(__file__).with_name("run.py"))
run = importlib.util.module_from_spec(spec)
spec.loader.exec_module(run)


class Completeness(unittest.TestCase):
    def test_security_version_comes_from_frozen_driver_not_current_source(self):
        with tempfile.TemporaryDirectory() as directory:
            driver = Path(directory) / "run.py"
            for version in (3, 4, 5):
                driver.write_text(f"raise RuntimeError('must not execute')\nHARNESS_VERSION = {version}\n")
                self.assertEqual(run.harness_version(driver), version)
            for source in ("", "HARNESS_VERSION = True", "HARNESS_VERSION = 0", "HARNESS_VERSION = '5'"):
                driver.write_text(source)
                with self.assertRaises(ValueError):
                    run.harness_version(driver)

    def test_complete_rows_from_another_model_are_not_adopted(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "rows.jsonl"
            job = {"kind": "workflow", "seed": 7, "expected": ["a"], "result": str(path), "inputs": {"model_sha256": "expected-model"}}
            path.write_text(json.dumps({"id": "a", "seed": 7, "error": None}) + "\n")
            self.assertFalse(run.job_status(job)["complete"])
            path.with_suffix('.manifest.json').write_text(json.dumps({"model_sha256": "other-model"}))
            self.assertFalse(run.job_status(job)["complete"])
            path.with_suffix('.manifest.json').write_text(json.dumps(job["inputs"]))
            self.assertTrue(run.job_status(job)["complete"])

    def test_resume_requires_every_expected_unique_key_and_seed(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "rows.jsonl"
            job = {"kind": "workflow", "seed": 7, "expected": ["a", "b"], "result": str(path)}
            row = {"id": "a", "seed": 7, "error": None}
            path.write_text(json.dumps(row) + "\n")
            self.assertFalse(run.job_status(job)["complete"])
            for other in [row, {**row, "id": "b", "seed": 1}, {**row, "id": "c"}]:
                path.write_text(json.dumps(row) + "\n" + json.dumps(other) + "\n")
                self.assertIsNotNone(run.job_status(job)["problem"])
                self.assertFalse(run.job_status(job)["complete"])
            path.write_text(json.dumps(row) + "\n" + json.dumps({**row, "id": "b"}) + "\n")
            self.assertTrue(run.job_status(job)["complete"])
            with path.open("a") as file:
                file.write('{"id":')
            self.assertFalse(run.job_status(job)["complete"])

    def test_security_errors_are_unscored_and_visible(self):
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            path = out / "rows.jsonl"
            good = {"mode": "pr3", "user_task": "u1", "injection_task": "a1", "seed": 1, "attack_succeeded": True, "utility": False, "error": None, "wall_seconds": 2}
            error = {**good, "injection_task": "a2", "error": "model crashed", "attack_succeeded": None, "wall_seconds": 3}
            path.write_text(json.dumps(good) + "\n" + json.dumps(error) + "\n")
            job = {"name": "security-test", "kind": "security", "seed": 1, "result": str(path), "expected": [["pr3", "u1", "a1"], ["pr3", "u1", "a2"]]}
            state = run.report(out, {"jobs": [job], "caveats": []})
            self.assertTrue(state["complete"])
            self.assertEqual(state["errors"], 1)
            self.assertIn("ASR 1/1", (out / "report.md").read_text())
            self.assertNotIn("ASR 1/2", (out / "report.md").read_text())

    def test_latency_uses_nearest_rank_and_does_not_invent_empty_measurements(self):
        self.assertIsNone(run.percentile([], .95))
        self.assertEqual(run.percentile([1, 2, 3, 100], .95), 100)
