/** Exposed development cases for evidence beyond the old first-twelve-part cap.
 * node bench/research/coverage-regressions.ts NEW_OUTPUT.json
 */
import { writeFileSync } from 'node:fs';

const navigation = 'Archive navigation and earlier release announcements.\n';
const padding = navigation.repeat(1000);
const cases = [
  {
    id: 'late-comparison-restart', chat: { web: true, websites: 'public.example' }, documents: [],
    topic: 'Compare Orchid and Reed reader behavior during commit. Also state the Orchid host restriction. Use https://public.example/orchid and https://public.example/reed',
    pages: [
      { url: 'https://public.example/orchid', text: `${padding}Orchid lets readers continue while a writer commits.\n${navigation.repeat(100)}Orchid requires every process to run on the same host.\n${navigation.repeat(50)}` },
      { url: 'https://public.example/reed', text: `${padding}Reed waits for existing readers to finish before commit and excludes new readers until the commit completes.\n${navigation.repeat(100)}` },
    ],
    citations: 2,
    quotes: ['Orchid lets readers continue while a writer commits.', 'Orchid requires every process to run on the same host.', 'Reed waits for existing readers to finish before commit and excludes new readers until the commit completes.'],
    review: {
      facts: [['Orchid'], ['Reed'], ['same host']], quotes: [['readers continue'], ['excludes new readers'], ['same host']], minSources: 2, analysis: [], forbidden: [],
      rubric: 'Answer all three requested points: Orchid readers continue during commit; Reed waits for existing readers and excludes new ones during commit; Orchid requires the same host. Each finding must attach the evidence for its full claim. Do not substitute an uncited comparison or claim the late facts are unavailable.',
    },
    restart: 'Correction: quote only the Orchid host restriction from the saved sources. Do not fetch or repeat the reader comparison.',
    restart_checks: { citations: 1, quotes: ['Orchid requires every process to run on the same host.'], absent: ['Reed waits', 'Orchid lets readers'] },
    restart_review: { facts: [['same host']], quotes: [['same host']], minSources: 1, analysis: [], forbidden: ['Reed waits', 'Orchid lets readers'], rubric: 'Only the host restriction, unchanged saved evidence, no fresh search/read or live-check claim.' },
  },
  {
    id: 'late-conditions-restart', chat: { web: true, websites: 'public.example' }, documents: [],
    topic: 'Can every Delta member export? Can free accounts export, and what is the trial export quota? Use https://public.example/delta',
    pages: [{ url: 'https://public.example/delta', text: `${padding}Delta export is available only to administrators.\n${navigation.repeat(160)}The trial export quota is 3 reports per day. Free accounts cannot export.\n${navigation.repeat(60)}` }],
    citations: 2, quotes: ['Delta export is available only to administrators.', 'The trial export quota is 3 reports per day.', 'Free accounts cannot export.'],
    review: { facts: [['administrator'], ['3'], ['Free accounts cannot export']], quotes: [['only to administrators'], ['3 reports per day'], ['Free accounts cannot export']], minSources: 1, analysis: [], forbidden: ['every member can export', 'all members can export'], rubric: 'Explicitly answer no: export is limited to administrators, free accounts cannot export, and the trial quota is three reports per day. Preserve all conditions with supporting quotes.' },
    restart: 'Correction: quote only the trial quota from the saved source, without discussing eligibility or fetching pages.',
    restart_checks: { citations: 1, quotes: ['The trial export quota is 3 reports per day.'], absent: ['only to administrators'] },
    restart_review: { facts: [['3']], quotes: [['3 reports per day']], minSources: 1, analysis: [], forbidden: ['only to administrators'], rubric: 'Answer only the corrected quota request from the original snapshot; no new retrieval, no obsolete eligibility finding.' },
  },
];
const output = process.argv[2];
if (!output) throw new Error('Usage: coverage-regressions.ts NEW_OUTPUT.json');
writeFileSync(output, JSON.stringify({ status: 'Exposed synthetic development regressions; not held-out or live network evidence', cases }, null, 2) + '\n', { flag: 'wx' });
