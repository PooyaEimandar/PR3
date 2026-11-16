import { readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import type { Checks, Corpus, Row, Turn } from './types.ts';

const normalize = (text: string) => text.normalize('NFKC').toLocaleLowerCase().replace(/\s+/gu, ' ').trim();
// Accept presentation wrappers without normalizing the underlying quotation.
function quotationVariants(text: string): string[] {
  const variants = [text, text.trim()];
  let current = text.trim();
  for (let i = 0; i < 4; i++) {
    const next = current.replace(/^(?:\*\*([\s\S]+)\*\*|\*([\s\S]+)\*|_([\s\S]+)_|"([\s\S]+)"|“([\s\S]+)”)$/u,
      (_match, ...groups: unknown[]) => String(groups.slice(0, 5).find(g => g !== undefined)));
    if (next === current) break;
    variants.push(next); current = next;
  }
  return variants;
}
export function hasFact(text: string, value: string): boolean {
  const escaped = normalize(value).replace(/[.*+?^${}()|[\]\\]/gu, '\\$&');
  return new RegExp(`(?<![\\p{L}\\p{N}])${escaped}(?![\\p{L}\\p{N}])`, 'u').test(normalize(text));
}
export function score(turn: Turn, checks: Checks) {
  const analysis: string[] = [], content: string[] = [];
  const pairs: { id: string; quote: string }[] = [];
  let quotation: string[] = [];
  for (const line of turn.answer.split('\n')) {
    const label = line.replace(/^>\s*/u, '').replace(/^(?:—|-)\s*/u, '').replace(/\*\*|__/gu, '').trim();
    if (/^Source:\s/iu.test(label)) {
      const id = line.match(/\[((?:document|research):[^\]\s]+)\]\s*$/u)?.[1];
      if (id && quotation.length) pairs.push({ id, quote: quotation.join('\n') });
      quotation = [];
      continue;
    }
    if (/^Coverage:\s/iu.test(label) || label.startsWith('Research findings') || label.startsWith('The analysis below')) continue;
    // Metadata, URLs and fingerprints cannot supply required answer facts.
    const clean = line.replace(/\[(?:document|research):[^\]]+\]/gu, '').replace(/https?:\/\/\S+/gu, '');
    content.push(clean);
    if (line.startsWith('> ')) quotation.push(line.slice(2));
    else if (line.trim()) { analysis.push(clean); quotation = []; }
  }
  const byId = new Map(turn.evidence.sources.map(s => [s.source, s]));
  const verifiedPairs = pairs.flatMap(p => {
    const quote = quotationVariants(p.quote).find(q => byId.get(p.id)?.text.includes(q));
    return quote === undefined ? [] : [{ id:p.id, quote }];
  });
  const origins = new Set(verifiedPairs.map(p => p.id.startsWith('document:')
    ? p.id.split(':').slice(0, 2).join(':')
    : byId.get(p.id)!.location.split(' · part ')[0]));
  const text = content.join('\n'), prose = analysis.join('\n');
  const quoted = verifiedPairs.map(p => p.quote).join('\n');
  const facts = checks.facts.every(group => group.some(value => hasFact(text, value)));
  const quotations = checks.quotes.every(group => group.some(value => normalize(quoted).includes(normalize(value))));
  const reasoning = checks.analysis.every(pattern => new RegExp(pattern, 'isu').test(prose));
  const forbidden = checks.forbidden.some(pattern => new RegExp(pattern, 'isu').test(text));
  const reportTool = turn.events.some(e => e.name === 'answer_research' && e.success === true);
  const cited = origins.size >= checks.minSources && pairs.length === verifiedPairs.length;
  const noRuntimeError = turn.error === null;
  return {
    facts, quotations, reasoning, forbidden, cited, reportTool, noRuntimeError,
    sources: origins.size,
    automaticPass: facts && quotations && reasoning && !forbidden && cited && reportTool && noRuntimeError,
    // Regex and string checks are useful diagnostics, never a general entailment judge.
    semanticReview: 'required' as const,
  };
}

export function summarize(corpus: Corpus, rows: Row[], models: string[], seeds: number[]) {
  const seen = new Set<string>();
  const groups: Record<string, { runs: number; turns: number; automaticPass: number; facts: number; reasoning: number; citations: number; errors: number; seconds: number[] }> = {};
  for (const row of rows) {
    const key = `${row.model}/${row.side}/${row.seed}/${row.id}`;
    if (seen.has(key)) throw new Error(`Duplicate row: ${key}`);
    seen.add(key);
    const test = corpus.cases.find(c => c.id === row.id);
    if (!test || !models.includes(row.model) || !seeds.includes(row.seed) || !['pr3','openjarvis'].includes(row.side)) throw new Error(`Foreign row: ${key}`);
    const expected = test.restart ? 2 : 1;
    if (row.turns.length !== expected && !row.error) throw new Error(`Incomplete turns: ${key}`);
    const group = groups[`${row.model}/${row.side}`] ??= { runs: 0, turns: 0, automaticPass: 0, facts: 0, reasoning: 0, citations: 0, errors: 0, seconds: [] };
    group.runs++;
    group.turns += expected; // Missing/error turns stay in the denominator.
    if (row.error) { group.errors += expected; continue; }
    const scores = row.turns.map((turn, i) => score(turn, i === 0 ? test.checks : test.restart!.checks));
    group.automaticPass += Number(scores.every(s => s.automaticPass));
    scores.forEach((s, i) => {
      group.facts += Number(s.facts); group.reasoning += Number(s.reasoning);
      group.citations += Number(s.cited && s.quotations && s.reportTool);
      group.errors += Number(!s.noRuntimeError);
      const seconds = row.turns[i]?.seconds;
      if (typeof seconds === 'number' && Number.isFinite(seconds)) group.seconds.push(seconds);
    });
  }
  const expectedRows = corpus.cases.length * models.length * seeds.length * 2;
  return { expectedRows, recordedRows: rows.length, complete: rows.length === expectedRows,
    scope: 'Frozen shared-source research comparison; automatic checks are not semantic entailment or native retrieval parity.',
    groups: Object.fromEntries(Object.entries(groups).map(([key, g]) => {
      const sorted = g.seconds.toSorted((a,b)=>a-b);
      const percentile = (p: number) => sorted[Math.max(0, Math.ceil(sorted.length*p)-1)] ?? null;
      const middle = Math.floor(sorted.length/2);
      const median = sorted.length ? (sorted[(sorted.length-1)>>1]! + sorted[middle]!)/2 : null;
      return [key, { ...g, seconds: undefined, medianSeconds: median, p95Seconds: percentile(.95) }];
    })) };
}

export function verifyCorpusBytes(bytes: string, digest: string): Corpus {
  if (createHash('sha256').update(bytes).digest('hex') !== digest) throw new Error('Recorded corpus hash mismatch');
  return JSON.parse(bytes) as Corpus;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const dir = resolve(process.argv[2] ?? '');
  const manifest = JSON.parse(readFileSync(resolve(dir,'manifest.json'),'utf8')) as { models: { name: string }[]; seeds: number[]; corpusSha256: string };
  const corpus = verifyCorpusBytes(readFileSync(resolve(dir,'corpus.json'),'utf8'), manifest.corpusSha256);
  const rows = readFileSync(resolve(dir,'results.jsonl'),'utf8').trim().split('\n').filter(Boolean).map(line=>JSON.parse(line) as Row);
  const summary = summarize(corpus,rows,manifest.models.map(m=>m.name),manifest.seeds);
  writeFileSync(resolve(dir,'summary.json'), JSON.stringify(summary,null,2)+'\n');
  console.log(JSON.stringify(summary,null,2));
}
