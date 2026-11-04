import json
import tempfile
import unittest
from pathlib import Path

from pr3_agentdojo.manifest import completed, ensure


class Resume(unittest.TestCase):
    def test_identity_round_trip_and_mismatched_seed_model_or_harness(self):
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory) / "run.jsonl"
            config = {"seed": 1, "model": "a", "harness": 3, "expected": [("pr3", "u1", None)]}
            run_id = ensure(out, config)
            self.assertEqual(run_id, ensure(out, config))
            for key, value in [("seed", 7), ("model", "b"), ("harness", 4)]:
                with self.assertRaisesRegex(ValueError, "Incompatible resume"):
                    ensure(out, {**config, key: value})

    def test_unknown_output_duplicates_foreign_and_truncated_rows_are_refused(self):
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory) / "run.jsonl"
            row = {"mode": "pr3", "user_task": "u1", "injection_task": None, "run_id": "known"}
            expected = {("pr3", "u1", None)}
            out.write_text(json.dumps(row) + "\n")
            with self.assertRaisesRegex(ValueError, "Unidentified"):
                ensure(out, {})
            self.assertEqual(completed(out, "known", expected), expected)
            for text in [json.dumps(row) + "\n" + json.dumps(row), json.dumps({**row, "run_id": "other"}), json.dumps({**row, "user_task": "u2"}), '{"mode":']:
                out.write_text(text)
                with self.assertRaises(ValueError):
                    completed(out, "known", expected)
