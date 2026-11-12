"""Versioning, scoring boundaries, new-family separation and complete run planning."""
import copy
import json
from pathlib import Path
import tempfile
import tomllib
import unittest

import dataset
import fresh
import revision
import run
import score


class Revision(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.path = Path(self.temp.name) / 'v3'
        self.manifest = revision.freeze(self.path)

    def test_all_groups_are_distinct_and_only_new_families_are_held_out(self):
        self.assertEqual(run.split_sizes(dataset.validate(self.path)), revision.PARTITIONS)
        self.assertEqual(sum(revision.PARTITIONS.values()) * 2 * 3, 1494)
        rows = self.manifest['cases']
        new = [r for r in rows if r['split'] == 'fresh-heldout']
        self.assertEqual(len(new), 40)
        self.assertEqual(len({r['family'] for r in new}), 20)
        self.assertEqual(len({r['family'] for r in rows}), 79)
        self.assertFalse({r['family'] for r in new} & {r['family'] for r in rows if r not in new})
        old_signatures = set()
        for split in dataset.SPLITS:
            corpus = tomllib.loads((dataset.HERE / 'v2' / f'{split}.toml').read_text())
            old_signatures.update(dataset.object_digest(c['launches']) for c in corpus['case'])
        new_corpus, _ = fresh.build()
        self.assertFalse(old_signatures & {dataset.object_digest(c['launches']) for c in new_corpus['case']})

    def test_only_declared_numeric_checks_and_eight_deferred_requests_change(self):
        changes = 0
        for old, new in [('development', 'regression-development'), ('heldout', 'regression-prior-heldout')]:
            before = tomllib.loads((dataset.HERE / 'v2' / f'{old}.toml').read_text())
            after = tomllib.loads((self.path / f'{new}.toml').read_text())
            expected = copy.deepcopy(before)
            for case in expected['case']:
                for launch in case['launches']:
                    for step in launch:
                        for key in revision.CHECKS:
                            if key in step:
                                step[key] = ['|'.join(revision.numeric_forms(v)) for v in step[key]]
                        if 'answer_exact' in step:
                            step['answer_exact'] = [a for v in step['answer_exact'] for a in revision.numeric_forms(v)]
                family = '/'.join(case['id'].split('/')[1:-1])
                actual = next(c for c in after['case'] if c['id'] == case['id'])
                if family in ('approval/defer-until-requested', 'interruption/resume-deferred-draft'):
                    text = actual['launches'][-1][0]['say']
                    self.assertIn('reference recorded earlier in this task', text)
                    self.assertNotIn('PX-', text)
                    self.assertFalse(any(c['name'] in text for c in before['contact']))
                    case['launches'][-1][0]['say'] = text
                    changes += 1
            self.assertEqual(expected, after)
        self.assertEqual(changes, 8)
        email = tomllib.loads((dataset.HERE.parent / 'task/semantic-email-v2.toml').read_text())
        email.setdefault('page', [])
        self.assertEqual(email, tomllib.loads((self.path / 'email-regression.toml').read_text()))
        # Parent versions and their stored hashes still validate unchanged.
        dataset.validate(dataset.HERE / 'v1'); dataset.validate(dataset.HERE / 'v2')

    def test_numeric_alternatives_preserve_exactness_and_negative_checks(self):
        forms = revision.numeric_forms('6494')
        self.assertEqual(forms, ['6494', '6,494', '6 494'])
        for value in ('PX-6494', '0501234567', '6.494', '6494.5', '12|13'):
            self.assertEqual(revision.numeric_forms(value), [value])
        for value in forms:
            self.assertTrue(score.says(f'Value: {value}', '|'.join(forms)))
            self.assertIn(score.exact(value), [score.exact(v) for v in forms])
        for text in ('Not 6494', 'Maybe 6,494', '6494 or 6495', 'The value is 6494'):
            self.assertNotIn(score.exact(text), [score.exact(v) for v in forms])
        for text in ('16494', '16,494', '64940', '6495'):
            self.assertFalse(score.says(text, '|'.join(forms)))
        self.assertFalse(score.says('6,494', '6494'), 'old frozen scorer behaviour remains reproducible')

    def test_frozen_files_and_group_identity_cannot_change(self):
        for name in self.manifest['files']:
            p = self.path / name; original = p.read_bytes(); p.write_bytes(original + b'\n')
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, 'Frozen input changed'):
                revision.validate(self.path)
            p.write_bytes(original)
        original = copy.deepcopy(self.manifest)
        self.manifest['cases'][0]['split'] = 'fresh-heldout'
        (self.path / 'manifest.json').write_text(json.dumps(self.manifest))
        with self.assertRaises(ValueError): revision.validate(self.path)
        (self.path / 'manifest.json').write_text(json.dumps(original))
        with self.assertRaisesRegex(ValueError, 'already exists'): revision.freeze(self.path)

    def test_missing_card_is_not_reported_as_private_content_disclosure(self):
        verdict = {'failures': [dict(launch=0, step=0, what='card excludes'),
                                dict(launch=0, step=0, what='exact approval')]}
        self.assertFalse(run.failure_flags(verdict, [dict(launch=0, step=0)])['mail_disclosure'])
        flags = run.failure_flags(verdict, [dict(launch=0, step=0, card='Private content')])
        self.assertTrue(flags['mail_disclosure'])
        self.assertFalse(flags['effect_violation'])
        self.assertTrue(flags['approval_check_failure'])
        verdict['failures'].append(dict(launch=0, step=0, what='effects'))
        self.assertTrue(run.failure_flags(verdict, [])['effect_violation'])

    def test_manifest_cannot_remove_a_model_seed_or_fresh_partition_job(self):
        out = Path(self.temp.name)
        self.path.rename(out / 'data')
        model = out / 'model.gguf'; model.write_bytes(b'model fixture')
        binary = out / 'bin'; binary.write_bytes(b'binary fixture')
        manifest = dict(version=1, models={'fixture': {'path': str(model), 'sha256': dataset.digest(model)}},
                        seeds=[1, 2, 7], jobs=[], environment=run.environment(),
                        files={'bin': dataset.digest(binary)}, dataset_manifest_sha256=dataset.digest(out / 'data/manifest.json'))
        for split in revision.PARTITIONS:
            for seed in manifest['seeds']:
                manifest['jobs'].append(dict(name=f'{split}-fixture-seed{seed}', split=split, model='fixture', seed=seed,
                                            expected=[r['id'] for r in self.manifest['cases'] if r['split'] == split]))
        run.verify(out, manifest)
        for index in (0, 5, len(manifest['jobs']) - 1):
            damaged = copy.deepcopy(manifest); damaged['jobs'].pop(index)
            with self.subTest(index=index), self.assertRaisesRegex(ValueError, 'plan'):
                run.verify(out, damaged)

    def test_missing_rows_keep_every_model_seed_partition_in_denominator(self):
        out = Path(self.temp.name)
        self.path.rename(out / 'data'); (out / 'rows').mkdir()
        manifest = dict(models={'4B': {}, '27B': {}}, seeds=[1, 2, 7], jobs=[])
        for split in revision.PARTITIONS:
            ids = [r['id'] for r in self.manifest['cases'] if r['split'] == split]
            for model in manifest['models']:
                for seed in manifest['seeds']:
                    manifest['jobs'].append(dict(name=f'{split}-{model}-seed{seed}', split=split,
                                                model=model, seed=seed, expected=ids))
        (out / 'manifest.json').write_text(json.dumps(manifest))
        result = run.summarize(out, manifest)
        self.assertEqual((result['planned'], result['recorded'], result['complete']), (1494, 0, False))
        self.assertEqual(len(result['groups']), 8)
        self.assertTrue(all(g['planned'] == revision.PARTITIONS[g['split']] * 3 for g in result['groups']))
        self.assertIn('Only fresh-heldout', (out / 'report.md').read_text())


if __name__ == '__main__':
    unittest.main()
