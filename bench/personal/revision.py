"""Personal-200 v3 plus separate fresh challenges and email regressions.

Only dataset/oracle changes: the app, Rust harness and independent scorer stay
unchanged. Frozen v1/v2 inputs and published verdicts are never rewritten.
"""
import argparse
from collections import Counter
import copy
from datetime import datetime, timezone
import json
from pathlib import Path
import re
import shutil
import tomllib

from dataset import HERE, digest, object_digest, render

VERSION = 'personal-200-v3'
PARTITIONS = {'regression-development': 120, 'regression-prior-heldout': 80,
              'email-regression': 9, 'fresh-heldout': 40}
FILES = {*[f'{name}.toml' for name in PARTITIONS], 'scoring.json', 'changes.json',
         'authoring.py', 'fresh-authoring.py'}
NUMERIC = re.compile(r'(?<![\w.,-])([1-9][0-9]{3,5})(?![\w.,-])')
CHECKS = ('expect', 'reject', 'forbid', 'card_contains', 'card_excludes', 'sent', 'unsent')


def numeric_forms(value):
    """Explicit alternatives, never modify model output or accept arbitrary prose.

Comma and space grouping of ASCII integers only; no decimal/units conversion,
Unicode digit conversion, leading-zero change, or formatting inside identifiers.
    """
    forms = [value]
    if '|' in value:
        return forms
    for separator in (',', ' '):
        formatted = NUMERIC.sub(lambda m: format(int(m[0]), ',').replace(',', separator), value)
        if formatted not in forms:
            forms.append(formatted)
    return forms


def revise(part):
    part = copy.deepcopy(part)
    changes = []
    for case in part['case']:
        original = copy.deepcopy(case)
        for launch in case['launches']:
            for step in launch:
                for key in CHECKS:
                    if key in step:
                        step[key] = ['|'.join(numeric_forms(v)) for v in step[key]]
                if 'answer_exact' in step:
                    step['answer_exact'] = [form for v in step['answer_exact'] for form in numeric_forms(v)]
        family = '/'.join(case['id'].split('/')[1:-1])
        if family in ('approval/defer-until-requested', 'interruption/resume-deferred-draft'):
            # The original conversation still supplies the reference and recipient.
            # Only make the resumed message's purpose explicit; do not restate the answer.
            purpose = 'reservation' if family.startswith('approval/') else 'permit'
            case['launches'][-1][0]['say'] = (
                f'Prepare that email for my approval now. Tell the intended recipient '
                f'the {purpose} reference recorded earlier in this task; put that reference in the body.')
        if case != original:
            changes.append({'id': case['id'], 'before_sha256': object_digest(original),
                            'after_sha256': object_digest(case),
                            'reason': 'Explicit equivalent integer grouping checks; ' +
                            ('clarify resumed email purpose without supplying missing answers.' if family in (
                                'approval/defer-until-requested', 'interruption/resume-deferred-draft') else 'conversation unchanged.')})
    return part, changes


def freeze(out):
    import fresh
    from dataset import validate as validate_legacy
    old = validate_legacy(HERE / 'v2')
    if out.exists():
        raise ValueError('Frozen dataset path already exists; publish a new version instead')
    data, metadata, changes = {}, [], []
    for before, after in [('development', 'regression-development'), ('heldout', 'regression-prior-heldout')]:
        data[after], changed = revise(tomllib.loads((HERE / 'v2' / f'{before}.toml').read_text()))
        changes.extend(changed)
        metadata.extend({**m, 'split': after, 'origin': 'reused-v2',
                         'sha256': object_digest(next(c for c in data[after]['case'] if c['id'] == m['id']))}
                        for m in old['cases'] if m['split'] == before)
    email = HERE.parent / 'task/semantic-email-v2.toml'
    data['email-regression'] = tomllib.loads(email.read_text())
    # Preserve exact existing input semantics; the TOML renderer accepts absent page arrays.
    data['email-regression'].setdefault('page', [])
    metadata.extend(dict(id=c['id'], category='email', family=c['id'], language={
        'fr-finance': 'fr/en', 'es-health': 'es/en', 'fa-finance': 'fa/en', 'ar-contact': 'ar/en'
    }.get(c['id'].split('/')[-1], 'en'), variant=1, split='email-regression', origin='reused-email-v2',
                         sha256=object_digest(c)) for c in data['email-regression']['case'])
    data['fresh-heldout'], new = fresh.build()
    metadata.extend({**m, 'origin': 'new-family', 'sha256': object_digest(c)}
                    for c, m in zip(data['fresh-heldout']['case'], new, strict=True))
    out.mkdir(parents=True)
    for name, corpus in data.items():
        (out / f'{name}.toml').write_text(render(corpus).replace('personal-200-v2 inputs', 'personal-200-v3 inputs', 1))
    policy = {'version': 3, 'checks': 'Unchanged Rust checks and independent Python scorer. Equivalent integer forms are explicit alternatives in positive AND negative checks.',
              'changes': 'Eight resumed-email messages clarify purpose without restating the reference or recipient. Other conversations unchanged. Strict bare-answer checks remain strict.',
              'groups': 'The 200 v2 tasks and nine email cases are reused regressions. Only fresh-heldout is a new public runtime holdout: 20 new combined-transition families, two variants each.',
              'frozen': 'Freeze before inference. Both models and all three seeds are one evaluation. No product/prompt tuning, case selection, same-build retries, or oracle edits after results.',
              'limits': 'Deterministic lexical checks, not general semantic truth/negation/privacy proof. No real mail, Android, transport, or encryption acceptance.',
              'latency': 'Desktop first-visible and whole-turn median/p95, per group/model. Excludes model load and background summaries. All error rows retained.'}
    for name, obj in [('scoring.json', policy), ('changes.json', changes)]:
        (out / name).write_text(json.dumps(obj, ensure_ascii=False, indent=2) + '\n')
    shutil.copy2(Path(__file__), out / 'authoring.py')
    shutil.copy2(HERE / 'fresh.py', out / 'fresh-authoring.py')
    manifest = {'version': VERSION, 'frozen_at_utc': datetime.now(timezone.utc).isoformat(),
                'partitions': PARTITIONS, 'cases': metadata,
                'parents': {'v2_manifest_sha256': digest(HERE / 'v2/manifest.json'), 'email_v2_sha256': digest(email)},
                'heldout_policy': policy['groups'] + ' ' + policy['frozen'],
                'files': {name: digest(out / name) for name in sorted(FILES)}}
    (out / 'manifest.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + '\n')
    validate(out)
    return manifest


def validate(path):
    manifest = json.loads((path / 'manifest.json').read_text())
    if manifest.get('version') != VERSION or manifest.get('partitions') != PARTITIONS or set(manifest['files']) != FILES:
        raise ValueError('Unknown v3 dataset contract')
    for name, wanted in manifest['files'].items():
        if digest(path / name) != wanted:
            raise ValueError(f'Frozen input changed: {name}')
    metadata = manifest['cases']
    if len(metadata) != sum(PARTITIONS.values()) or len({m['id'] for m in metadata}) != len(metadata):
        raise ValueError('Duplicate/missing metadata')
    seen, conversations, family_splits, page_splits = set(), set(), {}, {}
    for split, size in PARTITIONS.items():
        corpus = tomllib.loads((path / f'{split}.toml').read_text())
        indexed = {m['id']: m for m in metadata if m['split'] == split}
        if len(corpus['case']) != size or len(indexed) != size:
            raise ValueError('Wrong partition size')
        for page in corpus['page']:
            if page_splits.setdefault(page['url'], split) != split:
                raise ValueError('Source fixture crosses partitions')
        for case in corpus['case']:
            row = indexed.get(case['id'])
            signature = object_digest(case['launches'])
            if row is None or case['id'] in seen or signature in conversations or object_digest(case) != row['sha256']:
                raise ValueError('Duplicate, foreign or modified case')
            if family_splits.setdefault(row['family'], split) != split:
                raise ValueError('Family crosses partitions')
            if split == 'fresh-heldout' and (row['origin'] != 'new-family' or not row['family'].startswith('fresh/')):
                raise ValueError('Reused family labelled fresh')
            seen.add(case['id']); conversations.add(signature)
            if not case['launches'] or any(not launch for launch in case['launches']):
                raise ValueError('Empty conversation')
            if not any(s.get('expect') or s.get('answer_exact') or s.get('sent') or s.get('card_contains') for launch in case['launches'] for s in launch):
                raise ValueError('No positive oracle')
            for launch in case['launches']:
                for index, step in enumerate(launch):
                    if sum(k in step for k in ('say', 'again', 'fill', 'delete')) != 1:
                        raise ValueError('Invalid step operation')
                    if step.get('card') == 'close' and index != len(launch) - 1:
                        raise ValueError('Unreachable step after close')
                    if (step.get('card') == 'approve' or step.get('again') == 'approve') and not step.get('sent'):
                        raise ValueError('Approved action lacks positive effect oracle')
                    if step.get('fill', 0) > len(corpus['filler']):
                        raise ValueError('Too few filler messages')
    fresh_rows = [m for m in metadata if m['split'] == 'fresh-heldout']
    families = Counter(m['family'] for m in fresh_rows)
    if len(families) != 20 or set(families.values()) != {2} or any(
            {m['variant'] for m in fresh_rows if m['family'] == family} != {1, 2} for family in families):
        raise ValueError('Expected 20 fresh families of two variants')
    return manifest


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=['freeze', 'validate'])
    parser.add_argument('--data', type=Path, default=HERE / 'v3')
    args = parser.parse_args()
    manifest = freeze(args.data) if args.command == 'freeze' else validate(args.data)
    print(manifest['version'], manifest['partitions'])
