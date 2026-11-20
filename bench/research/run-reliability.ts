/** Production-host development regressions, not a new holdout or upstream comparison.
 * node bench/research/run-reliability.ts FROZEN_BINARY NEW_OUTPUT_DIRECTORY [BASELINE_CORPUS] [--replay-only]
 */
import { createReadStream, createWriteStream, readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { spawn, execFileSync, type ChildProcess } from 'node:child_process';
import { resolve, relative } from 'node:path';
import { platform, release, arch, cpus, totalmem } from 'node:os';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../', import.meta.url));
const here = resolve(root, 'bench/research');
const binary = resolve(process.argv[2] ?? '');
const out = resolve(process.argv[3] ?? '');
const replayOnly = process.argv[5] === '--replay-only';
if (![4,5].includes(process.argv.length) && !(process.argv.length === 6 && replayOnly)) throw new Error('Usage: run-reliability.ts FROZEN_BINARY NEW_OUTPUT_DIRECTORY [BASELINE_CORPUS] [--replay-only]');
mkdirSync(out, { recursive: false });
async function hash(path: string) {
  const h = createHash('sha256');
  for await (const chunk of createReadStream(path)) h.update(chunk);
  return h.digest('hex');
}
const power = () => platform() === 'darwin' ? execFileSync('pmset', ['-g', 'batt'], { encoding: 'utf8' }) : 'unmeasured';
type Case = { id: string; restart?: string; [key: string]: unknown };
const cases: Case[] = [];
const baseline = process.argv[4];
if (baseline) {
  const bytes = readFileSync(resolve(baseline));
  const original = JSON.parse(bytes.toString()) as { cases: Case[] };
  if (!Array.isArray(original.cases) || original.cases.some(c => !c.id || typeof c.suite !== 'string')) throw new Error('Baseline requires cases with IDs and suite names');
  writeFileSync(resolve(out,'baseline-corpus.json'),bytes,{flag:'wx'});
  cases.push(...original.cases);
}
for (const suite of replayOnly ? [] : baseline ? ['coverage'] : ['context', 'quotation', 'reliability', 'live-pages', 'coverage']) {
  const path = resolve(out, `${suite}.json`);
  if (suite === 'quotation') writeFileSync(path, readFileSync(resolve(here, 'quotation-regressions.json')), { flag: 'wx' });
  else execFileSync(process.execPath, [resolve(here, `${suite}-regressions.ts`), path], { cwd: root });
  const corpus = JSON.parse(readFileSync(path, 'utf8')) as { cases: Case[] };
  cases.push(...corpus.cases.map(c => ({ ...c, suite })));
}
if (new Set(cases.map(c => c.id)).size !== cases.length) throw new Error('Duplicate diagnostic case ID');
const corpusPath = resolve(out, 'corpus.json');
writeFileSync(corpusPath, JSON.stringify({ status: 'Exposed development regressions, not held-out', cases }, null, 2) + '\n', { flag: 'wx' });
const models = [];
for (const name of ['Qwen3.5-4B-Q4_K_M.gguf', 'Qwen3.8-27B-Q4_K_M.gguf']) {
  const path = resolve(root, 'model', name);
  models.push({ name, path, sha256: await hash(path) });
}
const files = execFileSync('rg', ['--files', 'crate', 'app/pr3-app/src-tauri/src', 'app/pr3-app/src', 'app/pr3-app/ui', 'bench/research', 'bench/documents/src'], { cwd: root, encoding: 'utf8' })
  .trim().split('\n').filter(p => /\.(rs|ts|html|css|toml)$/u.test(p));
const productionSources: Record<string, string> = {};
for (const path of [...files, 'Cargo.lock', 'Cargo.toml', 'app/pr3-app/src-tauri/Cargo.toml', 'bench/documents/Cargo.toml']) productionSources[path] = await hash(resolve(root, path));
const manifest = {
  startedAt: new Date().toISOString(), binary, binarySha256: await hash(binary), corpusSha256: await hash(corpusPath),
  pr3Head: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim(), productionSources,
  baselineCorpusSha256: baseline ? await hash(resolve(out,'baseline-corpus.json')) : null,
  models, seeds: [1, 2, 7], expectedRows: cases.length * 6, expectedTurns: cases.reduce((n, c) => n + (c.restart ? 2 : 1), 0) * 6,
  engine: { context: 4096, maxNewTokens: 512, temperature: .7, topP: .8, topK: 20, presencePenalty: 1.5, thinking: false, gpuLayers: 999 },
  limits: { toolRounds: 4, callsPerRound: 4, toolCalls: 12 },
  hardware: { platform: platform(), release: release(), arch: arch(), cpu: cpus()[0]?.model, memoryBytes: totalmem() }, node: process.version,
  rustc: execFileSync('rustc', ['--version'], { encoding: 'utf8' }).trim(),
  powerAtStart: power(), scope: 'Sequential production Host with offline fixture pages. Diagnostic pass checks report/citations and selected text only; conclusions require separate review. No failed-case retries.',
};
writeFileSync(resolve(out, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n', { flag: 'wx' });
let active: ChildProcess | undefined;
process.once('SIGINT', () => { active?.kill('SIGTERM'); process.exitCode = 130; });
const runs: { model: string; seed: number; file: string; rows: number; turns: number; diagnosticPassed: number; power: string }[] = [];
for (const model of models) for (const seed of manifest.seeds) {
  if (process.exitCode) throw new Error('Evaluation interrupted; partial output retained');
  if (await hash(binary) !== manifest.binarySha256 || await hash(corpusPath) !== manifest.corpusSha256) throw new Error('Frozen diagnostic input changed');
  const label = model.name.includes('4B-') ? '4b' : '27b';
  const output = resolve(out, `${label}-seed${seed}.jsonl`);
  const log = createWriteStream(resolve(out, `${label}-seed${seed}.log`), { flags: 'wx' });
  console.log(`Starting ${label} seed ${seed}`);
  await new Promise<void>((done, reject) => {
    active = spawn(binary, [model.path, String(seed), corpusPath, output], { cwd: root, stdio: ['ignore', 'pipe', 'pipe'] });
    active.stdout?.pipe(log, { end: false }); active.stderr?.pipe(log, { end: false });
    active.once('error', reject);
    active.once('close', code => { log.end(); active = undefined; code === 0 ? done() : reject(new Error(`Diagnostic exited ${code}; partial output retained`)); });
  });
  const rows = readFileSync(output, 'utf8').trim().split('\n').map(line => JSON.parse(line) as { id: string; turns: { pass: boolean }[] });
  if (rows.length !== cases.length || new Set(rows.map(r => r.id)).size !== cases.length || rows.some(r => !cases.some(c => c.id === r.id && r.turns.length === (c.restart ? 2 : 1)))) throw new Error('Incomplete diagnostic output');
  const run = { model: model.name, seed, file: relative(out, output), rows: rows.length, turns: rows.reduce((n, r) => n + r.turns.length, 0), diagnosticPassed: rows.reduce((n, r) => n + r.turns.filter(t => t.pass).length, 0), power: power() };
  runs.push(run);
  writeFileSync(resolve(out, 'progress.json'), JSON.stringify({ complete: runs.length === 6, runs }, null, 2) + '\n');
  console.log(`${label} seed ${seed}: ${run.diagnosticPassed}/${run.turns} diagnostic checks; semantic review required`);
}
