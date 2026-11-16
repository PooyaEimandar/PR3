import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { test } from 'node:test';
import { here, validateCorpus } from './freeze.ts';
import type { Corpus } from './types.ts';
test('fresh corpus has distinct scenario families and grounded quotation oracles',()=>{
  const corpus=JSON.parse(readFileSync(resolve(here,'v1/held-out.json'),'utf8')) as Corpus;
  const smoke=JSON.parse(readFileSync(resolve(here,'v1/smoke.json'),'utf8')) as Corpus;
  validateCorpus(corpus);validateCorpus(smoke);
  assert.equal(corpus.cases.length,16);
  assert.equal(corpus.cases.filter(c=>c.restart).length,4);
  assert.equal(corpus.cases.some(c=>smoke.cases.some(s=>s.family===c.family)),false);
  const bad=structuredClone(corpus);bad.cases[0]!.checks.quotes=[['invented quotation']];
  assert.throws(()=>validateCorpus(bad),/oracle absent/);
});
