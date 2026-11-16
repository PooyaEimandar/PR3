import { spawn, execFileSync, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { createReadStream, createWriteStream, readFileSync, writeFileSync, mkdirSync, appendFileSync, readdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { createInterface } from 'node:readline';
import { resolve, relative } from 'node:path';
import { platform, release, arch, cpus, totalmem } from 'node:os';
import { performance } from 'node:perf_hooks';
import { here, root, sha, verifyFreeze, validateCorpus } from './freeze.ts';
import { score, summarize } from './score.ts';
import type { Corpus, ResearchCase, Row, Turn, ToolEvent } from './types.ts';

type ObjectValue = Record<string, unknown>;
class JsonProcess {
  child:ChildProcessWithoutNullStreams;
  private log:string;
  private lines:string[]=[];
  private waiter:{resolve:(v:ObjectValue)=>void;reject:(e:Error)=>void;timer:NodeJS.Timeout}|undefined;
  private failure:Error|undefined;
  constructor(command:string,args:string[],log:string,env:NodeJS.ProcessEnv=process.env) {
    this.log=log;
    this.child=spawn(command,args,{cwd:root,env,stdio:['pipe','pipe','pipe']});
    const output=createWriteStream(log,{flags:'a'}); this.child.stderr.pipe(output);
    createInterface({input:this.child.stdout}).on('line',line=>{
      appendFileSync(this.log+'.ipc.jsonl',JSON.stringify({pid:this.child.pid,direction:'response',line})+'\n');
      this.lines.push(line);this.deliver();
    });
    this.child.on('error',e=>this.fail(e));
    this.child.on('exit',code=>{this.fail(new Error(`Process exited (${code})`));output.end();});
  }
  private fail(error:Error) { this.failure=error; if(this.waiter){clearTimeout(this.waiter.timer);this.waiter.reject(error);this.waiter=undefined;} }
  private deliver() {
    if(!this.waiter || !this.lines.length)return;
    const w=this.waiter;this.waiter=undefined;clearTimeout(w.timer);
    try {w.resolve(JSON.parse(this.lines.shift()!) as ObjectValue);} catch(e){w.reject(new Error(`Invalid adapter output: ${String(e)}`));}
  }
  send(value:unknown) {
    appendFileSync(this.log+'.ipc.jsonl',JSON.stringify({pid:this.child.pid,direction:'request',value})+'\n');
    this.child.stdin.write(JSON.stringify(value)+'\n');
  }
  next():Promise<ObjectValue> {
    if(this.failure && !this.lines.length)return Promise.reject(this.failure);
    if(this.waiter)return Promise.reject(new Error('Concurrent IPC request refused'));
    return new Promise((resolve,reject)=>{
      const timer=setTimeout(()=>{this.child.kill('SIGKILL');this.fail(new Error('Adapter response deadline: 300 seconds'));},300000);
      this.waiter={resolve,reject,timer};this.deliver();
    });
  }
  async call(value:unknown) {this.send(value);return this.next();}
  async close() {
    if(this.waiter){clearTimeout(this.waiter.timer);this.waiter=undefined;}
    if(this.child.exitCode!==null || this.child.signalCode!==null)return;
    await new Promise<void>(resolve=>{
      const timer=setTimeout(()=>this.child.kill('SIGKILL'),10000);
      this.child.once('exit',()=>{clearTimeout(timer);resolve();});
      this.child.stdin.end();
    });
  }
}
async function fileHash(path:string):Promise<string> {const hash=createHash('sha256');for await(const chunk of createReadStream(path))hash.update(chunk);return hash.digest('hex');}
function checked(value:ObjectValue):ObjectValue {if(value['error'])throw new Error(String(value['error']));return value;}
function inputOnly(c:ResearchCase) {const {checks:_,restart:__,...input}=c;return input;}
async function upstreamTurn(bridge:JsonProcess,python:string,upstream:string,question:string,history:{role:string;content:string}[],out:string):Promise<Turn> {
  const worker=new JsonProcess(python,[resolve(here,'openjarvis_adapter.py'),resolve(upstream,'src')],resolve(out,'upstream.log'),{
    ...process.env,OPENJARVIS_HOME:resolve(out,'isolated-upstream-home'),DO_NOT_TRACK:'1',PYTHONDONTWRITEBYTECODE:'1',
  });
  const events:ToolEvent[]=[],generations:ObjectValue[]=[];
  try {
    const specs=checked(await bridge.call({op:'specs'}));
    worker.send({question,history,specs});
    for(;;) {
      const message=await worker.next();
      if(message['result'])return {...message['result'] as Turn,events,generations,evidence:{sources:[],network:[]}};
      const request=message['bridge'] as ObjectValue|undefined;
      if(!request || !['generate','tool'].includes(String(request['op'])))throw new Error('Unknown upstream IPC operation');
      const response=await bridge.call(request);
      if(request['op']==='generate')generations.push({request,response});
      else events.push(response['error'] ? {name:String(request['name']),arguments:request['arguments'],success:false,output:String(response['error'])} : response as ToolEvent);
      worker.send(response);
    }
  } finally {await worker.close();}
}
const options=new Map<string,string>();
for(let i=2;i<process.argv.length;i+=2){const key=process.argv[i],value=process.argv[i+1];if(!key||!value)throw new Error('Options require values');options.set(key,value);}
const required=(key:string)=>{const value=options.get(key);if(!value)throw new Error(`Missing ${key}`);return resolve(value);};
const upstream=required('--upstream'),python=required('--python'),out=required('--out');
const split=options.get('--split')??'held-out';if(!['smoke','held-out'].includes(split))throw new Error('Invalid split');
const frozen=verifyFreeze();
if(execFileSync('git',['-C',upstream,'rev-parse','HEAD'],{encoding:'utf8'}).trim()!==frozen.upstream)throw new Error('Upstream revision changed');
if(execFileSync('git',['-C',upstream,'status','--porcelain'],{encoding:'utf8'}).trim())throw new Error('Upstream checkout must be clean');
const corpus=JSON.parse(readFileSync(resolve(here,`v1/${split}.json`),'utf8')) as Corpus;validateCorpus(corpus);
const seeds=split==='smoke'?[1]:frozen.seeds;
const names=split==='smoke'?[frozen.models[0]!]:frozen.models;
const binary=resolve(root,'target/debug/research_bridge');
mkdirSync(out,{recursive:false});
writeFileSync(resolve(out,'corpus.json'),readFileSync(resolve(here,`v1/${split}.json`)));
const models=[];
for(const name of names){const path=resolve(root,'model',name);models.push({name,path,sha256:await fileHash(path)});}
const harnessFiles=readdirSync(here).filter(f=>/\.(ts|py|json)$/u.test(f));
const manifest={version:1,startedAt:new Date().toISOString(),split,upstream:frozen.upstream,freezeSha256:sha(resolve(here,'v1/freeze.json')),corpusSha256:sha(resolve(here,`v1/${split}.json`)),
  pr3Head:execFileSync('git',['rev-parse','HEAD'],{cwd:root,encoding:'utf8'}).trim(),models,seeds,binarySha256:sha(binary),
  harnessHashes:Object.fromEntries([...harnessFiles.map(f=>resolve(here,f)),resolve(root,'bench/documents/src/bin/research_bridge.rs')].map(p=>[relative(root,p),sha(p)])),
  productionSources:frozen.productionSources,hardware:{platform:platform(),release:release(),arch:arch(),cpu:cpus()[0]?.model,memoryBytes:totalmem()},
  power:platform()==='darwin'?execFileSync('pmset',['-g','batt'],{encoding:'utf8'}):'not recorded',
  python:execFileSync(python,['--version'],{encoding:'utf8'}).trim(),pythonPackages:execFileSync('uv',['pip','freeze','--python',python],{encoding:'utf8'}).trim(),
  engine:{context:4096,maxNewTokens:512,temperature:.7,topP:.8,topK:20,presencePenalty:1.5,thinking:false,gpuLayers:999},limits:frozen.limits,
  scope:'Upstream OrchestratorAgent function-calling mode with shared Rust research tools, fixture network and llama.cpp. This is not a native retrieval or live search comparison. Restart reopens shared SQLite and reconstructs agents; model stays loaded. No production changes after freeze; no automatic failed-case retry.'};
writeFileSync(resolve(out,'manifest.json'),JSON.stringify(manifest,null,2)+'\n');
const rows:Row[]=[];
const expected=corpus.cases.length*models.length*seeds.length*2;
for(const model of models)for(const seed of seeds)for(const [index,c] of corpus.cases.entries())for(const side of ((index+seed)%2?['pr3','openjarvis']:['openjarvis','pr3']) as Row['side'][]) {
  verifyFreeze();
  const bridge=new JsonProcess(binary,[],resolve(out,'bridge.log'));
  const row:Row={id:c.id,side,model:model.name,seed,turns:[]};
  try {
    const init=checked(await bridge.call({op:'init',side,model:model.path,seed,case:inputOnly(c)}));
    if(JSON.stringify(init['limits'])!==JSON.stringify({calls_per_round:4,tool_calls:12,tool_rounds:4}))throw new Error('Unmatched runtime limits');
    const history:{role:string;content:string}[]=[];
    const questions=[`Research ${c.online?'online':'locally'}: ${c.topic}`,...(c.restart?[c.restart.question]:[])];
    for(const [i,question] of questions.entries()) {
      if(i) {writeFileSync(resolve(out,'restart-history.json'),JSON.stringify(history));checked(await bridge.call({op:'restart'}));}
      checked(await bridge.call({op:'begin',question}));
      const start=performance.now();
      const turn=side==='pr3'?await bridge.call({op:'run',question}) as unknown as Turn
        :await upstreamTurn(bridge,python,upstream,question,i?JSON.parse(readFileSync(resolve(out,'restart-history.json'),'utf8')) as typeof history:[],out);
      turn.adapter_seconds=(performance.now()-start)/1000;
      turn.answer ??= ''; turn.error ??= null; turn.events ??= [];
      turn.evidence=checked(await bridge.call({op:'evidence'})) as unknown as Turn['evidence'];
      row.turns.push(turn);history.push({role:'user',content:question},{role:'assistant',content:turn.answer});
    }
  } catch(error) {row.error=String(error);} finally {await bridge.close();}
  rows.push(row);appendFileSync(resolve(out,'results.jsonl'),JSON.stringify(row)+'\n');
  const summary=summarize(corpus,rows,models.map(m=>m.name),seeds);
  writeFileSync(resolve(out,'summary.json'),JSON.stringify(summary,null,2)+'\n');
  const passes=!row.error && row.turns.every((turn,i)=>score(turn,i===0?c.checks:c.restart!.checks).automaticPass);
  console.log(`${rows.length}/${expected} ${model.name} ${side} ${c.id} seed=${seed}: ${row.error?'ERROR':passes?'PASS':'FAIL'} (${row.turns.reduce((n,t)=>n+t.seconds,0).toFixed(1)}s)`);
}
verifyFreeze();
const review=rows.flatMap(row=>row.turns.map((turn,i)=>({
  blindId:createHash('sha256').update(`${row.model}/${row.side}/${row.seed}/${row.id}/${i}`).digest('hex').slice(0,16),
  case:row.id,turn:i,answer:turn.answer,sources:turn.evidence.sources,
  rubric:(i===0?corpus.cases.find(c=>c.id===row.id)?.checks:corpus.cases.find(c=>c.id===row.id)?.restart?.checks)?.rubric,
  judgment:null,reason:null,
}))).sort((a,b)=>a.blindId.localeCompare(b.blindId));
writeFileSync(resolve(out,'semantic-review.json'),JSON.stringify(review,null,2)+'\n');
console.log(`Completed ${rows.length} rows. Semantic review remains separate: ${resolve(out,'semantic-review.json')}`);
