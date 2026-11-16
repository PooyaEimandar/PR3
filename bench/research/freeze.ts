import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { dirname, relative, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { execFileSync } from 'node:child_process';
import type { Corpus } from './types.ts';
export const here = dirname(fileURLToPath(import.meta.url));
export const root = resolve(here,'../..');
export const sha = (path:string) => createHash('sha256').update(readFileSync(path)).digest('hex');
export function sourceHashes(): Record<string,string> {
  const files = ['Cargo.toml','Cargo.lock','config/pr3.toml'];
  for (const dir of ['crate','app/pr3-app/src-tauri/src']) {
    for (const entry of readdirSync(resolve(root,dir),{recursive:true,withFileTypes:true})) {
      if (entry.isFile() && /\.(rs|toml)$/u.test(entry.name)) files.push(relative(root,resolve(entry.parentPath,entry.name)));
    }
  }
  return Object.fromEntries(files.toSorted().map(path=>[path,sha(resolve(root,path))]));
}
export function validateCorpus(corpus:Corpus):void {
  const ids = new Set<string>(), families = new Set<string>();
  for (const c of corpus.cases) {
    if (!/^[hs][0-9]{2}$/u.test(c.id) || ids.has(c.id) || families.has(c.family)) throw new Error('Duplicate/invalid case or family');
    ids.add(c.id); families.add(c.family);
    if (!c.topic || Buffer.byteLength(c.topic)>512 || /[\r\n]/u.test(c.topic)) throw new Error(`Invalid topic ${c.id}`);
    const text=[...c.documents.map(d=>d.text),...c.pages.map(p=>p.text)].join('\n');
    for (const check of [c.checks,...(c.restart?[c.restart.checks]:[])]) {
      if (!check.rubric || !Number.isInteger(check.minSources)) throw new Error(`Missing rubric ${c.id}`);
      for (const options of check.quotes) if (!options.length || !options.some(q=>q && text.includes(q))) throw new Error(`Quotation oracle absent from ${c.id}: ${options}`);
      for (const regex of [...check.analysis,...check.forbidden]) new RegExp(regex,'isu');
    }
  }
}
export interface Freeze {
  version:1; frozenAt:string; upstream:string; models:string[]; seeds:number[];
  corpusSha256:string; smokeSha256:string; productionSources:Record<string,string>;
  limits:{tool_rounds:number;tool_calls:number;calls_per_round:number};
}
export function verifyFreeze():Freeze {
  const f=JSON.parse(readFileSync(resolve(here,'v1/freeze.json'),'utf8')) as Freeze;
  if (sha(resolve(here,'v1/held-out.json'))!==f.corpusSha256 || sha(resolve(here,'v1/smoke.json'))!==f.smokeSha256) throw new Error('Frozen corpus changed');
  if (JSON.stringify(sourceHashes())!==JSON.stringify(f.productionSources)) throw new Error('Frozen production source changed; do not mix builds');
  return f;
}
if (process.argv[1] && import.meta.url===pathToFileURL(resolve(process.argv[1])).href) {
  const corpus=JSON.parse(readFileSync(resolve(here,'v1/held-out.json'),'utf8')) as Corpus;
  const smoke=JSON.parse(readFileSync(resolve(here,'v1/smoke.json'),'utf8')) as Corpus;
  validateCorpus(corpus); validateCorpus(smoke);
  if (corpus.cases.some(c=>smoke.cases.some(s=>c.family===s.family))) throw new Error('Split family overlap');
  const upstream=process.argv[2]; if(!upstream) throw new Error('Usage: node freeze.ts UPSTREAM_CHECKOUT');
  const f:Freeze={version:1,frozenAt:new Date().toISOString(),upstream:execFileSync('git',['-C',upstream,'rev-parse','HEAD'],{encoding:'utf8'}).trim(),
    models:['Qwen3.5-4B-Q4_K_M.gguf','Qwen3.8-27B-Q4_K_M.gguf'],seeds:[1,2,7],
    corpusSha256:sha(resolve(here,'v1/held-out.json')),smokeSha256:sha(resolve(here,'v1/smoke.json')),productionSources:sourceHashes(),
    limits:{tool_rounds:4,tool_calls:12,calls_per_round:4}};
  writeFileSync(resolve(here,'v1/freeze.json'),JSON.stringify(f,null,2)+'\n',{flag:'wx'});
  console.log(`Frozen ${corpus.cases.length} held-out cases before inference; ${corpus.cases.filter(c=>c.restart).length} restart cases.`);
}
