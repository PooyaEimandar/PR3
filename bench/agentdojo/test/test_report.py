"""The report generator, on made-up results and AgentDojo's real task definitions:
.venv/bin/python -m unittest discover test"""

import tempfile
import unittest
from pathlib import Path

import report
from report import BEGIN, END, Kinds, kinds, model_name, summarize, update, versions


def record(mode, user, attack=None, utility=True, succeeded=None, fooled=None, approvals=0):
    return {
        "mode": mode,
        "user_task": user,
        "injection_task": attack,
        "utility": utility,
        "attack_succeeded": succeeded,
        "model_attempted_attack": fooled,
        "stats": {"approvals_asked": approvals},
        "approver": 2,
        "harness": 2,
    }


KIND = Kinds(
    gaps=frozenset({("u2", "a1")}),
    text_only=frozenset({"a2"}),
    user_tasks=2,
    injection_tasks=2,
)


class Summary(unittest.TestCase):
    def test_rates_and_the_kinds_of_successful_attack(self):
        records = [
            record("plain", "u1"),
            record("plain", "u2", utility=False),
            record("plain", "u1", "a1", succeeded=True, fooled=True),
            record("plain", "u2", "a1", succeeded=True, fooled=True),
            record("plain", "u1", "a2", succeeded=True, fooled=False),
            record("plain", "u2", "a2", utility=False, succeeded=False, fooled=False),
            record("pr3", "u1", approvals=1),
            record("pr3", "u1", "a1", succeeded=False, fooled=True, approvals=2),
        ]
        pr3, plain = summarize(records, KIND)
        self.assertEqual((plain.arm, plain.runs), ("plain", 6))
        self.assertEqual((plain.benign, plain.attacked, plain.fooled, plain.succeeded), (50.0, 75.0, 50.0, 75.0))
        # u2×a1 is an approver gap, a2 a text-only attack, u1×a1 neither.
        self.assertEqual((plain.text_only, plain.in_gaps), (1, 1))
        self.assertEqual((pr3.succeeded, pr3.approvals), (0.0, 1.5))

    def test_versions_must_agree(self):
        self.assertEqual(versions([record("pr3", "u1"), record("plain", "u1")]), (2, 2))
        # The plain loop ran the same under every harness; its older rows record none.
        older_plain = dict(record("plain", "u1"), harness=None)
        self.assertEqual(versions([record("pr3", "u1"), older_plain]), (2, 2))
        older_pr3 = dict(record("pr3", "u1"), harness=None)
        with self.assertRaises(ValueError):
            versions([record("pr3", "u1"), older_pr3])
        with self.assertRaises(ValueError):
            versions([record("pr3", "u1"), dict(record("plain", "u1"), approver=1)])

    def test_model_names_drop_the_quantization(self):
        self.assertEqual(model_name("banking-Qwen3.8-27B-Q4_K_M"), "Qwen3.8-27B")
        self.assertEqual(model_name("travel-Qwen3.5-4B-Q4_K_M"), "Qwen3.5-4B")


class TaskKinds(unittest.TestCase):
    def test_text_only_attacks_and_gaps_come_from_the_task_definitions(self):
        travel = kinds("travel")
        self.assertEqual(travel.text_only, {"injection_task_6"})
        self.assertIn(("user_task_3", "injection_task_5"), travel.gaps)
        slack = kinds("slack")
        self.assertEqual(slack.text_only, frozenset())
        self.assertEqual(
            {pair for pair in slack.gaps},
            {("user_task_1", "injection_task_1"), ("user_task_14", "injection_task_1"), ("user_task_18", "injection_task_1")},
        )
        self.assertEqual((kinds("banking").gaps, kinds("banking").text_only), (frozenset(), frozenset()))


class Update(unittest.TestCase):
    def test_only_the_generated_section_changes(self):
        with tempfile.TemporaryDirectory() as tmp:
            doc = Path(tmp) / "doc.md"
            doc.write_text(f"intro\n{BEGIN}\nold\n{END}\noutro\n")
            update(doc, "new")
            self.assertEqual(doc.read_text(), f"intro\n{BEGIN}\n\nnew\n\n{END}\noutro\n")
            doc.write_text("no markers")
            with self.assertRaises(ValueError):
                update(doc, "new")


if __name__ == "__main__":
    unittest.main()
