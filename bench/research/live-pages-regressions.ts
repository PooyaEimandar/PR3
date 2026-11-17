/** Public-page development replay of an observed live failure, not held-out.
 * Fetches public HTML once; model evaluation subsequently uses offline fixtures.
 */
import { writeFileSync } from 'node:fs';
const output = process.argv[2];
if (!output) throw new Error('Usage: live-pages-regressions.ts NEW_OUTPUT.json');
const pages = [];
for (const url of ['https://www.sqlite.org/wal.html', 'https://www.sqlite.org/lockingv3.html']) {
  const response = await fetch(url, { redirect: 'error', signal: AbortSignal.timeout(20000) });
  if (!response.ok) throw new Error(`Public fixture fetch failed: ${response.status}`);
  const text = await response.text();
  if (Buffer.byteLength(text) > 2 * 1024 * 1024 || !response.headers.get('content-type')?.includes('text/html')) throw new Error('Unexpected public fixture size/type');
  pages.push({ url, text, media_type: 'text/html' });
}
const cases = [{
  id: 'public-sqlite-comparison-restart', online: true, documents: [], pages,
  topic: "Compare WAL and rollback reader behavior during commit, and state WAL's host restriction. Use https://www.sqlite.org/wal.html and https://www.sqlite.org/lockingv3.html Sources only: sqlite.org",
  citations: 2,
  review: {
    facts: [['WAL'], ['rollback'], ['same host', 'same computer', 'same machine']],
    quotes: [['readers', 'read transaction'], ['EXCLUSIVE', 'SHARED', 'PENDING'], ['same host', 'same machine']],
    analysis: ['(?:rollback.{0,180}(?:block|exclusive|wait)|(?:block|exclusive|wait).{0,180}rollback)', 'same (?:host|computer|machine)'],
    forbidden: ['readers must scan the entire database'], minSources: 2,
    rubric: 'Explain WAL readers continuing during commits, rollback exclusive-lock behavior, and the same-host restriction. Do not confuse crash/ROLLBACK recovery with normal COMMIT. Every quotation must support its associated finding; reject duplicate findings used instead of covering the requested comparison.',
  },
  restart: "Correction: quote only WAL's same-host restriction from the saved source. Web is off; do not repeat the comparison or imply a live check.",
  restart_checks: { citations: 1, quotes: ['same host computer'], absent: ['The checkpoint remembers'] },
  restart_review: {
    facts: [['same host', 'same computer', 'same machine']], quotes: [['same host', 'same machine']],
    analysis: ['same (?:host|computer|machine)'], forbidden: ['The checkpoint remembers'], minSources: 1,
    rubric: 'Answer only the corrected same-host question using saved evidence. No fresh search/read, no claimed live recheck and no unrelated checkpoint quotation. Source timestamp must remain unchanged.',
  },
}];
writeFileSync(output, JSON.stringify({ status: 'Public-source development replay, not held-out; no Tavily provider or live search during evaluation', retrievedAt: new Date().toISOString(), cases }, null, 2) + '\n', { flag: 'wx' });
