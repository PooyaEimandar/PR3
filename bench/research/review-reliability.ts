/** Summarize a completed diagnostic matrix without treating lexical checks as semantic review. */
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { score } from './score.ts';
import type { Checks, Turn } from './types.ts';

const dir = resolve(process.argv[2] ?? '');
if (process.argv.length !== 3) throw new Error('Usage: review-reliability.ts RESULT_DIRECTORY');
const manifest = JSON.parse(readFileSync(resolve(dir, 'manifest.json'), 'utf8')) as { corpusSha256: string; models: { name: string }[]; seeds: number[] };
const corpusBytes = readFileSync(resolve(dir, 'corpus.json'), 'utf8');
if (createHash('sha256').update(corpusBytes).digest('hex') !== manifest.corpusSha256) throw new Error('Corpus changed since freeze');
const corpus = JSON.parse(corpusBytes) as { cases: { id: string; suite: string; topic: string; review?: Checks; restart_review?: Checks; restart?: string; quotes?: string[]; restart_checks?: {quotes?: string[]} }[] };
const progress = JSON.parse(readFileSync(resolve(dir, 'progress.json'), 'utf8')) as { complete: boolean; runs: { model: string; seed: number; file: string }[] };
if (!progress.complete || progress.runs.length !== 6) throw new Error('Matrix is incomplete; retain missing turns in the denominator');
if (new Set(progress.runs.map(r => `${r.model}/${r.seed}`)).size !== 6 || progress.runs.some(r => !manifest.models.some(m => m.name === r.model) || !manifest.seeds.includes(r.seed))) throw new Error('Duplicate or foreign model/seed run');
type DiagnosticTurn = Turn & { pass: boolean; checks: Record<string, boolean> };
const review = [];
const groups: Record<string, { turns: number; diagnosticPass: number; validatedReport: number; runtimeErrors: number; restartSearchOrReads: number; seconds: number[] }> = {};
for (const run of progress.runs) {
  const rows = readFileSync(resolve(dir, run.file), 'utf8').trim().split('\n').map(line => JSON.parse(line) as { id: string; seed: number; turns: DiagnosticTurn[] });
  if (rows.length !== corpus.cases.length || new Set(rows.map(r => r.id)).size !== rows.length) throw new Error('Missing or duplicate scenarios');
  for (const row of rows) {
    const test = corpus.cases.find(c => c.id === row.id);
    if (!test || row.seed !== run.seed || row.turns.length !== (test.restart ? 2 : 1)) throw new Error('Invalid scenario, seed or turn count');
    const key = `${run.model}/${test.suite}`;
    const group = groups[key] ??= { turns: 0, diagnosticPass: 0, validatedReport: 0, runtimeErrors: 0, restartSearchOrReads: 0, seconds: [] };
    for (const [i, turn] of row.turns.entries()) {
      group.turns++;
      group.diagnosticPass += Number(turn.pass);
      group.validatedReport += Number(turn.events.some(e => e.name === 'answer_research' && e.success));
      group.runtimeErrors += Number(turn.error !== null);
      group.restartSearchOrReads += Number(i > 0 && turn.events.some(e => ['research_sources', 'read_research_source'].includes(e.name ?? '')));
      group.seconds.push(turn.seconds);
      const rubric = i === 0 ? test.review : test.restart_review ?? test.review;
      review.push({ model: run.model, seed: run.seed, id: row.id, turn: i, request: i === 0 ? test.topic : test.restart, expectedQuotes: i === 0 ? test.quotes : test.restart_checks?.quotes ?? test.quotes, diagnosticPass: turn.pass, checks: turn.checks, strongerAutomaticChecks: rubric ? score(turn, rubric) : null, rubric: rubric?.rubric ?? 'Answer the latest request completely with relevant supporting quotations; preserve conditions and uncertainty. No obsolete or unrequested findings. Restart must reuse saved evidence without claiming a live check.', answer: turn.answer, error: turn.error, sources: turn.evidence.sources, judgment: null, reason: null });
    }
  }
}
const summary = {
  scope: 'Exposed development replay, not held-out or an OpenJarvis comparison. Diagnostic checks are not semantic entailment. Timings are observational desktop measurements.',
  groups: Object.fromEntries(Object.entries(groups).map(([key, g]) => {
    const sorted = g.seconds.toSorted((a, b) => a - b);
    const medianSeconds = (sorted[(sorted.length - 1) >> 1]! + sorted[sorted.length >> 1]!) / 2;
    return [key, { ...g, seconds: undefined, medianSeconds, p95Seconds: sorted[Math.ceil(sorted.length * .95) - 1] }];
  })),
};
writeFileSync(resolve(dir, 'summary.json'), JSON.stringify(summary, null, 2) + '\n', { flag: 'wx' });
writeFileSync(resolve(dir, 'review-required.json'), JSON.stringify(review, null, 2) + '\n', { flag: 'wx' });
console.log(JSON.stringify(summary, null, 2));
