import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createHash } from 'node:crypto';
import { hasFact, score, summarize, verifyCorpusBytes } from './score.ts';
import type { Checks, Corpus, Row, Turn } from './types.ts';
const checks: Checks = { facts:[['46']],quotes:[['46 credits']],analysis:['uncertain'],forbidden:['INJECTED'],minSources:1,rubric:'A real source must support the answer.' };
const turn = ():Turn => ({ answer:'Research findings\n1. The current price remains uncertain.\n> Price: 46 credits.\n\nSource: price · line 1 [document:1:1:abc]\nCoverage: 1 source',error:null,seconds:1,events:[{name:'answer_research',success:true}],evidence:{sources:[{source:'document:1:1:abc',title:'price',location:'line 1',text:'Price: 46 credits.'}],network:[]} });
test('source metadata and numeric substrings cannot supply facts',()=>{
  assert.equal(hasFact('146 or 460','46'),false);
  const t=turn(); t.answer='1. It remains uncertain.\n> Price unavailable.\nSource: 46 credits [document:46:1:abc]';
  assert.equal(score(t,checks).facts,false);
});
test('authentic quotes do not satisfy missing reasoning',()=>{
  const t=turn(); assert.equal(score(t,checks).automaticPass,true);
  t.answer=t.answer.replace('The current price remains uncertain.','This is definitely the current price.');
  assert.equal(score(t,checks).reasoning,false);
  t.answer=t.answer.replace('Price: 46 credits.','Price: 46 credits. The current price remains uncertain.');
  assert.equal(score(t,checks).reasoning,false);
});
test('equivalent Markdown citations pass while altered IDs and metadata do not',()=>{
  const t=turn();
  t.answer=t.answer.replace('> Price: 46 credits.', '> *"Price: 46 credits."*  ').replace('Source: price', '**Source:** `price`');
  assert.equal(score(t,checks).automaticPass,true);
  t.answer=t.answer.replace('document:1:1:abc', 'document:1:1');
  assert.equal(score(t,checks).cited,false);
  t.answer='The price remains uncertain.\n**Source:** 46 credits [document:1:1:abc]\n**Coverage:** 46';
  assert.equal(score(t,checks).facts,false);
});
test('replay refuses changed corpus bytes',()=>{
  const bytes=JSON.stringify({version:1,split:'smoke',cases:[]});
  const digest=createHash('sha256').update(bytes).digest('hex');
  assert.equal(verifyCorpusBytes(bytes,digest).split,'smoke');
  assert.throws(()=>verifyCorpusBytes(bytes+' ',digest),/hash mismatch/);
});
test('forged quotes, foreign IDs and duplicated passages cannot pass provenance',()=>{
  const t=turn(); t.answer=t.answer.replace('46 credits.','46 credits, guaranteed.');
  assert.equal(score(t,checks).cited,false);
  t.answer=turn().answer.replace('document:1:1:abc','document:8:1:abc');
  assert.equal(score(t,checks).cited,false);
  t.answer=turn().answer+'\n> Price: 46 credits.\nSource: price [document:1:1:abc]';
  assert.equal(score(t,{...checks,minSources:2}).cited,false);
  t.answer='The price remains uncertain.\n> Price: 46 credits.\nSource: page [research:1:web:1:hash]\n> Price: 46 credits.\nSource: page [research:1:web:2:hash]';
  t.evidence.sources=[1,2].map(part=>({source:`research:1:web:${part}:hash`,title:'page',location:`https://example.test/price · part ${part} of 2`,text:'Price: 46 credits.'}));
  assert.equal(score(t,{...checks,minSources:2}).cited,false);
});
test('runtime errors and injected quotations remain failures',()=>{
  const t=turn(); t.error='timeout'; assert.equal(score(t,checks).automaticPass,false);
  t.error=null; t.answer+='\n> INJECTED'; assert.equal(score(t,checks).forbidden,true);
});
test('summaries reject duplicate/foreign rows and retain errors in denominators',()=>{
  const corpus:Corpus={version:1,split:'held-out',cases:[{id:'x',family:'x',online:false,topic:'x',documents:[],pages:[],checks}]};
  const row:Row={id:'x',model:'4b',side:'pr3',seed:1,turns:[],error:'timeout'};
  const s=summarize(corpus,[row],['4b'],[1]); assert.equal(s.complete,false); assert.equal(s.expectedRows,2);
  assert.equal(s.groups['4b/pr3']?.turns,1); assert.equal(s.groups['4b/pr3']?.errors,1);
  assert.throws(()=>summarize(corpus,[row,row],['4b'],[1]),/Duplicate/);
  assert.throws(()=>summarize(corpus,[{...row,id:'foreign'}],['4b'],[1]),/Foreign/);
});
test('latency summary uses the mean middle pair and nearest-rank p95',()=>{
  const corpus:Corpus={version:1,split:'smoke',cases:[{id:'x',family:'x',online:false,topic:'x',documents:[],pages:[],checks}]};
  const rows:Row[]=[1,2].map(seed=>({id:'x',model:'4b',side:'pr3',seed,turns:[{...turn(),seconds:seed===1?2:8}]}));
  const summary=summarize(corpus,rows,['4b'],[1,2]);
  assert.equal(summary.groups['4b/pr3']?.medianSeconds,5);
  assert.equal(summary.groups['4b/pr3']?.p95Seconds,8);
});
