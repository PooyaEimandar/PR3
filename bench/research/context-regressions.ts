/** Development fixtures for the live context failures. Not a new held-out score.
 * node bench/research/context-regressions.ts /tmp/pr3-context-cases.json
 * target/debug/research MODEL SEED /tmp/pr3-context-cases.json OUTPUT.jsonl
 */
import { writeFileSync } from 'node:fs';

const background = 'Archive navigation and historical release notes are available in the directory.\n';
const license = `${background.repeat(26)}The Larch database engine is in the public domain.\n${background.repeat(17)}The exception covers the documentation, which is licensed separately.\n${background.repeat(10)}`;
const wal = `${background.repeat(24)}WAL permits readers to continue while a writer commits.\n${background.repeat(10)}WAL requires all processes to run on the same host.\n${background.repeat(9)}`;
const rollback = `${background.repeat(24)}Rollback journaling briefly excludes readers during commit.\n${background.repeat(15)}`;
const cases = [
  {
    id: 'short-page-restart', online: true,
    topic: 'State the purpose of https://public.example/demo in one cited finding.', documents: [],
    pages: [{ url: 'https://public.example/demo', text: 'This domain is for use in illustrative examples in documents.' }],
    quotes: ['This domain is for use in illustrative examples in documents.'], citations: 1,
    restart: 'Using only the saved page, quote its stated purpose in one finding. Web reading is off; do not imply that you checked a live page now.',
  },
  {
    id: 'long-license-restart', online: true,
    topic: 'Is Larch public domain and what is the exception? Use https://public.example/license', documents: [],
    pages: [{ url: 'https://public.example/license', text: license }],
    quotes: ['The Larch database engine is in the public domain.', 'The exception covers the documentation, which is licensed separately.'], citations: 2,
    restart: 'Correction: discuss only the documentation exception, using the saved snapshots. Do not fetch pages or repeat the public-domain finding.',
    restart_checks: { quotes: ['The exception covers the documentation, which is licensed separately.'], absent: ['The Larch database engine is in the public domain.'], citations: 1 },
  },
  {
    id: 'long-comparison-restart', online: true,
    topic: 'Compare reader behavior during commit in WAL and rollback. Also state the WAL host restriction. Use https://public.example/wal and https://public.example/rollback', documents: [],
    pages: [{ url: 'https://public.example/wal', text: wal }, { url: 'https://public.example/rollback', text: rollback }],
    quotes: ['WAL permits readers to continue while a writer commits.', 'WAL requires all processes to run on the same host.', 'Rollback journaling briefly excludes readers during commit.'], citations: 2,
    restart: 'Use only the saved evidence. Now quote the WAL host restriction in one finding.',
    restart_checks: { quotes: ['WAL requires all processes to run on the same host.'], citations: 1 },
  },
];
const output = process.argv[2];
if (!output) throw new Error('Usage: node context-regressions.ts OUTPUT.json');
writeFileSync(output, JSON.stringify({ status: 'Development context regressions; synthetic offline pages, not held-out', cases }, null, 2), { flag: 'wx' });
